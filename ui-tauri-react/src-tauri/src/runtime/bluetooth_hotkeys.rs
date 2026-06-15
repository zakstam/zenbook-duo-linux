use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use crate::models::{ConnectionType, EventCategory, HardwareEvent};
use crate::runtime::{logger, state::RuntimeState};

const HIDRAW_ROOT: &str = "/sys/class/hidraw";
const REPORT_ID: u8 = 0x5a;
const REPORT_RELEASE: u8 = 0x00;
const REPORT_BACKLIGHT_CYCLE: u8 = 0xc7;
const REPORT_BRIGHTNESS_DOWN: u8 = 0x10;
const REPORT_BRIGHTNESS_UP: u8 = 0x20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BluetoothHotkeyAction {
    BacklightCycle,
    BrightnessDown,
    BrightnessUp,
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
                if let Err(err) = watch_device(file, state.clone()) {
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

fn watch_device(mut file: fs::File, state: Arc<RwLock<RuntimeState>>) -> Result<(), String> {
    let mut active_action: Option<BluetoothHotkeyAction> = None;
    let mut buffer = [0_u8; 64];

    loop {
        match file.read(&mut buffer) {
            Ok(0) => return Err("hidraw read returned EOF".into()),
            Ok(count) => {
                let report = &buffer[..count];
                if is_release_report(report) {
                    active_action = None;
                    continue;
                }

                let Some(action) = parse_hotkey_report(report) else {
                    continue;
                };

                if active_action == Some(action) {
                    continue;
                }
                active_action = Some(action);

                if let Err(err) = handle_action(action, &state) {
                    log::warn!("failed to handle Bluetooth hotkey {action:?}: {err}");
                    let _ = logger::append_line(format!(
                        "rust-daemon: failed to handle Bluetooth hotkey {action:?}: {err}"
                    ));
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err.to_string()),
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
            Some(Path::new("/dev").join(entry.file_name()))
        })
        .collect::<Vec<_>>();
    devices.sort();
    devices.into_iter().next()
}

fn is_bluetooth_keyboard_uevent(contents: &str) -> bool {
    (contents.contains("Zenbook Duo Keyboard") || contents.contains("ASUS_DUO"))
        && contents.contains("HID_ID=0005:")
}

fn parse_hotkey_report(report: &[u8]) -> Option<BluetoothHotkeyAction> {
    if report.len() < 2 || report[0] != REPORT_ID {
        return None;
    }

    match report[1] {
        REPORT_BACKLIGHT_CYCLE => Some(BluetoothHotkeyAction::BacklightCycle),
        REPORT_BRIGHTNESS_DOWN => Some(BluetoothHotkeyAction::BrightnessDown),
        REPORT_BRIGHTNESS_UP => Some(BluetoothHotkeyAction::BrightnessUp),
        _ => None,
    }
}

fn is_release_report(report: &[u8]) -> bool {
    report.len() >= 2 && report[0] == REPORT_ID && report[1] == REPORT_RELEASE
}

fn handle_action(
    action: BluetoothHotkeyAction,
    state: &Arc<RwLock<RuntimeState>>,
) -> Result<(), String> {
    if !matches!(
        crate::hardware::sysfs::detect_connection_type(),
        ConnectionType::Bluetooth
    ) {
        return Ok(());
    }

    match action {
        BluetoothHotkeyAction::BacklightCycle => cycle_backlight(state),
        BluetoothHotkeyAction::BrightnessDown | BluetoothHotkeyAction::BrightnessUp => {
            step_brightness(action, state)
        }
    }
}

fn cycle_backlight(state: &Arc<RwLock<RuntimeState>>) -> Result<(), String> {
    let current = state.blocking_read().status.backlight_level;
    let next = (current + 1) % 4;
    crate::hardware::hid::set_backlight(next)?;

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
        BluetoothHotkeyAction::BacklightCycle => current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
