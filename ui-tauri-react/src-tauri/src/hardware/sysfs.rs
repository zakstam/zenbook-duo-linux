use std::fs;
use std::path::{Path, PathBuf};

use crate::models::{ConnectionType, DuoStatus, Orientation};
use crate::runtime::{paths, state::RuntimeState};

const BACKLIGHT_ROOT: &str = "/sys/class/backlight";

fn load_runtime_state() -> Option<RuntimeState> {
    let path = paths::state_file_path();
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn backlight_dirs_from(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .map(|entry| entry.path())
        .filter(|path| path.join("brightness").exists())
        .collect();
    dirs.sort();
    dirs
}

fn backlight_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn is_secondary_backlight_name(name: &str) -> bool {
    name.contains("edp-2") || name.contains("edp2")
}

fn primary_backlight_dir_from(root: &Path) -> Option<PathBuf> {
    let dirs = backlight_dirs_from(root);
    dirs.iter()
        .find(|path| backlight_name(path) == "intel_backlight")
        .or_else(|| {
            dirs.iter()
                .find(|path| backlight_name(path).contains("edp-1"))
        })
        .or_else(|| {
            dirs.iter()
                .find(|path| !is_secondary_backlight_name(&backlight_name(path)))
        })
        .cloned()
}

fn secondary_backlight_dir_from(root: &Path) -> Option<PathBuf> {
    backlight_dirs_from(root)
        .into_iter()
        .find(|path| is_secondary_backlight_name(&backlight_name(path)))
}

pub fn primary_backlight_dir() -> Option<PathBuf> {
    primary_backlight_dir_from(Path::new(BACKLIGHT_ROOT))
}

pub fn secondary_backlight_dir() -> Option<PathBuf> {
    secondary_backlight_dir_from(Path::new(BACKLIGHT_ROOT))
}

fn read_backlight_value(path: &Path) -> Option<u32> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// The keyboard backlight level the daemon last set, if its state is readable.
pub fn load_backlight_level() -> Option<u8> {
    load_runtime_state().map(|state| state.status.backlight_level)
}

pub fn read_backlight_level() -> u8 {
    load_runtime_state()
        .map(|state| state.status.backlight_level)
        .unwrap_or(0)
}

pub fn read_display_brightness() -> u32 {
    primary_backlight_dir()
        .and_then(|dir| read_backlight_value(&dir.join("brightness")))
        .unwrap_or(0)
}

pub fn read_max_brightness() -> u32 {
    primary_backlight_dir()
        .and_then(|dir| read_backlight_value(&dir.join("max_brightness")))
        .unwrap_or(1)
}

pub fn detect_connection_type() -> ConnectionType {
    let hidraw_dir = Path::new("/sys/class/hidraw");
    if !hidraw_dir.exists() {
        return ConnectionType::None;
    }

    let mut saw_docked_usb = false;
    let mut saw_usb_cable = false;
    let mut saw_bluetooth = false;

    if let Ok(entries) = fs::read_dir(hidraw_dir) {
        for entry in entries.flatten() {
            let uevent_path = entry.path().join("device/uevent");
            if let Ok(contents) = fs::read_to_string(&uevent_path) {
                if contents.contains("Zenbook Duo Keyboard") || contents.contains("ASUS_DUO") {
                    // Prefer bus id detection: HID_ID=0005:... is Bluetooth HID.
                    if contents.contains("HID_ID=0005:") {
                        saw_bluetooth = true;
                        continue;
                    }

                    // HID_ID=0003:... is USB HID; treat unknown as USB-ish. The keyboard
                    // is USB both when docked on the pogo pins and when plugged into a
                    // USB-C port by cable (e.g. to charge); only the pogo pins mean docked.
                    if hidraw_is_on_usb_cable(&entry.path()) {
                        saw_usb_cable = true;
                    } else {
                        saw_docked_usb = true;
                    }
                }
            }
        }
    }

    // The keyboard can expose both paired Bluetooth and docked USB HID interfaces at the
    // same time. Prefer USB so docked behavior, including the media remap helper, stays active.
    classify_connection_type(saw_docked_usb, saw_bluetooth, saw_usb_cable)
}

fn classify_connection_type(
    saw_docked_usb: bool,
    saw_bluetooth: bool,
    saw_usb_cable: bool,
) -> ConnectionType {
    if saw_docked_usb {
        ConnectionType::Usb
    } else if saw_bluetooth {
        ConnectionType::Bluetooth
    } else if saw_usb_cable {
        ConnectionType::UsbCable
    } else {
        ConnectionType::None
    }
}

/// Whether a USB hidraw node (`/sys/class/hidraw/hidrawN`) is the keyboard on a
/// cable (any external USB port) rather than docked on the pogo pins.
fn hidraw_is_on_usb_cable(hidraw_entry: &Path) -> bool {
    fs::canonicalize(hidraw_entry.join("device"))
        .ok()
        .and_then(|device| usb_device_of(&device))
        .map(|usb_device| {
            !usb_device_is_docked(&usb_device, crate::hardware::duo::dock_usb_port())
        })
        .unwrap_or(false)
}

/// The USB device a HID device belongs to: its first ancestor with `idVendor`.
fn usb_device_of(hid_device: &Path) -> Option<PathBuf> {
    hid_device
        .ancestors()
        .find(|dir| dir.join("idVendor").exists())
        .map(Path::to_path_buf)
}

/// Whether the keyboard's USB device is the docked one.
///
/// The pogo pins are an ordinary root-hub port: ACPI marks them hotplug like the
/// external ports and gives them the same physical location, so on a model with a
/// known pogo port only that port means docked (UX8406CA: port 6 of the
/// 0000:00:14.0 xHCI; the USB-C ports are 1-2 and the USB-A port 4). On other
/// models a port linked to a Type-C connector means a cable and anything else
/// counts as docked, as before.
fn usb_device_is_docked(usb_device: &Path, dock: Option<crate::hardware::duo::DockUsbPort>) -> bool {
    match dock {
        Some(dock) => {
            let devpath = fs::read_to_string(usb_device.join("devpath")).unwrap_or_default();
            devpath.trim() == dock.devpath
                && usb_device
                    .ancestors()
                    .any(|dir| dir.file_name().and_then(|n| n.to_str()) == Some(dock.controller))
        }
        None => fs::symlink_metadata(usb_device.join("port").join("connector")).is_err(),
    }
}

pub fn is_service_active() -> bool {
    is_unit_active_system("zenbook-duo-rust-daemon.service")
        && is_unit_active_user("zenbook-duo-session-agent.service")
}

fn is_unit_active_user(unit: &str) -> bool {
    std::process::Command::new("systemctl")
        .args(["--user", "is-active", unit])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false)
}

fn is_unit_active_system(unit: &str) -> bool {
    std::process::Command::new("systemctl")
        .args(["is-active", unit])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false)
}

pub fn get_full_status() -> DuoStatus {
    if let Some(mut status) = load_runtime_state().map(|state| state.status) {
        status.display_brightness = read_display_brightness();
        status.max_brightness = read_max_brightness();
        status.connection_type = detect_connection_type();
        status.service_active = is_service_active();
        return status;
    }

    DuoStatus {
        keyboard_attached: false,
        connection_type: detect_connection_type(),
        monitor_count: 0,
        wifi_enabled: false,
        bluetooth_enabled: false,
        backlight_level: read_backlight_level(),
        display_brightness: read_display_brightness(),
        max_brightness: read_max_brightness(),
        service_active: is_service_active(),
        orientation: Orientation::Normal,
        fn_lock: crate::usb_media_remap_helper::load_fn_lock(),
        keyboard_battery: None,
        mic_muted: None,
        hardware_fn_lock: false,
        desktop: None,
    }
}

pub fn clear_log() -> Result<(), String> {
    let runtime_path = paths::log_file_path();
    if let Some(parent) = runtime_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(runtime_path, "").map_err(|e| e.to_string())
}

pub fn read_log_lines(count: usize) -> Vec<String> {
    let runtime_path = paths::log_file_path();
    let contents = fs::read_to_string(&runtime_path).unwrap_or_default();

    contents
        .lines()
        .rev()
        .take(count)
        .map(String::from)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_usb_when_both_transport_interfaces_are_visible() {
        assert_eq!(classify_connection_type(true, true, false), ConnectionType::Usb);
    }

    #[test]
    fn returns_bluetooth_when_only_bluetooth_is_visible() {
        assert_eq!(
            classify_connection_type(false, true, false),
            ConnectionType::Bluetooth
        );
    }

    #[test]
    fn keyboard_on_a_usb_cable_is_not_docked() {
        assert_eq!(
            classify_connection_type(false, false, true),
            ConnectionType::UsbCable
        );
        // Charging by cable while paired: the Bluetooth link is the detached keyboard.
        assert_eq!(
            classify_connection_type(false, true, true),
            ConnectionType::Bluetooth
        );
        // Docked and a second keyboard on a cable: docked wins.
        assert_eq!(classify_connection_type(true, false, true), ConnectionType::Usb);
    }

    /// A fake xHCI with the keyboard on root port `n`, as the UX8406CA lays it out.
    #[cfg(unix)]
    fn fake_keyboard_on_port(root: &Path, n: u32, typec: bool) -> PathBuf {
        use std::os::unix::fs::symlink;
        let hub = root.join("0000:00:14.0/usb3/3-0:1.0");
        let port = hub.join(format!("usb3-port{n}"));
        fs::create_dir_all(&port).unwrap();
        if typec {
            let connector = root.join(format!("typec/port{n}"));
            fs::create_dir_all(&connector).unwrap();
            symlink(&connector, port.join("connector")).unwrap();
        }
        let usb = root.join(format!("0000:00:14.0/usb3/3-{n}"));
        fs::create_dir_all(usb.join(format!("3-{n}:1.0/0003:0B05:1BF2.0001"))).unwrap();
        fs::write(usb.join("idVendor"), "0b05").unwrap();
        fs::write(usb.join("devpath"), format!("{n}\n")).unwrap();
        symlink(&port, usb.join("port")).unwrap();
        usb
    }

    #[cfg(unix)]
    #[test]
    fn only_the_pogo_port_is_docked_on_a_known_model() {
        let root = unique_temp_dir("dock-port");
        let dock = Some(crate::hardware::duo::DockUsbPort { controller: "0000:00:14.0", devpath: "6" });
        let pogo = fake_keyboard_on_port(&root, 6, false);
        let usb_c = fake_keyboard_on_port(&root, 2, true);
        let usb_a = fake_keyboard_on_port(&root, 4, false);

        assert!(usb_device_is_docked(&pogo, dock));
        assert!(!usb_device_is_docked(&usb_c, dock));
        assert!(!usb_device_is_docked(&usb_a, dock));

        let hid = pogo.join("3-6:1.0/0003:0B05:1BF2.0001");
        assert_eq!(usb_device_of(&hid), Some(pogo.clone()));

        fs::remove_dir_all(root).expect("remove temp dir");
    }

    #[cfg(unix)]
    #[test]
    fn unknown_model_treats_only_typec_ports_as_cable() {
        let root = unique_temp_dir("dock-unknown");
        let pogo = fake_keyboard_on_port(&root, 6, false);
        let usb_c = fake_keyboard_on_port(&root, 1, true);

        assert!(usb_device_is_docked(&pogo, None));
        assert!(!usb_device_is_docked(&usb_c, None));

        fs::remove_dir_all(root).expect("remove temp dir");
    }

    #[test]
    fn returns_none_when_no_transport_is_visible() {
        assert_eq!(classify_connection_type(false, false, false), ConnectionType::None);
    }

    #[test]
    fn discovers_primary_and_secondary_backlight_devices() {
        let root = unique_temp_dir("backlight");
        create_backlight(&root, "card1-eDP-2-backlight");
        create_backlight(&root, "intel_backlight");

        assert_eq!(
            primary_backlight_dir_from(&root)
                .and_then(|path| path.file_name().map(|name| name.to_owned())),
            Some(std::ffi::OsString::from("intel_backlight"))
        );
        assert_eq!(
            secondary_backlight_dir_from(&root)
                .and_then(|path| path.file_name().map(|name| name.to_owned())),
            Some(std::ffi::OsString::from("card1-eDP-2-backlight"))
        );

        fs::remove_dir_all(root).expect("remove temp dir");
    }

    fn create_backlight(root: &Path, name: &str) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).expect("create backlight dir");
        fs::write(dir.join("brightness"), "100").expect("write brightness");
        fs::write(dir.join("max_brightness"), "400").expect("write max brightness");
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "zenbook-duo-sysfs-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }
}
