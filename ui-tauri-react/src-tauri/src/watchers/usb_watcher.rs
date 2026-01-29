use std::path::Path;

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

use crate::commands::events::{push_event, EventBuffer};
use crate::models::{EventCategory, HardwareEvent};

pub fn start(app: AppHandle, buffer: EventBuffer) {
    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel::<Result<Event, notify::Error>>();

        let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to create USB watcher: {e}");
                return;
            }
        };

        // Watch /dev/bus/usb for device changes
        let usb_dir = Path::new("/dev/bus/usb");
        if usb_dir.exists() {
            let _ = watcher.watch(usb_dir, RecursiveMode::Recursive);
        }

        // Watch hidraw for new HID devices
        let dev_dir = Path::new("/dev");
        if dev_dir.exists() {
            let _ = watcher.watch(dev_dir, RecursiveMode::NonRecursive);
        }

        for event in rx {
            match event {
                Ok(event) => {
                    if !matches!(event.kind, EventKind::Create(_) | EventKind::Remove(_)) {
                        continue;
                    }

                    let is_usb = event.paths.iter().any(|p| {
                        let s = p.to_string_lossy();
                        s.contains("/dev/bus/usb") || s.contains("hidraw")
                    });

                    if is_usb {
                        let msg = if matches!(event.kind, EventKind::Create(_)) {
                            "USB device connected"
                        } else {
                            "USB device disconnected"
                        };

                        let _ = app.emit("duo://keyboard-changed", ());
                        let _ = app.emit("duo://status-changed", ());

                        let hw_event = HardwareEvent::info(EventCategory::Usb, msg, "usb_watcher");
                        push_event(&buffer, hw_event);
                        let _ = app.emit("duo://hardware-event", ());
                    }
                }
                Err(e) => {
                    log::error!("USB watcher error: {e}");
                }
            }
        }
    });
}
