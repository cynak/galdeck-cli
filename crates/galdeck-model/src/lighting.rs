//! The keyboard's own lighting: every key and the light bar along its top,
//! as a theme or a profile describes it.
//!
//! A theme's `[lighting]` is folded through its `extends` chain like its
//! palette, and a profile's goes on top, field by field — so a profile can
//! slow the theme's wave down without restating its colours. Colours are
//! [`ColorRef`]s, so `@accent` lights the keyboard in the same colour it
//! paints the deck.
//!
//! No `[lighting]` anywhere means the keyboard is left alone: the daemon never
//! takes it over. `effect = "off"` says the same thing explicitly, which is
//! what a profile needs to switch off its theme's lighting.
//!
//! `[lighting.reactive]` makes the keys answer presses over the effect, a
//! ripple or a glow. It lights only where the keyboard's key reports can be
//! read, which the udev rule allows as an opt-in: they are every key press.

use std::collections::BTreeMap;

use galdeck::Rgb;
use serde::Deserialize;

use crate::color::{ColorRef, ResolvedPalette};
use crate::diag::{Diagnostic, Diagnostics};

/// Slowest and fastest an effect may cycle, in cycles per second. Slower
/// than this looks frozen; faster flickers.
pub const MIN_SPEED: f64 = 0.01;
pub const MAX_SPEED: f64 = 4.0;
pub const DEFAULT_SPEED: f64 = 0.2;
pub const DEFAULT_BRIGHTNESS: u8 = 60;
/// Quickest and slowest a press may fade, in milliseconds. Quicker is a
/// flicker nobody sees; slower leaves the keyboard lit long after typing.
pub const MIN_FADE_MS: u32 = 100;
pub const MAX_FADE_MS: u32 = 5_000;
pub const DEFAULT_FADE_MS: u32 = 800;

/// What the keys do.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LightingEffect {
    /// Every key the first colour.
    #[default]
    Static,
    /// The colours spread left to right across the keyboard, still.
    Gradient,
    /// The first colour fading in and out.
    Breathe,
    /// The colours rolling left to right.
    Wave,
    /// The whole keyboard cycling through every hue.
    Spectrum,
    /// Hand the lighting back to the keyboard's own effects.
    Off,
}

/// How the keys answer presses.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReactiveEffect {
    /// A ring spreads from the pressed key across the keyboard.
    #[default]
    Ripple,
    /// The pressed key flares and fades.
    Glow,
    /// Presses light nothing: how a profile switches off its theme's.
    None,
}

/// `[lighting.reactive]` as written, every field optional as in `[lighting]`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reactive {
    #[serde(default)]
    pub effect: Option<ReactiveEffect>,
    #[serde(default)]
    pub color: Option<ColorRef>,
    /// How long a press takes to fade out, in milliseconds.
    #[serde(default)]
    pub fade_ms: Option<u32>,
}

impl Reactive {
    /// `above` over this layer, field by field.
    pub fn overlay(&mut self, above: &Reactive) {
        if above.effect.is_some() {
            self.effect = above.effect;
        }
        if above.color.is_some() {
            self.color.clone_from(&above.color);
        }
        if above.fade_ms.is_some() {
            self.fade_ms = above.fade_ms;
        }
    }
}

/// `[lighting]` as written. Every field is optional so a layer can change
/// one thing and inherit the rest.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lighting {
    #[serde(default)]
    pub effect: Option<LightingEffect>,
    #[serde(default)]
    pub colors: Option<Vec<ColorRef>>,
    /// Cycles per second, for the effects that move.
    #[serde(default)]
    pub speed: Option<f64>,
    /// 0-100, of the keyboard's full brightness.
    #[serde(default)]
    pub brightness: Option<u8>,
    /// The light bar, when it should not follow the effect.
    #[serde(default)]
    pub bar: Option<ColorRef>,
    /// Keys lit in a colour of their own over the effect. A name may list
    /// several keys: `"w a s d" = "@red"`.
    #[serde(default)]
    pub keys: BTreeMap<String, ColorRef>,
    /// Keys answering presses, over the effect.
    #[serde(default)]
    pub reactive: Option<Reactive>,
}

impl Lighting {
    /// `above` over this layer, field by field. Per-key colours merge, so a
    /// profile adds to its theme's rather than replacing them.
    pub fn overlay(&mut self, above: &Lighting) {
        if above.effect.is_some() {
            self.effect = above.effect;
        }
        if above.colors.is_some() {
            self.colors.clone_from(&above.colors);
        }
        if above.speed.is_some() {
            self.speed = above.speed;
        }
        if above.brightness.is_some() {
            self.brightness = above.brightness;
        }
        if above.bar.is_some() {
            self.bar.clone_from(&above.bar);
        }
        for (keys, color) in &above.keys {
            self.keys.insert(keys.clone(), color.clone());
        }
        if let Some(above) = &above.reactive {
            self.reactive
                .get_or_insert_with(Reactive::default)
                .overlay(above);
        }
    }

    /// Resolve against a palette. A colour whose token is unknown is left
    /// out, reported under `path`.
    pub fn resolve(
        &self,
        palette: &ResolvedPalette,
        path: &str,
        out: &mut Diagnostics,
    ) -> ResolvedLighting {
        let colors: Vec<Rgb> = match &self.colors {
            Some(colors) => colors
                .iter()
                .enumerate()
                .filter_map(|(i, c)| palette.resolve(c, &format!("{path}.colors[{i}]"), out))
                .collect(),
            None => Vec::new(),
        };
        ResolvedLighting {
            effect: self.effect.unwrap_or_default(),
            colors: if colors.is_empty() {
                vec![Rgb::WHITE]
            } else {
                colors
            },
            speed: self
                .speed
                .unwrap_or(DEFAULT_SPEED)
                .clamp(MIN_SPEED, MAX_SPEED),
            brightness: self.brightness.unwrap_or(DEFAULT_BRIGHTNESS).min(100),
            bar: self
                .bar
                .as_ref()
                .and_then(|c| palette.resolve(c, &format!("{path}.bar"), out)),
            keys: self
                .keys
                .iter()
                .filter_map(|(keys, c)| {
                    let rgb = palette.resolve(c, &format!("{path}.keys.{keys:?}"), out)?;
                    Some((keys.clone(), rgb))
                })
                .collect(),
            reactive: self.reactive.as_ref().and_then(|reactive| {
                let effect = reactive.effect.unwrap_or_default();
                if effect == ReactiveEffect::None {
                    return None;
                }
                // An unknown colour is reported, and the keys still answer, in
                // white, rather than going quiet with no clue why.
                let color = reactive
                    .color
                    .as_ref()
                    .and_then(|c| palette.resolve(c, &format!("{path}.reactive.color"), out))
                    .unwrap_or(Rgb::WHITE);
                Some(ResolvedReactive {
                    effect,
                    color,
                    fade_ms: reactive
                        .fade_ms
                        .unwrap_or(DEFAULT_FADE_MS)
                        .clamp(MIN_FADE_MS, MAX_FADE_MS),
                })
            }),
        }
    }

    /// Problems with this layer's own values, reported under `path`.
    /// Colours are checked by [`Lighting::resolve`], against the palette the
    /// layer will be used with.
    pub fn check(&self, path: &str, out: &mut Diagnostics) {
        if let Some(brightness) = self.brightness {
            if brightness > 100 {
                out.push(
                    Diagnostic::error(
                        "E0200",
                        format!("{path}.brightness"),
                        "brightness must be 0-100",
                    )
                    .with_help(format!("got {brightness}")),
                );
            }
        }
        if let Some(speed) = self.speed {
            if !(MIN_SPEED..=MAX_SPEED).contains(&speed) {
                out.push(
                    Diagnostic::warning(
                        "W0201",
                        format!("{path}.speed"),
                        format!("speed is clamped to {MIN_SPEED}-{MAX_SPEED} cycles a second"),
                    )
                    .with_help(format!("got {speed}")),
                );
            }
        }
        if self.colors.as_ref().is_some_and(Vec::is_empty) {
            out.push(
                Diagnostic::error("E0202", format!("{path}.colors"), "`colors` is empty")
                    .with_help("give at least one colour, or leave `colors` out for white"),
            );
        }
        for keys in self.keys.keys() {
            if keys.split_whitespace().next().is_none() {
                out.push(Diagnostic::error(
                    "E0203",
                    format!("{path}.keys"),
                    "a key colour needs at least one key name",
                ));
            }
        }
        if let Some(fade) = self.reactive.as_ref().and_then(|r| r.fade_ms) {
            if !(MIN_FADE_MS..=MAX_FADE_MS).contains(&fade) {
                out.push(
                    Diagnostic::warning(
                        "W0204",
                        format!("{path}.reactive.fade_ms"),
                        format!("fade_ms is clamped to {MIN_FADE_MS}-{MAX_FADE_MS}"),
                    )
                    .with_help(format!("got {fade}")),
                );
            }
        }
    }
}

/// Said of a name in `keys`, or of a pressed key, that names no key or
/// group. Checked where the keyboard's layout is known: by the daemon.
pub fn unknown_key(path: impl Into<String>, name: &str) -> Diagnostic {
    Diagnostic::warning("W0205", path, format!("no key or group is called {name:?}"))
        .with_help("a key's name, as `W` or `LShift`; a group, as `letters` or `bar`; or `all`")
}

/// Lighting with every colour resolved and every default applied.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedLighting {
    pub effect: LightingEffect,
    /// Never empty.
    pub colors: Vec<Rgb>,
    pub speed: f64,
    pub brightness: u8,
    pub bar: Option<Rgb>,
    /// Space-separated key names, and their colour. Names are checked where
    /// the keyboard's layout is known: by the daemon.
    pub keys: Vec<(String, Rgb)>,
    /// How the keys answer presses; `None` when they do not.
    pub reactive: Option<ResolvedReactive>,
}

/// `[lighting.reactive]` with its colour resolved and every default applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedReactive {
    /// Never [`ReactiveEffect::None`]: that resolves to no reaction at all.
    pub effect: ReactiveEffect,
    pub color: Rgb,
    pub fade_ms: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Lighting {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn a_layer_above_changes_only_what_it_names() {
        let mut theme = parse(
            r##"
            effect = "wave"
            colors = ["#ff0000", "#0000ff"]
            speed = 0.5
            keys = { "w a s d" = "#00ff00" }
            "##,
        );
        theme.overlay(&parse(
            r##"speed = 0.1
            keys = { esc = "#ffffff" }"##,
        ));
        assert_eq!(theme.effect, Some(LightingEffect::Wave));
        assert_eq!(theme.colors.as_ref().map(Vec::len), Some(2));
        assert_eq!(theme.speed, Some(0.1));
        assert_eq!(theme.keys.len(), 2, "per-key colours merge");
    }

    #[test]
    fn defaults_fill_what_nobody_set() {
        let resolved = Lighting::default().resolve(
            &ResolvedPalette::default(),
            "lighting",
            &mut Diagnostics::new(),
        );
        assert_eq!(resolved.effect, LightingEffect::Static);
        assert_eq!(resolved.colors, vec![Rgb::WHITE]);
        assert_eq!(resolved.speed, DEFAULT_SPEED);
        assert_eq!(resolved.brightness, DEFAULT_BRIGHTNESS);
    }

    #[test]
    fn out_of_range_values_are_reported() {
        let mut out = Diagnostics::new();
        parse(
            r#"brightness = 150
            speed = 9.0
            colors = []"#,
        )
        .check("lighting", &mut out);
        let codes: Vec<&str> = out.iter().map(|d| d.code.as_str()).collect();
        assert_eq!(codes, ["E0200", "W0201", "E0202"]);
    }

    #[test]
    fn an_unknown_token_is_reported_and_left_out() {
        let mut out = Diagnostics::new();
        let resolved = parse(r##"colors = ["@nope", "#102030"]"##).resolve(
            &ResolvedPalette::default(),
            "themes.t.lighting",
            &mut out,
        );
        assert_eq!(resolved.colors, vec![Rgb::new(0x10, 0x20, 0x30)]);
        assert_eq!(out.iter().count(), 1);
        assert!(out
            .iter()
            .next()
            .unwrap()
            .path
            .starts_with("themes.t.lighting.colors[0]"));
    }

    #[test]
    fn presses_ripple_in_white_unless_told_otherwise() {
        let resolved = parse("[reactive]").resolve(
            &ResolvedPalette::default(),
            "lighting",
            &mut Diagnostics::new(),
        );
        assert_eq!(
            resolved.reactive,
            Some(ResolvedReactive {
                effect: ReactiveEffect::Ripple,
                color: Rgb::WHITE,
                fade_ms: DEFAULT_FADE_MS,
            })
        );
        // Without the table, the keys do not answer at all.
        let quiet = Lighting::default().resolve(
            &ResolvedPalette::default(),
            "lighting",
            &mut Diagnostics::new(),
        );
        assert_eq!(quiet.reactive, None);
    }

    #[test]
    fn a_profile_can_recolour_or_silence_its_themes_presses() {
        let mut theme = parse(
            r##"
            [reactive]
            effect = "glow"
            fade_ms = 400
            "##,
        );
        theme.overlay(&parse("[reactive]\ncolor = \"#ff0000\""));
        let resolved = theme.resolve(&ResolvedPalette::default(), "l", &mut Diagnostics::new());
        let reactive = resolved.reactive.unwrap();
        assert_eq!(reactive.effect, ReactiveEffect::Glow);
        assert_eq!(reactive.fade_ms, 400);
        assert_eq!(reactive.color, Rgb::new(255, 0, 0));

        theme.overlay(&parse("[reactive]\neffect = \"none\""));
        let silenced = theme.resolve(&ResolvedPalette::default(), "l", &mut Diagnostics::new());
        assert_eq!(silenced.reactive, None);
    }

    #[test]
    fn a_fade_out_of_range_is_clamped_and_said_so() {
        let lighting = parse("[reactive]\nfade_ms = 20");
        let mut out = Diagnostics::new();
        lighting.check("lighting", &mut out);
        let codes: Vec<&str> = out.iter().map(|d| d.code.as_str()).collect();
        assert_eq!(codes, ["W0204"]);
        let resolved = lighting.resolve(&ResolvedPalette::default(), "lighting", &mut out);
        assert_eq!(resolved.reactive.unwrap().fade_ms, MIN_FADE_MS);
    }

    #[test]
    fn an_unknown_press_colour_is_reported_and_white_is_used() {
        let mut out = Diagnostics::new();
        let resolved = parse("[reactive]\ncolor = \"@nope\"").resolve(
            &ResolvedPalette::default(),
            "themes.t.lighting",
            &mut out,
        );
        assert_eq!(resolved.reactive.unwrap().color, Rgb::WHITE);
        assert!(out
            .iter()
            .any(|d| d.path.starts_with("themes.t.lighting.reactive.color")));
    }

    #[test]
    fn unknown_fields_are_refused() {
        assert!(toml::from_str::<Lighting>("colour = \"#ffffff\"").is_err());
        assert!(toml::from_str::<Lighting>("effect = \"sparkle\"").is_err());
        assert!(toml::from_str::<Lighting>("[reactive]\nduration = 800").is_err());
    }
}
