use super::lying_flat;
use super::*;

pub(crate) struct RotationWatcherSupervisor;

impl RotationWatcherSupervisor {
    pub(crate) async fn supervise() {
        supervise_rotation_watcher().await;
    }
}

async fn supervise_rotation_watcher() {
    let mut notified_failure = false;
    let mut consecutive_accelerometer_timeouts = 0;

    loop {
        let restart_delay = match watch_rotation().await {
            Ok(RotationWatchExit::Clean) => {
                consecutive_accelerometer_timeouts = 0;
                runtime_log_warn(format!(
                    "rotation watcher exited cleanly; restarting in {}s",
                    rotation_watcher_restart_delay().as_secs()
                ));
                rotation_watcher_restart_delay()
            }
            Ok(RotationWatchExit::AccelerometerClaimTimeout) => {
                consecutive_accelerometer_timeouts += 1;
                let delay = rotation_watcher_accelerometer_timeout_delay(
                    consecutive_accelerometer_timeouts,
                );
                runtime_log_warn(format!(
                    "monitor-sensor could not claim accelerometer; retrying in {}s",
                    delay.as_secs()
                ));
                delay
            }
            Err(err) => {
                consecutive_accelerometer_timeouts = 0;
                runtime_log_warn(format!(
                    "rotation watcher failed: {err}; restarting in {}s",
                    rotation_watcher_restart_delay().as_secs()
                ));
                if !notified_failure {
                    let _ = send_runtime_notification(
                        "Zenbook Duo Runtime Error",
                        &format!("Rotation watcher failed and will restart: {err}"),
                        true,
                    );
                    notified_failure = true;
                }
                rotation_watcher_restart_delay()
            }
        };

        tokio::time::sleep(restart_delay).await;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RotationWatchExit {
    Clean,
    AccelerometerClaimTimeout,
}

#[derive(Debug, Default)]
struct MonitorSensorStderrSummary {
    accelerometer_claim_timeout: bool,
}

pub(super) fn rotation_watcher_restart_delay() -> Duration {
    Duration::from_secs(2)
}

pub(super) fn rotation_watcher_accelerometer_timeout_delay(consecutive_timeouts: u32) -> Duration {
    let exponent = consecutive_timeouts.saturating_sub(1).min(4);
    Duration::from_secs(30 * 2_u64.pow(exponent))
}

async fn watch_rotation() -> Result<RotationWatchExit, String> {
    let mut child = TokioCommand::new("monitor-sensor")
        .arg("--accel")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start monitor-sensor: {e}"))?;

    runtime_log_info("rotation watcher started monitor-sensor --accel");

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "monitor-sensor stderr unavailable".to_string())?;
    let stderr_task = tokio::spawn(log_monitor_sensor_stderr(stderr));

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "monitor-sensor stdout unavailable".to_string())?;
    let mut lines = BufReader::new(stdout).lines();

    let accelerometer = lying_flat::find_accelerometer();
    if accelerometer.is_none() {
        runtime_log_warn("no accel_3d IIO device found; lying-flat detection is off");
    }
    let mut flat_detector = lying_flat::LyingFlatDetector::default();
    let mut sensor_orientation_now: Option<Orientation> = None;
    let mut flat_poll = tokio::time::interval(lying_flat_poll_interval());
    flat_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) =
                    line.map_err(|e| format!("Failed reading monitor-sensor output: {e}"))?
                else {
                    break;
                };
                let Some(sensor_orientation) = sensor_orientation_value(&line) else {
                    continue;
                };
                remember_sensor_orientation(sensor_orientation);
                // With the keyboard attached the Duo is a laptop: tilting it should
                // not turn the screen.
                if super::brightness_sync::keyboard_attached_from_runtime() {
                    runtime_log_info(format!(
                        "ignoring accelerometer orientation {sensor_orientation}: keyboard attached"
                    ));
                    continue;
                }
                // Table mode switched on by hand (display controls): stay put.
                if !flat_detector.lying_flat() && table_mode_active() {
                    runtime_log_info(format!(
                        "ignoring accelerometer orientation {sensor_orientation}: table mode"
                    ));
                    continue;
                }
                let invert_sensor_rotation =
                    crate::commands::settings::load_settings_local().invert_sensor_rotation;
                match display_orientation_from_sensor_value(sensor_orientation, invert_sensor_rotation)
                {
                    Some(orientation) => {
                        runtime_log_info(format!(
                            "monitor-sensor orientation changed: sensor={sensor_orientation} mapped_display={orientation:?} invert_sensor_rotation={invert_sensor_rotation}"
                        ));
                        sensor_orientation_now = Some(orientation.clone());
                        if flat_detector.lying_flat() {
                            runtime_log_info(format!(
                                "lying flat: keeping the top screen turned; {orientation:?} applies when it is picked up"
                            ));
                            continue;
                        }
                        apply_sensor_orientation(&orientation);
                    }
                    None => {
                        runtime_log_warn(format!(
                            "monitor-sensor reported unsupported accelerometer orientation: {sensor_orientation}"
                        ));
                    }
                }
            }
            _ = flat_poll.tick(), if accelerometer.is_some() => {
                let Some(reading) = accelerometer
                    .as_deref()
                    .and_then(lying_flat::read_accelerometer)
                else {
                    continue;
                };
                // Attached, the Duo is a laptop: lying flat changes nothing either.
                if super::brightness_sync::keyboard_attached_from_runtime() {
                    continue;
                }
                match flat_detector.update(reading) {
                    Some(true) => apply_lying_flat_layout(),
                    Some(false) => {
                        runtime_log_info("picked up from lying flat");
                        apply_sensor_orientation(
                            sensor_orientation_now.as_ref().unwrap_or(&Orientation::Normal),
                        );
                    }
                    None => {}
                }
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed waiting for monitor-sensor: {e}"))?;
    let stderr_summary = match stderr_task.await {
        Ok(summary) => summary,
        Err(err) => {
            runtime_log_warn(format!("monitor-sensor stderr logger task failed: {err}"));
            MonitorSensorStderrSummary::default()
        }
    };

    if status.success() {
        if stderr_summary.accelerometer_claim_timeout {
            Ok(RotationWatchExit::AccelerometerClaimTimeout)
        } else {
            Ok(RotationWatchExit::Clean)
        }
    } else {
        Err(format!("monitor-sensor exited with status {status}"))
    }
}

async fn log_monitor_sensor_stderr(
    stderr: tokio::process::ChildStderr,
) -> MonitorSensorStderrSummary {
    let mut lines = BufReader::new(stderr).lines();
    let mut summary = MonitorSensorStderrSummary::default();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                if monitor_sensor_accelerometer_claim_timeout(line) {
                    summary.accelerometer_claim_timeout = true;
                    continue;
                }

                runtime_log_warn(format!("monitor-sensor stderr: {line}"));
            }
            Ok(None) => break,
            Err(err) => {
                runtime_log_warn(format!("failed reading monitor-sensor stderr: {err}"));
                break;
            }
        }
    }
    summary
}

fn lying_flat_poll_interval() -> Duration {
    Duration::from_secs(1)
}

fn apply_sensor_orientation(orientation: &Orientation) {
    match crate::hardware::display_layout::set_orientation(orientation) {
        Ok(()) => runtime_log_info(format!(
            "applied accelerometer orientation: {orientation:?}"
        )),
        Err(err) => runtime_log_warn(format!(
            "failed to apply accelerometer orientation {orientation:?}: {err}"
        )),
    }
}

/// Lying flat on a table: only the top screen turns around, for the person
/// opposite (the Presentation profile's arrangement).
fn apply_lying_flat_layout() {
    let result = crate::hardware::display_layout::get_display_layout().and_then(|current| {
        crate::hardware::display_layout::apply_display_layout(&lying_flat::lying_flat_layout(
            &current,
        ))
    });
    match result {
        Ok(()) => runtime_log_info("lying flat: turned only the top screen around"),
        Err(err) => runtime_log_warn(format!("failed to apply lying-flat layout: {err}")),
    }
}

pub(super) fn monitor_sensor_accelerometer_claim_timeout(line: &str) -> bool {
    line.contains("Failed to claim accelerometer") && line.contains("Timeout was reached")
}

#[cfg(test)]
pub(super) fn parse_rotation_line(line: &str) -> Option<Orientation> {
    sensor_orientation_value(line)
        .and_then(|value| display_orientation_from_sensor_value(value, false))
}

/// The accelerometer's last reading, kept while the keyboard is attached so
/// detaching can apply it (the sensor only reports changes).
static LAST_SENSOR_ORIENTATION: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn remember_sensor_orientation(value: &str) {
    if let Ok(mut last) = LAST_SENSOR_ORIENTATION.lock() {
        *last = Some(value.to_string());
    }
}

/// Table mode (the GNOME extension's display controls): the top panel alone is
/// upside down for someone across the table. Rotation turns both panels
/// together, so that layout never comes from it, and it stays put meanwhile.
/// Only GNOME can switch it on; elsewhere the same transforms can be an
/// upright UX8407AA (its top panel is mounted 180° rotated), so never there.
fn table_mode_active() -> bool {
    crate::runtime::session::detect_backend_from_env() == crate::ipc::protocol::SessionBackend::Gnome
        && crate::hardware::display_layout::get_display_layout().is_ok_and(|layout| is_table_mode(&layout))
}

pub(super) fn is_table_mode(layout: &crate::models::DisplayLayout) -> bool {
    let transform = |connector: &str| {
        layout
            .displays
            .iter()
            .find(|d| d.connector == connector)
            .map(|d| d.transform)
    };
    transform("eDP-1") == Some(180) && transform("eDP-2") == Some(0)
}

/// After detaching: turn the screens the way the device is held, if not upright.
pub(crate) fn apply_last_orientation() {
    let Some(value) = LAST_SENSOR_ORIENTATION.lock().ok().and_then(|last| last.clone()) else {
        return;
    };
    let invert = crate::commands::settings::load_settings_local().invert_sensor_rotation;
    match display_orientation_from_sensor_value(&value, invert) {
        Some(Orientation::Normal) | None => {}
        Some(orientation) => {
            if let Err(err) = crate::hardware::display_layout::set_orientation(&orientation) {
                runtime_log_warn(format!("failed to apply {orientation:?} after detaching: {err}"));
            }
        }
    }
}

fn sensor_orientation_value(line: &str) -> Option<&str> {
    line.split("Accelerometer orientation changed:")
        .nth(1)
        .map(str::trim)
}

pub(super) fn display_orientation_from_sensor_value(
    value: &str,
    invert_sensor_rotation: bool,
) -> Option<Orientation> {
    match (value, invert_sensor_rotation) {
        ("left-up", false) => Some(Orientation::Left),
        ("left-up", true) => Some(Orientation::Right),
        ("right-up", false) => Some(Orientation::Right),
        ("right-up", true) => Some(Orientation::Left),
        ("bottom-up", _) => Some(Orientation::Inverted),
        ("normal", _) => Some(Orientation::Normal),
        _ => None,
    }
}

fn runtime_log_info(message: impl AsRef<str>) {
    let text = message.as_ref();
    let _ = crate::runtime::logger::append_line(format!("session-agent: {text}"));
    log::info!("{text}");
}

fn runtime_log_warn(message: impl AsRef<str>) {
    let text = message.as_ref();
    let _ = crate::runtime::logger::append_line(format!("session-agent: {text}"));
    log::warn!("{text}");
}
