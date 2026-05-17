use crate::hardware::{hid, sysfs};
use crate::ipc::protocol::{DaemonRequest, DaemonResponse};
use crate::runtime::client;

#[tauri::command]
pub fn get_backlight() -> u8 {
    sysfs::read_backlight_level()
}

#[tauri::command]
pub fn set_backlight(level: u8) -> Result<(), String> {
    set_backlight_daemon_first(level)
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

pub fn set_backlight_daemon_first(level: u8) -> Result<(), String> {
    daemon_ack_or_transport_fallback(
        client::request(DaemonRequest::SetBacklight { level }),
        "setting backlight",
        || hid::set_backlight(level),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_ack_mapping_surfaces_error_and_unexpected_response() {
        assert!(daemon_ack_or_transport_fallback(Ok(DaemonResponse::Ack), "testing", || {
            Err("fallback should not run".into())
        })
        .is_ok());

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
    }

    #[test]
    fn daemon_ack_mapping_falls_back_only_on_transport_failure() {
        assert!(daemon_ack_or_transport_fallback(Err("socket missing".into()), "testing", || {
            Ok(())
        })
        .is_ok());
        assert_eq!(
            daemon_ack_or_transport_fallback(Err("socket missing".into()), "testing", || {
                Err("fallback failed".into())
            })
            .expect_err("fallback failure should propagate"),
            "fallback failed"
        );
    }
}
