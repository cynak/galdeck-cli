//! Loading a config directory, and migrating an old one.
//!
//! A v1 `config.toml` is migrated in memory on the way in. Nothing on disk
//! changes until the user asks — an upgrade that silently rewrites the file
//! whose comments are the user's own work is not an upgrade.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::action::KeyPart;
use crate::color::{resolve_palette, ColorRef, Palette, ResolvedPalette};
use crate::diag::{Diagnostic, Diagnostics, LineIndex};
use crate::theme::{
    resolve, ResolvedStyle, StyleLayer, StyleProvenance, StyleSource, MAX_EXTENDS_DEPTH,
};
use crate::v1;
use crate::v2::{
    EncoderConfig, Global, KeyConfig, ModeEntry, Page, Profile, Workspace, CURRENT_VERSION,
    MAX_MODES, MAX_STATES, MIN_STATES, MIN_STATUS_INTERVAL_MS,
};

/// Where a config directory lives: `$XDG_CONFIG_HOME/galdeck`.
pub fn default_config_dir() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config")
        });
    base.join("galdeck")
}

impl Workspace {
    /// Load a config directory, or migrate a v1 file found in its place.
    ///
    /// Returns whatever could be loaded alongside everything wrong with it, so
    /// a user interface can show all the problems at once rather than one per
    /// save.
    pub fn load(dir: &Path) -> (Option<Workspace>, Vec<Diagnostic>) {
        Self::load_with_overrides(dir, &BTreeMap::new())
    }

    /// Load a config directory, substituting the text of named files.
    ///
    /// This is what makes live validation honest: an edit is checked against
    /// the whole workspace it would create, so "this theme no longer defines
    /// @accent, and three keys reference it" is caught before saving rather
    /// than after.
    pub fn load_with_overrides(
        dir: &Path,
        overrides: &BTreeMap<String, String>,
    ) -> (Option<Workspace>, Vec<Diagnostic>) {
        let mut out = Diagnostics::new();

        // An existing single-file config keeps working untouched.
        let legacy = dir.join("config.toml");
        if !dir.join("galdeck.toml").exists() && legacy.exists() {
            return match std::fs::read_to_string(&legacy) {
                Ok(text) => match v1::Config::parse(&text) {
                    Ok(config) => {
                        let mut workspace = Workspace::from_v1(&config);
                        workspace.dress_widgets();
                        out.push(Diagnostic::hint(
                            "H0100",
                            "config.toml",
                            "loaded a version 1 config; run `galdeck config migrate` to split it into profiles and themes",
                        ));
                        let mut all = workspace.validate();
                        for d in out.sorted() {
                            all.push(d);
                        }
                        (Some(workspace), all.sorted())
                    }
                    Err(v1::ParseError::Parse(e)) => {
                        out.push(Diagnostic::error("E0001", "config.toml", e.to_string()));
                        (None, out.sorted())
                    }
                    Err(v1::ParseError::Invalid(diagnostics)) => (None, diagnostics),
                },
                Err(e) => {
                    out.push(Diagnostic::error("E0002", "config.toml", e.to_string()));
                    (None, out.sorted())
                }
            };
        }

        let global = match read_toml::<Global>(
            &dir.join("galdeck.toml"),
            "galdeck.toml",
            overrides.get("galdeck.toml").map(String::as_str),
            &mut out,
        ) {
            Some(global) => global,
            None => return (None, out.sorted()),
        };

        if global.version > CURRENT_VERSION {
            out.push(
                Diagnostic::error(
                    "E0003",
                    "galdeck.toml.version",
                    format!(
                        "this config is version {}, but this build understands up to {CURRENT_VERSION}",
                        global.version
                    ),
                )
                .with_help("it was probably written by a newer galdeck; upgrading should fix it"),
            );
            return (None, out.sorted());
        }

        let profiles = read_dir_of("profiles", dir, overrides, &mut out);
        let themes = read_dir_of("themes", dir, overrides, &mut out);

        let mut workspace = Workspace {
            global,
            profiles,
            themes,
        };
        workspace.dress_widgets();
        let mut all = workspace.validate();
        for d in out.sorted() {
            all.push(d);
        }
        (Some(workspace), all.sorted())
    }

    /// Turn a version 1 config into the version 2 shape, in memory.
    ///
    /// Everything lands in one profile called `default` with no theme, because
    /// a v1 config expressed its colours per key and per encoder — which is
    /// exactly what a cell-level style layer is.
    pub fn from_v1(config: &v1::Config) -> Workspace {
        let pages = config
            .pages
            .iter()
            .map(|page| Page {
                id: page.name.clone(),
                lcd_text: page.lcd_text.clone(),
                lcd: Vec::new(),
                background: None,
                lcd_columns: None,
                lcd_rows: None,
                style: StyleLayer::default(),
                keys: page
                    .keys
                    .iter()
                    .map(|key| KeyConfig {
                        key: key.key,
                        label: key.label.clone(),
                        // A version 1 `image` was always a file, whatever
                        // it looks like.
                        icon: key.image.clone().map(crate::icon::IconRef::from),
                        exec: key.exec.clone().map(crate::action::Action::Shell),
                        hold: None,
                        double: None,
                        page: key.page.clone(),
                        profile: None,
                        back: false,
                        animation: None,
                        widget: None,
                        plugin: None,
                        states: Vec::new(),
                        status: None,
                        status_interval_ms: None,
                        style: StyleLayer {
                            // v1's `color` was the key background.
                            key_bg: key.color.as_deref().and_then(literal),
                            ..StyleLayer::default()
                        },
                    })
                    .collect(),
                encoders: page
                    .encoders
                    .iter()
                    .map(|encoder| EncoderConfig {
                        encoder: encoder.encoder,
                        preset: None,
                        modes: Vec::new(),
                        step: None,
                        target: None,
                        press: encoder.press.clone().map(crate::action::Action::Shell),
                        cw: encoder.cw.clone().map(crate::action::Action::Shell),
                        ccw: encoder.ccw.clone().map(crate::action::Action::Shell),
                        hold: None,
                        style: StyleLayer {
                            ring: encoder.ring.as_deref().and_then(literal),
                            ..StyleLayer::default()
                        },
                        animation: None,
                    })
                    .collect(),
                widgets: None,
            })
            .collect();

        let profile = Profile {
            name: Some("Default".to_string()),
            theme: None,
            home: config.pages.first().map(|page| page.name.clone()),
            style: StyleLayer::default(),
            background: None,
            encoders: Vec::new(),
            lcd_columns: None,
            lcd_rows: None,
            pages,
            lighting: None,
            widgets: None,
        };

        Workspace {
            global: Global {
                version: CURRENT_VERSION,
                brightness: config.brightness,
                font: config.font.clone(),
                profile: Some("default".to_string()),
                ..Global::default()
            },
            profiles: BTreeMap::from([("default".to_string(), profile)]),
            themes: BTreeMap::new(),
        }
    }

    /// A theme with its `extends` chain folded in, and its palette resolved.
    pub fn theme_for(
        &self,
        id: Option<&str>,
        out: &mut Diagnostics,
    ) -> (StyleLayer, ResolvedPalette) {
        let Some(id) = id else {
            return (StyleLayer::default(), ResolvedPalette::default());
        };
        let Some(theme) = self.themes.get(id) else {
            out.push(Diagnostic::error(
                "E0115",
                "theme",
                format!("unknown theme {id:?}"),
            ));
            return (StyleLayer::default(), ResolvedPalette::default());
        };

        // Walk the chain from this theme up to its most distant ancestor, then
        // fold back down so the nearest theme wins.
        let mut chain = vec![theme];
        let mut seen = vec![id.to_string()];
        let mut current = theme;
        while let Some(parent_id) = current.extends.as_deref() {
            if seen.iter().any(|s| s == parent_id) {
                out.push(Diagnostic::error(
                    "E0114",
                    format!("themes.{parent_id}.extends"),
                    format!("theme {parent_id:?} extends itself, directly or otherwise"),
                ));
                break;
            }
            if chain.len() >= MAX_EXTENDS_DEPTH {
                out.push(Diagnostic::error(
                    "E0113",
                    format!("themes.{id}.extends"),
                    format!("theme inheritance is deeper than {MAX_EXTENDS_DEPTH} levels"),
                ));
                break;
            }
            let Some(parent) = self.themes.get(parent_id) else {
                out.push(Diagnostic::error(
                    "E0115",
                    format!("themes.{}.extends", seen.last().expect("non-empty")),
                    format!("unknown theme {parent_id:?}"),
                ));
                break;
            };
            seen.push(parent_id.to_string());
            chain.push(parent);
            current = parent;
        }

        let mut palette = Palette::default();
        let mut style = StyleLayer::default();
        for theme in chain.iter().rev() {
            for (name, color) in &theme.palette.0 {
                palette.0.insert(name.clone(), color.clone());
            }
            overlay(&mut style, &theme.style);
        }

        (style, resolve_palette(&palette, out))
    }

    /// The keyboard lighting a profile asks for: its theme's `[lighting]`
    /// with the `extends` chain folded in, then the profile's own over it.
    /// `None` when neither says anything, which leaves the keyboard alone.
    pub fn lighting_for(
        &self,
        profile: &Profile,
        out: &mut Diagnostics,
    ) -> Option<crate::lighting::ResolvedLighting> {
        // Nearest theme first. Bounded like `theme_for`, which is what
        // reports a broken chain; this only has to stop.
        let mut layers = Vec::new();
        let mut id = profile.theme.as_deref();
        for _ in 0..MAX_EXTENDS_DEPTH {
            let Some(theme) = id.and_then(|id| self.themes.get(id)) else {
                break;
            };
            layers.extend(theme.lighting.as_ref());
            id = theme.extends.as_deref();
        }
        if layers.is_empty() && profile.lighting.is_none() {
            return None;
        }

        let mut lighting = crate::lighting::Lighting::default();
        for layer in layers.iter().rev() {
            lighting.overlay(layer);
        }
        if let Some(own) = &profile.lighting {
            lighting.overlay(own);
        }
        // The theme's own problems are reported by `theme_for` wherever the
        // theme is used; only the lighting's are new here.
        let (_, palette) = self.theme_for(profile.theme.as_deref(), &mut Diagnostics::new());
        Some(lighting.resolve(&palette, "lighting", out))
    }

    /// A theme and the themes it extends, nearest first: `mine`, then the
    /// theme `mine` extends, and so on up.
    ///
    /// Stops where [`Workspace::theme_for`] stops -- at an unknown theme, a
    /// repeat, or [`MAX_EXTENDS_DEPTH`] themes -- and leaves reporting that to
    /// it. Empty for an unknown `id`.
    pub fn theme_chain(&self, id: &str) -> Vec<(&str, &crate::theme::Theme)> {
        let mut chain: Vec<(&str, &crate::theme::Theme)> = Vec::new();
        let mut next = self.themes.get_key_value(id);
        while let Some((id, theme)) = next {
            if chain.len() >= MAX_EXTENDS_DEPTH || chain.iter().any(|(seen, _)| *seen == id) {
                break;
            }
            chain.push((id, theme));
            next = theme
                .extends
                .as_deref()
                .and_then(|parent| self.themes.get_key_value(parent));
        }
        chain
    }

    /// What a theme and the themes it extends say about a widget of `kind`,
    /// the nearest winning field by field -- the theme on its own, as its
    /// editor previews it.
    pub fn theme_widget_look(
        &self,
        theme: &str,
        kind: crate::widget::WidgetKind,
    ) -> crate::look::WidgetLook {
        look_through(
            self.theme_chain(theme)
                .into_iter()
                .filter_map(|(_, theme)| theme.widgets.as_ref()),
            kind,
        )
    }

    /// What a widget of `kind` on `page` of `profile` takes from them, under
    /// whatever it says itself: the page's `[widgets]`, then the profile's,
    /// then its theme's.
    pub fn widget_look(
        &self,
        profile: &Profile,
        page: Option<&Page>,
        kind: crate::widget::WidgetKind,
    ) -> crate::look::WidgetLook {
        let theme = profile
            .theme
            .as_deref()
            .map(|id| self.theme_chain(id))
            .unwrap_or_default();
        look_through(
            page.and_then(|page| page.widgets.as_ref())
                .into_iter()
                .chain(profile.widgets.as_ref())
                .chain(
                    theme
                        .into_iter()
                        .filter_map(|(_, theme)| theme.widgets.as_ref()),
                ),
            kind,
        )
    }

    /// Give every widget its look; see [`crate::widget::Widget::look`].
    ///
    /// Once, when the configuration loads, so that drawing a widget never
    /// has to go looking through themes -- and a widget keeps its look for
    /// as long as the configuration it came from.
    pub fn dress_widgets(&mut self) {
        // Worked out in full first: the looks read the themes and profiles
        // that the second pass is about to borrow mutably.
        let mut looks = Vec::new();
        for (id, profile) in &self.profiles {
            for (index, page) in profile.pages.iter().enumerate() {
                let key_looks: Vec<_> = page
                    .keys
                    .iter()
                    .map(|key| {
                        let widget = key.widget.as_ref()?;
                        Some(self.widget_look(profile, Some(page), widget.kind))
                    })
                    .collect();
                let tile_looks: Vec<_> = page
                    .lcd
                    .iter()
                    .map(|tile| self.widget_look(profile, Some(page), tile.widget.kind))
                    .collect();
                looks.push((id.clone(), index, key_looks, tile_looks));
            }
        }
        for (id, index, key_looks, tile_looks) in looks {
            let Some(page) = self
                .profiles
                .get_mut(&id)
                .and_then(|profile| profile.pages.get_mut(index))
            else {
                continue;
            };
            for (key, look) in page.keys.iter_mut().zip(key_looks) {
                if let (Some(widget), Some(look)) = (key.widget.as_mut(), look) {
                    widget.look = look;
                }
            }
            for (tile, look) in page.lcd.iter_mut().zip(tile_looks) {
                tile.widget.look = look;
            }
        }
    }

    /// A knob on a page in its first mode; see [`Workspace::encoder_for_mode`].
    pub fn encoder_for(
        profile: &Profile,
        page: &Page,
        global: &[EncoderConfig],
        encoder: u8,
    ) -> EncoderPlan {
        Self::encoder_for_mode(profile, page, global, encoder, 0)
    }

    /// A knob on a page in `mode`, with the global, profile and page layers
    /// folded together one gesture at a time.
    ///
    /// At each layer a preset supplies all its gestures and anything written
    /// beside it replaces the preset's one; then each layer's gestures replace
    /// the layer below's, gesture by gesture. So a volume knob set once in
    /// galdeck.toml is on every page, and a page that sets only `press`
    /// keeps the volume turn.
    ///
    /// Modes fold as a preset's turn does. The highest layer that says what
    /// turning does -- through modes, a preset, or `cw` and `ccw` -- owns
    /// the turn, and modes beneath it are shadowed: that layer then adds only
    /// what is written beside them, since a hold cycling modes nobody can
    /// see would be worse than none. At the owning layer the active mode's
    /// preset stands in for `preset`, and gestures written there and above
    /// still replace its own. Holding a knob with modes, when nothing else
    /// is bound to holding it, switches to the next.
    pub fn encoder_for_mode(
        profile: &Profile,
        page: &Page,
        global: &[EncoderConfig],
        encoder: u8,
        mode: usize,
    ) -> EncoderPlan {
        let mut plan = EncoderPlan::default();
        let layers = [
            (Layer::Global, global.iter().find(|e| e.encoder == encoder)),
            (
                Layer::Profile,
                profile.encoders.iter().find(|e| e.encoder == encoder),
            ),
            (
                Layer::Page,
                page.encoders.iter().find(|e| e.encoder == encoder),
            ),
        ];
        let owner = layers
            .iter()
            .rev()
            .find(|(_, config)| config.is_some_and(EncoderConfig::supplies_turn))
            .map(|(layer, _)| *layer);
        for (layer, config) in layers {
            let Some(config) = config else { continue };
            let expanded = match config.mode_stack() {
                Some(_) if owner != Some(layer) => None,
                Some(entries) => {
                    plan.mode = mode.min(entries.len() - 1);
                    plan.modes = Some(ModeStack {
                        entries: entries.to_vec(),
                        layer,
                    });
                    config.preset_in_mode(plan.mode)
                }
                None => config.preset_in_mode(0),
            };
            let [press, cw, ccw] = expanded
                .map(|(preset, step, target)| preset.gestures(step, target))
                .unwrap_or_default();
            let press = config.press.clone().or(press);
            let cw = config.cw.clone().or(cw);
            let ccw = config.ccw.clone().or(ccw);
            let hold = config.hold.clone();
            let tag = |action: Option<crate::action::Action>| action.map(|a| (a, layer));
            if cw.is_some() || ccw.is_some() {
                // The turn comes as a pair; what the ring shows goes with it.
                plan.turn_preset = expanded
                    .map(|(preset, ..)| preset)
                    .filter(|_| config.cw.is_none() && config.ccw.is_none())
                    .map(|preset| (preset, layer));
            }
            plan.press = tag(press).or(plan.press.take());
            plan.cw = tag(cw).or(plan.cw.take());
            plan.ccw = tag(ccw).or(plan.ccw.take());
            plan.hold = tag(hold).or(plan.hold.take());
            overlay(&mut plan.style, &config.style);
            if config.animation.is_some() {
                plan.animation = config.animation.clone();
            }
        }
        // After the fold, so a hold written on any layer -- above the modes
        // or beneath them -- still wins.
        if plan.hold.is_none() {
            if let Some(stack) = &plan.modes {
                plan.hold = Some((
                    crate::action::Action::built_in(crate::action::BuiltIn::NextMode),
                    stack.layer,
                ));
            }
        }
        plan
    }

    /// The grid a page's screen is laid out on: the page's own columns and
    /// rows, else its profile's, else 12 by 6.
    pub fn grid_for(profile: &Profile, page: &Page) -> crate::widget::LcdGrid {
        crate::widget::LcdGrid::new(
            page.lcd_columns.or(profile.lcd_columns),
            page.lcd_rows.or(profile.lcd_rows),
        )
    }

    /// The background behind a page: its own, else its profile's, else the
    /// nearest one in its theme's `extends` chain.
    pub fn background_for<'a>(
        &'a self,
        profile: &'a Profile,
        page: &'a Page,
    ) -> Option<&'a crate::backdrop::Backdrop> {
        if let Some(own) = page.background.as_ref().or(profile.background.as_ref()) {
            return Some(own);
        }
        let mut id = profile.theme.as_deref();
        // Bounded like `theme_for`, so a cycle it already reported cannot
        // spin here.
        for _ in 0..MAX_EXTENDS_DEPTH {
            let theme = self.themes.get(id?)?;
            if let Some(background) = &theme.background {
                return Some(background);
            }
            id = theme.extends.as_deref();
        }
        None
    }

    /// Resolve the style for one cell.
    ///
    /// Layers run from the most general to the most specific, so a page may
    /// restyle one field without restating the theme.
    #[allow(clippy::too_many_arguments)]
    pub fn style_for(
        &self,
        theme: &StyleLayer,
        palette: &ResolvedPalette,
        profile: &Profile,
        page: &Page,
        cell: Option<&StyleLayer>,
        path: &str,
        out: &mut Diagnostics,
    ) -> (ResolvedStyle, StyleProvenance) {
        let mut layers = vec![
            (StyleSource::Theme, theme),
            (StyleSource::Profile, &profile.style),
            (StyleSource::Page, &page.style),
        ];
        if let Some(cell) = cell {
            layers.push((StyleSource::Cell, cell));
        }
        resolve(&layers, palette, path, out)
    }

    /// Everything wrong with this workspace.
    pub fn validate(&self) -> Diagnostics {
        let mut out = Diagnostics::new();

        if self.global.brightness > 100 {
            out.push(
                Diagnostic::error("E0101", "brightness", "brightness must be 0-100")
                    .with_help(format!("got {}", self.global.brightness)),
            );
        }
        if self.profiles.is_empty() {
            out.push(Diagnostic::error(
                "E0104",
                "profiles",
                "no profiles found; there is nothing to show",
            ));
        }
        if let Some(start) = self.global.profile.as_deref() {
            if !self.profiles.contains_key(start) {
                out.push(Diagnostic::error(
                    "E0105",
                    "profile",
                    format!("start profile {start:?} does not exist"),
                ));
            }
        }

        for (id, theme) in &self.themes {
            if let Some(background) = &theme.background {
                check_backdrop(background, &format!("themes.{id}.background"), &mut out);
            }
        }

        check_encoders(
            &self.global.encoders,
            "encoders",
            Layer::Global,
            self.global.virtual_input,
            &mut out,
        );
        for (i, entry) in self.global.outputs.iter().enumerate() {
            let sink = crate::action::TargetKind::Sink;
            if entry.trim().is_empty() || !sink.accepts(entry) {
                out.push(
                    Diagnostic::warning(
                        "W0139",
                        format!("outputs[{i}]"),
                        format!("{entry:?} is not a valid {}", sink.noun()),
                    )
                    .with_help(sink.hint()),
                );
            }
        }
        if let Some(theme) = self.global.icon_theme.as_deref() {
            // The daemon looks for a directory by this name, so anything
            // that would take it somewhere else is refused there.
            let one_directory = !theme.is_empty()
                && theme != "."
                && theme != ".."
                && !theme.contains('/')
                && !theme.chars().any(char::is_control);
            if !one_directory {
                out.push(
                    Diagnostic::warning(
                        "W0196",
                        "icon_theme",
                        format!("{theme:?} is not the name of an icon theme, so it is not used"),
                    )
                    .with_help("the name of a folder in /usr/share/icons or ~/.local/share/icons, such as \"Adwaita\""),
                );
            }
        }

        for (id, profile) in &self.profiles {
            let at = |what: &str| format!("profiles.{id}.{what}");
            check_encoders(
                &profile.encoders,
                &at("encoders"),
                Layer::Profile,
                self.global.virtual_input,
                &mut out,
            );
            if let Some(background) = &profile.background {
                check_backdrop(background, &at("background"), &mut out);
            }
            for (which, value, max) in [
                (
                    "lcd_columns",
                    profile.lcd_columns,
                    crate::widget::MAX_LCD_COLUMNS,
                ),
                ("lcd_rows", profile.lcd_rows, crate::widget::MAX_LCD_ROWS),
            ] {
                check_grid_size(value, max, &at(which), &mut out);
            }
            if profile.pages.is_empty() {
                out.push(Diagnostic::error(
                    "E0102",
                    at("pages"),
                    "a profile needs at least one page",
                ));
            }
            if let Some(theme) = profile.theme.as_deref() {
                if !self.themes.contains_key(theme) {
                    out.push(Diagnostic::error(
                        "E0115",
                        at("theme"),
                        format!("unknown theme {theme:?}"),
                    ));
                }
            }
            if let Some(home) = profile.home.as_deref() {
                if profile.page(home).is_none() {
                    out.push(Diagnostic::error(
                        "E0106",
                        at("home"),
                        format!("home page {home:?} does not exist"),
                    ));
                }
            }

            let mut seen_pages: Vec<&str> = Vec::new();
            for (p, page) in profile.pages.iter().enumerate() {
                let at = |what: &str| format!("profiles.{id}.pages[{p}].{what}");
                // Still reached by `next_page`, which goes by place in the
                // list. But a switch by id, and what the daemon keeps of a
                // key's states and timer, find the first page with the id.
                if seen_pages.contains(&page.id.as_str()) {
                    out.push(
                        Diagnostic::warning(
                            "W0103",
                            at("id"),
                            format!(
                                "duplicate page {:?}; switching to it goes to the first, and this one's keys have no states or timers",
                                page.id
                            ),
                        )
                        .with_help("rename one of them"),
                    );
                }
                seen_pages.push(&page.id);
                if let Some(background) = &page.background {
                    check_backdrop(background, &at("background"), &mut out);
                }

                let mut seen_keys: Vec<u8> = Vec::new();
                for (k, key) in page.keys.iter().enumerate() {
                    let at = |what: &str| format!("profiles.{id}.pages[{p}].keys[{k}].{what}");
                    if key.key >= galdeck::Buttons::COUNT {
                        out.push(Diagnostic::error(
                            "E0110",
                            at("key"),
                            format!(
                                "key {} out of range 0-{}",
                                key.key,
                                galdeck::Buttons::COUNT - 1
                            ),
                        ));
                    } else if seen_keys.contains(&key.key) {
                        out.push(Diagnostic::warning(
                            "W0111",
                            at("key"),
                            format!(
                                "key {} is configured twice; only the first applies",
                                key.key
                            ),
                        ));
                    } else {
                        seen_keys.push(key.key);
                    }

                    if let Some(target) = key.page.as_deref() {
                        if profile.page(target).is_none() {
                            out.push(
                                Diagnostic::error(
                                    "E0112",
                                    at("page"),
                                    format!("switches to unknown page {target:?}"),
                                )
                                .with_help(nearest(target, &seen_pages_of(profile))),
                            );
                        }
                    }
                    if let Some(target) = key.profile.as_deref() {
                        if !self.profiles.contains_key(target) {
                            out.push(
                                Diagnostic::error(
                                    "E0107",
                                    at("profile"),
                                    format!("switches to unknown profile {target:?}"),
                                )
                                .with_help(nearest(
                                    target,
                                    &self.profiles.keys().map(String::as_str).collect::<Vec<_>>(),
                                )),
                            );
                        }
                    }
                    if let Some(animation) = &key.animation {
                        if animation.kind.is_ring_only() {
                            out.push(
                                Diagnostic::error(
                                    "E0140",
                                    at("animation.kind"),
                                    format!("{:?} only works on an encoder ring", animation.kind),
                                )
                                .with_help("try \"pulse\", \"breathe\" or \"blink\""),
                            );
                        }
                        check_period(animation, &at("animation.period_ms"), &mut out);
                        // The flash when a timer finishes is drawn on the
                        // key, and frames made in advance would cover it.
                        if key
                            .widget
                            .as_ref()
                            .is_some_and(|w| w.kind == crate::widget::WidgetKind::Timer)
                        {
                            out.push(
                                Diagnostic::warning(
                                    "W0184",
                                    at("animation"),
                                    "an animation on a timer's key hides the flash when it finishes",
                                )
                                .with_help("remove the animation, or give the timer a key of its own"),
                            );
                        }
                    }
                    if let Some(widget) = &key.widget {
                        check_widget(
                            widget,
                            &at("widget"),
                            false,
                            self.global.virtual_input,
                            &mut out,
                        );
                    }
                    if let Some(icon) = &key.icon {
                        icon.check(&at("icon"), &mut out);
                    }
                    check_states(key, &at, self.global.virtual_input, &mut out);
                    let counts = key.widget.as_ref().is_some_and(|w| w.kind.is_timer());
                    for (name, action) in [
                        ("exec", &key.exec),
                        ("hold", &key.hold),
                        ("double", &key.double),
                    ] {
                        if let Some(action) = action {
                            let slot = if name == "exec" {
                                Slot::KeyTap
                            } else {
                                Slot::Key
                            };
                            check_action(
                                action,
                                &at(name),
                                slot,
                                self.global.virtual_input,
                                &mut out,
                            );
                            let own = action
                                .as_built_in()
                                .and_then(|i| Some((i.action, i.action.acts_on()?)));
                            match own {
                                Some((built_in, KeyPart::Timer)) if !counts => out.push(
                                    Diagnostic::warning(
                                        "W0183",
                                        at(name),
                                        format!(
                                            "{} acts on the key's own timer, and this key has none",
                                            built_in.name()
                                        ),
                                    )
                                    .with_help("give the key a `timer` or `stopwatch` widget"),
                                ),
                                Some((built_in, KeyPart::States)) if key.states.is_empty() => {
                                    out.push(
                                        Diagnostic::warning(
                                            "W0192",
                                            at(name),
                                            format!(
                                                "{} steps through the states of the key it is bound to, and this key has none",
                                                built_in.name()
                                            ),
                                        )
                                        .with_help("give the key `states` to step through"),
                                    )
                                }
                                _ => {}
                            }
                        }
                    }
                    // Push-to-talk answers the key's own press and release, so
                    // the gesture machine that would find a hold, a double
                    // tap or a page switch never sees the key.
                    let talks = key
                        .exec
                        .as_ref()
                        .and_then(crate::action::Action::as_built_in)
                        .is_some_and(|i| i.action == crate::action::BuiltIn::PushToTalk);
                    if talks {
                        let shadowed: Vec<&str> = [
                            ("hold", key.hold.is_some()),
                            ("double", key.double.is_some()),
                            ("page", key.page.is_some()),
                            ("profile", key.profile.is_some()),
                            ("back", key.back),
                            ("plugin", key.plugin.is_some()),
                        ]
                        .into_iter()
                        .filter_map(|(name, set)| set.then_some(name))
                        .collect();
                        if !shadowed.is_empty() {
                            out.push(
                                Diagnostic::warning(
                                    "W0131",
                                    at("exec"),
                                    format!(
                                        "push-to-talk takes the key's press and release, so its {} never happen{}",
                                        shadowed.join(", "),
                                        if shadowed.len() == 1 { "s" } else { "" }
                                    ),
                                )
                                .with_help("move them to another key"),
                            );
                        }
                    }
                    // A key that only shows something -- a clock, a graph -- is
                    // doing its job without being pressed.
                    if !key.is_bound() && key.widget.is_none() {
                        out.push(
                            Diagnostic::hint(
                                "H0113",
                                // Not `at("")`, which leaves a trailing dot on
                                // a path the user reads.
                                format!("profiles.{id}.pages[{p}].keys[{k}]"),
                                "this key does nothing when pressed",
                            )
                            .with_help(
                                "give it an `exec`, a `page`, a `profile`, or `back = true`",
                            ),
                        );
                    }
                }

                for (which, value, max) in [
                    (
                        "lcd_columns",
                        page.lcd_columns,
                        crate::widget::MAX_LCD_COLUMNS,
                    ),
                    ("lcd_rows", page.lcd_rows, crate::widget::MAX_LCD_ROWS),
                ] {
                    check_grid_size(value, max, &at(which), &mut out);
                }
                let grid = Self::grid_for(profile, page);
                for (t, tile) in page.lcd.iter().enumerate() {
                    let at = |what: &str| format!("profiles.{id}.pages[{p}].lcd[{t}].{what}");
                    let cells = tile.cells(grid);
                    if !tile.fits(grid) {
                        out.push(
                            Diagnostic::error(
                                "E0160",
                                at("column"),
                                format!(
                                    "a {}x{} tile at column {}, row {} does not fit the screen",
                                    cells.columns, cells.rows, cells.column, cells.row
                                ),
                            )
                            .with_help(format!(
                                "this page's screen is {} columns by {} rows, numbered from 0",
                                grid.columns, grid.rows
                            )),
                        );
                    } else if let Some(o) = page.lcd[..t]
                        .iter()
                        .position(|other| other.fits(grid) && other.cells(grid).overlaps(&cells))
                    {
                        out.push(Diagnostic::warning(
                            "W0161",
                            at("column"),
                            format!("overlaps lcd[{o}]; this one is drawn over it"),
                        ));
                    }
                    check_widget(
                        &tile.widget,
                        &at("widget"),
                        true,
                        self.global.virtual_input,
                        &mut out,
                    );
                }
                if page.lcd_text.is_some() && !page.lcd.is_empty() {
                    out.push(Diagnostic::hint(
                        "H0162",
                        at("lcd_text"),
                        "not shown, because this page lays out widgets on the screen",
                    ));
                }

                check_encoders(
                    &page.encoders,
                    &format!("profiles.{id}.pages[{p}].encoders"),
                    Layer::Page,
                    self.global.virtual_input,
                    &mut out,
                );
            }
        }

        // Whether a knob's modes can be switched depends on every layer at
        // once, so it is checked on the folded knob of every page. Reported
        // at the modes, once however many pages they reach.
        for (id, profile) in &self.profiles {
            for (p, page) in profile.pages.iter().enumerate() {
                for encoder in 0..galdeck::Encoders::COUNT {
                    let plan =
                        Self::encoder_for_mode(profile, page, &self.global.encoders, encoder, 0);
                    let (Some(stack), Some((hold, held_at))) = (&plan.modes, &plan.hold) else {
                        continue;
                    };
                    if hold
                        .as_built_in()
                        .is_some_and(|i| i.action == crate::action::BuiltIn::NextMode)
                    {
                        continue;
                    }
                    let (list, prefix) = match stack.layer {
                        Layer::Global => (&self.global.encoders, "encoders".to_string()),
                        Layer::Profile => (&profile.encoders, format!("profiles.{id}.encoders")),
                        Layer::Page => {
                            (&page.encoders, format!("profiles.{id}.pages[{p}].encoders"))
                        }
                    };
                    let Some(e) = list.iter().position(|knob| knob.encoder == encoder) else {
                        continue;
                    };
                    let path = format!("{prefix}[{e}].modes");
                    if !out.iter().any(|d| d.code == "W0135" && d.path == path) {
                        out.push(
                            Diagnostic::warning(
                                "W0135",
                                path,
                                "hold is set, so these modes can never be switched",
                            )
                            .with_help(format!(
                                "`hold` is set in the {} layer; without it, holding the knob switches modes",
                                held_at.name()
                            )),
                        );
                    }
                }
            }
        }

        // Themes are checked by resolving them, which is what surfaces palette
        // cycles and unknown tokens in the palette itself.
        for id in self.themes.keys() {
            let _ = self.theme_for(Some(id), &mut out);
        }

        // Resolving every cell's style is the only thing that reaches tokens
        // used in a style layer rather than in a palette. It is also a free
        // check that the whole cascade holds together.
        for (id, profile) in &self.profiles {
            let mut theme_diagnostics = Diagnostics::new();
            let (theme, palette) = self.theme_for(profile.theme.as_deref(), &mut theme_diagnostics);
            // Already reported above; resolving again would duplicate them.
            drop(theme_diagnostics);

            // A knob set in galdeck.toml or in a profile resolves against
            // each profile's palette, as the daemon does. Only its own
            // tokens: the layers beneath it are checked where they are
            // written.
            let knobs =
                self.global
                    .encoders
                    .iter()
                    .enumerate()
                    .map(|(e, knob)| (format!("encoders[{e}].style"), &knob.style))
                    .chain(profile.encoders.iter().enumerate().map(|(e, knob)| {
                        (format!("profiles.{id}.encoders[{e}].style"), &knob.style)
                    }));
            for (path, style) in knobs {
                let mut found = Diagnostics::new();
                let _ = resolve(&[(StyleSource::Cell, style)], &palette, &path, &mut found);
                for diagnostic in found.iter() {
                    // A global knob fails the same way under every profile
                    // whose theme lacks the token; once is enough.
                    let seen = out.iter().any(|d| {
                        d.code == diagnostic.code
                            && d.path == diagnostic.path
                            && d.message == diagnostic.message
                    });
                    if !seen {
                        out.push(diagnostic.clone());
                    }
                }
            }

            for (p, page) in profile.pages.iter().enumerate() {
                let at = |what: &str| format!("profiles.{id}.pages[{p}].{what}");
                let _ = self.style_for(
                    &theme,
                    &palette,
                    profile,
                    page,
                    None,
                    &at("style"),
                    &mut out,
                );
                for (k, key) in page.keys.iter().enumerate() {
                    let path = format!("profiles.{id}.pages[{p}].keys[{k}].style");
                    let _ = self.style_for(
                        &theme,
                        &palette,
                        profile,
                        page,
                        Some(&key.style),
                        &path,
                        &mut out,
                    );
                    // Only the state's own layer: the key's beneath it was
                    // resolved just above, and would be reported twice.
                    for (s, state) in key.states.iter().enumerate() {
                        let path = format!("profiles.{id}.pages[{p}].keys[{k}].states[{s}].style");
                        let _ = resolve(
                            &[(StyleSource::Cell, &state.style)],
                            &palette,
                            &path,
                            &mut out,
                        );
                    }
                }
                for (e, encoder) in page.encoders.iter().enumerate() {
                    let path = format!("profiles.{id}.pages[{p}].encoders[{e}].style");
                    let _ = self.style_for(
                        &theme,
                        &palette,
                        profile,
                        page,
                        Some(&encoder.style),
                        &path,
                        &mut out,
                    );
                }
            }
        }

        // Lighting is checked layer by layer, against the palette each layer
        // is used with, so a problem is reported where it is written.
        let lit_themes = self.themes.iter().filter_map(|(id, theme)| {
            let lighting = theme.lighting.as_ref()?;
            Some((format!("themes.{id}.lighting"), lighting, Some(id.as_str())))
        });
        let lit_profiles = self.profiles.iter().filter_map(|(id, profile)| {
            let lighting = profile.lighting.as_ref()?;
            Some((
                format!("profiles.{id}.lighting"),
                lighting,
                profile.theme.as_deref(),
            ))
        });
        for (path, lighting, theme) in lit_themes.chain(lit_profiles) {
            lighting.check(&path, &mut out);
            let (_, palette) = self.theme_for(theme, &mut Diagnostics::new());
            let _ = lighting.resolve(&palette, &path, &mut out);
        }

        // Widget looks likewise: each `[widgets]` checked where it is
        // written, and its colours against the palette it is used with.
        let themed_looks = self.themes.iter().filter_map(|(id, theme)| {
            let looks = theme.widgets.as_ref()?;
            Some((format!("themes.{id}.widgets"), looks, Some(id.as_str())))
        });
        let own_looks = self.profiles.iter().flat_map(|(id, profile)| {
            let theme = profile.theme.as_deref();
            let pages = profile
                .pages
                .iter()
                .enumerate()
                .filter_map(move |(index, page)| {
                    let looks = page.widgets.as_ref()?;
                    Some((
                        format!("profiles.{id}.pages[{index}].widgets"),
                        looks,
                        theme,
                    ))
                });
            profile
                .widgets
                .iter()
                .map(move |looks| (format!("profiles.{id}.widgets"), looks, theme))
                .chain(pages)
        });
        for (path, looks, theme) in themed_looks.chain(own_looks) {
            looks.check(&path, &mut out);
            let (_, palette) = self.theme_for(theme, &mut Diagnostics::new());
            for (at, _, look) in looks.layers(&path) {
                for (field, color) in look.colors() {
                    let _ = palette.resolve(color, &format!("{at}.{field}"), &mut out);
                }
            }
        }

        out
    }
}

/// Fold `[widgets]` layers, nearest first, into what they say about a
/// widget of `kind`: in each layer its kind's look over its every-widget
/// look, and a nearer layer over a farther one.
fn look_through<'a>(
    layers: impl Iterator<Item = &'a crate::look::WidgetLooks>,
    kind: crate::widget::WidgetKind,
) -> crate::look::WidgetLook {
    layers.fold(crate::look::WidgetLook::default(), |look, layer| {
        look.or(&layer.for_kind(kind))
    })
}

/// A period outside what the panel can show is clamped rather than refused,
/// but silently clamping a value the user typed is how a config stops meaning
/// what it says.
fn check_period(animation: &crate::animation::Animation, path: &str, out: &mut Diagnostics) {
    use crate::animation::{MAX_PERIOD_MS, MIN_PERIOD_MS};
    if animation.period_ms < MIN_PERIOD_MS || animation.period_ms > MAX_PERIOD_MS {
        out.push(
            Diagnostic::warning(
                "W0141",
                path,
                format!(
                    "period {} ms is outside {MIN_PERIOD_MS}-{MAX_PERIOD_MS} and will be clamped to {}",
                    animation.period_ms,
                    animation.period_ms()
                ),
            )
            .with_help("below the minimum it reads as a flicker rather than motion"),
        );
    }
}

/// A key's states, what sits beside them, and each one's own settings.
fn check_states(
    key: &KeyConfig,
    at: &dyn Fn(&str) -> String,
    virtual_input: bool,
    out: &mut Diagnostics,
) {
    let count = key.states.len();
    if count == 0 && key.status.is_some() {
        out.push(
            Diagnostic::warning(
                "W0195",
                at("status"),
                "`status` reads which state the key is in, and this key has no states",
            )
            .with_help("give the key `states`, or remove `status`"),
        );
    }
    if count > 0 && !(MIN_STATES..=MAX_STATES).contains(&count) {
        out.push(
            Diagnostic::error(
                "E0186",
                at("states"),
                format!(
                    "a key steps through {MIN_STATES} to {MAX_STATES} states; this one has {count}"
                ),
            )
            .with_help(if count < MIN_STATES {
                "one state is nothing to switch between: add another, or give the key its look directly"
            } else {
                "taps only go forward, so past a handful a key for each choice is quicker"
            }),
        );
    }
    if count > 0 {
        // A key with states steps through them when tapped; anything else
        // bound to the tap would take it over, and the states would never
        // change.
        for (field, set) in [
            ("exec", key.exec.is_some()),
            ("page", key.page.is_some()),
            ("profile", key.profile.is_some()),
            ("back", key.back),
            ("plugin", key.plugin.is_some()),
        ] {
            if set {
                out.push(
                    Diagnostic::error(
                        "E0188",
                        at(field),
                        format!("`{field}` and `states` both say what tapping this key does"),
                    )
                    .with_help("a key with states steps through them when tapped; move this to `hold`, `double` or another key"),
                );
            }
        }
        if key.widget.is_some() {
            out.push(
                Diagnostic::error(
                    "E0188",
                    at("widget"),
                    "a key with states shows which one it is in, so it cannot show a widget too",
                )
                .with_help("give the widget a key of its own"),
            );
        }
    }
    if let Some(interval) = key.status_interval_ms {
        if key.status.is_none() {
            out.push(Diagnostic::warning(
                "W0190",
                at("status_interval_ms"),
                "`status_interval_ms` is how often `status` is read, and this key has no `status`",
            ));
        } else if interval < MIN_STATUS_INTERVAL_MS {
            out.push(
                Diagnostic::warning(
                    "W0190",
                    at("status_interval_ms"),
                    format!(
                        "every {interval} ms is more often than every {MIN_STATUS_INTERVAL_MS} ms, and will be clamped to it"
                    ),
                )
                .with_help("every read starts a process, for as long as the page is showing"),
            );
        }
    }
    if key.status.as_deref().is_some_and(|c| c.trim().is_empty()) {
        out.push(Diagnostic::warning(
            "W0122",
            at("status"),
            "an empty command does nothing",
        ));
    }

    for (s, state) in key.states.iter().enumerate() {
        let at = |what: &str| at(&format!("states[{s}].{what}"));
        let same_name = |other: &crate::v2::KeyState| {
            other.name.trim().to_lowercase() == state.name.trim().to_lowercase()
        };
        if state.name.trim().is_empty() {
            out.push(
                Diagnostic::error("E0187", at("name"), "a state needs a name")
                    .with_help("the name is how the key remembers which state it is in"),
            );
        } else if let Some(first) = key.states[..s].iter().position(same_name) {
            out.push(
                Diagnostic::error(
                    "E0187",
                    at("name"),
                    format!("states[{first}] is called {:?} too", key.states[first].name),
                )
                .with_help(
                    "names differ by more than capitals, so the key can tell which state it is in",
                ),
            );
        }

        // What `status` prints can mean only one state; the first that
        // matches it is the one shown. Two names that are the same are
        // reported above, so they are not reported again here -- but names
        // that differ only by quotes are different names that match alike.
        let own = !state.matches.is_empty();
        let mut reported: Vec<String> = Vec::new();
        for value in state.match_values() {
            let value = crate::v2::match_key(value);
            if value.is_empty() || reported.contains(&value) {
                continue;
            }
            let earlier = key.states[..s].iter().position(|other| {
                (own || !other.matches.is_empty() || !same_name(other))
                    && other
                        .match_values()
                        .any(|theirs| crate::v2::match_key(theirs) == value)
            });
            if let Some(first) = earlier {
                out.push(
                    Diagnostic::warning(
                        "W0191",
                        at(if own { "match" } else { "name" }),
                        format!(
                            "{value:?} is also what states[{first}] matches, so it always means states[{first}]"
                        ),
                    )
                    .with_help("give each state `match` values of its own"),
                );
                reported.push(value);
            }
        }

        if let Some(icon) = &state.icon {
            icon.check(&at("icon"), out);
        }
        if let Some(animation) = &state.animation {
            if animation.kind.is_ring_only() {
                out.push(
                    Diagnostic::error(
                        "E0140",
                        at("animation.kind"),
                        format!("{:?} only works on an encoder ring", animation.kind),
                    )
                    .with_help("try \"pulse\", \"breathe\" or \"blink\""),
                );
            }
            check_period(animation, &at("animation.period_ms"), out);
        }
        if let Some(action) = &state.exec {
            check_action(action, &at("exec"), Slot::StateEntry, virtual_input, out);
        }
    }
}

/// Where a knob was configured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    Global,
    Profile,
    Page,
}

impl Layer {
    pub fn name(self) -> &'static str {
        match self {
            Layer::Global => "global",
            Layer::Profile => "profile",
            Layer::Page => "page",
        }
    }
}

/// A knob with every layer folded in: what each gesture does and which layer
/// said so.
#[derive(Clone, Debug, Default)]
pub struct EncoderPlan {
    pub press: Option<(crate::action::Action, Layer)>,
    pub cw: Option<(crate::action::Action, Layer)>,
    pub ccw: Option<(crate::action::Action, Layer)>,
    pub hold: Option<(crate::action::Action, Layer)>,
    /// The preset the turn came from, if it came from one: what the ring and
    /// the on-screen display show while turning.
    pub turn_preset: Option<(crate::action::Preset, Layer)>,
    /// Ring colour and other style, the layers overlaid.
    pub style: StyleLayer,
    pub animation: Option<crate::animation::Animation>,
    /// The modes the knob switches between, when the turn comes from a
    /// layer that has them.
    pub modes: Option<ModeStack>,
    /// Which of them is on, kept within the stack. Zero without modes.
    pub mode: usize,
}

/// A knob's modes, and the layer that wrote them.
///
/// The two together are what a mode is remembered by: a page that lists
/// the same modes as another shares its place in them, and a changed list
/// starts again from the first.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ModeStack {
    pub entries: Vec<ModeEntry>,
    pub layer: Layer,
}

impl EncoderPlan {
    pub fn is_empty(&self) -> bool {
        self.press.is_none() && self.cw.is_none() && self.ccw.is_none() && self.hold.is_none()
    }

    pub fn ring(&self) -> Option<crate::action::RingShows> {
        self.turn_preset.and_then(|(preset, _)| preset.ring())
    }

    /// The mode the knob is in, when it has modes.
    pub fn active_mode(&self) -> Option<&ModeEntry> {
        self.modes.as_ref()?.entries.get(self.mode)
    }
}

/// Which gesture slot an action is in, for the checks that depend on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    /// A key's tap: the only place push-to-talk can work, because only it
    /// sees the key come back up.
    KeyTap,
    /// A key's hold or double tap.
    Key,
    /// Any of a knob's gestures.
    Knob,
    /// What a key's state runs as the key enters it.
    StateEntry,
    /// None of these: what a timer does when it finishes.
    Other,
}

/// Knobs written at one layer.
fn check_encoders(
    encoders: &[EncoderConfig],
    prefix: &str,
    layer: Layer,
    virtual_input: bool,
    out: &mut Diagnostics,
) {
    let mut seen: Vec<u8> = Vec::new();
    for (e, encoder) in encoders.iter().enumerate() {
        let at = |what: &str| format!("{prefix}[{e}].{what}");
        if encoder.encoder >= galdeck::Encoders::COUNT {
            out.push(Diagnostic::error(
                "E0120",
                at("encoder"),
                format!(
                    "encoder {} out of range 0-{}",
                    encoder.encoder,
                    galdeck::Encoders::COUNT - 1
                ),
            ));
        } else if seen.contains(&encoder.encoder) {
            out.push(Diagnostic::warning(
                "W0121",
                at("encoder"),
                format!(
                    "encoder {} is configured twice; only the first applies",
                    encoder.encoder
                ),
            ));
        } else {
            seen.push(encoder.encoder);
        }
        if let Some(animation) = &encoder.animation {
            check_period(animation, &at("animation.period_ms"), out);
        }
        for (slot, action) in [
            ("press", &encoder.press),
            ("cw", &encoder.cw),
            ("ccw", &encoder.ccw),
            ("hold", &encoder.hold),
        ] {
            if let Some(action) = action {
                check_action(action, &at(slot), Slot::Knob, virtual_input, out);
            }
        }
        if !encoder.modes.is_empty() {
            check_modes(encoder, &at, layer, virtual_input, out);
        } else if let Some(preset) = encoder.preset {
            check_preset(
                preset,
                encoder.step,
                encoder.target.as_deref(),
                &at,
                layer,
                virtual_input,
                out,
            );
        } else {
            if encoder.step.is_some() {
                out.push(Diagnostic::warning(
                    "W0123",
                    at("step"),
                    "`step` is for a preset's actions; this knob has no preset",
                ));
            }
            if encoder.target.is_some() {
                out.push(Diagnostic::warning(
                    "W0126",
                    at("target"),
                    "`target` is for a preset's actions; this knob has no preset",
                ));
            }
        }
    }
}

/// A knob's modes, written at one layer: how many, what sits beside them,
/// and each one as a preset of its own.
fn check_modes(
    encoder: &EncoderConfig,
    at: &dyn Fn(&str) -> String,
    layer: Layer,
    virtual_input: bool,
    out: &mut Diagnostics,
) {
    let count = encoder.modes.len();
    // Fewer than two modes are no modes at all, so a preset beside them is
    // what the knob does, with the step and target beside it.
    let used = encoder.mode_stack().is_some();
    if !(2..=MAX_MODES).contains(&count) {
        out.push(
            Diagnostic::error(
                "E0132",
                at("modes"),
                format!("a knob switches between 2 and {MAX_MODES} modes; this one has {count}"),
            )
            .with_help(if count < 2 {
                "with only one, write it as `preset` instead"
            } else {
                "the ring has four segments to show which mode is on, so only the first four are used"
            }),
        );
    }
    if let Some(preset) = encoder.preset {
        out.push(
            Diagnostic::error(
                "E0133",
                at("preset"),
                if used {
                    "`preset` and `modes` on the same knob; the modes are used"
                } else {
                    "`preset` and `modes` on the same knob; with fewer than two modes, the preset is used"
                },
            )
            .with_help("keep one of them"),
        );
        if !used {
            check_preset(
                preset,
                encoder.step,
                encoder.target.as_deref(),
                at,
                layer,
                virtual_input,
                out,
            );
        }
    }
    for (field, set) in [
        ("step", encoder.step.is_some()),
        ("target", encoder.target.is_some()),
    ] {
        if set && (used || encoder.preset.is_none()) {
            out.push(
                Diagnostic::warning(
                    "W0134",
                    at(field),
                    format!("`{field}` beside `modes` is ignored"),
                )
                .with_help(format!(
                    "give each mode its own, as in modes = [{{ preset = \"volume\", {field} = ... }}]"
                )),
            );
        }
    }
    for (m, mode) in encoder.modes.iter().enumerate() {
        // A mode written as a bare name has no `preset` key to point at.
        let at = |what: &str| match what {
            "preset" => at(&format!("modes[{m}]")),
            _ => at(&format!("modes[{m}].{what}")),
        };
        check_preset(
            mode.preset,
            mode.step,
            mode.target.as_deref(),
            &at,
            layer,
            virtual_input,
            out,
        );
    }
}

/// A preset where it is used, with the step and target given to it.
///
/// A preset's own actions are checked by the model's tests; what can still
/// go wrong is what it is given and where it is used.
fn check_preset(
    preset: crate::action::Preset,
    step: Option<f64>,
    target: Option<&str>,
    at: &dyn Fn(&str) -> String,
    layer: Layer,
    virtual_input: bool,
    out: &mut Diagnostics,
) {
    use crate::action::Preset;
    if let Some(step) = step {
        check_preset_step(preset, step, &at("step"), out);
    }
    if let Some(target) = target {
        match preset.target_kind() {
            None => out.push(
                Diagnostic::warning(
                    "W0126",
                    at("target"),
                    format!("the {} preset has nothing to target", preset.name()),
                )
                .with_help(match preset {
                    Preset::Outputs => "its press mutes whichever output it has just switched to",
                    Preset::Mic => "it always turns the default microphone",
                    _ => "only volume, app_volume, tracks and seek take a target",
                }),
            ),
            Some(kind) if !kind.accepts(target) => out.push(
                Diagnostic::warning(
                    "W0138",
                    at("target"),
                    format!("{target:?} is not a valid {}", kind.noun()),
                )
                .with_help(kind.hint()),
            ),
            Some(_) => {}
        }
    }
    if preset == Preset::Profiles && layer == Layer::Page {
        out.push(
            Diagnostic::hint(
                "H0128",
                at("preset"),
                "turning through profiles from one page lands on pages whose knob may do something else",
            )
            .with_help("set it in galdeck.toml or the profile instead, so every page has it"),
        );
    }
    if !virtual_input
        && preset
            .gestures(step, target)
            .iter()
            .flatten()
            .any(crate::action::Action::needs_virtual_input)
    {
        out.push(Diagnostic::warning(
            "W0129",
            at("preset"),
            format!(
                "the {} preset types keys or moves the wheel, and virtual_input is off",
                preset.name()
            ),
        ));
    }
}

/// A preset's `step`, against the built-ins it goes to: the same warnings as
/// a step written on one of them.
fn check_preset_step(preset: crate::action::Preset, step: f64, path: &str, out: &mut Diagnostics) {
    let ranges: Vec<(crate::action::BuiltIn, f64, f64, f64)> = preset
        .gestures(None, None)
        .iter()
        .flatten()
        .filter_map(crate::action::Action::as_built_in)
        .filter_map(|invocation| {
            let (_, default, min, max) = invocation.action.step()?;
            Some((invocation.action, default, min, max))
        })
        .collect();
    let Some(&(built_in, default, min, max)) = ranges.first() else {
        out.push(Diagnostic::warning(
            "W0123",
            path,
            format!("the {} preset takes no step", preset.name()),
        ));
        return;
    };
    if !step.is_finite() {
        out.push(Diagnostic::warning(
            "W0125",
            path,
            format!("a step of {step} is not a number, so the default of {default} is used"),
        ));
    } else if !(min..=max).contains(&step) {
        out.push(Diagnostic::warning(
            "W0125",
            path,
            format!(
                "a step of {step} for {} is outside {min}-{max}, and will be clamped",
                built_in.name()
            ),
        ));
    }
}

/// One action, wherever it is.
fn check_action(
    action: &crate::action::Action,
    path: &str,
    slot: Slot,
    virtual_input: bool,
    out: &mut Diagnostics,
) {
    use crate::action::{Action, BuiltIn, SlotRule, TargetKind};
    match action {
        Action::Shell(command) => {
            if command.trim().is_empty() {
                out.push(Diagnostic::warning(
                    "W0122",
                    path,
                    "an empty command does nothing",
                ));
            }
        }
        Action::Keys(chord) => match crate::keys::parse_chord(chord) {
            Err(problem) => out.push(Diagnostic::error("E0118", path, problem.to_string())),
            Ok(codes) => {
                if codes.iter().all(|code| crate::keys::is_modifier(*code)) {
                    out.push(Diagnostic::warning(
                        "W0119",
                        path,
                        format!("{chord:?} is only modifiers, which do nothing on their own"),
                    ));
                }
                if codes.iter().any(|code| crate::keys::needs_num_lock(*code)) {
                    out.push(
                        Diagnostic::hint(
                            "H0124",
                            path,
                            "keypad digits type digits only while Num Lock is on",
                        )
                        .with_help("the deck turns Num Lock on first when it can tell it is off"),
                    );
                }
                if !virtual_input {
                    out.push(Diagnostic::warning(
                        "W0129",
                        path,
                        "keystrokes need the virtual keyboard, and virtual_input is off",
                    ));
                }
            }
        },
        Action::BuiltIn(invocation) => {
            let built_in = invocation.action;
            match (built_in.step(), invocation.step) {
                (None, Some(_)) => out.push(Diagnostic::warning(
                    "W0123",
                    path,
                    format!("{} takes no step", built_in.name()),
                )),
                (Some((_, default, _, _)), Some(step)) if !step.is_finite() => {
                    out.push(Diagnostic::warning(
                        "W0125",
                        path,
                        format!(
                            "a step of {step} for {} is not a number, so the default of {default} is used",
                            built_in.name()
                        ),
                    ))
                }
                (Some((_, _, min, max)), Some(step)) if !(min..=max).contains(&step) => {
                    out.push(Diagnostic::warning(
                        "W0125",
                        path,
                        format!(
                            "a step of {step} for {} is outside {min}-{max}, and will be clamped",
                            built_in.name()
                        ),
                    ))
                }
                _ => {}
            }
            match (built_in.target_kind(), invocation.target.as_deref()) {
                (None, Some(_)) => out.push(Diagnostic::warning(
                    "W0126",
                    path,
                    format!("{} has nothing to target", built_in.name()),
                )),
                (Some(kind), Some(target)) if !kind.accepts(target) => out.push(
                    Diagnostic::warning(
                        "W0138",
                        path,
                        format!("{target:?} is not a valid {}", kind.noun()),
                    )
                    .with_help(kind.hint()),
                ),
                _ => {}
            }
            if built_in == BuiltIn::SetOutput && invocation.target.is_none() {
                out.push(
                    Diagnostic::error(
                        "E0136",
                        path,
                        "set_output needs a `target`: the output to switch to",
                    )
                    .with_help(TargetKind::Sink.hint()),
                );
            }
            let (fits, message, help) = match built_in.slot_rule() {
                SlotRule::Anywhere => (true, "", ""),
                SlotRule::KeyTapOnly => (
                    slot == Slot::KeyTap,
                    "works only as what pressing a key does",
                    "it needs to see the key come back up",
                ),
                SlotRule::KeyOnly => (
                    matches!(slot, Slot::KeyTap | Slot::Key),
                    "works only on a key",
                    "it acts on the key it is bound to",
                ),
                SlotRule::KnobOnly => (
                    slot == Slot::Knob,
                    "works only on a knob",
                    "it acts on the knob it is bound to",
                ),
            };
            // A state's action is on a key, but it is not a gesture: it runs
            // because the key's state changed, so one that changes it again
            // would never stop. The daemon refuses these too, since a config
            // with errors still loads.
            let from_state = slot == Slot::StateEntry
                && matches!(
                    built_in.slot_rule(),
                    SlotRule::KeyTapOnly | SlotRule::KeyOnly
                );
            if from_state {
                out.push(
                    Diagnostic::error(
                        "E0189",
                        path,
                        format!(
                            "{} cannot be what a state runs as the key enters it",
                            built_in.name()
                        ),
                    )
                    .with_help(match built_in.acts_on() {
                        Some(KeyPart::States) => {
                            "entering a state that moves the key on to another would never stop"
                        }
                        Some(KeyPart::Timer) => "a key with states has no timer to act on",
                        None => "it needs a key held down, and a state's action is not one",
                    }),
                );
            } else if !fits {
                out.push(
                    Diagnostic::warning("W0127", path, format!("{} {message}", built_in.name()))
                        .with_help(help),
                );
            }
            if built_in.needs_virtual_input() && !virtual_input {
                out.push(Diagnostic::warning(
                    "W0129",
                    path,
                    format!(
                        "{} needs the virtual pointer, and virtual_input is off",
                        built_in.name()
                    ),
                ));
            }
        }
    }
}

/// Whether a name has the shape of an IANA zone: `UTC`, or `Area/Place`.
///
/// Only the shape: whether the zone exists is the tz database's to say, and
/// that is the daemon's business, not the model's.
fn looks_like_zone(zone: &str) -> bool {
    let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/');
    zone.chars().all(plain)
        && !zone.starts_with('/')
        && !zone.ends_with('/')
        && (zone.contains('/')
            || zone.eq_ignore_ascii_case("utc")
            || zone.eq_ignore_ascii_case("gmt"))
}

/// A grid dimension outside what is legible.
fn check_grid_size(value: Option<u8>, max: u8, path: &str, out: &mut Diagnostics) {
    if let Some(value) = value {
        if value == 0 || value > max {
            out.push(Diagnostic::warning(
                "W0166",
                path,
                format!("{value} is outside 1-{max}, and will be clamped"),
            ));
        }
    }
}

/// A background that would draw nothing, or not what was asked for.
fn check_backdrop(backdrop: &crate::backdrop::Backdrop, path: &str, out: &mut Diagnostics) {
    use crate::backdrop::{MAX_DIM, MAX_FPS, MIN_FPS};
    let at = |what: &str| format!("{path}.{what}");
    if backdrop.is_empty() {
        out.push(
            Diagnostic::warning("W0170", path, "a background with nothing to show")
                .with_help("give it an `image`, or an `animation` such as \"aurora\""),
        );
    }
    if backdrop.image.is_some() && backdrop.animation.is_some() {
        out.push(Diagnostic::warning(
            "W0171",
            at("animation"),
            "a background has an image and an animation; the image is shown",
        ));
    }
    if backdrop.animation.is_none() && !backdrop.colors.is_empty() {
        out.push(Diagnostic::warning(
            "W0172",
            at("colors"),
            "`colors` only colours an animation",
        ));
    }
    if backdrop.colors.len() > 4 {
        out.push(Diagnostic::warning(
            "W0173",
            at("colors"),
            format!(
                "{} colours given; an animation uses the first four",
                backdrop.colors.len()
            ),
        ));
    }
    if let Some(fps) = backdrop.fps {
        if !(MIN_FPS..=MAX_FPS).contains(&fps) {
            out.push(
                Diagnostic::warning(
                    "W0174",
                    at("fps"),
                    format!("{fps} fps is outside {MIN_FPS}-{MAX_FPS} and will be clamped"),
                )
                .with_help("every frame is a JPEG per surface; the USB link sets the ceiling"),
            );
        }
    }
    if backdrop
        .dim
        .is_some_and(|dim| !(0.0..=MAX_DIM).contains(&dim))
    {
        out.push(Diagnostic::warning(
            "W0175",
            at("dim"),
            format!("`dim` is 0 to {MAX_DIM}, and will be clamped"),
        ));
    }
}

/// Settings that are missing, or that the widget's kind would ignore.
///
/// `on_screen` for a tile, which always has a card behind it for `opacity`
/// to fade.
fn check_widget(
    widget: &crate::widget::Widget,
    path: &str,
    on_screen: bool,
    virtual_input: bool,
    out: &mut Diagnostics,
) {
    use crate::widget::{WidgetKind, WidgetView};
    let at = |what: &str| format!("{path}.{what}");
    let kind = widget.kind;

    if kind == WidgetKind::Timer {
        check_duration(widget, &at("duration"), out);
    }
    if kind.is_timer() && on_screen {
        out.push(
            Diagnostic::warning(
                "W0182",
                at("kind"),
                format!(
                    "a {} on the screen can never start: it starts when its key is tapped",
                    kind.name()
                ),
            )
            .with_help("put it on a key"),
        );
    }
    if let Some(action) = &widget.on_done {
        if kind != WidgetKind::Timer {
            out.push(Diagnostic::warning(
                "W0185",
                at("on_done"),
                format!(
                    "only a timer finishes, so {} widgets never run `on_done`",
                    kind.name()
                ),
            ));
        }
        check_action(action, &at("on_done"), Slot::Other, virtual_input, out);
    }

    if kind == WidgetKind::Command && widget.command.as_deref().unwrap_or_default().is_empty() {
        out.push(Diagnostic::error(
            "E0142",
            at("command"),
            "a command widget needs a `command` to run",
        ));
    }
    if kind != WidgetKind::Command && widget.command.is_some() {
        out.push(Diagnostic::warning(
            "W0143",
            at("command"),
            format!("{} widgets ignore `command`", kind.name()),
        ));
    }

    if kind == WidgetKind::Weather {
        match (widget.latitude, widget.longitude) {
            (Some(latitude), Some(longitude)) => {
                if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
                    out.push(Diagnostic::error(
                        "E0145",
                        at("latitude"),
                        format!("{latitude}, {longitude} is not a place on Earth"),
                    ));
                }
            }
            _ => out.push(
                Diagnostic::error(
                    "E0144",
                    at("latitude"),
                    "a weather widget needs a `latitude` and a `longitude`",
                )
                .with_help("in decimal degrees, e.g. latitude = 51.5, longitude = -0.13"),
            ),
        }
    } else if widget.latitude.is_some() || widget.longitude.is_some() {
        out.push(Diagnostic::warning(
            "W0146",
            at("latitude"),
            format!("{} widgets ignore a location", kind.name()),
        ));
    }

    if let Some(zone) = &widget.timezone {
        if !kind.is_time() {
            out.push(Diagnostic::warning(
                "W0164",
                at("timezone"),
                format!("{} widgets have no time to tell", kind.name()),
            ));
        } else if !zone.is_empty() && !looks_like_zone(zone) {
            out.push(
                Diagnostic::warning(
                    "W0165",
                    at("timezone"),
                    format!("{zone:?} does not look like a time zone"),
                )
                .with_help("an IANA name, such as \"Europe/London\" or \"Asia/Tokyo\""),
            );
        }
    }
    if widget.source.is_some() && !kind.takes_source() {
        out.push(Diagnostic::warning(
            "W0147",
            at("source"),
            format!("{} widgets ignore `source`", kind.name()),
        ));
    }
    if widget.units.is_some() && !matches!(kind, WidgetKind::Temperature | WidgetKind::Weather) {
        out.push(Diagnostic::warning(
            "W0148",
            at("units"),
            format!("{} widgets have no temperature to convert", kind.name()),
        ));
    }
    if !widget.view().suits(kind) {
        let help = match (kind, widget.view()) {
            (WidgetKind::Weather | WidgetKind::Media, _) => "it always draws as a card",
            (WidgetKind::Timer, _) => "a timer draws as text, a bar or a gauge",
            (WidgetKind::Stopwatch, _) => "a stopwatch draws as text",
            (_, WidgetView::Analog | WidgetView::Nixie) => "only a clock has a face",
            _ => "only widgets that measure something can be graphed",
        };
        out.push(
            Diagnostic::warning(
                "W0149",
                at("view"),
                format!(
                    "{} widgets cannot be drawn as a {:?}",
                    kind.name(),
                    widget.view()
                )
                .to_lowercase(),
            )
            .with_help(help),
        );
    }
    if widget.opacity.is_some_and(|o| !(0.0..=1.0).contains(&o)) {
        out.push(Diagnostic::warning(
            "W0176",
            at("opacity"),
            "`opacity` is 0 to 1, and will be clamped",
        ));
    }
    if !on_screen
        && widget.opacity.is_some()
        && widget.background().is_none()
        && widget.image.is_none()
    {
        out.push(Diagnostic::warning(
            "W0177",
            at("opacity"),
            "`opacity` is for a `background` or an `image`, and this widget has neither",
        ));
    }
    // Its own drawing styles, checked the way a theme's are. View and
    // opacity are checked above, in words about this widget.
    crate::look::WidgetLook {
        segments: widget.segments,
        sweep: widget.sweep,
        thickness: widget.thickness,
        radius: widget.radius,
        ..crate::look::WidgetLook::default()
    }
    .check(path, Some(kind), out);
    if (widget.warn.is_some() || widget.critical.is_some()) && !kind.is_numeric() {
        out.push(Diagnostic::warning(
            "W0178",
            at("warn"),
            format!("{} widgets have no number to warn about", kind.name()),
        ));
    }
    if widget.place.is_some() && kind != WidgetKind::Weather {
        out.push(Diagnostic::warning(
            "W0179",
            at("place"),
            format!("{} widgets have no place to show", kind.name()),
        ));
    }
    if widget.max.is_some_and(|max| max <= 0.0) {
        out.push(Diagnostic::warning(
            "W0163",
            at("max"),
            "a `max` of zero or less is ignored",
        ));
    }
}

/// A timer's `duration`: there, readable, and a length that makes sense.
fn check_duration(widget: &crate::widget::Widget, path: &str, out: &mut Diagnostics) {
    const FORMS: &str = "such as \"25m\", \"1h 30m\", \"90s\", or \"4:30\" for minutes and seconds";
    const DAY: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);
    let Some(text) = widget.duration.as_deref() else {
        out.push(Diagnostic::error("E0180", path, "a timer needs a `duration`").with_help(FORMS));
        return;
    };
    match widget.duration() {
        None => out.push(
            Diagnostic::error("E0180", path, format!("{text:?} is not a duration"))
                .with_help(FORMS),
        ),
        Some(length) if length > DAY => out.push(Diagnostic::warning(
            "W0181",
            path,
            format!("a timer of {text} is longer than a day"),
        )),
        Some(length) if length < std::time::Duration::from_secs(1) => {
            out.push(Diagnostic::warning(
                "W0181",
                path,
                format!("a timer of {text} is done as soon as it starts"),
            ))
        }
        Some(_) => {}
    }
}

fn seen_pages_of(profile: &Profile) -> Vec<&str> {
    profile.pages.iter().map(|page| page.id.as_str()).collect()
}

fn nearest(target: &str, candidates: &[&str]) -> String {
    let best = candidates
        .iter()
        .map(|c| (crate::edit_distance(target, c), *c))
        .filter(|(d, _)| *d <= 3)
        .min_by_key(|(d, _)| *d);
    match best {
        Some((_, name)) => format!("did you mean {name:?}?"),
        None if candidates.is_empty() => "none are defined".to_string(),
        None => format!("known: {}", candidates.join(", ")),
    }
}

/// Copy every field the overlay sets onto the base.
fn overlay(base: &mut StyleLayer, over: &StyleLayer) {
    *base = base.over(over);
}

fn literal(value: &str) -> Option<ColorRef> {
    ColorRef::parse(value).ok()
}

fn read_toml<T: serde::de::DeserializeOwned>(
    path: &Path,
    label: &str,
    override_text: Option<&str>,
    out: &mut Diagnostics,
) -> Option<T> {
    let text = match override_text {
        Some(text) => text.to_string(),
        None => match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) => {
                out.push(Diagnostic::error(
                    "E0002",
                    label,
                    format!("reading {}: {e}", path.display()),
                ));
                return None;
            }
        },
    };
    match toml::from_str::<T>(&text) {
        Ok(value) => Some(value),
        Err(e) => {
            let index = LineIndex::new(&text);
            let mut diagnostic = Diagnostic::error("E0001", label, e.message().to_string());
            if let Some(span) = e.span() {
                diagnostic = diagnostic.at(span, &index);
            }
            out.push(diagnostic);
            None
        }
    }
}

/// Read every `.toml` in a directory, keyed by filename stem.
fn read_dir_of<T: serde::de::DeserializeOwned>(
    label: &str,
    root: &Path,
    overrides: &BTreeMap<String, String>,
    out: &mut Diagnostics,
) -> BTreeMap<String, T> {
    let dir = root.join(label);
    let mut paths: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
            .collect(),
        // A missing directory is not an error: a config may have no themes.
        Err(_) => Vec::new(),
    };
    // An override may name a file that does not exist on disk yet.
    for name in overrides.keys() {
        if let Some(stem) = name.strip_prefix(&format!("{label}/")) {
            let path = dir.join(stem);
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    // Sorted so loading is deterministic, which keeps diagnostics stable.
    paths.sort();

    let mut found = BTreeMap::new();
    for path in paths {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let relative = format!("{label}/{stem}.toml");
        let at = format!("{label}.{stem}");
        if let Some(value) = read_toml::<T>(
            &path,
            &at,
            overrides.get(&relative).map(String::as_str),
            out,
        ) {
            found.insert(stem.to_string(), value);
        }
    }
    found
}

/// Every configuration file in a directory, by its path relative to it.
pub fn config_file_names(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if dir.join("galdeck.toml").is_file() {
        names.push("galdeck.toml".to_string());
    }
    for sub in ["profiles", "themes"] {
        let Ok(entries) = std::fs::read_dir(dir.join(sub)) else {
            continue;
        };
        let mut found: Vec<String> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
            .filter_map(|path| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| format!("{sub}/{n}"))
            })
            .collect();
        found.sort();
        names.extend(found);
    }
    names
}
