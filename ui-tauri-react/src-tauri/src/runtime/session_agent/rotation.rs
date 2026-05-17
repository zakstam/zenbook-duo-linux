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

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| format!("Failed reading monitor-sensor output: {e}"))?
    {
        if let Some(sensor_orientation) = sensor_orientation_value(&line) {
            let invert_sensor_rotation =
                crate::commands::settings::load_settings_local().invert_sensor_rotation;
            match display_orientation_from_sensor_value(sensor_orientation, invert_sensor_rotation)
            {
                Some(orientation) => {
                    runtime_log_info(format!(
                        "monitor-sensor orientation changed: sensor={sensor_orientation} mapped_display={orientation:?} invert_sensor_rotation={invert_sensor_rotation}"
                    ));
                    if let Err(err) = crate::hardware::display_layout::set_orientation(&orientation)
                    {
                        runtime_log_warn(format!(
                            "failed to apply accelerometer orientation: sensor={sensor_orientation} mapped_display={orientation:?} invert_sensor_rotation={invert_sensor_rotation}: {err}"
                        ));
                    } else {
                        runtime_log_info(format!(
                            "applied accelerometer orientation: sensor={sensor_orientation} mapped_display={orientation:?} invert_sensor_rotation={invert_sensor_rotation}"
                        ));
                    }
                }
                None => {
                    runtime_log_warn(format!(
                        "monitor-sensor reported unsupported accelerometer orientation: {sensor_orientation}"
                    ));
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

pub(super) fn monitor_sensor_accelerometer_claim_timeout(line: &str) -> bool {
    line.contains("Failed to claim accelerometer") && line.contains("Timeout was reached")
}

#[cfg(test)]
pub(super) fn parse_rotation_line(line: &str) -> Option<Orientation> {
    sensor_orientation_value(line)
        .and_then(|value| display_orientation_from_sensor_value(value, false))
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
