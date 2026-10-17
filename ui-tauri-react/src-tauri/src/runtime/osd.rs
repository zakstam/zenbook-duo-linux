//! On-screen pop-ups like GNOME's volume and brightness ones, shown by the
//! Zenbook Duo GNOME Tools extension. Callers fall back to a notification when the
//! extension is not running (another desktop, or not installed).

use std::process::Command;

/// One pop-up: an icon name, a label, and optionally a level bar (0 to 1).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Osd {
    pub icon: &'static str,
    pub label: String,
    pub level: Option<f64>,
}

impl Osd {
    pub(crate) fn new(icon: &'static str, label: impl Into<String>) -> Self {
        Self {
            icon,
            label: label.into(),
            level: None,
        }
    }

    pub(crate) fn with_level(mut self, level: f64) -> Self {
        self.level = Some(level.clamp(0.0, 1.0));
        self
    }
}

/// The `gdbus call` arguments for the extension's ShowOsd method; a negative
/// level means no level bar.
pub(crate) fn gdbus_args(osd: &Osd) -> Vec<String> {
    let mut args: Vec<String> = [
        "call",
        "--session",
        "--dest",
        "org.gnome.Shell",
        "--object-path",
        "/org/gnome/Shell/Extensions/ZenbookDuoGnomeTools",
        "--method",
        "org.gnome.Shell.Extensions.ZenbookDuoGnomeTools.ShowOsd",
        // Ends gdbus' options, so a negative level is not taken for one.
        "--",
    ]
    .map(String::from)
    .to_vec();
    args.push(osd.icon.into());
    args.push(osd.label.clone());
    args.push(format!("{:.3}", osd.level.unwrap_or(-1.0)));
    args
}

/// Shows the pop-up from inside the desktop session (the session agent).
/// Returns whether the extension showed it.
pub(crate) fn show_in_session(osd: &Osd) -> bool {
    run(Command::new("gdbus").args(gdbus_args(osd)))
}

/// A warning bubble that stays until OK is pressed (a newer one with the same
/// id replaces it), from inside the desktop session. Returns whether the
/// extension showed it.
pub(crate) fn show_alert_in_session(id: &str, icon: &str, title: &str, message: &str) -> bool {
    run(Command::new("gdbus").args([
        "call",
        "--session",
        "--dest",
        "org.gnome.Shell",
        "--object-path",
        "/org/gnome/Shell/Extensions/ZenbookDuoGnomeTools",
        "--method",
        "org.gnome.Shell.Extensions.ZenbookDuoGnomeTools.ShowAlert",
        id,
        icon,
        title,
        message,
    ]))
}

/// Shows the pop-up in `user`'s desktop session, from a root process (the
/// daemon or the USB remap helper). Returns whether the extension showed it.
pub(crate) fn show_for_user(user: &str, osd: &Osd) -> bool {
    let Ok(Some(account)) = nix::unistd::User::from_name(user) else {
        return false;
    };
    let mut command =
        crate::usb_media_remap_helper::user_session_command(user, account.uid.as_raw());
    run(command.arg("gdbus").args(gdbus_args(osd)))
}

fn run(command: &mut Command) -> bool {
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_icon_label_and_level() {
        let args = gdbus_args(&Osd::new("keyboard-brightness-symbolic", "Keyboard backlight").with_level(2.0 / 3.0));
        assert_eq!(
            &args[8..],
            ["--", "keyboard-brightness-symbolic", "Keyboard backlight", "0.667"]
        );
    }

    #[test]
    fn no_level_bar_is_a_negative_level() {
        let args = gdbus_args(&Osd::new("input-keyboard-symbolic", "Top row: F1–F12"));
        assert_eq!(args.last().map(String::as_str), Some("-1.000"));
    }
}
