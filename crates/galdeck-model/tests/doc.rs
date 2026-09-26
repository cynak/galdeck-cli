//! The user interface will be editing files a human wrote. What it must not do
//! is quietly reformat them.

use galdeck_model::{ConfigDocument, Patch, Value};

const COMMENTED: &str = r##"# galdeck — my own config
#
# The comments here are the point.

version = 2

# Panel brightness, 0-100.
brightness = 60

profile = "work"   # trailing comments too
"##;

fn doc(text: &str) -> ConfigDocument {
    ConfigDocument::parse(std::path::Path::new("galdeck.toml"), text).expect("parses")
}

#[test]
fn an_untouched_document_round_trips_byte_for_byte() {
    assert_eq!(doc(COMMENTED).text(), COMMENTED);
}

#[test]
fn setting_a_value_leaves_every_comment_and_the_ordering_alone() {
    let d = doc(COMMENTED);
    let staged = d
        .preview(
            &[Patch::Set {
                path: "brightness".into(),
                value: Value::Integer(85),
            }],
            None,
        )
        .expect("a valid edit");

    assert!(staged.text.contains("# galdeck — my own config"));
    assert!(staged.text.contains("# Panel brightness, 0-100."));
    assert!(staged.text.contains("# trailing comments too"));
    assert!(staged.text.contains("brightness = 85"));
    assert!(!staged.text.contains("brightness = 60"));
    // Ordering is preserved: version still precedes brightness.
    let version_at = staged.text.find("version").unwrap();
    let brightness_at = staged.text.find("brightness").unwrap();
    assert!(version_at < brightness_at);
}

#[test]
fn a_new_key_is_added_without_disturbing_what_is_there() {
    let d = doc(COMMENTED);
    let staged = d
        .preview(
            &[Patch::Set {
                path: "font".into(),
                value: Value::String("/usr/share/fonts/TTF/DejaVuSans.ttf".into()),
            }],
            None,
        )
        .unwrap();
    assert!(staged.text.contains("# The comments here are the point."));
    assert!(staged
        .text
        .contains("font = \"/usr/share/fonts/TTF/DejaVuSans.ttf\""));
}

#[test]
fn a_path_reaches_into_an_array_of_tables() {
    let text = r##"
[[pages]]
id = "main"

[[pages.keys]]
key = 0
label = "Terminal"   # keep me

[[pages.keys]]
key = 1
label = "Browser"
"##;
    let d = doc(text);
    let staged = d
        .preview(
            &[Patch::Set {
                path: "pages[0].keys[1].label".into(),
                value: Value::String("Firefox".into()),
            }],
            None,
        )
        .unwrap();
    assert!(staged.text.contains("label = \"Firefox\""));
    assert!(
        staged.text.contains("# keep me"),
        "the sibling is untouched"
    );
    assert!(staged.text.contains("label = \"Terminal\""));
}

#[test]
fn removing_a_key_leaves_the_rest_intact() {
    let d = doc(COMMENTED);
    let staged = d
        .preview(
            &[Patch::Remove {
                path: "profile".into(),
            }],
            None,
        )
        .unwrap();
    assert!(!staged.text.contains("profile ="));
    assert!(staged.text.contains("brightness = 60"));
    assert!(staged.text.contains("# galdeck — my own config"));
}

#[test]
fn removing_something_absent_is_not_an_error() {
    // It is the state the caller asked for.
    let d = doc(COMMENTED);
    let staged = d
        .preview(
            &[Patch::Remove {
                path: "not_here".into(),
            }],
            None,
        )
        .unwrap();
    assert_eq!(staged.text, COMMENTED);
}

#[test]
fn an_edit_built_on_stale_content_is_refused() {
    // Two browser tabs, or a hand edit between read and save. Without this the
    // second save silently discards the first.
    let mut d = doc(COMMENTED);
    let staged = d
        .preview(
            &[Patch::Set {
                path: "brightness".into(),
                value: Value::Integer(70),
            }],
            Some(0),
        )
        .unwrap();
    d.commit(staged).unwrap();
    assert_eq!(d.generation(), 1);

    let err = d
        .preview(
            &[Patch::Set {
                path: "brightness".into(),
                value: Value::Integer(90),
            }],
            Some(0),
        )
        .expect_err("a stale edit must be refused");
    let stale = err.iter().find(|d| d.code == "E0150").unwrap();
    assert!(stale.help.as_deref().unwrap().contains("re-read"));
}

#[test]
fn a_bad_path_is_reported_rather_than_silently_ignored() {
    let d = doc(COMMENTED);
    let err = d
        .preview(
            &[Patch::Set {
                path: "brightness.nested".into(),
                value: Value::Integer(1),
            }],
            None,
        )
        .expect_err("cannot descend into an integer");
    assert!(err.iter().any(|d| d.code == "E0151"));
}

#[test]
fn many_edits_apply_together_or_not_at_all() {
    let d = doc(COMMENTED);
    let err = d
        .preview(
            &[
                Patch::Set {
                    path: "brightness".into(),
                    value: Value::Integer(10),
                },
                Patch::Set {
                    path: "brightness.nope".into(),
                    value: Value::Integer(1),
                },
            ],
            None,
        )
        .expect_err("the batch fails");
    assert!(err.has_errors());
    // The document itself is untouched -- preview never mutates.
    assert_eq!(d.text(), COMMENTED);
}

#[test]
fn saving_writes_atomically_and_keeps_a_backup() {
    let dir = std::env::temp_dir().join(format!("galdeck-doc-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("galdeck.toml");
    std::fs::write(&path, COMMENTED).unwrap();

    let mut d = ConfigDocument::load(&path).unwrap();
    let staged = d
        .preview(
            &[Patch::Set {
                path: "brightness".into(),
                value: Value::Integer(42),
            }],
            None,
        )
        .unwrap();
    d.commit(staged).unwrap();
    d.save().unwrap();

    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("brightness = 42"));
    assert!(written.contains("# The comments here are the point."));

    let backup = std::fs::read_to_string(dir.join("galdeck.toml.bak")).unwrap();
    assert_eq!(backup, COMMENTED, "the previous contents are recoverable");

    // No staging file is left behind.
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with("galdeck.toml.") && !name.ends_with(".bak"))
        .collect();
    assert!(leftovers.is_empty(), "left behind {leftovers:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_edited_config_still_loads_as_a_workspace() {
    // The point of the split: edit as a document, then re-read as the model.
    // If these two ever disagree the user interface can write something the
    // daemon refuses.
    let text = "version = 2\nbrightness = 60\nprofile = \"work\"\n";
    let d = doc(text);
    let staged = d
        .preview(
            &[Patch::Set {
                path: "brightness".into(),
                value: Value::Integer(15),
            }],
            None,
        )
        .unwrap();
    let global: galdeck_model::Global = toml::from_str(&staged.text).expect("still valid");
    assert_eq!(global.brightness, 15);
}

const STATES: &str = r##"[[pages]]
id = "main"

[[pages.keys]]
key = 0
label = "Power"

# Quiet, for the train.
[[pages.keys.states]]
name = "power-saver"
label = "Saver"   # short, to fit

# The default.
[[pages.keys.states]]
name = "balanced"
style = { key_bg = "#5e81ac" }

# Loud.
[[pages.keys.states]]
name = "performance"

[[pages.keys]]
key = 1
label = "Next"
"##;

fn names_in_order(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| line.strip_prefix("name = \""))
        .map(|rest| rest.trim_end_matches('"'))
        .collect()
}

fn moved(text: &str, from: usize, to: usize) -> String {
    doc(text)
        .preview(
            &[Patch::Move {
                path: "pages[0].keys[0].states".into(),
                from,
                to,
            }],
            None,
        )
        .expect("a valid move")
        .text
}

#[test]
fn moving_a_table_puts_it_in_its_new_place_with_its_comments() {
    let text = moved(STATES, 0, 2);
    assert_eq!(
        names_in_order(&text),
        ["balanced", "performance", "power-saver"]
    );
    // Each table's comments and trailing comments went with it.
    let quiet = text.find("# Quiet, for the train.").unwrap();
    let saver = text.find("name = \"power-saver\"").unwrap();
    let loud = text.find("# Loud.").unwrap();
    assert!(loud < quiet && quiet < saver, "{text}");
    assert!(text.contains("label = \"Saver\"   # short, to fit"));
    assert!(text[text.find("# The default.").unwrap()..].starts_with(
        "# The default.\n[[pages.keys.states]]\nname = \"balanced\"\nstyle = { key_bg = \"#5e81ac\" }"
    ));
    // Nothing outside the array moved.
    assert!(text.find("key = 1").unwrap() > saver);
    assert!(text.starts_with("[[pages]]\nid = \"main\"\n\n[[pages.keys]]\nkey = 0\n"));
}

#[test]
fn moving_a_table_back_again_restores_the_file() {
    let there = moved(STATES, 2, 0);
    assert_eq!(
        names_in_order(&there),
        ["performance", "power-saver", "balanced"]
    );
    assert_eq!(moved(&there, 0, 2), STATES);
}

#[test]
fn a_moved_table_takes_the_tables_inside_it_along() {
    let text = r##"[[pages]]
id = "main"

[[pages.keys]]
key = 0

[[pages.keys.states]]
name = "off"

[pages.keys.states.style]
key_bg = "#3b4252"   # grey

[[pages.keys.states]]
name = "on"

[pages.keys.states.style]
key_bg = "#a3be8c"
"##;
    let staged = moved(text, 1, 0);
    assert_eq!(names_in_order(&staged), ["on", "off"]);
    let on = staged.find("name = \"on\"").unwrap();
    let green = staged.find("#a3be8c").unwrap();
    let off = staged.find("name = \"off\"").unwrap();
    let grey = staged.find("key_bg = \"#3b4252\"   # grey").unwrap();
    assert!(on < green && green < off && off < grey, "{staged}");

    // And it still reads as the same two states, in the new order.
    let profile: toml::Value = toml::from_str(&staged).unwrap();
    let states = profile["pages"][0]["keys"][0]["states"].as_array().unwrap();
    assert_eq!(states[0]["name"].as_str(), Some("on"));
    assert_eq!(states[0]["style"]["key_bg"].as_str(), Some("#a3be8c"));
    assert_eq!(states[1]["style"]["key_bg"].as_str(), Some("#3b4252"));
}

#[test]
fn a_move_leaves_the_key_s_own_tables_where_they_belong() {
    // The key's style is written between two of its states, and the first
    // state's own style after it: toml puts each under the header above it,
    // so a move must not hand the key's table to a state or the other way.
    let text = r##"[[pages]]
id = "main"

[[pages.keys]]
key = 0

[[pages.keys.states]]
name = "a"

[pages.keys.style]
key_bg = "#ffffff"

[pages.keys.states.style]
key_bg = "#000000"

[[pages.keys.states]]
name = "b"

[[pages.keys.states]]
name = "c"

[[pages.keys]]
key = 1
"##;
    let before: toml::Value = toml::from_str(text).unwrap();
    for (from, to) in [(0, 2), (2, 0), (1, 0), (0, 1)] {
        let staged = moved(text, from, to);
        let after: toml::Value = toml::from_str(&staged).expect("still valid");
        let mut states = before["pages"][0]["keys"][0]["states"]
            .as_array()
            .unwrap()
            .clone();
        let state = states.remove(from);
        states.insert(to, state);
        let keys = &after["pages"][0]["keys"];
        assert_eq!(keys[0]["states"].as_array(), Some(&states), "{staged}");
        assert_eq!(keys[0]["style"], before["pages"][0]["keys"][0]["style"]);
        assert_eq!(keys[1], before["pages"][0]["keys"][1]);
        assert_eq!(moved(&staged, to, from), text, "{from} to {to} and back");
    }
}

#[test]
fn a_table_added_in_the_same_edit_can_be_moved() {
    let d = doc(STATES);
    let staged = d
        .preview(
            &[
                Patch::Append {
                    path: "pages[0].keys[0].states".into(),
                    fields: [("name".to_string(), Value::String("turbo".into()))].into(),
                },
                Patch::Move {
                    path: "pages[0].keys[0].states".into(),
                    from: 3,
                    to: 0,
                },
            ],
            None,
        )
        .unwrap();
    assert_eq!(
        names_in_order(&staged.text),
        ["turbo", "power-saver", "balanced", "performance"]
    );
}

#[test]
fn a_move_to_nowhere_is_refused() {
    let d = doc(STATES);
    for (path, from, to) in [
        ("pages[0].keys[0].states", 0, 3),
        ("pages[0].keys[0].states", 5, 0),
        ("pages[0].keys[1].states", 0, 0),
        ("pages[0].keys[0].label", 0, 1),
    ] {
        let err = d
            .preview(
                &[Patch::Move {
                    path: path.into(),
                    from,
                    to,
                }],
                None,
            )
            .expect_err("nothing to move");
        assert!(err.iter().any(|d| d.code == "E0151"), "{path} {from} {to}");
    }
    // Moving a table onto itself is no change at all.
    assert_eq!(moved(STATES, 1, 1), STATES);
}
