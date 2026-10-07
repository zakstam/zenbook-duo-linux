use chrono::Local;
use evdev::uinput::VirtualDeviceBuilder;
use evdev::{AttributeSet, Device, EventType, InputEvent, Key};
use nix::fcntl::{fcntl, FcntlArg, Flock, FlockArg, OFlag};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use signal_hook::flag;
use std::collections::HashSet;
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

    let mut keys = AttributeSet::<Key>::new();
    if let Some(supported) = device.supported_keys() {
        for key in supported.iter() {
            keys.insert(key);
        }
    }

    for key in [
        Key::KEY_MUTE,
        Key::KEY_VOLUMEDOWN,
        Key::KEY_VOLUMEUP,
        Key::KEY_BRIGHTNESSDOWN,
        Key::KEY_BRIGHTNESSUP,
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

    // Fn may be reported on the grabbed node or on a sibling node of the same
    // keyboard. Siblings are only read, never grabbed, so the desktop keeps them.
    let grabbed_reports_fn = reports_fn(&device);
    let mut fn_watchers = open_fn_watchers(&device_path);
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
        fn_tap_readers = open_fn_tap_readers(&device_path);
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
                emit_key(&mut uinput, key, event.value())?;
                continue;
            }
            match fn_state.route(key, event.value(), paused) {
                Route::Raw => emit_key(&mut uinput, key, event.value())?,
                Route::Remap => handle_event(&mut uinput, &args, event)?,
            }
        }
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
                cycle_backlight();
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

fn configure_nonblocking(device: &Device) -> Result<(), nix::Error> {
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

fn cycle_backlight() {
    // Backlight state is shared with the main app via files next to the pid file.
    let pid_file = pid_file_from_env_args();
    let base_dir = base_dir_from_pid_file(&pid_file);
    if ensure_dir(&base_dir).is_err() {
        return;
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
        Err(_) => return,
    };

    let _lock = match Flock::lock(lock, FlockArg::LockExclusiveNonblock) {
        Ok(l) => l,
        Err(_) => return,
    };

    let now = current_time_ms();
    if let Some(last) = fs::read_to_string(&kbl_last_cycle_path)
        .ok()
        .and_then(|s| s.trim().parse::<u128>().ok())
    {
        if now.saturating_sub(last) < 600 {
            return;
        }
    }

    let _ = fs::write(&kbl_last_cycle_path, now.to_string());

    let level = fs::read_to_string(&kbl_level_path)
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok())
        .unwrap_or(0);
    let next = (level + 1) % 4;
    if crate::commands::backlight::set_backlight_daemon_first(next).is_ok() {
        let _ = fs::write(&kbl_level_path, next.to_string());
    }
}

fn open_emoji_picker(user: Option<&str>) {
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

    let _ = cmd.arg("gnome-characters").spawn();
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
