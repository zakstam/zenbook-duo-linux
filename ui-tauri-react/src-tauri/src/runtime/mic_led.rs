//! The orange LED on the mic-mute key. The session agent reports whether the
//! default microphone is muted; the LED follows it. The keyboard forgets the
//! LED on every reconnect, so the last state is cached and sent again.

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use crate::runtime::logger;

const UNKNOWN: u8 = u8::MAX;
static CACHED: AtomicU8 = AtomicU8::new(UNKNOWN);

/// Right after connecting the keyboard ignores commands for a moment (seen in
/// the Windows driver's traffic), so the state is sent again after each delay.
const RESEND_DELAYS: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(2)];

/// Whether the microphone is muted, once the session agent has reported it.
pub(crate) fn cached_muted() -> Option<bool> {
    match CACHED.load(Ordering::Relaxed) {
        UNKNOWN => None,
        value => Some(value == 1),
    }
}

/// Records the mute state and sets the LED. The state is kept even when the
/// keyboard cannot be reached; it is sent on the next connect.
pub(crate) fn set_muted(muted: bool) -> Result<(), String> {
    CACHED.store(u8::from(muted), Ordering::Relaxed);
    crate::hardware::hid::set_mic_led(muted)
}

/// Sends the cached state again after the keyboard (re)connected.
pub(crate) fn restore_after_connect(link: &'static str) {
    let Some(muted) = cached_muted() else {
        return;
    };
    std::thread::spawn(move || {
        let mut result = Err(String::new());
        for delay in RESEND_DELAYS {
            std::thread::sleep(delay);
            // The state may have changed meanwhile.
            result = crate::hardware::hid::set_mic_led(cached_muted().unwrap_or(muted));
        }
        let _ = logger::append_line(match result {
            Ok(()) => format!("rust-daemon: restored mic LED over {link}"),
            Err(err) => format!("rust-daemon: failed to restore mic LED over {link}: {err}"),
        });
    });
}
