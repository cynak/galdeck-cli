//! What a key or a knob does.
//!
//! Every gesture slot — a key's `exec`, `hold` and `double`, a knob's `press`,
//! `cw`, `ccw` and `hold` — takes the same thing, so anything that can be done
//! at all can be done from any of them:
//!
//! ```toml
//! exec = "firefox"                          # a shell command, as it always was
//! exec = { action = "play_pause" }          # something built in
//! cw = { action = "volume_up", step = 5 }   # ...with a step
//! exec = { keys = "ctrl+shift+t" }          # a keystroke
//! ```
//!
//! A bare string stays a shell command, so every config written before this
//! existed means what it meant.
//!
//! Built-ins are a closed list rather than a way to name arbitrary D-Bus calls
//! or commands: each is something the daemon can do well, with feedback on the
//! ring, and without a shell.

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::Deserialize;

/// Something the daemon knows how to do itself.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BuiltIn {
    VolumeUp,
    VolumeDown,
    VolumeMute,
    MicUp,
    MicDown,
    MicMute,
    /// The microphone is live only while the key is held. Keys only.
    PushToTalk,
    NextOutput,
    PreviousOutput,
    /// Makes the output its `target` names the default. Needs a target.
    SetOutput,
    AppVolumeUp,
    AppVolumeDown,
    AppMute,
    /// Moves a knob on to the next app playing sound. Knobs only.
    NextApp,
    PlayPause,
    NextTrack,
    PreviousTrack,
    SeekForward,
    SeekBackward,
    NextPage,
    PreviousPage,
    HomePage,
    NextProfile,
    PreviousProfile,
    StartProfile,
    DeckBrighter,
    DeckDimmer,
    /// Switches the knob it is bound to on to its next mode. Knobs only.
    NextMode,
    /// Starts, pauses and resumes the key's own timer. Keys only.
    TimerToggle,
    TimerReset,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    /// Moves the key it is bound to on to its next state. Keys only.
    NextState,
    /// Moves the key it is bound to back to its previous state. Keys only.
    PreviousState,
}

/// What a step counts, for a built-in that takes one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepUnit {
    Percent,
    Seconds,
    Notches,
}

/// Which gestures a built-in can be bound to.
///
/// Most work anywhere. The rest act on the thing they are bound to -- the
/// key's own timer or states, the knob's own modes -- or need to see the key
/// come back up, and bound anywhere else they would have nothing to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotRule {
    Anywhere,
    /// Only what tapping a key does: push-to-talk.
    KeyTapOnly,
    /// Only a key's gestures: the timer and state built-ins.
    KeyOnly,
    /// Only a knob's gestures: `next_mode` and `next_app`.
    KnobOnly,
}

/// The part of its own key a key-only built-in acts on, which that key
/// has to have for it to do anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyPart {
    /// A `timer` or `stopwatch` widget.
    Timer,
    /// `states` to step through.
    States,
}

/// What a built-in's `target` names.
///
/// One word, `target`, means four different things, and a name meant for one
/// sent to another is refused at best: wpctl reads a player's name as a
/// node. So a knob's target reaches only the built-ins of its preset's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetKind {
    /// `mic`, or a PipeWire node id or name.
    AudioNode,
    /// An MPRIS player, such as `spotify`.
    Player,
    /// An app playing sound, by its program or its name.
    App,
    /// A sound output, by part of its name.
    Sink,
}

/// Longest `target` of any kind, in characters.
pub const MAX_TARGET: usize = 128;

impl TargetKind {
    /// The name the protocol uses.
    pub fn name(self) -> &'static str {
        match self {
            TargetKind::AudioNode => "audio_node",
            TargetKind::Player => "player",
            TargetKind::App => "app",
            TargetKind::Sink => "sink",
        }
    }

    /// What one is called, for messages.
    pub fn noun(self) -> &'static str {
        match self {
            TargetKind::AudioNode => "sound device",
            TargetKind::Player => "player",
            TargetKind::App => "app",
            TargetKind::Sink => "sound output",
        }
    }

    /// What a good one looks like, for messages.
    pub fn hint(self) -> &'static str {
        match self {
            TargetKind::AudioNode => {
                "\"mic\", or a node id such as 42 from `wpctl status`: up to 128 letters, digits and . _ : -, not starting with -"
            }
            TargetKind::Player => {
                "a player such as \"spotify\", in up to 128 characters and no control characters"
            }
            TargetKind::App => {
                "an app's program or name, such as \"firefox\", in up to 128 characters and no control characters"
            }
            TargetKind::Sink => {
                "part of an output's name, such as \"Headphones\", in up to 128 characters and no control characters"
            }
        }
    }

    /// Whether `target` has the shape this kind needs.
    ///
    /// Only the shape: whether the device, player or app exists is for the
    /// daemon to find out on the machine. A sound device is held to what the
    /// daemon will pass to a sound tool, so nothing it would refuse loads
    /// quietly here.
    pub fn accepts(self, target: &str) -> bool {
        if !(1..=MAX_TARGET).contains(&target.chars().count()) {
            return false;
        }
        match self {
            TargetKind::AudioNode => {
                !target.starts_with('-')
                    && target
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
            }
            TargetKind::Player | TargetKind::App | TargetKind::Sink => {
                !target.chars().any(char::is_control)
            }
        }
    }
}

impl BuiltIn {
    pub const ALL: &'static [BuiltIn] = &[
        BuiltIn::VolumeUp,
        BuiltIn::VolumeDown,
        BuiltIn::VolumeMute,
        BuiltIn::MicUp,
        BuiltIn::MicDown,
        BuiltIn::MicMute,
        BuiltIn::PushToTalk,
        BuiltIn::NextOutput,
        BuiltIn::PreviousOutput,
        BuiltIn::SetOutput,
        BuiltIn::AppVolumeUp,
        BuiltIn::AppVolumeDown,
        BuiltIn::AppMute,
        BuiltIn::NextApp,
        BuiltIn::PlayPause,
        BuiltIn::NextTrack,
        BuiltIn::PreviousTrack,
        BuiltIn::SeekForward,
        BuiltIn::SeekBackward,
        BuiltIn::NextPage,
        BuiltIn::PreviousPage,
        BuiltIn::HomePage,
        BuiltIn::NextProfile,
        BuiltIn::PreviousProfile,
        BuiltIn::StartProfile,
        BuiltIn::DeckBrighter,
        BuiltIn::DeckDimmer,
        BuiltIn::NextMode,
        BuiltIn::TimerToggle,
        BuiltIn::TimerReset,
        BuiltIn::ScrollUp,
        BuiltIn::ScrollDown,
        BuiltIn::ScrollLeft,
        BuiltIn::ScrollRight,
        BuiltIn::ZoomIn,
        BuiltIn::ZoomOut,
        BuiltIn::ZoomReset,
        BuiltIn::NextState,
        BuiltIn::PreviousState,
    ];

    /// Where it is in [`BuiltIn::ALL`].
    ///
    /// Written out rather than searched for, so a new built-in does not
    /// compile until it has a place, and a test then holds `ALL` to it.
    pub fn index(self) -> usize {
        match self {
            BuiltIn::VolumeUp => 0,
            BuiltIn::VolumeDown => 1,
            BuiltIn::VolumeMute => 2,
            BuiltIn::MicUp => 3,
            BuiltIn::MicDown => 4,
            BuiltIn::MicMute => 5,
            BuiltIn::PushToTalk => 6,
            BuiltIn::NextOutput => 7,
            BuiltIn::PreviousOutput => 8,
            BuiltIn::SetOutput => 9,
            BuiltIn::AppVolumeUp => 10,
            BuiltIn::AppVolumeDown => 11,
            BuiltIn::AppMute => 12,
            BuiltIn::NextApp => 13,
            BuiltIn::PlayPause => 14,
            BuiltIn::NextTrack => 15,
            BuiltIn::PreviousTrack => 16,
            BuiltIn::SeekForward => 17,
            BuiltIn::SeekBackward => 18,
            BuiltIn::NextPage => 19,
            BuiltIn::PreviousPage => 20,
            BuiltIn::HomePage => 21,
            BuiltIn::NextProfile => 22,
            BuiltIn::PreviousProfile => 23,
            BuiltIn::StartProfile => 24,
            BuiltIn::DeckBrighter => 25,
            BuiltIn::DeckDimmer => 26,
            BuiltIn::NextMode => 27,
            BuiltIn::TimerToggle => 28,
            BuiltIn::TimerReset => 29,
            BuiltIn::ScrollUp => 30,
            BuiltIn::ScrollDown => 31,
            BuiltIn::ScrollLeft => 32,
            BuiltIn::ScrollRight => 33,
            BuiltIn::ZoomIn => 34,
            BuiltIn::ZoomOut => 35,
            BuiltIn::ZoomReset => 36,
            BuiltIn::NextState => 37,
            BuiltIn::PreviousState => 38,
        }
    }

    /// The name used in config.
    pub fn name(self) -> &'static str {
        match self {
            BuiltIn::VolumeUp => "volume_up",
            BuiltIn::VolumeDown => "volume_down",
            BuiltIn::VolumeMute => "volume_mute",
            BuiltIn::MicUp => "mic_up",
            BuiltIn::MicDown => "mic_down",
            BuiltIn::MicMute => "mic_mute",
            BuiltIn::PushToTalk => "push_to_talk",
            BuiltIn::NextOutput => "next_output",
            BuiltIn::PreviousOutput => "previous_output",
            BuiltIn::SetOutput => "set_output",
            BuiltIn::AppVolumeUp => "app_volume_up",
            BuiltIn::AppVolumeDown => "app_volume_down",
            BuiltIn::AppMute => "app_mute",
            BuiltIn::NextApp => "next_app",
            BuiltIn::PlayPause => "play_pause",
            BuiltIn::NextTrack => "next_track",
            BuiltIn::PreviousTrack => "previous_track",
            BuiltIn::SeekForward => "seek_forward",
            BuiltIn::SeekBackward => "seek_backward",
            BuiltIn::NextPage => "next_page",
            BuiltIn::PreviousPage => "previous_page",
            BuiltIn::HomePage => "home_page",
            BuiltIn::NextProfile => "next_profile",
            BuiltIn::PreviousProfile => "previous_profile",
            BuiltIn::StartProfile => "start_profile",
            BuiltIn::DeckBrighter => "deck_brighter",
            BuiltIn::DeckDimmer => "deck_dimmer",
            BuiltIn::NextMode => "next_mode",
            BuiltIn::TimerToggle => "timer_toggle",
            BuiltIn::TimerReset => "timer_reset",
            BuiltIn::ScrollUp => "scroll_up",
            BuiltIn::ScrollDown => "scroll_down",
            BuiltIn::ScrollLeft => "scroll_left",
            BuiltIn::ScrollRight => "scroll_right",
            BuiltIn::ZoomIn => "zoom_in",
            BuiltIn::ZoomOut => "zoom_out",
            BuiltIn::ZoomReset => "zoom_reset",
            BuiltIn::NextState => "next_state",
            BuiltIn::PreviousState => "previous_state",
        }
    }

    /// Words for a person: what the UI and the on-screen display say.
    pub fn describe(self) -> &'static str {
        match self {
            BuiltIn::VolumeUp => "volume up",
            BuiltIn::VolumeDown => "volume down",
            BuiltIn::VolumeMute => "mute or unmute the output",
            BuiltIn::MicUp => "microphone up",
            BuiltIn::MicDown => "microphone down",
            BuiltIn::MicMute => "mute or unmute the microphone",
            BuiltIn::PushToTalk => "microphone live while held",
            BuiltIn::NextOutput => "next sound output",
            BuiltIn::PreviousOutput => "previous sound output",
            BuiltIn::SetOutput => "switch to a sound output",
            BuiltIn::AppVolumeUp => "an app's volume up",
            BuiltIn::AppVolumeDown => "an app's volume down",
            BuiltIn::AppMute => "mute or unmute an app",
            BuiltIn::NextApp => "the next app playing sound",
            BuiltIn::PlayPause => "play or pause",
            BuiltIn::NextTrack => "next track",
            BuiltIn::PreviousTrack => "previous track",
            BuiltIn::SeekForward => "skip forward",
            BuiltIn::SeekBackward => "skip back",
            BuiltIn::NextPage => "next page",
            BuiltIn::PreviousPage => "previous page",
            BuiltIn::HomePage => "the profile's home page",
            BuiltIn::NextProfile => "next profile",
            BuiltIn::PreviousProfile => "previous profile",
            BuiltIn::StartProfile => "the start profile",
            BuiltIn::DeckBrighter => "deck brighter",
            BuiltIn::DeckDimmer => "deck dimmer",
            BuiltIn::NextMode => "the knob's next mode",
            BuiltIn::TimerToggle => "start, pause or resume the timer",
            BuiltIn::TimerReset => "reset the timer",
            BuiltIn::ScrollUp => "scroll up",
            BuiltIn::ScrollDown => "scroll down",
            BuiltIn::ScrollLeft => "scroll left",
            BuiltIn::ScrollRight => "scroll right",
            BuiltIn::ZoomIn => "zoom in",
            BuiltIn::ZoomOut => "zoom out",
            BuiltIn::ZoomReset => "reset zoom",
            BuiltIn::NextState => "the key's next state",
            BuiltIn::PreviousState => "the key's previous state",
        }
    }

    /// A heading to group it under.
    pub fn group(self) -> &'static str {
        match self {
            BuiltIn::VolumeUp
            | BuiltIn::VolumeDown
            | BuiltIn::VolumeMute
            | BuiltIn::MicUp
            | BuiltIn::MicDown
            | BuiltIn::MicMute
            | BuiltIn::PushToTalk
            | BuiltIn::NextOutput
            | BuiltIn::PreviousOutput
            | BuiltIn::SetOutput
            | BuiltIn::AppVolumeUp
            | BuiltIn::AppVolumeDown
            | BuiltIn::AppMute
            | BuiltIn::NextApp => "Sound",
            BuiltIn::PlayPause
            | BuiltIn::NextTrack
            | BuiltIn::PreviousTrack
            | BuiltIn::SeekForward
            | BuiltIn::SeekBackward => "Media",
            BuiltIn::NextPage
            | BuiltIn::PreviousPage
            | BuiltIn::HomePage
            | BuiltIn::NextProfile
            | BuiltIn::PreviousProfile
            | BuiltIn::StartProfile => "Deck",
            BuiltIn::DeckBrighter | BuiltIn::DeckDimmer | BuiltIn::NextMode => "Deck",
            BuiltIn::TimerToggle | BuiltIn::TimerReset => "Timer",
            BuiltIn::NextState | BuiltIn::PreviousState => "Toggle",
            BuiltIn::ScrollUp
            | BuiltIn::ScrollDown
            | BuiltIn::ScrollLeft
            | BuiltIn::ScrollRight
            | BuiltIn::ZoomIn
            | BuiltIn::ZoomOut
            | BuiltIn::ZoomReset => "Pointer",
        }
    }

    /// What `step` means for this built-in, if it takes one, with its
    /// default and the range it is kept to.
    ///
    /// The ranges are safety as much as sense: a volume step of 100 on a fast
    /// spin is a hearing risk, and a seek of an hour is a skip.
    pub fn step(self) -> Option<(StepUnit, f64, f64, f64)> {
        match self {
            BuiltIn::VolumeUp
            | BuiltIn::VolumeDown
            | BuiltIn::MicUp
            | BuiltIn::MicDown
            | BuiltIn::AppVolumeUp
            | BuiltIn::AppVolumeDown => Some((StepUnit::Percent, 2.0, 1.0, 20.0)),
            BuiltIn::DeckBrighter | BuiltIn::DeckDimmer => {
                Some((StepUnit::Percent, 5.0, 1.0, 25.0))
            }
            BuiltIn::SeekForward | BuiltIn::SeekBackward => {
                Some((StepUnit::Seconds, 5.0, 1.0, 600.0))
            }
            BuiltIn::ScrollUp
            | BuiltIn::ScrollDown
            | BuiltIn::ScrollLeft
            | BuiltIn::ScrollRight => Some((StepUnit::Notches, 1.0, 1.0, 10.0)),
            _ => None,
        }
    }

    /// Whether it goes through the virtual keyboard and pointer.
    pub fn needs_virtual_input(self) -> bool {
        matches!(
            self,
            BuiltIn::ScrollUp
                | BuiltIn::ScrollDown
                | BuiltIn::ScrollLeft
                | BuiltIn::ScrollRight
                | BuiltIn::ZoomIn
                | BuiltIn::ZoomOut
                | BuiltIn::ZoomReset
        )
    }

    /// Which gestures it can be bound to.
    pub fn slot_rule(self) -> SlotRule {
        match self {
            // It has to see the key come back up.
            BuiltIn::PushToTalk => SlotRule::KeyTapOnly,
            // They act on the key's own timer, or its own states.
            BuiltIn::TimerToggle
            | BuiltIn::TimerReset
            | BuiltIn::NextState
            | BuiltIn::PreviousState => SlotRule::KeyOnly,
            // They act on the knob's own modes, or on the app it remembers.
            BuiltIn::NextMode | BuiltIn::NextApp => SlotRule::KnobOnly,
            _ => SlotRule::Anywhere,
        }
    }

    /// The part of its own key it acts on, for the built-ins that act on the
    /// key they are bound to.
    pub fn acts_on(self) -> Option<KeyPart> {
        match self {
            BuiltIn::TimerToggle | BuiltIn::TimerReset => Some(KeyPart::Timer),
            BuiltIn::NextState | BuiltIn::PreviousState => Some(KeyPart::States),
            _ => None,
        }
    }

    /// What `target` names for it, if it takes one.
    pub fn target_kind(self) -> Option<TargetKind> {
        match self {
            BuiltIn::VolumeUp | BuiltIn::VolumeDown | BuiltIn::VolumeMute => {
                Some(TargetKind::AudioNode)
            }
            BuiltIn::PlayPause
            | BuiltIn::NextTrack
            | BuiltIn::PreviousTrack
            | BuiltIn::SeekForward
            | BuiltIn::SeekBackward => Some(TargetKind::Player),
            BuiltIn::AppVolumeUp | BuiltIn::AppVolumeDown | BuiltIn::AppMute => {
                Some(TargetKind::App)
            }
            BuiltIn::SetOutput => Some(TargetKind::Sink),
            _ => None,
        }
    }

    /// Whether `target` means anything to it.
    pub fn takes_target(self) -> bool {
        self.target_kind().is_some()
    }

    /// Whether it moves between pages or profiles, which a spin must apply
    /// once, not once per detent.
    pub fn is_navigation(self) -> bool {
        matches!(
            self,
            BuiltIn::NextPage
                | BuiltIn::PreviousPage
                | BuiltIn::HomePage
                | BuiltIn::NextProfile
                | BuiltIn::PreviousProfile
                | BuiltIn::StartProfile
        )
    }
}

/// A built-in with its step, as written in a table.
#[derive(Clone, Debug, PartialEq)]
pub struct Invocation {
    pub action: BuiltIn,
    pub step: Option<f64>,
    /// Which one, for the built-ins that act on something: a player such as
    /// `spotify`, `mic` or a PipeWire node, an app, or an output; see
    /// [`BuiltIn::target_kind`]. Defaults to the playing player, the default
    /// output, and the app playing sound.
    pub target: Option<String>,
}

impl Invocation {
    /// The step to use: as written, kept in range, or the default. A step
    /// that is not a number (TOML has `nan` and `inf`) is no step at all:
    /// NaN would pass straight through the clamp.
    pub fn step(&self) -> f64 {
        match self.action.step() {
            Some((_, default, min, max)) => self
                .step
                .filter(|step| step.is_finite())
                .unwrap_or(default)
                .clamp(min, max),
            None => 0.0,
        }
    }
}

/// What a gesture does.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// A shell command, run with `sh -c`.
    Shell(String),
    BuiltIn(Invocation),
    /// A keystroke chord, as written. Checked by the validator; parsed by
    /// [`crate::keys::parse_chord`].
    Keys(String),
}

impl Action {
    pub fn built_in(action: BuiltIn) -> Self {
        Action::BuiltIn(Invocation {
            action,
            step: None,
            target: None,
        })
    }

    pub fn keys(chord: &str) -> Self {
        Action::Keys(chord.to_string())
    }

    /// The shell command, if that is what this is.
    pub fn shell(&self) -> Option<&str> {
        match self {
            Action::Shell(command) => Some(command),
            _ => None,
        }
    }

    /// The built-in, if that is what this is.
    pub fn as_built_in(&self) -> Option<&Invocation> {
        match self {
            Action::BuiltIn(invocation) => Some(invocation),
            _ => None,
        }
    }

    /// Whether it needs the virtual keyboard and pointer.
    pub fn needs_virtual_input(&self) -> bool {
        match self {
            Action::Keys(_) => true,
            Action::BuiltIn(invocation) => invocation.action.needs_virtual_input(),
            Action::Shell(_) => false,
        }
    }

    /// Words for a person.
    pub fn describe(&self) -> String {
        match self {
            Action::Shell(command) => format!("run `{command}`"),
            Action::BuiltIn(invocation) => invocation.action.describe().to_string(),
            Action::Keys(chord) => format!("press {chord}"),
        }
    }
}

/// The table form, before it is checked for having exactly one of its fields.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    #[serde(default)]
    action: Option<BuiltIn>,
    #[serde(default)]
    step: Option<f64>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    keys: Option<String>,
    /// Accepted so a table can also spell a shell command, which lets a UI
    /// write every slot the same way.
    #[serde(default)]
    exec: Option<String>,
}

impl<'de> Deserialize<'de> for Action {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ActionVisitor;

        impl<'de> Visitor<'de> for ActionVisitor {
            type Value = Action;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(
                    "a shell command, or a table with `action` (a built-in) or `keys` (a chord)",
                )
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Action, E> {
                Ok(Action::Shell(value.to_string()))
            }

            fn visit_string<E: de::Error>(self, value: String) -> Result<Action, E> {
                Ok(Action::Shell(value))
            }

            // Going through the map deserializer keeps serde's own messages
            // for a misspelt built-in or an unknown field, which list what
            // was expected -- an untagged enum would say only "did not match".
            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Action, M::Error> {
                let table = Table::deserialize(de::value::MapAccessDeserializer::new(map))?;
                match (table.action, table.keys, table.exec) {
                    (Some(action), None, None) => Ok(Action::BuiltIn(Invocation {
                        action,
                        step: table.step,
                        target: table.target,
                    })),
                    (None, Some(keys), None) if table.step.is_none() && table.target.is_none() => {
                        Ok(Action::Keys(keys))
                    }
                    (None, None, Some(exec)) if table.step.is_none() && table.target.is_none() => {
                        Ok(Action::Shell(exec))
                    }
                    (None, None, None) => Err(de::Error::custom(
                        "an action table needs `action`, `keys` or `exec`",
                    )),
                    (None, _, _) if table.step.is_some() || table.target.is_some() => Err(
                        de::Error::custom("`step` and `target` only go with a built-in `action`"),
                    ),
                    _ => Err(de::Error::custom(
                        "an action table takes one of `action`, `keys` or `exec`, not several",
                    )),
                }
            }
        }

        deserializer.deserialize_any(ActionVisitor)
    }
}

/// A ready-made set of gestures for a knob.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// Turn: output volume. Press: mute. Ring: the level.
    Volume,
    /// Turn: microphone volume. Press: mute. Ring: the level, red while muted.
    Mic,
    /// Turn: next and previous sound output. Press: mute. Ring: which one.
    Outputs,
    /// Turn: one app's volume. Press: the next app, or mute it when the knob
    /// names one. Ring: its level.
    AppVolume,
    /// Turn: next and previous track. Press: play or pause.
    Tracks,
    /// Turn: skip forward and back. Press: play or pause.
    Seek,
    /// Turn: next and previous page. Press: the home page. Ring: position.
    Pages,
    /// Turn: next and previous profile. Press: the start profile.
    Profiles,
    /// Turn: the deck's own brightness. Ring: the level.
    DeckBrightness,
    /// Turn: the mouse wheel.
    Scroll,
    /// Turn: ctrl and the wheel. Press: ctrl+0.
    Zoom,
    /// Turn: ctrl+page down and ctrl+page up, which move through tabs in
    /// browsers, editors and terminals alike.
    Tabs,
    /// Turn: ctrl+alt+right and ctrl+alt+left, GNOME's workspace keys.
    Workspaces,
    /// Turn: the down and up arrows.
    UpDown,
    /// Turn: the right and left arrows.
    LeftRight,
}

/// What a knob shows on its ring while it is being turned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RingShows {
    OutputLevel,
    InputLevel,
    DeckBrightness,
    PagePosition,
    ProfilePosition,
    /// Which of the outputs being turned through is the default.
    OutputPosition,
    /// The level of the app the knob turns, red while it is muted.
    AppLevel,
}

impl Preset {
    pub const ALL: &'static [Preset] = &[
        Preset::Volume,
        Preset::Mic,
        Preset::Outputs,
        Preset::AppVolume,
        Preset::Tracks,
        Preset::Seek,
        Preset::Pages,
        Preset::Profiles,
        Preset::DeckBrightness,
        Preset::Scroll,
        Preset::Zoom,
        Preset::Tabs,
        Preset::Workspaces,
        Preset::UpDown,
        Preset::LeftRight,
    ];

    /// Where it is in [`Preset::ALL`]; see [`BuiltIn::index`].
    pub fn index(self) -> usize {
        match self {
            Preset::Volume => 0,
            Preset::Mic => 1,
            Preset::Outputs => 2,
            Preset::AppVolume => 3,
            Preset::Tracks => 4,
            Preset::Seek => 5,
            Preset::Pages => 6,
            Preset::Profiles => 7,
            Preset::DeckBrightness => 8,
            Preset::Scroll => 9,
            Preset::Zoom => 10,
            Preset::Tabs => 11,
            Preset::Workspaces => 12,
            Preset::UpDown => 13,
            Preset::LeftRight => 14,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Preset::Volume => "volume",
            Preset::Mic => "mic",
            Preset::Outputs => "outputs",
            Preset::AppVolume => "app_volume",
            Preset::Tracks => "tracks",
            Preset::Seek => "seek",
            Preset::Pages => "pages",
            Preset::Profiles => "profiles",
            Preset::DeckBrightness => "deck_brightness",
            Preset::Scroll => "scroll",
            Preset::Zoom => "zoom",
            Preset::Tabs => "tabs",
            Preset::Workspaces => "workspaces",
            Preset::UpDown => "up_down",
            Preset::LeftRight => "left_right",
        }
    }

    /// Its name for a person: the UI's cards and the on-screen display when
    /// a knob changes mode.
    pub fn title(self) -> &'static str {
        match self {
            Preset::Volume => "Volume",
            Preset::Mic => "Microphone",
            Preset::Outputs => "Output switcher",
            Preset::AppVolume => "App volume",
            Preset::Tracks => "Tracks",
            Preset::Seek => "Seek",
            Preset::Pages => "Pages",
            Preset::Profiles => "Profiles",
            Preset::DeckBrightness => "Deck brightness",
            Preset::Scroll => "Scroll",
            Preset::Zoom => "Zoom",
            Preset::Tabs => "Tabs",
            Preset::Workspaces => "Workspaces",
            Preset::UpDown => "Up/down",
            Preset::LeftRight => "Left/right",
        }
    }

    /// What a knob's `target` names for this preset, if anything.
    ///
    /// The microphone preset has none because it always means the default
    /// input. The output switcher has none because its press mutes whichever
    /// output it has just switched to.
    pub fn target_kind(self) -> Option<TargetKind> {
        match self {
            Preset::Volume => Some(TargetKind::AudioNode),
            Preset::Tracks | Preset::Seek => Some(TargetKind::Player),
            Preset::AppVolume => Some(TargetKind::App),
            _ => None,
        }
    }

    /// The press, clockwise and anticlockwise gestures, with `step` applied
    /// to whichever built-ins take one and `target` to those of the preset's
    /// own kind.
    ///
    /// Only its own kind: a target meant for the preset's turn must not
    /// reach a built-in that reads it as something else.
    pub fn gestures(self, step: Option<f64>, target: Option<&str>) -> [Option<Action>; 3] {
        let kind = self.target_kind();
        let b = |action: BuiltIn| {
            Some(Action::BuiltIn(Invocation {
                action,
                step,
                target: target
                    .filter(|_| kind.is_some() && action.target_kind() == kind)
                    .map(str::to_string),
            }))
        };
        let k = |chord: &str| Some(Action::keys(chord));
        match self {
            Preset::Volume => [
                b(BuiltIn::VolumeMute),
                b(BuiltIn::VolumeUp),
                b(BuiltIn::VolumeDown),
            ],
            Preset::Mic => [b(BuiltIn::MicMute), b(BuiltIn::MicUp), b(BuiltIn::MicDown)],
            Preset::Outputs => [
                b(BuiltIn::VolumeMute),
                b(BuiltIn::NextOutput),
                b(BuiltIn::PreviousOutput),
            ],
            // A knob that names its app has no other app to move on to, so
            // its press mutes that one instead.
            Preset::AppVolume => [
                b(if target.is_some() {
                    BuiltIn::AppMute
                } else {
                    BuiltIn::NextApp
                }),
                b(BuiltIn::AppVolumeUp),
                b(BuiltIn::AppVolumeDown),
            ],
            Preset::Tracks => [
                b(BuiltIn::PlayPause),
                b(BuiltIn::NextTrack),
                b(BuiltIn::PreviousTrack),
            ],
            Preset::Seek => [
                b(BuiltIn::PlayPause),
                b(BuiltIn::SeekForward),
                b(BuiltIn::SeekBackward),
            ],
            Preset::Pages => [
                b(BuiltIn::HomePage),
                b(BuiltIn::NextPage),
                b(BuiltIn::PreviousPage),
            ],
            Preset::Profiles => [
                b(BuiltIn::StartProfile),
                b(BuiltIn::NextProfile),
                b(BuiltIn::PreviousProfile),
            ],
            Preset::DeckBrightness => [None, b(BuiltIn::DeckBrighter), b(BuiltIn::DeckDimmer)],
            Preset::Scroll => [None, b(BuiltIn::ScrollDown), b(BuiltIn::ScrollUp)],
            Preset::Zoom => [
                b(BuiltIn::ZoomReset),
                b(BuiltIn::ZoomIn),
                b(BuiltIn::ZoomOut),
            ],
            // No press on the keystroke presets: a knob is pressed by accident
            // while it is turned, and a stray key lands in whatever window
            // has focus.
            Preset::Tabs => [None, k("ctrl+page_down"), k("ctrl+page_up")],
            Preset::Workspaces => [None, k("ctrl+alt+right"), k("ctrl+alt+left")],
            Preset::UpDown => [None, k("down"), k("up")],
            Preset::LeftRight => [None, k("right"), k("left")],
        }
    }

    pub fn ring(self) -> Option<RingShows> {
        match self {
            Preset::Volume => Some(RingShows::OutputLevel),
            Preset::Mic => Some(RingShows::InputLevel),
            Preset::Outputs => Some(RingShows::OutputPosition),
            Preset::AppVolume => Some(RingShows::AppLevel),
            Preset::DeckBrightness => Some(RingShows::DeckBrightness),
            Preset::Pages => Some(RingShows::PagePosition),
            Preset::Profiles => Some(RingShows::ProfilePosition),
            _ => None,
        }
    }

    /// Words for a person, for the UI's preset cards.
    pub fn describe(self) -> &'static str {
        match self {
            Preset::Volume => "Turn for volume, press to mute. The ring shows the level.",
            Preset::Mic => {
                "Turn for microphone volume, press to mute. The ring turns red while muted."
            }
            Preset::Outputs => {
                "Turn through sound outputs, press to mute. The ring shows which one is on."
            }
            Preset::AppVolume => {
                "Turn for one app's volume, press for the next app. The ring shows its level."
            }
            Preset::Tracks => "Turn for the next or previous track, press to play or pause.",
            Preset::Seek => "Turn to skip forward or back, press to play or pause.",
            Preset::Pages => "Turn through this profile's pages, press for the home page.",
            Preset::Profiles => "Turn through profiles, press for the start profile.",
            Preset::DeckBrightness => "Turn for the deck's brightness.",
            Preset::Scroll => "Turn to scroll whatever is under the pointer.",
            Preset::Zoom => "Turn to zoom, press to reset it.",
            Preset::Tabs => "Turn through tabs in browsers, editors and terminals.",
            Preset::Workspaces => "Turn through workspaces (ctrl+alt+arrows, GNOME's default).",
            Preset::UpDown => "Turn to press the down and up arrows.",
            Preset::LeftRight => "Turn to press the right and left arrows.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Slot {
        exec: Action,
    }

    fn parse(text: &str) -> Result<Action, String> {
        toml::from_str::<Slot>(text)
            .map(|slot| slot.exec)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn a_string_is_still_a_shell_command() {
        assert_eq!(
            parse("exec = \"firefox\""),
            Ok(Action::Shell("firefox".into()))
        );
    }

    #[test]
    fn a_table_is_a_built_in_or_a_chord() {
        assert_eq!(
            parse("exec = { action = \"volume_up\", step = 5 }"),
            Ok(Action::BuiltIn(Invocation {
                action: BuiltIn::VolumeUp,
                step: Some(5.0),
                target: None,
            }))
        );
        assert_eq!(
            parse("exec = { keys = \"ctrl+t\" }"),
            Ok(Action::keys("ctrl+t"))
        );
        assert_eq!(
            parse("exec = { exec = \"ls\" }"),
            Ok(Action::Shell("ls".into()))
        );
    }

    #[test]
    fn a_bad_table_says_what_it_wanted() {
        let unknown = parse("exec = { action = \"volume_upp\" }").unwrap_err();
        assert!(unknown.contains("volume_up"), "{unknown}");
        let both = parse("exec = { action = \"play_pause\", keys = \"a\" }").unwrap_err();
        assert!(both.contains("not several"), "{both}");
        let stray = parse("exec = { keys = \"a\", step = 2 }").unwrap_err();
        assert!(stray.contains("step"), "{stray}");
        let empty = parse("exec = {}").unwrap_err();
        assert!(empty.contains("needs"), "{empty}");
        assert!(parse("exec = { action = \"play_pause\", colour = 1 }").is_err());
    }

    #[test]
    fn steps_are_kept_in_range() {
        let loud = Invocation {
            action: BuiltIn::VolumeUp,
            step: Some(100.0),
            target: None,
        };
        assert_eq!(loud.step(), 20.0);
        let default = Invocation {
            action: BuiltIn::SeekForward,
            step: None,
            target: None,
        };
        assert_eq!(default.step(), 5.0);
    }

    #[test]
    fn every_built_in_has_a_name_that_round_trips() {
        for built_in in BuiltIn::ALL {
            let text = format!("exec = {{ action = \"{}\" }}", built_in.name());
            assert_eq!(parse(&text), Ok(Action::built_in(*built_in)));
        }
    }

    #[test]
    fn every_preset_names_real_chords() {
        for preset in Preset::ALL {
            for action in preset.gestures(None, None).into_iter().flatten() {
                if let Action::Keys(chord) = action {
                    assert!(
                        crate::keys::parse_chord(&chord).is_ok(),
                        "{preset:?}: {chord}"
                    );
                }
            }
        }
    }

    /// Every variant serde knows, in the order they are declared, read from
    /// the message it gives for one it does not know. The derive is the one
    /// list that cannot leave a variant out.
    fn declared<T: serde::de::DeserializeOwned>() -> Vec<String> {
        use serde::de::IntoDeserializer;
        let unknown: de::value::StrDeserializer<de::value::Error> = "?".into_deserializer();
        let message = match T::deserialize(unknown) {
            Ok(_) => panic!("\"?\" named something"),
            Err(e) => e.to_string(),
        };
        let (_, expected) = message
            .split_once("expected")
            .expect("serde lists variants");
        expected
            .split('`')
            .skip(1)
            .step_by(2)
            .map(String::from)
            .collect()
    }

    #[test]
    fn all_lists_every_built_in_once_in_the_order_of_its_index() {
        let listed: Vec<&str> = BuiltIn::ALL.iter().map(|b| b.name()).collect();
        assert_eq!(listed, declared::<BuiltIn>());
        for (i, built_in) in BuiltIn::ALL.iter().enumerate() {
            assert_eq!(built_in.index(), i, "{built_in:?}");
        }
    }

    #[test]
    fn all_lists_every_preset_once_in_the_order_of_its_index() {
        let listed: Vec<&str> = Preset::ALL.iter().map(|p| p.name()).collect();
        assert_eq!(listed, declared::<Preset>());
        for (i, preset) in Preset::ALL.iter().enumerate() {
            assert_eq!(preset.index(), i, "{preset:?}");
        }
    }

    fn targets(gestures: [Option<Action>; 3]) -> [Option<String>; 3] {
        gestures.map(|action| action?.as_built_in()?.target.clone())
    }

    #[test]
    fn a_knob_s_target_reaches_only_the_built_ins_of_its_preset_s_kind() {
        let x = || Some("x".to_string());
        assert_eq!(
            targets(Preset::Volume.gestures(None, Some("x"))),
            [x(), x(), x()]
        );
        assert_eq!(
            targets(Preset::Tracks.gestures(None, Some("x"))),
            [x(), x(), x()]
        );
        // The output switcher's press mutes the output it has just switched
        // to, and the microphone is always the default input.
        assert_eq!(
            targets(Preset::Outputs.gestures(None, Some("x"))),
            [None, None, None]
        );
        assert_eq!(
            targets(Preset::Mic.gestures(None, Some("x"))),
            [None, None, None]
        );
        assert_eq!(
            targets(Preset::AppVolume.gestures(None, Some("x"))),
            [x(), x(), x()]
        );
    }

    #[test]
    fn an_app_knob_that_names_its_app_mutes_it_and_one_that_does_not_moves_on() {
        let press = |target| {
            Preset::AppVolume.gestures(Some(4.0), target)[0]
                .as_ref()
                .and_then(Action::as_built_in)
                .map(|i| i.action)
        };
        assert_eq!(press(Some("spotify")), Some(BuiltIn::AppMute));
        assert_eq!(press(None), Some(BuiltIn::NextApp));
        let [_, up, _] = Preset::AppVolume.gestures(Some(4.0), None);
        let up = up.unwrap();
        let up = up.as_built_in().unwrap();
        assert_eq!((up.action, up.step()), (BuiltIn::AppVolumeUp, 4.0));
    }

    #[test]
    fn targets_are_held_to_the_shape_their_kind_needs() {
        let node = TargetKind::AudioNode;
        assert!(node.accepts("mic"));
        assert!(node.accepts("42"));
        assert!(node.accepts("alsa_output.pci-0000_00_1f.3.analog-stereo"));
        assert!(node.accepts(&"a".repeat(MAX_TARGET)));
        assert!(!node.accepts(&"a".repeat(MAX_TARGET + 1)));
        assert!(!node.accepts(""));
        assert!(
            !node.accepts("-p"),
            "a sound tool would read it as an option"
        );
        assert!(!node.accepts("Spotify Player"));
        for kind in [TargetKind::Player, TargetKind::App, TargetKind::Sink] {
            assert!(kind.accepts("Tiger Lake-H HD Audio Controller Speaker"));
            assert!(
                kind.accepts(&"é".repeat(MAX_TARGET)),
                "characters, not bytes"
            );
            assert!(!kind.accepts(""));
            assert!(!kind.accepts("Head\nphones"));
            assert!(!kind.accepts("\u{1b}[31mred"));
        }
    }

    #[test]
    fn built_ins_that_act_on_their_own_key_or_knob_say_so() {
        assert_eq!(BuiltIn::PushToTalk.slot_rule(), SlotRule::KeyTapOnly);
        assert_eq!(BuiltIn::TimerToggle.slot_rule(), SlotRule::KeyOnly);
        assert_eq!(BuiltIn::TimerReset.slot_rule(), SlotRule::KeyOnly);
        assert_eq!(BuiltIn::NextState.slot_rule(), SlotRule::KeyOnly);
        assert_eq!(BuiltIn::PreviousState.slot_rule(), SlotRule::KeyOnly);
        assert_eq!(BuiltIn::NextMode.slot_rule(), SlotRule::KnobOnly);
        assert_eq!(BuiltIn::NextApp.slot_rule(), SlotRule::KnobOnly);
        assert_eq!(BuiltIn::NextOutput.slot_rule(), SlotRule::Anywhere);
        assert_eq!(BuiltIn::SetOutput.target_kind(), Some(TargetKind::Sink));
        assert_eq!(BuiltIn::AppMute.target_kind(), Some(TargetKind::App));
        assert!(!BuiltIn::MicMute.takes_target());
    }

    #[test]
    fn every_built_in_that_acts_on_its_own_key_says_which_part() {
        for built_in in BuiltIn::ALL {
            let key_only = built_in.slot_rule() == SlotRule::KeyOnly;
            assert_eq!(
                built_in.acts_on().is_some(),
                key_only,
                "{built_in:?}: a key-only built-in acts on something its key has"
            );
        }
        assert_eq!(BuiltIn::TimerToggle.acts_on(), Some(KeyPart::Timer));
        assert_eq!(BuiltIn::PreviousState.acts_on(), Some(KeyPart::States));
        assert_eq!(BuiltIn::PushToTalk.acts_on(), None);
    }
}
