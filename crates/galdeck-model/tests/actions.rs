//! Actions, keystrokes and knob presets: what loads, how the layers combine,
//! and what is said about the things that cannot work.

use galdeck_model::{
    Action, BuiltIn, Diagnostic, Invocation, Layer, Level, Preset, RingShows, Widget, WidgetKind,
    Workspace,
};

/// Load a workspace from whole files.
fn load(files: &[(&str, &str)]) -> (Option<Workspace>, Vec<Diagnostic>) {
    let dir = std::env::temp_dir().join(format!(
        "galdeck-model-actions-{}-{:?}",
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

/// A one-page profile with this TOML after `[[pages]] id = "main"`.
fn page(body: &str) -> (Option<Workspace>, Vec<Diagnostic>) {
    load(&[
        ("galdeck.toml", "version = 2\n"),
        (
            "profiles/work.toml",
            &format!("[[pages]]\nid = \"main\"\n{body}"),
        ),
    ])
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn every_gesture_takes_a_command_a_built_in_or_a_chord() {
    let (workspace, diagnostics) = page(
        r##"
[[pages.keys]]
key = 0
exec = "firefox"
hold = { action = "next_track" }
double = { keys = "ctrl+shift+t" }

[[pages.encoders]]
encoder = 1
press = { action = "play_pause", target = "spotify" }
cw = { action = "volume_up", step = 4 }
ccw = "wpctl set-volume @DEFAULT_AUDIO_SINK@ 4%-"
"##,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let workspace = workspace.unwrap();
    let page = &workspace.profile("work").unwrap().pages[0];
    let key = &page.keys[0];
    assert_eq!(key.exec, Some(Action::Shell("firefox".into())));
    assert_eq!(key.hold, Some(Action::built_in(BuiltIn::NextTrack)));
    assert_eq!(key.double, Some(Action::keys("ctrl+shift+t")));
    let encoder = &page.encoders[0];
    assert_eq!(
        encoder.press,
        Some(Action::BuiltIn(Invocation {
            action: BuiltIn::PlayPause,
            step: None,
            target: Some("spotify".into()),
        }))
    );
}

#[test]
fn a_misspelt_key_suggests_the_right_one() {
    let (_, diagnostics) = page(
        r##"
[[pages.keys]]
key = 0
exec = { keys = "ctrl+pagedwn" }
"##,
    );
    let error = diagnostics.iter().find(|d| d.code == "E0118").unwrap();
    assert!(error.message.contains("page_down"), "{error:#?}");
}

#[test]
fn chords_that_cannot_mean_anything_are_pointed_out() {
    let (_, diagnostics) = page(
        r##"
[[pages.keys]]
key = 0
exec = { keys = "ctrl+shift" }

[[pages.keys]]
key = 1
exec = { keys = "kp_7" }

[[pages.keys]]
key = 2
exec = { keys = "ctrl+k ctrl+c" }
"##,
    );
    let found = codes(&diagnostics);
    assert!(found.contains(&"W0119"), "{found:?}");
    assert!(found.contains(&"H0124"), "{found:?}");
    assert!(found.contains(&"E0118"), "sequences are refused: {found:?}");
}

#[test]
fn a_knob_set_once_is_on_every_page_and_a_page_changes_one_gesture() {
    let (workspace, diagnostics) = load(&[
        (
            "galdeck.toml",
            "version = 2\n\n[[encoders]]\nencoder = 0\npreset = \"volume\"\nstep = 3\n",
        ),
        (
            "profiles/work.toml",
            r##"
[[encoders]]
encoder = 1
preset = "tracks"

[[pages]]
id = "main"

[[pages]]
id = "numpad"

[[pages.encoders]]
encoder = 0
press = { action = "home_page" }
"##,
        ),
    ]);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let workspace = workspace.unwrap();
    let profile = workspace.profile("work").unwrap();
    let global = &workspace.global.encoders;

    let main = Workspace::encoder_for(profile, &profile.pages[0], global, 0);
    let (cw, from) = main.cw.clone().unwrap();
    assert_eq!(from, Layer::Global);
    assert_eq!(
        cw,
        Action::BuiltIn(Invocation {
            action: BuiltIn::VolumeUp,
            step: Some(3.0),
            target: None
        })
    );
    assert_eq!(main.ring(), Some(RingShows::OutputLevel));

    // The numpad page changed only what pressing does; turning is still
    // volume, and the ring still shows the level.
    let numpad = Workspace::encoder_for(profile, &profile.pages[1], global, 0);
    assert_eq!(
        numpad.press.clone().unwrap(),
        (Action::built_in(BuiltIn::HomePage), Layer::Page)
    );
    assert_eq!(numpad.cw.clone().unwrap().1, Layer::Global);
    assert_eq!(numpad.ring(), Some(RingShows::OutputLevel));

    let right = Workspace::encoder_for(profile, &profile.pages[0], global, 1);
    assert_eq!(
        right.press.unwrap(),
        (Action::built_in(BuiltIn::PlayPause), Layer::Profile)
    );
    assert!(Workspace::encoder_for(profile, &profile.pages[0], &[], 1)
        .cw
        .is_some());
}

#[test]
fn a_gesture_written_beside_a_preset_replaces_the_presets() {
    let (workspace, _) = page(
        r##"
[[pages.encoders]]
encoder = 0
preset = "volume"
press = { action = "play_pause" }
"##,
    );
    let workspace = workspace.unwrap();
    let profile = workspace.profile("work").unwrap();
    let plan = Workspace::encoder_for(profile, &profile.pages[0], &[], 0);
    assert_eq!(plan.press.unwrap().0, Action::built_in(BuiltIn::PlayPause));
    assert_eq!(plan.cw.unwrap().0, Action::built_in(BuiltIn::VolumeUp));
    // An explicit turn takes the preset's ring display with it.
    let (workspace, _) = page(
        "[[pages.encoders]]\nencoder = 0\npreset = \"volume\"\ncw = \"true\"\nccw = \"true\"\n",
    );
    let workspace = workspace.unwrap();
    let profile = workspace.profile("work").unwrap();
    assert_eq!(
        Workspace::encoder_for(profile, &profile.pages[0], &[], 0).ring(),
        None
    );
}

#[test]
fn turning_through_profiles_from_a_page_is_a_one_way_trip() {
    let (_, diagnostics) = page("[[pages.encoders]]\nencoder = 0\npreset = \"profiles\"\n");
    assert!(codes(&diagnostics).contains(&"H0128"));
    let (_, diagnostics) = load(&[
        (
            "galdeck.toml",
            "version = 2\n[[encoders]]\nencoder = 0\npreset = \"profiles\"\n",
        ),
        ("profiles/work.toml", "[[pages]]\nid = \"main\"\n"),
    ]);
    assert!(!codes(&diagnostics).contains(&"H0128"));
}

#[test]
fn steps_and_targets_that_mean_nothing_are_pointed_out() {
    let (_, diagnostics) = page(
        r##"
[[pages.keys]]
key = 0
exec = { action = "next_page", step = 2 }

[[pages.keys]]
key = 1
exec = { action = "volume_up", step = 90 }

[[pages.keys]]
key = 2
exec = { action = "next_page", target = "spotify" }

[[pages.encoders]]
encoder = 0
step = 5
"##,
    );
    let found = codes(&diagnostics);
    for code in ["W0123", "W0125", "W0126"] {
        assert!(found.contains(&code), "missing {code} in {found:?}");
    }
}

#[test]
fn push_to_talk_needs_to_see_the_key_come_up() {
    let (_, diagnostics) = page(
        r##"
[[pages.keys]]
key = 0
exec = { action = "push_to_talk" }

[[pages.keys]]
key = 1
hold = { action = "push_to_talk" }

[[pages.encoders]]
encoder = 0
press = { action = "push_to_talk" }
"##,
    );
    let warnings: Vec<_> = diagnostics.iter().filter(|d| d.code == "W0127").collect();
    assert_eq!(warnings.len(), 2, "{warnings:#?}");
}

#[test]
fn with_virtual_input_off_keystrokes_are_flagged() {
    let (_, diagnostics) = load(&[
        (
            "galdeck.toml",
            "version = 2\nvirtual_input = false\n[[encoders]]\nencoder = 0\npreset = \"tabs\"\n",
        ),
        (
            "profiles/work.toml",
            "[[pages]]\nid = \"main\"\n[[pages.keys]]\nkey = 0\nexec = { keys = \"a\" }\n",
        ),
    ]);
    let warnings = diagnostics.iter().filter(|d| d.code == "W0129").count();
    assert_eq!(warnings, 2, "{diagnostics:#?}");
}

#[test]
fn a_key_showing_media_or_the_output_volume_does_something_when_tapped() {
    let (workspace, diagnostics) = page(
        r##"
[[pages.keys]]
key = 0

[pages.keys.widget]
kind = "media"
source = "spotify"

[[pages.keys]]
key = 1

[pages.keys.widget]
kind = "volume"

[[pages.keys]]
key = 2

[pages.keys.widget]
kind = "volume"
source = "mic"
"##,
    );
    // Key 2 is a live microphone waiting to happen, so it is left alone --
    // and so it is the one key reported as doing nothing.
    let unbound: Vec<_> = diagnostics.iter().filter(|d| d.code == "H0113").collect();
    assert!(
        unbound.is_empty(),
        "widget keys are not unbound: {unbound:#?}"
    );
    let workspace = workspace.unwrap();
    let keys = &workspace.profile("work").unwrap().pages[0].keys;
    assert_eq!(
        keys[0].tap(),
        Some(Action::BuiltIn(Invocation {
            action: BuiltIn::PlayPause,
            step: None,
            target: Some("spotify".into())
        }))
    );
    assert_eq!(
        keys[1]
            .tap()
            .and_then(|a| a.as_built_in().map(|i| i.action)),
        Some(BuiltIn::VolumeMute)
    );
    assert_eq!(keys[2].tap(), None);
}

#[test]
fn an_explicit_tap_wins_over_the_widgets() {
    let (workspace, _) =
        page("[[pages.keys]]\nkey = 0\nexec = \"true\"\n\n[pages.keys.widget]\nkind = \"media\"\n");
    let workspace = workspace.unwrap();
    let key = &workspace.profile("work").unwrap().pages[0].keys[0];
    assert_eq!(key.tap(), Some(Action::Shell("true".into())));
}

#[test]
fn thresholds_turn_amber_then_red_and_do_not_flicker() {
    let hot = Widget {
        warn: Some(80.0),
        critical: Some(90.0),
        ..Widget::of(WidgetKind::Temperature)
    };
    assert_eq!(hot.level(50.0, Level::Normal, 100.0), Level::Normal);
    assert_eq!(hot.level(85.0, Level::Normal, 100.0), Level::Warn);
    assert_eq!(hot.level(95.0, Level::Warn, 100.0), Level::Critical);
    // Just under the line, it stays where it was until it is clearly back.
    assert_eq!(hot.level(79.0, Level::Warn, 100.0), Level::Warn);
    assert_eq!(hot.level(77.0, Level::Warn, 100.0), Level::Normal);
    assert_eq!(hot.level(79.0, Level::Normal, 100.0), Level::Normal);

    let battery = Widget {
        warn: Some(20.0),
        critical: Some(10.0),
        ..Widget::of(WidgetKind::Battery)
    };
    assert!(battery.counts_down());
    assert_eq!(battery.level(50.0, Level::Normal, 100.0), Level::Normal);
    assert_eq!(battery.level(15.0, Level::Normal, 100.0), Level::Warn);
    assert_eq!(battery.level(5.0, Level::Normal, 100.0), Level::Critical);
    // One threshold alone: a battery still counts down.
    let low = Widget {
        warn: Some(20.0),
        ..Widget::of(WidgetKind::Battery)
    };
    assert_eq!(low.level(15.0, Level::Normal, 100.0), Level::Warn);
}

#[test]
fn thresholds_and_places_where_they_mean_nothing_are_pointed_out() {
    let (_, diagnostics) = page(
        r##"
[[pages.keys]]
key = 0
exec = "true"

[pages.keys.widget]
kind = "clock"
warn = 3
place = "Paris"
"##,
    );
    let found = codes(&diagnostics);
    assert!(
        found.contains(&"W0178") && found.contains(&"W0179"),
        "{found:?}"
    );
}

#[test]
fn every_preset_loads_and_its_ring_matches_its_job() {
    for preset in Preset::ALL {
        let (workspace, diagnostics) = page(&format!(
            "[[pages.encoders]]\nencoder = 0\npreset = \"{}\"\n",
            preset.name()
        ));
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == galdeck_model::Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{preset:?}: {errors:#?}");
        let workspace = workspace.unwrap();
        let profile = workspace.profile("work").unwrap();
        let plan = Workspace::encoder_for(profile, &profile.pages[0], &[], 0);
        assert!(plan.cw.is_some() && plan.ccw.is_some(), "{preset:?} turns");
        assert_eq!(plan.ring(), preset.ring());
    }
}

#[test]
fn a_step_that_is_not_a_number_is_no_step() {
    let nan = Invocation {
        action: BuiltIn::DeckBrighter,
        step: Some(f64::NAN),
        target: None,
    };
    let default = BuiltIn::DeckBrighter.step().unwrap().1;
    assert_eq!(nan.step(), default);
    let infinite = Invocation {
        step: Some(f64::INFINITY),
        ..nan
    };
    assert_eq!(infinite.step(), default);

    let (_, diagnostics) = page(
        r#"
[[pages.encoders]]
encoder = 0
cw = { action = "deck_brighter", step = nan }
"#,
    );
    let warning = diagnostics.iter().find(|d| d.code == "W0125").unwrap();
    assert!(warning.message.contains("default"), "{warning:#?}");
}

#[test]
fn a_preset_s_step_is_checked_like_its_actions() {
    let (_, diagnostics) = load(&[
        (
            "galdeck.toml",
            "version = 2\n[[encoders]]\nencoder = 0\npreset = \"volume\"\nstep = 100\n\n[[encoders]]\nencoder = 1\npreset = \"tracks\"\nstep = 5\n\n[[encoders]]\nencoder = 2\npreset = \"deck_brightness\"\nstep = nan\n\n[[encoders]]\nencoder = 3\npreset = \"volume\"\nstep = 3\n",
        ),
        ("profiles/work.toml", "[[pages]]\nid = \"main\"\n"),
    ]);
    let at = |path: &str| {
        diagnostics
            .iter()
            .filter(|d| d.path == path)
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(at("encoders[0].step"), ["W0125"], "{diagnostics:#?}");
    assert_eq!(at("encoders[1].step"), ["W0123"]);
    assert_eq!(at("encoders[2].step"), ["W0125"]);
    assert!(at("encoders[3].step").is_empty());
}

#[test]
fn push_to_talk_says_what_it_hides() {
    let (_, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0
exec = { action = "push_to_talk" }
hold = "notify-send hi"
page = "main"
"#,
    );
    let warning = diagnostics.iter().find(|d| d.code == "W0131").unwrap();
    assert_eq!(warning.path, "profiles.work.pages[0].keys[0].exec");
    assert!(warning.message.contains("hold, page"), "{warning:#?}");

    let (_, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0
exec = { action = "push_to_talk" }
"#,
    );
    assert!(!codes(&diagnostics).contains(&"W0131"), "{diagnostics:#?}");
}

#[test]
fn a_knob_s_colour_is_checked_at_every_layer() {
    let (_, diagnostics) = load(&[
        (
            "galdeck.toml",
            "version = 2\n[[encoders]]\nencoder = 0\npreset = \"volume\"\nstyle = { ring = \"@acent\" }\n",
        ),
        (
            "profiles/work.toml",
            "[[encoders]]\nencoder = 1\nstyle = { ring = \"@nope\" }\n\n[[pages]]\nid = \"main\"\n",
        ),
        (
            "profiles/play.toml",
            "[[pages]]\nid = \"main\"\n",
        ),
    ]);
    let paths: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == "E0117")
        .map(|d| d.path.as_str())
        .collect();
    // Errors, as on a page, so an editor's save is refused. Two profiles,
    // and the global knob is reported once.
    assert_eq!(
        paths,
        [
            "encoders[0].style.ring",
            "profiles.work.encoders[0].style.ring"
        ],
        "{diagnostics:#?}"
    );
}
