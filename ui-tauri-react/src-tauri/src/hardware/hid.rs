use std::fs;
use std::os::unix::io::AsRawFd;

use rusb::UsbContext;

use crate::models::ConnectionType;

/// USB HID SET_REPORT for keyboard backlight control using rusb.
///
/// Protocol:
///   Report ID: 0x5A
///   Data: [0x5A, 0xBA, 0xC5, 0xC4, level, 0x00 x 11]
///   wValue: 0x035A, wIndex: 4, wLength: 16
pub fn set_backlight_usb(level: u8) -> Result<(), String> {
    let level = level.min(3);
    // Through hidraw first, like the mic LED and Fn lock: claiming the USB
    // interface below detaches the kernel driver, so the keyboard's vendor
    // interface (and the key reports on it) is gone for about a second.
    if send_vendor_feature_to(&[VENDOR_REPORT_ID, 0xBA, 0xC5, 0xC4, level], Nodes::Usb).is_ok() {
        return Ok(());
    }

    let context = rusb::Context::new().map_err(|e| format!("USB context error: {e}"))?;
    let devices = context
        .devices()
        .map_err(|e| format!("USB device list error: {e}"))?;

    for device in devices.iter() {
        let desc: rusb::DeviceDescriptor = match device.device_descriptor() {
            Ok(d) => d,
            Err(_) => continue,
        };

        // ASUS Zenbook Duo keyboard: vendor 0x0B05
        if desc.vendor_id() != 0x0B05 {
            continue;
        }

        let handle: rusb::DeviceHandle<rusb::Context> = match device.open() {
            Ok(h) => h,
            Err(_) => continue,
        };

        // Check if this is the keyboard by reading product string
        if let Ok(product) = handle.read_product_string_ascii(&desc) {
            if !product.contains("Zenbook Duo Keyboard") && !product.contains("ASUS_DUO") {
                continue;
            }
        } else {
            continue;
        }

        // Detach kernel driver if needed
        let interface = 4u8;
        let _ = handle.set_auto_detach_kernel_driver(true);
        let _ = handle.claim_interface(interface as u8);

        // Build the HID SET_REPORT payload
        let mut data = [0u8; 16];
        data[0] = 0x5A; // Report ID
        data[1] = 0xBA;
        data[2] = 0xC5;
        data[3] = 0xC4;
        data[4] = level;

        // HID SET_REPORT: bmRequestType=0x21, bRequest=0x09
        // wValue = 0x0300 | report_id = 0x035A
        // wIndex = interface number
        let request_type = 0x21; // Host-to-device, class, interface
        let request = 0x09; // SET_REPORT
        let value = 0x035A; // Feature report, ID 0x5A
        let index = interface as u16;
        let timeout = std::time::Duration::from_secs(2);

        handle
            .write_control(request_type, request, value, index, &data, timeout)
            .map_err(|e| format!("USB write error: {e}"))?;

        return Ok(());
    }

    Err("Zenbook Duo keyboard not found via USB".into())
}

/// Bluetooth HID Feature Report for keyboard backlight (`5a ba c5 c4 <level>`).
/// The detached keyboard has two hidraw nodes; only the one declaring the
/// vendor report accepts it (the other fails with an I/O error).
pub fn set_backlight_bluetooth(level: u8) -> Result<(), String> {
    send_vendor_feature_to(&[VENDOR_REPORT_ID, 0xBA, 0xC5, 0xC4, level.min(3)], Nodes::Bluetooth)
}

/// The keyboard's vendor report: ASUS keys, backlight and Fn lock.
const VENDOR_REPORT_ID: u8 = 0x5A;

/// The keyboard's own Fn lock (feature report `5a d0 4e <on>`, captured from
/// the Windows driver): on, the top row sends F1-F12 and Fn+key the ASUS
/// functions; off, the other way round. The keyboard forgets it on every
/// (re)connect, docked and detached. Only for the keyboard it was captured
/// on (see [`has_hardware_fn_lock`]).
pub fn set_fn_lock_bluetooth(locked: bool) -> Result<(), String> {
    send_vendor_feature_to(&[VENDOR_REPORT_ID, 0xD0, 0x4E, u8::from(locked)], Nodes::CapturedBluetooth)
}

/// The same for the docked keyboard (USB).
pub fn set_fn_lock_usb(locked: bool) -> Result<(), String> {
    send_vendor_feature_to(&[VENDOR_REPORT_ID, 0xD0, 0x4E, u8::from(locked)], Nodes::CapturedUsb)
}

/// The keyboards the Fn lock command was captured on: the
/// UX8406MA's keyboard, docked (0b05:1b2c) and detached (0b05:1b2d). Other
/// Duo keyboards keep the software Fn handling.
const CAPTURED_KEYBOARDS: [&str; 2] = [":00000B05:00001B2C", ":00000B05:00001B2D"];

/// Whether a USB or Bluetooth product ID is one of [`CAPTURED_KEYBOARDS`].
pub fn has_hardware_fn_lock(vendor: u16, product: u16) -> bool {
    vendor == 0x0B05 && matches!(product, 0x1B2C | 0x1B2D)
}

/// Which of the keyboard's vendor hidraw nodes a report goes to.
#[derive(Clone, Copy)]
enum Nodes {
    Usb,
    Bluetooth,
    Captured,
    CapturedUsb,
    CapturedBluetooth,
}

impl Nodes {
    fn accepts(self, uevent: &str) -> bool {
        let bluetooth = uevent.contains("HID_ID=0005:");
        let captured = is_captured_keyboard(uevent);
        match self {
            Nodes::Usb => !bluetooth,
            Nodes::Bluetooth => bluetooth,
            Nodes::Captured => captured,
            Nodes::CapturedUsb => captured && !bluetooth,
            Nodes::CapturedBluetooth => captured && bluetooth,
        }
    }
}

/// Sends a 16-byte feature report to the keyboard's hidraw nodes that carry
/// the vendor report and match `which`. Succeeds when one accepts it.
fn send_vendor_feature_to(payload: &[u8], which: Nodes) -> Result<(), String> {
    let nodes: Vec<String> = vendor_hidraw_nodes()
        .into_iter()
        .filter(|(_, uevent)| which.accepts(uevent))
        .map(|(node, _)| node)
        .collect();
    if nodes.is_empty() {
        return Err("Zenbook Duo keyboard not found".into());
    }
    let mut errors = Vec::new();
    for node in &nodes {
        let mut data = [0u8; 16];
        data[..payload.len()].copy_from_slice(payload);
        if let Err(err) = write_feature(node, &mut data) {
            errors.push(format!("{node}: {err}"));
        }
    }
    if errors.len() == nodes.len() {
        return Err(errors.join("; "));
    }
    Ok(())
}

fn write_feature(node: &str, data: &mut [u8; 16]) -> Result<(), String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(node)
        .map_err(|e| e.to_string())?;
    // HIDIOCSFEATURE(16) = _IOC(_IOC_WRITE|_IOC_READ, 'H', 0x06, 16)
    let hidiocsfeature: libc::c_ulong = 0xC010_4806;
    let ret = unsafe { libc::ioctl(file.as_raw_fd(), hidiocsfeature, data.as_mut_ptr()) };
    if ret < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

/// hidraw nodes (with their uevent) of the Duo keyboard whose report
/// descriptor declares the vendor report. The Bluetooth keyboard has a second
/// node without it, which rejects the report.
fn vendor_hidraw_nodes() -> Vec<(String, String)> {
    let Ok(entries) = fs::read_dir("/sys/class/hidraw") else {
        return Vec::new();
    };
    let mut nodes: Vec<(String, String)> = entries
        .flatten()
        .filter_map(|entry| {
            let device = entry.path().join("device");
            let uevent = fs::read_to_string(device.join("uevent")).unwrap_or_default();
            let descriptor = fs::read(device.join("report_descriptor")).unwrap_or_default();
            (is_duo_keyboard(&uevent) && declares_report_id(&descriptor, VENDOR_REPORT_ID)).then(|| {
                (format!("/dev/{}", entry.file_name().to_string_lossy()), uevent)
            })
        })
        .collect();
    nodes.sort();
    nodes
}

/// The Duo keyboard by name, as the backlight lookup always matched it, so
/// models with other product IDs are found too.
fn is_duo_keyboard(uevent: &str) -> bool {
    uevent.contains("Zenbook Duo Keyboard") || uevent.contains("ASUS_DUO") || is_captured_keyboard(uevent)
}

pub(crate) fn is_captured_keyboard(uevent: &str) -> bool {
    uevent.lines().any(|line| {
        let id = line.trim().to_ascii_uppercase();
        id.starts_with("HID_ID=") && CAPTURED_KEYBOARDS.iter().any(|captured| id.ends_with(captured))
    })
}

/// Whether a HID report descriptor has a Report ID item with this ID.
fn declares_report_id(descriptor: &[u8], report_id: u8) -> bool {
    let mut i = 0;
    while i < descriptor.len() {
        let prefix = descriptor[i];
        if prefix == 0xFE {
            // Long item: data size in the next byte, plus the tag byte.
            let size = descriptor.get(i + 1).copied().unwrap_or(0) as usize;
            i += 3 + size;
            continue;
        }
        let size = match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
        // Report ID: global item, tag 8 (prefix 0x85 with one data byte).
        if prefix & 0xFC == 0x84 && size >= 1 && descriptor.get(i + 1) == Some(&report_id) {
            return true;
        }
        i += 1 + size;
    }
    false
}

/// Set backlight, trying USB first then Bluetooth.
pub fn set_backlight(level: u8) -> Result<(), String> {
    // Try USB first
    let usb_err = match set_backlight_usb(level) {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };

    // Try Bluetooth
    let bt_err = match set_backlight_bluetooth(level) {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };

    Err(format!(
        "Failed to set keyboard backlight natively (usb: {usb_err}; bt: {bt_err})"
    ))
}

/// Apply the keyboard backlight for a lifecycle phase, best effort.
///
/// The backlight is cosmetic, so it never fails a lifecycle phase: a keyboard that
/// is absent, asleep over Bluetooth (HID ioctl EIO) or otherwise unreachable must
/// not stop boot, resume or `./install.sh`. A missing keyboard returns `None`
/// quietly; any other failure is logged and returned so callers can record it.
pub fn set_backlight_if_keyboard_present(level: u8) -> Option<String> {
    set_backlight_for_lifecycle(crate::hardware::sysfs::detect_connection_type, || {
        set_backlight(level)
    })
}

fn set_backlight_for_lifecycle<D, F>(mut detect: D, set: F) -> Option<String>
where
    D: FnMut() -> ConnectionType,
    F: FnOnce() -> Result<(), String>,
{
    if detect() == ConnectionType::None {
        log::warn!(
            "keyboard backlight unavailable during lifecycle: detachable keyboard is absent"
        );
        return None;
    }

    match set() {
        Ok(()) => None,
        Err(err) if detect() == ConnectionType::None => {
            log::warn!(
                "keyboard backlight became unavailable during lifecycle; continuing because the detachable keyboard is absent: {err}"
            );
            None
        }
        Err(err) => {
            let warning = format!(
                "keyboard backlight not applied during lifecycle; continuing: {err}"
            );
            log::warn!("{warning}");
            Some(warning)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn attached_usb_keyboard_applies_lifecycle_backlight() {
        let called = Cell::new(false);
        let result = set_backlight_for_lifecycle(
            || ConnectionType::Usb,
            || {
                called.set(true);
                Ok(())
            },
        );

        assert_eq!(result, None);
        assert!(called.get());
    }

    #[test]
    fn attached_bluetooth_keyboard_applies_lifecycle_backlight() {
        let called = Cell::new(false);
        let result = set_backlight_for_lifecycle(
            || ConnectionType::Bluetooth,
            || {
                called.set(true);
                Ok(())
            },
        );

        assert_eq!(result, None);
        assert!(called.get());
    }

    #[test]
    fn entirely_absent_keyboard_is_recoverable() {
        let called = Cell::new(false);
        let result = set_backlight_for_lifecycle(
            || ConnectionType::None,
            || {
                called.set(true);
                Err("keyboard missing".into())
            },
        );

        assert_eq!(result, None);
        assert!(!called.get());
    }

    #[test]
    fn keyboard_disappearing_during_suspend_is_recoverable() {
        let detections = Cell::new(0);
        let result = set_backlight_for_lifecycle(
            || {
                let count = detections.get();
                detections.set(count + 1);
                if count == 0 {
                    ConnectionType::Usb
                } else {
                    ConnectionType::None
                }
            },
            || Err("USB device disappeared".into()),
        );

        assert_eq!(result, None);
        assert_eq!(detections.get(), 2);
    }

    #[test]
    fn unavailable_backlight_is_recoverable_when_keyboard_is_absent() {
        let result = set_backlight_for_lifecycle(
            || ConnectionType::None,
            || Err("backlight unavailable".into()),
        );

        assert_eq!(result, None);
    }

    #[test]
    fn attached_keyboard_failure_is_reported_but_never_fatal() {
        let result = set_backlight_for_lifecycle(
            || ConnectionType::Usb,
            || Err("permission denied while writing attached keyboard".into()),
        );

        let warning = result.expect("failure on an attached keyboard should be reported");
        assert!(warning.contains("permission denied while writing attached keyboard"));
    }

    #[test]
    fn unreachable_bluetooth_backlight_does_not_fail_lifecycle() {
        // Duo 2026-10-15: keyboard detached, paired over Bluetooth but asleep, so
        // USB finds nothing and HIDIOCSFEATURE returns EIO. Boot must still succeed.
        let err = "Failed to set keyboard backlight natively (usb: Zenbook Duo keyboard not found via USB; bt: ioctl HIDIOCSFEATURE failed: Input/output error (os error 5))";
        let result = set_backlight_for_lifecycle(
            || ConnectionType::Bluetooth,
            || Err(err.into()),
        );

        let warning = result.expect("unreachable backlight should be reported");
        assert!(warning.contains("os error 5"));
    }

    #[test]
    fn finds_the_vendor_report_id_in_a_report_descriptor() {
        // Usage page (vendor), collection, report ID 0x5a, report size, end.
        let vendor = [0x06, 0x31, 0xFF, 0xA1, 0x01, 0x85, 0x5A, 0x75, 0x08, 0xC0];
        assert!(declares_report_id(&vendor, 0x5A));
        // 0x85 0x5a as data of another item (logical maximum, 2 bytes) is not one.
        let data_only = [0x26, 0x85, 0x5A, 0x85, 0x01, 0xC0];
        assert!(!declares_report_id(&data_only, 0x5A));
        assert!(!declares_report_id(&[], 0x5A));
    }

    #[test]
    fn finds_duo_keyboards_by_name_and_gates_the_captured_commands() {
        let docked = "DRIVER=asus\nHID_ID=0003:00000B05:00001B2C\nHID_NAME=ASUS Zenbook Duo Keyboard\n";
        let detached = "HID_ID=0005:00000b05:00001b2d\nHID_NAME=ASUS Zenbook Duo Keyboard\n";
        let other_model = "HID_ID=0003:00000B05:00001CD7\nHID_NAME=ASUS Zenbook Duo Keyboard\n";
        assert!(is_duo_keyboard(docked) && is_duo_keyboard(detached) && is_duo_keyboard(other_model));
        assert!(!is_duo_keyboard("HID_ID=0003:00000B05:000019B6\nHID_NAME=ASUS Keyboard\n"));

        // Backlight: every Duo keyboard. Fn lock: the captured ones.
        assert!(Nodes::Usb.accepts(other_model));
        assert!(!Nodes::CapturedUsb.accepts(other_model) && !Nodes::Captured.accepts(other_model));
        assert!(Nodes::CapturedUsb.accepts(docked) && !Nodes::CapturedUsb.accepts(detached));
        assert!(Nodes::CapturedBluetooth.accepts(detached) && !Nodes::CapturedBluetooth.accepts(docked));
        assert!(has_hardware_fn_lock(0x0B05, 0x1B2C) && !has_hardware_fn_lock(0x0B05, 0x1CD7));
    }
}
