//! Keyboard backlight sleep: after a while without input the session agent
//! reports idle and the backlight goes off; the next input turns it back to
//! its level. The level itself is left as it is.

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether the backlight is off because of idle (and should come back).
static DIMMED: AtomicBool = AtomicBool::new(false);

/// Idle: backlight off (when it is on). Active again: back to `level`.
pub(crate) fn set_idle(idle: bool, level: u8) -> Result<(), String> {
    if idle {
        if level == 0 || DIMMED.load(Ordering::Relaxed) {
            return Ok(());
        }
        crate::hardware::hid::set_backlight(0)?;
        DIMMED.store(true, Ordering::Relaxed);
    } else if DIMMED.swap(false, Ordering::Relaxed) {
        set_with_retry(level)?;
    }
    Ok(())
}

/// Sets the backlight, trying again for a few seconds: on wake the detached
/// keyboard may still be reconnecting and rejects commands meanwhile.
pub(crate) fn set_with_retry(level: u8) -> Result<(), String> {
    let mut result = crate::hardware::hid::set_backlight(level);
    for _ in 0..3 {
        if result.is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        result = crate::hardware::hid::set_backlight(level);
    }
    result
}

/// A level was set some other way (key, settings app): nothing to restore.
pub(crate) fn level_set() {
    DIMMED.store(false, Ordering::Relaxed);
}
