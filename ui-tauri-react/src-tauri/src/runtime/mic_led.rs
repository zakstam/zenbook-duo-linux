//! The orange LED on the mic-mute key. The session agent reports whether the
//! default microphone is muted; the LED follows it. The keyboard forgets the
//! LED on every reconnect, so the last state is cached and sent again. Like
//! Caps Lock, it goes dark with the keyboard lighting (idle, screens blanked,
//! sleep) and comes back with it.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Duration;

use crate::runtime::logger;

const UNKNOWN: u8 = u8::MAX;
static CACHED: AtomicU8 = AtomicU8::new(UNKNOWN);
/// The keyboard lighting is off: the LED stays dark whatever the state.
static DARK: AtomicBool = AtomicBool::new(false);

/// What the LED shows now: lit for a muted microphone, unless dark.
fn shown(muted: bool) -> bool {
    muted && !DARK.load(Ordering::Relaxed)
}

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
    crate::hardware::hid::set_mic_led(shown(muted))
}

/// The keyboard lighting went off (dark) or came back: the LED follows, and
/// shows the microphone's state again when it comes back.
pub(crate) fn set_dark(dark: bool) {
    if DARK.swap(dark, Ordering::Relaxed) == dark {
        return;
    }
    if let Some(muted) = cached_muted() {
        let _ = crate::hardware::hid::set_mic_led(shown(muted));
    }
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
            result = crate::hardware::hid::set_mic_led(shown(cached_muted().unwrap_or(muted)));
        }
        let _ = logger::append_line(match result {
            Ok(()) => format!("rust-daemon: restored mic LED over {link}"),
            Err(err) => format!("rust-daemon: failed to restore mic LED over {link}: {err}"),
        });
    });
}
