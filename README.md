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
| `galdeck detect` | Find the module and read its firmware (works without the daemon, changes nothing) |
| `galdeck status` | Daemon and device state |
| `galdeck page <name>` | Switch page within the current profile |
| `galdeck profile <name>` | Switch profile |
| `galdeck brightness <0-100> [--device …]` | Set panel brightness, for the whole deck or just `left-encoder`, `right-encoder` or `lcd-panel` |
| `galdeck reload` | Re-read the config; a broken edit leaves the running one alone |
| `galdeck ping` | Check the daemon is alive |
| `galdeck ui` | Print the configuration UI's address, token included |
| `galdeck probe [--size N \| --zones]` | Draw a calibration pattern on every key to check the panel's real size, or fill every calibrated zone |
| `galdeck calibrate [--print \| --json \| --show]` | Measure where the keys sit behind the bezel and save it (the daemon hands the device over for the run); or print, or draw, the saved layout |

`--socket <path>`, or `$GALDECK_SOCKET`, points any of them at a daemon other
than the installed one — which is what developing against `--device virtual`
wants.

`galdeck ui` reads the token from a file beside the control socket rather than
asking the daemon for it. That is deliberate: it means the address can still be
recovered when the browser tab holding the old one has gone stale, and it is
the only place in this repository that knows the UI exists at all. It knows a
file path, not a crate.

## The crates

| Crate | What it is |
|---|---|
| [`galdeck-cli`](crates/galdeck-cli) | The `galdeck` binary |
| [`galdeck-ipc`](crates/galdeck-ipc) | The control protocol: line-delimited JSON over a Unix socket, one request and one response per line. Simple enough that `socat` is a usable client, which matters for something people script against. Every field added since the first release has a default, so an older daemon and a newer CLI still understand each other |
| [`galdeck-model`](crates/galdeck-model) | The configuration model: the schema (profiles, pages and themes; keys and their gestures; actions, keystrokes and knob presets, layered from `galdeck.toml` down to a page, with dial modes; widgets, timers and thresholds), validation diagnostics with stable codes, and write-back that preserves comments and formatting |

`galdeck-ipc` and `galdeck-model` are published to crates.io, because the
daemon consumes them from there. `galdeck-model` is deliberately pure — no
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
