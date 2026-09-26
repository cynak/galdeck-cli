/// Keep the main.rs clean of clutter but all constants and
/// publicly used strcutures in here
///
///
///
/// Control the galdeck daemon (Corsair Galleon 100 SD Stream Deck module).
use clap::{Parser, Subcommand};

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use galdeck_ipc::{Request, Response};

#[derive(Parser)]
#[command(version, about)]
pub struct Args {
    /// Control socket path (default: $GALDECK_SOCKET, else
    /// $XDG_RUNTIME_DIR/galdeck.sock).
    ///
    /// Point this at a development daemon running alongside the installed one.
    #[arg(long, global = true, value_name = "PATH")]
    pub socket: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}
/// The `--device` values, mapped to the protocol's own enum.
///
/// A separate enum on purpose. galdeck-ipc is published and the daemon
/// depends on it; a clap derive there would make every consumer of the
/// protocol carry a command-line parser it has no use for.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum DeviceArg {
    LeftEncoder,
    RightEncoder,
    LcdPanel,
    All,
}

#[derive(Subcommand)]
pub enum Command {
    /// Check that the daemon is running.
    Ping,
    /// Show daemon and device state.
    Status,
    /// Set panel brightness (0-100).
    Brightness {
        percent: u8,
        /// Which part of the deck to dim.
        #[arg(long, value_enum, default_value_t = DeviceArg::All)]
        device: DeviceArg,
    },
    /// Switch to a named page in the current profile.
    Page { name: String },
    /// Switch to a named profile.
    Profile { name: String },
    /// Reload the config file and re-apply the current page.
    Reload,
    /// Open the configuration UI in your browser, signed in.
    ///
    /// Asks the daemon for a one-time link and opens it. Over SSH, or with no
    /// display to open a browser on, prints the link instead.
    Ui {
        /// Print the link rather than opening a browser.
        #[arg(long, conflicts_with = "open")]
        print: bool,
        /// Open a browser even over SSH, where the link is printed by default.
        #[arg(long)]
        open: bool,
        /// Give the UI a new token first, which signs every open tab out.
        ///
        /// For when a link was used by someone who was not you.
        #[arg(long)]
        new_token: bool,
    },
    /// Draw a calibration pattern on every key to check the panel's real size.
    ///
    /// A yellow border is drawn at the very edge of the image. If it sits
    /// flush with the physical key, the size is right; if anything shows
    /// outside it, the image is too small for the panel. `galdeck reload`
    /// puts the page back.
    Probe {
        /// Pixel size to draw at. The protocol's declared size is 160.
        #[arg(long, default_value_t = 160)]
        size: u32,
        /// Fill every calibrated zone through the panel region path instead.
        ///
        /// The other half of the question: `--size` asks how much of a key
        /// the firmware's key path covers, this asks whether the region path
        /// can cover the rest, at the rectangles calibration measured.
        #[arg(long, conflicts_with = "size")]
        zones: bool,
    },
    /// Enumerate the module over HID directly (works without the daemon).
    Detect,
    /// Measure where the keys sit behind the bezel, and save the result.
    ///
    /// Nothing in the protocol says where the key area starts or how big a
    /// key is, and it varies with how the panel sits behind the bezel, so it
    /// is measured per unit. The daemon hands the device over for the run and
    /// takes it back afterwards.
    Calibrate {
        /// Print the saved layout as a matrix and exit. Needs no device.
        #[arg(long)]
        print: bool,
        /// Print the saved layout as JSON and exit. Needs no device.
        #[arg(long, conflicts_with = "print")]
        json: bool,
        /// Draw the saved layout on the panel and hold until a knob is
        /// pressed, instead of running the wizard.
        #[arg(long, conflicts_with_all = ["print", "json"])]
        show: bool,
    },
}

impl From<DeviceArg> for galdeck_ipc::DeckDevice {
    fn from(arg: DeviceArg) -> Self {
        match arg {
            DeviceArg::LeftEncoder => galdeck_ipc::DeckDevice::LeftEncoder,
            DeviceArg::RightEncoder => galdeck_ipc::DeckDevice::RightEncoder,
            DeviceArg::LcdPanel => galdeck_ipc::DeckDevice::LcdPanel,
            DeviceArg::All => galdeck_ipc::DeckDevice::All,
        }
    }
}

/// Send one request to the daemon and read its reply.
pub fn request(request: &Request) -> Result<Response> {
    request_at(&galdeck_ipc::socket_path(), request)
}

/// [`request`], to the daemon listening at `path`.
pub fn request_at(path: &Path, request: &Request) -> Result<Response> {
    let stream = UnixStream::connect(path).with_context(|| {
        format!(
            "connecting to {} — is galdeck-daemon running? (systemctl --user start galdeck)",
            path.display()
        )
    })?;
    let mut writer = stream.try_clone()?;
    let mut payload = serde_json::to_string(request)?;
    payload.push('\n');
    writer.write_all(payload.as_bytes())?;

    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    if line.trim().is_empty() {
        bail!("daemon closed the connection without responding");
    }
    Ok(serde_json::from_str(&line)?)
}

/// What `galdeck ui` was asked for.
#[derive(Debug, Clone, Copy, Default)]
pub struct UiArgs {
    pub print: bool,
    pub open: bool,
    pub new_token: bool,
}

/// How long a link lives when a browser is opened on it at once.
const OPEN_TTL_S: u32 = 60;
/// How long a printed link lives: long enough to set up a tunnel and paste it.
const PRINT_TTL_S: u32 = 300;
/// How long to wait for xdg-open to say whether it worked. On GNOME it hands
/// the address over and returns at once; elsewhere it can run the browser in
/// the foreground, and then it is left running.
pub const OPEN_WAIT: Duration = Duration::from_secs(3);

/// What the daemon's refusal starts with when it was started without
/// `--http`.
const UI_OFF_REPLY: &str = "the UI is off";
/// What `galdeck ui` says to that.
pub const UI_OFF: &str = "The daemon is running without its configuration UI. Start it with \
     --http 8787 (for the systemd unit: `systemctl --user edit --full galdeck`, add it to \
     ExecStart, then restart).";
/// What it says to a daemon that cannot parse the request at all.
pub const UI_PREDATES: &str = "This daemon predates `galdeck ui`: restart it to use this.";

/// The environment, read once, so the decisions below can be tested with an
/// environment of the test's own.
#[derive(Debug, Clone, Default)]
pub struct Env(BTreeMap<OsString, OsString>);

impl Env {
    pub fn current() -> Self {
        Self(std::env::vars_os().collect())
    }

    fn get(&self, name: &str) -> Option<&OsStr> {
        self.0.get(OsStr::new(name)).map(OsString::as_os_str)
    }

    /// Set to something: an empty `DISPLAY` is no display.
    fn is_set(&self, name: &str) -> bool {
        self.get(name).is_some_and(|value| !value.is_empty())
    }
}

impl<K: Into<OsString>, V: Into<OsString>> FromIterator<(K, V)> for Env {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(pairs: I) -> Self {
        Self(
            pairs
                .into_iter()
                .map(|(name, value)| (name.into(), value.into()))
                .collect(),
        )
    }
}

/// Whether `galdeck ui` opens a browser or prints a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    Open,
    Print,
}

/// Open, unless asked to print, or over SSH, or with no display.
///
/// Over SSH, xdg-open finds the desktop session through the user's bus,
/// launches a browser with no display to draw on and reports success; with
/// no display at all it may run a text browser on this very terminal. So
/// both are decided here rather than left to it. `--open` overrides SSH,
/// for a session with X forwarding, but not the lack of a display.
pub fn ui_mode(args: &UiArgs, env: &Env) -> UiMode {
    let display = env.is_set("DISPLAY") || env.is_set("WAYLAND_DISPLAY");
    if args.print || (over_ssh(env) && !args.open) || !display {
        UiMode::Print
    } else {
        UiMode::Open
    }
}

fn over_ssh(env: &Env) -> bool {
    env.is_set("SSH_CONNECTION") || env.is_set("SSH_TTY")
}

/// What to change in the environment a browser is started with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvChanges {
    pub remove: Vec<OsString>,
    pub set: Vec<(OsString, OsString)>,
}

/// Variables a snap (VS Code's terminal, typically) sets for its own GTK and
/// GLib, which a browser started from inside it would pick up and load the
/// snap's modules with.
const SNAP_LEAKS: &[&str] = &[
    "GTK_PATH",
    "GTK_EXE_PREFIX",
    "GTK_IM_MODULE_FILE",
    "GIO_MODULE_DIR",
    "GDK_PIXBUF_MODULE_FILE",
    "GDK_PIXBUF_MODULEDIR",
    "GSETTINGS_SCHEMA_DIR",
    "LOCPATH",
    "XDG_DATA_HOME",
];

/// The environment a browser should get: this one, minus what a snap put in
/// it, with the search paths it rewrote put back as they were.
pub fn browser_env(env: &Env) -> EnvChanges {
    let mut changes = EnvChanges::default();
    if env.get("SNAP").is_none() {
        return changes;
    }
    for name in env.0.keys() {
        let leaked = name
            .to_str()
            .is_some_and(|name| name.starts_with("SNAP") || SNAP_LEAKS.contains(&name));
        if leaked {
            changes.remove.push(name.clone());
        }
    }
    for name in ["XDG_DATA_DIRS", "XDG_CONFIG_DIRS"] {
        // VS Code's snap keeps what it replaced. Empty means it was unset,
        // and an empty search path is not the same as the default one.
        match env.get(&format!("{name}_VSCODE_SNAP_ORIG")) {
            Some(original) if original.is_empty() => changes.remove.push(name.into()),
            Some(original) => changes.set.push((name.into(), original.to_owned())),
            None => {}
        }
    }
    changes
}

/// Open `url` in the user's browser: with xdg-open, or with `gio open` where
/// there is no xdg-open.
pub fn open_in_browser(url: &str, env: &Env) -> Result<(), String> {
    spawn_opener(
        &[&["xdg-open"], &["gio", "open"]],
        url,
        &browser_env(env),
        OPEN_WAIT,
    )
}

/// Run the first of `openers` that is installed, with `url` as its last
/// argument, and give it `wait` to fail.
///
/// Exiting 0, or still running at the end of the wait, counts as opened:
/// the second is xdg-open running a browser in the foreground, which is left
/// to it. It gets no terminal -- nothing to read, nowhere to print -- and a
/// process group of its own, so a Ctrl-C meant for this one never reaches a
/// browser it had to start.
pub fn spawn_opener(
    openers: &[&[&str]],
    url: &str,
    changes: &EnvChanges,
    wait: Duration,
) -> Result<(), String> {
    for opener in openers {
        let Some((program, arguments)) = opener.split_first() else {
            continue;
        };
        let mut command = std::process::Command::new(program);
        command
            .args(arguments)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        for name in &changes.remove {
            command.env_remove(name);
        }
        for (name, value) in &changes.set {
            command.env(name, value);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{program}: {e}")),
        };
        let deadline = Instant::now() + wait;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => return Err(format!("{program} failed ({status})")),
                Ok(None) if Instant::now() >= deadline => return Ok(()),
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => return Err(format!("{program}: {e}")),
            }
        }
    }
    let names: Vec<&str> = openers.iter().filter_map(|o| o.first().copied()).collect();
    Err(format!("{} is not installed", names.join(" or ")))
}

/// `galdeck ui`: open the configuration UI signed in, or print a link that
/// signs in.
///
/// The link carries a one-time code from the daemon, never the token: it
/// passes through xdg-open's and the browser's command lines, which every
/// account on the machine can read, and into the browser's history. `open`
/// starts a browser on a link; everything said goes to `out`.
pub fn ui(
    args: UiArgs,
    env: &Env,
    socket: &Path,
    open: &mut dyn FnMut(&str) -> Result<(), String>,
    out: &mut dyn Write,
) -> Result<()> {
    if args.new_token {
        match ui_request(socket, &Request::UiRotateToken)? {
            Response::Ok => writeln!(
                out,
                "The UI has a new token: every tab that had it open is signed out."
            )?,
            other => bail!("unexpected reply to a new token: {other:?}"),
        }
    }

    let mode = ui_mode(&args, env);
    if mode == UiMode::Open {
        let (port, code) = ui_login(socket, OPEN_TTL_S)?;
        match open(&link(port, &code)) {
            Ok(()) => {
                writeln!(
                    out,
                    "Opening http://127.0.0.1:{port}/ in your browser. If nothing appears, \
                     run `galdeck ui --print`."
                )?;
                return Ok(());
            }
            // That code dies unused in a minute; the one printed below gets
            // the time a printed link needs.
            Err(why) => writeln!(out, "Could not open a browser: {why}.\n")?,
        }
    } else if args.open {
        writeln!(
            out,
            "There is no display to open a browser on (DISPLAY and WAYLAND_DISPLAY are unset).\n"
        )?;
    }

    let (port, code) = ui_login(socket, PRINT_TTL_S)?;
    if over_ssh(env) {
        writeln!(
            out,
            "This is an SSH session. On the machine your browser is on, forward the port\n\
             first, keeping the same port number on both sides:\n\n  \
             ssh -N -L {port}:127.0.0.1:{port} <host>\n\n\
             Then open this there (valid for 5 minutes, once):"
        )?;
    } else {
        writeln!(
            out,
            "Open this in a browser on this machine (valid for 5 minutes, once):"
        )?;
    }
    writeln!(out, "\n  {}", link(port, &code))?;
    Ok(())
}

/// Always the literal address: `localhost` can resolve to ::1 first, where
/// nothing listens.
fn link(port: u16, code: &str) -> String {
    format!("http://127.0.0.1:{port}/?code={code}")
}

/// A one-time code for a link that lives `ttl_s` seconds.
fn ui_login(socket: &Path, ttl_s: u32) -> Result<(u16, String)> {
    match ui_request(socket, &Request::UiLogin { ttl_s: Some(ttl_s) })? {
        Response::UiLogin { port, code } => Ok((port, code)),
        other => bail!("unexpected reply to a sign-in: {other:?}"),
    }
}

/// A request only a daemon serving the UI answers, with its two refusals
/// said as what to do about them.
fn ui_request(socket: &Path, request: &Request) -> Result<Response> {
    match request_at(socket, request)? {
        Response::Error { message } if message.starts_with(UI_OFF_REPLY) => bail!(UI_OFF),
        // What a daemon from before these requests says about one it cannot
        // parse.
        Response::Error { message } if message.contains("unknown variant") => bail!(UI_PREDATES),
        Response::Error { message } => bail!("{message}"),
        other => Ok(other),
    }
}

#[cfg(test)]
mod ui_tests;
