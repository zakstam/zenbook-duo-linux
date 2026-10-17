//! Follows whether the default microphone is muted (PipeWire, through its
//! PulseAudio interface) and reports it to the daemon, which drives the LED on
//! the mic-mute key. Every way of muting counts: the key, GNOME's quick
//! settings, or an app. A microphone at volume 0 counts as muted too: it picks
//! up nothing either way.

use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::ipc::protocol::DaemonRequest;

/// Also re-sent this often, so a restarted daemon (which forgets the state)
/// gets it back.
const RESEND_INTERVAL: Duration = Duration::from_secs(300);

pub(crate) async fn watch() -> Result<(), String> {
    let mut last_sent: Option<bool> = None;
    loop {
        let mut child = Command::new("pactl")
            .arg("subscribe")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Failed to start pactl subscribe: {e}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "pactl subscribe has no output".to_string())?;
        let mut lines = BufReader::new(stdout).lines();

        sync(&mut last_sent, false).await;
        loop {
            match tokio::time::timeout(RESEND_INTERVAL, lines.next_line()).await {
                Ok(Ok(Some(line))) => {
                    if is_mute_relevant(&line) {
                        sync(&mut last_sent, false).await;
                    }
                }
                // pactl exited (e.g. PipeWire restarted): start it again.
                Ok(Ok(None)) | Ok(Err(_)) => break,
                Err(_) => sync(&mut last_sent, true).await,
            }
        }
        let _ = child.kill().await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Reads the mute state and reports it when it changed (or always, `force`).
async fn sync(last_sent: &mut Option<bool>, force: bool) {
    let Some(muted) = default_source_muted().await else {
        return;
    };
    if !force && *last_sent == Some(muted) {
        return;
    }
    let sent = tokio::task::spawn_blocking(move || {
        crate::runtime::client::request(DaemonRequest::SetMicMute { muted })
    })
    .await;
    match sent {
        Ok(Ok(_)) => *last_sent = Some(muted),
        // Tried again on the next change or resend.
        Ok(Err(err)) => log::warn!("failed to report microphone mute state: {err}"),
        Err(err) => log::warn!("failed to report microphone mute state: {err}"),
    }
}

async fn default_source_muted() -> Option<bool> {
    if parse_mute(&pactl(&["get-source-mute", "@DEFAULT_SOURCE@"]).await?)? {
        return Some(true);
    }
    let volume = pactl(&["get-source-volume", "@DEFAULT_SOURCE@"]).await?;
    Some(volume_is_zero(&volume))
}

async fn pactl(args: &[&str]) -> Option<String> {
    let output = Command::new("pactl").args(args).output().await.ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `pactl get-source-volume` prints each channel as "<raw> / <percent>% / <dB>";
/// zero when every channel is at 0%.
fn volume_is_zero(output: &str) -> bool {
    let percents: Vec<&str> = output
        .split('/')
        .map(str::trim)
        .filter(|part| part.ends_with('%'))
        .collect();
    !percents.is_empty() && percents.iter().all(|p| p.trim_end_matches('%').trim() == "0")
}

/// Mic key + Up/Down: one step of the default microphone's volume, capped at
/// 100%. Raising it also unmutes, like the desktop's volume keys. Then shows
/// the level in an on-screen pop-up (through the Zenbook Duo GNOME Tools extension;
/// skipped when it is not installed).
pub(crate) fn step_volume(up: bool) -> Result<(), String> {
    let step = format!(
        "{}%{}",
        crate::runtime::mic_chord::STEP_PERCENT,
        if up { "+" } else { "-" }
    );
    if up {
        wpctl(&["set-mute", "@DEFAULT_AUDIO_SOURCE@", "0"])?;
    }
    wpctl(&["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SOURCE@", &step])?;
    let (level, muted) = parse_wpctl_volume(&wpctl(&["get-volume", "@DEFAULT_AUDIO_SOURCE@"])?)
        .ok_or_else(|| "Could not read the microphone volume".to_string())?;
    let shown = std::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.gnome.Shell",
            "--object-path",
            "/org/gnome/Shell/Extensions/ZenbookDuoGnomeTools",
            "--method",
            "org.gnome.Shell.Extensions.ZenbookDuoGnomeTools.ShowMicVolume",
            &format!("{level:.2}"),
            if muted { "true" } else { "false" },
        ])
        .output();
    if !shown.is_ok_and(|output| output.status.success()) {
        log::debug!("no mic volume pop-up (Zenbook Duo GNOME Tools extension not running)");
    }
    Ok(())
}

fn wpctl(args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("wpctl")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run wpctl: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "wpctl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `wpctl get-volume` prints "Volume: 0.45", with " [MUTED]" when muted.
fn parse_wpctl_volume(output: &str) -> Option<(f64, bool)> {
    let rest = output.trim().strip_prefix("Volume:")?.trim();
    let level = rest.split_whitespace().next()?.parse().ok()?;
    Some((level, rest.contains("[MUTED]")))
}

/// `pactl get-source-mute` prints "Mute: yes" or "Mute: no".
fn parse_mute(output: &str) -> Option<bool> {
    match output.trim().strip_prefix("Mute:")?.trim() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

/// Events after which the default microphone's mute state may differ: a
/// source changed, or the default source did (a "server" change).
fn is_mute_relevant(event: &str) -> bool {
    event.contains("on source #") || event.contains("on server")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pactl_mute_output() {
        assert_eq!(parse_mute("Mute: yes\n"), Some(true));
        assert_eq!(parse_mute("Mute: no"), Some(false));
        assert_eq!(parse_mute("Stummschaltung: ja"), None);
    }

    #[test]
    fn a_microphone_at_zero_volume_counts_as_muted() {
        let zero = "Volume: front-left: 0 /   0% / -inf dB,   front-right: 0 /   0% / -inf dB\n        balance 0.00\n";
        let quiet = "Volume: front-left: 16384 /  25% / -36.12 dB,   front-right: 16384 /  25% / -36.12 dB\n";
        let one_side = "Volume: front-left: 0 /   0% / -inf dB,   front-right: 655 /   1% / -120.00 dB\n";
        assert!(volume_is_zero(zero));
        assert!(!volume_is_zero(quiet));
        assert!(!volume_is_zero(one_side));
        assert!(!volume_is_zero(""));
    }

    #[test]
    fn parses_wpctl_volume() {
        assert_eq!(parse_wpctl_volume("Volume: 0.45\n"), Some((0.45, false)));
        assert_eq!(parse_wpctl_volume("Volume: 0.25 [MUTED]\n"), Some((0.25, true)));
        assert_eq!(parse_wpctl_volume("nothing"), None);
    }

    #[test]
    fn only_source_and_server_events_matter() {
        assert!(is_mute_relevant("Event 'change' on source #57"));
        assert!(is_mute_relevant("Event 'change' on server #4294967295"));
        assert!(!is_mute_relevant("Event 'change' on sink-input #120"));
        assert!(!is_mute_relevant("Event 'change' on source-output #88"));
        assert!(!is_mute_relevant("Event 'change' on sink #56"));
    }
}
