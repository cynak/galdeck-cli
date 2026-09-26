//! Themes are only worth having if one change moves everything downstream, so
//! the cascade and the token resolution are what get tested.

use galdeck::Rgb;
use galdeck_model::theme::{resolve, StyleSource};
use galdeck_model::{Diagnostics, ResolvedStyle, StyleLayer, Workspace};

fn workspace_from(files: &[(&str, &str)]) -> (tempdir::Dir, Workspace) {
    let dir = tempdir::Dir::new();
    for (path, body) in files {
        dir.write(path, body);
    }
    let (workspace, diagnostics) = Workspace::load(dir.path());
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == galdeck_model::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
    (dir, workspace.expect("workspace should load"))
}

mod tempdir {
    use std::path::{Path, PathBuf};

    /// A throwaway directory. Named by process and a counter rather than a
    /// random number, so the crate stays dependency-free and the paths are
    /// reproducible in a failure message.
    pub struct Dir(PathBuf);

    impl Dir {
        pub fn new() -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("galdeck-model-test-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("temp dir");
            Self(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }

        pub fn write(&self, rel: &str, body: &str) {
            let path = self.0.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("temp subdir");
            }
            std::fs::write(path, body).expect("write fixture");
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

const GLOBAL: &str = "version = 2\nbrightness = 60\nprofile = \"work\"\n";

#[test]
fn a_more_specific_layer_wins_field_by_field() {
    let mut out = Diagnostics::new();
    let palette = Default::default();

    let theme = StyleLayer {
        key_label_size: Some(20.0),
        key_label_strip: Some(30),
        ..StyleLayer::default()
    };
    let page = StyleLayer {
        key_label_size: Some(40.0),
        ..StyleLayer::default()
    };

    let (style, from) = resolve(
        &[(StyleSource::Theme, &theme), (StyleSource::Page, &page)],
        &palette,
        "test",
        &mut out,
    );
    assert_eq!(style.key_label_size, 40.0, "the page overrides the theme");
    assert_eq!(style.key_label_strip, 30, "and leaves what it did not set");
    assert_eq!(from.key_label_size, StyleSource::Page);
    assert_eq!(from.key_label_strip, StyleSource::Theme);
    assert_eq!(
        from.lcd_text_size,
        StyleSource::Builtin,
        "untouched fields still say where they came from"
    );
    assert!(out.is_empty());
}

#[test]
fn an_empty_cascade_is_exactly_the_old_hardcoded_look() {
    // A config with no theme at all must look precisely as it did before
    // themes existed, or every upgrade is a visual regression.
    let mut out = Diagnostics::new();
    let (style, _) = resolve(&[], &Default::default(), "test", &mut out);
    assert_eq!(style, ResolvedStyle::BUILTIN);
    assert_eq!(style.key_label_size, 26.0);
    assert_eq!(style.key_label_strip, 36);
    assert_eq!(style.lcd_text_size, 56.0);
}

#[test]
fn tokens_resolve_through_other_tokens() {
    let (_dir, workspace) = workspace_from(&[
        ("galdeck.toml", GLOBAL),
        (
            "themes/nord.toml",
            r##"
[palette]
frost = "#88c0d0"
accent = "@frost"
brand = "@accent"

[style]
key_bg = "@brand"
"##,
        ),
        (
            "profiles/work.toml",
            "theme = \"nord\"\n[[pages]]\nid = \"main\"\n",
        ),
    ]);

    let mut out = Diagnostics::new();
    let (style, palette) = workspace.theme_for(Some("nord"), &mut out);
    assert!(out.is_empty(), "{}", out.render());
    assert_eq!(palette.get("brand"), Some(Rgb::new(0x88, 0xc0, 0xd0)));

    let profile = workspace.profile("work").unwrap();
    let page = &profile.pages[0];
    let (resolved, from) =
        workspace.style_for(&style, &palette, profile, page, None, "key", &mut out);
    assert_eq!(resolved.key_bg, Rgb::new(0x88, 0xc0, 0xd0));
    assert_eq!(from.key_bg, StyleSource::Theme);
}

#[test]
fn a_palette_cycle_is_reported_with_the_loop() {
    let dir = tempdir::Dir::new();
    dir.write("galdeck.toml", GLOBAL);
    dir.write(
        "themes/broken.toml",
        "[palette]\naccent = \"@brand\"\nbrand = \"@accent\"\n",
    );
    dir.write(
        "profiles/work.toml",
        "theme = \"broken\"\n[[pages]]\nid = \"main\"\n",
    );

    let (_, diagnostics) = Workspace::load(dir.path());
    let cycle = diagnostics
        .iter()
        .find(|d| d.code == "E0116")
        .expect("a cycle should be reported");
    let help = cycle.help.as_deref().unwrap_or_default();
    assert!(
        help.contains("@accent") && help.contains("@brand"),
        "the loop should be printed, got {help:?}"
    );
}

#[test]
fn an_unknown_token_suggests_a_near_miss() {
    let dir = tempdir::Dir::new();
    dir.write("galdeck.toml", GLOBAL);
    dir.write(
        "themes/nord.toml",
        "[palette]\naccent = \"#88c0d0\"\n\n[style]\nkey_bg = \"@accnt\"\n",
    );
    dir.write(
        "profiles/work.toml",
        "theme = \"nord\"\n[[pages]]\nid = \"main\"\n",
    );

    let (_, diagnostics) = Workspace::load(dir.path());
    let unknown = diagnostics
        .iter()
        .find(|d| d.code == "E0117")
        .expect("an unknown token should be reported");
    assert!(
        unknown
            .help
            .as_deref()
            .unwrap_or_default()
            .contains("accent"),
        "got {:?}",
        unknown.help
    );
}

#[test]
fn extends_folds_parents_in_with_the_child_winning() {
    let (_dir, workspace) = workspace_from(&[
        ("galdeck.toml", GLOBAL),
        (
            "themes/base.toml",
            r##"
[palette]
ink = "#101010"
paper = "#f0f0f0"

[style]
key_bg = "@ink"
key_label_size = 22.0
"##,
        ),
        (
            "themes/bright.toml",
            r##"
extends = "base"

[palette]
ink = "#202080"

[style]
key_label_size = 30.0
"##,
        ),
        (
            "profiles/work.toml",
            "theme = \"bright\"\n[[pages]]\nid = \"main\"\n",
        ),
    ]);

    let mut out = Diagnostics::new();
    let (style, palette) = workspace.theme_for(Some("bright"), &mut out);
    assert!(out.is_empty(), "{}", out.render());
    // The child redefines `ink` and the parent's `key_bg = "@ink"` follows it.
    assert_eq!(palette.get("ink"), Some(Rgb::new(0x20, 0x20, 0x80)));
    assert_eq!(palette.get("paper"), Some(Rgb::new(0xf0, 0xf0, 0xf0)));
    assert_eq!(style.key_label_size, Some(30.0));
    assert!(style.key_bg.is_some(), "inherited from the parent");
}

#[test]
fn a_theme_that_extends_itself_is_caught() {
    let dir = tempdir::Dir::new();
    dir.write("galdeck.toml", GLOBAL);
    dir.write("themes/loop.toml", "extends = \"loop\"\n");
    dir.write(
        "profiles/work.toml",
        "theme = \"loop\"\n[[pages]]\nid = \"main\"\n",
    );

    let (_, diagnostics) = Workspace::load(dir.path());
    assert!(
        diagnostics.iter().any(|d| d.code == "E0114"),
        "{diagnostics:#?}"
    );
}

#[test]
fn a_chain_runs_from_the_theme_up_through_what_it_extends() {
    let (_dir, workspace) = workspace_from(&[
        ("galdeck.toml", GLOBAL),
        ("themes/base.toml", "name = \"Base\"\n"),
        ("themes/middle.toml", "extends = \"base\"\n"),
        ("themes/mine.toml", "extends = \"middle\"\n"),
        (
            "profiles/work.toml",
            "theme = \"mine\"\n[[pages]]\nid = \"main\"\n",
        ),
    ]);
    let ids = |id: &str| -> Vec<String> {
        workspace
            .theme_chain(id)
            .into_iter()
            .map(|(id, _)| id.to_string())
            .collect()
    };
    assert_eq!(ids("mine"), ["mine", "middle", "base"]);
    assert_eq!(ids("base"), ["base"]);
    assert!(ids("nope").is_empty());
}

#[test]
fn a_chain_stops_at_a_loop_instead_of_going_round_it() {
    let dir = tempdir::Dir::new();
    dir.write("galdeck.toml", GLOBAL);
    dir.write("themes/ping.toml", "extends = \"pong\"\n");
    dir.write("themes/pong.toml", "extends = \"ping\"\n");
    dir.write(
        "profiles/work.toml",
        "theme = \"ping\"\n[[pages]]\nid = \"main\"\n",
    );
    let (workspace, _) = Workspace::load(dir.path());
    let workspace = workspace.expect("a loop is an error in the themes, not in the files");
    let chain: Vec<&str> = workspace
        .theme_chain("ping")
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(chain, ["ping", "pong"]);
}

#[test]
fn every_style_field_is_listed_under_the_name_it_is_written_as() {
    use galdeck_model::theme::StyleValue;
    // Write each field on its own, by the name `fields` gives it; parsing
    // refuses an unknown name, and exactly that field has to come back set.
    for (name, value) in StyleLayer::default().fields() {
        let sample = match value {
            StyleValue::Color(_) => "\"#102030\"",
            StyleValue::Size(_) => "12.5",
            StyleValue::Pixels(_) => "40",
        };
        let layer: StyleLayer = toml::from_str(&format!("{name} = {sample}"))
            .unwrap_or_else(|e| panic!("{name} is not a style field: {e}"));
        let set: Vec<&str> = layer
            .fields()
            .into_iter()
            .filter(|(_, value)| value.is_set())
            .map(|(name, _)| name)
            .collect();
        assert_eq!(set, [name]);
    }
    let written: Vec<&str> = StyleLayer::default().fields().map(|(n, _)| n).into();
    let resolved: Vec<&str> = ResolvedStyle::BUILTIN.fields().map(|(n, _)| n).into();
    assert_eq!(written, resolved);
}

#[test]
fn a_config_from_the_future_is_refused_rather_than_guessed_at() {
    let dir = tempdir::Dir::new();
    dir.write("galdeck.toml", "version = 99\n");
    let (workspace, diagnostics) = Workspace::load(dir.path());
    assert!(workspace.is_none());
    let refusal = diagnostics.iter().find(|d| d.code == "E0003").unwrap();
    assert!(refusal.help.as_deref().unwrap().contains("newer galdeck"));
}

#[test]
fn a_pulse_rises_and_falls_over_its_cycle() {
    use galdeck_model::{Animation, AnimationKind};

    let pulse = Animation {
        kind: AnimationKind::Pulse,
        period_ms: 1000,
        to: None,
        frames: 4,
    };
    // Quarter, half, three-quarters: up to the top and back down.
    assert_eq!(pulse.mix_for_frame(0), 0.0);
    assert_eq!(pulse.mix_for_frame(1), 0.5);
    assert_eq!(pulse.mix_for_frame(2), 1.0);
    assert_eq!(pulse.mix_for_frame(3), 0.5);
    assert_eq!(pulse.frame_interval_ms(), 250);
}

#[test]
fn breathing_lingers_at_both_ends() {
    use galdeck_model::{Animation, AnimationKind};

    // The difference from a pulse: a raised cosine has no corner at the top
    // or the bottom, which is what makes it read as breathing rather than as
    // a triangle wave.
    let breathe = Animation {
        kind: AnimationKind::Breathe,
        period_ms: 1000,
        to: None,
        frames: 8,
    };
    let first_step = breathe.mix_for_frame(1) - breathe.mix_for_frame(0);
    let middle_step = breathe.mix_for_frame(3) - breathe.mix_for_frame(2);
    assert!(
        middle_step > first_step * 1.5,
        "it should move fastest through the middle: {first_step} then {middle_step}"
    );
}

#[test]
fn a_blink_has_exactly_two_frames_however_many_are_asked_for() {
    use galdeck_model::{Animation, AnimationKind};

    let blink = Animation {
        kind: AnimationKind::Blink,
        period_ms: 1000,
        to: None,
        frames: 30,
    };
    // Any more would be identical copies, each costing a JPEG encode.
    assert_eq!(blink.frames(), 2);
    assert_eq!(blink.mix_for_frame(0), 1.0);
    assert_eq!(blink.mix_for_frame(1), 0.0);
}

#[test]
fn an_impossible_period_is_clamped_and_said_out_loud() {
    let dir = tempdir::Dir::new();
    dir.write("galdeck.toml", GLOBAL);
    dir.write(
        "profiles/work.toml",
        r##"
[[pages]]
id = "main"

[[pages.keys]]
key = 0
label = "Fast"
exec = "true"

[pages.keys.animation]
kind = "pulse"
period_ms = 5
"##,
    );

    let (workspace, diagnostics) = Workspace::load(dir.path());
    assert!(workspace.is_some(), "clamping is not a refusal");
    let warned = diagnostics
        .iter()
        .find(|d| d.code == "W0141")
        .expect("clamping silently would make the config stop meaning what it says");
    assert!(warned.message.contains("clamped"));
}

#[test]
fn a_ring_only_animation_on_a_key_is_an_error() {
    let dir = tempdir::Dir::new();
    dir.write("galdeck.toml", GLOBAL);
    dir.write(
        "profiles/work.toml",
        r##"
[[pages]]
id = "main"

[[pages.keys]]
key = 0
label = "Spin"
exec = "true"

[pages.keys.animation]
kind = "comet"
"##,
    );

    let (_, diagnostics) = Workspace::load(dir.path());
    let error = diagnostics.iter().find(|d| d.code == "E0140").unwrap();
    assert!(error.help.as_deref().unwrap().contains("pulse"));
}

/// The widget on key `key` of page `page` in profile `work`.
fn widget_on(workspace: &Workspace, page: usize, key: u8) -> &galdeck_model::Widget {
    workspace.profiles["work"].pages[page]
        .keys
        .iter()
        .find(|k| k.key == key)
        .and_then(|k| k.widget.as_ref())
        .expect("a widget on that key")
}

const LOOKS_PROFILE: &str = r##"
theme = "retro"

[widgets]
color = "#222222"

[[pages]]
id = "main"
lcd = [{ column = 0, row = 0, widget = { kind = "clock", view = "analog" } }]

[pages.widgets.cpu]
color = "#333333"

[[pages.keys]]
key = 0
widget = { kind = "clock" }

[[pages.keys]]
key = 1
widget = { kind = "cpu", view = "graph" }

[[pages.keys]]
key = 2
widget = { kind = "memory", color = "#444444" }

[[pages.keys]]
key = 3
widget = { kind = "date" }

[[pages]]
id = "second"

[[pages.keys]]
key = 0
widget = { kind = "cpu" }
"##;

const RETRO: &str = r##"
[palette]
amber = "#ffb000"

[widgets]
color = "#111111"
view = "gauge"
graph = "line"
bar = "segmented"
segments = 12

[widgets.clock]
view = "nixie"
color = "@amber"
"##;

#[test]
fn a_theme_says_how_widgets_look_and_a_widget_may_say_otherwise() {
    use galdeck_model::{GraphStyle, WidgetView};
    let (_dir, workspace) = workspace_from(&[
        ("galdeck.toml", GLOBAL),
        ("themes/retro.toml", RETRO),
        ("profiles/work.toml", LOOKS_PROFILE),
    ]);

    // Every clock is a nixie clock...
    let clock = widget_on(&workspace, 0, 0);
    assert_eq!(clock.view(), WidgetView::Nixie);
    // ...except one that says it is analog.
    let tile = &workspace.profiles["work"].pages[0].lcd[0].widget;
    assert_eq!(tile.view(), WidgetView::Analog);

    // The theme's graph style reaches a graph, and its own view wins.
    let cpu = widget_on(&workspace, 0, 1);
    assert_eq!(cpu.view(), WidgetView::Graph);
    assert_eq!(cpu.graph_style(), GraphStyle::Line);
    assert_eq!(cpu.segments(), 12);

    // `view = "gauge"` for every widget is only for widgets that can be one.
    assert_eq!(widget_on(&workspace, 0, 2).view(), WidgetView::Gauge);
    assert_eq!(widget_on(&workspace, 0, 3).view(), WidgetView::Text);

    // What a widget says is never replaced, and is all that is its own.
    assert_eq!(cpu.view, Some(WidgetView::Graph));
    assert_eq!(clock.view, None, "the look is not written into the widget");
}

#[test]
fn a_page_goes_over_its_profile_and_a_profile_over_its_theme() {
    let (_dir, workspace) = workspace_from(&[
        ("galdeck.toml", GLOBAL),
        ("themes/retro.toml", RETRO),
        ("profiles/work.toml", LOOKS_PROFILE),
    ]);
    let colour = |page: usize, key: u8| {
        widget_on(&workspace, page, key)
            .color()
            .cloned()
            .expect("a colour")
    };
    let hex = |text: &str| galdeck_model::ColorRef::parse(text).unwrap();
    // The page says cpu widgets are #333333.
    assert_eq!(colour(0, 1), hex("#333333"));
    // The profile says every widget is #222222, over the theme's #111111.
    assert_eq!(colour(1, 0), hex("#222222"));
    // A widget's own colour beats them all.
    assert_eq!(colour(0, 2), hex("#444444"));
    // And the profile's every-widget colour beats the theme's clock colour:
    // a nearer layer wins, whatever it is written for.
    assert_eq!(colour(0, 0), hex("#222222"));
}

#[test]
fn widget_looks_are_checked_where_they_are_written() {
    let dir = tempdir::Dir::new();
    dir.write("galdeck.toml", GLOBAL);
    dir.write(
        "themes/t.toml",
        "[widgets]\ncolor = \"@nope\"\n\n[widgets.cpu]\nview = \"analog\"\nsweep = 20\n",
    );
    dir.write(
        "profiles/work.toml",
        "theme = \"t\"\n[[pages]]\nid = \"main\"\n[[pages.keys]]\nkey = 0\nwidget = { kind = \"cpu\", view = \"gauge\", thickness = 0.9 }\n",
    );
    let (workspace, diagnostics) = Workspace::load(dir.path());
    assert!(workspace.is_some());
    let at = |code: &str| {
        diagnostics
            .iter()
            .filter(|d| d.code == code)
            .map(|d| d.path.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(at("E0117"), ["themes.t.widgets.color"]);
    assert_eq!(at("W0220"), ["themes.t.widgets.cpu.view"]);
    assert_eq!(at("W0223"), ["themes.t.widgets.cpu.sweep"]);
    assert_eq!(
        at("W0224"),
        ["profiles.work.pages[0].keys[0].widget.thickness"]
    );
}
