//! What a key's `icon` names: a picture on disk, or an icon from the desktop's
//! icon theme.
//!
//! ```toml
//! icon = "~/Pictures/deck/firefox.png"          # a file
//! icon = "network-wireless-symbolic"            # a name, looked up in the theme
//! ```
//!
//! One field takes both because a person reaching for an icon does not care
//! where it lives, and the two cannot be confused: a file is written with a
//! `/` or starts at `~`, and a theme's names never contain either. A bare
//! `firefox.png` is therefore a name, and a name that looks like a file is
//! warned about rather than guessed at -- it would otherwise be a path
//! relative to wherever the daemon happened to be started.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::diag::{Diagnostic, Diagnostics};

/// Longest icon name, in characters. The longest in Adwaita and hicolor is
/// under 60.
pub const MAX_ICON_NAME: usize = 128;

/// An `icon`, as written.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum IconRef {
    /// A picture on disk, exactly as written: a leading `~` is the daemon's
    /// to expand, since only it knows whose home that is.
    Path(PathBuf),
    /// An icon theme's name for a picture, such as `audio-volume-muted`.
    Name(String),
}

impl IconRef {
    /// Read what was written: a path if it has a `/` or starts with `~`,
    /// else a name. Never fails; a name that cannot be one is reported by
    /// [`IconRef::check`], so a config with one still loads.
    pub fn parse(text: &str) -> IconRef {
        if text.contains('/') || text.starts_with('~') {
            IconRef::Path(PathBuf::from(text))
        } else {
            IconRef::Name(text.to_string())
        }
    }

    /// The file, when it names one.
    pub fn as_path(&self) -> Option<&Path> {
        match self {
            IconRef::Path(path) => Some(path),
            IconRef::Name(_) => None,
        }
    }

    /// The theme's name for it, when it is one.
    pub fn name(&self) -> Option<&str> {
        match self {
            IconRef::Name(name) => Some(name),
            IconRef::Path(_) => None,
        }
    }

    /// Whether `name` is something an icon theme could call a picture:
    /// letters, digits and `_ . + -`, not starting with a dot.
    ///
    /// The daemon joins a name onto directories it searches, so this is
    /// what keeps one from climbing out of them.
    pub fn is_valid_name(name: &str) -> bool {
        (1..=MAX_ICON_NAME).contains(&name.chars().count())
            && !name.starts_with('.')
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '+' | '-'))
    }

    /// A name that cannot be one, or that looks like a file.
    pub fn check(&self, path: &str, out: &mut Diagnostics) {
        let IconRef::Name(name) = self else {
            return;
        };
        if !Self::is_valid_name(name) {
            out.push(
                Diagnostic::error(
                    "E0193",
                    path,
                    format!("{name:?} is neither an icon name nor a path to a picture"),
                )
                .with_help(format!(
                    "a name is up to {MAX_ICON_NAME} letters, digits and _ . + -, not starting with a dot, such as \"audio-volume-muted\"; a file is written with a / or starts at ~"
                )),
            );
            return;
        }
        let lower = name.to_ascii_lowercase();
        if [".png", ".svg", ".jpg"]
            .iter()
            .any(|extension| lower.ends_with(extension))
        {
            out.push(
                Diagnostic::warning(
                    "W0194",
                    path,
                    format!("{name:?} is looked up as an icon name, not opened as a file"),
                )
                .with_help(format!(
                    "for a file, write its whole path, such as \"~/Pictures/{name}\"; for a theme's icon, leave the extension off"
                )),
            );
        }
    }

    /// The warning for icon `name`, at `path`, that none of the installed
    /// icon themes -- `themes`, as a list to show -- has. Only the daemon
    /// knows which themes are installed, so it is the one that says; the
    /// warning is numbered here, with the config's other icon problems.
    pub fn not_found(name: &str, path: &str, themes: &str) -> Diagnostic {
        Diagnostic::warning(
            "W0197",
            path,
            format!("there is no icon called {name:?} in the icon themes {themes}"),
        )
        .with_help(
            "the key shows its label alone; pick a name the icon list offers, or give the path to a picture",
        )
    }

    /// The warning for an icon, at `path`, that cannot be drawn: `what`
    /// says which -- the file, or the name and the file it was found at --
    /// and `reason` why, such as a file that is missing, too large, or an
    /// SVG that would take too long to draw. Only the daemon draws icons,
    /// so it is the one that says.
    pub fn cannot_draw(what: &str, path: &str, reason: &str) -> Diagnostic {
        Diagnostic::warning("W0198", path, format!("{what} cannot be drawn: {reason}")).with_help(
            "the key shows its label alone; fix or replace the file, or pick another icon",
        )
    }
}

impl std::fmt::Display for IconRef {
    /// As it was written, so an editor that shows it and writes it back
    /// changes nothing.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IconRef::Path(path) => write!(f, "{}", path.display()),
            IconRef::Name(name) => f.write_str(name),
        }
    }
}

impl<'de> Deserialize<'de> for IconRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(|text| IconRef::parse(&text))
    }
}

impl From<PathBuf> for IconRef {
    /// Always a path, whatever it looks like: for a picture that is known to
    /// be a file, such as a version 1 config's `image`.
    fn from(path: PathBuf) -> Self {
        IconRef::Path(path)
    }
}
