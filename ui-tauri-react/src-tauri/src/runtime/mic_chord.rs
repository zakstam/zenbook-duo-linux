//! Mic volume from the keyboard: hold the mic-mute key and press the volume
//! keys to change the microphone's volume; holding a volume key repeats the
//! step. (Not Up/Down: with the top row in F1-F12 mode the mic key needs Fn,
//! and Fn turns the arrows into Page Up/Down.) A plain press of the mic key
//! still toggles mute, sent when the key is released, and only if no volume
//! step was made meanwhile. Shared by the USB remap helper and the Bluetooth
//! hotkey watcher.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use evdev::Key;

/// Volume step, in percent.
pub(crate) const STEP_PERCENT: u8 = 5;

/// A held volume key repeats its step after this, then every REPEAT_INTERVAL.
/// The volume keys send no key repeats themselves.
const REPEAT_DELAY: Duration = Duration::from_millis(400);
pub(crate) const REPEAT_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MicStep {
    Up,
    Down,
}

/// What to do with a key event while the chord logic watches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChordKey {
    /// Not part of a chord: handle the key as usual.
    Pass,
    /// Part of a chord: swallow the event.
    Swallow,
    /// Swallow the event and change the volume.
    Step(MicStep),
}

#[derive(Debug, Default)]
pub(crate) struct MicChord {
    held: bool,
    stepped: bool,
    /// Keys whose press was swallowed; their repeats and release are too, even
    /// after the mic key is let go.
    swallowed: HashSet<u16>,
    /// The held volume key's step and when it repeats next.
    repeat: Option<(u16, MicStep, Instant)>,
}

impl MicChord {
    pub(crate) fn is_held(&self) -> bool {
        self.held
    }

    /// The mic key went down (1), repeated (2) or up (0). Returns true when
    /// mute should be toggled now: on a release without a volume step.
    pub(crate) fn mic(&mut self, value: i32) -> bool {
        match value {
            1 => {
                self.held = true;
                self.stepped = false;
                false
            }
            0 => {
                let toggle = self.held && !self.stepped;
                self.held = false;
                self.repeat = None;
                toggle
            }
            _ => false,
        }
    }

    /// Any other key event.
    pub(crate) fn key(&mut self, key: Key, value: i32) -> ChordKey {
        let code = key.code();
        if value == 0 {
            if self.repeat.is_some_and(|(held, ..)| held == code) {
                self.repeat = None;
            }
            return if self.swallowed.remove(&code) {
                ChordKey::Swallow
            } else {
                ChordKey::Pass
            };
        }
        if self.swallowed.contains(&code) {
            // Repeats come from `due` instead, so every keyboard behaves alike.
            return ChordKey::Swallow;
        }
        if value != 1 || !self.held {
            return ChordKey::Pass;
        }
        match step_for(key) {
            Some(step) => {
                self.stepped = true;
                self.swallowed.insert(code);
                self.repeat = Some((code, step, Instant::now() + REPEAT_DELAY));
                ChordKey::Step(step)
            }
            None => ChordKey::Pass,
        }
    }
}

impl MicChord {
    /// A repeat step that is due now, for a volume key held with the mic key.
    /// Callers check this at least every REPEAT_INTERVAL while `is_held`.
    pub(crate) fn due(&mut self, now: Instant) -> Option<MicStep> {
        let (code, step, at) = self.repeat?;
        if !self.held || now < at {
            return None;
        }
        self.repeat = Some((code, step, now + REPEAT_INTERVAL));
        Some(step)
    }
}

fn step_for(key: Key) -> Option<MicStep> {
    match key {
        Key::KEY_VOLUMEUP => Some(MicStep::Up),
        Key::KEY_VOLUMEDOWN => Some(MicStep::Down),
        _ => None,
    }
}

/// Asks the daemon (which asks the session agent) to change the microphone
/// volume. In a thread, so key handling never waits for it.
pub(crate) fn request_step(step: MicStep) {
    std::thread::spawn(move || {
        let result = crate::runtime::client::request(
            crate::ipc::protocol::DaemonRequest::StepMicVolume {
                up: step == MicStep::Up,
            },
        );
        if !matches!(result, Ok(crate::ipc::protocol::DaemonResponse::Ack)) {
            log::warn!("mic volume step failed: {result:?}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_press_toggles_mute_on_release() {
        let mut chord = MicChord::default();
        assert!(!chord.mic(1));
        assert!(!chord.mic(2));
        assert!(chord.mic(0));
    }

    #[test]
    fn volume_keys_while_held_change_the_volume_instead_of_muting() {
        let mut chord = MicChord::default();
        chord.mic(1);
        assert_eq!(chord.key(Key::KEY_VOLUMEUP, 1), ChordKey::Step(MicStep::Up));
        assert_eq!(chord.key(Key::KEY_VOLUMEUP, 2), ChordKey::Swallow);
        assert_eq!(chord.key(Key::KEY_VOLUMEUP, 0), ChordKey::Swallow);
        assert_eq!(chord.key(Key::KEY_VOLUMEDOWN, 1), ChordKey::Step(MicStep::Down));
        assert_eq!(chord.key(Key::KEY_VOLUMEDOWN, 0), ChordKey::Swallow);
        assert!(!chord.mic(0), "no mute toggle after a volume step");
    }

    #[test]
    fn arrows_are_not_part_of_the_chord() {
        let mut chord = MicChord::default();
        chord.mic(1);
        assert_eq!(chord.key(Key::KEY_UP, 1), ChordKey::Pass);
        assert_eq!(chord.key(Key::KEY_PAGEDOWN, 1), ChordKey::Pass);
    }

    #[test]
    fn a_held_volume_key_repeats_its_step() {
        let mut chord = MicChord::default();
        chord.mic(1);
        chord.key(Key::KEY_VOLUMEDOWN, 1);
        let start = Instant::now();
        assert_eq!(chord.due(start), None, "not right away");
        let first = start + REPEAT_DELAY + Duration::from_millis(1);
        assert_eq!(chord.due(first), Some(MicStep::Down));
        assert_eq!(chord.due(first), None);
        assert_eq!(chord.due(first + REPEAT_INTERVAL), Some(MicStep::Down));
        chord.key(Key::KEY_VOLUMEDOWN, 0);
        assert_eq!(chord.due(first + REPEAT_INTERVAL * 3), None, "stops on release");
    }

    #[test]
    fn keys_pass_when_the_mic_key_is_not_held() {
        let mut chord = MicChord::default();
        assert_eq!(chord.key(Key::KEY_VOLUMEUP, 1), ChordKey::Pass);
        assert_eq!(chord.key(Key::KEY_VOLUMEUP, 0), ChordKey::Pass);
        chord.mic(1);
        assert_eq!(chord.key(Key::KEY_A, 1), ChordKey::Pass);
        assert_eq!(chord.key(Key::KEY_A, 0), ChordKey::Pass);
    }

    #[test]
    fn a_volume_key_released_after_the_mic_key_stays_swallowed() {
        let mut chord = MicChord::default();
        chord.mic(1);
        chord.key(Key::KEY_VOLUMEDOWN, 1);
        chord.mic(0);
        // It no longer steps, and its release never reaches the desktop.
        assert_eq!(chord.due(Instant::now() + REPEAT_DELAY * 2), None);
        assert_eq!(chord.key(Key::KEY_VOLUMEDOWN, 2), ChordKey::Swallow);
        assert_eq!(chord.key(Key::KEY_VOLUMEDOWN, 0), ChordKey::Swallow);
        assert_eq!(chord.key(Key::KEY_VOLUMEDOWN, 1), ChordKey::Pass);
    }

    #[test]
    fn a_volume_key_held_before_the_mic_key_passes() {
        let mut chord = MicChord::default();
        assert_eq!(chord.key(Key::KEY_VOLUMEUP, 1), ChordKey::Pass);
        chord.mic(1);
        assert_eq!(chord.key(Key::KEY_VOLUMEUP, 2), ChordKey::Pass);
        assert_eq!(chord.key(Key::KEY_VOLUMEUP, 0), ChordKey::Pass);
        assert!(chord.mic(0));
    }
}
