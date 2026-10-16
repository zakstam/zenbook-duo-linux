//! Per-key overrides for the top row: what F1-F12 do in place of their ASUS
//! function (volume, brightness, the app keys and so on). Each key keeps its
//! default, sends its plain F-key, runs a command as the user, sends a key
//! combination, or does nothing. The plain F-key (Fn, or the top row in
//! F1-F12 mode) is never changed.
//!
//! The daemon publishes the settings to a root-owned file; the USB remap
//! helper and the Bluetooth hotkey watcher read it when it changes. Commands
//! only ever run as the desktop user, never as root.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use evdev::Key;
use serde::{Deserialize, Serialize};

use crate::runtime::{logger, paths};

const FILE_NAME: &str = "key_overrides.json";

/// What one top-row key does in place of its ASUS function.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum KeyOverride {
    #[default]
    Default,
    /// The key's own F-key (F5 for the F5 key).
    FKey,
    /// A shell command, run as the desktop user.
    Command { command: String },
    /// A key combination such as "Ctrl+Alt+T".
    Keys { keys: String },
    /// A key GNOME has no use for (one of SHORTCUT_SIGNALS), held as long as
    /// the F-key, to bind to anything in the desktop's keyboard shortcuts.
    Shortcut { signal: String },
    Nothing,
}

/// Overrides by key name, "F1" to "F12" and "Screen" (the lower-screen key
/// right of F12); keys not listed keep their default.
pub type KeyOverrides = BTreeMap<String, KeyOverride>;

fn file_path() -> PathBuf {
    paths::system_runtime_dir().join(FILE_NAME)
}

/// Writes the overrides for the key handlers (daemon only, as root).
pub(crate) fn publish(overrides: &KeyOverrides) {
    let active: KeyOverrides = overrides
        .iter()
        .filter(|(_, o)| **o != KeyOverride::Default)
        .map(|(k, o)| (k.clone(), o.clone()))
        .collect();
    let result = serde_json::to_string_pretty(&active)
        .map_err(|e| e.to_string())
        .and_then(|json| {
            let path = file_path();
            let tmp = path.with_extension("json.tmp");
            fs::write(&tmp, json).map_err(|e| e.to_string())?;
            fs::rename(&tmp, &path).map_err(|e| e.to_string())
        });
    if let Err(err) = result {
        let _ = logger::append_line(format!("rust-daemon: saving key overrides failed: {err}"));
    }
}

/// The published overrides, read again whenever the file changes.
#[derive(Debug, Default)]
pub(crate) struct Loaded {
    modified: Option<SystemTime>,
    overrides: KeyOverrides,
}

impl Loaded {
    fn refresh(&mut self) {
        let path = file_path();
        let modified = fs::metadata(&path).and_then(|m| m.modified()).ok();
        if modified == self.modified {
            return;
        }
        self.modified = modified;
        self.overrides = fs::read_to_string(&path)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
    }

    /// The override for F-key `n` (1-12), or the screen key (SCREEN_KEY).
    pub(crate) fn get(&mut self, n: u8) -> KeyOverride {
        self.refresh();
        let name = if n == SCREEN_KEY { "Screen".to_string() } else { format!("F{n}") };
        self.overrides.get(&name).cloned().unwrap_or_default()
    }
}

const F_KEYS: [Key; 12] = [
    Key::KEY_F1,
    Key::KEY_F2,
    Key::KEY_F3,
    Key::KEY_F4,
    Key::KEY_F5,
    Key::KEY_F6,
    Key::KEY_F7,
    Key::KEY_F8,
    Key::KEY_F9,
    Key::KEY_F10,
    Key::KEY_F11,
    Key::KEY_F12,
];

/// The lower-screen key's number next to F1-F12 (it has no F-key of its own).
pub(crate) const SCREEN_KEY: u8 = 13;

/// 1-12 for F1-F12.
#[cfg(test)]
fn fkey_number(key: Key) -> Option<u8> {
    F_KEYS.iter().position(|k| *k == key).map(|i| i as u8 + 1)
}

pub(crate) fn fkey(n: u8) -> Key {
    F_KEYS[usize::from(n.clamp(1, 12)) - 1]
}

/// Key names accepted in a combination (case-insensitive). Letters and
/// digits are physical keys (their US-layout position).
const NAMED_KEYS: &[(&str, Key)] = &[
    ("ctrl", Key::KEY_LEFTCTRL),
    ("control", Key::KEY_LEFTCTRL),
    ("shift", Key::KEY_LEFTSHIFT),
    ("alt", Key::KEY_LEFTALT),
    ("altgr", Key::KEY_RIGHTALT),
    ("super", Key::KEY_LEFTMETA),
    ("meta", Key::KEY_LEFTMETA),
    ("win", Key::KEY_LEFTMETA),
    ("esc", Key::KEY_ESC),
    ("escape", Key::KEY_ESC),
    ("tab", Key::KEY_TAB),
    ("enter", Key::KEY_ENTER),
    ("return", Key::KEY_ENTER),
    ("space", Key::KEY_SPACE),
    ("backspace", Key::KEY_BACKSPACE),
    ("delete", Key::KEY_DELETE),
    ("del", Key::KEY_DELETE),
    ("insert", Key::KEY_INSERT),
    ("home", Key::KEY_HOME),
    ("end", Key::KEY_END),
    ("pageup", Key::KEY_PAGEUP),
    ("pagedown", Key::KEY_PAGEDOWN),
    ("up", Key::KEY_UP),
    ("down", Key::KEY_DOWN),
    ("left", Key::KEY_LEFT),
    ("right", Key::KEY_RIGHT),
    ("print", Key::KEY_SYSRQ),
    ("printscreen", Key::KEY_SYSRQ),
    ("minus", Key::KEY_MINUS),
    ("equal", Key::KEY_EQUAL),
    ("comma", Key::KEY_COMMA),
    ("period", Key::KEY_DOT),
    ("slash", Key::KEY_SLASH),
    ("mute", Key::KEY_MUTE),
    ("volumedown", Key::KEY_VOLUMEDOWN),
    ("volumeup", Key::KEY_VOLUMEUP),
    ("micmute", Key::KEY_MICMUTE),
    ("playpause", Key::KEY_PLAYPAUSE),
    ("next", Key::KEY_NEXTSONG),
    ("previous", Key::KEY_PREVIOUSSONG),
    ("stop", Key::KEY_STOPCD),
    ("brightnessdown", Key::KEY_BRIGHTNESSDOWN),
    ("brightnessup", Key::KEY_BRIGHTNESSUP),
    ("calculator", Key::KEY_CALC),
    ("search", Key::KEY_SEARCH),
    // XF86Launch1-9, free for the desktop's own keyboard shortcuts. The
    // keyboard layouts give Launch5-9 to F14-F18 (there is no Launch10-12).
    ("launch1", Key::KEY_PROG1),
    ("launch2", Key::KEY_PROG2),
    ("launch3", Key::KEY_PROG3),
    ("launch4", Key::KEY_PROG4),
    ("launch5", Key::KEY_F14),
    ("launch6", Key::KEY_F15),
    ("launch7", Key::KEY_F16),
    ("launch8", Key::KEY_F17),
    ("launch9", Key::KEY_F18),
    ("launcha", Key::KEY_SCALE),
    ("launchb", Key::KEY_DASHBOARD),
];

/// Keys GNOME does not use by default, one per top-row key (the Control
/// Panel suggests them in this order and lets each key pick another one).
/// Launch2 is F8's (window swap); F13 is XF86Tools (opens Settings) and
/// F20-F23 are mic mute and the touchpad keys.
pub const SHORTCUT_SIGNALS: [&str; 12] = [
    "Launch1", "Launch3", "Launch4", "Launch5", "Launch6", "Launch7", "Launch8", "Launch9",
    "LaunchA", "LaunchB", "F19", "F24",
];

const LETTERS: [Key; 26] = [
    Key::KEY_A, Key::KEY_B, Key::KEY_C, Key::KEY_D, Key::KEY_E, Key::KEY_F, Key::KEY_G,
    Key::KEY_H, Key::KEY_I, Key::KEY_J, Key::KEY_K, Key::KEY_L, Key::KEY_M, Key::KEY_N,
    Key::KEY_O, Key::KEY_P, Key::KEY_Q, Key::KEY_R, Key::KEY_S, Key::KEY_T, Key::KEY_U,
    Key::KEY_V, Key::KEY_W, Key::KEY_X, Key::KEY_Y, Key::KEY_Z,
];

const DIGITS: [Key; 10] = [
    Key::KEY_0, Key::KEY_1, Key::KEY_2, Key::KEY_3, Key::KEY_4,
    Key::KEY_5, Key::KEY_6, Key::KEY_7, Key::KEY_8, Key::KEY_9,
];

const HIGH_F_KEYS: [Key; 12] = [
    Key::KEY_F13, Key::KEY_F14, Key::KEY_F15, Key::KEY_F16, Key::KEY_F17, Key::KEY_F18,
    Key::KEY_F19, Key::KEY_F20, Key::KEY_F21, Key::KEY_F22, Key::KEY_F23, Key::KEY_F24,
];

fn key_named(name: &str) -> Option<Key> {
    let name = name.trim().to_lowercase();
    if let Some((_, key)) = NAMED_KEYS.iter().find(|(n, _)| *n == name) {
        return Some(*key);
    }
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_lowercase() {
            return Some(LETTERS[(c as u8 - b'a') as usize]);
        }
        if c.is_ascii_digit() {
            return Some(DIGITS[(c as u8 - b'0') as usize]);
        }
    }
    let n: usize = name.strip_prefix('f')?.parse().ok()?;
    match n {
        1..=12 => Some(F_KEYS[n - 1]),
        13..=24 => Some(HIGH_F_KEYS[n - 13]),
        _ => None,
    }
}

/// The key a shortcut signal is sent as; only SHORTCUT_SIGNALS are allowed.
pub(crate) fn shortcut_key(signal: &str) -> Result<Key, String> {
    if !SHORTCUT_SIGNALS.iter().any(|s| s.eq_ignore_ascii_case(signal)) {
        return Err(format!("\"{signal}\" is not a shortcut key"));
    }
    key_named(signal).ok_or_else(|| format!("unknown key \"{signal}\""))
}

/// Whether an override keeps a key down until the F-key is released (the
/// F-key itself or a shortcut signal), rather than a tap or a command.
pub(crate) fn holds_key(over: &KeyOverride) -> bool {
    matches!(over, KeyOverride::FKey | KeyOverride::Shortcut { .. })
}

/// "Ctrl+Alt+T" → the keys to hold, in order.
pub(crate) fn parse_keys(text: &str) -> Result<Vec<Key>, String> {
    let keys = text
        .split('+')
        .filter(|part| !part.trim().is_empty())
        .map(|part| key_named(part).ok_or_else(|| format!("unknown key \"{}\"", part.trim())))
        .collect::<Result<Vec<_>, _>>()?;
    if keys.is_empty() {
        return Err("no keys given".into());
    }
    Ok(keys)
}

/// Every key a combination can send, for the virtual keyboards' key sets.
pub(crate) fn emittable_keys() -> Vec<Key> {
    NAMED_KEYS
        .iter()
        .map(|(_, key)| *key)
        .chain(LETTERS)
        .chain(DIGITS)
        .chain(F_KEYS)
        .chain(HIGH_F_KEYS)
        .collect()
}

/// Applies an override to a press (1), repeat (2) or release (0) of F-key
/// `n`. Returns false for Default, so the caller does the usual thing.
pub(crate) fn apply(
    over: &KeyOverride,
    n: u8,
    value: i32,
    user: Option<&str>,
    emit: &mut dyn FnMut(Key, i32) -> Result<(), String>,
) -> Result<bool, String> {
    match over {
        KeyOverride::Default => Ok(false),
        KeyOverride::Nothing => Ok(true),
        // The screen key has no F-key: nothing to send.
        KeyOverride::FKey if n == SCREEN_KEY => Ok(true),
        KeyOverride::FKey => emit(fkey(n), value).map(|()| true),
        KeyOverride::Shortcut { signal } => {
            let key = shortcut_key(signal)?;
            emit(key, value).map(|()| true)
        }
        KeyOverride::Command { command } => {
            if value == 1 {
                run_command(user, command)?;
            }
            Ok(true)
        }
        KeyOverride::Keys { keys } => {
            if value == 1 {
                let keys = parse_keys(keys)?;
                for key in &keys {
                    emit(*key, 1)?;
                }
                for key in keys.iter().rev() {
                    emit(*key, 0)?;
                }
            }
            Ok(true)
        }
    }
}

/// Runs `command` with `sh -c` as the desktop user, in their session.
fn run_command(user: Option<&str>, command: &str) -> Result<(), String> {
    let user = user
        .filter(|u| !u.is_empty() && *u != "root")
        .ok_or_else(|| "no desktop user to run the command as".to_string())?;
    let account = nix::unistd::User::from_name(user)
        .ok()
        .flatten()
        .ok_or_else(|| format!("unknown user {user}"))?;
    // As an app of the user's session, not a child of the daemon.
    crate::usb_media_remap_helper::launch_in_user_session(
        user,
        account.uid.as_raw(),
        &["sh", "-c", command],
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_combinations() {
        assert_eq!(
            parse_keys("Ctrl+Alt+T"),
            Ok(vec![Key::KEY_LEFTCTRL, Key::KEY_LEFTALT, Key::KEY_T])
        );
        assert_eq!(parse_keys("super + e"), Ok(vec![Key::KEY_LEFTMETA, Key::KEY_E]));
        assert_eq!(parse_keys("F13"), Ok(vec![Key::KEY_F13]));
        assert_eq!(parse_keys("PlayPause"), Ok(vec![Key::KEY_PLAYPAUSE]));
        assert_eq!(parse_keys("Launch4"), Ok(vec![Key::KEY_PROG4]));
        assert_eq!(parse_keys("Launch9"), Ok(vec![Key::KEY_F18]));
        assert!(parse_keys("Ctrl+Banana").is_err());
        assert!(parse_keys("").is_err());
    }

    #[test]
    fn shortcut_signals_are_keys_gnome_leaves_free() {
        for signal in SHORTCUT_SIGNALS {
            assert!(shortcut_key(signal).is_ok(), "{signal}");
        }
        assert_eq!(shortcut_key("launch5"), Ok(Key::KEY_F14));
        assert_eq!(shortcut_key("LaunchB"), Ok(Key::KEY_DASHBOARD));
        assert!(shortcut_key("Launch2").is_err(), "F8's swap signal");
        assert!(shortcut_key("F13").is_err(), "XF86Tools opens Settings");
        let mut sent = Vec::new();
        let over = KeyOverride::Shortcut { signal: "Launch6".into() };
        let mut emit = |k: Key, v: i32| {
            sent.push((k, v));
            Ok(())
        };
        assert_eq!(apply(&over, 3, 1, None, &mut emit), Ok(true));
        assert_eq!(apply(&over, 3, 0, None, &mut emit), Ok(true));
        assert_eq!(sent, [(Key::KEY_F15, 1), (Key::KEY_F15, 0)]);
    }

    #[test]
    fn f_key_numbers() {
        assert_eq!(fkey_number(Key::KEY_F5), Some(5));
        assert_eq!(fkey_number(Key::KEY_A), None);
        assert_eq!(fkey(12), Key::KEY_F12);
    }

    #[test]
    fn applies_overrides() {
        let mut sent = Vec::new();
        let mut emit = |key: Key, value: i32| {
            sent.push((key, value));
            Ok(())
        };
        assert_eq!(apply(&KeyOverride::Default, 2, 1, None, &mut emit), Ok(false));
        assert_eq!(apply(&KeyOverride::Nothing, 2, 1, None, &mut emit), Ok(true));
        assert_eq!(apply(&KeyOverride::FKey, 2, 1, None, &mut emit), Ok(true));
        assert_eq!(apply(&KeyOverride::FKey, 2, 0, None, &mut emit), Ok(true));
        let combo = KeyOverride::Keys { keys: "Ctrl+T".into() };
        assert_eq!(apply(&combo, 7, 1, None, &mut emit), Ok(true));
        assert_eq!(apply(&combo, 7, 0, None, &mut emit), Ok(true));
        assert_eq!(
            sent,
            [
                (Key::KEY_F2, 1),
                (Key::KEY_F2, 0),
                (Key::KEY_LEFTCTRL, 1),
                (Key::KEY_T, 1),
                (Key::KEY_T, 0),
                (Key::KEY_LEFTCTRL, 0),
            ]
        );
    }

    #[test]
    fn commands_never_run_without_a_user_or_as_root() {
        let command = KeyOverride::Command { command: "true".into() };
        let mut emit = |_: Key, _: i32| Ok(());
        assert!(apply(&command, 1, 1, None, &mut emit).is_err());
        assert!(apply(&command, 1, 1, Some("root"), &mut emit).is_err());
    }

    #[test]
    fn settings_json_shape() {
        let overrides: KeyOverrides = serde_json::from_str(
            r#"{"F2":{"action":"fKey"},"F7":{"action":"command","command":"firefox"},"F12":{"action":"keys","keys":"Super+E"}}"#,
        )
        .unwrap();
        assert_eq!(overrides["F2"], KeyOverride::FKey);
        assert_eq!(overrides["F7"], KeyOverride::Command { command: "firefox".into() });
    }
}
