# galdeck-cli

The command-line control surface for a **Corsair Galleon 100 SD** keyboard's
built-in Stream Deck, and the two crates the rest of the stack is built on:
the control protocol, and the configuration model.

The daemon that actually drives the panel — and the configuration UI it
serves — lives in
[galdeck-daemon](https://github.com/cynak/galdeck-daemon) and depends on this
repository. The dependency runs one way only. Nothing here knows the daemon or
the UI exists, which is what lets `galdeck` be installed, used and reasoned
about on its own.

## Install

```sh
# the build needs libudev headers: apt install libudev-dev
cargo install --path crates/galdeck-cli
```

Most commands talk to a running daemon, so you will want that too; see the
daemon's README for the service and the udev rule. `galdeck detect` is the
exception — it opens the module over HID itself and works with nothing else
installed.

## Commands

| Command | What it does |
|---|---|
| `galdeck ping` | Check the daemon is alive: it prints `pong` |
| `galdeck status` | Daemon and device state: connected or not, firmware, serial, the profiles and the one showing, the pages and the one showing, brightness |
| `galdeck page <name>` | Switch to a page of the current profile |
| `galdeck profile <name>` | Switch profile |
| `galdeck brightness <0-100> [--device <which>]` | Set the panel's brightness. `--device` is `all` (the default), `lcd-panel`, `left-encoder` or `right-encoder`; the module has one brightness control for the whole panel, so `all` and `lcd-panel` both set it, and the daemon refuses the two encoders rather than dim everything and say it did not |
| `galdeck reload` | Re-read the config and apply it again; a broken edit leaves the running one alone |
| `galdeck ui [--print \| --open] [--new-token]` | Open the configuration UI in your browser, signed in with a one-time link. `--print` prints the link instead (valid for 5 minutes, once); `--open` opens a browser even over SSH; `--new-token` gives the UI a new token first, which signs every open tab out. See [below](#galdeck-ui) |
| `galdeck probe [--size <N> \| --zones]` | Draw a calibration pattern on every key at `N` pixels (default 160) to check the panel's real size; `--zones` fills every calibrated zone instead. `galdeck reload` puts the page back |
| `galdeck detect` | Find the module over HID and read its firmware and serial (works without the daemon, changes nothing) |
| `galdeck calibrate [--print \| --json \| --show]` | Measure where the keys sit behind the bezel and save it; a running daemon hands the device over for the run and takes it back after. `--print` prints the saved layout as a matrix and `--json` as JSON, neither touching the device; `--show` draws it on the panel until a knob is pressed |
| `galdeck help [<command>]` | Help for every command, or for one; `galdeck <command> --help` too |

Every command also takes:

| Option | What it does |
|---|---|
| `--socket <path>` | The daemon's control socket. Without it, `$GALDECK_SOCKET`, else `$XDG_RUNTIME_DIR/galdeck.sock`, else `/tmp/galdeck-<uid>/galdeck.sock`. This is how to reach a daemon other than the installed one, such as one developed against `--device virtual` |
| `-h`, `--help` | Help; `--help` says more than `-h` |
| `-V`, `--version` | The version, on `galdeck` itself |

### `galdeck ui`

It asks the daemon, over the control socket, for a one-time code and opens
`http://127.0.0.1:<port>/?code=…` with xdg-open, or `gio open` where there is
no xdg-open. The code works once, for a minute, and the page trades it for the
UI's token, so the token is never on a command line or in browser history, and
what is there is worth nothing afterwards. The browser is started with no
terminal and in a process group of its own; from inside a snap, such as VS
Code's terminal, it also gets the environment without the snap's GTK and GIO
settings, with `XDG_DATA_DIRS` and `XDG_CONFIG_DIRS` put back as they were.
If the opener fails within three seconds, or neither is installed, a link is
printed instead.

It prints the link rather than opening a browser:

- when asked to, with `--print`;
- over SSH (`SSH_CONNECTION` or `SSH_TTY` set), unless `--open` says to open
  one anyway, for a session with X forwarding;
- with no display, when neither `DISPLAY` nor `WAYLAND_DISPLAY` is set, even
  with `--open`, and it says why.

A printed link is good for five minutes, once. Over SSH it also prints the
tunnel to run on the machine your browser is on, and the link to open there:

```sh
ssh -N -L 8787:127.0.0.1:8787 <host>
```

Keep the same port number on both sides: the daemon answers only to its own
address.

`--new-token` replaces the UI's token, on disk and in the running daemon, and
forgets every link not yet used, so every tab that had the UI open has to be
signed in again. The page suggests it when the link it was opened with had
already been used: if that wasn't you, someone else used it.

The daemon needs `--http <port>` for any of this, and `galdeck ui` says how to
add it, to the systemd unit too, when it is missing. A daemon older than this
CLI cannot answer it; restart it after upgrading. The CLI never reads the
token file itself.

## The crates

| Crate | What it is |
|---|---|
| [`galdeck-cli`](crates/galdeck-cli) | The `galdeck` binary |
| [`galdeck-ipc`](crates/galdeck-ipc) | The control protocol: line-delimited JSON over a Unix socket, one request and one response per line. Simple enough that `socat` is a usable client, which matters for something people script against. Every field added since the first release has a default, so an older daemon and a newer CLI still understand each other |
| [`galdeck-model`](crates/galdeck-model) | The configuration model: the schema (profiles, pages and themes, with their widget looks, motion and keyboard lighting; keys and their gestures, and keys that cycle through states; icons by file or by icon-theme name; actions, keystrokes and knob presets, layered from `galdeck.toml` down to a page, with dial modes; widgets, timers and thresholds), validation diagnostics with stable codes, and write-back that preserves comments and formatting |

`galdeck-ipc` and `galdeck-model` are not on crates.io yet. The daemon builds
them by path from a checkout of this repository beside its own, and its CI
checks out this repository's `main`, so what lands on `main` has to build the
daemon too. `galdeck-model` is deliberately pure — no
clock, no subprocesses, no sockets — so its tests can name every instant
rather than wait for one.

## Development

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./ci/layering.sh
```

`ci/layering.sh` checks the boundaries that this split exists to create —
above all that nothing here has acquired a dependency on the daemon or the UI.
It also checks that no crate reaches outside its own directory for a file,
which compiles here and fails for anyone who unpacks the published tarball.

The [`galdeck`](https://crates.io/crates/galdeck) framework comes from
crates.io. To work against an unpublished change to it, add a patch in
`.cargo/config.toml` (which is not committed):

```toml
[patch.crates-io]
galdeck = { path = "../galdeck" }
```

Working on this and the daemon together means checking both out side by side,
because the daemon finds these crates by relative path:

```
~/Code/galdeck-cli
~/Code/galdeck-daemon
```

## License

MIT.
