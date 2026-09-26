//! Widgets and the info screen's tiles: what loads, and what is said about
//! settings that are missing or would be ignored.

use galdeck_model::{Diagnostic, WidgetKind, WidgetView, Workspace};

type Diagnostics = Vec<Diagnostic>;

const GLOBAL: &str = "version = 2\n";

/// Load a one-page profile and return its diagnostics.
fn load(page: &str) -> (Option<Workspace>, Diagnostics) {
    let dir = std::env::temp_dir().join(format!(
        "galdeck-model-widget-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("profiles")).unwrap();
    std::fs::write(dir.join("galdeck.toml"), GLOBAL).unwrap();
    std::fs::write(
        dir.join("profiles/work.toml"),
        format!("[[pages]]\nid = \"main\"\n{page}"),
    )
    .unwrap();
    let loaded = Workspace::load(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    loaded
}

fn codes(diagnostics: &Diagnostics) -> Vec<&str> {
    diagnostics.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn a_screen_of_tiles_loads() {
    let (workspace, diagnostics) = load(
        r##"
[[pages.lcd]]
column = 0
row = 0
columns = 6
rows = 2

[pages.lcd.widget]
kind = "weather"
latitude = 51.5
longitude = -0.13
units = "fahrenheit"

[[pages.lcd]]
column = 6
row = 0
columns = 6
rows = 2

[pages.lcd.widget]
kind = "temperature"
source = "k10temp"
view = "graph"
color = "#ff0000"
"##,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let workspace = workspace.unwrap();
    let page = &workspace.profile("work").unwrap().pages[0];
    assert_eq!(page.lcd.len(), 2);
    assert_eq!(page.lcd[0].widget.kind, WidgetKind::Weather);
    assert_eq!(page.lcd[1].widget.view(), WidgetView::Graph);
}

#[test]
fn a_tile_defaults_to_the_whole_screen() {
    let (workspace, _) = load(
        r##"
[[pages.lcd]]
column = 0
row = 0

[pages.lcd.widget]
kind = "clock"
"##,
    );
    let workspace = workspace.unwrap();
    let tile = &workspace.profile("work").unwrap().pages[0].lcd[0];
    let grid = galdeck_model::LcdGrid::default();
    let cells = tile.cells(grid);
    assert_eq!((cells.columns, cells.rows), (12, 6));
    assert!(tile.fits(grid));
}

#[test]
fn a_tile_off_the_screen_is_an_error() {
    let (_, diagnostics) = load(
        r##"
[[pages.lcd]]
column = 8
row = 0
columns = 6
rows = 2

[pages.lcd.widget]
kind = "clock"
"##,
    );
    let error = diagnostics.iter().find(|d| d.code == "E0160").unwrap();
    assert!(error.help.as_deref().unwrap().contains("12 columns"));
}

#[test]
fn overlapping_tiles_are_a_warning_and_text_under_tiles_a_hint() {
    let (_, diagnostics) = load(
        r##"
lcd_text = "hidden"

[[pages.lcd]]
column = 0
row = 0
columns = 6
rows = 3

[pages.lcd.widget]
kind = "clock"

[[pages.lcd]]
column = 4
row = 2
columns = 4
rows = 2

[pages.lcd.widget]
kind = "date"
"##,
    );
    let found = codes(&diagnostics);
    assert!(found.contains(&"W0161"), "{found:?}");
    assert!(found.contains(&"H0162"), "{found:?}");
}

#[test]
fn weather_needs_somewhere_to_be_about() {
    let (_, diagnostics) = load(
        r##"
[[pages.lcd]]
column = 0
row = 0

[pages.lcd.widget]
kind = "weather"
latitude = 51.5
"##,
    );
    assert!(codes(&diagnostics).contains(&"E0144"));

    let (_, diagnostics) = load(
        r##"
[[pages.lcd]]
column = 0
row = 0

[pages.lcd.widget]
kind = "weather"
latitude = 151.5
longitude = 0.0
"##,
    );
    assert!(codes(&diagnostics).contains(&"E0145"));
}

#[test]
fn settings_a_kind_would_ignore_are_pointed_out() {
    let (_, diagnostics) = load(
        r##"
[[pages.keys]]
key = 0
exec = "true"

[pages.keys.widget]
kind = "clock"
source = "eth0"
units = "celsius"
latitude = 1.0
view = "graph"
max = 0
"##,
    );
    let found = codes(&diagnostics);
    for code in ["W0146", "W0147", "W0148", "W0149", "W0163"] {
        assert!(found.contains(&code), "missing {code} in {found:?}");
    }
}

#[test]
fn a_graphed_command_is_fine_but_a_graphed_card_is_not() {
    let (_, diagnostics) = load(
        r##"
[[pages.keys]]
key = 0
exec = "true"

[pages.keys.widget]
kind = "command"
command = "echo 42"
view = "bar"

[[pages.keys]]
key = 1
exec = "true"

[pages.keys.widget]
kind = "media"
view = "graph"
"##,
    );
    let warnings: Vec<_> = diagnostics.iter().filter(|d| d.code == "W0149").collect();
    assert_eq!(warnings.len(), 1, "{warnings:#?}");
    assert!(warnings[0].path.contains("keys[1]"));
}

#[test]
fn weather_is_never_asked_more_than_once_a_minute() {
    let widget = galdeck_model::Widget {
        interval_ms: Some(1000),
        ..galdeck_model::Widget::of(WidgetKind::Weather)
    };
    assert_eq!(widget.interval_ms(), 60_000);
}

/// Load a workspace from whole files, for backgrounds set at several levels.
fn load_files(files: &[(&str, &str)]) -> (Option<Workspace>, Diagnostics) {
    let dir = std::env::temp_dir().join(format!(
        "galdeck-model-backdrop-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, body) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
    let loaded = Workspace::load(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    loaded
}

#[test]
fn the_nearest_background_wins_page_then_profile_then_theme() {
    let theme = "[background]\nanimation = \"aurora\"\n";
    let profile = r##"
theme = "moody"

[background]
image = "/profile.png"

[[pages]]
id = "own"

[pages.background]
image = "/page.gif"
span = "keys"

[[pages]]
id = "inherits"
"##;
    let (workspace, diagnostics) = load_files(&[
        ("galdeck.toml", GLOBAL),
        ("themes/moody.toml", theme),
        ("profiles/work.toml", profile),
        (
            "profiles/plain.toml",
            "theme = \"moody\"\n[[pages]]\nid = \"main\"\n",
        ),
    ]);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let workspace = workspace.unwrap();

    let work = workspace.profile("work").unwrap();
    let own = workspace.background_for(work, &work.pages[0]).unwrap();
    assert_eq!(
        own.image.as_deref(),
        Some(std::path::Path::new("/page.gif"))
    );
    assert_eq!(own.span, galdeck_model::Span::Keys);
    let inherited = workspace.background_for(work, &work.pages[1]).unwrap();
    assert_eq!(
        inherited.image.as_deref(),
        Some(std::path::Path::new("/profile.png"))
    );

    let plain = workspace.profile("plain").unwrap();
    let themed = workspace.background_for(plain, &plain.pages[0]).unwrap();
    assert_eq!(themed.animation, Some(galdeck_model::Motion::Aurora));
    assert_eq!(themed.span, galdeck_model::Span::Both);
}

#[test]
fn a_background_that_cannot_be_what_was_meant_is_pointed_out() {
    let (_, diagnostics) = load(
        r##"
[pages.background]
image = "/a.png"
animation = "plasma"
colors = ["#ff0000"]
fps = 60
dim = 2.0
"##,
    );
    let found = codes(&diagnostics);
    for code in ["W0171", "W0174", "W0175"] {
        assert!(found.contains(&code), "missing {code} in {found:?}");
    }

    let (_, diagnostics) = load("[pages.background]\nspan = \"lcd\"\n");
    assert!(codes(&diagnostics).contains(&"W0170"));

    let (_, diagnostics) = load("[pages.background]\nimage = \"/a.png\"\ncolors = [\"#ff0000\"]\n");
    assert!(codes(&diagnostics).contains(&"W0172"));
}

#[test]
fn opacity_needs_something_to_fade_on_a_key_but_not_on_the_screen() {
    let (_, diagnostics) = load(
        r##"
[[pages.keys]]
key = 0
exec = "true"

[pages.keys.widget]
kind = "cpu"
opacity = 0.5

[[pages.lcd]]
column = 0
row = 0
columns = 4
rows = 2

[pages.lcd.widget]
kind = "cpu"
opacity = 0.5
"##,
    );
    let warnings: Vec<_> = diagnostics.iter().filter(|d| d.code == "W0177").collect();
    assert_eq!(warnings.len(), 1, "{warnings:#?}");
    assert!(warnings[0].path.contains("keys[0]"));
}

#[test]
fn a_widget_background_loads() {
    let (workspace, diagnostics) = load(
        r##"
[[pages.lcd]]
column = 0
row = 0

[pages.lcd.widget]
kind = "clock"
background = "@accent"
opacity = 0.4
image = "/pictures/sky.png"
"##,
    );
    // `@accent` is not in an empty palette, which is reported elsewhere if
    // at all; what matters here is that the fields are accepted.
    assert!(
        !codes(&diagnostics).iter().any(|c| c.starts_with('E')),
        "{diagnostics:#?}"
    );
    let workspace = workspace.unwrap();
    let widget = &workspace.profile("work").unwrap().pages[0].lcd[0].widget;
    assert_eq!(widget.opacity(), 0.4);
    assert!(widget.background.is_some() && widget.image.is_some());
}

#[test]
fn a_page_can_lay_its_screen_out_on_a_grid_of_its_own() {
    let (workspace, diagnostics) = load(
        r##"
lcd_columns = 4
lcd_rows = 2

[[pages.lcd]]
column = 2
row = 1

[pages.lcd.widget]
kind = "clock"
"##,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let workspace = workspace.unwrap();
    let profile = workspace.profile("work").unwrap();
    let page = &profile.pages[0];
    let grid = Workspace::grid_for(profile, page);
    assert_eq!((grid.columns, grid.rows), (4, 2));
    // A span left out runs to the edge of this grid, not the default one.
    let cells = page.lcd[0].cells(grid);
    assert_eq!((cells.columns, cells.rows), (2, 1));
}

#[test]
fn a_tile_that_fits_the_default_grid_but_not_the_pages_is_an_error() {
    let (_, diagnostics) = load(
        r##"
lcd_columns = 6

[[pages.lcd]]
column = 4
row = 0
columns = 4
rows = 2

[pages.lcd.widget]
kind = "clock"
"##,
    );
    let error = diagnostics.iter().find(|d| d.code == "E0160").unwrap();
    assert!(
        error.help.as_deref().unwrap().contains("6 columns"),
        "{error:#?}"
    );
}

#[test]
fn a_grid_size_out_of_range_is_clamped_and_said() {
    let (workspace, diagnostics) = load("lcd_columns = 40\nlcd_rows = 0\n");
    let warnings = diagnostics.iter().filter(|d| d.code == "W0166").count();
    assert_eq!(warnings, 2);
    let workspace = workspace.unwrap();
    let profile = workspace.profile("work").unwrap();
    let grid = Workspace::grid_for(profile, &profile.pages[0]);
    assert_eq!((grid.columns, grid.rows), (24, 1));
}

#[test]
fn the_new_kinds_load_and_measure() {
    for kind in ["battery", "fan", "load", "uptime", "volume"] {
        let (workspace, diagnostics) = load(&format!(
            "[[pages.keys]]\nkey = 0\n\n[pages.keys.widget]\nkind = \"{kind}\"\n"
        ));
        assert!(diagnostics.is_empty(), "{kind}: {diagnostics:#?}");
        assert!(workspace.is_some());
    }
    assert!(WidgetKind::Battery.is_numeric() && !WidgetKind::Uptime.is_numeric());
    assert!(WidgetKind::Volume.is_blocking());
}

#[test]
fn a_clock_can_have_a_face_and_nothing_else_can() {
    let (_, diagnostics) = load(
        r##"
[[pages.keys]]
key = 0

[pages.keys.widget]
kind = "clock"
view = "analog"
timezone = "Asia/Tokyo"

[[pages.keys]]
key = 1

[pages.keys.widget]
kind = "cpu"
view = "analog"

[[pages.keys]]
key = 2

[pages.keys.widget]
kind = "memory"
view = "gauge"
"##,
    );
    let warnings: Vec<_> = diagnostics.iter().filter(|d| d.code == "W0149").collect();
    assert_eq!(warnings.len(), 1, "{warnings:#?}");
    assert!(warnings[0].path.contains("keys[1]"));
    assert!(warnings[0].help.as_deref().unwrap().contains("clock"));
}

#[test]
fn only_a_clock_can_be_nixie_tubes() {
    let (_, diagnostics) = load(
        r##"
[[pages.keys]]
key = 0

[pages.keys.widget]
kind = "clock"
view = "nixie"

[[pages.keys]]
key = 1

[pages.keys.widget]
kind = "date"
view = "nixie"
"##,
    );
    let warnings: Vec<_> = diagnostics.iter().filter(|d| d.code == "W0149").collect();
    assert_eq!(warnings.len(), 1, "{warnings:#?}");
    assert!(warnings[0].path.contains("keys[1]"));
}

#[test]
fn a_nixie_clock_counts_seconds_and_refreshes_at_its_frame_rate() {
    use galdeck_model::Widget;
    let nixie = Widget {
        view: Some(WidgetView::Nixie),
        ..Widget::of(WidgetKind::Clock)
    };
    assert_eq!(nixie.format(), "%H:%M:%S");
    assert_eq!(nixie.interval_ms(), galdeck_model::widget::MIN_INTERVAL_MS);

    // What was asked for still wins.
    let asked = Widget {
        interval_ms: Some(500),
        format: Some("%I:%M".into()),
        ..nixie
    };
    assert_eq!(asked.interval_ms(), 500);
    assert_eq!(asked.format(), "%I:%M");

    // And a clock drawn as text is as it was.
    let text = Widget::of(WidgetKind::Clock);
    assert_eq!(text.format(), "%H:%M");
    assert_eq!(text.interval_ms(), 1_000);
}

#[test]
fn a_time_zone_is_for_telling_the_time_in() {
    let (_, diagnostics) = load(
        r##"
[[pages.keys]]
key = 0

[pages.keys.widget]
kind = "cpu"
timezone = "Europe/London"

[[pages.keys]]
key = 1

[pages.keys.widget]
kind = "clock"
timezone = "not a zone"
"##,
    );
    let found = codes(&diagnostics);
    assert!(
        found.contains(&"W0164") && found.contains(&"W0165"),
        "{found:?}"
    );
}

#[test]
fn a_rainbow_goes_round_the_wheel_and_a_heartbeat_rests() {
    use galdeck::Rgb;
    use galdeck_model::{Animation, AnimationKind};
    let rainbow = Animation {
        kind: AnimationKind::Rainbow,
        period_ms: 2000,
        to: None,
        frames: 12,
    };
    let white = Rgb::new(255, 255, 255);
    let first = rainbow.color_for_frame(0, Rgb::new(0, 0, 0), white);
    let third = rainbow.color_for_frame(4, Rgb::new(0, 0, 0), white);
    assert_ne!(first, third);

    let heart = Animation {
        kind: AnimationKind::Heartbeat,
        period_ms: 1200,
        to: None,
        frames: 8,
    };
    // Sharp enough to need more frames than asked for.
    assert!(heart.frames() >= 16);
    // Beating early in the cycle, resting late in it.
    assert!(AnimationKind::Heartbeat.mix_at(0.09) > 0.5);
    assert_eq!(AnimationKind::Heartbeat.mix_at(0.8), 0.0);
}
