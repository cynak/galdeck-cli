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
    }
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
    fn unknown_fields_are_refused() {
        assert!(toml::from_str::<Lighting>("colour = \"#ffffff\"").is_err());
        assert!(toml::from_str::<Lighting>("effect = \"sparkle\"").is_err());
    }
}
