use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use anyhow::{bail, Context, Result};
use clap::Parser;
// The framework directly: `detect` and `calibrate` both bypass the daemon and
// open the module themselves.
use galdeck::{calibrate, Font, Galleon, Layout};
use galdeck_cli::{Args, Command};
use galdeck_ipc::{Request, Response};

fn request(request: &Request) -> Result<Response> {
    let path = galdeck_ipc::socket_path();
    let stream = UnixStream::connect(&path).with_context(|| {
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

fn expect_ok(response: Response) -> Result<()> {
    match response {
        Response::Ok => Ok(()),
        Response::Error { message } => bail!("{message}"),
        Response::Diagnostics { diagnostics } => {
            if diagnostics.is_empty() {
                return Ok(());
            }
            bail!("{}", render_diagnostics(&diagnostics));
        }
        other => bail!("unexpected response: {other:?}"),
    }
}

fn render_diagnostics(diagnostics: &[galdeck_ipc::Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|d| {
            let where_ = match d.start {
                Some(loc) => format!("{}:{}", loc.line, loc.col),
                None => d.path.clone(),
            };
            let help = d
                .help
                .as_ref()
                .map(|h| format!(" ({h})"))
                .unwrap_or_default();
            format!("{where_} [{}] {}{help}", d.code, d.message)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn detect() -> Result<()> {
    let api = galdeck::hidapi::HidApi::new()?;
    let paths = galdeck::Galleon::list(&api);
    if paths.is_empty() {
        println!("no Galleon 100 SD stream deck module found (usb 1b1c:2b18)");
        println!("- is the keyboard plugged in?");
        println!("- is the udev rule installed? (see udev/70-galdeck.rules)");
        return Ok(());
    }
    for path in paths {
        println!("module at {path}");
        // Passive open: no keepalive is sent, so the module is left in
        // whatever mode it is in — detect really is read-only.
        match galdeck::Galleon::open_passive(&api, &path) {
            Ok(mut deck) => {
                let firmware = deck.firmware_version()?;
                println!("  firmware: {firmware}");
                println!("  serial:   {}", deck.serial_number()?);
                let validated = galdeck::ids::VALIDATED_FIRMWARES;
                if !validated.contains(&firmware.as_str()) {
                    println!("  note: protocol is only validated on firmware {validated:?}");
                }
            }
            Err(e) => println!("  open failed: {e}"),
        }
    }
    Ok(())
}

/// Run the calibration wizard, or read a calibration back.
///
/// The wizard needs the only open handle on the hidraw node -- two handles is
/// the leading suspect for the module dropping off the USB bus -- so the
/// device is borrowed from the daemon for the duration and handed straight
/// back, on the error path as much as the happy one.
fn calibration_ui(print: bool, json: bool, show: bool) -> Result<()> {
    let path = Layout::default_path();
    // Refining a saved calibration beats starting over. The template is the
    // fallback rather than the default because its values are arithmetic,
    // not measured on any unit.
    let starting = match Layout::load(&path) {
        Ok(layout) => {
            eprintln!("==> loaded {}", path.display());
            layout
        }
        Err(error) => {
            eprintln!("==> no usable saved layout ({error}); starting from the template");
            eprintln!("    NOTE: template values are arithmetic, not measured");
            Layout::TEMPLATE
        }
    };

    // Both of these read a calibration without touching the hardware, so they
    // answer before the daemon is asked to give anything up. Progress has
    // gone to stderr throughout, which leaves stdout clean enough to pipe.
    if print {
        print!("{}", starting.to_ascii());
        return Ok(());
    }
    if json {
        print!("{}", starting.to_json());
        return Ok(());
    }

    // Three outcomes, and only two of them make it safe to open the device.
    // Telling the middle one apart from the last is the whole point: a daemon
    // built before the handover existed cannot parse this request and answers
    // `{"result":"error","message":"bad request: ..."}` -- a perfectly
    // successful round trip carrying a refusal. Reading that as "no daemon"
    // and opening anyway puts a second handle on the hidraw node while the
    // daemon still holds the first, which is the one thing this must never
    // do, and is exactly how the module gets knocked off the USB bus.
    let borrowed = match request(&Request::ReleaseDevice) {
        // It let go, and confirmed the handle is closed before answering.
        Ok(Response::Ok) => true,
        // Something is listening on the socket and would not hand the device
        // over. Refuse rather than race it.
        Ok(Response::Error { message }) => bail!(
            "the daemon would not hand the device over: {message}\n\
             \n\
             If that reads like a parse failure, the running daemon predates \
             the handover and cannot be asked to let go. Update and restart \
             it (cargo install --path crates/galdeck-daemon, then systemctl \
             --user restart galdeck), or stop it for the duration:\n\
             \n\
             \x20   systemctl --user stop galdeck && galdeck calibrate; \
             systemctl --user start galdeck"
        ),
        Ok(other) => bail!("unexpected reply to a device release: {other:?}"),
        // Nothing answered, so nothing is holding the device. This is how
        // `galdeck detect` already behaves.
        Err(_) => {
            eprintln!("==> no daemon holding the device; opening it directly");
            false
        }
    };

    // Everything that touches the device runs inside the closure, so there is
    // exactly one place that can return early while it is still borrowed:
    // none.
    let outcome = (|| -> Result<Option<Layout>> {
        let api = galdeck::hidapi::HidApi::new()?;
        let mut deck = Galleon::open(&api)?;
        let font = Font::system();

        if show {
            eprintln!("==> holding — press either knob to end");
            calibrate::show(&mut deck, &starting, font.as_ref())?;
            return Ok(None);
        }

        eprintln!("{}", calibrate::INSTRUCTIONS);
        Ok(Some(calibrate::run(&mut deck, starting, font.as_ref())?))
    })();

    // Written before the device goes back, because ResumeDevice makes the
    // daemon re-read the file: saving first is what makes a fresh calibration
    // take effect without a second round trip.
    let saved = match &outcome {
        Ok(Some(layout)) => Some(layout.save(&path)),
        _ => None,
    };

    // Handed back on every path, including the error one. A failure here is
    // not worth losing a completed calibration over, but it does leave the
    // deck dark, so it is said out loud rather than swallowed.
    if borrowed {
        if let Err(error) = request(&Request::ResumeDevice).and_then(expect_ok) {
            eprintln!("==> warning: the daemon did not take the device back: {error}");
        }
    }

    // In causal order: a wizard that failed is why nothing was saved.
    outcome?;
    if let Some(result) = saved {
        result.with_context(|| format!("saving {}", path.display()))?;
        println!("saved {}", path.display());
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    // Set before anything connects, and before any thread exists, so that
    // `socket_path()` picks it up wherever it is called from. An explicit flag
    // beats the environment, which is why this overwrites rather than checks.
    if let Some(path) = &args.socket {
        std::env::set_var("GALDECK_SOCKET", path);
    }
    match args.command {
        Command::Ping => {
            expect_ok(request(&Request::Ping)?)?;
            println!("pong");
        }
        Command::Status => match request(&Request::Status)? {
            Response::Status(status) => {
                println!("connected:  {}", status.connected);
                if let Some(firmware) = status.firmware {
                    println!("firmware:   {firmware}");
                }
                if let Some(serial) = status.serial {
                    println!("serial:     {serial}");
                }
                // Absent when talking to a daemon older than profiles.
                if !status.profile.is_empty() {
                    println!("profile:    {}", status.profile);
                }
                if !status.profiles.is_empty() {
                    println!("profiles:   {}", status.profiles.join(", "));
                }
                println!("page:       {}", status.page);
                println!("pages:      {}", status.pages.join(", "));
                println!("brightness: {}", status.brightness);
            }
            Response::Error { message } => bail!("{message}"),
            other => bail!("unexpected response: {other:?}"),
        },
        Command::Brightness { percent, device } => {
            expect_ok(request(&Request::SetBrightness {
                percent,
                device: device.into(),
            })?)?;
        }
        Command::Page { name } => {
            expect_ok(request(&Request::SwitchPage { name })?)?;
        }
        Command::Profile { name } => {
            expect_ok(request(&Request::SwitchProfile { name })?)?;
        }
        Command::Reload => {
            expect_ok(request(&Request::Reload)?)?;
        }
        Command::Ui => ui()?,
        Command::Probe { size, zones } => {
            if zones {
                expect_ok(request(&Request::ZonePattern)?)?;
                println!("filled every calibrated zone through the region path.");
                println!();
                println!("  all rows green-bordered    -> the region path reaches the whole panel");
                println!(
                    "  only the top row draws     -> region writes stop below the info screen"
                );
                println!("  borders flush with the cap -> the calibration is right");
                println!(
                    "  drawn, then gone a moment  -> the firmware repaints the key area itself"
                );
                println!();
                println!("`galdeck reload` puts your page back.");
            } else {
                expect_ok(request(&Request::TestPattern { size })?)?;
                println!("drew a {size}x{size} pattern on every key.");
                println!();
                println!("  border flush with the key edge  -> {size} is right");
                println!("  something visible outside it    -> too small for the panel");
                println!("  border clipped or a corner gone -> too large, or cropped");
                println!();
                println!("`galdeck reload` puts your page back.");
            }
        }
        Command::Detect => detect()?,
        Command::Calibrate { print, json, show } => calibration_ui(print, json, show)?,
    }
    Ok(())
}

/// Print the address of the configuration UI.
///
/// The token lives beside the control socket, readable only by its owner, so
/// this needs no help from the daemon -- which also means it still works when
/// the browser tab holding the old one has gone stale.
fn ui() -> Result<()> {
    let socket = galdeck_ipc::socket_path();
    let token_file = socket
        .parent()
        .unwrap_or_else(|| std::path::Path::new("/tmp"))
        .join("galdeck-ui-token");
    let token = std::fs::read_to_string(&token_file)
        .map(|t| t.trim().to_string())
        .with_context(|| {
            format!(
                "reading {} — start the daemon with --http <port> to serve the UI",
                token_file.display()
            )
        })?;
    println!("http://127.0.0.1:<port>/?token={token}");
    println!();
    println!("The port is the one passed to --http; the daemon prints the whole");
    println!("address at startup.");
    Ok(())
}
