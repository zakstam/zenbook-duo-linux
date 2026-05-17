use crate::hardware::display_layout;
use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::models::{DisplayLayout, Orientation};
use crate::runtime::client;

fn daemon_display_layout_or_transport_fallback(
    response: Result<DaemonResponse, String>,
    fallback: impl FnOnce() -> Result<DisplayLayout, String>,
) -> Result<DisplayLayout, String> {
    match response {
        Ok(DaemonResponse::DisplayLayout { layout }) => {
            Ok(display_layout::normalize_display_layout(layout))
        }
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
        Ok(other) => Err(format!("Unexpected daemon response while {action}: {other:?}")),
        Err(_) => fallback(),
    }
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
        assert!(daemon_display_layout_or_transport_fallback(Ok(DaemonResponse::Pong), || {
            Ok(empty_layout())
        })
        .expect_err("unexpected response should fail")
        .contains("Unexpected daemon response"));
    }

    #[test]
    fn display_layout_mapping_falls_back_on_transport_failure() {
        assert!(daemon_display_layout_or_transport_fallback(Err("socket missing".into()), || {
            Ok(empty_layout())
        })
        .is_ok());
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
        assert!(daemon_ack_or_transport_fallback(Ok(DaemonResponse::Pong), "testing", || Ok(()))
            .expect_err("unexpected response should fail")
            .contains("Unexpected daemon response"));
        assert!(daemon_ack_or_transport_fallback(Err("socket missing".into()), "testing", || {
            Ok(())
        })
        .is_ok());
    }
}
