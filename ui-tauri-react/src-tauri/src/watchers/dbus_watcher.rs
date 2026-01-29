use tauri::{AppHandle, Emitter};

use crate::commands::events::{push_event, EventBuffer};
use crate::models::{EventCategory, HardwareEvent};

pub fn start(app: AppHandle, buffer: EventBuffer) {
    // NetworkManager Wi-Fi state watcher
    let app_nm = app.clone();
    let buf_nm = buffer.clone();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("Failed to create tokio runtime for NM watcher: {e}");
                return;
            }
        };
        rt.block_on(async {
            if let Err(e) = watch_network_manager(app_nm, buf_nm).await {
                log::error!("NetworkManager watcher failed: {e}");
            }
        });
    });

    // BlueZ Bluetooth state watcher
    let app_bz = app.clone();
    let buf_bz = buffer.clone();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("Failed to create tokio runtime for BlueZ watcher: {e}");
                return;
            }
        };
        rt.block_on(async {
            if let Err(e) = watch_bluez(app_bz, buf_bz).await {
                log::error!("BlueZ watcher failed: {e}");
            }
        });
    });

    // logind LockedHint watcher
    let app_li = app.clone();
    let buf_li = buffer;
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("Failed to create tokio runtime for logind watcher: {e}");
                return;
            }
        };
        rt.block_on(async {
            if let Err(e) = watch_logind(app_li, buf_li).await {
                log::error!("logind watcher failed: {e}");
            }
        });
    });
}

async fn watch_network_manager(app: AppHandle, buffer: EventBuffer) -> Result<(), zbus::Error> {
    let connection = zbus::Connection::system().await?;

    let proxy = zbus::fdo::PropertiesProxy::builder(&connection)
        .destination("org.freedesktop.NetworkManager")?
        .path("/org/freedesktop/NetworkManager")?
        .build()
        .await?;

    let mut stream = proxy.receive_properties_changed().await?;

    use futures_util::StreamExt;
    while let Some(signal) = stream.next().await {
        let args = signal.args().ok();
        if let Some(args) = args {
            if args.changed_properties().contains_key("WirelessEnabled") {
                let _ = app.emit("duo://status-changed", ());
                let hw_event = HardwareEvent::info(
                    EventCategory::Network,
                    "Wi-Fi state changed",
                    "dbus_watcher",
                );
                push_event(&buffer, hw_event);
                let _ = app.emit("duo://hardware-event", ());
            }
        }
    }

    Ok(())
}

async fn watch_bluez(app: AppHandle, buffer: EventBuffer) -> Result<(), zbus::Error> {
    let connection = zbus::Connection::system().await?;

    let proxy = zbus::fdo::PropertiesProxy::builder(&connection)
        .destination("org.bluez")?
        .path("/org/bluez/hci0")?
        .build()
        .await?;

    let mut stream = proxy.receive_properties_changed().await?;

    use futures_util::StreamExt;
    while let Some(signal) = stream.next().await {
        let args = signal.args().ok();
        if let Some(args) = args {
            if args.changed_properties().contains_key("Powered") {
                let _ = app.emit("duo://status-changed", ());
                let hw_event = HardwareEvent::info(
                    EventCategory::Bluetooth,
                    "Bluetooth state changed",
                    "dbus_watcher",
                );
                push_event(&buffer, hw_event);
                let _ = app.emit("duo://hardware-event", ());
            }
        }
    }

    Ok(())
}

async fn watch_logind(app: AppHandle, buffer: EventBuffer) -> Result<(), zbus::Error> {
    let connection = zbus::Connection::system().await?;

    // Find the current session
    let manager = zbus::fdo::PropertiesProxy::builder(&connection)
        .destination("org.freedesktop.login1")?
        .path("/org/freedesktop/login1")?
        .build()
        .await?;

    let mut stream = manager.receive_properties_changed().await?;

    use futures_util::StreamExt;
    while let Some(signal) = stream.next().await {
        let args = signal.args().ok();
        if let Some(args) = args {
            if args.changed_properties().contains_key("LockedHint") {
                let _ = app.emit("duo://status-changed", ());
                let hw_event = HardwareEvent::info(
                    EventCategory::Display,
                    "Screen lock state changed",
                    "dbus_watcher",
                );
                push_event(&buffer, hw_event);
                let _ = app.emit("duo://hardware-event", ());
            }
        }
    }

    Ok(())
}
