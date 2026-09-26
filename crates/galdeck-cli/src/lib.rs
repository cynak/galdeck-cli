/// Keep the main.rs clean of clutter but all constants and
/// publicly used strcutures in here
///
///
///
/// Control the galdeck daemon (Corsair Galleon 100 SD Stream Deck module).
use clap::{Parser, Subcommand};

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
    /// Print the configuration UI's address, token included.
    Ui,
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
