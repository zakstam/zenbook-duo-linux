use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

use crate::commands::events::{push_event, EventBuffer};
use crate::models::{EventCategory, HardwareEvent};
use crate::runtime::paths;

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

        let runtime_dir = paths::system_runtime_dir();
        if runtime_dir.exists() {
            let _ = watcher.watch(runtime_dir.as_path(), RecursiveMode::NonRecursive);
        }

        // Watch the primary sysfs backlight when one is available.
        if let Some(backlight_path) =
            crate::hardware::sysfs::primary_backlight_dir().map(|dir| dir.join("brightness"))
        {
            let _ = watcher.watch(backlight_path.as_path(), RecursiveMode::NonRecursive);
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
                            "state.json" => {
                                let _ = app.emit("duo://status-changed", ());
                                let hw_event = HardwareEvent::info(
                                    EventCategory::Service,
                                    "Runtime state updated",
                                    "file_watcher",
                                );
                                push_event(&buffer, hw_event);
                                let _ = app.emit("duo://hardware-event", ());
                            }
                            "daemon.log" => {
                                let _ = app.emit("duo://log-updated", ());
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
