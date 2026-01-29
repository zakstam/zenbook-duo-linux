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
                log::error!("Failed to create file watcher: {e}");
                return;
            }
        };

        // Watch /tmp/duo/ directory
        let duo_dir = Path::new("/tmp/duo");
        if duo_dir.exists() {
            let _ = watcher.watch(duo_dir, RecursiveMode::NonRecursive);
        }

        // Watch sysfs backlight
        let backlight_path = Path::new("/sys/class/backlight/intel_backlight/brightness");
        if backlight_path.exists() {
            let _ = watcher.watch(backlight_path, RecursiveMode::NonRecursive);
        }

        for event in rx {
            match event {
                Ok(event) => {
                    if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                        continue;
                    }

                    for path in &event.paths {
                        let filename = path
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_default();

                        match filename.as_str() {
                            "status" => {
                                let _ = app.emit("duo://status-changed", ());
                                let hw_event = HardwareEvent::info(
                                    EventCategory::Service,
                                    "Status file updated",
                                    "file_watcher",
                                );
                                push_event(&buffer, hw_event);
                                let _ = app.emit("duo://hardware-event", ());
                            }
                            "duo.log" => {
                                let _ = app.emit("duo://log-updated", ());
                            }
                            "kb_backlight_level" => {
                                let _ = app.emit("duo://status-changed", ());
                                let hw_event = HardwareEvent::info(
                                    EventCategory::Keyboard,
                                    "Keyboard backlight changed",
                                    "file_watcher",
                                );
                                push_event(&buffer, hw_event);
                                let _ = app.emit("duo://hardware-event", ());
                            }
                            "brightness" => {
                                let _ = app.emit("duo://status-changed", ());
                                let hw_event = HardwareEvent::info(
                                    EventCategory::Display,
                                    "Display brightness changed",
                                    "file_watcher",
                                );
                                push_event(&buffer, hw_event);
                                let _ = app.emit("duo://hardware-event", ());
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    log::error!("File watcher error: {e}");
                }
            }
        }
    });
}
