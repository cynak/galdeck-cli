//! Knobs with modes, what a target reaches, and the list of outputs: how the
//! layers fold, and what is said about what cannot work.

use galdeck_model::{
    Action, BuiltIn, ColorRef, Diagnostic, EncoderPlan, Invocation, Layer, ModeEntry, Preset,
    RingShows, Workspace,
};

/// Load a workspace from whole files.
fn load(files: &[(&str, &str)]) -> (Option<Workspace>, Vec<Diagnostic>) {
    let dir = std::env::temp_dir().join(format!(
        "galdeck-model-modes-{}-{:?}",
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

/// The diagnostics with this code, by path.
fn at<'a>(diagnostics: &'a [Diagnostic], code: &str) -> Vec<&'a str> {
    diagnostics
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.path.as_str())
        .collect()
}

/// Knob 0 on page `p` of the `work` profile, in `mode`.
fn knob(workspace: &Workspace, p: usize, mode: usize) -> EncoderPlan {
    let profile = workspace.profile("work").unwrap();
    Workspace::encoder_for_mode(
        profile,
        &profile.pages[p],
        &workspace.global.encoders,
        0,
        mode,
    )
}

fn built_in(slot: &Option<(Action, Layer)>) -> Option<BuiltIn> {
    slot.as_ref()?.0.as_built_in().map(|i| i.action)
}

#[test]
fn a_mode_is_a_preset_s_name_or_a_table() {
    let (workspace, diagnostics) = page(
        r##"
[[pages.encoders]]
encoder = 0
modes = ["volume", { preset = "app_volume", target = "spotify", ring = "#a3be8c" }, { preset = "scroll", step = 3 }]
"##,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let workspace = workspace.unwrap();
    let modes = &workspace.profile("work").unwrap().pages[0].encoders[0].modes;
    assert_eq!(
        modes,
        &[
            ModeEntry::of(Preset::Volume),
            ModeEntry {
                target: Some("spotify".into()),
                ring: Some(ColorRef::parse("#a3be8c").unwrap()),
                ..ModeEntry::of(Preset::AppVolume)
            },
            ModeEntry {
                step: Some(3.0),
                ..ModeEntry::of(Preset::Scroll)
            },
        ]
    );
}

#[test]
fn a_mode_that_is_not_one_says_what_it_wanted() {
    let (_, diagnostics) =
        page("[[pages.encoders]]\nencoder = 0\nmodes = [\"volume\", \"scrol\"]\n");
    let error = diagnostics.iter().find(|d| d.code == "E0001").unwrap();
    assert!(error.message.contains("scroll"), "{error:#?}");
    let (_, diagnostics) = page(
        "[[pages.encoders]]\nencoder = 0\nmodes = [\"volume\", { preset = \"scroll\", press = \"true\" }]\n",
    );
    let error = diagnostics.iter().find(|d| d.code == "E0001").unwrap();
    assert!(error.message.contains("press"), "{error:#?}");
    let (_, diagnostics) =
        page("[[pages.encoders]]\nencoder = 0\nmodes = [\"volume\", { step = 3 }]\n");
    assert!(
        diagnostics.iter().any(|d| d.code == "E0001"),
        "{diagnostics:#?}"
    );
}

#[test]
fn a_knob_turns_with_its_active_mode_and_holding_it_switches() {
    let (workspace, diagnostics) = page(
        r##"
[[pages.encoders]]
encoder = 0
modes = [{ preset = "volume", step = 5, target = "42" }, { preset = "app_volume", target = "spotify" }, "pages"]
"##,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let workspace = workspace.unwrap();

    let first = knob(&workspace, 0, 0);
    assert_eq!(
        first.cw.clone().unwrap(),
        (
            Action::BuiltIn(Invocation {
                action: BuiltIn::VolumeUp,
                step: Some(5.0),
                target: Some("42".into()),
            }),
            Layer::Page
        )
    );
    assert_eq!(first.ring(), Some(RingShows::OutputLevel));
    assert_eq!(
        first.hold.clone().unwrap(),
        (Action::built_in(BuiltIn::NextMode), Layer::Page)
    );
    assert_eq!(first.mode, 0);
    assert_eq!(first.modes.as_ref().unwrap().entries.len(), 3);
    assert_eq!(first.modes.as_ref().unwrap().layer, Layer::Page);

    // Each mode keeps its own step and target: the volume's step does not
    // follow the knob into the app's mode.
    let second = knob(&workspace, 0, 1);
    let cw = second.cw.clone().unwrap().0;
    let cw = cw.as_built_in().unwrap();
    assert_eq!(
        (cw.action, cw.step, cw.target.as_deref()),
        (BuiltIn::AppVolumeUp, None, Some("spotify"))
    );
    assert_eq!(built_in(&second.press), Some(BuiltIn::AppMute));
    assert_eq!(second.ring(), Some(RingShows::AppLevel));
    assert_eq!(second.active_mode().unwrap().preset, Preset::AppVolume);

    // A mode past the end is the last one.
    let past = knob(&workspace, 0, 7);
    assert_eq!(past.mode, 2);
    assert_eq!(built_in(&past.cw), Some(BuiltIn::NextPage));
    assert_eq!(
        Workspace::encoder_for(
            workspace.profile("work").unwrap(),
            &workspace.profile("work").unwrap().pages[0],
            &[],
            0
        )
        .mode,
        0
    );
}

#[test]
fn a_gesture_written_beside_modes_replaces_every_mode_s() {
    let (workspace, _) = page(
        r##"
[[pages.encoders]]
encoder = 0
modes = ["volume", "tracks"]
press = { action = "home_page" }
"##,
    );
    let workspace = workspace.unwrap();
    for mode in 0..2 {
        let plan = knob(&workspace, 0, mode);
        assert_eq!(built_in(&plan.press), Some(BuiltIn::HomePage));
        assert!(plan.modes.is_some());
    }
}

#[test]
fn modes_under_a_layer_that_turns_are_shadowed() {
    let (workspace, diagnostics) = load(&[
        (
            "galdeck.toml",
            "version = 2\n[[encoders]]\nencoder = 0\nmodes = [\"volume\", \"scroll\"]\n",
        ),
        (
            "profiles/work.toml",
            r##"
[[pages]]
id = "main"

[[pages]]
id = "media"

[[pages.encoders]]
encoder = 0
preset = "seek"

[[pages]]
id = "numpad"

[[pages.encoders]]
encoder = 0
press = { action = "home_page" }

[[pages]]
id = "arrows"

[[pages.encoders]]
encoder = 0
cw = { keys = "right" }
ccw = { keys = "left" }
"##,
        ),
    ]);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let workspace = workspace.unwrap();

    let main = knob(&workspace, 0, 1);
    assert_eq!(built_in(&main.cw), Some(BuiltIn::ScrollDown));
    assert_eq!(main.modes.as_ref().unwrap().layer, Layer::Global);

    // The page owns the turn: the global modes add nothing, so holding does
    // not cycle modes nobody could see.
    let media = knob(&workspace, 1, 1);
    assert_eq!(built_in(&media.cw), Some(BuiltIn::SeekForward));
    assert!(media.modes.is_none());
    assert!(media.hold.is_none());
    assert_eq!(media.turn_preset, Some((Preset::Seek, Layer::Page)));

    // A page that changes only the press keeps the modes.
    let numpad = knob(&workspace, 2, 1);
    assert_eq!(built_in(&numpad.cw), Some(BuiltIn::ScrollDown));
    assert_eq!(built_in(&numpad.press), Some(BuiltIn::HomePage));
    assert_eq!(built_in(&numpad.hold), Some(BuiltIn::NextMode));

    // A turn written out owns it just as a preset does, and the shadowed
    // modes' press goes with them.
    let arrows = knob(&workspace, 3, 0);
    assert!(arrows.modes.is_none() && arrows.press.is_none() && arrows.hold.is_none());
    assert_eq!(arrows.ring(), None);
}

#[test]
fn a_hold_written_anywhere_keeps_modes_from_switching_and_says_so_once() {
    let (workspace, diagnostics) = load(&[
        (
            "galdeck.toml",
            "version = 2\n[[encoders]]\nencoder = 0\nhold = { action = \"play_pause\" }\n",
        ),
        (
            "profiles/work.toml",
            r##"
[[encoders]]
encoder = 0
modes = ["volume", "tracks"]

[[encoders]]
encoder = 1
modes = ["volume", "tracks"]
hold = { action = "next_mode" }

[[pages]]
id = "main"

[[pages]]
id = "other"
"##,
        ),
    ]);
    assert_eq!(
        at(&diagnostics, "W0135"),
        ["profiles.work.encoders[0].modes"],
        "{diagnostics:#?}"
    );
    let workspace = workspace.unwrap();
    let plan = knob(&workspace, 0, 0);
    assert_eq!(
        plan.hold.clone().unwrap(),
        (Action::built_in(BuiltIn::PlayPause), Layer::Global)
    );
    assert!(plan.modes.is_some());
}

#[test]
fn a_mode_count_outside_two_to_four_is_an_error_and_the_daemon_copes() {
    let (workspace, diagnostics) = page(
        r##"
[[pages.encoders]]
encoder = 0
modes = ["tracks"]

[[pages.encoders]]
encoder = 1
modes = ["volume", "mic", "tracks", "seek", "pages"]
"##,
    );
    assert_eq!(
        at(&diagnostics, "E0132"),
        [
            "profiles.work.pages[0].encoders[0].modes",
            "profiles.work.pages[0].encoders[1].modes"
        ],
        "{diagnostics:#?}"
    );
    let workspace = workspace.unwrap();
    // One mode is no modes: nothing turns, and holding switches nothing.
    let one = knob(&workspace, 0, 0);
    assert!(one.modes.is_none() && one.hold.is_none() && one.cw.is_none());
    // Five keeps the first four.
    let profile = workspace.profile("work").unwrap();
    let five = Workspace::encoder_for_mode(profile, &profile.pages[0], &[], 1, 9);
    assert_eq!(five.modes.as_ref().unwrap().entries.len(), 4);
    assert_eq!(five.mode, 3);
    assert_eq!(built_in(&five.cw), Some(BuiltIn::SeekForward));
}

#[test]
fn a_preset_beside_a_single_mode_is_what_the_knob_does_and_is_checked_as_such() {
    let (workspace, diagnostics) = page(
        "[[pages.encoders]]\nencoder = 0\npreset = \"volume\"\nmodes = [\"tracks\"]\nstep = 50\n",
    );
    let e0133: Vec<_> = diagnostics.iter().filter(|d| d.code == "E0133").collect();
    assert_eq!(e0133.len(), 1, "{diagnostics:#?}");
    assert!(
        e0133[0].message.contains("the preset is used"),
        "{e0133:#?}"
    );
    // The step goes to the volume preset, so it is held to that preset's
    // range rather than called ignored.
    assert!(at(&diagnostics, "W0134").is_empty(), "{diagnostics:#?}");
    assert_eq!(
        at(&diagnostics, "W0125"),
        ["profiles.work.pages[0].encoders[0].step"]
    );
    let plan = knob(&workspace.unwrap(), 0, 0);
    let cw = plan.cw.clone().unwrap().0;
    let cw = cw.as_built_in().unwrap();
    assert_eq!((cw.action, cw.step), (BuiltIn::VolumeUp, Some(50.0)));
    assert!(plan.modes.is_none() && plan.hold.is_none());
}

#[test]
fn a_preset_beside_modes_is_an_error_and_the_modes_win() {
    let (workspace, diagnostics) = page(
        "[[pages.encoders]]\nencoder = 0\npreset = \"zoom\"\nmodes = [\"volume\", \"tracks\"]\n",
    );
    assert_eq!(
        at(&diagnostics, "E0133"),
        ["profiles.work.pages[0].encoders[0].preset"]
    );
    let plan = knob(&workspace.unwrap(), 0, 1);
    assert_eq!(built_in(&plan.cw), Some(BuiltIn::NextTrack));
}

#[test]
fn a_step_or_target_beside_modes_is_ignored_and_said() {
    let (workspace, diagnostics) = page(
        "[[pages.encoders]]\nencoder = 0\nmodes = [\"volume\", \"seek\"]\nstep = 10\ntarget = \"spotify\"\n",
    );
    assert_eq!(
        at(&diagnostics, "W0134"),
        [
            "profiles.work.pages[0].encoders[0].step",
            "profiles.work.pages[0].encoders[0].target"
        ],
        "{diagnostics:#?}"
    );
    // Not W0123 as well: the knob does have presets.
    assert!(at(&diagnostics, "W0123").is_empty());
    let cw = knob(&workspace.unwrap(), 0, 0).cw.unwrap().0;
    let cw = cw.as_built_in().unwrap();
    assert_eq!((cw.step, cw.target.clone()), (None, None));
}

#[test]
fn each_mode_s_step_and_target_are_checked_as_a_preset_s() {
    let (_, diagnostics) = page(
        r##"
[[pages.encoders]]
encoder = 0
modes = [{ preset = "volume", step = 100 }, { preset = "outputs", target = "Headphones", step = 2 }, { preset = "tracks", target = "" }, { preset = "volume", target = "Spotify Player" }]
"##,
    );
    let path = |what: &str| format!("profiles.work.pages[0].encoders[0].modes{what}");
    assert_eq!(at(&diagnostics, "W0125"), [path("[0].step")]);
    assert_eq!(at(&diagnostics, "W0123"), [path("[1].step")]);
    assert_eq!(at(&diagnostics, "W0126"), [path("[1].target")]);
    assert_eq!(
        at(&diagnostics, "W0138"),
        [path("[2].target"), path("[3].target")],
        "{diagnostics:#?}"
    );
}

#[test]
fn modes_are_checked_for_where_they_are_used() {
    let (_, diagnostics) = load(&[
        ("galdeck.toml", "version = 2\nvirtual_input = false\n"),
        (
            "profiles/work.toml",
            "[[pages]]\nid = \"main\"\n[[pages.encoders]]\nencoder = 0\nmodes = [\"profiles\", \"scroll\"]\n",
        ),
    ]);
    let path = |m: usize| format!("profiles.work.pages[0].encoders[0].modes[{m}]");
    assert_eq!(at(&diagnostics, "H0128"), [path(0)]);
    assert_eq!(at(&diagnostics, "W0129"), [path(1)], "{diagnostics:#?}");
}

#[test]
fn every_gesture_of_every_mode_is_seen_by_checks_over_them_all() {
    let (workspace, _) = page(
        "[[pages.encoders]]\nencoder = 0\nmodes = [\"volume\", \"scroll\"]\npress = \"true\"\n",
    );
    let workspace = workspace.unwrap();
    let knob = &workspace.profile("work").unwrap().pages[0].encoders[0];
    let all = knob.all_gestures();
    assert!(all.contains(&Action::Shell("true".into())));
    assert!(all.iter().any(Action::needs_virtual_input), "{all:#?}");
    assert!(!knob
        .gestures()
        .iter()
        .flatten()
        .any(Action::needs_virtual_input));
    assert_eq!(
        knob.presets().collect::<Vec<_>>(),
        [Preset::Volume, Preset::Scroll]
    );
}

#[test]
fn a_knob_s_target_reaches_its_preset_s_own_kind_and_nothing_else() {
    let (workspace, diagnostics) = page(
        r##"
[[pages.encoders]]
encoder = 0
preset = "volume"
target = "42"

[[pages.encoders]]
encoder = 1
preset = "outputs"
target = "Headphones"
"##,
    );
    assert_eq!(
        at(&diagnostics, "W0126"),
        ["profiles.work.pages[0].encoders[1].target"],
        "{diagnostics:#?}"
    );
    let workspace = workspace.unwrap();
    let profile = workspace.profile("work").unwrap();
    let plan = |e| Workspace::encoder_for(profile, &profile.pages[0], &[], e);
    let target =
        |slot: Option<(Action, Layer)>| slot.unwrap().0.as_built_in().unwrap().target.clone();
    assert_eq!(target(plan(0).press), Some("42".into()));
    assert_eq!(target(plan(0).ccw), Some("42".into()));
    // The switcher's press mutes whichever output it just switched to.
    assert_eq!(built_in(&plan(1).press), Some(BuiltIn::VolumeMute));
    assert_eq!(target(plan(1).press), None);
    assert_eq!(plan(1).ring(), Some(RingShows::OutputPosition));
}

#[test]
fn a_knob_s_target_is_held_to_its_kind_s_shape() {
    let (_, diagnostics) = page(
        r##"
[[pages.encoders]]
encoder = 0
preset = "volume"
target = "Spotify Player"

[[pages.encoders]]
encoder = 1
target = "spotify"
"##,
    );
    assert_eq!(
        at(&diagnostics, "W0138"),
        ["profiles.work.pages[0].encoders[0].target"]
    );
    assert_eq!(
        at(&diagnostics, "W0126"),
        ["profiles.work.pages[0].encoders[1].target"],
        "{diagnostics:#?}"
    );
}

#[test]
fn a_target_on_an_action_is_held_to_what_its_built_in_names() {
    let (_, diagnostics) = page(
        r##"
[[pages.keys]]
key = 0
exec = { action = "volume_up", target = "-p" }

[[pages.keys]]
key = 1
exec = { action = "app_mute", target = "Firefox Nightly" }

[[pages.keys]]
key = 2
exec = { action = "set_output", target = "Headphones" }

[[pages.keys]]
key = 3
exec = { action = "set_output" }

[[pages.keys]]
key = 4
exec = { action = "play_pause", target = "spot\nify" }
"##,
    );
    assert_eq!(
        at(&diagnostics, "W0138"),
        [
            "profiles.work.pages[0].keys[0].exec",
            "profiles.work.pages[0].keys[4].exec"
        ],
        "{diagnostics:#?}"
    );
    assert_eq!(
        at(&diagnostics, "E0136"),
        ["profiles.work.pages[0].keys[3].exec"]
    );
}

#[test]
fn knob_only_built_ins_are_pointed_out_on_keys() {
    let (_, diagnostics) = page(
        r##"
[[pages.keys]]
key = 0
exec = { action = "next_mode" }

[[pages.keys]]
key = 1
hold = { action = "next_app" }

[[pages.encoders]]
encoder = 0
preset = "app_volume"
hold = { action = "next_mode" }
"##,
    );
    assert_eq!(
        at(&diagnostics, "W0127"),
        [
            "profiles.work.pages[0].keys[0].exec",
            "profiles.work.pages[0].keys[1].hold"
        ],
        "{diagnostics:#?}"
    );
}

#[test]
fn the_outputs_list_is_checked_entry_by_entry() {
    let (workspace, diagnostics) = load(&[
        (
            "galdeck.toml",
            "version = 2\noutputs = [\"Headphones\", \"\", \"Speaker\\u0007\", \"HDMI 1\"]\n",
        ),
        ("profiles/work.toml", "[[pages]]\nid = \"main\"\n"),
    ]);
    assert_eq!(
        at(&diagnostics, "W0139"),
        ["outputs[1]", "outputs[2]"],
        "{diagnostics:#?}"
    );
    assert_eq!(workspace.unwrap().global.outputs.len(), 4);
}

#[test]
fn a_mode_is_the_same_mode_however_its_step_is_written() {
    use std::collections::HashSet;
    let nan = ModeEntry {
        step: Some(f64::NAN),
        ..ModeEntry::of(Preset::Volume)
    };
    assert_eq!(nan, nan.clone());
    let set: HashSet<ModeEntry> = [nan.clone(), nan, ModeEntry::of(Preset::Volume)].into();
    assert_eq!(set.len(), 2);
}
