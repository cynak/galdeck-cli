//! `galdeck ui` against a daemon that is a Unix socket in a temp directory,
//! with the browser replaced by a closure, so no test here ever starts one.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;

const CODE: &str = "0123456789abcdef0123456789abcdef";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("galdeck-cli-ui-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A daemon that answers each connection with the next of `replies`, and
/// keeps every request it was sent.
struct FakeDaemon {
    socket: PathBuf,
    seen: Arc<Mutex<Vec<serde_json::Value>>>,
    dir: PathBuf,
}

impl FakeDaemon {
    fn start(name: &str, replies: &[&str]) -> Self {
        let dir = scratch(name);
        let socket = dir.join("galdeck.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let replies: Vec<String> = replies.iter().map(|r| r.to_string()).collect();
        {
            let seen = Arc::clone(&seen);
            std::thread::spawn(move || {
                for reply in replies {
                    let Ok((stream, _)) = listener.accept() else {
                        return;
                    };
                    let mut line = String::new();
                    BufReader::new(stream.try_clone().unwrap())
                        .read_line(&mut line)
                        .unwrap();
                    seen.lock()
                        .unwrap()
                        .push(serde_json::from_str(&line).unwrap());
                    let mut writer = stream;
                    writer.write_all(format!("{reply}\n").as_bytes()).unwrap();
                }
            });
        }
        Self { socket, seen, dir }
    }

    fn seen(&self) -> Vec<serde_json::Value> {
        self.seen.lock().unwrap().clone()
    }
}

impl Drop for FakeDaemon {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn login_reply() -> String {
    format!(r#"{{"result":"ui_login","port":8787,"code":"{CODE}"}}"#)
}

fn desktop() -> Env {
    Env::from_iter([("DISPLAY", ":0"), ("WAYLAND_DISPLAY", "wayland-0")])
}

fn over_ssh() -> Env {
    Env::from_iter([
        ("DISPLAY", "localhost:10.0"),
        ("SSH_CONNECTION", "10.0.0.2 51000 10.0.0.1 22"),
    ])
}

/// Run `galdeck ui`, with `opened` standing in for the browser: what it
/// returns is what the opener reports. Gives back what was printed, or the
/// error, and every address a browser was asked to open.
fn run(
    daemon: &Path,
    args: UiArgs,
    env: &Env,
    opened: Result<(), String>,
) -> (Result<String, String>, Vec<String>) {
    let mut asked = Vec::new();
    let mut out = Vec::new();
    let result = ui(
        args,
        env,
        daemon,
        &mut |url| {
            asked.push(url.to_string());
            opened.clone()
        },
        &mut out,
    );
    let printed = String::from_utf8(out).unwrap();
    (
        result.map(|()| printed).map_err(|e| format!("{e:#}")),
        asked,
    )
}

#[test]
fn on_a_desktop_it_opens_a_short_lived_link_and_prints_no_secret() {
    let reply = login_reply();
    let daemon = FakeDaemon::start("open", &[&reply]);
    let (printed, asked) = run(&daemon.socket, UiArgs::default(), &desktop(), Ok(()));
    let printed = printed.unwrap();

    assert_eq!(asked, [format!("http://127.0.0.1:8787/?code={CODE}")]);
    assert_eq!(
        printed.trim(),
        "Opening http://127.0.0.1:8787/ in your browser. If nothing appears, run `galdeck ui --print`."
    );
    assert!(!printed.contains(CODE), "the code is not for the terminal");
    assert_eq!(
        daemon.seen(),
        [serde_json::json!({"cmd": "ui_login", "ttl_s": 60})]
    );
}

#[test]
fn print_asks_for_a_five_minute_link_and_opens_nothing() {
    let reply = login_reply();
    let daemon = FakeDaemon::start("print", &[&reply]);
    let args = UiArgs {
        print: true,
        ..UiArgs::default()
    };
    let (printed, asked) = run(&daemon.socket, args, &desktop(), Ok(()));
    let printed = printed.unwrap();

    assert!(asked.is_empty());
    assert!(printed.contains(&format!("http://127.0.0.1:8787/?code={CODE}")));
    assert!(printed.contains("valid for 5 minutes, once"));
    assert!(!printed.contains("ssh -N"), "not an SSH session: {printed}");
    assert_eq!(
        daemon.seen(),
        [serde_json::json!({"cmd": "ui_login", "ttl_s": 300})]
    );
}

#[test]
fn over_ssh_it_prints_the_link_and_the_tunnel_that_keeps_the_port() {
    // xdg-open would find the desktop session over the user's bus, start a
    // browser with nowhere to draw, and report success.
    let reply = login_reply();
    let daemon = FakeDaemon::start("ssh", &[&reply]);
    let (printed, asked) = run(&daemon.socket, UiArgs::default(), &over_ssh(), Ok(()));
    let printed = printed.unwrap();

    assert!(asked.is_empty());
    assert!(printed.contains("ssh -N -L 8787:127.0.0.1:8787 <host>"));
    assert!(printed.contains("keeping the same port number on both sides"));
    assert!(printed.contains(&format!("http://127.0.0.1:8787/?code={CODE}")));
    assert!(printed.contains("valid for 5 minutes, once"));
}

#[test]
fn open_over_ssh_opens_all_the_same() {
    // For a session with X forwarding.
    let reply = login_reply();
    let daemon = FakeDaemon::start("ssh-open", &[&reply]);
    let args = UiArgs {
        open: true,
        ..UiArgs::default()
    };
    let (printed, asked) = run(&daemon.socket, args, &over_ssh(), Ok(()));
    assert!(printed.unwrap().starts_with("Opening"));
    assert_eq!(asked.len(), 1);
}

#[test]
fn a_browser_that_would_not_open_gets_a_printed_link_instead() {
    let reply = login_reply();
    let daemon = FakeDaemon::start("fallback", &[&reply, &reply]);
    let (printed, asked) = run(
        &daemon.socket,
        UiArgs::default(),
        &desktop(),
        Err("xdg-open failed (exit status: 3)".into()),
    );
    let printed = printed.unwrap();

    assert_eq!(asked.len(), 1);
    assert!(printed.contains("Could not open a browser: xdg-open failed (exit status: 3)."));
    assert!(printed.contains(&format!("http://127.0.0.1:8787/?code={CODE}")));
    // A fresh code with the time a printed one needs, not the minute the
    // browser had.
    assert_eq!(
        daemon.seen(),
        [
            serde_json::json!({"cmd": "ui_login", "ttl_s": 60}),
            serde_json::json!({"cmd": "ui_login", "ttl_s": 300}),
        ]
    );
}

#[test]
fn a_new_token_comes_first_and_then_the_link() {
    let reply = login_reply();
    let daemon = FakeDaemon::start("new-token", &[r#"{"result":"ok"}"#, &reply]);
    let args = UiArgs {
        new_token: true,
        ..UiArgs::default()
    };
    let (printed, asked) = run(&daemon.socket, args, &desktop(), Ok(()));
    let printed = printed.unwrap();

    assert!(printed.contains("signed out"), "{printed}");
    assert_eq!(asked.len(), 1);
    assert_eq!(
        daemon.seen(),
        [
            serde_json::json!({"cmd": "ui_rotate_token"}),
            serde_json::json!({"cmd": "ui_login", "ttl_s": 60}),
        ]
    );
}

#[test]
fn a_daemon_without_the_ui_is_told_how_to_turn_it_on() {
    let daemon = FakeDaemon::start(
        "off",
        &[
            r#"{"result":"error","message":"the UI is off: start the daemon with --http <port> to serve it"}"#,
        ],
    );
    let (result, asked) = run(&daemon.socket, UiArgs::default(), &desktop(), Ok(()));
    assert_eq!(result.unwrap_err(), UI_OFF);
    assert!(UI_OFF.contains("--http 8787"));
    assert!(UI_OFF.contains("systemctl --user edit --full galdeck"));
    assert!(asked.is_empty());
}

#[test]
fn a_daemon_from_before_sign_in_links_is_told_to_restart() {
    // What an older daemon says about a request it has no variant for.
    let daemon = FakeDaemon::start(
        "old",
        &[
            r#"{"result":"error","message":"bad request: unknown variant `ui_login`, expected one of `ping`, `status`"}"#,
        ],
    );
    let (result, _) = run(&daemon.socket, UiArgs::default(), &desktop(), Ok(()));
    assert_eq!(
        result.unwrap_err(),
        "This daemon predates `galdeck ui`: restart it to use this."
    );

    let daemon = FakeDaemon::start(
        "old-rotate",
        &[
            r#"{"result":"error","message":"bad request: unknown variant `ui_rotate_token`, expected one of `ping`"}"#,
        ],
    );
    let args = UiArgs {
        new_token: true,
        ..UiArgs::default()
    };
    let (result, _) = run(&daemon.socket, args, &desktop(), Ok(()));
    assert_eq!(result.unwrap_err(), UI_PREDATES);
}

#[test]
fn no_daemon_is_the_usual_connection_error() {
    let dir = scratch("none");
    let (result, asked) = run(
        &dir.join("galdeck.sock"),
        UiArgs::default(),
        &desktop(),
        Ok(()),
    );
    let error = result.unwrap_err();
    assert!(
        error.contains("is galdeck-daemon running? (systemctl --user start galdeck)"),
        "{error}"
    );
    assert!(asked.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn open_with_no_display_prints_and_says_why() {
    let reply = login_reply();
    let daemon = FakeDaemon::start("no-display", &[&reply]);
    let args = UiArgs {
        open: true,
        ..UiArgs::default()
    };
    let (printed, asked) = run(&daemon.socket, args, &Env::default(), Ok(()));
    let printed = printed.unwrap();
    assert!(asked.is_empty());
    assert!(printed.contains("There is no display"), "{printed}");
    assert!(printed.contains(&format!("?code={CODE}")));
}

#[test]
fn the_mode_follows_the_flags_and_the_environment() {
    let print = UiArgs {
        print: true,
        ..UiArgs::default()
    };
    let open = UiArgs {
        open: true,
        ..UiArgs::default()
    };
    let plain = UiArgs::default();
    let wayland = Env::from_iter([("WAYLAND_DISPLAY", "wayland-0")]);
    let x11 = Env::from_iter([("DISPLAY", ":1")]);
    let ssh_tty = Env::from_iter([("DISPLAY", ":0"), ("SSH_TTY", "/dev/pts/3")]);
    let empty_display = Env::from_iter([("DISPLAY", ""), ("WAYLAND_DISPLAY", "")]);

    for (args, env, expected, why) in [
        (&plain, &wayland, UiMode::Open, "wayland"),
        (&plain, &x11, UiMode::Open, "x11"),
        (&print, &wayland, UiMode::Print, "--print"),
        (&plain, &over_ssh(), UiMode::Print, "SSH_CONNECTION"),
        (&plain, &ssh_tty, UiMode::Print, "SSH_TTY"),
        (&open, &ssh_tty, UiMode::Open, "--open over ssh"),
        (&plain, &Env::default(), UiMode::Print, "no display"),
        (&open, &Env::default(), UiMode::Print, "--open, no display"),
        (&plain, &empty_display, UiMode::Print, "empty is unset"),
    ] {
        assert_eq!(ui_mode(args, env), expected, "{why}");
    }
}

#[test]
fn outside_a_snap_the_browser_gets_the_environment_as_it_is() {
    let env = Env::from_iter([("GTK_PATH", "/usr/lib/gtk"), ("DISPLAY", ":0")]);
    assert_eq!(browser_env(&env), EnvChanges::default());
}

#[test]
fn inside_a_snap_its_variables_are_taken_out_and_the_search_paths_put_back() {
    let env = Env::from_iter([
        ("SNAP", "/snap/code/247"),
        ("SNAP_NAME", "code"),
        ("SNAP_REVISION", "247"),
        (
            "GTK_PATH",
            "/snap/code/247/usr/lib/x86_64-linux-gnu/gtk-3.0",
        ),
        ("GTK_EXE_PREFIX", "/snap/code/247/usr"),
        (
            "GTK_IM_MODULE_FILE",
            "/home/u/snap/code/common/.cache/immodules.cache",
        ),
        (
            "GIO_MODULE_DIR",
            "/home/u/snap/code/common/.cache/gio-modules",
        ),
        (
            "GDK_PIXBUF_MODULE_FILE",
            "/home/u/snap/code/common/.cache/gdk-pixbuf.cache",
        ),
        ("GDK_PIXBUF_MODULEDIR", "/snap/code/247/usr/lib/gdk-pixbuf"),
        (
            "GSETTINGS_SCHEMA_DIR",
            "/snap/code/247/usr/share/glib-2.0/schemas",
        ),
        ("LOCPATH", "/snap/code/247/usr/lib/locale"),
        ("XDG_DATA_HOME", "/home/u/snap/code/247/.local/share"),
        (
            "XDG_DATA_DIRS",
            "/home/u/snap/code/247/.local/share:/snap/code/247/usr/share:/usr/share",
        ),
        (
            "XDG_DATA_DIRS_VSCODE_SNAP_ORIG",
            "/usr/share/ubuntu:/usr/local/share:/usr/share",
        ),
        ("XDG_CONFIG_DIRS", "/snap/code/247/etc/xdg"),
        ("XDG_CONFIG_DIRS_VSCODE_SNAP_ORIG", ""),
        ("DISPLAY", ":0"),
        ("HOME", "/home/u"),
        ("GDK_BACKEND", "x11"),
    ]);
    let changes = browser_env(&env);
    let removed: Vec<&str> = changes.remove.iter().map(|n| n.to_str().unwrap()).collect();

    for name in [
        "SNAP",
        "SNAP_NAME",
        "SNAP_REVISION",
        "GTK_PATH",
        "GTK_EXE_PREFIX",
        "GTK_IM_MODULE_FILE",
        "GIO_MODULE_DIR",
        "GDK_PIXBUF_MODULE_FILE",
        "GDK_PIXBUF_MODULEDIR",
        "GSETTINGS_SCHEMA_DIR",
        "LOCPATH",
        "XDG_DATA_HOME",
        // Unset before the snap set it: an empty search path is not the
        // default one.
        "XDG_CONFIG_DIRS",
    ] {
        assert!(
            removed.contains(&name),
            "{name} should be removed: {removed:?}"
        );
    }
    for kept in ["DISPLAY", "HOME", "GDK_BACKEND", "XDG_DATA_DIRS"] {
        assert!(!removed.contains(&kept), "{kept} should be left alone");
    }
    assert_eq!(
        changes.set,
        [(
            OsString::from("XDG_DATA_DIRS"),
            OsString::from("/usr/share/ubuntu:/usr/local/share:/usr/share")
        )]
    );
}

#[test]
fn a_missing_opener_falls_through_to_the_next() {
    let dir = scratch("fallthrough");
    let marker = dir.join("opened");
    let marker_arg = marker.to_str().unwrap();
    let result = spawn_opener(
        &[
            &["/nonexistent/galdeck-test-opener"],
            // The address arrives as $1; $0 is where to write it.
            &["sh", "-c", "printf %s \"$1\" > \"$0\"", marker_arg],
        ],
        "http://127.0.0.1:8787/?code=x",
        &EnvChanges::default(),
        Duration::from_secs(5),
    );
    assert_eq!(result, Ok(()));
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap(),
        "http://127.0.0.1:8787/?code=x"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_opener_that_fails_is_reported_and_none_at_all_is_too() {
    let failed = spawn_opener(
        &[&["sh", "-c", "exit 3"]],
        "http://127.0.0.1:1/",
        &EnvChanges::default(),
        Duration::from_secs(5),
    );
    assert!(failed.unwrap_err().contains("exit status: 3"));

    let missing = spawn_opener(
        &[&["/nonexistent/one"], &["/nonexistent/two", "open"]],
        "http://127.0.0.1:1/",
        &EnvChanges::default(),
        Duration::from_secs(5),
    );
    assert_eq!(
        missing.unwrap_err(),
        "/nonexistent/one or /nonexistent/two is not installed"
    );
}

#[test]
fn an_opener_still_running_after_the_wait_is_left_to_it() {
    // xdg-open's generic path runs the browser in the foreground.
    let started = std::time::Instant::now();
    let result = spawn_opener(
        &[&["sh", "-c", "sleep 2"]],
        "http://127.0.0.1:1/",
        &EnvChanges::default(),
        Duration::from_millis(200),
    );
    assert_eq!(result, Ok(()));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn the_opener_gets_the_changed_environment_and_a_process_group_of_its_own() {
    let dir = scratch("env");
    let marker = dir.join("seen");
    // Changes to the child's environment only; the test's own is untouched.
    let changes = EnvChanges {
        remove: vec![OsString::from("HOME")],
        set: vec![(
            OsString::from("GALDECK_TEST_RESTORED"),
            OsString::from("/usr/share"),
        )],
    };
    // The fifth field of /proc/PID/stat is the process group.
    let script = "read -r _ _ _ _ group _ < /proc/$$/stat; \
                  if [ \"$group\" = $$ ]; then own=own; else own=shared; fi; \
                  printf '%s|%s|%s' \"${HOME-unset}\" \"$GALDECK_TEST_RESTORED\" \"$own\" > \"$0\"";
    let result = spawn_opener(
        &[&["sh", "-c", script, marker.to_str().unwrap()]],
        "http://127.0.0.1:1/",
        &changes,
        Duration::from_secs(5),
    );
    assert_eq!(result, Ok(()));
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap(),
        "unset|/usr/share|own"
    );
    let _ = std::fs::remove_dir_all(dir);
}
