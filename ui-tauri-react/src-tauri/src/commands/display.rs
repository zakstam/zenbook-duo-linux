use crate::hardware::display_layout;
use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::models::{DisplayLayout, Orientation};
use crate::runtime::client;

fn daemon_display_layout_or_transport_fallback(
    response: Result<DaemonResponse, String>,
    fallback: impl FnOnce() -> Result<DisplayLayout, String>,
) -> Result<DisplayLayout, String> {
    match response {
        // Shown as the compositor has it; apply_display_layout stacks the screens.
        Ok(DaemonResponse::DisplayLayout { layout }) => Ok(layout),
        Ok(DaemonResponse::Error { message }) => Err(message),
        Ok(other) => Err(format!(
            "Unexpected daemon response while reading display layout: {other:?}"
        )),
        Err(_) => fallback(),
    }
}

fn daemon_ack_or_transport_fallback(
    response: Result<DaemonResponse, String>,
    action: &str,
    fallback: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    match response {
        Ok(DaemonResponse::Ack) => Ok(()),
        Ok(DaemonResponse::Error { message }) => Err(message),
        Ok(other) => Err(format!(
            "Unexpected daemon response while {action}: {other:?}"
        )),
        Err(_) => fallback(),
    }
}

/// The monitors' names by connector, as GNOME Settings shows them.
#[tauri::command]
pub fn get_monitor_names() -> std::collections::HashMap<String, String> {
    crate::hardware::display_layout::monitor_names()
}

#[tauri::command]
pub fn get_display_layout() -> Result<DisplayLayout, String> {
    daemon_display_layout_or_transport_fallback(
        client::request(DaemonRequest::GetDisplayLayout),
        display_layout::get_display_layout,
    )
}

#[tauri::command]
pub fn apply_display_layout(layout: DisplayLayout) -> Result<(), String> {
    let normalized = display_layout::normalize_display_layout(layout);

    daemon_ack_or_transport_fallback(
        client::request(DaemonRequest::ApplyDisplayLayout {
            layout: normalized.clone(),
        }),
        "applying display layout",
        || display_layout::apply_display_layout(&normalized),
    )
}

#[tauri::command]
pub fn set_orientation(orientation: Orientation) -> Result<(), String> {
    daemon_ack_or_transport_fallback(
        client::request(DaemonRequest::SetOrientation {
            orientation: orientation.clone(),
        }),
        "setting orientation",
        || display_layout::set_orientation(&orientation),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_layout() -> DisplayLayout {
        DisplayLayout { displays: vec![] }
    }

    #[test]
    fn display_layout_mapping_surfaces_daemon_failures() {
        assert_eq!(
            daemon_display_layout_or_transport_fallback(
                Ok(DaemonResponse::Error {
                    message: "daemon rejected".into(),
                }),
                || Ok(empty_layout()),
            )
            .expect_err("daemon error should fail"),
            "daemon rejected"
        );
        assert!(
            daemon_display_layout_or_transport_fallback(Ok(DaemonResponse::Pong), || {
                Ok(empty_layout())
            })
            .expect_err("unexpected response should fail")
            .contains("Unexpected daemon response")
        );
    }

    #[test]
    fn display_layout_read_keeps_a_bottom_screen_the_compositor_moved() {
        let screen = |connector: &str, x: i32, y: i32| {
            let mode = crate::models::DisplayMode {
                mode_id: "2880x1800@120".into(),
                backend_mode_id: None,
                width: 2880,
                height: 1800,
                refresh_rate: 120.0,
            };
            crate::models::DisplayInfo {
                connector: connector.into(),
                width: mode.width,
                height: mode.height,
                refresh_rate: mode.refresh_rate,
                scale: 1.66,
                x,
                y,
                transform: 0,
                primary: connector == "eDP-1",
                current_mode: mode.clone(),
                available_modes: vec![mode],
                refresh_policy: crate::models::RefreshPolicy::Fixed,
                supports_dynamic_refresh: false,
            }
        };
        let layout = DisplayLayout {
            displays: vec![screen("eDP-1", 0, 0), screen("eDP-2", 1737, 0)],
        };

        let read = daemon_display_layout_or_transport_fallback(
            Ok(DaemonResponse::DisplayLayout {
                layout: layout.clone(),
            }),
            || Ok(empty_layout()),
        )
        .expect("daemon layout should be returned");

        let bottom = read
            .displays
            .iter()
            .find(|display| display.connector == "eDP-2")
            .expect("bottom screen present");
        assert_eq!((bottom.x, bottom.y), (1737, 0));
    }

    #[test]
    fn display_layout_mapping_falls_back_on_transport_failure() {
        assert!(
            daemon_display_layout_or_transport_fallback(Err("socket missing".into()), || {
                Ok(empty_layout())
            })
            .is_ok()
        );
    }

    #[test]
    fn display_ack_mapping_surfaces_error_and_falls_back_only_on_transport_failure() {
        assert_eq!(
            daemon_ack_or_transport_fallback(
                Ok(DaemonResponse::Error {
                    message: "daemon rejected".into(),
                }),
                "testing",
                || Ok(()),
            )
            .expect_err("daemon error should fail"),
            "daemon rejected"
        );
        assert!(
            daemon_ack_or_transport_fallback(Ok(DaemonResponse::Pong), "testing", || Ok(()))
                .expect_err("unexpected response should fail")
                .contains("Unexpected daemon response")
        );
        assert!(
            daemon_ack_or_transport_fallback(Err("socket missing".into()), "testing", || {
                Ok(())
            })
            .is_ok()
        );
    }
}
