//! How widgets look, as a theme, a profile or a page says they should.
//!
//! A widget can say how it is drawn -- its view, its colours, the style of
//! its graph, bar or dial -- and whatever it leaves unsaid comes from here:
//! `[widgets]` in its page, then in its profile, then in its theme and the
//! themes that one extends. Within each of those, `[widgets.<kind>]` goes
//! over `[widgets]`, so a theme can make every clock a nixie clock and every
//! graph a line without saying so on each widget.
//!
//! These are defaults and never settings: a widget's own value always wins,
//! and saving a widget writes only what the widget itself says.

use serde::{Deserialize, Deserializer};

use crate::color::ColorRef;
use crate::diag::{Diagnostic, Diagnostics};
use crate::widget::{WidgetKind, WidgetView};

/// Fewest and most steps a segmented bar may have.
pub const MIN_SEGMENTS: u8 = 2;
pub const MAX_SEGMENTS: u8 = 40;
pub const DEFAULT_SEGMENTS: u8 = 10;
/// How far round a dial may go, in degrees. Less than a quarter turn does
/// not read as a dial.
pub const MIN_SWEEP: u16 = 90;
pub const MAX_SWEEP: u16 = 360;
/// Open at the bottom, as a dial has always been drawn.
pub const DEFAULT_SWEEP: u16 = 270;
/// How thick a dial's arc may be, as a share of its radius.
pub const MIN_THICKNESS: f32 = 0.04;
pub const MAX_THICKNESS: f32 = 0.5;
pub const DEFAULT_THICKNESS: f32 = 0.16;
/// Roundest a card on the screen may be, in pixels.
pub const MAX_RADIUS: u32 = 64;

/// How a graph draws its readings.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GraphStyle {
    /// A line with the area under it tinted, as graphs have always been.
    #[default]
    Area,
    /// The line alone.
    Line,
    /// A column for each stretch of readings.
    Bars,
}

impl GraphStyle {
    pub fn name(self) -> &'static str {
        match self {
            GraphStyle::Area => "area",
            GraphStyle::Line => "line",
            GraphStyle::Bars => "bars",
        }
    }
}

/// How a bar fills.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BarStyle {
    /// A rounded track, filled smoothly, as bars have always been.
    #[default]
    Rounded,
    /// The same with square ends.
    Flat,
    /// Lit a step at a time, like a level meter.
    Segmented,
}

impl BarStyle {
    pub fn name(self) -> &'static str {
        match self {
            BarStyle::Rounded => "rounded",
            BarStyle::Flat => "flat",
            BarStyle::Segmented => "segmented",
        }
    }
}

/// What one layer says about how widgets look. Every field is optional:
/// what one layer leaves out, the one under it may say.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WidgetLook {
    /// How to draw them. Only widgets that can be drawn that way are; the
    /// rest keep their own default.
    #[serde(default)]
    pub view: Option<WidgetView>,
    /// A graph's line, a bar's fill, a dial's arc.
    #[serde(default)]
    pub color: Option<ColorRef>,
    /// What a widget sits on.
    #[serde(default)]
    pub background: Option<ColorRef>,
    /// How opaque that background is, 0 to 1.
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub graph: Option<GraphStyle>,
    #[serde(default)]
    pub bar: Option<BarStyle>,
    /// Steps in a segmented bar.
    #[serde(default)]
    pub segments: Option<u8>,
    /// How far round a dial goes, in degrees.
    #[serde(default)]
    pub sweep: Option<u16>,
    /// How thick a dial's arc is, as a share of its radius.
    #[serde(default)]
    pub thickness: Option<f32>,
    /// How round the corners of a widget's card on the screen are, in
    /// pixels.
    #[serde(default)]
    pub radius: Option<u32>,
}

impl WidgetLook {
    /// This layer, with whatever it leaves unset taken from `below`.
    ///
    /// Destructured without `..`, so a field added to the struct does not
    /// compile until it is folded here too.
    pub fn or(mut self, below: &WidgetLook) -> WidgetLook {
        let WidgetLook {
            view,
            color,
            background,
            opacity,
            graph,
            bar,
            segments,
            sweep,
            thickness,
            radius,
        } = below;
        self.view = self.view.or(*view);
        self.color = self.color.or_else(|| color.clone());
        self.background = self.background.or_else(|| background.clone());
        self.opacity = self.opacity.or(*opacity);
        self.graph = self.graph.or(*graph);
        self.bar = self.bar.or(*bar);
        self.segments = self.segments.or(*segments);
        self.sweep = self.sweep.or(*sweep);
        self.thickness = self.thickness.or(*thickness);
        self.radius = self.radius.or(*radius);
        self
    }

    pub fn is_empty(&self) -> bool {
        *self == WidgetLook::default()
    }

    /// Every colour it names, with the field each is under.
    pub fn colors(&self) -> impl Iterator<Item = (&'static str, &ColorRef)> {
        [("color", &self.color), ("background", &self.background)]
            .into_iter()
            .filter_map(|(field, color)| Some((field, color.as_ref()?)))
    }

    /// Values it would not use as they are, reported under `path`. `kind` is
    /// the kind of widget it is for, when it is for one.
    ///
    /// Colours are not checked here: whether a token resolves depends on the
    /// palette the layer is used with.
    pub fn check(&self, path: &str, kind: Option<WidgetKind>, out: &mut Diagnostics) {
        let at = |field: &str| format!("{path}.{field}");
        if let (Some(kind), Some(view)) = (kind, self.view) {
            if !view.suits(kind) {
                out.push(
                    Diagnostic::warning(
                        "W0220",
                        at("view"),
                        format!("{} widgets cannot be drawn as a {view:?}", kind.name())
                            .to_lowercase(),
                    )
                    .with_help("it is left out, and they keep their own view"),
                );
            }
        }
        let outside = |code: &'static str, field: &str, range: String| {
            Diagnostic::warning(
                code,
                at(field),
                format!("`{field}` is {range}, and will be clamped"),
            )
        };
        if self.opacity.is_some_and(|o| !(0.0..=1.0).contains(&o)) {
            out.push(outside("W0221", "opacity", "0 to 1".into()));
        }
        if self
            .segments
            .is_some_and(|s| !(MIN_SEGMENTS..=MAX_SEGMENTS).contains(&s))
        {
            out.push(outside(
                "W0222",
                "segments",
                format!("{MIN_SEGMENTS} to {MAX_SEGMENTS}"),
            ));
        }
        if self
            .sweep
            .is_some_and(|s| !(MIN_SWEEP..=MAX_SWEEP).contains(&s))
        {
            out.push(outside(
                "W0223",
                "sweep",
                format!("{MIN_SWEEP} to {MAX_SWEEP} degrees"),
            ));
        }
        if self
            .thickness
            .is_some_and(|t| !(MIN_THICKNESS..=MAX_THICKNESS).contains(&t))
        {
            out.push(outside(
                "W0224",
                "thickness",
                format!("{MIN_THICKNESS} to {MAX_THICKNESS} of the dial's radius"),
            ));
        }
        if self.radius.is_some_and(|r| r > MAX_RADIUS) {
            out.push(outside(
                "W0225",
                "radius",
                format!("at most {MAX_RADIUS} pixels"),
            ));
        }
    }
}

/// `[widgets]`: how every widget looks, with `[widgets.<kind>]` for one kind
/// over that.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WidgetLooks {
    /// For every widget.
    pub all: WidgetLook,
    /// For widgets of one kind, over `all`.
    pub kinds: Vec<(WidgetKind, WidgetLook)>,
}

impl WidgetLooks {
    /// What this layer says about a widget of `kind`.
    pub fn for_kind(&self, kind: WidgetKind) -> WidgetLook {
        match self.kinds.iter().find(|(k, _)| *k == kind) {
            Some((_, look)) => look.clone().or(&self.all),
            None => self.all.clone(),
        }
    }

    /// Each look in it, with the path it is written under: `path` for every
    /// widget's, then `path.<kind>` for each kind's.
    pub fn layers(&self, path: &str) -> Vec<(String, Option<WidgetKind>, &WidgetLook)> {
        std::iter::once((path.to_string(), None, &self.all))
            .chain(
                self.kinds
                    .iter()
                    .map(|(kind, look)| (format!("{path}.{}", kind.name()), Some(*kind), look)),
            )
            .collect()
    }

    /// Problems with its values; see [`WidgetLook::check`].
    pub fn check(&self, path: &str, out: &mut Diagnostics) {
        for (path, kind, look) in self.layers(path) {
            look.check(&path, kind, out);
        }
    }
}

impl<'de> Deserialize<'de> for WidgetLooks {
    /// Read as a table: a sub-table named for a kind of widget is that
    /// kind's look, and everything else is every widget's.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let mut table = toml::Table::deserialize(deserializer)?;
        let names: Vec<String> = table
            .iter()
            .filter(|(_, value)| value.is_table())
            .map(|(name, _)| name.clone())
            .collect();
        let mut kinds = Vec::new();
        for name in names {
            let kind = WidgetKind::deserialize(toml::Value::String(name.clone()))
                .map_err(|_| D::Error::custom(unknown_kind(&name)))?;
            let look: WidgetLook = table
                .remove(&name)
                .expect("listed above")
                .try_into()
                .map_err(|e: toml::de::Error| {
                    D::Error::custom(format!("in [widgets.{name}]: {}", e.message()))
                })?;
            kinds.push((kind, look));
        }
        let all: WidgetLook = toml::Value::Table(table)
            .try_into()
            .map_err(|e: toml::de::Error| D::Error::custom(e.message()))?;
        Ok(WidgetLooks { all, kinds })
    }
}

/// Every kind of widget, for suggesting one when a name is not.
const KINDS: [WidgetKind; 18] = [
    WidgetKind::Clock,
    WidgetKind::Date,
    WidgetKind::Cpu,
    WidgetKind::Memory,
    WidgetKind::Temperature,
    WidgetKind::Gpu,
    WidgetKind::Network,
    WidgetKind::Disk,
    WidgetKind::Weather,
    WidgetKind::Media,
    WidgetKind::Command,
    WidgetKind::Battery,
    WidgetKind::Fan,
    WidgetKind::Load,
    WidgetKind::Uptime,
    WidgetKind::Volume,
    WidgetKind::Timer,
    WidgetKind::Stopwatch,
];

fn unknown_kind(name: &str) -> String {
    let nearest = KINDS
        .iter()
        .map(|kind| kind.name())
        .min_by_key(|known| crate::edit_distance(name, known))
        .filter(|known| crate::edit_distance(name, known) <= 3);
    match nearest {
        Some(known) => {
            format!("[widgets.{name}] is not a kind of widget; did you mean [widgets.{known}]?")
        }
        None => format!("[widgets.{name}] is not a kind of widget"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> WidgetLooks {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn every_kind_is_listed_for_suggestions() {
        // A kind missing here only makes a suggestion worse, but it should
        // not go missing quietly: the names come from the kinds themselves.
        for kind in KINDS {
            assert_eq!(
                WidgetKind::deserialize(toml::Value::String(kind.name().into())).ok(),
                Some(kind)
            );
        }
    }

    #[test]
    fn a_kind_goes_over_every_widget_field_by_field() {
        let looks = parse(
            r##"
            color = "#ff0000"
            graph = "line"

            [clock]
            view = "nixie"
            color = "#ffaa00"
            "##,
        );
        let clock = looks.for_kind(WidgetKind::Clock);
        assert_eq!(clock.view, Some(WidgetView::Nixie));
        assert_eq!(clock.color, Some(ColorRef::parse("#ffaa00").unwrap()));
        assert_eq!(clock.graph, Some(GraphStyle::Line), "from every widget's");
        let cpu = looks.for_kind(WidgetKind::Cpu);
        assert_eq!(cpu.view, None);
        assert_eq!(cpu.color, Some(ColorRef::parse("#ff0000").unwrap()));
    }

    #[test]
    fn a_misspelt_kind_is_refused_with_a_suggestion() {
        let error = toml::from_str::<WidgetLooks>("[clok]\nview = \"nixie\"")
            .unwrap_err()
            .to_string();
        assert!(error.contains("did you mean [widgets.clock]"), "{error}");
        assert!(toml::from_str::<WidgetLooks>("colour = \"#ffffff\"").is_err());
        assert!(toml::from_str::<WidgetLooks>("[cpu]\ngraph = \"pie\"").is_err());
    }

    #[test]
    fn a_layer_below_fills_only_what_is_unset() {
        let above = WidgetLook {
            sweep: Some(180),
            ..WidgetLook::default()
        };
        let below = WidgetLook {
            sweep: Some(360),
            thickness: Some(0.3),
            ..WidgetLook::default()
        };
        let both = above.or(&below);
        assert_eq!(both.sweep, Some(180));
        assert_eq!(both.thickness, Some(0.3));
    }

    #[test]
    fn values_out_of_range_and_views_that_do_not_fit_are_reported() {
        let mut out = Diagnostics::new();
        parse(
            r#"
            opacity = 2.0
            segments = 1
            sweep = 30
            thickness = 0.9
            radius = 500

            [cpu]
            view = "analog"
            "#,
        )
        .check("themes.t.widgets", &mut out);
        let codes: Vec<&str> = out.iter().map(|d| d.code.as_str()).collect();
        assert_eq!(
            codes,
            ["W0221", "W0222", "W0223", "W0224", "W0225", "W0220"]
        );
        assert!(out.iter().any(|d| d.path == "themes.t.widgets.cpu.view"));
    }
}
