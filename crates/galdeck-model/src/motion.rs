//! How a theme moves: what a key does the moment it is pressed, how a widget
//! past its threshold draws attention, and what the knob rings do at rest.
//!
//! `[motion]` in a theme, folded through `extends`, with the profile's
//! `[motion]` over it. Each setting is taken whole from the nearest layer
//! that has it: half of one animation and half of another is never what
//! anyone meant.
//!
//! Nothing here moves unless asked. A theme without `[motion]` looks and
//! behaves exactly as it did before there was one.

use serde::Deserialize;

use crate::animation::{Animation, AnimationKind};
use crate::color::ColorRef;
use crate::diag::{Diagnostic, Diagnostics};

/// Quickest and slowest a press may fade.
pub const MIN_PRESS_MS: u32 = 80;
pub const MAX_PRESS_MS: u32 = 1000;
pub const DEFAULT_PRESS_MS: u32 = 220;
/// How far towards its colour a pressed key goes at first.
pub const PRESS_STRENGTH: f32 = 0.6;

/// What a key does the moment it is pressed.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PressKind {
    /// Nothing but what pressing it does.
    #[default]
    None,
    /// A flash towards a colour, fading out.
    Flash,
    /// A dip towards black, as if pushed in, coming back up.
    Dim,
}

impl PressKind {
    pub fn name(self) -> &'static str {
        match self {
            PressKind::None => "none",
            PressKind::Flash => "flash",
            PressKind::Dim => "dim",
        }
    }
}

/// `press` in `[motion]`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Press {
    pub kind: PressKind,
    /// What a flash goes towards. Defaults to the theme's `@accent`, else
    /// white.
    #[serde(default)]
    pub color: Option<ColorRef>,
    /// How long it takes to fade, in milliseconds.
    #[serde(default)]
    pub ms: Option<u32>,
}

impl Press {
    /// How long it takes to fade, clamped to what reads as an answer.
    pub fn ms(&self) -> u32 {
        self.ms
            .unwrap_or(DEFAULT_PRESS_MS)
            .clamp(MIN_PRESS_MS, MAX_PRESS_MS)
    }

    /// How far towards its colour a key is `elapsed` milliseconds after it
    /// was pressed, or `None` once it is over: most of the way at once,
    /// easing out, so it reads as an answer to the press and not as a blink.
    pub fn share_at(&self, elapsed: u32) -> Option<f32> {
        if self.kind == PressKind::None {
            return None;
        }
        let left = 1.0 - elapsed as f32 / self.ms() as f32;
        (left > 0.0).then_some(PRESS_STRENGTH * left * left)
    }
}

/// `[motion]`, as a theme or a profile writes it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MotionStyle {
    /// What a key does the moment it is pressed.
    #[serde(default)]
    pub press: Option<Press>,
    /// How a widget past its `warn` or `critical` threshold draws
    /// attention: moving between its usual colour and the theme's
    /// `@warning` or `@critical` as this animation does. Without it, the
    /// widget only changes colour.
    #[serde(default)]
    pub alarm: Option<Animation>,
    /// What every knob's ring does at rest, unless the knob has an
    /// animation of its own.
    #[serde(default)]
    pub rings: Option<Animation>,
}

impl MotionStyle {
    /// This layer, with whatever it leaves unset taken from `below`.
    ///
    /// Destructured without `..`, so a setting added to the struct does not
    /// compile until it is folded here too.
    pub fn or(mut self, below: &MotionStyle) -> MotionStyle {
        let MotionStyle {
            press,
            alarm,
            rings,
        } = below;
        self.press = self.press.or_else(|| press.clone());
        self.alarm = self.alarm.or_else(|| alarm.clone());
        self.rings = self.rings.or_else(|| rings.clone());
        self
    }

    /// Every colour it names, with the path of the field it is in.
    pub fn colors(&self) -> Vec<(&'static str, &ColorRef)> {
        let press = self.press.as_ref().and_then(|press| press.color.as_ref());
        let rings = self.rings.as_ref().and_then(|rings| rings.to.as_ref());
        [("press.color", press), ("rings.to", rings)]
            .into_iter()
            .filter_map(|(field, color)| Some((field, color?)))
            .collect()
    }

    /// Settings it would not use as they are, reported under `path`.
    /// Colours are checked where the palette they resolve against is known,
    /// and animation periods with every other animation's.
    pub fn check(&self, path: &str, out: &mut Diagnostics) {
        let at = |field: &str| format!("{path}.{field}");
        if let Some(alarm) = &self.alarm {
            // An alarm is a widget's own colours moving towards the alarm's:
            // a travelling segment has nowhere to travel, and a rainbow
            // would hide which alarm it is.
            if alarm.kind.is_ring_only() || alarm.kind == AnimationKind::Rainbow {
                out.push(
                    Diagnostic::warning(
                        "W0226",
                        at("alarm.kind"),
                        format!("an alarm cannot {}", describe(alarm.kind)),
                    )
                    .with_help(
                        "use pulse, breathe, blink or heartbeat; until then it only changes colour",
                    ),
                );
            }
            if alarm.to.is_some() {
                out.push(
                    Diagnostic::warning(
                        "W0229",
                        at("alarm.to"),
                        "an alarm moves towards the theme's @warning or @critical, so `to` is not used",
                    )
                    .with_help("change @warning and @critical in the palette instead"),
                );
            }
        }
        if let Some(press) = &self.press {
            if press
                .ms
                .is_some_and(|ms| !(MIN_PRESS_MS..=MAX_PRESS_MS).contains(&ms))
            {
                out.push(Diagnostic::warning(
                    "W0227",
                    at("press.ms"),
                    format!("a press fades in {MIN_PRESS_MS} to {MAX_PRESS_MS} ms, and this will be clamped"),
                ));
            }
            if press.kind == PressKind::Dim && press.color.is_some() {
                out.push(Diagnostic::warning(
                    "W0230",
                    at("press.color"),
                    "a dim goes towards black, so `color` is not used",
                ));
            }
        }
    }
}

fn describe(kind: AnimationKind) -> &'static str {
    match kind {
        AnimationKind::Spin => "spin",
        AnimationKind::Comet => "be a comet",
        AnimationKind::Rainbow => "be a rainbow",
        _ => "move that way",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> MotionStyle {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn a_press_starts_strong_and_eases_out_to_nothing() {
        let press = Press {
            kind: PressKind::Flash,
            color: None,
            ms: Some(200),
        };
        let first = press.share_at(0).unwrap();
        let half = press.share_at(100).unwrap();
        assert!((first - PRESS_STRENGTH).abs() < 1e-6);
        assert!(half < first / 2.0, "eased out, not linear: {half}");
        assert_eq!(press.share_at(200), None);
        let none = Press {
            kind: PressKind::None,
            ..press
        };
        assert_eq!(none.share_at(0), None);
    }

    #[test]
    fn a_layer_below_fills_only_what_is_unset() {
        let theme = parse(
            r#"
            press = { kind = "flash", ms = 300 }
            rings = { kind = "breathe", period_ms = 4000 }
            "#,
        );
        let profile = parse(r#"press = { kind = "dim" }"#);
        let both = profile.or(&theme);
        // Taken whole: the profile's press, not its kind with the theme's ms.
        assert_eq!(both.press.as_ref().unwrap().kind, PressKind::Dim);
        assert_eq!(both.press.as_ref().unwrap().ms, None);
        assert_eq!(both.rings.as_ref().unwrap().kind, AnimationKind::Breathe);
        assert!(both.alarm.is_none());
    }

    #[test]
    fn what_would_not_be_used_is_reported() {
        let mut out = Diagnostics::new();
        parse(
            r##"
            press = { kind = "dim", color = "#ffffff", ms = 5000 }
            alarm = { kind = "spin", to = "#ff0000" }
            "##,
        )
        .check("themes.t.motion", &mut out);
        let codes: Vec<&str> = out.iter().map(|d| d.code.as_str()).collect();
        assert_eq!(codes, ["W0226", "W0229", "W0227", "W0230"]);
    }

    #[test]
    fn unknown_settings_and_kinds_are_refused() {
        assert!(toml::from_str::<MotionStyle>("presses = { kind = \"flash\" }").is_err());
        assert!(toml::from_str::<MotionStyle>("press = { kind = \"wobble\" }").is_err());
        // Not yet: a page change is at once.
        assert!(toml::from_str::<MotionStyle>("pages = { kind = \"fade\" }").is_err());
    }
}
