//! Keys that step through states: how they are written, how a state changes
//! the key's look, what tapping does, how `status` output is read, and what
//! is said about the ways they cannot work. Also icons, which states brought
//! in: a name from the icon theme or a file.

use galdeck_model::v2::KeyConfig;
use galdeck_model::{
    status_value, Action, BuiltIn, ColorRef, Diagnostic, IconRef, KeyPart, StyleLayer, Workspace,
};

/// Load a workspace from whole files.
fn load(files: &[(&str, &str)]) -> (Option<Workspace>, Vec<Diagnostic>) {
    let dir = std::env::temp_dir().join(format!(
        "galdeck-model-states-{}-{:?}",
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

/// The first key of a page that loaded.
fn first_key(workspace: Option<Workspace>) -> KeyConfig {
    workspace.expect("loads").profiles["work"].pages[0].keys[0].clone()
}

fn key(toml: &str) -> KeyConfig {
    toml::from_str(toml).expect("a key")
}

const K: &str = "profiles.work.pages[0].keys[0]";

/// The Wi-Fi toggle, as the gallery writes it.
const WIFI: &str = r##"
[[pages.keys]]
key = 0
label = "Wi-Fi"
status = "nmcli radio wifi"

[[pages.keys.states]]
name = "off"
match = ["disabled"]
label = "Wi-Fi off"
icon = "network-wireless-disabled-symbolic"
style = { key_bg = "#3b4252" }
exec = "nmcli radio wifi off"

[[pages.keys.states]]
name = "on"
match = ["enabled"]
label = "Wi-Fi"
icon = "network-wireless-symbolic"
style = { key_bg = "#5e81ac" }
exec = "nmcli radio wifi on"
"##;

#[test]
fn a_toggle_as_the_gallery_writes_it_loads_with_nothing_to_say() {
    let (workspace, diagnostics) = page(WIFI);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let key = first_key(workspace);
    assert_eq!(key.status.as_deref(), Some("nmcli radio wifi"));
    assert_eq!(key.states.len(), 2);
    let on = &key.states[1];
    assert_eq!(on.name, "on");
    assert_eq!(on.matches, ["enabled"]);
    assert_eq!(on.label.as_deref(), Some("Wi-Fi"));
    assert_eq!(
        on.icon,
        Some(IconRef::Name("network-wireless-symbolic".into()))
    );
    assert_eq!(on.exec, Some(Action::Shell("nmcli radio wifi on".into())));
    assert_eq!(key.status_interval_ms(), 5000);
}

#[test]
fn a_state_takes_only_the_fields_it_knows() {
    // `matches` is the Rust name; `match` is what is written.
    for stray in ["matches = [\"x\"]", "sync = \"true\"", "hold = \"ls\""] {
        let text = format!("key = 0\n[[states]]\nname = \"on\"\n{stray}\n");
        assert!(toml::from_str::<KeyConfig>(&text).is_err(), "{stray}");
    }
    // The key itself no longer takes the name the design started with.
    assert!(toml::from_str::<KeyConfig>("key = 0\nsync = \"true\"\n").is_err());
}

#[test]
fn an_icon_is_a_file_when_it_has_a_slash_or_starts_at_home() {
    for (text, file) in [
        ("/usr/share/pixmaps/firefox.png", true),
        ("~/Pictures/deck.png", true),
        ("~", true),
        ("icons/mute.svg", true),
        ("./mute.png", true),
        ("audio-volume-muted-symbolic", false),
        ("mute.png", false),
        ("", false),
    ] {
        let icon = IconRef::parse(text);
        assert_eq!(icon.as_path().is_some(), file, "{text:?}");
        assert_eq!(icon.name().is_some(), !file, "{text:?}");
        assert_eq!(icon.to_string(), text, "written back as it was");
    }
    let key = key("key = 0\nicon = \"~/deck/mic.png\"\n");
    assert_eq!(
        key.icon.as_ref().and_then(IconRef::as_path),
        Some(std::path::Path::new("~/deck/mic.png"))
    );
}

#[test]
fn an_icon_name_is_one_a_theme_could_have() {
    for good in [
        "audio-volume-muted",
        "network-wireless-symbolic",
        "org.gnome.Settings",
        "gtk+-3.0",
        "a_b",
        &"x".repeat(128),
    ] {
        assert!(IconRef::is_valid_name(good), "{good:?}");
    }
    for bad in [
        "",
        ".hidden",
        "..",
        "two words",
        "back\\slash",
        "ünïcode",
        "tab\there",
        &"x".repeat(129),
    ] {
        assert!(!IconRef::is_valid_name(bad), "{bad:?}");
    }
}

#[test]
fn an_icon_that_is_neither_a_name_nor_a_file_is_an_error_and_a_file_name_is_warned_about() {
    let (workspace, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0
icon = "..secret"

[[pages.keys]]
key = 1
icon = "mute.PNG"

[[pages.keys]]
key = 2
icon = "/home/me/mute.png"

[[pages.keys]]
key = 3
icon = "microphone-sensitivity-muted-symbolic"

[[pages.keys.states]]
name = "a"
icon = "two words"

[[pages.keys.states]]
name = "b"
icon = "logo.svg"
"#,
    );
    assert_eq!(
        at(&diagnostics, "E0193"),
        [
            "profiles.work.pages[0].keys[0].icon",
            "profiles.work.pages[0].keys[3].states[0].icon"
        ],
        "{diagnostics:#?}"
    );
    assert_eq!(
        at(&diagnostics, "W0194"),
        [
            "profiles.work.pages[0].keys[1].icon",
            "profiles.work.pages[0].keys[3].states[1].icon"
        ]
    );
    let help = diagnostics
        .iter()
        .find(|d| d.code == "W0194")
        .and_then(|d| d.help.as_deref())
        .unwrap();
    assert!(help.contains("whole path"), "{help}");
    // An error in an icon is still an error in a config that loads, as the
    // daemon draws what it can.
    assert!(workspace.is_some());
}

#[test]
fn a_version_1_image_stays_a_file_whatever_it_looks_like() {
    let v1 = galdeck_model::Config::parse(
        "[[pages]]\nname = \"main\"\n\n[[pages.keys]]\nkey = 0\nimage = \"mute.png\"\nexec = \"true\"\n",
    )
    .unwrap();
    let workspace = Workspace::from_v1(&v1);
    let key = &workspace.profiles["default"].pages[0].keys[0];
    assert_eq!(key.icon, Some(IconRef::Path("mute.png".into())));
    assert!(key.states.is_empty() && key.status.is_none());
    assert!(!workspace.validate().iter().any(|d| d.code == "W0194"));
}

#[test]
fn an_icon_theme_is_the_name_of_one_directory() {
    let global = |text: &str| {
        load(&[
            ("galdeck.toml", &format!("version = 2\n{text}")),
            ("profiles/work.toml", "[[pages]]\nid = \"main\"\n"),
        ])
    };
    let (workspace, diagnostics) = global("icon_theme = \"Yaru\"\n");
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert_eq!(
        workspace.unwrap().global.icon_theme.as_deref(),
        Some("Yaru")
    );
    assert_eq!(galdeck_model::Global::default().icon_theme, None);
    for bad in ["../../etc", "", "..", "Yaru/48x48"] {
        let (_, diagnostics) = global(&format!("icon_theme = {bad:?}\n"));
        assert_eq!(at(&diagnostics, "W0196"), ["icon_theme"], "{bad:?}");
    }
}

#[test]
fn a_layer_over_another_takes_only_what_it_sets() {
    let base = StyleLayer {
        key_bg: Some(ColorRef::parse("#111111").unwrap()),
        key_label_color: Some(ColorRef::parse("#eeeeee").unwrap()),
        key_label_size: Some(20.0),
        ..StyleLayer::default()
    };
    let top = StyleLayer {
        key_bg: Some(ColorRef::parse("@accent").unwrap()),
        key_label_strip: Some(30),
        ..StyleLayer::default()
    };
    let both = base.over(&top);
    assert_eq!(both.key_bg, top.key_bg);
    assert_eq!(both.key_label_color, base.key_label_color);
    assert_eq!(both.key_label_size, Some(20.0));
    assert_eq!(both.key_label_strip, Some(30));
    assert!(both.ring.is_none());
    assert!(StyleLayer::default()
        .over(&StyleLayer::default())
        .is_empty());
}

#[test]
fn a_key_in_a_state_looks_like_the_state_where_it_says_and_like_the_key_elsewhere() {
    let key = key(r##"
key = 0
label = "Power"
icon = "power-profile-balanced-symbolic"
style = { key_bg = "#222222", key_label_color = "#ffffff" }
animation = { kind = "blink" }
status = "powerprofilesctl get"

[[states]]
name = "saver"
label = "Saver"
style = { key_bg = "#a3be8c" }

[[states]]
name = "Performance"
icon = "/usr/share/icons/perf.png"
animation = { kind = "pulse", period_ms = 3000 }
"##);
    let saver = key.in_state("saver");
    assert_eq!(saver.label.as_deref(), Some("Saver"));
    assert_eq!(saver.icon, key.icon, "the state has none of its own");
    assert_eq!(
        saver.style.key_bg,
        Some(ColorRef::parse("#a3be8c").unwrap())
    );
    assert_eq!(saver.style.key_label_color, key.style.key_label_color);
    assert_eq!(saver.animation, key.animation);

    // Names are found ignoring case, as they are unique ignoring it.
    let performance = key.in_state("performance");
    assert_eq!(performance.label.as_deref(), Some("Power"));
    assert_eq!(
        performance.icon,
        Some(IconRef::Path("/usr/share/icons/perf.png".into()))
    );
    assert_eq!(performance.animation.as_ref().unwrap().period_ms, 3000);
    assert_eq!(performance.style.key_bg, key.style.key_bg);

    // A look, not a binding: it has no states, so nothing to step through.
    for look in [&saver, &performance, &key.in_state("turbo")] {
        assert!(look.states.is_empty() && look.status.is_none());
        assert!(look.status_interval_ms.is_none());
    }
    let unknown = key.in_state("turbo");
    assert_eq!(unknown.label, key.label);
    assert_eq!(unknown.style.key_bg, key.style.key_bg);
    assert_eq!(key.state("SAVER").map(|s| s.name.as_str()), Some("saver"));
    assert!(key.state("turbo").is_none());
}

#[test]
fn tapping_a_key_with_states_moves_it_on_even_with_a_widget_beside_them() {
    let states = "[[states]]\nname = \"a\"\n[[states]]\nname = \"b\"\n";
    let plain = key(&format!("key = 0\n{states}"));
    assert_eq!(
        plain.implicit_tap(),
        Some(Action::built_in(BuiltIn::NextState))
    );
    assert_eq!(plain.tap(), Some(Action::built_in(BuiltIn::NextState)));
    assert!(plain.is_bound());
    // A timer beside states is an error; the tap still steps, and holding
    // does not reset a timer the tap no longer runs.
    let timed = key(&format!(
        "key = 0\nwidget = {{ kind = \"timer\", duration = \"5m\" }}\n{states}"
    ));
    assert_eq!(
        timed.implicit_tap(),
        Some(Action::built_in(BuiltIn::NextState))
    );
    assert_eq!(timed.implicit_hold(), None);
    // A tap that is bound says what the key does, as it always has.
    let bound = key(&format!("key = 0\nexec = \"true\"\n{states}"));
    assert_eq!(bound.implicit_tap(), None);
    assert_eq!(bound.tap(), Some(Action::Shell("true".into())));
}

#[test]
fn every_action_on_a_key_includes_what_its_states_run() {
    let key = key(r#"
key = 0
hold = { keys = "ctrl+c" }

[[states]]
name = "a"
exec = { keys = "super+a" }

[[states]]
name = "b"

[[states]]
name = "c"
exec = "true"
"#);
    let actions: Vec<&Action> = key.actions().collect();
    assert_eq!(
        actions,
        [
            &Action::keys("ctrl+c"),
            &Action::keys("super+a"),
            &Action::Shell("true".into())
        ]
    );
}

#[test]
fn state_built_ins_act_on_the_states_of_their_own_key() {
    for built_in in [BuiltIn::NextState, BuiltIn::PreviousState] {
        assert_eq!(built_in.acts_on(), Some(KeyPart::States));
        assert_eq!(built_in.group(), "Toggle");
        assert!(!built_in.takes_target() && built_in.step().is_none());
    }
    let (_, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0
exec = { action = "next_state" }

[[pages.keys]]
key = 1
hold = { action = "previous_state" }

[[pages.keys.states]]
name = "a"

[[pages.keys.states]]
name = "b"

[[pages.keys]]
key = 2
double = { action = "timer_reset" }

[[pages.encoders]]
encoder = 0
press = { action = "previous_state" }
"#,
    );
    // Only the key that has no states is told so, and not that it has no
    // timer, which a state built-in never needed.
    assert_eq!(
        at(&diagnostics, "W0192"),
        ["profiles.work.pages[0].keys[0].exec"],
        "{diagnostics:#?}"
    );
    assert_eq!(
        at(&diagnostics, "W0183"),
        ["profiles.work.pages[0].keys[2].double"]
    );
    let knob = diagnostics.iter().find(|d| d.code == "W0127").unwrap();
    assert_eq!(knob.path, "profiles.work.pages[0].encoders[0].press");
    assert_eq!(
        knob.help.as_deref(),
        Some("it acts on the key it is bound to")
    );
}

#[test]
fn what_a_state_runs_cannot_change_the_state_or_need_the_key_held() {
    let (workspace, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0

[[pages.keys.states]]
name = "a"
exec = { action = "next_state" }

[[pages.keys.states]]
name = "b"
exec = { action = "previous_state" }

[[pages.keys.states]]
name = "c"
exec = { action = "timer_toggle" }

[[pages.keys.states]]
name = "d"
exec = { action = "push_to_talk" }

[[pages.keys.states]]
name = "e"
exec = { action = "next_mode" }

[[pages.keys.states]]
name = "f"
exec = { action = "mic_mute" }

[[pages.keys.states]]
name = "g"
exec = ""
"#,
    );
    assert_eq!(
        at(&diagnostics, "E0189"),
        [
            format!("{K}.states[0].exec"),
            format!("{K}.states[1].exec"),
            format!("{K}.states[2].exec"),
            format!("{K}.states[3].exec"),
        ],
        "{diagnostics:#?}"
    );
    // Not also warned about as misplaced, which says the same thing less.
    assert_eq!(at(&diagnostics, "W0127"), [format!("{K}.states[4].exec")]);
    assert_eq!(at(&diagnostics, "W0122"), [format!("{K}.states[6].exec")]);
    let helps: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == "E0189")
        .filter_map(|d| d.help.as_deref())
        .collect();
    assert!(helps[0].contains("never stop"), "{helps:?}");
    assert!(helps[2].contains("no timer"), "{helps:?}");
    assert!(helps[3].contains("held down"), "{helps:?}");
    // Configs with errors still load; the daemon refuses these as well.
    assert!(workspace.is_some());
}

#[test]
fn a_key_steps_through_two_to_eight_states() {
    let with = |count: usize| {
        let states: String = (0..count)
            .map(|n| format!("[[pages.keys.states]]\nname = \"s{n}\"\n"))
            .collect();
        page(&format!("[[pages.keys]]\nkey = 0\n{states}")).1
    };
    for count in [2, 8] {
        assert!(at(&with(count), "E0186").is_empty(), "{count}");
    }
    for count in [1, 9] {
        let diagnostics = with(count);
        assert_eq!(
            at(&diagnostics, "E0186"),
            [format!("{K}.states")],
            "{count}"
        );
        let message = &diagnostics
            .iter()
            .find(|d| d.code == "E0186")
            .unwrap()
            .message;
        assert!(message.contains(&format!("has {count}")), "{message}");
    }
}

#[test]
fn every_state_has_a_name_of_its_own_whatever_its_capitals() {
    let (_, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0

[[pages.keys.states]]
name = "On"

[[pages.keys.states]]
name = "on"

[[pages.keys.states]]
name = "  "

[[pages.keys.states]]
label = "no name at all"
"#,
    );
    assert_eq!(
        at(&diagnostics, "E0187"),
        [
            format!("{K}.states[1].name"),
            format!("{K}.states[2].name"),
            format!("{K}.states[3].name"),
        ],
        "{diagnostics:#?}"
    );
    let twice = diagnostics.iter().find(|d| d.code == "E0187").unwrap();
    assert!(twice.message.contains("states[0]"), "{}", twice.message);
    // The same names are one problem, not also two states matching alike.
    assert!(at(&diagnostics, "W0191").is_empty(), "{diagnostics:#?}");
}

#[test]
fn states_own_the_tap_and_leave_no_room_for_a_widget() {
    let states = "[[pages.keys.states]]\nname = \"a\"\n[[pages.keys.states]]\nname = \"b\"\n";
    let (_, diagnostics) = page(&format!(
        r#"
[[pages]]
id = "other"

[[pages.keys]]
key = 0
exec = "true"
page = "main"
profile = "work"
back = true
plugin = {{ id = "clock" }}
widget = {{ kind = "clock" }}
{states}
[[pages.keys]]
key = 1
hold = "true"
double = {{ action = "previous_state" }}
{states}"#
    ));
    let k = "profiles.work.pages[1].keys[0]";
    assert_eq!(
        at(&diagnostics, "E0188"),
        [
            format!("{k}.exec"),
            format!("{k}.page"),
            format!("{k}.profile"),
            format!("{k}.back"),
            format!("{k}.plugin"),
            format!("{k}.widget"),
        ],
        "{diagnostics:#?}"
    );
}

#[test]
fn how_often_the_state_is_read_is_said_only_where_it_is_read() {
    let (_, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0
status_interval_ms = 10000

[[pages.keys.states]]
name = "a"

[[pages.keys.states]]
name = "b"

[[pages.keys]]
key = 1
status = "cat /tmp/state"
status_interval_ms = 500

[[pages.keys.states]]
name = "a"

[[pages.keys.states]]
name = "b"

[[pages.keys]]
key = 2
status_interval_ms = 2000
"#,
    );
    assert_eq!(
        at(&diagnostics, "W0190"),
        [
            "profiles.work.pages[0].keys[0].status_interval_ms",
            "profiles.work.pages[0].keys[1].status_interval_ms",
            "profiles.work.pages[0].keys[2].status_interval_ms",
        ],
        "{diagnostics:#?}"
    );
    let clamped = &diagnostics
        .iter()
        .find(|d| d.code == "W0190" && d.path.contains("keys[1]"))
        .unwrap()
        .message;
    assert!(clamped.contains("clamped"), "{clamped}");

    let interval = |text: &str| key(&format!("key = 0\n{text}")).status_interval_ms();
    assert_eq!(interval(""), 5000);
    assert_eq!(interval("status_interval_ms = 500"), 2000);
    assert_eq!(interval("status_interval_ms = 30000"), 30000);
}

#[test]
fn a_status_without_states_has_nothing_to_read_into() {
    let (_, diagnostics) = page(
        "[[pages.keys]]\nkey = 0\nexec = \"true\"\nstatus = \"nmcli radio wifi\"\n\n[[pages.keys]]\nkey = 1\nstatus = \" \"\n[[pages.keys.states]]\nname = \"a\"\n[[pages.keys.states]]\nname = \"b\"\n",
    );
    assert_eq!(at(&diagnostics, "W0195"), [format!("{K}.status")]);
    assert_eq!(
        at(&diagnostics, "W0122"),
        ["profiles.work.pages[0].keys[1].status"]
    );
}

#[test]
fn what_status_prints_can_mean_only_one_state() {
    let (_, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0
status = "gsettings get org.gnome.desktop.interface color-scheme"

[[pages.keys.states]]
name = "light"
match = ["'default'", "prefer-light"]

[[pages.keys.states]]
name = "dark"
match = ["prefer-dark", "DEFAULT"]

[[pages.keys.states]]
name = "Prefer-Light"
"#,
    );
    assert_eq!(
        at(&diagnostics, "W0191"),
        [
            format!("{K}.states[1].match"),
            format!("{K}.states[2].name")
        ],
        "{diagnostics:#?}"
    );
    assert!(diagnostics
        .iter()
        .any(|d| d.code == "W0191" && d.message.contains("\"default\"")));
}

#[test]
fn names_that_differ_only_by_quotes_are_different_names_that_match_alike() {
    let (_, diagnostics) = page(
        "[[pages.keys]]\nkey = 0\n[[pages.keys.states]]\nname = \"'on'\"\n[[pages.keys.states]]\nname = \"on\"\n",
    );
    assert!(at(&diagnostics, "E0187").is_empty(), "{diagnostics:#?}");
    assert_eq!(
        at(&diagnostics, "W0191"),
        [format!("{K}.states[1].name")],
        "{diagnostics:#?}"
    );
}

#[test]
fn status_output_is_read_from_its_first_line_without_quotes_or_case() {
    assert_eq!(
        status_value("\n  'prefer-dark'  \nmore\n").as_deref(),
        Some("prefer-dark")
    );
    assert_eq!(status_value("\"on\"").as_deref(), Some("on"));
    // One pair only, and only a pair.
    assert_eq!(status_value("''x''").as_deref(), Some("'x'"));
    assert_eq!(status_value("'half").as_deref(), Some("'half"));
    assert_eq!(status_value("'").as_deref(), Some("'"));
    assert_eq!(status_value(""), None);
    assert_eq!(status_value(" \n\t\n"), None);
    let long = "y".repeat(500);
    assert_eq!(status_value(&long).map(|v| v.len()), Some(128));

    let key = key(r#"
key = 0
status = "true"

[[states]]
name = "off"
match = ["disabled", "'no'"]

[[states]]
name = "on"
"#);
    let found = |output: &str| key.state_matching(output).map(|s| s.name.as_str());
    assert_eq!(found("disabled\n"), Some("off"));
    assert_eq!(found("NO"), Some("off"));
    assert_eq!(found("'no'"), Some("off"));
    // A state with no `match` is matched by its name.
    assert_eq!(found("On"), Some("on"));
    assert_eq!(found("  \n'on'\n"), Some("on"));
    assert_eq!(found("enabled"), None);
    assert_eq!(found(""), None);
}

#[test]
fn a_state_s_look_is_checked_where_it_is_written() {
    let (_, diagnostics) = load(&[
        ("galdeck.toml", "version = 2\n"),
        ("themes/t.toml", "[palette]\naccent = \"#88c0d0\"\n"),
        (
            "profiles/work.toml",
            r#"theme = "t"

[[pages]]
id = "main"

[[pages.keys]]
key = 0
style = { key_bg = "@accent" }

[[pages.keys.states]]
name = "a"
style = { key_bg = "@missing" }
animation = { kind = "spin" }

[[pages.keys.states]]
name = "b"
style = { key_label_color = "@accent" }
animation = { kind = "pulse", period_ms = 50 }
"#,
        ),
    ]);
    assert_eq!(
        at(&diagnostics, "E0117"),
        [format!("{K}.states[0].style.key_bg")],
        "{diagnostics:#?}"
    );
    assert_eq!(
        at(&diagnostics, "E0140"),
        [format!("{K}.states[0].animation.kind")]
    );
    assert_eq!(
        at(&diagnostics, "W0141"),
        [format!("{K}.states[1].animation.period_ms")]
    );
}

#[test]
fn a_key_with_states_is_not_said_to_do_nothing() {
    let (_, diagnostics) =
        page("[[pages.keys]]\nkey = 0\n[[pages.keys.states]]\nname = \"a\"\n[[pages.keys.states]]\nname = \"b\"\n");
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn a_second_page_with_an_id_is_told_its_keys_have_no_states_or_timers() {
    // `next_page` reaches it by its place in the list, so it is not out of
    // reach; but a key's states and timer are kept by its page's id, and the
    // id is the first page's.
    let (workspace, diagnostics) = page(&format!("{WIFI}\n[[pages]]\nid = \"main\"\n{WIFI}"));
    assert!(workspace.is_some());
    assert_eq!(
        at(&diagnostics, "W0103"),
        ["profiles.work.pages[1].id"],
        "{diagnostics:#?}"
    );
    let warning = diagnostics.iter().find(|d| d.code == "W0103").unwrap();
    assert!(
        warning.message.contains("no states or timers"),
        "{}",
        warning.message
    );
    assert_eq!(warning.help.as_deref(), Some("rename one of them"));
}
