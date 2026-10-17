use chrono::Local;
use evdev::uinput::VirtualDeviceBuilder;
use evdev::{AttributeSet, Device, EventType, InputEvent, Key};
use nix::fcntl::{fcntl, FcntlArg, Flock, FlockArg, OFlag};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use signal_hook::flag;
use std::collections::HashSet;

use crate::runtime::bluetooth_hotkeys::{
    parse_hotkey_report, BluetoothHotkeyAction, REPORT_FN_LOCK, REPORT_ID, REPORT_RELEASE,
};
use crate::runtime::mic_chord::{request_step, ChordKey, MicChord};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::runtime::paths;

pub fn run_from_env() -> Result<(), String> {
    run_with_args(env::args().skip(1))
}

pub fn run_with_args<I>(args: I) -> Result<(), String>
where
    I: Iterator<Item = String>,
{
    let args = parse_args(args);
    let base_dir = base_dir_from_pid_file(&args.pid_file);

    if args.stop {
        return stop_process(&args.pid_file);
    }

    ensure_dir(&base_dir)?;

    // Claim the pid file before touching the keyboard, so a duplicate helper
    // exits without grabbing it or creating a second virtual device.
    write_pid(&args.pid_file)?;
    let _pid_guard = PidFileGuard::new(args.pid_file.clone());

    let device_path = args
        .device
        .clone()
        .or_else(find_keyboard_device)
        .ok_or_else(|| "USB keyboard event device not found".to_string())?;

    log_info(&format!(
        "Starting helper with keyboard device {}",
        device_path.display()
    ));

    let mut device = Device::open(&device_path)
        .map_err(|e| format!("Failed to open {}: {e}", device_path.display()))?;
    device.grab().map_err(|e| {
        format!(
            "Failed to grab device {}: {}. Another process may already hold an exclusive grab.",
            device_path.display(),
            e
        )
    })?;
    configure_nonblocking(&device).map_err(|e| {
        format!(
            "Failed to configure {} as non-blocking: {e}",
            device_path.display()
        )
    })?;
    log_info(&format!(
        "Grabbed keyboard device {} successfully",
        device_path.display()
    ));

    // The keyboard's own Fn lock is only known for the UX8406MA's keyboard.
    // Others keep the F-key remap with Fn pass-through (run_pass_through).
    let id = device.input_id();
    let hardware_fn_lock = crate::hardware::hid::has_hardware_fn_lock(id.vendor(), id.product());

    // Mute and volume come from the keyboard's consumer-control node. It is
    // grabbed too, so they can drive the mic-volume chord; otherwise
    // everything is passed on unchanged.
    let mut consumer = if hardware_fn_lock {
        open_consumer_node(&device_path)
    } else {
        None
    };

    let mut keys = AttributeSet::<Key>::new();
    for source in std::iter::once(&device).chain(consumer.as_ref()) {
        if let Some(supported) = source.supported_keys() {
            for key in supported.iter() {
                keys.insert(key);
            }
        }
    }
    for key in [
        Key::KEY_MUTE,
        Key::KEY_VOLUMEDOWN,
        Key::KEY_VOLUMEUP,
        Key::KEY_BRIGHTNESSDOWN,
        Key::KEY_BRIGHTNESSUP,
        // ASUS keys (see handle_vendor_report).
        Key::KEY_MICMUTE,
        Key::KEY_PROG2,
    ] {
        keys.insert(key);
    }

    // Reuse the keyboard's vendor/product so libinput pairs the virtual device
    // with the keyboard's touchpad for disable-while-typing (see the
    // AttrTPKComboLayout quirk installed by setup-common.sh).
    let mut uinput = VirtualDeviceBuilder::new()
        .map_err(|e| format!("Failed to init uinput builder: {e}"))?
        .name("Zenbook Duo USB Remap")
        .input_id(device.input_id())
        .with_keys(&keys)
        .map_err(|e| format!("Failed to set keys for uinput: {e}"))?
        .build()
        .map_err(|e| format!("Failed to create uinput device: {e}"))?;

    let terminate = Arc::new(AtomicBool::new(false));
    flag::register(signal_hook::consts::SIGTERM, Arc::clone(&terminate))
        .map_err(|e| format!("Failed to register SIGTERM handler: {e}"))?;
    flag::register(signal_hook::consts::SIGINT, Arc::clone(&terminate))
        .map_err(|e| format!("Failed to register SIGINT handler: {e}"))?;

    let pause_file = base_dir.join("usb_media_remap.paused");

    if !hardware_fn_lock {
        log_info(&format!(
            "Keyboard {:04x}:{:04x}: Fn lock in software (F-key remap, Fn passes F-keys through)",
            id.vendor(),
            id.product()
        ));
        let result = run_pass_through(&mut device, &device_path, &mut uinput, &args, &terminate, &pause_file);
        let _ = device.ungrab();
        log_info("Stopping helper");
        return result;
    }

    // The ASUS functions arrive as vendor reports on the keyboard's hidraw
    // nodes (hidraw reads are not affected by the evdev grab).
    let mut vendor_readers = open_vendor_readers(&device_path);
    let expected_readers = vendor_readers.len();
    let mut last_reader_reopen = Instant::now();

    // The keyboard does Fn itself; it only needs to be told its mode, again
    // after every connect. Paused, the top row sends plain F1-F12.
    let mut mode = wanted_mode(&pause_file);
    send_mode_after_connect(mode);
    log_info(&format!(
        "Top row: {} (Fn lock is the keyboard's own; Fn+Esc switches)",
        if mode { "F1-F12" } else { "media keys" }
    ));
    let mut last_check = Instant::now();

    let mut keys_state = KeyState::default();

    while !terminate.load(Ordering::Relaxed) {
        let mut idle = true;

        let mut reports = Vec::new();
        vendor_readers.retain_mut(|(path, reader)| {
            let mut report = [0u8; 64];
            loop {
                match reader.read(&mut report) {
                    Ok(0) => return false,
                    Ok(n) => {
                        idle = false;
                        reports.push(report[..n].to_vec());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return true,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        log_info(&format!("Stopped reading {}: {e}", path.display()));
                        return false;
                    }
                }
            }
        });
        for report in reports {
            handle_vendor_report(&mut uinput, &args, &mut keys_state, &report)?;
        }
        // A volume key held with the mic key repeats its step.
        if let Some(step) = keys_state.mic_chord.due(Instant::now()) {
            request_step(step);
        }

        if last_check.elapsed() >= Duration::from_secs(1) {
            last_check = Instant::now();
            // Fn lock or the pause changed (Control Panel, Fn+Esc detached).
            let wanted = wanted_mode(&pause_file);
            if wanted != mode {
                mode = wanted;
                set_mode(mode);
                log_info(&format!("Top row: {}", if mode { "F1-F12" } else { "media keys" }));
            }
        }

        // The keyboard re-creates its vendor interface after every
        // backlight write, so reopen the hidraw nodes once they come back.
        if vendor_readers.len() < expected_readers
            && last_reader_reopen.elapsed() >= Duration::from_secs(1)
        {
            last_reader_reopen = Instant::now();
            let reopened = open_vendor_readers(&device_path);
            if reopened.len() >= expected_readers {
                log_info(&format!("Reopened {} keyboard hidraw nodes", reopened.len()));
                vendor_readers = reopened;
                set_mode(mode);
            }
        }

        let mut events: Vec<InputEvent> = match device.fetch_events() {
            Ok(events) => events.collect(),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Vec::new(),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("Failed to read events: {e}")),
        };
        let consumer_lost = consumer.as_mut().is_some_and(|node| match node.fetch_events() {
            Ok(more) => {
                events.extend(more);
                false
            }
            Err(e) => e.kind() != std::io::ErrorKind::WouldBlock,
        });
        if consumer_lost {
            consumer = None;
        }
        if events.is_empty() {
            if idle {
                std::thread::sleep(Duration::from_millis(50));
            }
            continue;
        }
        for event in events {
            if terminate.load(Ordering::Relaxed) {
                break;
            }
            handle_key_event(&mut uinput, &mut keys_state, &event)?;
        }
    }

    if let Some(node) = consumer.as_mut() {
        let _ = node.ungrab();
    }
    let _ = device.ungrab();
    log_info("Stopping helper");
    Ok(())
}

pub fn log_error(message: &str) {
    eprintln!("USB-REMAP - ERROR: {}", message);
    log_line("ERROR", message);
}

pub fn log_info(message: &str) {
    eprintln!("USB-REMAP - INFO: {}", message);
    log_line("INFO", message);
}

fn log_line(level: &str, message: &str) {
    // Best-effort: derive the log location from --pid-file (or default).
    let pid_file = pid_file_from_env_args();
    let base_dir = base_dir_from_pid_file(&pid_file);
    let _ = ensure_dir(&base_dir);
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let log_path = base_dir.join("duo.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = writeln!(file, "{} - USB-REMAP - {}: {}", timestamp, level, message);
    }
}

#[derive(Debug, Clone)]
struct Args {
    pid_file: String,
    user: Option<String>,
    device: Option<PathBuf>,
    stop: bool,
}

fn parse_args<I>(mut args: I) -> Args
where
    I: Iterator<Item = String>,
{
    let mut pid_file = default_pid_file();
    let mut user = None;
    let mut device = None;
    let mut stop = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pid-file" => {
                if let Some(value) = args.next() {
                    pid_file = value;
                }
            }
            "--user" => {
                if let Some(value) = args.next() {
                    user = Some(value);
                }
            }
            "--device" => {
                if let Some(value) = args.next() {
                    device = Some(PathBuf::from(value));
                }
            }
            "--stop" => stop = true,
            _ => {}
        }
    }

    Args {
        pid_file,
        user,
        device,
        stop,
    }
}

/// What the helper keeps between events: the mic-volume chord, and the key
/// held for a vendor report until its release report.
#[derive(Default)]
struct KeyState {
    mic_chord: MicChord,
    held: Option<Key>,
    active_report: Option<u8>,
}

/// A key from the keyboard: passed on, except the mic-volume chord.
fn handle_key_event(
    uinput: &mut evdev::uinput::VirtualDevice,
    state: &mut KeyState,
    event: &InputEvent,
) -> Result<(), String> {
    if event.event_type() != EventType::KEY {
        return Ok(());
    }
    let key = Key::new(event.code());
    let value = event.value();

    // Mic key held + Up/Down or the volume keys: mic volume.
    match state.mic_chord.key(key, value) {
        ChordKey::Pass => {}
        ChordKey::Swallow => return Ok(()),
        ChordKey::Step(step) => {
            request_step(step);
            return Ok(());
        }
    }
    emit_key(uinput, key, value)
}

/// An ASUS vendor report (`5a <code>`, then `5a 00` on release): the top
/// row's ASUS functions (backlight, brightness, screen arrows, mic, emoji,
/// ASUS hub) and Fn+Esc. The same reports as detached.
fn handle_vendor_report(
    uinput: &mut evdev::uinput::VirtualDevice,
    args: &Args,
    state: &mut KeyState,
    report: &[u8],
) -> Result<(), String> {
    if report.len() < 2 || report[0] != REPORT_ID {
        return Ok(());
    }
    let code = report[1];
    if code == REPORT_RELEASE {
        state.active_report = None;
        if let Some(key) = state.held.take() {
            emit_key(uinput, key, 0)?;
        }
        if state.mic_chord.is_held() && state.mic_chord.mic(0) {
            emit_key(uinput, Key::KEY_MICMUTE, 1)?;
            emit_key(uinput, Key::KEY_MICMUTE, 0)?;
        }
        return Ok(());
    }
    if code == REPORT_FN_LOCK {
        let locked = !load_fn_lock();
        save_fn_lock(locked);
        notify_fn_lock(args.user.as_deref(), locked);
        log_info(&format!("Fn lock {}", if locked { "on" } else { "off" }));
        return Ok(());
    }
    let Some(action) = parse_hotkey_report(report) else {
        return Ok(()); // e.g. the battery-level reports (5a 3d)
    };
    // The keyboard may repeat the report while the key is held.
    if state.active_report == Some(code) {
        return Ok(());
    }
    state.active_report = Some(code);
    if let Some(key) = state.held.take() {
        emit_key(uinput, key, 0)?;
    }

    match action {
        BluetoothHotkeyAction::BacklightCycle => {
            if let Some(level) = cycle_backlight() {
                show_backlight_osd(args.user.as_deref(), level);
            }
        }
        BluetoothHotkeyAction::BrightnessDown => {
            handle_brightness_key(uinput, Key::KEY_BRIGHTNESSDOWN, 1, "down")?;
            state.held = Some(Key::KEY_BRIGHTNESSDOWN);
        }
        BluetoothHotkeyAction::BrightnessUp => {
            handle_brightness_key(uinput, Key::KEY_BRIGHTNESSUP, 1, "up")?;
            state.held = Some(Key::KEY_BRIGHTNESSUP);
        }
        // The mic key: mute on release, or mic volume with Up/Down.
        BluetoothHotkeyAction::Key(Key::KEY_MICMUTE) => {
            state.mic_chord.mic(1);
        }
        BluetoothHotkeyAction::Key(key) => {
            emit_key(uinput, key, 1)?;
            state.held = Some(key);
        }
        BluetoothHotkeyAction::Emoji => open_emoji_picker(args.user.as_deref()),
        BluetoothHotkeyAction::ControlPanel => open_control_panel(args.user.as_deref()),
    }
    Ok(())
}

/// The remap for keyboards without the known Fn lock command: F1-F6 and F11
/// do their media functions in software, and F-keys pressed with Fn held
/// (or after an Fn tap) pass through unmapped.
fn run_pass_through(
    device: &mut Device,
    device_path: &Path,
    uinput: &mut evdev::uinput::VirtualDevice,
    args: &Args,
    terminate: &AtomicBool,
    pause_file: &Path,
) -> Result<(), String> {
    // Fn may be reported on the grabbed node or on a sibling node of the same
    // keyboard. Siblings are only read, never grabbed, so the desktop keeps them.
    let grabbed_reports_fn = reports_fn(device);
    let mut fn_watchers = open_fn_watchers(device_path);
    let mut fn_tap_readers = Vec::new();
    if grabbed_reports_fn || !fn_watchers.is_empty() {
        let mut nodes: Vec<String> = fn_watchers
            .iter()
            .map(|(path, _)| path.display().to_string())
            .collect();
        if grabbed_reports_fn {
            nodes.insert(0, device_path.display().to_string());
        }
        log_info(&format!(
            "Fn key reported by {}; F-keys pressed with Fn held pass through unmapped",
            nodes.join(", ")
        ));
    } else {
        fn_tap_readers = open_fn_tap_readers(device_path);
        if fn_tap_readers.is_empty() {
            log_info(
                "No keyboard node reports the Fn key; Fn+F-keys cannot be told apart from F-keys. \
                 Pause the remap (zenbook-duo-control --toggle-remap-pause) to use F1-F12.",
            );
        } else {
            let nodes: Vec<String> = fn_tap_readers
                .iter()
                .map(|(path, _)| path.display().to_string())
                .collect();
            log_info(&format!(
                "Watching {} for Fn presses; the key pressed next after Fn passes through unmapped",
                nodes.join(", ")
            ));
        }
    }
    let mut fn_state = FnPassthrough::default();

    while !terminate.load(Ordering::Relaxed) {
        let mut idle = true;
        fn_watchers.retain_mut(|(path, watcher)| match watcher.fetch_events() {
            Ok(events) => {
                for event in events {
                    idle = false;
                    if is_fn_event(&event) {
                        fn_state.observe_fn(event.value());
                    }
                }
                true
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::Interrupted =>
            {
                true
            }
            Err(e) => {
                log_info(&format!("Stopped watching {} for Fn: {e}", path.display()));
                fn_state.observe_fn(0);
                false
            }
        });

        // Read Fn taps before the keyboard so a tap is seen before the key it precedes.
        fn_tap_readers.retain_mut(|(path, reader)| {
            let mut report = [0u8; 64];
            loop {
                match reader.read(&mut report) {
                    Ok(0) => return false,
                    Ok(n) => {
                        idle = false;
                        if report[..n] == FN_TAP_REPORT {
                            fn_state.observe_fn_tap(Instant::now());
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return true,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        log_info(&format!("Stopped watching {} for Fn: {e}", path.display()));
                        return false;
                    }
                }
            }
        });

        let events: Vec<InputEvent> = match device.fetch_events() {
            Ok(events) => events.collect(),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Vec::new(),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("Failed to read events: {e}")),
        };
        if events.is_empty() {
            if idle {
                std::thread::sleep(Duration::from_millis(50));
            }
            continue;
        }
        let paused = pause_file.exists();
        for event in events {
            if terminate.load(Ordering::Relaxed) {
                break;
            }
            if event.event_type() != EventType::KEY {
                continue;
            }
            let key = Key::new(event.code());
            if is_fn_event(&event) {
                fn_state.observe_fn(event.value());
                emit_key(uinput, key, event.value())?;
                continue;
            }
            match fn_state.route(key, event.value(), paused) {
                Route::Raw => emit_key(uinput, key, event.value())?,
                Route::Remap => handle_event(uinput, args, event)?,
            }
        }
    }

    Ok(())
}

fn handle_event(
    uinput: &mut evdev::uinput::VirtualDevice,
    args: &Args,
    event: InputEvent,
) -> Result<(), String> {
    if event.event_type() != EventType::KEY {
        return Ok(());
    }

    let key = Key::new(event.code());
    let value = event.value();

    match key {
        Key::KEY_F4 => {
            if value == 1 {
                if let Some(level) = cycle_backlight() {
                    show_backlight_osd(args.user.as_deref(), level);
                }
            }
            return Ok(());
        }
        Key::KEY_F11 => {
            if value == 1 {
                open_emoji_picker(args.user.as_deref());
            }
            return Ok(());
        }
        _ => {}
    }

    if let Some((mapped, direction)) = brightness_key_mapping(key) {
        return handle_brightness_key(uinput, mapped, value, direction);
    }

    let mapped = match key {
        Key::KEY_F1 => Key::KEY_MUTE,
        Key::KEY_F2 => Key::KEY_VOLUMEDOWN,
        Key::KEY_F3 => Key::KEY_VOLUMEUP,
        _ => key,
    };

    emit_key(uinput, mapped, value)
}

#[derive(Debug, PartialEq, Eq)]
enum Route {
    /// Send the key exactly as the keyboard reported it.
    Raw,
    /// Apply the media remap.
    Remap,
}

/// How long an Fn tap (a press with no matching release, see FN_TAP_REPORT)
/// keeps the next key press raw.
const FN_TAP_WINDOW: Duration = Duration::from_secs(3);

/// Decides per key press whether the remap applies. A key pressed while Fn is
/// held (or while the remap is paused) stays raw until it is released, even if
/// Fn or the pause ends first, so press and release always match. An Fn tap
/// makes only the next non-modifier key press raw.
#[derive(Debug, Default)]
struct FnPassthrough {
    fn_held: bool,
    fn_tap_at: Option<Instant>,
    raw_keys: HashSet<u16>,
}

impl FnPassthrough {
    fn observe_fn(&mut self, value: i32) {
        self.fn_held = value != 0;
    }

    fn observe_fn_tap(&mut self, now: Instant) {
        self.fn_tap_at = Some(now);
    }

    fn route(&mut self, key: Key, value: i32, paused: bool) -> Route {
        self.route_at(key, value, paused, Instant::now())
    }

    fn route_at(&mut self, key: Key, value: i32, paused: bool, now: Instant) -> Route {
        let code = key.code();
        let raw = match value {
            1 => {
                let tapped = !is_modifier(key)
                    && self
                        .fn_tap_at
                        .take()
                        .is_some_and(|at| now.duration_since(at) <= FN_TAP_WINDOW);
                if self.fn_held || paused || tapped {
                    self.raw_keys.insert(code);
                    true
                } else {
                    self.raw_keys.remove(&code);
                    false
                }
            }
            0 => self.raw_keys.remove(&code),
            _ => self.raw_keys.contains(&code),
        };
        if raw {
            Route::Raw
        } else {
            Route::Remap
        }
    }
}

fn is_modifier(key: Key) -> bool {
    matches!(
        key,
        Key::KEY_LEFTCTRL
            | Key::KEY_RIGHTCTRL
            | Key::KEY_LEFTSHIFT
            | Key::KEY_RIGHTSHIFT
            | Key::KEY_LEFTALT
            | Key::KEY_RIGHTALT
            | Key::KEY_LEFTMETA
            | Key::KEY_RIGHTMETA
    )
}

/// The docked keyboard has no Fn key code over USB, and Fn+F2 sends the same
/// key report as F2. Each Fn press instead sends an empty report on the
/// keyboard's mouse interface (report ID 1: no buttons, no motion), which evdev
/// drops because nothing changed. Fn release sends nothing.
const FN_TAP_REPORT: [u8; 5] = [0x01, 0x00, 0x00, 0x00, 0x00];

/// Opens the hidraw nodes of the grabbed keyboard's USB device, read-only, to
/// watch for FN_TAP_REPORT. hidraw reads are not affected by the evdev grab.
fn open_fn_tap_readers(grabbed: &Path) -> Vec<(PathBuf, fs::File)> {
    let Some(usb_dir) = usb_device_dir(grabbed) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir("/sys/class/hidraw") else {
        return Vec::new();
    };
    let mut readers = Vec::new();
    for entry in entries.flatten() {
        let Ok(hid_dir) = fs::canonicalize(entry.path().join("device")) else {
            continue;
        };
        if !hid_dir.starts_with(&usb_dir) {
            continue;
        }
        let path = Path::new("/dev").join(entry.file_name());
        if let Ok(file) = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
        {
            readers.push((path, file));
        }
    }
    readers
}

fn is_fn_event(event: &InputEvent) -> bool {
    event.event_type() == EventType::KEY && Key::new(event.code()) == Key::KEY_FN
}

fn reports_fn(device: &Device) -> bool {
    device
        .supported_keys()
        .map(|keys| keys.contains(Key::KEY_FN))
        .unwrap_or(false)
}

/// Opens the keyboard's other event nodes that can report Fn, without grabbing them.
fn open_fn_watchers(grabbed: &Path) -> Vec<(PathBuf, Device)> {
    let grabbed = fs::canonicalize(grabbed).unwrap_or_else(|_| grabbed.to_path_buf());
    let Ok(entries) = fs::read_dir("/dev/input/by-id") else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut watchers = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.contains("Zenbook_Duo_Keyboard") || !name.contains("event") {
            continue;
        }
        let path = entry.path();
        let real = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if real == grabbed || !seen.insert(real) {
            continue;
        }
        let Ok(watcher) = Device::open(&path) else {
            continue;
        };
        if reports_fn(&watcher) && configure_nonblocking(&watcher).is_ok() {
            watchers.push((path, watcher));
        }
    }
    watchers
}

fn brightness_key_mapping(key: Key) -> Option<(Key, &'static str)> {
    match key {
        Key::KEY_F5 => Some((Key::KEY_BRIGHTNESSDOWN, "down")),
        Key::KEY_F6 => Some((Key::KEY_BRIGHTNESSUP, "up")),
        _ => None,
    }
}

/// The keyboard's mode: F1-F12 with Fn lock on or the remap paused.
fn wanted_mode(pause_file: &Path) -> bool {
    load_fn_lock() || pause_file.exists()
}

fn set_mode(f_keys: bool) {
    if let Err(err) = crate::hardware::hid::set_fn_lock_usb(f_keys) {
        log_error(&format!("Could not set the keyboard's Fn lock: {err}"));
    }
}

/// Right after connecting, the keyboard ignores commands for a moment (seen
/// in the Windows driver's traffic), so the mode is sent a few times.
fn send_mode_after_connect(f_keys: bool) {
    std::thread::spawn(move || {
        for delay in [0, 1, 2] {
            std::thread::sleep(Duration::from_secs(delay));
            set_mode(f_keys);
        }
    });
}

/// The keyboard's consumer-control event node (mute, volume), grabbed.
fn open_consumer_node(grabbed: &Path) -> Option<Device> {
    let grabbed = fs::canonicalize(grabbed).unwrap_or_else(|_| grabbed.to_path_buf());
    let usb_dir = usb_device_dir(&grabbed)?;
    for (path, mut node) in evdev::enumerate() {
        let real = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if real == grabbed || !usb_device_dir(&real).is_some_and(|dir| dir == usb_dir) {
            continue;
        }
        if !node.supported_keys().is_some_and(|keys| keys.contains(Key::KEY_VOLUMEUP)) {
            continue;
        }
        if node.grab().is_ok() && configure_nonblocking(&node).is_ok() {
            log_info(&format!("Grabbed consumer keys {}", path.display()));
            return Some(node);
        }
    }
    None
}

fn fn_lock_path() -> PathBuf {
    paths::system_runtime_dir().join("fn_lock")
}

pub(crate) fn load_fn_lock() -> bool {
    fs::read_to_string(fn_lock_path()).is_ok_and(|s| s.trim() == "1")
}

pub(crate) fn save_fn_lock(locked: bool) {
    // The daemon owns this directory (its socket lives there, and the session
    // agent must be able to reach it), so only create it if missing and never
    // change its permissions — ensure_dir() would force it to 0700.
    let path = fn_lock_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(path, if locked { "1" } else { "0" });
}

/// Opens the hidraw nodes of the grabbed keyboard's USB device, read-only, for
/// the vendor reports. hidraw reads are not affected by the evdev grab.
fn open_vendor_readers(grabbed: &Path) -> Vec<(PathBuf, fs::File)> {
    let Some(usb_dir) = usb_device_dir(grabbed) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir("/sys/class/hidraw") else {
        return Vec::new();
    };
    let mut readers = Vec::new();
    for entry in entries.flatten() {
        let Ok(hid_dir) = fs::canonicalize(entry.path().join("device")) else {
            continue;
        };
        if !hid_dir.starts_with(&usb_dir) {
            continue;
        }
        let path = Path::new("/dev").join(entry.file_name());
        if let Ok(file) = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
        {
            readers.push((path, file));
        }
    }
    readers
}

/// The sysfs directory of the USB device an input event node belongs to.
fn usb_device_dir(event_node: &Path) -> Option<PathBuf> {
    let node = fs::canonicalize(event_node).ok()?;
    let input_dir =
        fs::canonicalize(Path::new("/sys/class/input").join(node.file_name()?).join("device"))
            .ok()?;
    input_dir
        .ancestors()
        .find(|dir| dir.join("idVendor").exists())
        .map(Path::to_path_buf)
}

fn handle_brightness_key(
    uinput: &mut evdev::uinput::VirtualDevice,
    key: Key,
    value: i32,
    direction: &str,
) -> Result<(), String> {
    let before = if value == 1 {
        read_primary_brightness().ok()
    } else {
        None
    };

    emit_key(uinput, key, value)?;

    if value == 1 {
        maybe_step_brightness_after_native_event(direction, before)?;
    }

    Ok(())
}

fn maybe_step_brightness_after_native_event(
    direction: &str,
    before: Option<i32>,
) -> Result<(), String> {
    std::thread::sleep(Duration::from_millis(200));
    let after = read_primary_brightness().ok();
    if brightness_fallback_needed(before, after) {
        step_brightness(direction)?;
    }
    Ok(())
}

fn brightness_fallback_needed(before: Option<i32>, after: Option<i32>) -> bool {
    matches!((before, after), (Some(before), Some(after)) if before == after)
}

fn emit_key(uinput: &mut evdev::uinput::VirtualDevice, key: Key, value: i32) -> Result<(), String> {
    let events = [
        InputEvent::new(EventType::KEY, key.code(), value),
        InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
    ];
    uinput
        .emit(&events)
        .map_err(|e| format!("Failed to emit key event: {e}"))
}

/// Writes our pid unless another live process owns the file. The daemon's status
/// check may already have written our own pid while we were starting; that is us.
fn write_pid(path: &str) -> Result<(), String> {
    let own_pid = std::process::id() as i32;
    if let Ok(existing) = fs::read_to_string(path) {
        if let Ok(pid) = existing.trim().parse::<i32>() {
            if pid != own_pid && unsafe { libc::kill(pid, 0) } == 0 {
                return Err(format!("Remapper already running (pid {})", pid));
            }
        }
    }
    fs::write(path, std::process::id().to_string())
        .map_err(|e| format!("Failed to write pid file: {e}"))
}

pub(crate) fn configure_nonblocking(device: &Device) -> Result<(), nix::Error> {
    let fd = device.as_raw_fd();
    let current_flags = OFlag::from_bits_truncate(fcntl(fd, FcntlArg::F_GETFL)?);
    fcntl(fd, FcntlArg::F_SETFL(current_flags | OFlag::O_NONBLOCK))?;
    Ok(())
}

struct PidFileGuard {
    path: String,
}

impl PidFileGuard {
    fn new(path: String) -> Self {
        Self { path }
    }
}

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn stop_process(path: &str) -> Result<(), String> {
    let pid = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read pid file: {e}"))?
        .trim()
        .parse::<i32>()
        .map_err(|e| format!("Invalid pid file contents: {e}"))?;

    let pid = Pid::from_raw(pid);
    let _ = kill(pid, Signal::SIGTERM);

    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        let running = unsafe { libc::kill(pid.as_raw(), 0) == 0 };
        if !running {
            let _ = fs::remove_file(path);
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let _ = kill(pid, Signal::SIGKILL);
    let _ = fs::remove_file(path);
    Ok(())
}

fn find_keyboard_device() -> Option<PathBuf> {
    let by_id = Path::new("/dev/input/by-id");
    let entries = fs::read_dir(by_id).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.contains("Zenbook_Duo_Keyboard") && name.contains("event-kbd") {
            return Some(entry.path());
        }
        if name.contains("ASUS_Zenbook_Duo_Keyboard") && name.contains("event-kbd") {
            return Some(entry.path());
        }
    }
    None
}

/// The keyboard backlight pop-up, levels 0-3 as a level bar.
pub(crate) fn show_backlight_osd(user: Option<&str>, level: u8) {
    let Some(user) = user.map(str::to_string) else { return };
    std::thread::spawn(move || {
        let osd = crate::runtime::osd::Osd::new("keyboard-brightness-symbolic", "Keyboard backlight")
            .with_level(f64::from(level) / 3.0);
        crate::runtime::osd::show_for_user(&user, &osd);
    });
}

/// Steps the keyboard backlight; returns the new level when it changed.
fn cycle_backlight() -> Option<u8> {
    // Backlight state is shared with the main app via files next to the pid file.
    let pid_file = pid_file_from_env_args();
    let base_dir = base_dir_from_pid_file(&pid_file);
    if ensure_dir(&base_dir).is_err() {
        return None;
    }

    let kbl_lock_path = base_dir.join("kb_backlight_lock");
    let kbl_level_path = base_dir.join("kb_backlight_level");
    let kbl_last_cycle_path = base_dir.join("kb_backlight_last_cycle");

    let lock = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(kbl_lock_path)
    {
        Ok(file) => file,
        Err(_) => return None,
    };

    let _lock = match Flock::lock(lock, FlockArg::LockExclusiveNonblock) {
        Ok(l) => l,
        Err(_) => return None,
    };

    let now = current_time_ms();
    if let Some(last) = fs::read_to_string(&kbl_last_cycle_path)
        .ok()
        .and_then(|s| s.trim().parse::<u128>().ok())
    {
        // Short enough not to drop presses at a normal typing pace.
        if now.saturating_sub(last) < 250 {
            return None;
        }
    }

    let _ = fs::write(&kbl_last_cycle_path, now.to_string());

    // The daemon's level, which the Control Panel and the detached keyboard
    // change too; this helper's own file only as a fallback.
    let level = crate::hardware::sysfs::load_backlight_level().unwrap_or_else(|| {
        fs::read_to_string(&kbl_level_path)
            .ok()
            .and_then(|s| s.trim().parse::<u8>().ok())
            .unwrap_or(0)
    });
    let next = (level + 1) % 4;
    if crate::commands::backlight::set_backlight_daemon_first(next).is_ok() {
        let _ = fs::write(&kbl_level_path, next.to_string());
        return Some(next);
    }
    None
}

pub(crate) fn open_emoji_picker(user: Option<&str>) {
    let user = match user {
        Some(u) => u,
        None => return,
    };

    let uid = match nix::unistd::User::from_name(user) {
        Ok(Some(u)) => u.uid.as_raw(),
        _ => return,
    };

    let is_running = Command::new("pgrep")
        .arg("-u")
        .arg(uid.to_string())
        .arg("-x")
        .arg("gnome-characters")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if is_running {
        return;
    }

    launch_in_user_session(user, uid, &["gnome-characters"]);
}

/// Starts a program for `user` as an app of their own session (a transient
/// systemd user service), not as a child of the daemon: otherwise it would
/// run in the daemon's system service, outside the login session (logind
/// and portals don't know it), and be stopped whenever the daemon restarts.
pub(crate) fn launch_in_user_session(user: &str, uid: u32, program: &[&str]) {
    let child = user_session_command(user, uid)
        .args(["systemd-run", "--user", "--quiet", "--collect", "--"])
        .args(program)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    match child {
        // systemd-run returns once the program is started: reap it then.
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(err) => log_error(&format!("Could not start {}: {err}", program.join(" "))),
    }
}

/// The ASUS hub key: opens the Zenbook Duo Control Panel (or brings it to
/// the front; it runs as a single instance), as MyASUS on Windows.
pub(crate) fn open_control_panel(user: Option<&str>) {
    let Some(user) = user else {
        return;
    };
    let Ok(Some(account)) = nix::unistd::User::from_name(user) else {
        return;
    };
    launch_in_user_session(user, account.uid.as_raw(), &["zenbook-duo-control"]);
}

/// A command that runs as `user` inside their desktop session (dbus + display env).
pub(crate) fn user_session_command(user: &str, uid: u32) -> Command {
    let runtime_dir = format!("/run/user/{uid}");
    let bus_address = format!("unix:path={runtime_dir}/bus");

    let mut cmd = if nix::unistd::Uid::current().is_root() {
        let mut cmd = Command::new("runuser");
        cmd.arg("-u").arg(user).arg("--").arg("env");
        cmd
    } else {
        Command::new("env")
    };

    cmd.arg(format!("XDG_RUNTIME_DIR={runtime_dir}"))
        .arg(format!("DBUS_SESSION_BUS_ADDRESS={bus_address}"));

    if Path::new(&format!("{runtime_dir}/wayland-0")).exists() {
        cmd.arg("WAYLAND_DISPLAY=wayland-0");
    } else if Path::new("/tmp/.X11-unix/X0").exists() {
        cmd.arg("DISPLAY=:0");
    }
    cmd
}

/// The keyboard has no Fn lock LED, so show a pop-up (or, without the GNOME
/// extension, a notification whose synchronous hint replaces the last one).
pub(crate) fn notify_fn_lock(user: Option<&str>, locked: bool) {
    let Some(user) = user.map(str::to_string) else { return };
    // Named by what the top row sends: "Fn lock on/off" means opposite
    // things on different laptops (Windows calls the media-key mode "on").
    let (title, body) = if locked {
        ("Top row: F1–F12", "Hold Fn for media keys. Fn+Esc switches back.")
    } else {
        ("Top row: media keys", "Hold Fn for F1–F12. Fn+Esc switches back.")
    };
    // In a thread: key handling never waits for the desktop.
    std::thread::spawn(move || {
        let osd = crate::runtime::osd::Osd::new("input-keyboard-symbolic", title);
        if crate::runtime::osd::show_for_user(&user, &osd) {
            return;
        }
        let Ok(Some(account)) = nix::unistd::User::from_name(&user) else {
            return;
        };
        let _ = user_session_command(&user, account.uid.as_raw())
            .args([
                "notify-send",
                "--transient",
                "-a",
                "Zenbook Duo",
                "-i",
                "input-keyboard",
                "-h",
                "string:x-canonical-private-synchronous:zenbook-duo-fn-lock",
                title,
                body,
            ])
            .status();
    });
}

fn current_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis()
}

fn read_primary_brightness() -> Result<i32, String> {
    let primary = crate::hardware::sysfs::primary_backlight_dir()
        .ok_or_else(|| "no primary backlight device found".to_string())?;
    read_backlight_value(&primary.join("brightness"))
}

fn step_brightness(direction: &str) -> Result<(), String> {
    let primary = crate::hardware::sysfs::primary_backlight_dir()
        .ok_or_else(|| "no primary backlight device found".to_string())?;

    let primary_max = read_backlight_value(&primary.join("max_brightness"))?;
    let current = read_backlight_value(&primary.join("brightness"))?;
    let next = next_brightness_value(current, primary_max, direction);

    fs::write(primary.join("brightness"), next.to_string())
        .map_err(|e| format!("Failed to write primary brightness: {e}"))?;

    if let Some(secondary) = crate::hardware::sysfs::secondary_backlight_dir() {
        let secondary_max = read_backlight_value(&secondary.join("max_brightness"))?;
        let mirrored = next.min(secondary_max);
        fs::write(secondary.join("brightness"), mirrored.to_string())
            .map_err(|e| format!("Failed to write secondary brightness: {e}"))?;
    }

    Ok(())
}

fn read_backlight_value(path: &Path) -> Result<i32, String> {
    fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?
        .trim()
        .parse::<i32>()
        .map_err(|e| format!("Invalid brightness value in {}: {e}", path.display()))
}

fn next_brightness_value(current: i32, max: i32, direction: &str) -> i32 {
    let step = (max / 20).max(1);
    if direction == "up" {
        (current + step).min(max)
    } else {
        (current - step).max(0)
    }
}

fn default_pid_file() -> String {
    paths::current_user_runtime_dir()
        .join("usb_media_remap.pid")
        .to_string_lossy()
        .into_owned()
}

fn pid_file_from_env_args() -> String {
    let mut it = env::args().skip(1);
    while let Some(arg) = it.next() {
        if arg == "--pid-file" {
            if let Some(v) = it.next() {
                return v;
            }
        }
    }
    default_pid_file()
}

fn base_dir_from_pid_file(pid_file: &str) -> PathBuf {
    Path::new(pid_file)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(paths::current_user_runtime_dir)
}

fn ensure_dir(dir: &Path) -> Result<(), String> {
    crate::runtime::runtime_dir::ensure_dir_owned_like_parent(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_step_uses_five_percent_chunks() {
        assert_eq!(next_brightness_value(200, 400, "up"), 220);
        assert_eq!(next_brightness_value(200, 400, "down"), 180);
        assert_eq!(next_brightness_value(395, 400, "up"), 400);
        assert_eq!(next_brightness_value(5, 400, "down"), 0);
    }

    #[test]
    fn brightness_keys_map_to_native_desktop_events() {
        assert_eq!(
            brightness_key_mapping(Key::KEY_F5),
            Some((Key::KEY_BRIGHTNESSDOWN, "down"))
        );
        assert_eq!(
            brightness_key_mapping(Key::KEY_F6),
            Some((Key::KEY_BRIGHTNESSUP, "up"))
        );
        assert_eq!(brightness_key_mapping(Key::KEY_F4), None);
    }

    #[test]
    fn keys_pressed_without_fn_are_remapped() {
        let mut fn_state = FnPassthrough::default();
        assert_eq!(fn_state.route(Key::KEY_F2, 1, false), Route::Remap);
        assert_eq!(fn_state.route(Key::KEY_F2, 2, false), Route::Remap);
        assert_eq!(fn_state.route(Key::KEY_F2, 0, false), Route::Remap);
    }

    #[test]
    fn keys_pressed_with_fn_pass_through_until_released() {
        let mut fn_state = FnPassthrough::default();
        fn_state.observe_fn(1);
        assert_eq!(fn_state.route(Key::KEY_F2, 1, false), Route::Raw);
        // Fn released before F2: F2 still releases as the raw key.
        fn_state.observe_fn(0);
        assert_eq!(fn_state.route(Key::KEY_F2, 2, false), Route::Raw);
        assert_eq!(fn_state.route(Key::KEY_F2, 0, false), Route::Raw);
        // Next plain press is remapped again.
        assert_eq!(fn_state.route(Key::KEY_F2, 1, false), Route::Remap);
    }

    #[test]
    fn key_held_before_fn_keeps_its_remap_on_release() {
        let mut fn_state = FnPassthrough::default();
        assert_eq!(fn_state.route(Key::KEY_F3, 1, false), Route::Remap);
        fn_state.observe_fn(1);
        assert_eq!(fn_state.route(Key::KEY_F3, 0, false), Route::Remap);
    }

    #[test]
    fn pause_passes_presses_through_and_keeps_releases_matched() {
        let mut fn_state = FnPassthrough::default();
        assert_eq!(fn_state.route(Key::KEY_F1, 1, true), Route::Raw);
        // Unpaused before release: release still matches the raw press.
        assert_eq!(fn_state.route(Key::KEY_F1, 0, false), Route::Raw);
    }

    #[test]
    fn fn_tap_passes_only_the_next_key_press_through() {
        let mut fn_state = FnPassthrough::default();
        let t0 = Instant::now();
        fn_state.observe_fn_tap(t0);
        assert_eq!(fn_state.route_at(Key::KEY_F2, 1, false, t0), Route::Raw);
        assert_eq!(fn_state.route_at(Key::KEY_F2, 0, false, t0), Route::Raw);
        // Fn release is never reported, so a second F2 is remapped.
        assert_eq!(fn_state.route_at(Key::KEY_F2, 1, false, t0), Route::Remap);
        assert_eq!(fn_state.route_at(Key::KEY_F2, 0, false, t0), Route::Remap);
    }

    #[test]
    fn fn_tap_is_kept_across_modifiers() {
        let mut fn_state = FnPassthrough::default();
        let t0 = Instant::now();
        fn_state.observe_fn_tap(t0);
        assert_eq!(fn_state.route_at(Key::KEY_LEFTALT, 1, false, t0), Route::Remap);
        assert_eq!(fn_state.route_at(Key::KEY_F4, 1, false, t0), Route::Raw);
    }

    #[test]
    fn fn_tap_is_spent_by_any_other_key() {
        let mut fn_state = FnPassthrough::default();
        let t0 = Instant::now();
        fn_state.observe_fn_tap(t0);
        assert_eq!(fn_state.route_at(Key::KEY_HOME, 1, false, t0), Route::Raw);
        assert_eq!(fn_state.route_at(Key::KEY_F2, 1, false, t0), Route::Remap);
    }

    #[test]
    fn fn_tap_expires() {
        let mut fn_state = FnPassthrough::default();
        let t0 = Instant::now();
        fn_state.observe_fn_tap(t0);
        let later = t0 + FN_TAP_WINDOW + Duration::from_millis(1);
        assert_eq!(fn_state.route_at(Key::KEY_F2, 1, false, later), Route::Remap);
    }

    fn temp_pid_file(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!(
            "zenbook-duo-remap-test-{}-{}",
            std::process::id(),
            name
        ));
        let _ = fs::create_dir_all(&dir);
        dir.join("usb_media_remap.pid")
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn helper_accepts_its_own_pid_written_by_the_daemon_status_check() {
        // The daemon's status check finds a starting helper in /proc and writes
        // its pid before the helper claims the file (GitHub #28, #19).
        let path = temp_pid_file("own");
        fs::write(&path, std::process::id().to_string()).unwrap();
        assert_eq!(write_pid(&path), Ok(()));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            std::process::id().to_string()
        );
    }

    #[test]
    fn helper_refuses_when_another_live_process_owns_the_pid_file() {
        let path = temp_pid_file("other");
        let other = std::os::unix::process::parent_id();
        fs::write(&path, other.to_string()).unwrap();
        assert!(write_pid(&path).is_err());
    }

    #[test]
    fn helper_replaces_a_stale_pid_file() {
        let path = temp_pid_file("stale");
        fs::write(&path, i32::MAX.to_string()).unwrap();
        assert_eq!(write_pid(&path), Ok(()));
    }

    #[test]
    fn brightness_fallback_only_runs_when_native_event_did_not_change_level() {
        assert!(brightness_fallback_needed(Some(100), Some(100)));
        assert!(!brightness_fallback_needed(Some(100), Some(110)));
        assert!(!brightness_fallback_needed(None, Some(100)));
        assert!(!brightness_fallback_needed(Some(100), None));
    }
}
