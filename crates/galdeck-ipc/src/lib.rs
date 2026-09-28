//! The control protocol shared by the daemon, the CLI and the web UI.
//!
//! Line-delimited JSON over a Unix stream socket: one request per line, one
//! response per line, in order. Simple enough that `socat` is a usable client,
//! which matters for a protocol people will script against.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use galdeck_model::{Diagnostic, Patch, Severity, Value};

/// Which part of the deck a brightness change applies to.
///
/// The module itself has one brightness control for the whole panel, so the
/// daemon accepts `All` and `LcdPanel` and refuses the encoders rather than
/// dimming everything and reporting success. The distinction is on the wire
/// so a future firmware that does more does not need a protocol break.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeckDevice {
    LeftEncoder,
    RightEncoder,
    LcdPanel,
    #[default]
    All,
}

/// A rectangle in absolute panel pixels, origin top-left.
///
/// A plain mirror of the framework's `galdeck::layout::Rect`, redeclared here
/// rather than re-exported: this crate is the protocol, and the daemon is free
/// to move to a framework version whose types have shifted without that
/// changing what goes over the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// Where a calibration's numbers came from -- worth knowing before trusting
/// them. A `Template` layout is arithmetic, not measured on any hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalSource {
    Template,
    File,
    Calibrated,
}

/// One calibrated key cell, as it resolves after bands and overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalZone {
    pub row: u8,
    pub column: u8,
    /// Row-major, matching the key index the device reports.
    pub index: u8,
    pub bounds: CalRect,
    /// Whether these bounds came from a per-zone override rather than from
    /// the grid. The wizard sets these when one key sits slightly off.
    pub overridden: bool,
}

/// The calibrated geometry of this unit: where the zones actually are.
///
/// Carries both the editable numbers and the zones they resolve to, because
/// deriving the second from the first means reimplementing the framework's
/// band and override rules in JavaScript, and a drawing that disagrees with
/// the panel is worse than no drawing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calibration {
    /// Where the layout file lives, so a UI can name it.
    pub path: String,
    pub source: CalSource,
    /// Visible area of the info screen, above the zones.
    pub screen: CalRect,
    /// Outer boundary of the whole zone area.
    pub bounds: CalRect,
    pub rows: u8,
    pub columns: u8,
    /// Edge offsets applied to every cell. Zero fills the track exactly;
    /// negative exposes bleed, positive overlaps the neighbouring track.
    pub bleed_x: i16,
    pub bleed_y: i16,
    pub zones: Vec<CalZone>,
    /// Panel dimensions, so a drawing can scale without hardcoding them.
    pub panel_width: u16,
    pub panel_height: u16,
    /// Edge length of a key image on the `02 07` path.
    ///
    /// Sent so a drawing can show it against the measured zone rather than
    /// hardcoding a number the framework owns. The firmware blits an image of
    /// this size and does not scale it, so where a zone is larger the
    /// difference is panel the key path simply cannot reach.
    pub key_image_size: u32,
    /// The editable text form, exactly as it is on disk.
    pub text: String,
    /// True while the daemon has handed the device to another process.
    pub released: bool,
    /// Why the saved file could not be read, when it could not. The rest of
    /// this struct then describes the template that was used instead.
    pub problem: Option<String>,
}

/// A request to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Ping,
    Status,
    SetBrightness {
        percent: u8,
        /// Defaulted, so `{"cmd":"set_brightness","percent":60}` still works.
        /// This protocol is documented as something `socat` can drive, and a
        /// field added later must not break the scripts people wrote against
        /// it.
        #[serde(default)]
        device: DeckDevice,
    },
    SwitchPage {
        name: String,
    },
    /// Switch to another profile.
    SwitchProfile {
        name: String,
    },
    Reload,
    /// Every configuration file, as text, for an editor to work on.
    GetConfig,
    /// The current page, resolved, with the config path of every control.
    ///
    /// An editor needs to know that the key in the top-left corner is
    /// `pages[0].keys[2]` in `profiles/work.toml`. Answering that here keeps
    /// the model in one place instead of reimplemented in JavaScript.
    GetLayout,
    /// Draw a calibration pattern on every key, at a chosen pixel size.
    ///
    /// The protocol's report builders never check a key image's dimensions --
    /// only its index -- so any size can be sent and the hardware's reaction
    /// observed. That is the only way to find out what the panel really is.
    TestPattern {
        size: u32,
    },
    /// Fill every calibrated zone through the panel region path.
    ///
    /// The companion to `TestPattern`, and the question it answers is the
    /// other half of the same one: that probe asks how much of a key the
    /// firmware's key path can cover, this one asks whether the region path
    /// can cover the rest. Each zone is drawn at its measured rectangle with
    /// a border on its exact edge.
    ZonePattern,
    /// Try edits without saving anything, and report what they would do.
    ///
    /// This is what makes live validation possible: the editor can show
    /// problems in an edit nobody has committed.
    ValidateConfig {
        file: String,
        patches: Vec<Patch>,
        /// The generation the editor read. Omitted skips the check.
        #[serde(default)]
        generation: Option<u64>,
    },
    /// Apply edits, save them, and reload.
    ApplyConfig {
        file: String,
        patches: Vec<Patch>,
        #[serde(default)]
        generation: Option<u64>,
    },
    /// Close the device handle and stay closed until resumed.
    ///
    /// `Ok` means the hidraw handle is *gone*, not that the daemon intends to
    /// close it: the caller opens the device the instant it reads the reply,
    /// and two handles on one hidraw node is the leading suspect for the
    /// module dropping off the USB bus. So the daemon holds this request open
    /// until its io thread confirms the close.
    ///
    /// Idempotent, and safe to send to a daemon with no device attached --
    /// there is nothing to wait for, and it answers at once.
    ReleaseDevice,
    /// Take the device back and repaint everything from the daemon's model.
    ///
    /// Also re-reads the calibration, because the only thing that changes it
    /// is a wizard run, and a wizard run always ends here.
    ResumeDevice,
    /// The calibrated geometry of this unit.
    GetCalibration,
    /// Replace the calibration's grid and save it.
    ///
    /// Only the numbers a person can sensibly type. Measured per-row bands
    /// and per-zone overrides are carried over, because they come from a
    /// wizard run and are the part of a calibration that was actually
    /// measured -- except where the edit moves the rows or columns they
    /// describe, which invalidates them wholesale.
    SetCalibration {
        screen: CalRect,
        bounds: CalRect,
        rows: u8,
        columns: u8,
        bleed_x: i16,
        bleed_y: i16,
    },
    /// Re-read the layout file from disk, discarding unsaved edits.
    ReloadCalibration,
    /// What a widget's `source` could name on this machine: sensors, cards,
    /// interfaces, filesystems and media players that exist right now.
    ///
    /// So an editor can offer a list rather than ask the user to go and read
    /// `/sys/class/hwmon` to find out that their CPU is called `k10temp`.
    WidgetSources,
    /// Everything the model knows how to do: built-ins, knob presets, key
    /// names. So an editor's pickers come from one list and cannot drift.
    Catalog,
    /// Do something now, as a key would: an action table's fields
    /// (`action`/`step`/`target`, `keys`, or `exec`). For an editor's "Try it".
    RunAction {
        fields: std::collections::BTreeMap<String, Value>,
    },
    /// Look a place up by name, for a weather widget.
    Geocode {
        name: String,
    },
    /// A one-time code that signs a browser in to the configuration UI.
    ///
    /// Answered by the control socket alone, never over HTTP: the socket is
    /// this user's and nobody else's, which is what makes a code it hands
    /// out worth trusting. The code goes in the page's address, and the page
    /// trades it for the token once; after that, or after the time to live,
    /// it is worth nothing.
    UiLogin {
        /// How long the code stays good, in seconds, up to five minutes.
        /// Omitted is one minute, for a browser opened straight away.
        #[serde(default)]
        ttl_s: Option<u32>,
    },
    /// Replace the UI's token with a new one and forget every code, so each
    /// open tab has to be signed in again. Control socket only, like
    /// `UiLogin`.
    UiRotateToken,
    /// Draw a widget that is not in the config, with made-up readings, for
    /// an editor's gallery.
    ///
    /// The fields are a widget table, as a patch would write them. The
    /// readings are the same every time, so a preview shows what a widget
    /// looks like rather than what this machine happens to be doing.
    RenderWidget {
        fields: std::collections::BTreeMap<String, Value>,
        width: u32,
        height: u32,
    },
    /// Every theme, for an editor: each value it sets, inherits from a theme
    /// it extends, or leaves to the built-in default, and which it is.
    GetThemes,
    /// Start a theme in `themes/<id>.toml`: one that extends `extends`, a
    /// copy of the theme `copy`, or an empty one. Refused if the file is
    /// already there.
    CreateTheme {
        id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        extends: Option<String>,
        #[serde(default)]
        copy: Option<String>,
    },
    /// Draw a theme with edits that are not saved, for an editor's preview.
    ///
    /// The patches are to the theme's own file, as `apply_config` would take
    /// them. Nothing is written, and the drawing is made even when the edits
    /// have problems, with whatever still resolves.
    PreviewTheme {
        theme: String,
        #[serde(default)]
        patches: Vec<Patch>,
    },
    /// The keyboard's LEDs, for drawing it: each one's index, name, place
    /// and group. The same for every Galleon, and answered with no keyboard
    /// there.
    KeyboardLayout,
    /// What the keyboard's lighting shows this moment, while the daemon
    /// lights it.
    KeyboardFrame,
    /// Draw keyboard lighting that is not saved, for an editor's preview.
    ///
    /// `lighting` is a `[lighting]` table as TOML. Its `@` colours resolve
    /// against the palette of `theme`, or of the profile showing's theme when
    /// that is left out. `presses` are keys pressed along the way, to show how
    /// they are answered. Nothing is written, and the drawing is made even
    /// when the lighting has problems, with whatever still resolves.
    PreviewLighting {
        lighting: String,
        #[serde(default)]
        theme: Option<String>,
        /// How long to draw, in seconds, and at what rate.
        #[serde(default)]
        seconds: Option<f32>,
        #[serde(default)]
        fps: Option<u8>,
        #[serde(default)]
        presses: Vec<LightPress>,
    },
    /// Store an image in the config directory, for a background or an icon.
    ///
    /// Base64, because the protocol is JSON. The name is reduced to a plain
    /// file name -- no directories -- and the reply says where it went.
    SaveAsset {
        name: String,
        data: String,
    },
    /// Download a picture from a link and store it as `SaveAsset` would.
    ///
    /// The daemon fetches it rather than the editor, because a page served
    /// by the daemon may only talk to the daemon. The reply is an `Asset`.
    FetchAsset {
        url: String,
    },
    /// The sound outputs and the apps playing sound on this machine right
    /// now, so an editor can offer names for a `target` and for `outputs`
    /// rather than ask for them to be typed.
    AudioTargets,
    /// Put a key on the current page in one of its states.
    ///
    /// With `run`, the state's `exec` runs as a tap would run it -- "Switch
    /// to this". Without, only what the key shows changes -- "Show this",
    /// for when the key has got out of step with what it stands for.
    /// Defaulted to not running anything, so a script that leaves it out
    /// never starts a command by accident.
    SetKeyState {
        /// Position on the current page, 0-11.
        key: u8,
        state: String,
        #[serde(default)]
        run: bool,
    },
    /// The names a key's `icon` can take from the icon theme, for an
    /// editor to offer.
    IconNames,
    /// Draw a key on the current page as it looks in one of its states, or
    /// as it looks with none, for an editor's previews of each state.
    RenderKeyState {
        /// Position on the current page, 0-11.
        key: u8,
        #[serde(default)]
        state: Option<String>,
    },
    /// Which of these programs are on the daemon's `PATH`, so an editor can
    /// say what a ready-made key needs before it is put on the deck. Looked
    /// for, never run.
    Which {
        names: Vec<String>,
    },
}

/// A reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Error {
        message: String,
    },
    Status(Status),
    Config(ConfigSnapshot),
    /// Boxed because a page's layout is by far the largest reply; the
    /// encoding on the wire is the same either way.
    Layout(Box<Layout>),
    Calibration(Calibration),
    WidgetSources(WidgetSources),
    Catalog(Catalog),
    Places {
        places: Vec<PlaceInfo>,
    },
    /// Where the configuration UI is, and a one-time code for it: the page
    /// to open is `http://127.0.0.1:{port}/?code={code}`.
    UiLogin {
        port: u16,
        code: String,
    },
    /// A picture, as a `data:` URL an `<img>` can show directly.
    Image {
        url: String,
    },
    /// Where a saved asset was written.
    Asset {
        path: String,
    },
    /// What `get_themes` found.
    Themes {
        themes: Vec<ThemeInfo>,
    },
    /// What `preview_theme` drew. Boxed, like `Layout`, because it is far
    /// larger than any other reply; the wire is the same either way.
    ThemePreview(Box<ThemePreview>),
    /// What `keyboard_layout` found.
    KeyboardLayout {
        #[serde(default)]
        leds: Vec<LedPlace>,
    },
    /// What `keyboard_frame` found: `None` while the daemon is not lighting
    /// the keyboard.
    KeyboardFrame {
        #[serde(default)]
        frame: Option<String>,
    },
    /// What `preview_lighting` drew. Boxed, like `ThemePreview`.
    LightingPreview(Box<LightingPreview>),
    /// Everything an edit would produce. An empty list means it is clean.
    Diagnostics {
        diagnostics: Vec<Diagnostic>,
    },
    /// What `audio_targets` found.
    AudioTargets {
        #[serde(default)]
        outputs: Vec<OutputInfo>,
        #[serde(default)]
        apps: Vec<AppInfo>,
    },
    /// What `icon_names` found, sorted.
    IconNames {
        #[serde(default)]
        names: Vec<String>,
    },
    /// The programs `which` asked about that are there.
    Which {
        #[serde(default)]
        found: Vec<String>,
    },
}

/// One of the keyboard's LEDs, for drawing it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LedPlace {
    /// Its place in a frame.
    pub index: u8,
    /// Its name in a config: `"W"`, `"LShift"`, `"Bar1"`.
    pub name: String,
    /// Its centre, in key widths from Esc's left and key heights from the
    /// function row; the light bar is above that, so negative.
    pub x: f32,
    pub y: f32,
    /// Its group's name in a config: `"letters"`, `"bar"`.
    pub group: String,
}

/// A key pressed during a lighting preview.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LightPress {
    /// The key's name, as in a config.
    pub key: String,
    /// Seconds into the preview.
    pub at: f32,
}

/// Keyboard lighting drawn for a preview.
///
/// Every frame, like `keyboard_frame`'s, is one `rrggbb` per LED in frame
/// order, run together: 147 LEDs, 882 characters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LightingPreview {
    pub fps: u8,
    pub frames: Vec<String>,
    /// What is wrong with the lighting, if anything.
    pub diagnostics: Vec<Diagnostic>,
    /// The lighting as it was read, for an editor to change a field of and
    /// write back. `None` when it could not be read at all.
    pub form: Option<LightingForm>,
}

/// A `[lighting]` table as written, field by field: what is left out is
/// `None`, and colours are as written, `@name` or `#rrggbb`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LightingForm {
    pub effect: Option<String>,
    pub colors: Option<Vec<String>>,
    pub speed: Option<f64>,
    pub brightness: Option<u8>,
    pub bar: Option<String>,
    /// Key, group or `all` names, and their colour.
    pub keys: std::collections::BTreeMap<String, String>,
    pub reactive: Option<ReactiveForm>,
}

/// `[lighting.reactive]` as written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReactiveForm {
    pub effect: Option<String>,
    pub color: Option<String>,
    pub fade_ms: Option<u32>,
}

/// A theme, for an editor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeInfo {
    /// Its id, which is its file's name without `.toml`.
    pub id: String,
    /// The file it is written in, for patching: `themes/<id>.toml`.
    pub file: String,
    pub name: Option<String>,
    pub extends: Option<String>,
    /// The themes it inherits from, nearest first, as far as the chain
    /// could be followed.
    pub ancestors: Vec<String>,
    /// The profiles that use it.
    pub profiles: Vec<String>,
    /// The themes that extend it, which change along with it.
    pub extended_by: Vec<String>,
    /// Every colour it can name: its own, then the ones it inherits.
    pub palette: Vec<PaletteEntryInfo>,
    /// Every style field, in the order an editor lists them.
    pub style: Vec<StyleFieldInfo>,
    /// Its background, or the nearest one it inherits. `theme` says which
    /// theme set it.
    pub background: Option<BackdropInfo>,
    /// `[lighting]` as this theme writes it.
    pub lighting: Option<LightingInfo>,
    /// What it inherits for lighting: the themes it extends, folded
    /// together.
    pub inherited_lighting: Option<LightingInfo>,
    /// `[widgets]` as this theme writes it.
    pub widgets: Option<WidgetLooksInfo>,
    /// What it inherits for widgets: the themes it extends, folded
    /// together.
    pub inherited_widgets: Option<WidgetLooksInfo>,
    /// `[motion]` as this theme writes it.
    pub motion: Option<MotionInfo>,
    /// What it inherits for motion: the themes it extends, folded together.
    pub inherited_motion: Option<MotionInfo>,
}

/// A `[widgets]` section, for an editor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetLooksInfo {
    /// For every widget.
    pub all: WidgetLookInfo,
    /// For one kind of widget each, over `all`.
    pub kinds: Vec<WidgetKindLookInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetKindLookInfo {
    /// The kind, as config names it: `clock`.
    pub kind: String,
    pub look: WidgetLookInfo,
}

/// How widgets look, as one layer writes it. Unset fields are inherited.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetLookInfo {
    pub view: Option<String>,
    /// As written, which may be a `@token`.
    pub color: Option<String>,
    pub color_hex: Option<String>,
    pub background: Option<String>,
    pub background_hex: Option<String>,
    pub opacity: Option<f32>,
    /// `area`, `line` or `bars`.
    pub graph: Option<String>,
    /// `rounded`, `flat` or `segmented`.
    pub bar: Option<String>,
    pub segments: Option<u8>,
    pub sweep: Option<u16>,
    pub thickness: Option<f32>,
    pub radius: Option<u32>,
}

/// A `[motion]` section, for an editor. What it leaves unset is inherited.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MotionInfo {
    pub press: Option<PressInfo>,
    pub alarm: Option<MotionAnimationInfo>,
    pub rings: Option<MotionAnimationInfo>,
}

/// What a key does when pressed, for an editor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PressInfo {
    /// `none`, `flash` or `dim`.
    pub kind: String,
    /// As written, which may be a `@token`.
    pub color: Option<String>,
    pub color_hex: Option<String>,
    /// As written.
    pub ms: Option<u32>,
}

/// An animation in `[motion]`, for an editor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MotionAnimationInfo {
    /// `pulse`, `breathe` and so on.
    pub kind: String,
    pub period_ms: u32,
    /// What it moves towards, as written, which may be a `@token`.
    pub to: Option<String>,
    pub to_hex: Option<String>,
}

/// A palette colour, for an editor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PaletteEntryInfo {
    pub name: String,
    /// As written: `#rrggbb` or `@token`.
    pub value: String,
    /// What it comes to, when it resolves.
    pub hex: Option<String>,
    /// The theme that defines it: this one, or the one it is inherited
    /// from.
    pub origin: String,
}

/// A style field, for an editor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StyleFieldInfo {
    /// Its name under `[style]`, such as `key_bg`.
    pub field: String,
    /// `color`, `size` (points) or `pixels`.
    pub kind: String,
    /// As this theme writes it, when it does.
    pub value: Option<String>,
    /// What it comes to: `#rrggbb` for a colour, else the number.
    pub resolved: String,
    /// The theme that supplies it, or `builtin`.
    pub origin: String,
    /// What it would come to if this theme did not set it, and where that
    /// would come from, for an editor to say what emptying it does.
    pub inherited: String,
    pub inherited_origin: String,
}

/// A `[lighting]` layer, for an editor. What it leaves unset is inherited.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LightingInfo {
    pub effect: Option<String>,
    /// As written, which may be `@tokens`.
    pub colors: Option<Vec<String>>,
    /// Each of those resolved, in the same order; `None` where one does
    /// not resolve.
    pub colors_hex: Vec<Option<String>>,
    pub speed: Option<f64>,
    pub brightness: Option<u8>,
    pub bar: Option<String>,
    pub bar_hex: Option<String>,
    /// Keys lit in a colour of their own.
    pub keys: Vec<LightingKeyInfo>,
    /// `[lighting.reactive]`: what a press lights.
    pub reactive: Option<ReactiveInfo>,
}

/// What a press on the keyboard lights, for an editor. Unset fields are
/// inherited, field by field.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ReactiveInfo {
    /// `ripple`, `glow` or `none`.
    pub effect: Option<String>,
    /// As written, which may be a `@token`.
    pub color: Option<String>,
    pub color_hex: Option<String>,
    pub fade_ms: Option<u32>,
}

/// Keys lit in a colour of their own, for an editor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LightingKeyInfo {
    /// Key names, separated by spaces, as written.
    pub keys: String,
    /// As written, which may be a `@token`.
    pub color: String,
    pub hex: Option<String>,
}

/// A theme drawn with edits that are not saved.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemePreview {
    /// Sample keys, as `data:` URLs: a label, then widgets in the theme's
    /// look -- a gauge, a graph past its critical threshold, a bar and a
    /// clock.
    pub keys: Vec<String>,
    /// The screen, as a `data:` URL.
    pub lcd: String,
    /// The colour a knob's ring rests at.
    pub ring: String,
    /// Every colour the theme can name, resolved.
    pub palette: Vec<PaletteEntryInfo>,
    /// The keyboard's lighting with the theme's chain folded in, when any
    /// theme in it has some.
    pub lighting: Option<LightingInfo>,
    /// How things move, with the theme's chain folded in, when any theme
    /// in it says. A press is also drawn, as the last of `keys`: a key
    /// mid-flash, as a picture cannot move.
    pub motion: Option<MotionInfo>,
    /// What is wrong with the edits. The preview is drawn anyway, from
    /// whatever still resolves.
    pub diagnostics: Vec<Diagnostic>,
}

/// A sound output, as `audio_targets` found it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputInfo {
    /// A short name to show: its nickname, or its description without the
    /// part it shares with its card. Made safe to show, and at most 40
    /// characters.
    pub display: String,
    /// PipeWire's name for it, which never changes while it exists.
    pub name: String,
    pub nick: Option<String>,
    pub description: Option<String>,
    /// False while it cannot play anything, such as headphones that are
    /// unplugged. The output switcher passes over these.
    pub usable: bool,
    /// Whether it is where sound goes now.
    pub default: bool,
}

/// An app with sound streams, as `audio_targets` found it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppInfo {
    /// What a `target` should say to reach it: its program, else its name.
    pub app: String,
    /// Its name to show, made safe to show.
    pub display: String,
    pub binary: Option<String>,
    /// Whether any of its streams is playing right now.
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub connected: bool,
    pub firmware: Option<String>,
    pub serial: Option<String>,
    /// Which profile is showing.
    ///
    /// Defaulted rather than required so a current CLI still gets a readable
    /// answer out of a daemon built before profiles existed -- which is the
    /// normal state of affairs for as long as an old build is installed.
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub profiles: Vec<String>,
    pub page: String,
    pub pages: Vec<String>,
    pub brightness: u8,
    /// True while the device has been handed to another process, which is
    /// why `connected` is false without the keyboard having gone anywhere.
    #[serde(default)]
    pub released: bool,
    #[serde(default)]
    pub capabilities: Capabilities,
}

/// The whole configuration, as the files it is written in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSnapshot {
    pub dir: PathBuf,
    pub files: Vec<ConfigFile>,
    /// Everything currently wrong with it.
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    /// Path relative to the config directory, e.g. `profiles/work.toml`.
    pub name: String,
    pub text: String,
    /// Bumped on every save. An edit carrying a stale one is refused.
    pub generation: u64,
}

/// The current page, with everything an editor needs to address it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    pub profile: String,
    /// The file the current profile is written in, for patching.
    pub file: String,
    pub page: String,
    /// Index of the current page within that file's `pages` array.
    pub page_index: usize,
    pub pages: Vec<String>,
    pub keys: Vec<KeyInfo>,
    pub encoders: Vec<EncoderInfo>,
    /// Whether a `back` key would go anywhere.
    pub can_go_back: bool,
    /// The page's text for the info screen, as configured. Not shown while
    /// there are tiles.
    #[serde(default)]
    pub lcd_text: Option<String>,
    /// Widgets laid out on the info screen, in the order they are drawn.
    #[serde(default)]
    pub lcd: Vec<TileInfo>,
    /// The grid tiles are placed on, so an editor does not hard-code it.
    #[serde(default)]
    pub lcd_grid: Option<LcdGrid>,
    /// `page` or `profile`: where the grid was set, when it was set at all.
    #[serde(default)]
    pub lcd_grid_origin: Option<String>,
    /// The background behind this page, if there is one, and where it was
    /// set.
    #[serde(default)]
    pub background: Option<BackdropInfo>,
    /// The `outputs` list in galdeck.toml, as written, for an editor of it:
    /// the outputs a switcher cycles through, in order.
    #[serde(default)]
    pub outputs: Vec<String>,
    /// What is wrong with the page that only the daemon can tell, such as an
    /// icon name no installed icon theme has, each at its place in the
    /// config. The config's own problems are in `get_config`'s diagnostics.
    #[serde(default)]
    pub warnings: Vec<Diagnostic>,
}

/// A background's settings, as configured.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackdropInfo {
    /// `page`, `profile` or `theme`: which one this came from.
    pub origin: String,
    /// The theme's file when it came from a theme, for an editor that
    /// wants to say where to go and change it.
    pub theme: Option<String>,
    /// `lcd`, `keys` or `both`.
    pub span: String,
    pub image: Option<String>,
    pub animation: Option<String>,
    /// As configured, which may be `@tokens`.
    pub colors: Vec<String>,
    /// The same, resolved, for colour pickers.
    pub colors_hex: Vec<String>,
    pub fps: u8,
    pub speed: f32,
    pub dim: f32,
}

/// The info screen's layout grid.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LcdGrid {
    pub columns: u8,
    pub rows: u8,
    /// The screen's size in pixels.
    pub width: u16,
    pub height: u16,
}

/// One widget on the info screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileInfo {
    /// Index within the page's `lcd` array, for building a patch path.
    pub index: usize,
    pub column: u8,
    pub row: u8,
    pub columns: u8,
    pub rows: u8,
    pub widget: WidgetInfo,
    /// What the widget last read, as text, when it has read anything.
    pub text: Option<String>,
}

/// One configured key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyInfo {
    /// Position on the 3x4 grid, 0-11.
    pub key: u8,
    /// Index within the page's `keys` array, for building a patch path.
    pub index: usize,
    /// The label as configured, which is what an editor edits.
    pub label: Option<String>,
    /// What the key is showing right now. Differs from `label` when a widget
    /// has produced text, so an editor can show both without guessing.
    pub text: Option<String>,
    /// The widget on this key, if any, with everything an editor needs to
    /// show its current settings rather than guess at them.
    pub widget: Option<WidgetInfo>,
    /// What tapping, holding and double-tapping do. `tap` includes a
    /// widget's own tap, and a key with states moving on to the next (kind
    /// `implicit`), when nothing else is bound.
    #[serde(default)]
    pub tap: Option<ActionInfo>,
    #[serde(default)]
    pub hold: Option<ActionInfo>,
    #[serde(default)]
    pub double: Option<ActionInfo>,
    pub animation: Option<AnimationInfo>,
    pub icon: Option<String>,
    pub exec: Option<String>,
    pub page: Option<String>,
    pub profile: Option<String>,
    pub back: bool,
    /// The background this key resolves to, after the whole cascade.
    pub background: String,
    /// Whether the background came from this key rather than a theme above it.
    pub background_is_own: bool,
    /// The label colour this key resolves to, after the whole cascade; empty
    /// from a daemon that does not say.
    #[serde(default)]
    pub label_color: String,
    /// Whether the label colour came from this key rather than a theme above
    /// it.
    #[serde(default)]
    pub label_color_is_own: bool,
    /// Where the key's timer or stopwatch is, when it has one.
    #[serde(default)]
    pub timer: Option<TimerInfo>,
    /// The states the key steps through, as configured; empty for a key
    /// without them. `label`, `icon` and `background` above stay the key's
    /// own, which is what an editor writes back to it.
    #[serde(default)]
    pub states: Vec<KeyStateInfo>,
    /// The state the key is showing, by name. `None` while it has states but
    /// has not yet read which one it is in, when it shows its own look.
    #[serde(default)]
    pub state: Option<String>,
    /// Whether `state` came from reading it with `status`, rather than from
    /// what the key last did.
    #[serde(default)]
    pub state_known: bool,
    /// The command that reads which state the key is in, as written.
    #[serde(default)]
    pub status: Option<String>,
    /// How often it is read, as written; `None` for the default.
    #[serde(default)]
    pub status_interval_ms: Option<u32>,
    /// What the last read found, when there has been one.
    #[serde(default)]
    pub status_result: Option<StatusResultInfo>,
}

/// One of a key's states, as configured.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyStateInfo {
    pub name: String,
    /// What `status` prints in this state, as written; empty when it is the
    /// name. Spelt as in config, so an editor builds its patch path from it.
    #[serde(rename = "match")]
    pub matches: Vec<String>,
    pub label: Option<String>,
    pub icon: Option<String>,
    /// What runs as the key enters this state.
    pub exec: Option<ActionInfo>,
    /// The state's own background and label colour, resolved through the
    /// palette, when it sets them: what an editor saving this state starts
    /// from, so the key's colour is never copied into it.
    pub background: Option<String>,
    pub label_color: Option<String>,
    pub animation: Option<AnimationInfo>,
}

/// What reading a key's state with `status` last found.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StatusResultInfo {
    /// The line it printed, made safe to show.
    pub output: Option<String>,
    /// Why it failed -- a non-zero exit, a timeout, no output -- made safe
    /// to show. `None` when it ran.
    pub error: Option<String>,
    /// Whether what it printed is one of the key's states.
    pub ok: bool,
    /// How long ago it was read.
    pub age_ms: u64,
}

/// A timer's or a stopwatch's count.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TimerInfo {
    /// `stopped`, `running`, `paused` or `done`.
    pub state: String,
    /// What is left of a timer; `None` for a stopwatch.
    pub remaining_ms: Option<u64>,
    /// How long it has run, not counting pauses.
    pub elapsed_ms: u64,
}

/// A widget's settings, as configured.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetInfo {
    pub kind: String,
    /// The interval actually in use, after clamping.
    pub interval_ms: u32,
    pub format: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    pub command: Option<String>,
    pub placeholder: Option<String>,
    /// `text`, `graph` or `bar`, after defaulting.
    #[serde(default)]
    pub view: String,
    /// The view the widget sets itself, if it does. `view` may instead come
    /// from its look, and an editor must not write that one back into it.
    #[serde(default)]
    pub own_view: Option<String>,
    /// The view its page, profile or theme gives widgets like it, when that
    /// is one it can be drawn as.
    #[serde(default)]
    pub look_view: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    /// `celsius` or `fahrenheit`, when set.
    #[serde(default)]
    pub units: Option<String>,
    #[serde(default)]
    pub place: Option<String>,
    #[serde(default)]
    pub warn: Option<f64>,
    #[serde(default)]
    pub critical: Option<f64>,
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub history: Option<u16>,
    /// The colour as configured, which may be a `@token`.
    #[serde(default)]
    pub color: Option<String>,
    /// That colour resolved through the palette, for a colour picker.
    #[serde(default)]
    pub color_hex: Option<String>,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub background_hex: Option<String>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub image: Option<String>,
    /// For a timer: its length, as written.
    #[serde(default)]
    pub duration: Option<String>,
    /// For a timer: what it does when it finishes.
    #[serde(default)]
    pub on_done: Option<ActionInfo>,
}

/// Things a widget's `source` could name, per widget kind.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WidgetSources {
    pub temperature: Vec<SourceOption>,
    pub gpu: Vec<SourceOption>,
    pub network: Vec<SourceOption>,
    pub disk: Vec<SourceOption>,
    pub media: Vec<SourceOption>,
    #[serde(default)]
    pub battery: Vec<SourceOption>,
    #[serde(default)]
    pub fan: Vec<SourceOption>,
    #[serde(default)]
    pub volume: Vec<SourceOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceOption {
    /// What to write in `source`.
    pub value: String,
    /// Something to recognise it by: a reading, a size, what is playing.
    pub detail: Option<String>,
}

/// An animation's settings, as configured.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationInfo {
    pub kind: String,
    /// The period actually in use, after clamping.
    pub period_ms: u32,
    /// The colour it moves towards, resolved through the palette.
    pub to: Option<String>,
    pub frames: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderInfo {
    pub encoder: u8,
    /// Index within the page's own `encoders` array, when the page has an
    /// entry for this knob; `None` when everything comes from further up.
    pub index: Option<usize>,
    /// The page entry's shell commands, as written, for older editors.
    pub press: Option<String>,
    pub cw: Option<String>,
    pub ccw: Option<String>,
    pub ring: String,
    pub ring_is_own: bool,
    pub animation: Option<AnimationInfo>,
    /// What each gesture does once the global, profile and page layers are
    /// folded together, each saying which layer it came from.
    #[serde(default)]
    pub resolved: GestureInfo,
    /// The preset the turn came from, and what the ring shows while turning
    /// (`output_level`, `input_level`, `deck_brightness`, `page_position`,
    /// `profile_position`, `output_position`, `app_level`).
    #[serde(default)]
    pub turn_preset: Option<String>,
    #[serde(default)]
    pub ring_shows: Option<String>,
    /// Each layer's own entry for this knob, as written, for editing it where
    /// it lives. Empty for a knob set nowhere, which is still listed so an
    /// editor shows its real colour.
    #[serde(default)]
    pub layers: Vec<EncoderLayerInfo>,
    /// The ring colour before any knob layer: the theme, profile and page
    /// styles. What a layer with no colour of its own inherits, if no layer
    /// under it sets one.
    #[serde(default)]
    pub base_ring: Option<String>,
    /// The modes the knob switches between when held, once the layers are
    /// folded; empty without them. Each one's `ring` is where the ring
    /// rests while it is on, resolved.
    #[serde(default)]
    pub modes: Vec<ModeInfo>,
    /// Which of them is on.
    #[serde(default)]
    pub mode: Option<usize>,
}

/// One of a knob's modes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ModeInfo {
    /// The preset's name, as in config.
    pub preset: String,
    /// Its name for a person: "App volume".
    pub title: String,
    pub step: Option<f64>,
    pub target: Option<String>,
    /// A ring colour, `#rrggbb`.
    pub ring: Option<String>,
}

/// One layer's entry for a knob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderLayerInfo {
    /// `global`, `profile` or `page`.
    pub layer: String,
    /// The file it is in, and its path there, for building patches.
    pub file: String,
    pub path: String,
    pub preset: Option<String>,
    pub step: Option<f64>,
    /// This layer's own modes, as written, with each one's own ring colour
    /// resolved when it sets one.
    #[serde(default)]
    pub modes: Vec<ModeInfo>,
    #[serde(default)]
    pub target: Option<String>,
    pub gestures: GestureInfo,
    /// This layer's own ring colour and animation, resolved, if it sets
    /// them: what an editor saving this layer starts from, so an inherited
    /// value is never copied into it.
    #[serde(default)]
    pub ring: Option<String>,
    #[serde(default)]
    pub animation: Option<AnimationInfo>,
}

/// The gestures of a knob or key.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GestureInfo {
    pub press: Option<ActionInfo>,
    pub cw: Option<ActionInfo>,
    pub ccw: Option<ActionInfo>,
    pub hold: Option<ActionInfo>,
}

/// An action, taken apart for an editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionInfo {
    /// `shell`, `builtin`, `keys`, or `implicit` for a tap nothing is bound
    /// to: a widget's own, or a key with states moving on to the next.
    pub kind: String,
    pub command: Option<String>,
    pub action: Option<String>,
    pub keys: Option<String>,
    pub step: Option<f64>,
    pub target: Option<String>,
    /// Words for a person: "volume up", "press ctrl+t".
    pub label: String,
    /// Which layer it came from, for a knob: `global`, `profile`, `page`.
    pub origin: Option<String>,
}

/// Everything the model knows how to do, for an editor's pickers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub built_ins: Vec<BuiltInInfo>,
    pub presets: Vec<PresetInfo>,
    pub keys: Vec<KeyNameInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltInInfo {
    pub name: String,
    pub group: String,
    pub label: String,
    /// `percent`, `seconds` or `notches`, with the default and range.
    pub step_unit: Option<String>,
    pub step_default: Option<f64>,
    pub step_min: Option<f64>,
    pub step_max: Option<f64>,
    pub takes_target: bool,
    pub needs_virtual_input: bool,
    /// Keys only: push-to-talk has to see the key come back up, and the
    /// timer and state built-ins act on the key they are bound to.
    pub keys_only: bool,
    /// Knobs only: they act on the knob they are bound to.
    #[serde(default)]
    pub knobs_only: bool,
    /// What `target` names for it, when it takes one: `audio_node`,
    /// `player`, `app` or `sink`.
    #[serde(default)]
    pub target_kind: Option<String>,
    /// Whether it switches outputs or sets an app's volume, which needs
    /// what `Capabilities::mixer` reports on.
    #[serde(default)]
    pub needs_mixer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetInfo {
    pub name: String,
    /// Its name for a person: "Output switcher".
    #[serde(default)]
    pub title: String,
    pub label: String,
    pub press: Option<String>,
    pub cw: Option<String>,
    pub ccw: Option<String>,
    pub ring_shows: Option<String>,
    pub needs_virtual_input: bool,
    /// What a knob's `target` names with this preset, as for a built-in;
    /// `None` when the preset takes no target.
    #[serde(default)]
    pub target_kind: Option<String>,
    /// Whether any of its gestures needs what `Capabilities::mixer` reports
    /// on.
    #[serde(default)]
    pub needs_mixer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyNameInfo {
    pub name: String,
    pub group: String,
}

/// What this machine lets the daemon do, so an editor can say why an action
/// would do nothing before anyone binds it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    /// `ready`, `waiting` (created, settling), `unused` (nothing needs it
    /// yet), `off` (virtual_input = false) or `unavailable: <why>`.
    pub virtual_input: String,
    /// `wpctl`, `pactl` or `missing`.
    pub audio: String,
    /// `ok` or `unavailable: <why>`.
    pub media: String,
    /// Output switching and per-app volume: `ok`, or `unavailable: <why>`.
    /// They need wpctl and pw-dump.
    #[serde(default)]
    pub mixer: String,
    /// The desktop the daemon runs under, as `XDG_CURRENT_DESKTOP` says,
    /// such as `ubuntu:GNOME`; empty when it does not say. For an editor to
    /// mark what works only on one desktop.
    #[serde(default)]
    pub desktop: String,
}

/// A place found by name, for a weather widget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceInfo {
    pub label: String,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// Something that happened, for clients that asked to be told.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    DeviceConnected {
        firmware: String,
        serial: String,
    },
    DeviceDisconnected,
    PageChanged {
        profile: String,
        page: String,
    },
    ProfileChanged {
        profile: String,
    },
    BrightnessChanged {
        percent: u8,
    },
    KeyPressed {
        key: u8,
    },
    EncoderTurned {
        encoder: u8,
        delta: i8,
    },
    EncoderPressed {
        encoder: u8,
    },
    ConfigChanged,
    /// The device was handed to another process, or taken back.
    DeviceReleased,
    DeviceResumed,
    /// The calibration changed: re-fetch it.
    CalibrationChanged,
    /// A timer finished, on whichever page it is.
    TimerDone {
        profile: String,
        page: String,
        key: u8,
    },
    /// A knob was switched to another of its modes.
    ModeChanged {
        encoder: u8,
        mode: usize,
    },
    /// A key with states changed state, on whichever page it is: because it
    /// was pressed, or because reading it found it had changed.
    KeyStateChanged {
        profile: String,
        page: String,
        key: u8,
        /// `None` when it is not known which state it is in.
        state: Option<String>,
        /// Whether the state was read with `status`, rather than taken from
        /// what the key last did.
        known: bool,
    },
}

/// Path of the daemon's control socket.
///
/// `$GALDECK_SOCKET` if set, else `$XDG_RUNTIME_DIR/galdeck.sock`, else a
/// per-user directory under /tmp.
///
/// The fallback keys on the numeric uid rather than `$USER`. `$USER` is
/// attacker-controlled in the general case and simply absent in some service
/// managers, where it previously collapsed to a single shared
/// `/tmp/galdeck-unknown.sock` in a world-writable directory. Two accounts
/// would then race for one path, and this socket can define what commands the
/// daemon runs.
pub fn socket_path() -> PathBuf {
    // An explicit override, so a development build can run alongside the
    // installed service instead of fighting it for one well-known path.
    if let Ok(path) = std::env::var("GALDECK_SOCKET") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join("galdeck.sock");
        }
    }
    fallback_dir().join("galdeck.sock")
}

/// The `/tmp` fallback directory for this user.
pub fn fallback_dir() -> PathBuf {
    // Safety: getuid cannot fail and touches no memory.
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/galdeck-{uid}"))
}
