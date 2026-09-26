//! The galdeck configuration model.
//!
//! Everything here is pure: it parses, validates and describes configuration,
//! and never touches the device, the wall clock, a socket or a subprocess. A
//! CI job enforces that, which is what lets a user interface validate an edit
//! the user has not saved yet without any of the daemon's machinery.

pub mod action;
pub mod animation;
pub mod backdrop;
pub mod color;
pub mod diag;
pub mod doc;
pub mod icon;
pub mod keys;
pub mod lighting;
pub mod look;
pub mod theme;
pub mod v1;
pub mod v2;
pub mod widget;
pub mod workspace;

pub use action::{
    Action, BuiltIn, Invocation, KeyPart, Preset, RingShows, SlotRule, StepUnit, TargetKind,
};
pub use animation::{Animation, AnimationKind};
pub use backdrop::{Backdrop, Motion, Span};
pub use color::{ColorRef, Palette, ResolvedPalette};
pub use diag::{Diagnostic, Diagnostics, LineIndex, Loc, Severity};
pub use doc::{ConfigDocument, Patch, Staged, Value};
pub use icon::IconRef;
pub use lighting::{Lighting, LightingEffect, ResolvedLighting};
pub use look::{BarStyle, GraphStyle, WidgetLook, WidgetLooks};
pub use theme::{ResolvedStyle, StyleLayer, StyleSource, Theme};
pub use v1::{default_config_path, Config, EncoderConfig, KeyConfig, LoadError, Page, ParseError};
pub use v2::{
    status_value, Global, KeyState, ModeEntry, PluginBinding, Workspace, CURRENT_VERSION,
    DEFAULT_STATUS_INTERVAL_MS, MAX_MODES, MAX_STATES, MAX_STATUS_LINE, MIN_STATES,
    MIN_STATUS_INTERVAL_MS,
};
pub use widget::{
    parse_duration, Cells, LcdGrid, LcdTile, Level, Units, Widget, WidgetKind, WidgetView,
};
pub use workspace::{config_file_names, default_config_dir, EncoderPlan, Layer, ModeStack};

/// Levenshtein distance, iterative with one row of state.
pub(crate) fn edit_distance(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b_chars.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            let insert_delete = (row[j] + 1).min(row[j + 1] + 1);
            let substitute = previous + cost;
            previous = row[j + 1];
            row[j + 1] = insert_delete.min(substitute);
        }
    }
    row[b_chars.len()]
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    /// Every source file in this crate, which is where diagnostics are made.
    const SOURCES: &[(&str, &str)] = &[
        ("action.rs", include_str!("action.rs")),
        ("animation.rs", include_str!("animation.rs")),
        ("backdrop.rs", include_str!("backdrop.rs")),
        ("color.rs", include_str!("color.rs")),
        ("diag.rs", include_str!("diag.rs")),
        ("doc.rs", include_str!("doc.rs")),
        ("icon.rs", include_str!("icon.rs")),
        ("keys.rs", include_str!("keys.rs")),
        ("lib.rs", include_str!("lib.rs")),
        ("lighting.rs", include_str!("lighting.rs")),
        ("look.rs", include_str!("look.rs")),
        ("theme.rs", include_str!("theme.rs")),
        ("v1.rs", include_str!("v1.rs")),
        ("v2.rs", include_str!("v2.rs")),
        ("widget.rs", include_str!("widget.rs")),
        ("workspace.rs", include_str!("workspace.rs")),
    ];

    /// Numbers given to two diagnostics before the rule was checked. People
    /// may have matched on them, so they stay.
    const LEGACY: &[&str] = &["0104", "0113"];

    #[test]
    fn every_source_file_is_searched_for_diagnostics() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".rs"))
            .collect();
        found.sort();
        let listed: Vec<&str> = SOURCES.iter().map(|(name, _)| *name).collect();
        assert_eq!(found, listed, "add the new file to SOURCES");
    }

    /// A code is a letter for how bad it is and a number for what it is
    /// about, so a number is one problem whatever its letter: an editor may
    /// look a number up without the letter.
    #[test]
    fn a_diagnostic_number_is_one_problem() {
        let mut letters: BTreeMap<&str, BTreeSet<char>> = BTreeMap::new();
        for (_, text) in SOURCES {
            for (at, _) in text.match_indices('"') {
                let code = &text[at + 1..];
                let bytes = code.as_bytes();
                if bytes.len() > 5
                    && matches!(bytes[0], b'E' | b'W' | b'H')
                    && bytes[1..5].iter().all(u8::is_ascii_digit)
                    && bytes[5] == b'"'
                {
                    letters
                        .entry(&code[1..5])
                        .or_default()
                        .insert(char::from(bytes[0]));
                }
            }
        }
        assert!(letters.len() > 50, "found only {letters:?}");
        let clashes: Vec<_> = letters
            .iter()
            .filter(|(number, used)| used.len() > 1 && !LEGACY.contains(number))
            .collect();
        assert!(clashes.is_empty(), "{clashes:?}");
    }
}
