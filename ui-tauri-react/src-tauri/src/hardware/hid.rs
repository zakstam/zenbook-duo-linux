use std::fs;
use std::os::unix::io::AsRawFd;
use std::path::Path;

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

/// Bluetooth HID Feature Report for keyboard backlight using ioctl HIDIOCSFEATURE.
pub fn set_backlight_bluetooth(level: u8) -> Result<(), String> {
    let level = level.min(3);

    // Find the hidraw device for the Zenbook keyboard
    let hidraw_path = find_bt_hidraw()?;

    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&hidraw_path)
        .map_err(|e| format!("Failed to open {hidraw_path}: {e}"))?;

    let fd = file.as_raw_fd();

    // Build the same payload
    let mut data = [0u8; 16];
    data[0] = 0x5A;
    data[1] = 0xBA;
    data[2] = 0xC5;
    data[3] = 0xC4;
    data[4] = level;

    // HIDIOCSFEATURE = 0xC0104806 + len
    // This is _IOC(_IOC_WRITE|_IOC_READ, 'H', 0x06, len)
    // For 16 bytes: 0xC0104806
    let hidiocsfeature: libc::c_ulong = 0xC010_4806;

    let ret = unsafe { libc::ioctl(fd, hidiocsfeature, data.as_mut_ptr()) };

    if ret < 0 {
        return Err(format!(
            "ioctl HIDIOCSFEATURE failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    Ok(())
}

fn find_bt_hidraw() -> Result<String, String> {
    let hidraw_dir = Path::new("/sys/class/hidraw");
    if let Ok(entries) = fs::read_dir(hidraw_dir) {
        for entry in entries.flatten() {
            let uevent_path = entry.path().join("device/uevent");
            if let Ok(contents) = fs::read_to_string(&uevent_path) {
                if (contents.contains("Zenbook Duo Keyboard") || contents.contains("ASUS_DUO"))
                    && contents.contains("0005:")
                {
                    let name = entry.file_name();
                    return Ok(format!("/dev/{}", name.to_string_lossy()));
                }
            }
        }
    }
    Err("Bluetooth hidraw device not found".into())
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

/// Apply a lifecycle backlight restore only when the detachable keyboard is present.
/// A missing keyboard is a normal, recoverable dock state; failures remain visible
/// when USB or Bluetooth detection says the keyboard is attached.
pub fn set_backlight_if_keyboard_present(level: u8) -> Result<(), String> {
    set_backlight_for_lifecycle(crate::hardware::sysfs::detect_connection_type, || {
        set_backlight(level)
    })
}

fn set_backlight_for_lifecycle<D, F>(mut detect: D, set: F) -> Result<(), String>
where
    D: FnMut() -> ConnectionType,
    F: FnOnce() -> Result<(), String>,
{
    if detect() == ConnectionType::None {
        log::warn!(
            "keyboard backlight unavailable during lifecycle: detachable keyboard is absent"
        );
        return Ok(());
    }

    match set() {
        Ok(()) => Ok(()),
        Err(err) if detect() == ConnectionType::None => {
            log::warn!(
                "keyboard backlight became unavailable during lifecycle; continuing because the detachable keyboard is absent: {err}"
            );
            Ok(())
        }
        Err(err) => Err(err),
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

        assert_eq!(result, Ok(()));
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

        assert_eq!(result, Ok(()));
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

        assert_eq!(result, Ok(()));
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

        assert_eq!(result, Ok(()));
        assert_eq!(detections.get(), 2);
    }

    #[test]
    fn unavailable_backlight_is_recoverable_when_keyboard_is_absent() {
        let result = set_backlight_for_lifecycle(
            || ConnectionType::None,
            || Err("backlight unavailable".into()),
        );

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn unrelated_fatal_lifecycle_error_is_not_suppressed() {
        let result = set_backlight_for_lifecycle(
            || ConnectionType::Usb,
            || Err("permission denied while writing attached keyboard".into()),
        );

        assert_eq!(
            result,
            Err("permission denied while writing attached keyboard".into())
        );
    }
}
