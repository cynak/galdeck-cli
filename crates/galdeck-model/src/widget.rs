//! Widgets: keys and screen tiles that show something that changes.
//!
//! A widget samples something on a schedule and the daemon draws the result.
//! Most kinds produce a number with a line of text to go with it, and `view`
//! decides whether that shows as the text alone, as a graph of recent history,
//! or as a bar. Two kinds — `weather` and `media` — produce a small record
//! rather than a number, and have a card layout of their own.
//!
//! Everything about colour still belongs to the theme cascade. A widget may
//! name the one accent its graph is drawn in, and nothing else, so there is
//! still only one styling system.
//!
//! Two more — `timer` and `stopwatch` — sample nothing: they count time from
//! a tap on their key, and the daemon keeps the count.
//!
//! Cheap widgets are sampled on the thread that owns the daemon's state.
//! Anything that could block — a shell command, the network, D-Bus — is not,
//! and the distinction is [`WidgetKind::is_blocking`], because getting it
//! wrong stalls every key on the deck.

use std::time::Duration;

use serde::Deserialize;

use crate::action::Action;
use crate::color::ColorRef;

/// Fastest a widget may refresh.
///
/// A key repaint is a JPEG encode; ten a second, per key, is already more than
/// anything worth displaying needs.
pub const MIN_INTERVAL_MS: u32 = 100;
/// Slowest before it is not really a widget.
pub const MAX_INTERVAL_MS: u32 = 24 * 60 * 60 * 1000;
/// Fastest a network-backed widget may refresh.
///
/// Weather comes from someone else's free service. Asking it more than once a
/// minute gains nothing — forecasts do not change that fast — and is the kind
/// of thing that gets a client blocked.
pub const MIN_NETWORK_INTERVAL_MS: u32 = 60_000;
/// How much of a command's output is kept.
///
/// A key shows a handful of characters, and a command that prints a megabyte
/// should not cost a megabyte of memory per refresh.
pub const MAX_OUTPUT_BYTES: usize = 4096;
/// Most samples a graph may keep.
pub const MAX_HISTORY: u16 = 600;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WidgetKind {
    /// The time of day.
    Clock,
    /// The date.
    Date,
    /// Total CPU use across all cores, as a percentage.
    Cpu,
    /// Memory in use, as a percentage.
    Memory,
    /// A hardware temperature sensor, from `/sys/class/hwmon`.
    Temperature,
    /// GPU utilisation, as a percentage.
    Gpu,
    /// Network throughput, received and sent.
    Network,
    /// Space used on a filesystem, as a percentage.
    Disk,
    /// Current conditions and a short forecast.
    Weather,
    /// What a media player is playing, over MPRIS.
    Media,
    /// The first line of a shell command's output.
    Command,
    /// A battery's charge, as a percentage.
    Battery,
    /// A fan's speed, in RPM, from `/sys/class/hwmon`.
    Fan,
    /// The one-minute load average.
    Load,
    /// How long since the machine started.
    Uptime,
    /// The default audio output's volume, or an input's, as a percentage.
    Volume,
    /// Counts down from `duration`. Tap to start, pause and resume; hold to
    /// reset. Keys only.
    Timer,
    /// Counts up. Tap to start, pause and resume; hold to reset. Keys only.
    Stopwatch,
}

impl WidgetKind {
    /// Whether sampling this could block.
    ///
    /// A blocking sample runs on a worker, because doing it on the thread that
    /// owns the deck's state would stall every other key while some script,
    /// server or player decides to answer. GPU counts: without a sysfs counter
    /// the only way to ask an NVIDIA card is to run `nvidia-smi`.
    pub fn is_blocking(self) -> bool {
        matches!(
            self,
            WidgetKind::Command
                | WidgetKind::Weather
                | WidgetKind::Media
                | WidgetKind::Gpu
                // Asked of the sound server by running `wpctl`.
                | WidgetKind::Volume
        )
    }

    /// Whether this kind produces a number that can be graphed.
    pub fn is_numeric(self) -> bool {
        matches!(
            self,
            WidgetKind::Cpu
                | WidgetKind::Memory
                | WidgetKind::Temperature
                | WidgetKind::Gpu
                | WidgetKind::Network
                | WidgetKind::Disk
                | WidgetKind::Command
                | WidgetKind::Battery
                | WidgetKind::Fan
                | WidgetKind::Load
                | WidgetKind::Volume
        )
    }

    /// Whether this kind counts time from a tap on its key, which is what
    /// starts, pauses and resets it.
    pub fn is_timer(self) -> bool {
        matches!(self, WidgetKind::Timer | WidgetKind::Stopwatch)
    }

    /// Whether this kind tells the time, and so can be a clock face and can
    /// be in another time zone.
    pub fn is_time(self) -> bool {
        matches!(self, WidgetKind::Clock | WidgetKind::Date)
    }

    /// Whether `source` means anything to this kind.
    pub fn takes_source(self) -> bool {
        matches!(
            self,
            WidgetKind::Temperature
                | WidgetKind::Gpu
                | WidgetKind::Network
                | WidgetKind::Disk
                | WidgetKind::Media
                | WidgetKind::Battery
                | WidgetKind::Fan
                | WidgetKind::Volume
        )
    }

    /// A sensible refresh rate for this kind.
    pub fn default_interval_ms(self) -> u32 {
        match self {
            // A minute would drift visibly against the wall clock.
            WidgetKind::Clock => 1_000,
            WidgetKind::Date => 60_000,
            WidgetKind::Cpu
            | WidgetKind::Memory
            | WidgetKind::Temperature
            | WidgetKind::Gpu
            | WidgetKind::Network => 2_000,
            WidgetKind::Disk => 60_000,
            WidgetKind::Weather => 15 * 60_000,
            // A progress bar that moves once a second looks alive; anything
            // slower looks stuck.
            WidgetKind::Media => 1_000,
            // Someone else's script; do not run it more than necessary.
            WidgetKind::Command => 5_000,
            WidgetKind::Battery => 30_000,
            WidgetKind::Fan => 2_000,
            WidgetKind::Load => 5_000,
            WidgetKind::Uptime => 60_000,
            // A knob turned beside it should show at once.
            WidgetKind::Volume => 500,
            // Nothing to sample: the daemon repaints them as the count moves.
            WidgetKind::Timer | WidgetKind::Stopwatch => 1_000,
        }
    }

    /// The name used in config, for messages.
    pub fn name(self) -> &'static str {
        match self {
            WidgetKind::Clock => "clock",
            WidgetKind::Date => "date",
            WidgetKind::Cpu => "cpu",
            WidgetKind::Memory => "memory",
            WidgetKind::Temperature => "temperature",
            WidgetKind::Gpu => "gpu",
            WidgetKind::Network => "network",
            WidgetKind::Disk => "disk",
            WidgetKind::Weather => "weather",
            WidgetKind::Media => "media",
            WidgetKind::Command => "command",
            WidgetKind::Battery => "battery",
            WidgetKind::Fan => "fan",
            WidgetKind::Load => "load",
            WidgetKind::Uptime => "uptime",
            WidgetKind::Volume => "volume",
            WidgetKind::Timer => "timer",
            WidgetKind::Stopwatch => "stopwatch",
        }
    }
}

/// How a widget's reading is drawn.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WidgetView {
    /// The text alone, in place of the label.
    #[default]
    Text,
    /// A filled graph of recent readings, with the latest as text over it.
    Graph,
    /// A bar filled to the current reading.
    Bar,
    /// A dial: an arc filled to the current reading, with it written inside.
    Gauge,
    /// For `clock`: a clock face with hands.
    Analog,
    /// For `clock`: a glowing nixie tube for each digit of `format` --
    /// six, hours to seconds, unless it says otherwise -- with dust drifting
    /// past and the odd tube flickering. Drawn in warm neon unless `color`
    /// says otherwise, on a dark panel unless `background` does.
    Nixie,
}

impl WidgetView {
    /// Whether this view moves between readings, and so is redrawn on every
    /// refresh even when the reading has not changed.
    pub fn is_animated(self) -> bool {
        self == WidgetView::Nixie
    }

    /// Whether this view can draw that kind.
    ///
    /// A timer fills a bar or a gauge as it runs down, but a graph of it
    /// would be a straight line; a stopwatch has no end to fill towards.
    /// Neither counts as numeric, which would also let them be graphed and
    /// warned about.
    pub fn suits(self, kind: WidgetKind) -> bool {
        match (self, kind) {
            (WidgetView::Text, _) => true,
            (WidgetView::Bar | WidgetView::Gauge, WidgetKind::Timer) => true,
            (_, WidgetKind::Timer | WidgetKind::Stopwatch) => false,
            (WidgetView::Graph | WidgetView::Bar | WidgetView::Gauge, _) => kind.is_numeric(),
            (WidgetView::Analog | WidgetView::Nixie, _) => kind == WidgetKind::Clock,
        }
    }
}

/// How alarming a reading is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    #[default]
    Normal,
    Warn,
    Critical,
}

/// Temperature units, for `temperature` and `weather`.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    #[default]
    Celsius,
    Fahrenheit,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Widget {
    pub kind: WidgetKind,
    /// How often to refresh. Defaults to something sensible per kind.
    #[serde(default)]
    pub interval_ms: Option<u32>,
    /// How to draw it. Weather and media ignore this and draw a card.
    #[serde(default)]
    pub view: Option<WidgetView>,
    /// A caption drawn above a graph or bar, such as "CPU".
    #[serde(default)]
    pub title: Option<String>,
    /// For `clock` and `date`: a strftime-style format. A nixie clock gives
    /// each digit a tube and each `:` a pair of lamps, and leaves a tube dark
    /// for the padding `%k` and `%l` put before a single digit.
    #[serde(default)]
    pub format: Option<String>,
    /// For `clock` and `date`: an IANA time zone such as `Asia/Tokyo`, for a
    /// world clock. Defaults to the machine's own.
    #[serde(default)]
    pub timezone: Option<String>,
    /// For `command`: what to run, with `sh -c`. If the first line starts
    /// with a number, that number is what a graph or bar shows.
    #[serde(default)]
    pub command: Option<String>,
    /// Which one, where there is a choice:
    ///
    /// - `temperature`: a hwmon chip or sensor label, such as `k10temp`,
    ///   `coretemp`, `amdgpu`, `nvme` or `Tctl`. Defaults to the CPU.
    /// - `gpu`: a DRM card such as `card1`. Defaults to the first that
    ///   reports utilisation, then to `nvidia-smi`.
    /// - `network`: an interface such as `eth0`. Defaults to all but `lo`.
    /// - `disk`: a mount point. Defaults to `/`.
    /// - `media`: an MPRIS player such as `spotify`. Defaults to whichever
    ///   is playing.
    /// - `battery`: a power supply such as `BAT0`. Defaults to the first.
    /// - `fan`: a hwmon chip or fan label. Defaults to the first fan.
    /// - `volume`: a PipeWire node id or name, or `mic` for the default
    ///   input. Defaults to the default output.
    #[serde(default)]
    pub source: Option<String>,
    /// For `temperature` and `weather`.
    #[serde(default)]
    pub units: Option<Units>,
    /// For `weather`: the place's name, as chosen in a search, for the card
    /// to show. Only a label; `latitude` and `longitude` are what is asked.
    #[serde(default)]
    pub place: Option<String>,
    /// Where a reading turns amber and red. A widget whose `warn` is above
    /// its `critical` counts down (a battery running low); otherwise up (a
    /// temperature climbing). In the widget's own units.
    #[serde(default)]
    pub warn: Option<f64>,
    #[serde(default)]
    pub critical: Option<f64>,
    /// For `weather`: where. Both are needed; nothing is looked up without
    /// them, so the daemon never guesses at where you are.
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    /// Where a graph or bar is full. Defaults to 100 for percentages, and to
    /// the largest reading seen for everything else.
    #[serde(default)]
    pub max: Option<f64>,
    /// How many readings a graph keeps. Defaults to enough to fill it.
    #[serde(default)]
    pub history: Option<u16>,
    /// The colour a graph or bar is drawn in. Defaults to the theme's label
    /// colour.
    #[serde(default)]
    pub color: Option<ColorRef>,
    /// What the widget sits on. On a key this replaces the key's background;
    /// on the screen, the card behind the tile.
    #[serde(default)]
    pub background: Option<ColorRef>,
    /// How opaque that background is, 0 to 1. Zero lets whatever is behind
    /// show through -- the page's background image, say.
    #[serde(default)]
    pub opacity: Option<f32>,
    /// A picture behind the widget, scaled to cover it.
    #[serde(default)]
    pub image: Option<std::path::PathBuf>,
    /// Text shown before the widget has produced anything, and whenever it
    /// fails. Falls back to the key's label.
    #[serde(default)]
    pub placeholder: Option<String>,
    /// For `timer`: how long it counts down, such as `25m`, `1h 30m`, `90s`,
    /// or `4:30` (minutes and seconds) and `1:30:00` (hours too). Kept as
    /// written; [`Widget::duration`] reads it.
    #[serde(default)]
    pub duration: Option<String>,
    /// For `timer`: what to do, once, when it finishes -- play a sound, say.
    #[serde(default)]
    pub on_done: Option<Action>,
}

/// The units a `duration` may use, in the order they must come, and how many
/// seconds each is.
const SECONDS_PER: [(char, u64); 3] = [('h', 3600), ('m', 60), ('s', 1)];

/// Read a duration: `90s`, `4m`, `1h30m`, `1h 30m`, or the colon forms
/// `m:ss` and `h:mm:ss`.
///
/// Only those. `1:30` could mean ninety minutes to someone, so the colon
/// forms are fixed rather than guessed at, and a bare number has no unit.
/// Arithmetic is checked, so an absurd length is no duration rather than a
/// panic.
pub fn parse_duration(text: &str) -> Option<Duration> {
    let text = text.trim();
    let digits = |part: &str| -> Option<u64> {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    let seconds = if text.contains(':') {
        let parts: Vec<&str> = text.split(':').collect();
        // Minutes and seconds after the first part are two digits, as a
        // clock writes them, and below sixty.
        let sixtieth = |part: &str| digits(part).filter(|n| part.len() == 2 && *n < 60);
        match parts[..] {
            [minutes, seconds] => digits(minutes)?
                .checked_mul(60)?
                .checked_add(sixtieth(seconds)?)?,
            [hours, minutes, seconds] => digits(hours)?
                .checked_mul(3600)?
                .checked_add(sixtieth(minutes)? * 60)?
                .checked_add(sixtieth(seconds)?)?,
            _ => return None,
        }
    } else {
        // Hours, minutes and seconds, each at most once and in that order,
        // with or without spaces between them.
        if text.is_empty() {
            return None;
        }
        let mut rest = text;
        let mut total: u64 = 0;
        let mut units = SECONDS_PER.iter();
        while !rest.is_empty() {
            let end = rest.find(|c: char| !c.is_ascii_digit())?;
            let (number, after) = rest.split_at(end);
            let unit = after.chars().next()?;
            let (_, per) = units.by_ref().find(|(name, _)| *name == unit)?;
            total = total.checked_add(digits(number)?.checked_mul(*per)?)?;
            rest = after[unit.len_utf8()..].trim_start();
        }
        total
    };
    Some(Duration::from_secs(seconds))
}

impl Widget {
    /// A widget of this kind with every setting at its default.
    pub fn of(kind: WidgetKind) -> Self {
        Self {
            kind,
            interval_ms: None,
            view: None,
            title: None,
            format: None,
            timezone: None,
            command: None,
            source: None,
            units: None,
            place: None,
            warn: None,
            critical: None,
            latitude: None,
            longitude: None,
            max: None,
            history: None,
            color: None,
            background: None,
            opacity: None,
            image: None,
            placeholder: None,
            duration: None,
            on_done: None,
        }
    }

    /// How long a timer counts down, if `duration` says, in a form
    /// [`parse_duration`] reads.
    pub fn duration(&self) -> Option<Duration> {
        parse_duration(self.duration.as_deref()?)
    }

    pub fn interval_ms(&self) -> u32 {
        let floor = if self.kind == WidgetKind::Weather {
            MIN_NETWORK_INTERVAL_MS
        } else {
            MIN_INTERVAL_MS
        };
        // An animated view refreshes at its frame rate, not its data's.
        let default = if self.view().is_animated() {
            MIN_INTERVAL_MS
        } else {
            self.kind.default_interval_ms()
        };
        self.interval_ms
            .unwrap_or(default)
            .clamp(floor, MAX_INTERVAL_MS)
    }

    /// The format string to use, with a default per kind.
    pub fn format(&self) -> &str {
        self.format.as_deref().unwrap_or(match self.kind {
            WidgetKind::Date => "%a %d %b",
            // Six tubes: a nixie clock counts the seconds.
            _ if self.view() == WidgetView::Nixie => "%H:%M:%S",
            _ => "%H:%M",
        })
    }

    pub fn view(&self) -> WidgetView {
        self.view.unwrap_or_default()
    }

    pub fn units(&self) -> Units {
        self.units.unwrap_or_default()
    }

    /// Readings a graph keeps, if nobody said: enough for a line a pixel
    /// or two apart across a key, without holding on to minutes of history
    /// nobody can see.
    pub fn history(&self) -> usize {
        usize::from(self.history.unwrap_or(60).clamp(2, MAX_HISTORY))
    }

    /// Where a graph or bar is full, if that is known in advance.
    ///
    /// `None` means scale to what has been seen, which is the only honest
    /// choice for throughput and for someone else's command.
    pub fn fixed_max(&self) -> Option<f64> {
        if let Some(max) = self.max {
            return (max > 0.0).then_some(max);
        }
        match self.kind {
            WidgetKind::Cpu | WidgetKind::Memory | WidgetKind::Gpu | WidgetKind::Disk => {
                Some(100.0)
            }
            WidgetKind::Temperature => Some(match self.units() {
                Units::Celsius => 100.0,
                Units::Fahrenheit => 212.0,
            }),
            WidgetKind::Battery | WidgetKind::Volume => Some(100.0),
            // Full at the start, empty when it is done.
            WidgetKind::Timer => self
                .duration()
                .map(|length| length.as_secs_f64())
                .filter(|&seconds| seconds > 0.0),
            _ => None,
        }
    }

    /// The background's opacity, 0 to 1. Opaque unless said otherwise.
    pub fn opacity(&self) -> f32 {
        self.opacity.unwrap_or(1.0).clamp(0.0, 1.0)
    }

    /// The zone to tell the time in, if not the machine's own.
    pub fn timezone(&self) -> Option<&str> {
        self.timezone.as_deref().filter(|zone| !zone.is_empty())
    }

    /// Whether readings get worse as they fall rather than as they rise.
    ///
    /// Said by the thresholds when both are given; otherwise a battery counts
    /// down and everything else up.
    pub fn counts_down(&self) -> bool {
        match (self.warn, self.critical) {
            (Some(warn), Some(critical)) => warn > critical,
            _ => self.kind == WidgetKind::Battery,
        }
    }

    /// How alarming `value` is, given how alarming the last one was.
    ///
    /// Leaving a level takes going back past its threshold by 2% of `scale`,
    /// so a reading hovering at the line does not flicker between colours.
    pub fn level(&self, value: f64, previous: Level, scale: f64) -> Level {
        let margin = scale.abs() * 0.02;
        let down = self.counts_down();
        let past = |threshold: f64, holding: bool| {
            let slack = if holding { margin } else { 0.0 };
            if down {
                value <= threshold + slack
            } else {
                value >= threshold - slack
            }
        };
        if self
            .critical
            .is_some_and(|c| past(c, previous == Level::Critical))
        {
            Level::Critical
        } else if self.warn.is_some_and(|w| past(w, previous >= Level::Warn)) {
            Level::Warn
        } else {
            Level::Normal
        }
    }

    /// Whether this draws as a card or graph rather than as a label.
    pub fn is_graphic(&self) -> bool {
        matches!(self.kind, WidgetKind::Weather | WidgetKind::Media)
            || self.view() != WidgetView::Text
    }
}

/// The info screen is laid out on a grid of this many columns...
pub const LCD_COLUMNS: u8 = 12;
/// ...and this many rows, unless a page or profile says otherwise. On the
/// 720x384 panel that is 60x64 pixel cells: fine enough to line things up,
/// coarse enough to not need a ruler.
pub const LCD_ROWS: u8 = 6;
/// Most columns a grid may have. At 24 a cell is 30 pixels wide, about the
/// narrowest anything is still legible in.
pub const MAX_LCD_COLUMNS: u8 = 24;
/// Most rows: 32 pixels each.
pub const MAX_LCD_ROWS: u8 = 12;

/// The grid the info screen is laid out on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LcdGrid {
    pub columns: u8,
    pub rows: u8,
}

impl Default for LcdGrid {
    fn default() -> Self {
        Self {
            columns: LCD_COLUMNS,
            rows: LCD_ROWS,
        }
    }
}

impl LcdGrid {
    /// A grid from what a page or profile asked for, each clamped to what
    /// is legible and defaulted where not given.
    pub fn new(columns: Option<u8>, rows: Option<u8>) -> Self {
        Self {
            columns: columns.unwrap_or(LCD_COLUMNS).clamp(1, MAX_LCD_COLUMNS),
            rows: rows.unwrap_or(LCD_ROWS).clamp(1, MAX_LCD_ROWS),
        }
    }
}

/// A widget placed on the info screen.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LcdTile {
    /// Left edge, in grid columns from 0.
    pub column: u8,
    /// Top edge, in grid rows from 0.
    pub row: u8,
    /// Width in columns. Defaults to the rest of the row.
    #[serde(default)]
    pub columns: Option<u8>,
    /// Height in rows. Defaults to the rest of the screen.
    #[serde(default)]
    pub rows: Option<u8>,
    pub widget: Widget,
}

/// Where a tile is on its grid, with the spans worked out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cells {
    pub column: u8,
    pub row: u8,
    pub columns: u8,
    pub rows: u8,
}

impl Cells {
    /// Whether two rectangles cover any of the same cells.
    pub fn overlaps(&self, other: &Cells) -> bool {
        self.column < other.column + other.columns
            && other.column < self.column + self.columns
            && self.row < other.row + other.rows
            && other.row < self.row + self.rows
    }
}

impl LcdTile {
    /// Where the tile is on `grid`. A span left out runs to the edge.
    pub fn cells(&self, grid: LcdGrid) -> Cells {
        Cells {
            column: self.column,
            row: self.row,
            columns: self
                .columns
                .unwrap_or(grid.columns.saturating_sub(self.column)),
            rows: self.rows.unwrap_or(grid.rows.saturating_sub(self.row)),
        }
    }

    /// Whether the tile lies entirely on `grid`.
    pub fn fits(&self, grid: LcdGrid) -> bool {
        let cells = self.cells(grid);
        cells.columns > 0
            && cells.rows > 0
            && u16::from(cells.column) + u16::from(cells.columns) <= u16::from(grid.columns)
            && u16::from(cells.row) + u16::from(cells.rows) <= u16::from(grid.rows)
    }
}
