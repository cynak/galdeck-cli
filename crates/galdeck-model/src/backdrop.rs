//! Backgrounds: a picture, or something that moves, behind the info screen,
//! the keys, or both at once.
//!
//! "Both" is one picture across the whole panel rather than the same picture
//! twice. The keys and the screen are regions of one physical display, and
//! the calibration says where each sits on it, so a wallpaper can run
//! continuously from the screen down through the keycaps the way it would on
//! a phone behind its icons.
//!
//! A theme may carry one, which is what makes a theme animated; a profile or a
//! page replaces it wholesale. Wholesale rather than field by field, because
//! half of one background merged with half of another is never what anyone
//! meant.

use std::path::PathBuf;

use serde::Deserialize;

use crate::color::ColorRef;

/// Slowest an animated background may run.
pub const MIN_FPS: u8 = 1;
/// Fastest. Every frame is a JPEG for the screen and one for every key it
/// covers; past this the USB link, not the animation, sets the pace.
pub const MAX_FPS: u8 = 20;
pub const DEFAULT_FPS: u8 = 10;
/// How dark a background may be made. Past this it is not a background.
pub const MAX_DIM: f32 = 0.9;

/// What a background covers.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Span {
    /// The info screen only.
    Lcd,
    /// The keys, as one picture across all twelve.
    Keys,
    /// The screen and the keys, as one picture across the panel.
    #[default]
    Both,
}

impl Span {
    pub fn covers_lcd(self) -> bool {
        matches!(self, Span::Lcd | Span::Both)
    }

    pub fn covers_keys(self) -> bool {
        matches!(self, Span::Keys | Span::Both)
    }
}

/// A background that is drawn rather than loaded.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    /// Soft bands of colour drifting across each other.
    Aurora,
    /// A gradient slowly turning.
    Gradient,
    /// Horizontal waves rolling past.
    Waves,
    /// The classic demoscene plasma, gently.
    Plasma,
    /// Stars drifting past at different depths.
    Starfield,
    /// Columns of light falling, like a terminal in a film.
    Rain,
    /// Flames licking up from the bottom.
    Fire,
    /// Bubbles rising and wobbling.
    Bubbles,
}

impl Motion {
    pub fn name(self) -> &'static str {
        match self {
            Motion::Aurora => "aurora",
            Motion::Gradient => "gradient",
            Motion::Waves => "waves",
            Motion::Plasma => "plasma",
            Motion::Starfield => "starfield",
            Motion::Rain => "rain",
            Motion::Fire => "fire",
            Motion::Bubbles => "bubbles",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Backdrop {
    #[serde(default)]
    pub span: Span,
    /// A PNG, JPEG or GIF, scaled to cover what it spans. An animated GIF
    /// plays.
    #[serde(default)]
    pub image: Option<PathBuf>,
    /// A drawn background, instead of an image.
    #[serde(default)]
    pub animation: Option<Motion>,
    /// The colours an animation is drawn in. Two to four; defaults to the
    /// theme's accent and background.
    #[serde(default)]
    pub colors: Vec<ColorRef>,
    /// Frames per second, for an animation. A GIF keeps its own timing, but
    /// is never shown faster than this.
    #[serde(default)]
    pub fps: Option<u8>,
    /// How fast an animation moves, as a multiple of its natural pace.
    #[serde(default)]
    pub speed: Option<f32>,
    /// How much to darken it, 0 to 0.9, so text over it stays readable.
    #[serde(default)]
    pub dim: Option<f32>,
}

impl Backdrop {
    pub fn fps(&self) -> u8 {
        self.fps.unwrap_or(DEFAULT_FPS).clamp(MIN_FPS, MAX_FPS)
    }

    pub fn speed(&self) -> f32 {
        self.speed.unwrap_or(1.0).clamp(0.1, 5.0)
    }

    /// Darkening, defaulting to enough that white text reads over a photo.
    pub fn dim(&self) -> f32 {
        self.dim.unwrap_or(0.25).clamp(0.0, MAX_DIM)
    }

    /// Whether there is anything to draw.
    pub fn is_empty(&self) -> bool {
        self.image.is_none() && self.animation.is_none()
    }
}
