//! Key names, for keystroke actions.
//!
//! A name is a key's *position* on a US keyboard, not the character it types:
//! the daemon sends Linux key codes through a virtual keyboard, and the
//! desktop applies the user's layout to them. So `z` is the key right of left
//! shift, which types `w` on AZERTY. That is the only honest thing a virtual
//! keyboard can promise, and it is what the configuration UI captures when a
//! key is pressed in the browser (`KeyboardEvent.code` is positional too).
//!
//! The table is closed. The virtual keyboard registers exactly these codes and
//! no others, which is what keeps the keys that do something drastic out of
//! reach: SysRq (which the kernel lets reboot the machine), power, sleep,
//! suspend, wake, rfkill and screen lock have no names here, and a test holds
//! that line. `print` is `KEY_PRINT`, not the `KEY_SYSRQ` a real Print Screen
//! key sends, for the same reason: desktops map both to Print.

/// One nameable key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyName {
    /// The spelling the UI writes: lowercase, words joined with `_`.
    pub name: &'static str,
    /// The Linux input event code (`KEY_*` in input-event-codes.h).
    pub code: u16,
    /// What sort of key it is, for grouping in a picker.
    pub group: &'static str,
}

const fn key(name: &'static str, code: u16, group: &'static str) -> KeyName {
    KeyName { name, code, group }
}

/// Every key a chord may name, in the order a picker shows them.
pub const KEYS: &[KeyName] = &[
    key("ctrl", 29, "modifier"),
    key("shift", 42, "modifier"),
    key("alt", 56, "modifier"),
    key("super", 125, "modifier"),
    key("altgr", 100, "modifier"),
    key("rctrl", 97, "modifier"),
    key("rshift", 54, "modifier"),
    key("rsuper", 126, "modifier"),
    key("a", 30, "letter"),
    key("b", 48, "letter"),
    key("c", 46, "letter"),
    key("d", 32, "letter"),
    key("e", 18, "letter"),
    key("f", 33, "letter"),
    key("g", 34, "letter"),
    key("h", 35, "letter"),
    key("i", 23, "letter"),
    key("j", 36, "letter"),
    key("k", 37, "letter"),
    key("l", 38, "letter"),
    key("m", 50, "letter"),
    key("n", 49, "letter"),
    key("o", 24, "letter"),
    key("p", 25, "letter"),
    key("q", 16, "letter"),
    key("r", 19, "letter"),
    key("s", 31, "letter"),
    key("t", 20, "letter"),
    key("u", 22, "letter"),
    key("v", 47, "letter"),
    key("w", 17, "letter"),
    key("x", 45, "letter"),
    key("y", 21, "letter"),
    key("z", 44, "letter"),
    key("1", 2, "digit"),
    key("2", 3, "digit"),
    key("3", 4, "digit"),
    key("4", 5, "digit"),
    key("5", 6, "digit"),
    key("6", 7, "digit"),
    key("7", 8, "digit"),
    key("8", 9, "digit"),
    key("9", 10, "digit"),
    key("0", 11, "digit"),
    key("f1", 59, "function"),
    key("f2", 60, "function"),
    key("f3", 61, "function"),
    key("f4", 62, "function"),
    key("f5", 63, "function"),
    key("f6", 64, "function"),
    key("f7", 65, "function"),
    key("f8", 66, "function"),
    key("f9", 67, "function"),
    key("f10", 68, "function"),
    key("f11", 87, "function"),
    key("f12", 88, "function"),
    key("f13", 183, "function"),
    key("f14", 184, "function"),
    key("f15", 185, "function"),
    key("f16", 186, "function"),
    key("f17", 187, "function"),
    key("f18", 188, "function"),
    key("f19", 189, "function"),
    key("f20", 190, "function"),
    key("f21", 191, "function"),
    key("f22", 192, "function"),
    key("f23", 193, "function"),
    key("f24", 194, "function"),
    key("kp_0", 82, "keypad"),
    key("kp_1", 79, "keypad"),
    key("kp_2", 80, "keypad"),
    key("kp_3", 81, "keypad"),
    key("kp_4", 75, "keypad"),
    key("kp_5", 76, "keypad"),
    key("kp_6", 77, "keypad"),
    key("kp_7", 71, "keypad"),
    key("kp_8", 72, "keypad"),
    key("kp_9", 73, "keypad"),
    key("kp_add", 78, "keypad"),
    key("kp_subtract", 74, "keypad"),
    key("kp_multiply", 55, "keypad"),
    key("kp_divide", 98, "keypad"),
    key("kp_decimal", 83, "keypad"),
    key("kp_enter", 96, "keypad"),
    key("kp_equal", 117, "keypad"),
    key("num_lock", 69, "keypad"),
    key("enter", 28, "editing"),
    key("escape", 1, "editing"),
    key("tab", 15, "editing"),
    key("space", 57, "editing"),
    key("backspace", 14, "editing"),
    key("delete", 111, "editing"),
    key("insert", 110, "editing"),
    key("home", 102, "navigation"),
    key("end", 107, "navigation"),
    key("page_up", 104, "navigation"),
    key("page_down", 109, "navigation"),
    key("up", 103, "navigation"),
    key("down", 108, "navigation"),
    key("left", 105, "navigation"),
    key("right", 106, "navigation"),
    key("print", 210, "system"),
    key("pause", 119, "system"),
    key("scroll_lock", 70, "system"),
    key("caps_lock", 58, "system"),
    key("menu", 127, "system"),
    key("volume_up", 115, "media"),
    key("volume_down", 114, "media"),
    key("mute", 113, "media"),
    key("mic_mute", 248, "media"),
    key("play_pause", 164, "media"),
    key("next_song", 163, "media"),
    key("previous_song", 165, "media"),
    key("stop_cd", 166, "media"),
    key("brightness_up", 225, "media"),
    key("brightness_down", 224, "media"),
    key("calculator", 140, "media"),
    key("minus", 12, "punctuation"),
    key("equal", 13, "punctuation"),
    key("comma", 51, "punctuation"),
    key("period", 52, "punctuation"),
    key("slash", 53, "punctuation"),
    key("semicolon", 39, "punctuation"),
    key("apostrophe", 40, "punctuation"),
    key("grave", 41, "punctuation"),
    key("bracket_left", 26, "punctuation"),
    key("bracket_right", 27, "punctuation"),
    key("backslash", 43, "punctuation"),
    key("less", 86, "punctuation"),
];

/// Other spellings people reach for: X keysym names, browser `code` names,
/// and the single characters themselves.
///
/// `next` and `prior` are left out on purpose. They are X's names for page
/// down and page up, and anyone writing them on a deck means a track.
const ALIASES: &[(&str, &str)] = &[
    ("control", "ctrl"),
    ("lctrl", "ctrl"),
    ("controlleft", "ctrl"),
    ("controlright", "rctrl"),
    ("lshift", "shift"),
    ("shiftleft", "shift"),
    ("shiftright", "rshift"),
    ("lalt", "alt"),
    ("altleft", "alt"),
    ("ralt", "altgr"),
    ("altright", "altgr"),
    ("meta", "super"),
    ("win", "super"),
    ("logo", "super"),
    ("cmd", "super"),
    ("metaleft", "super"),
    ("metaright", "rsuper"),
    ("return", "enter"),
    ("esc", "escape"),
    ("del", "delete"),
    ("ins", "insert"),
    ("pgup", "page_up"),
    ("pgdn", "page_down"),
    ("pageup", "page_up"),
    ("pagedown", "page_down"),
    ("arrowup", "up"),
    ("arrowdown", "down"),
    ("arrowleft", "left"),
    ("arrowright", "right"),
    ("printscreen", "print"),
    ("prtsc", "print"),
    ("sysrq", "print"),
    ("apps", "menu"),
    ("contextmenu", "menu"),
    ("numlock", "num_lock"),
    ("kpplus", "kp_add"),
    ("kpminus", "kp_subtract"),
    ("kpasterisk", "kp_multiply"),
    ("kpslash", "kp_divide"),
    ("kpdot", "kp_decimal"),
    ("kpperiod", "kp_decimal"),
    ("numpadadd", "kp_add"),
    ("numpadsubtract", "kp_subtract"),
    ("numpadmultiply", "kp_multiply"),
    ("numpaddivide", "kp_divide"),
    ("numpaddecimal", "kp_decimal"),
    ("numpadenter", "kp_enter"),
    ("numpadequal", "kp_equal"),
    ("audioraisevolume", "volume_up"),
    ("audiolowervolume", "volume_down"),
    ("audiomute", "mute"),
    ("audiomicmute", "mic_mute"),
    ("audioplay", "play_pause"),
    ("mediaplaypause", "play_pause"),
    ("audionext", "next_song"),
    ("mediatracknext", "next_song"),
    ("nexttrack", "next_song"),
    ("audioprev", "previous_song"),
    ("mediatrackprevious", "previous_song"),
    ("prevtrack", "previous_song"),
    ("audiostop", "stop_cd"),
    ("mediastop", "stop_cd"),
    ("monbrightnessup", "brightness_up"),
    ("monbrightnessdown", "brightness_down"),
    ("calc", "calculator"),
    ("equals", "equal"),
    ("dot", "period"),
    ("quote", "apostrophe"),
    ("backtick", "grave"),
    ("backquote", "grave"),
    ("leftbracket", "bracket_left"),
    ("lbracket", "bracket_left"),
    ("rightbracket", "bracket_right"),
    ("rbracket", "bracket_right"),
    ("intlbackslash", "less"),
    ("102nd", "less"),
    ("-", "minus"),
    ("=", "equal"),
    (",", "comma"),
    (".", "period"),
    ("/", "slash"),
    (";", "semicolon"),
    ("'", "apostrophe"),
    ("`", "grave"),
    ("[", "bracket_left"),
    ("]", "bracket_right"),
    ("\\", "backslash"),
];

/// Codes that must never be reachable: SysRq, power, sleep, wake, suspend,
/// rfkill, screen lock. The virtual keyboard refuses to register them too.
pub const FORBIDDEN: &[u16] = &[99, 116, 142, 143, 205, 247, 152];

/// Longest chord worth writing. Five is ctrl+alt+shift+super+a; beyond that
/// it is not a shortcut anyone can remember.
pub const MAX_CHORD: usize = 5;

/// Lowercase with `_` and `-` between words dropped, so `Page_Up`, `pageup`
/// and `PAGE-UP` meet. A lone `-` is left alone: it is the minus key.
fn normalise(name: &str) -> String {
    if name.chars().count() == 1 {
        return name.to_lowercase();
    }
    name.chars()
        .filter(|c| *c != '_' && *c != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

/// The key a name means, if it means one.
pub fn lookup(name: &str) -> Option<&'static KeyName> {
    let wanted = normalise(name);
    // `KeyA`, `Digit7`, `Numpad7` are what a browser reports; accept them so
    // a pasted capture works.
    let wanted = wanted
        .strip_prefix("key")
        .filter(|rest| rest.len() == 1)
        .map(str::to_string)
        .or_else(|| {
            wanted
                .strip_prefix("digit")
                .filter(|rest| rest.len() == 1)
                .map(str::to_string)
        })
        .or_else(|| {
            wanted
                .strip_prefix("numpad")
                .filter(|rest| rest.len() == 1)
                .map(|rest| format!("kp{rest}"))
        })
        .unwrap_or(wanted);
    let canonical = ALIASES
        .iter()
        .find(|(alias, _)| normalise(alias) == wanted)
        .map_or(wanted.as_str(), |(_, to)| to);
    let canonical = normalise(canonical);
    KEYS.iter().find(|k| normalise(k.name) == canonical)
}

/// Whether a code is a modifier.
pub fn is_modifier(code: u16) -> bool {
    matches!(code, 29 | 42 | 56 | 125 | 100 | 97 | 54 | 126)
}

/// Whether a code is a keypad digit or decimal point, which type digits only
/// while Num Lock is on.
pub fn needs_num_lock(code: u16) -> bool {
    matches!(code, 71..=73 | 75..=77 | 79..=83)
}

/// Why a chord was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChordError {
    Empty,
    /// Spaces are reserved for sequences of chords, which are not supported.
    Sequence,
    TooLong,
    /// A name that is not a key, with the nearest one that is.
    Unknown {
        name: String,
        nearest: Option<&'static str>,
    },
}

impl std::fmt::Display for ChordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChordError::Empty => write!(f, "an empty chord presses nothing"),
            ChordError::Sequence => write!(
                f,
                "a chord is keys joined with +; sequences of chords are not supported"
            ),
            ChordError::TooLong => write!(f, "more than {MAX_CHORD} keys in one chord"),
            ChordError::Unknown { name, nearest } => match nearest {
                Some(nearest) => write!(f, "{name:?} is not a key name; did you mean {nearest:?}?"),
                None => write!(f, "{name:?} is not a key name"),
            },
        }
    }
}

/// A chord's key codes, in press order, duplicates removed.
///
/// `+` joins names; a trailing `+` means the plus key itself is not
/// nameable, which is deliberate — `equal` with `shift` is the key.
pub fn parse_chord(chord: &str) -> Result<Vec<u16>, ChordError> {
    let chord = chord.trim();
    if chord.is_empty() {
        return Err(ChordError::Empty);
    }
    if chord.contains(char::is_whitespace) {
        return Err(ChordError::Sequence);
    }
    let mut codes = Vec::new();
    for name in chord.split('+') {
        if name.is_empty() {
            return Err(ChordError::Unknown {
                name: "+".into(),
                nearest: Some("equal"),
            });
        }
        let Some(key) = lookup(name) else {
            return Err(ChordError::Unknown {
                name: name.to_string(),
                nearest: nearest(name),
            });
        };
        if !codes.contains(&key.code) {
            codes.push(key.code);
        }
    }
    if codes.len() > MAX_CHORD {
        return Err(ChordError::TooLong);
    }
    Ok(codes)
}

/// Every code the virtual keyboard needs to register.
pub fn all_codes() -> Vec<u16> {
    let mut codes: Vec<u16> = KEYS.iter().map(|k| k.code).collect();
    codes.sort_unstable();
    codes.dedup();
    codes
}

fn nearest(name: &str) -> Option<&'static str> {
    let wanted = normalise(name);
    KEYS.iter()
        .map(|k| (crate::edit_distance(&wanted, &normalise(k.name)), k.name))
        .filter(|(distance, _)| *distance <= 2)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_drastic_is_reachable() {
        let codes = all_codes();
        for forbidden in FORBIDDEN {
            assert!(
                !codes.contains(forbidden),
                "code {forbidden} must not be nameable"
            );
        }
        // Every alias lands on a real key.
        for (alias, to) in ALIASES {
            assert!(lookup(alias).is_some(), "alias {alias} -> {to}");
        }
    }

    #[test]
    fn names_are_forgiving_about_spelling() {
        for (spelling, code) in [
            ("Page_Up", 104),
            ("pageup", 104),
            ("PgUp", 104),
            ("KP_7", 71),
            ("kp7", 71),
            ("Numpad7", 71),
            ("KeyZ", 44),
            ("Digit1", 2),
            ("Return", 28),
            ("-", 12),
            ("[", 26),
            ("print", 210),
            ("sysrq", 210),
        ] {
            assert_eq!(lookup(spelling).map(|k| k.code), Some(code), "{spelling}");
        }
    }

    #[test]
    fn a_chord_is_its_keys_in_order() {
        assert_eq!(parse_chord("ctrl+shift+t"), Ok(vec![29, 42, 20]));
        assert_eq!(parse_chord("Ctrl+Page_Down"), Ok(vec![29, 109]));
        assert_eq!(parse_chord("ctrl+control+c"), Ok(vec![29, 46]));
    }

    #[test]
    fn a_bad_chord_says_why() {
        assert_eq!(parse_chord(""), Err(ChordError::Empty));
        assert_eq!(parse_chord("ctrl+k ctrl+c"), Err(ChordError::Sequence));
        assert!(matches!(
            parse_chord("ctrl+pageupp"),
            Err(ChordError::Unknown {
                nearest: Some("page_up"),
                ..
            })
        ));
        assert!(matches!(
            parse_chord("ctrl++"),
            Err(ChordError::Unknown { .. })
        ));
        assert_eq!(
            parse_chord("ctrl+alt+shift+super+altgr+a"),
            Err(ChordError::TooLong)
        );
        // X's names for the page keys are not accepted: on a deck, "next"
        // means a track.
        assert!(parse_chord("next").is_err());
    }

    #[test]
    fn the_keyboard_looks_like_a_keyboard() {
        // udev only calls a device a keyboard if it has every key from 1 to
        // 31; without that the desktop may ignore it.
        let codes = all_codes();
        for code in 1..=31 {
            assert!(codes.contains(&code), "key code {code} is missing");
        }
    }
}
