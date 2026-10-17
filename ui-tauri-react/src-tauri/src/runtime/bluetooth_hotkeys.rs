use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use std::os::unix::io::{AsRawFd, RawFd};

use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{AttributeSet, BusType, Device, EventType, InputEvent, Key};
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use tokio::sync::RwLock;

use crate::models::{ConnectionType, EventCategory, HardwareEvent};
use crate::runtime::mic_chord::{request_step, ChordKey, MicChord};
use crate::runtime::{logger, state::RuntimeState};

const HIDRAW_ROOT: &str = "/sys/class/hidraw";
pub(crate) const REPORT_ID: u8 = 0x5a;
pub(crate) const REPORT_RELEASE: u8 = 0x00;
const REPORT_BACKLIGHT_CYCLE: u8 = 0xc7;
const REPORT_BRIGHTNESS_DOWN: u8 = 0x10;
const REPORT_BRIGHTNESS_UP: u8 = 0x20;
// The other ASUS keys, captured from the detached keyboard.
const REPORT_SCREEN_ARROWS: u8 = 0x9c; // F8
const REPORT_MIC_MUTE: u8 = 0x7c; // F9
const REPORT_EMOJI: u8 = 0x7e; // F11
const REPORT_ASUS_HUB: u8 = 0x86; // F12
pub(crate) const REPORT_SCREEN_TOGGLE: u8 = 0x6a; // lower-screen key right of F12

// The vendor reports and the Shift+F7 detector below are shared with the USB remap helper: docked and detached, the keyboard
// sends the same reports once it handles Fn itself.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BluetoothHotkeyAction {
    BacklightCycle,
    BrightnessDown,
    BrightnessUp,
    /// Re-emitted as a key through a virtual keyboard, matching what the USB
    /// remap helper emits when the keyboard is docked.
    Key(Key),
    Emoji,
    /// The lower-screen key: the daemon cycles the internal screens.
    ScreenCycle,
    /// The ASUS hub key (F12): opens the Zenbook Duo Control Panel, the
    /// counterpart of MyASUS, which the key opens on Windows.
    ControlPanel,
}

pub(crate) const REPORT_FN_LOCK: u8 = 0x4e; // Fn+Esc

/// Keys the virtual keyboard can emit; the same codes the USB remap helper uses.
const EMITTED_KEYS: [Key; 10] = [
    Key::KEY_PROG1,
    Key::KEY_PROG2,
    Key::KEY_PROG3,
    Key::KEY_PROG4,
    Key::KEY_MICMUTE,
    Key::KEY_BRIGHTNESSDOWN,
    Key::KEY_BRIGHTNESSUP,
    Key::KEY_MUTE,
    Key::KEY_VOLUMEDOWN,
    Key::KEY_VOLUMEUP,
];

/// Fn lock over Bluetooth is the keyboard's own: in media mode a plain
/// top-row press sends a consumer key or an ASUS vendor report and Fn+key the
/// F-key; in F1-F12 mode (`5a d0 4e 01`) the other way round. Either way the
/// ASUS functions arrive as the same reports, so only the mode is sent.
fn set_hardware_fn_lock(locked: bool) {
    if let Err(err) = crate::hardware::hid::set_fn_lock_bluetooth(locked) {
        let _ = logger::append_line(format!("rust-daemon: Bluetooth Fn lock not sent: {err}"));
    }
}

/// The keyboard forgets its mode on every connect and ignores commands for a
/// moment right after, so the mode is sent again a few times (as Windows does).
fn restore_fn_lock_after_connect() {
    std::thread::spawn(|| {
        for delay in [0, 1, 2] {
            std::thread::sleep(Duration::from_secs(delay));
            set_hardware_fn_lock(crate::usb_media_remap_helper::load_fn_lock());
        }
    });
}

/// Recognises the detached keyboard's Shift+F7. For F7 the keyboard sends
/// Super+P, and with Shift held it releases Shift in the same report that
/// presses Super (same event time), then sends P. Nobody can do that by
/// hand, so it tells Shift+F7 apart from F7 and from a real Shift, Super, P.
#[derive(Debug, Default)]
pub(crate) struct ShiftF7Detector {
    shift_released: Option<std::time::SystemTime>,
    super_from_shift: bool,
}

/// Events of one keyboard report differ by microseconds at most.
const SAME_REPORT: Duration = Duration::from_millis(3);

impl ShiftF7Detector {
    /// Feeds one key event; true when it completes a Shift+F7.
    pub(crate) fn feed(&mut self, key: Key, value: i32, time: std::time::SystemTime) -> bool {
        match (key, value) {
            (Key::KEY_LEFTSHIFT | Key::KEY_RIGHTSHIFT, 0) => {
                self.shift_released = Some(time);
                self.super_from_shift = false;
                false
            }
            (Key::KEY_LEFTMETA, 1) => {
                self.super_from_shift = self.shift_released.take().is_some_and(|released| {
                    time.duration_since(released).map_or(true, |d| d <= SAME_REPORT)
                });
                false
            }
            (Key::KEY_P, 1) => std::mem::take(&mut self.super_from_shift),
            (_, 1) => {
                self.shift_released = None;
                self.super_from_shift = false;
                false
            }
            _ => false,
        }
    }

    fn feed_event(&mut self, event: &InputEvent) {
        let user = std::env::var("ZENBOOK_DUO_USER").ok();
        self.feed_event_for(event, user.as_deref());
    }

    /// Feeds a key event; on a Shift+F7, tells `user`'s desktop.
    pub(crate) fn feed_event_for(&mut self, event: &InputEvent, user: Option<&str>) {
        if event.event_type() == EventType::KEY
            && self.feed(Key::new(event.code()), event.value(), event.timestamp())
        {
            crate::runtime::osd::report_shift_f7(user);
        }
    }
}

/// The detached keyboard's event device, only read (not grabbed), to see
/// Shift+F7 while nothing grabs the keyboard.
fn open_key_watcher() -> Option<Device> {
    let (_, device) = evdev::enumerate().find(|(_, device)| is_bluetooth_duo_keyboard(device))?;
    crate::usb_media_remap_helper::configure_nonblocking(&device).ok()?;
    Some(device)
}

fn is_bluetooth_duo_keyboard(device: &Device) -> bool {
    device
        .name()
        .is_some_and(|name| name.contains("Zenbook Duo Keyboard"))
        && device.input_id().bus_type() == BusType::BUS_BLUETOOTH
        && device
            .supported_keys()
            .is_some_and(|keys| keys.contains(Key::KEY_A))
}

fn emit_on(device: &mut VirtualDevice, key: Key, value: i32) -> Result<(), String> {
    device
        .emit(&[
            InputEvent::new(EventType::KEY, key.code(), value),
            InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
        ])
        .map_err(|e| format!("Failed to emit {key:?}: {e}"))
}

/// The detached keyboard's event device, grabbed only while the mic key is
/// held (Fn lock off), so Up/Down can change the mic volume without reaching
/// the desktop. Other keys pass through `out`; any still down when the grab
/// ends are released there, so none stays stuck.
struct MicHoldGrab {
    device: Device,
    out: VirtualDevice,
    pressed: std::collections::HashSet<u16>,
}

fn open_mic_hold_grab() -> Option<MicHoldGrab> {
    let (_, mut device) = evdev::enumerate().find(|(_, device)| is_bluetooth_duo_keyboard(device))?;
    // A key already down when the grab starts would have its release taken
    // away from the desktop and stay stuck there: no chord then.
    if device.get_key_state().map_or(true, |down| down.iter().next().is_some()) {
        return None;
    }
    device.grab().ok()?;
    crate::usb_media_remap_helper::configure_nonblocking(&device).ok()?;
    let mut keys = AttributeSet::<Key>::new();
    for key in device.supported_keys()?.iter() {
        keys.insert(key);
    }
    let out = VirtualDeviceBuilder::new()
        .and_then(|builder| {
            builder
                .name("Zenbook Duo Bluetooth Mic Chord")
                .input_id(device.input_id())
                .with_keys(&keys)
        })
        .and_then(|builder| builder.build())
        .ok()?;
    Some(MicHoldGrab {
        device,
        out,
        pressed: std::collections::HashSet::new(),
    })
}

impl MicHoldGrab {
    fn forward_events(&mut self, chord: &mut MicChord) -> Result<(), String> {
        let events: Vec<InputEvent> = match self.device.fetch_events() {
            Ok(events) => events.collect(),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(e) => return Err(format!("Bluetooth keyboard read failed: {e}")),
        };
        for event in events {
            if event.event_type() != EventType::KEY {
                continue;
            }
            let key = Key::new(event.code());
            match chord.key(key, event.value()) {
                ChordKey::Pass => {
                    if event.value() == 0 {
                        self.pressed.remove(&key.code());
                    } else {
                        self.pressed.insert(key.code());
                    }
                    emit_on(&mut self.out, key, event.value())?;
                }
                ChordKey::Swallow => {}
                ChordKey::Step(step) => request_step(step),
            }
        }
        Ok(())
    }
}

impl Drop for MicHoldGrab {
    fn drop(&mut self) {
        for code in self.pressed.drain() {
            let _ = emit_on(&mut self.out, Key::new(code), 0);
        }
    }
}

/// Fn+Esc: flip the Fn lock shared with the USB remap helper and tell the
/// keyboard its new mode.
fn toggle_fn_lock() {
    let locked = !crate::usb_media_remap_helper::load_fn_lock();
    crate::usb_media_remap_helper::save_fn_lock(locked);
    let user = std::env::var("ZENBOOK_DUO_USER").ok();
    crate::usb_media_remap_helper::notify_fn_lock(user.as_deref(), locked);
    let _ = logger::append_line(format!(
        "rust-daemon: Bluetooth Fn lock {}",
        if locked { "on" } else { "off" }
    ));
    set_hardware_fn_lock(locked);
}

fn wait_readable(fds: &[RawFd], timeout_ms: i32) {
    let mut poll_fds: Vec<libc::pollfd> = fds
        .iter()
        .map(|&fd| libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    // SAFETY: poll_fds is a valid, correctly sized array of pollfd for the call.
    unsafe {
        libc::poll(poll_fds.as_mut_ptr(), poll_fds.len() as libc::nfds_t, timeout_ms);
    }
}

pub(crate) fn start(state: Arc<RwLock<RuntimeState>>) {
    match std::thread::Builder::new()
        .name("zenbook-duo-bt-hotkeys".into())
        .spawn(move || watch_loop(state))
    {
        Ok(_) => {}
        Err(err) => {
            log::warn!("failed to spawn Bluetooth hotkey watcher: {err}");
            let _ = logger::append_line(format!(
                "rust-daemon: failed to spawn Bluetooth hotkey watcher: {err}"
            ));
        }
    }
}

fn watch_loop(state: Arc<RwLock<RuntimeState>>) {
    let mut keys = create_virtual_keyboard();
    loop {
        let Some(path) = find_bluetooth_keyboard_hidraw() else {
            std::thread::sleep(Duration::from_secs(5));
            continue;
        };

        match fs::OpenOptions::new().read(true).open(&path) {
            Ok(file) => {
                let _ = logger::append_line(format!(
                    "rust-daemon: Bluetooth hotkey watcher opened {}",
                    path.display()
                ));
                crate::runtime::mic_led::restore_after_connect("Bluetooth");
                restore_backlight_on_connect(&state);
                let hardware_fn_lock = has_hardware_fn_lock(&path);
                if !hardware_fn_lock {
                    let _ = logger::append_line(
                        "rust-daemon: Bluetooth keyboard without the known Fn lock command; Fn+Esc is left to the keyboard",
                    );
                }
                if let Err(err) = watch_device(file, state.clone(), &mut keys, hardware_fn_lock) {
                    log::warn!("Bluetooth hotkey watcher lost {}: {err}", path.display());
                    let _ = logger::append_line(format!(
                        "rust-daemon: Bluetooth hotkey watcher lost {}: {err}",
                        path.display()
                    ));
                }
            }
            Err(err) => {
                log::warn!(
                    "failed to open Bluetooth hotkey hidraw {}: {err}",
                    path.display()
                );
            }
        }

        std::thread::sleep(Duration::from_secs(2));
    }
}

/// The keyboard resets its backlight when it switches to Bluetooth,
/// so re-send the last level once the Bluetooth link is up.
fn restore_backlight_on_connect(state: &Arc<RwLock<RuntimeState>>) {
    std::thread::sleep(Duration::from_secs(1));
    let level = state.blocking_read().status.backlight_level;
    let mut result = crate::hardware::hid::set_backlight_bluetooth(level);
    // Right after connecting the keyboard may still reject commands.
    for _ in 0..3 {
        if result.is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
        result = crate::hardware::hid::set_backlight_bluetooth(level);
    }
    let _ = logger::append_line(match &result {
        Ok(()) => format!("rust-daemon: restored Bluetooth backlight -> {level}"),
        Err(err) => format!("rust-daemon: failed to restore Bluetooth backlight: {err}"),
    });
}

fn create_virtual_keyboard() -> Option<VirtualDevice> {
    let mut keys = AttributeSet::<Key>::new();
    for key in EMITTED_KEYS {
        keys.insert(key);
    }
    let device = VirtualDeviceBuilder::new()
        .and_then(|builder| builder.name("Zenbook Duo Bluetooth Keys").with_keys(&keys))
        .and_then(|builder| builder.build());
    match device {
        Ok(device) => Some(device),
        Err(err) => {
            let _ = logger::append_line(format!(
                "rust-daemon: failed to create Bluetooth virtual keyboard: {err}"
            ));
            None
        }
    }
}

fn emit_key(keys: &mut Option<VirtualDevice>, key: Key, value: i32) -> Result<(), String> {
    let device = keys
        .as_mut()
        .ok_or_else(|| "Bluetooth virtual keyboard is unavailable".to_string())?;
    device
        .emit(&[
            InputEvent::new(EventType::KEY, key.code(), value),
            InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
        ])
        .map_err(|e| format!("Failed to emit {key:?}: {e}"))
}

/// Whether this hidraw node is the keyboard the Fn lock command was captured
/// on (0b05:1b2d); other models keep their own Fn handling.
fn has_hardware_fn_lock(node: &Path) -> bool {
    let Some(name) = node.file_name() else {
        return false;
    };
    fs::read_to_string(Path::new(HIDRAW_ROOT).join(name).join("device/uevent"))
        .is_ok_and(|uevent| crate::hardware::hid::is_captured_keyboard(&uevent))
}

fn watch_device(
    file: fs::File,
    state: Arc<RwLock<RuntimeState>>,
    keys: &mut Option<VirtualDevice>,
    hardware_fn_lock: bool,
) -> Result<(), String> {
    // A key emitted on a hotkey report is held until the keyboard's
    // release report, so the desktop's own key repeat works while it is held.
    let mut held: Option<Key> = None;
    let result = watch_reports(file, state, keys, &mut held, hardware_fn_lock);
    if let Some(key) = held.take() {
        let _ = emit_key(keys, key, 0);
    }
    result
}

fn watch_reports(
    mut file: fs::File,
    state: Arc<RwLock<RuntimeState>>,
    keys: &mut Option<VirtualDevice>,
    held: &mut Option<Key>,
    hardware_fn_lock: bool,
) -> Result<(), String> {
    let mut active_action: Option<BluetoothHotkeyAction> = None;
    let mut buffer = [0_u8; 64];

    let flags = OFlag::from_bits_truncate(
        fcntl(file.as_raw_fd(), FcntlArg::F_GETFL).map_err(|e| e.to_string())?,
    );
    fcntl(file.as_raw_fd(), FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))
        .map_err(|e| format!("Failed to make hidraw non-blocking: {e}"))?;
    let mut last_fn_lock_check = std::time::Instant::now();
    let mut fn_lock_sent = crate::usb_media_remap_helper::load_fn_lock();
    if hardware_fn_lock {
        restore_fn_lock_after_connect();
    }
    let mut chord = MicChord::default();
    let mut mic_grab: Option<MicHoldGrab> = None;
    let mut shift_f7 = ShiftF7Detector::default();
    let mut key_watcher = open_key_watcher();

    loop {
        let mut fds = vec![file.as_raw_fd()];
        if let Some(grab) = &mic_grab {
            fds.push(grab.device.as_raw_fd());
        }
        if let Some(watcher) = &key_watcher {
            fds.push(watcher.as_raw_fd());
        }
        // While the mic key is held, wake often enough for its volume repeat.
        wait_readable(&fds, if chord.is_held() { 50 } else { 1000 });
        if let Some(step) = chord.due(std::time::Instant::now()) {
            request_step(step);
        }

        // Only gets events while nothing grabs the keyboard.
        let watcher_lost = key_watcher.as_mut().is_some_and(|watcher| match watcher.fetch_events() {
            Ok(events) => {
                events.for_each(|event| shift_f7.feed_event(&event));
                false
            }
            Err(e) => e.kind() != std::io::ErrorKind::WouldBlock,
        });
        if watcher_lost {
            key_watcher = None;
        }

        loop {
            match file.read(&mut buffer) {
                Ok(0) => return Err("hidraw read returned EOF".into()),
                Ok(count) => {
                    let report = &buffer[..count];
                    if is_release_report(report) {
                        active_action = None;
                        if let Some(key) = held.take() {
                            emit_key(keys, key, 0)?;
                        }
                        if chord.is_held() {
                            // Before muting, so passed-through keys are let go.
                            mic_grab = None;
                            if chord.mic(0) {
                                emit_key(keys, Key::KEY_MICMUTE, 1)?;
                                emit_key(keys, Key::KEY_MICMUTE, 0)?;
                            }
                        }
                        continue;
                    }
                    if report.len() >= 2 && report[0] == REPORT_ID && report[1] == REPORT_FN_LOCK {
                        if !hardware_fn_lock {
                            continue;
                        }
                        toggle_fn_lock();
                        fn_lock_sent = crate::usb_media_remap_helper::load_fn_lock();
                        continue;
                    }

                    let Some(action) = parse_hotkey_report(report) else {
                        continue;
                    };

                    if active_action == Some(action) {
                        continue;
                    }
                    active_action = Some(action);

                    if let Some(key) = held.take() {
                        emit_key(keys, key, 0)?;
                    }
                    // The mic key: mute on release, or mic volume with Up/Down.
                    if action == BluetoothHotkeyAction::Key(Key::KEY_MICMUTE)
                        && matches!(
                            crate::hardware::sysfs::detect_connection_type(),
                            ConnectionType::Bluetooth
                        )
                    {
                        chord.mic(1);
                        mic_grab = open_mic_hold_grab();
                        continue;
                    }
                    match handle_action(action, &state, keys) {
                        Ok(pressed) => *held = pressed,
                        Err(err) => {
                            log::warn!("failed to handle Bluetooth hotkey {action:?}: {err}");
                            let _ = logger::append_line(format!(
                                "rust-daemon: failed to handle Bluetooth hotkey {action:?}: {err}"
                            ));
                        }
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(err.to_string()),
            }
        }

        // Fn lock can also be changed elsewhere (Control Panel, docked keyboard).
        if last_fn_lock_check.elapsed() >= Duration::from_secs(1) {
            last_fn_lock_check = std::time::Instant::now();
            if key_watcher.is_none() {
                key_watcher = open_key_watcher();
            }
            // Fn lock changed in the Control Panel or on the docked keyboard.
            let locked = crate::usb_media_remap_helper::load_fn_lock();
            if hardware_fn_lock && locked != fn_lock_sent {
                let _ = logger::append_line(format!(
                    "rust-daemon: Bluetooth Fn lock {} (changed elsewhere)",
                    if locked { "on" } else { "off" }
                ));
                set_hardware_fn_lock(locked);
                fn_lock_sent = locked;
            }
        }

        if let Some(grab) = mic_grab.as_mut() {
            if let Err(err) = grab.forward_events(&mut chord) {
                let _ = logger::append_line(format!("rust-daemon: {err}; ending mic key grab"));
                mic_grab = None;
            }
        }
    }
}

fn find_bluetooth_keyboard_hidraw() -> Option<PathBuf> {
    find_bluetooth_keyboard_hidraw_from(Path::new(HIDRAW_ROOT))
}

fn find_bluetooth_keyboard_hidraw_from(root: &Path) -> Option<PathBuf> {
    let mut devices = fs::read_dir(root)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let contents = fs::read_to_string(entry.path().join("device/uevent")).ok()?;
            if !is_bluetooth_keyboard_uevent(&contents) {
                return None;
            }
            // The keyboard has two Bluetooth hidraw nodes; prefer the
            // one whose descriptor declares the 0x5a hotkey report.
            let descriptor =
                fs::read(entry.path().join("device/report_descriptor")).unwrap_or_default();
            let declares_hotkeys = declares_report_id(&descriptor, REPORT_ID);
            Some((!declares_hotkeys, Path::new("/dev").join(entry.file_name())))
        })
        .collect::<Vec<_>>();
    devices.sort();
    devices.into_iter().next().map(|(_, path)| path)
}

/// Whether a HID report descriptor contains a Report ID item (0x85) for `id`.
fn declares_report_id(descriptor: &[u8], id: u8) -> bool {
    let mut i = 0;
    while i < descriptor.len() {
        let prefix = descriptor[i];
        let size = match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
        if prefix == 0x85 && descriptor.get(i + 1) == Some(&id) {
            return true;
        }
        i += 1 + size;
    }
    false
}

fn is_bluetooth_keyboard_uevent(contents: &str) -> bool {
    (contents.contains("Zenbook Duo Keyboard") || contents.contains("ASUS_DUO"))
        && contents.contains("HID_ID=0005:")
}

pub(crate) fn parse_hotkey_report(report: &[u8]) -> Option<BluetoothHotkeyAction> {
    if report.len() < 2 || report[0] != REPORT_ID {
        return None;
    }

    match report[1] {
        REPORT_BACKLIGHT_CYCLE => Some(BluetoothHotkeyAction::BacklightCycle),
        REPORT_BRIGHTNESS_DOWN => Some(BluetoothHotkeyAction::BrightnessDown),
        REPORT_BRIGHTNESS_UP => Some(BluetoothHotkeyAction::BrightnessUp),
        REPORT_SCREEN_ARROWS => Some(BluetoothHotkeyAction::Key(Key::KEY_PROG2)),
        REPORT_MIC_MUTE => Some(BluetoothHotkeyAction::Key(Key::KEY_MICMUTE)),
        REPORT_EMOJI => Some(BluetoothHotkeyAction::Emoji),
        REPORT_ASUS_HUB => Some(BluetoothHotkeyAction::ControlPanel),
        REPORT_SCREEN_TOGGLE => Some(BluetoothHotkeyAction::ScreenCycle),
        _ => None,
    }
}

fn is_release_report(report: &[u8]) -> bool {
    report.len() >= 2 && report[0] == REPORT_ID && report[1] == REPORT_RELEASE
}

/// Handles a hotkey press. Returns the key left pressed on the virtual
/// keyboard, which the caller releases on the keyboard's release report.
fn handle_action(
    action: BluetoothHotkeyAction,
    state: &Arc<RwLock<RuntimeState>>,
    keys: &mut Option<VirtualDevice>,
) -> Result<Option<Key>, String> {
    if !matches!(
        crate::hardware::sysfs::detect_connection_type(),
        ConnectionType::Bluetooth
    ) {
        return Ok(None);
    }

    match action {
        BluetoothHotkeyAction::BacklightCycle => cycle_backlight(state).map(|()| None),
        BluetoothHotkeyAction::BrightnessDown | BluetoothHotkeyAction::BrightnessUp => {
            press_brightness_key(action, state, keys).map(Some)
        }
        BluetoothHotkeyAction::Key(key) => {
            let user = std::env::var("ZENBOOK_DUO_USER").ok();
            crate::runtime::osd::report_app_key(user.as_deref(), key);
            emit_key(keys, key, 1).map(|()| Some(key))
        }
        BluetoothHotkeyAction::Emoji => {
            let user = std::env::var("ZENBOOK_DUO_USER").ok();
            crate::usb_media_remap_helper::open_emoji_picker(user.as_deref());
            Ok(None)
        }
        BluetoothHotkeyAction::ControlPanel => {
            let user = std::env::var("ZENBOOK_DUO_USER").ok();
            crate::usb_media_remap_helper::open_control_panel(user.as_deref());
            Ok(None)
        }
        BluetoothHotkeyAction::ScreenCycle => {
            // Through the daemon's own socket, like the USB remap helper; in a
            // thread so the hotkey loop never waits for the layout change.
            std::thread::spawn(|| {
                let result = crate::runtime::client::request(
                    crate::ipc::protocol::DaemonRequest::CycleScreens,
                );
                if !matches!(result, Ok(crate::ipc::protocol::DaemonResponse::Ack)) {
                    let _ = logger::append_line(format!(
                        "rust-daemon: Bluetooth screen cycle failed: {result:?}"
                    ));
                }
            });
            Ok(None)
        }
    }
}

/// Send the desktop's brightness key (so it shows its brightness
/// OSD and repeats while held), like the USB remap helper. Writes sysfs directly
/// only if the desktop did not change the brightness.
fn press_brightness_key(
    action: BluetoothHotkeyAction,
    state: &Arc<RwLock<RuntimeState>>,
    keys: &mut Option<VirtualDevice>,
) -> Result<Key, String> {
    let key = if action == BluetoothHotkeyAction::BrightnessUp {
        Key::KEY_BRIGHTNESSUP
    } else {
        Key::KEY_BRIGHTNESSDOWN
    };
    let brightness_path = crate::hardware::sysfs::primary_backlight_dir()
        .map(|dir| dir.join("brightness"));
    let before = brightness_path
        .as_deref()
        .and_then(|path| read_brightness_value(path).ok());

    if emit_key(keys, key, 1).is_err() {
        step_brightness(action, state)?;
        return Err("virtual keyboard unavailable; adjusted brightness directly".into());
    }

    std::thread::sleep(Duration::from_millis(200));
    let after = brightness_path
        .as_deref()
        .and_then(|path| read_brightness_value(path).ok());
    if before.is_some() && before == after {
        step_brightness(action, state)?;
    }
    Ok(key)
}

fn cycle_backlight(state: &Arc<RwLock<RuntimeState>>) -> Result<(), String> {
    let current = state.blocking_read().status.backlight_level;
    let next = (current + 1) % 4;
    crate::hardware::hid::set_backlight(next)?;
    crate::runtime::backlight_idle::level_set();

    let mut guard = state.blocking_write();
    guard.status.backlight_level = next;
    guard.push_recent_event(HardwareEvent::info(
        EventCategory::Keyboard,
        format!("Backlight set to {next}"),
        "rust-daemon",
    ));
    guard.touch();
    guard.save()?;
    let _ = logger::append_line(format!(
        "rust-daemon: Bluetooth hotkey cycled backlight -> {next}"
    ));
    let user = std::env::var("ZENBOOK_DUO_USER").ok();
    crate::usb_media_remap_helper::show_backlight_osd(user.as_deref(), next);
    Ok(())
}

fn step_brightness(
    action: BluetoothHotkeyAction,
    state: &Arc<RwLock<RuntimeState>>,
) -> Result<(), String> {
    let primary = crate::hardware::sysfs::primary_backlight_dir()
        .ok_or_else(|| "no primary backlight device found".to_string())?;
    let max = read_brightness_value(&primary.join("max_brightness"))?;
    let current = read_brightness_value(&primary.join("brightness"))?;
    let next = next_brightness_value(current, max, action);

    fs::write(primary.join("brightness"), next.to_string())
        .map_err(|e| format!("Failed to write primary brightness: {e}"))?;

    let sync_secondary = state.blocking_read().settings.sync_brightness;
    if sync_secondary {
        if let Some(secondary) = crate::hardware::sysfs::secondary_backlight_dir() {
            let secondary_max = read_brightness_value(&secondary.join("max_brightness"))?;
            let mirrored = next.min(secondary_max);
            fs::write(secondary.join("brightness"), mirrored.to_string())
                .map_err(|e| format!("Failed to write secondary brightness: {e}"))?;
        }
    }

    let mut guard = state.blocking_write();
    guard.status.display_brightness = next;
    guard.push_recent_event(HardwareEvent::info(
        EventCategory::Display,
        format!("Display brightness set to {next}"),
        "rust-daemon",
    ));
    guard.touch();
    guard.save()?;
    let _ = logger::append_line(format!(
        "rust-daemon: Bluetooth hotkey adjusted brightness -> {next}"
    ));
    Ok(())
}

fn read_brightness_value(path: &Path) -> Result<u32, String> {
    fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?
        .trim()
        .parse::<u32>()
        .map_err(|e| format!("Invalid brightness value in {}: {e}", path.display()))
}

fn next_brightness_value(current: u32, max: u32, action: BluetoothHotkeyAction) -> u32 {
    let step = (max / 20).max(1);
    match action {
        BluetoothHotkeyAction::BrightnessUp => current.saturating_add(step).min(max),
        BluetoothHotkeyAction::BrightnessDown => current.saturating_sub(step),
        BluetoothHotkeyAction::BacklightCycle
        | BluetoothHotkeyAction::Key(_)
        | BluetoothHotkeyAction::Emoji
        | BluetoothHotkeyAction::ScreenCycle
        | BluetoothHotkeyAction::ControlPanel => current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_keyboards_shift_f7() {
        use std::time::{Duration as D, UNIX_EPOCH};
        let at = |ms: u64| UNIX_EPOCH + D::from_micros(ms * 1000);
        let mut d = ShiftF7Detector::default();
        // Shift held, then the keyboard's F7: Shift up and Super down together, P.
        assert!(!d.feed(Key::KEY_LEFTSHIFT, 1, at(0)));
        assert!(!d.feed(Key::KEY_LEFTSHIFT, 0, at(400)));
        assert!(!d.feed(Key::KEY_LEFTMETA, 1, at(400)));
        assert!(d.feed(Key::KEY_P, 1, at(410)));
        // Plain F7: Super+P without Shift.
        assert!(!d.feed(Key::KEY_LEFTMETA, 1, at(1000)));
        assert!(!d.feed(Key::KEY_P, 1, at(1010)));
        // By hand: Shift released, Super pressed a little later.
        assert!(!d.feed(Key::KEY_LEFTSHIFT, 0, at(2000)));
        assert!(!d.feed(Key::KEY_LEFTMETA, 1, at(2080)));
        assert!(!d.feed(Key::KEY_P, 1, at(2100)));
        // Another key between Super and P.
        assert!(!d.feed(Key::KEY_LEFTSHIFT, 0, at(3000)));
        assert!(!d.feed(Key::KEY_LEFTMETA, 1, at(3000)));
        assert!(!d.feed(Key::KEY_A, 1, at(3005)));
        assert!(!d.feed(Key::KEY_P, 1, at(3010)));
    }

    #[test]
    fn parses_captured_bluetooth_hotkey_reports() {
        assert_eq!(
            parse_hotkey_report(&[0x5a, 0xc7, 0x00, 0x00, 0x00, 0x00]),
            Some(BluetoothHotkeyAction::BacklightCycle)
        );
        assert_eq!(
            parse_hotkey_report(&[0x5a, 0x10, 0x00, 0x00, 0x00, 0x00]),
            Some(BluetoothHotkeyAction::BrightnessDown)
        );
        assert_eq!(
            parse_hotkey_report(&[0x5a, 0x20, 0x00, 0x00, 0x00, 0x00]),
            Some(BluetoothHotkeyAction::BrightnessUp)
        );
    }

    #[test]
    fn parses_the_other_asus_key_reports() {
        let report = |code| parse_hotkey_report(&[0x5a, code, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(report(0x9c), Some(BluetoothHotkeyAction::Key(Key::KEY_PROG2)));
        assert_eq!(report(0x7c), Some(BluetoothHotkeyAction::Key(Key::KEY_MICMUTE)));
        assert_eq!(report(0x7e), Some(BluetoothHotkeyAction::Emoji));
        assert_eq!(report(0x86), Some(BluetoothHotkeyAction::ControlPanel));
        assert_eq!(report(0x6a), Some(BluetoothHotkeyAction::ScreenCycle));
        // Fn+Esc and the 5a 3d battery-level reports are not hotkeys.
        assert_eq!(report(0x4e), None);
        assert_eq!(report(0x3d), None);
    }

    #[test]
    fn every_emitted_key_is_registered_on_the_virtual_keyboard() {
        for code in [0x9c, 0x7c] {
            if let Some(BluetoothHotkeyAction::Key(key)) =
                parse_hotkey_report(&[0x5a, code, 0, 0, 0, 0])
            {
                assert!(EMITTED_KEYS.contains(&key), "{key:?} not in EMITTED_KEYS");
            } else {
                panic!("{code:#x} should map to a key");
            }
        }
    }

    #[test]
    fn finds_report_id_items_in_descriptors() {
        // Usage Page (0x06 ff31), Report ID 0x5a, then a 4-byte item containing 0x85 0x5a
        // as data that must not be mistaken for an item.
        let vendor = [0x06, 0x31, 0xff, 0x85, 0x5a, 0x09, 0x76];
        assert!(declares_report_id(&vendor, 0x5a));
        let data_only = [0x27, 0x85, 0x5a, 0x00, 0x00, 0x85, 0x01];
        assert!(!declares_report_id(&data_only, 0x5a));
        assert!(declares_report_id(&data_only, 0x01));
    }

    #[test]
    fn ignores_release_and_unrelated_reports() {
        assert_eq!(
            parse_hotkey_report(&[0x5a, 0x00, 0x00, 0x00, 0x00, 0x00]),
            None
        );
        assert_eq!(parse_hotkey_report(&[0x59, 0xc7, 0x00]), None);
        assert_eq!(parse_hotkey_report(&[0x5a]), None);
    }

    #[test]
    fn identifies_bluetooth_keyboard_hidraw_uevents() {
        assert!(is_bluetooth_keyboard_uevent(
            "HID_ID=0005:00000B05:00001B2C\nHID_NAME=ASUS Zenbook Duo Keyboard\n"
        ));
        assert!(!is_bluetooth_keyboard_uevent(
            "HID_ID=0003:00000B05:00001B2C\nHID_NAME=ASUS Zenbook Duo Keyboard\n"
        ));
        assert!(!is_bluetooth_keyboard_uevent(
            "HID_ID=0005:00001234:00005678\nHID_NAME=Other Keyboard\n"
        ));
    }

    #[test]
    fn brightness_steps_use_five_percent_chunks() {
        assert_eq!(
            next_brightness_value(200, 400, BluetoothHotkeyAction::BrightnessUp),
            220
        );
        assert_eq!(
            next_brightness_value(200, 400, BluetoothHotkeyAction::BrightnessDown),
            180
        );
        assert_eq!(
            next_brightness_value(395, 400, BluetoothHotkeyAction::BrightnessUp),
            400
        );
        assert_eq!(
            next_brightness_value(5, 400, BluetoothHotkeyAction::BrightnessDown),
            0
        );
    }
}
