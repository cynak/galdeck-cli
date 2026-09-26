//! Timers and stopwatches: how a duration is read, what their keys do when
//! nothing else is bound, and what is said about the ways they cannot work.

use std::time::Duration;

use galdeck_model::{
    parse_duration, Action, BuiltIn, Diagnostic, Widget, WidgetKind, WidgetView, Workspace,
};

/// A one-page profile with this TOML after `[[pages]] id = "main"`.
fn page(body: &str) -> (Option<Workspace>, Vec<Diagnostic>) {
    let dir = std::env::temp_dir().join(format!(
        "galdeck-model-timers-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("profiles")).unwrap();
    std::fs::write(dir.join("galdeck.toml"), "version = 2\n").unwrap();
    std::fs::write(
        dir.join("profiles/work.toml"),
        format!("[[pages]]\nid = \"main\"\n{body}"),
    )
    .unwrap();
    let loaded = Workspace::load(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    loaded
}

/// The diagnostics with this code, by path.
fn at<'a>(diagnostics: &'a [Diagnostic], code: &str) -> Vec<&'a str> {
    diagnostics
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.path.as_str())
        .collect()
}

fn timer(duration: &str) -> Widget {
    Widget {
        duration: Some(duration.into()),
        ..Widget::of(WidgetKind::Timer)
    }
}

#[test]
fn a_duration_is_read_in_the_forms_people_write() {
    let seconds = |text: &str| parse_duration(text).map(|d| d.as_secs());
    assert_eq!(seconds("90s"), Some(90));
    assert_eq!(seconds("4m"), Some(240));
    assert_eq!(seconds("25m"), Some(1500));
    assert_eq!(seconds("1h30m"), Some(5400));
    assert_eq!(seconds("1h 30m"), Some(5400));
    assert_eq!(seconds("1m30s"), Some(90));
    assert_eq!(seconds(" 2h "), Some(7200));
    assert_eq!(seconds("4:30"), Some(270));
    assert_eq!(seconds("90:00"), Some(5400));
    assert_eq!(seconds("1:30:00"), Some(5400));
    assert_eq!(seconds("0s"), Some(0));
}

#[test]
fn a_duration_that_could_mean_two_things_is_no_duration() {
    for text in [
        "", "90", "h", "1.5h", "25M", "1 h", "30m1h", "1h1h", "1:5", "1:60", "1:30:5", "1:2:3:4",
        ":30", "1h:30", "-5m", "twenty",
    ] {
        assert_eq!(parse_duration(text), None, "{text:?}");
    }
}

#[test]
fn an_absurd_duration_is_none_rather_than_a_panic() {
    assert_eq!(parse_duration("99999999999999999999h"), None);
    assert_eq!(parse_duration("9999999999999999h"), None);
    assert_eq!(parse_duration("99999999999999999:00:00"), None);
    let (_, diagnostics) = page(
        "[[pages.keys]]\nkey = 0\n\n[pages.keys.widget]\nkind = \"timer\"\nduration = \"99999999999999999999h\"\n",
    );
    assert_eq!(
        at(&diagnostics, "E0180"),
        ["profiles.work.pages[0].keys[0].widget.duration"]
    );
}

#[test]
fn a_timer_needs_a_duration_it_can_read() {
    let (workspace, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0

[pages.keys.widget]
kind = "timer"

[[pages.keys]]
key = 1

[pages.keys.widget]
kind = "timer"
duration = "1:30"

[[pages.keys]]
key = 2

[pages.keys.widget]
kind = "timer"
duration = "ninety"
"#,
    );
    assert_eq!(
        at(&diagnostics, "E0180"),
        [
            "profiles.work.pages[0].keys[0].widget.duration",
            "profiles.work.pages[0].keys[2].widget.duration"
        ],
        "{diagnostics:#?}"
    );
    // What still loads draws the key and says so when tapped.
    let keys = &workspace.unwrap().profiles["work"].pages[0].keys;
    assert_eq!(keys[0].widget.as_ref().unwrap().duration(), None);
    assert_eq!(
        keys[1].widget.as_ref().unwrap().duration(),
        Some(Duration::from_secs(90))
    );
}

#[test]
fn a_timer_under_a_second_or_over_a_day_is_pointed_out() {
    let (_, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0

[pages.keys.widget]
kind = "timer"
duration = "0s"

[[pages.keys]]
key = 1

[pages.keys.widget]
kind = "timer"
duration = "25h"

[[pages.keys]]
key = 2

[pages.keys.widget]
kind = "timer"
duration = "24h"
"#,
    );
    assert_eq!(
        at(&diagnostics, "W0181"),
        [
            "profiles.work.pages[0].keys[0].widget.duration",
            "profiles.work.pages[0].keys[1].widget.duration"
        ],
        "{diagnostics:#?}"
    );
}

#[test]
fn timers_and_stopwatches_on_keys_load_cleanly() {
    let (workspace, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0
label = "Tea"

[pages.keys.widget]
kind = "timer"
duration = "4m"
view = "bar"
on_done = "notify-send 'Tea is ready'"

[[pages.keys]]
key = 1

[pages.keys.widget]
kind = "stopwatch"
"#,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let workspace = workspace.unwrap();
    let tea = workspace.profiles["work"].pages[0].keys[0]
        .widget
        .clone()
        .unwrap();
    assert_eq!(
        tea.on_done,
        Some(Action::Shell("notify-send 'Tea is ready'".into()))
    );
    assert_eq!(tea.fixed_max(), Some(240.0));
}

#[test]
fn a_timer_on_the_screen_can_never_start() {
    let (_, diagnostics) = page(
        r#"
[[pages.lcd]]
column = 0
row = 0
columns = 6

[pages.lcd.widget]
kind = "timer"
duration = "25m"

[[pages.lcd]]
column = 6
row = 0

[pages.lcd.widget]
kind = "stopwatch"
"#,
    );
    assert_eq!(
        at(&diagnostics, "W0182"),
        [
            "profiles.work.pages[0].lcd[0].widget.kind",
            "profiles.work.pages[0].lcd[1].widget.kind"
        ],
        "{diagnostics:#?}"
    );
}

#[test]
fn a_timer_key_taps_to_start_and_holds_to_reset() {
    let (workspace, _) = page(
        r#"
[[pages.keys]]
key = 0

[pages.keys.widget]
kind = "timer"
duration = "25m"

[[pages.keys]]
key = 1
hold = "notify-send held"

[pages.keys.widget]
kind = "stopwatch"

[[pages.keys]]
key = 2
exec = "notify-send tapped"

[pages.keys.widget]
kind = "timer"
duration = "25m"
"#,
    );
    let workspace = workspace.unwrap();
    let keys = &workspace.profiles["work"].pages[0].keys;
    assert_eq!(keys[0].tap(), Some(Action::built_in(BuiltIn::TimerToggle)));
    assert_eq!(
        keys[0].hold_action(),
        Some(Action::built_in(BuiltIn::TimerReset))
    );
    assert!(keys[0].is_bound());
    // A hold written on the key is the key's.
    assert_eq!(keys[1].tap(), Some(Action::built_in(BuiltIn::TimerToggle)));
    assert_eq!(
        keys[1].hold_action(),
        Some(Action::Shell("notify-send held".into()))
    );
    assert_eq!(keys[1].implicit_hold(), None);
    // A key whose tap was given away keeps its hold free as well.
    assert_eq!(keys[2].implicit_tap(), None);
    assert_eq!(keys[2].hold_action(), None);
}

#[test]
fn timer_built_ins_need_a_timer_on_their_key() {
    let (_, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0
exec = { action = "timer_toggle" }

[[pages.keys]]
key = 1
hold = { action = "timer_reset" }

[pages.keys.widget]
kind = "clock"

[[pages.keys]]
key = 2
double = { action = "timer_reset" }

[pages.keys.widget]
kind = "stopwatch"

[[pages.encoders]]
encoder = 0
press = { action = "timer_toggle" }
"#,
    );
    assert_eq!(
        at(&diagnostics, "W0183"),
        [
            "profiles.work.pages[0].keys[0].exec",
            "profiles.work.pages[0].keys[1].hold"
        ],
        "{diagnostics:#?}"
    );
    // On a knob there is no key to have a timer.
    assert_eq!(
        at(&diagnostics, "W0127"),
        ["profiles.work.pages[0].encoders[0].press"]
    );
}

#[test]
fn an_animation_on_a_timer_key_is_pointed_out() {
    let (_, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0

[pages.keys.animation]
kind = "pulse"
period_ms = 2000

[pages.keys.widget]
kind = "timer"
duration = "25m"
"#,
    );
    assert_eq!(
        at(&diagnostics, "W0184"),
        ["profiles.work.pages[0].keys[0].animation"],
        "{diagnostics:#?}"
    );
}

#[test]
fn on_done_is_for_a_timer_and_is_checked_like_any_action() {
    let (workspace, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0

[pages.keys.widget]
kind = "stopwatch"
on_done = "notify-send never"

[[pages.keys]]
key = 1

[pages.keys.widget]
kind = "timer"
duration = "4m"
on_done = { action = "push_to_talk" }

[[pages.keys]]
key = 2

[pages.keys.widget]
kind = "timer"
duration = "4m"
on_done = { keys = "ctrl+shift" }
"#,
    );
    assert_eq!(
        at(&diagnostics, "W0185"),
        ["profiles.work.pages[0].keys[0].widget.on_done"],
        "{diagnostics:#?}"
    );
    assert_eq!(
        at(&diagnostics, "W0127"),
        ["profiles.work.pages[0].keys[1].widget.on_done"]
    );
    assert_eq!(
        at(&diagnostics, "W0119"),
        ["profiles.work.pages[0].keys[2].widget.on_done"]
    );
    // Checks that run over every action on a key see it too.
    let workspace = workspace.unwrap();
    let key = &workspace.profiles["work"].pages[0].keys[2];
    assert!(key.actions().any(Action::needs_virtual_input));
}

#[test]
fn a_timer_draws_as_text_a_bar_a_gauge_or_tubes_and_a_stopwatch_as_text_or_tubes() {
    for view in [
        WidgetView::Text,
        WidgetView::Bar,
        WidgetView::Gauge,
        WidgetView::Nixie,
    ] {
        assert!(view.suits(WidgetKind::Timer), "{view:?}");
    }
    for view in [WidgetView::Graph, WidgetView::Analog] {
        assert!(!view.suits(WidgetKind::Timer), "{view:?}");
    }
    assert!(WidgetView::Text.suits(WidgetKind::Stopwatch));
    assert!(WidgetView::Nixie.suits(WidgetKind::Stopwatch));
    assert!(!WidgetView::Bar.suits(WidgetKind::Stopwatch));
    assert!(!WidgetKind::Timer.is_numeric() && !WidgetKind::Stopwatch.is_numeric());

    let (_, diagnostics) = page(
        r#"
[[pages.keys]]
key = 0

[pages.keys.widget]
kind = "timer"
duration = "25m"
view = "graph"

[[pages.keys]]
key = 1

[pages.keys.widget]
kind = "stopwatch"
view = "gauge"
warn = 60
"#,
    );
    assert_eq!(
        at(&diagnostics, "W0149"),
        [
            "profiles.work.pages[0].keys[0].widget.view",
            "profiles.work.pages[0].keys[1].widget.view"
        ],
        "{diagnostics:#?}"
    );
    assert_eq!(
        at(&diagnostics, "W0178"),
        ["profiles.work.pages[0].keys[1].widget.warn"]
    );
}

#[test]
fn a_bar_is_full_at_the_timer_s_length() {
    assert_eq!(timer("25m").fixed_max(), Some(1500.0));
    assert_eq!(timer("nonsense").fixed_max(), None);
    assert_eq!(timer("0s").fixed_max(), None, "nothing to fill");
    let capped = Widget {
        max: Some(60.0),
        ..timer("25m")
    };
    assert_eq!(capped.fixed_max(), Some(60.0));
    assert_eq!(Widget::of(WidgetKind::Stopwatch).fixed_max(), None);
}
