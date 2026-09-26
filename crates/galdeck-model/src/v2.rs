//! The configuration model: profiles, pages, themes.
//!
//! Version 1 was a single flat file of pages. Version 2 splits it by who
//! authored what and by what it is about: one small global file, one file per
//! profile, one file per theme. A v1 config still loads — it is migrated in
//! memory on the way in, so nothing on disk changes until the user asks.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

use crate::action::{Action, BuiltIn, Preset};
use crate::animation::Animation;
use crate::backdrop::Backdrop;
use crate::color::ColorRef;
use crate::icon::IconRef;
use crate::theme::StyleLayer;
use crate::widget::{LcdTile, Widget};

/// The version this build writes and understands.
pub const CURRENT_VERSION: u32 = 2;

/// `galdeck.toml` — the small file at the root.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Global {
    /// Refused rather than guessed if it is from the future: a newer daemon
    /// may have written fields this one would silently drop.
    pub version: u32,
    /// Panel brightness 0-100. One global value: the hardware has no
    /// per-key or per-surface brightness and no way to read it back.
    #[serde(default = "default_brightness")]
    pub brightness: u8,
    /// Font used when a theme does not name one.
    #[serde(default)]
    pub font: Option<PathBuf>,
    /// Profile to start in. Defaults to the first one, alphabetically.
    #[serde(default)]
    pub profile: Option<String>,
    /// Knobs for every profile. A profile's or a page's own replace these
    /// one gesture at a time, so a page that only changes what pressing does
    /// keeps the turn it inherited.
    #[serde(default)]
    pub encoders: Vec<EncoderConfig>,
    /// Whether the daemon may create a virtual keyboard and pointer for
    /// keystroke and scroll actions. Off means it never opens /dev/uinput,
    /// whatever the system would allow.
    #[serde(default = "default_true")]
    pub virtual_input: bool,
    /// The sound outputs `next_output` and `previous_output` turn through,
    /// in this order. Each entry picks every usable output whose name
    /// contains it; one that matches nothing, such as headphones that are
    /// unplugged, is passed over. Empty means every usable output.
    #[serde(default)]
    pub outputs: Vec<String>,
    /// The icon theme a key's `icon` names are looked up in. Unset means
    /// the desktop's own, as far as the daemon can ask for it, else Adwaita.
    #[serde(default)]
    pub icon_theme: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for Global {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            brightness: default_brightness(),
            font: None,
            profile: None,
            encoders: Vec::new(),
            virtual_input: true,
            outputs: Vec::new(),
            icon_theme: None,
        }
    }
}

fn default_brightness() -> u8 {
    60
}

/// `profiles/<id>.toml`. The id is the filename stem.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Human-readable name for the user interface.
    #[serde(default)]
    pub name: Option<String>,
    /// Theme to style this profile with.
    #[serde(default)]
    pub theme: Option<String>,
    /// Page to start on. Defaults to the first one.
    #[serde(default)]
    pub home: Option<String>,
    /// Style overrides for the whole profile.
    #[serde(default)]
    pub style: StyleLayer,
    /// A background for every page in this profile, replacing the theme's.
    #[serde(default)]
    pub background: Option<Backdrop>,
    /// Knobs for every page in this profile, over the global ones and under
    /// each page's own.
    #[serde(default)]
    pub encoders: Vec<EncoderConfig>,
    /// The info screen's layout grid, for every page that does not set its
    /// own. Defaults to 12 columns by 6 rows.
    #[serde(default)]
    pub lcd_columns: Option<u8>,
    #[serde(default)]
    pub lcd_rows: Option<u8>,
    #[serde(default)]
    pub pages: Vec<Page>,
    /// The keyboard's lighting, over the theme's, field by field.
    #[serde(default)]
    pub lighting: Option<crate::lighting::Lighting>,
    /// How widgets look in this profile, over its theme's, field by field.
    #[serde(default)]
    pub widgets: Option<crate::look::WidgetLooks>,
}

impl Profile {
    pub fn page(&self, id: &str) -> Option<&Page> {
        self.pages.iter().find(|page| page.id == id)
    }

    /// The page to start on: `home` if it names a real page, else the first.
    pub fn home_index(&self) -> usize {
        self.home
            .as_deref()
            .and_then(|home| self.pages.iter().position(|page| page.id == home))
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub id: String,
    #[serde(default)]
    pub lcd_text: Option<String>,
    /// Widgets laid out on the info screen. When there are any they replace
    /// `lcd_text`; tiles are drawn over the screen's background in order.
    #[serde(default)]
    pub lcd: Vec<LcdTile>,
    /// A background for this page, replacing the profile's and the theme's.
    #[serde(default)]
    pub background: Option<Backdrop>,
    /// This page's layout grid, replacing the profile's. Each defaults
    /// separately, so a page can change only its rows.
    #[serde(default)]
    pub lcd_columns: Option<u8>,
    #[serde(default)]
    pub lcd_rows: Option<u8>,
    #[serde(default)]
    pub style: StyleLayer,
    #[serde(default)]
    pub keys: Vec<KeyConfig>,
    #[serde(default)]
    pub encoders: Vec<EncoderConfig>,
    /// How widgets look on this page, over its profile's and its theme's.
    #[serde(default)]
    pub widgets: Option<crate::look::WidgetLooks>,
}

/// One key, numbered row-major from the top-left of the 3x4 grid.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyConfig {
    pub key: u8,
    #[serde(default)]
    pub label: Option<String>,
    /// A picture on disk, or an icon theme's name for one.
    #[serde(default)]
    pub icon: Option<IconRef>,
    /// What pressing it does: a shell command, a built-in, or a keystroke.
    #[serde(default)]
    pub exec: Option<Action>,
    /// What holding it does.
    ///
    /// Binding this changes when `exec` fires: a key that might be held cannot
    /// be answered until it is released, because until then nobody knows which
    /// it was. A key with no `hold` and no `double` still fires the instant it
    /// goes down.
    #[serde(default)]
    pub hold: Option<Action>,
    /// What two quick presses do.
    #[serde(default)]
    pub double: Option<Action>,
    /// Page to switch to when pressed.
    #[serde(default)]
    pub page: Option<String>,
    /// Profile to switch to when pressed.
    #[serde(default)]
    pub profile: Option<String>,
    /// Return to the previous page when pressed.
    #[serde(default)]
    pub back: bool,
    #[serde(default)]
    pub style: StyleLayer,
    /// Makes this key move. Frames are pre-rendered when the page is applied.
    #[serde(default)]
    pub animation: Option<Animation>,
    /// Makes this key show something that changes.
    ///
    /// The widget's text replaces the label once it has produced one; until
    /// then, and whenever it fails, the label is what shows.
    #[serde(default)]
    pub widget: Option<Widget>,
    /// Hands this key to a plugin, which then owns its text and colour.
    #[serde(default)]
    pub plugin: Option<PluginBinding>,
    /// Two to eight states the key steps through, one per tap: a toggle is
    /// two. Each can look different and run something as the key enters it.
    #[serde(default)]
    pub states: Vec<KeyState>,
    /// A shell command that prints which state the key is in, so the key
    /// follows a change made somewhere else. Without it the key shows the
    /// state it last moved to.
    #[serde(default)]
    pub status: Option<String>,
    /// How often `status` is run while the key is showing. Defaults to
    /// [`DEFAULT_STATUS_INTERVAL_MS`]; see [`KeyConfig::status_interval_ms`].
    #[serde(default)]
    pub status_interval_ms: Option<u32>,
}

/// Fewest states worth stepping through. One is nothing to switch between.
pub const MIN_STATES: usize = 2;
/// Most states a key may have. Taps only go forward, so reaching the one
/// before takes all the others; past a handful, a key per choice is quicker.
pub const MAX_STATES: usize = 8;
/// How often a key's `status` is read, when it does not say.
pub const DEFAULT_STATUS_INTERVAL_MS: u32 = 5000;
/// How often at most. Every read is a process, for as long as the page shows.
pub const MIN_STATUS_INTERVAL_MS: u32 = 2000;
/// Longest line of `status` output compared with a state's `match`, in
/// characters. Nothing a state is recognised by is longer.
pub const MAX_STATUS_LINE: usize = 128;

/// One of the states a key steps through.
///
/// ```toml
/// [[pages.keys.states]]
/// name = "on"
/// match = ["enabled"]
/// label = "Wi-Fi"
/// icon = "network-wireless-symbolic"
/// exec = "nmcli radio wifi on"
/// ```
///
/// Its label, icon, style and animation replace the key's own while the key
/// is in it; whatever it leaves out, the key's own supplies.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyState {
    /// What the state is called, which is how the daemon remembers it:
    /// unique on its key, ignoring case.
    #[serde(default)]
    pub name: String,
    /// What `status` prints while the key is in this state. Defaults to the
    /// name. Compared ignoring case and one pair of surrounding quotes, so
    /// gsettings' `'prefer-dark'` matches `prefer-dark`.
    #[serde(default, rename = "match")]
    pub matches: Vec<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub icon: Option<IconRef>,
    #[serde(default)]
    pub style: StyleLayer,
    #[serde(default)]
    pub animation: Option<Animation>,
    /// What runs as the key enters this state: what turns the thing the key
    /// stands for on, off, or to this setting. Never run just to show the
    /// state again -- after a restart, a reload, or a `status` read.
    #[serde(default)]
    pub exec: Option<Action>,
}

impl KeyState {
    /// What `status` prints in this state: its `match`, else its name.
    pub fn match_values(&self) -> impl Iterator<Item = &str> {
        let own = self.matches.iter().map(String::as_str);
        let name = self
            .matches
            .is_empty()
            .then_some(self.name.as_str())
            .into_iter();
        own.chain(name)
    }

    /// Whether `value`, already reduced by [`status_value`], means this
    /// state.
    pub fn is_matched_by(&self, value: &str) -> bool {
        let value = value.to_lowercase();
        self.match_values()
            .any(|candidate| match_key(candidate) == value)
    }
}

/// A `match` value or a state's name as it is compared: without the
/// quotes and the spaces around it, and ignoring case.
pub(crate) fn match_key(value: &str) -> String {
    unquote(value.trim()).to_lowercase()
}

/// What a line of `status` output is compared as: its first line with
/// anything on it, trimmed, at most [`MAX_STATUS_LINE`] characters, with one
/// pair of surrounding quotes taken off. `None` when it printed nothing.
pub fn status_value(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let line: String = line.chars().take(MAX_STATUS_LINE).collect();
    Some(unquote(&line).to_string())
}

/// One pair of quotes around the whole of `text`, `'...'` or `"..."`, taken
/// off. Only one: a value that is itself quoted keeps its own.
fn unquote(text: &str) -> &str {
    for quote in ['\'', '"'] {
        if let Some(inner) = text
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner;
        }
    }
    text
}

/// A key given over to a plugin.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginBinding {
    /// The plugin's id, which is its directory name under `plugins/`.
    pub id: String,
    /// Passed to the plugin verbatim when the key appears, so one plugin can
    /// serve several keys that mean different things.
    #[serde(default)]
    pub options: std::collections::BTreeMap<String, String>,
}

impl KeyConfig {
    /// Whether pressing this key does anything at all.
    ///
    /// A key owned by a plugin counts: the plugin is told about the press, and
    /// whether it does anything with it is its business.
    pub fn is_bound(&self) -> bool {
        self.implicit_tap().is_some()
            || self.implicit_hold().is_some()
            || self.exec.is_some()
            || self.hold.is_some()
            || self.double.is_some()
            || self.page.is_some()
            || self.profile.is_some()
            || self.back
            || self.plugin.is_some()
    }
}

impl KeyConfig {
    /// Whether the key's tap is bound to something, which leaves its widget
    /// none of its own.
    fn tap_is_taken(&self) -> bool {
        self.exec.is_some()
            || self.page.is_some()
            || self.profile.is_some()
            || self.back
            || self.plugin.is_some()
    }

    /// What a tap does when nothing is bound to it: a key with states moves
    /// on to the next, a key showing what is playing plays and pauses it, a
    /// key showing the output volume mutes it, and a timer starts and
    /// pauses. Never the microphone -- an accidental tap must not open a
    /// live mic.
    pub fn implicit_tap(&self) -> Option<Action> {
        if self.tap_is_taken() {
            return None;
        }
        // Ahead of the widget, which a key with states is not meant to have
        // either: which of the two a tap does must not depend on which
        // mistake was made.
        if !self.states.is_empty() {
            return Some(Action::built_in(BuiltIn::NextState));
        }
        let widget = self.widget.as_ref()?;
        let invocation = |action| {
            Action::BuiltIn(crate::action::Invocation {
                action,
                step: None,
                target: widget.source.clone(),
            })
        };
        match widget.kind {
            crate::widget::WidgetKind::Media => Some(invocation(crate::action::BuiltIn::PlayPause)),
            crate::widget::WidgetKind::Volume if widget.source.is_none() => {
                Some(invocation(crate::action::BuiltIn::VolumeMute))
            }
            crate::widget::WidgetKind::Timer | crate::widget::WidgetKind::Stopwatch => {
                Some(Action::built_in(BuiltIn::TimerToggle))
            }
            _ => None,
        }
    }

    /// What a hold does when nothing is bound to it: a timer resets.
    ///
    /// Only where the tap is the widget's too, so a key whose tap was given
    /// something else does not find its hold taken as well.
    pub fn implicit_hold(&self) -> Option<Action> {
        if self.hold.is_some() || self.tap_is_taken() || !self.states.is_empty() {
            return None;
        }
        self.widget
            .as_ref()
            .filter(|widget| widget.kind.is_timer())
            .map(|_| Action::built_in(BuiltIn::TimerReset))
    }

    /// What a tap does: the key's own binding, else its widget's.
    pub fn tap(&self) -> Option<Action> {
        self.exec.clone().or_else(|| self.implicit_tap())
    }

    /// What a hold does: the key's own binding, else its widget's.
    pub fn hold_action(&self) -> Option<Action> {
        self.hold.clone().or_else(|| self.implicit_hold())
    }

    /// Every action on the key, for checks that apply to all of them: its
    /// gestures, what its widget does when a timer finishes, and what each
    /// of its states runs as the key enters it.
    pub fn actions(&self) -> impl Iterator<Item = &Action> {
        [&self.exec, &self.hold, &self.double]
            .into_iter()
            .flatten()
            .chain(self.widget.as_ref().and_then(|w| w.on_done.as_ref()))
            .chain(self.states.iter().filter_map(|state| state.exec.as_ref()))
    }

    /// The state called `name`. Names are unique ignoring case, so a name
    /// written with other capitals still finds its state.
    pub fn state(&self, name: &str) -> Option<&KeyState> {
        self.states
            .iter()
            .find(|state| state.name == name)
            .or_else(|| {
                let name = name.to_lowercase();
                self.states
                    .iter()
                    .find(|state| state.name.to_lowercase() == name)
            })
    }

    /// The state `status` output says the key is in: the first whose
    /// `match` it is. `None` for output that is empty or matches nothing.
    pub fn state_matching(&self, output: &str) -> Option<&KeyState> {
        let value = status_value(output)?;
        self.states.iter().find(|state| state.is_matched_by(&value))
    }

    /// How often `status` is read: as written, else the default, and never
    /// more often than [`MIN_STATUS_INTERVAL_MS`].
    pub fn status_interval_ms(&self) -> u32 {
        self.status_interval_ms
            .unwrap_or(DEFAULT_STATUS_INTERVAL_MS)
            .max(MIN_STATUS_INTERVAL_MS)
    }

    /// The key as it looks in the state called `name`: the state's label,
    /// icon and animation in place of the key's own, and its style over the
    /// key's, field by field. Whatever the state leaves out, the key's own
    /// supplies; a name that is no state is the key's own look.
    ///
    /// For drawing. The result has no states and no `status`, so it is a
    /// look rather than a binding: what its tap does is the key's to say.
    pub fn in_state(&self, name: &str) -> KeyConfig {
        let mut look = self.clone();
        look.states = Vec::new();
        look.status = None;
        look.status_interval_ms = None;
        if let Some(state) = self.state(name) {
            if state.label.is_some() {
                look.label.clone_from(&state.label);
            }
            if state.icon.is_some() {
                look.icon.clone_from(&state.icon);
            }
            look.style = self.style.over(&state.style);
            if state.animation.is_some() {
                look.animation.clone_from(&state.animation);
            }
        }
        look
    }
}

/// Most modes a knob may switch between: its ring has four segments to show
/// which one is on.
pub const MAX_MODES: usize = 4;

impl EncoderConfig {
    /// The modes this layer switches between, when it has them: the first
    /// [`MAX_MODES`], and only if there are at least two, since one mode is
    /// nothing to switch between.
    pub fn mode_stack(&self) -> Option<&[ModeEntry]> {
        (self.modes.len() >= 2).then(|| &self.modes[..self.modes.len().min(MAX_MODES)])
    }

    /// The preset this layer turns with in `mode`, with the step and target
    /// that go with it: the mode's own when there are modes -- which win
    /// over a `preset` beside them -- else the layer's.
    pub fn preset_in_mode(&self, mode: usize) -> Option<(Preset, Option<f64>, Option<&str>)> {
        match self.mode_stack() {
            Some(modes) => {
                let entry = &modes[mode.min(modes.len() - 1)];
                Some((entry.preset, entry.step, entry.target.as_deref()))
            }
            None => self
                .preset
                .map(|preset| (preset, self.step, self.target.as_deref())),
        }
    }

    /// Every preset this layer can turn with: its modes, or its preset.
    pub fn presets(&self) -> impl Iterator<Item = Preset> + '_ {
        (0..self.mode_stack().map_or(1, <[ModeEntry]>::len))
            .filter_map(|mode| self.preset_in_mode(mode).map(|(preset, ..)| preset))
    }

    /// Press, clockwise, anticlockwise and hold in `mode`, with the preset
    /// filled in and anything written explicitly taking its place.
    pub fn gestures_in_mode(&self, mode: usize) -> [Option<Action>; 4] {
        let [press, cw, ccw] = self
            .preset_in_mode(mode)
            .map(|(preset, step, target)| preset.gestures(step, target))
            .unwrap_or_default();
        [
            self.press.clone().or(press),
            self.cw.clone().or(cw),
            self.ccw.clone().or(ccw),
            self.hold.clone(),
        ]
    }

    /// The gestures in the first mode; see [`EncoderConfig::gestures_in_mode`].
    pub fn gestures(&self) -> [Option<Action>; 4] {
        self.gestures_in_mode(0)
    }

    /// Everything this layer could do in any of its modes, for checks that
    /// apply to all of them.
    pub fn all_gestures(&self) -> Vec<Action> {
        let modes = self.mode_stack().map_or(1, <[ModeEntry]>::len);
        let mut all: Vec<Action> = self.actions().cloned().collect();
        for mode in 0..modes {
            let [press, cw, ccw, _] = self.gestures_in_mode(mode);
            for action in [press, cw, ccw].into_iter().flatten() {
                if !all.contains(&action) {
                    all.push(action);
                }
            }
        }
        all
    }

    /// Whether this layer says what turning does: through modes, a preset,
    /// or a turn written out.
    pub fn supplies_turn(&self) -> bool {
        self.mode_stack().is_some()
            || self.preset.is_some()
            || self.cw.is_some()
            || self.ccw.is_some()
    }

    /// Every explicitly written action, for checks.
    pub fn actions(&self) -> impl Iterator<Item = &Action> {
        [&self.press, &self.cw, &self.ccw, &self.hold]
            .into_iter()
            .flatten()
    }
}

/// One rotary encoder: 0 is the left knob, 1 the right.
///
/// May be written in galdeck.toml, in a profile, or on a page; see
/// [`crate::Workspace::encoder_for`] for how the three combine.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncoderConfig {
    pub encoder: u8,
    /// A ready-made set of gestures. Any gesture also written here replaces
    /// the preset's.
    #[serde(default)]
    pub preset: Option<Preset>,
    /// Two to four presets to switch between by holding the knob, each a
    /// preset's name or a table with its own `step`, `target` and `ring`.
    /// In place of `preset`, and of the `step` and `target` beside it.
    #[serde(default)]
    pub modes: Vec<ModeEntry>,
    /// The step for the preset's built-ins: percent, seconds or notches.
    #[serde(default)]
    pub step: Option<f64>,
    /// What the preset acts on: a sound device for `volume`, a player for
    /// `tracks` and `seek`, an app for `app_volume`. Only the built-ins of
    /// that kind take it.
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub press: Option<Action>,
    #[serde(default)]
    pub cw: Option<Action>,
    #[serde(default)]
    pub ccw: Option<Action>,
    /// What holding the knob down does. Binding it makes `press` wait for the
    /// release, as `hold` on a key does.
    #[serde(default)]
    pub hold: Option<Action>,
    #[serde(default)]
    pub style: StyleLayer,
    /// Makes this ring move when it is at rest. Turn and click feedback still
    /// takes precedence -- an animation must not hide what the knob is doing.
    #[serde(default)]
    pub animation: Option<Animation>,
}

/// One of a knob's modes: a preset, with its own step, target and resting
/// ring colour.
///
/// Written as a preset's name, or as a table when it needs more:
///
/// ```toml
/// modes = ["volume", { preset = "app_volume", target = "spotify", ring = "#a3be8c" }]
/// ```
#[derive(Clone, Debug)]
pub struct ModeEntry {
    pub preset: Preset,
    pub step: Option<f64>,
    pub target: Option<String>,
    /// The ring's colour at rest while this mode is on, so the knob says
    /// which mode it is in without being turned.
    pub ring: Option<ColorRef>,
}

impl ModeEntry {
    pub fn of(preset: Preset) -> Self {
        Self {
            preset,
            step: None,
            target: None,
            ring: None,
        }
    }
}

// By the bits of the step, so an entry equals itself even with a step of
// NaN, which it has to for the daemon to find its mode again by the entry.
impl PartialEq for ModeEntry {
    fn eq(&self, other: &Self) -> bool {
        self.preset == other.preset
            && self.step.map(f64::to_bits) == other.step.map(f64::to_bits)
            && self.target == other.target
            && self.ring == other.ring
    }
}

impl Eq for ModeEntry {}

impl std::hash::Hash for ModeEntry {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.preset.hash(state);
        self.step.map(f64::to_bits).hash(state);
        self.target.hash(state);
        match &self.ring {
            None => 0u8.hash(state),
            Some(ColorRef::Literal(rgb)) => (1u8, rgb.r, rgb.g, rgb.b).hash(state),
            Some(ColorRef::Token(name)) => (2u8, name).hash(state),
        }
    }
}

/// The table form of a mode.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModeTable {
    preset: Preset,
    #[serde(default)]
    step: Option<f64>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    ring: Option<ColorRef>,
}

impl<'de> Deserialize<'de> for ModeEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::{self, MapAccess, Visitor};

        struct ModeVisitor;

        impl<'de> Visitor<'de> for ModeVisitor {
            type Value = ModeEntry;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a preset's name, or a table with `preset`")
            }

            // Through the preset's own deserializer, so a misspelt name gets
            // serde's list of the real ones.
            fn visit_str<E: de::Error>(self, value: &str) -> Result<ModeEntry, E> {
                Preset::deserialize(de::value::StrDeserializer::<E>::new(value)).map(ModeEntry::of)
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<ModeEntry, M::Error> {
                let table = ModeTable::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(ModeEntry {
                    preset: table.preset,
                    step: table.step,
                    target: table.target,
                    ring: table.ring,
                })
            }
        }

        deserializer.deserialize_any(ModeVisitor)
    }
}

/// Everything loaded from a config directory.
#[derive(Clone, Debug, Default)]
pub struct Workspace {
    pub global: Global,
    pub profiles: BTreeMap<String, Profile>,
    pub themes: BTreeMap<String, crate::theme::Theme>,
}

impl Workspace {
    /// The profile to start in.
    pub fn start_profile(&self) -> Option<&str> {
        self.global
            .profile
            .as_deref()
            .filter(|id| self.profiles.contains_key(*id))
            .or_else(|| self.profiles.keys().next().map(String::as_str))
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.get(id)
    }
}
