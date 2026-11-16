#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// #![forbid(unsafe_code)]

mod config;
mod daemon;
mod device;
mod haptic;
mod input;
mod gui;
mod i18n;
mod tray;
#[cfg(target_os = "windows")]
mod autostart;

use std::env;

use anyhow::{Context, Result, bail};

use crate::{
    config::Waveform,
    device::{DeviceManager, list_devices},
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Default,
    Foreground,
    ListDevices,
    Test(Waveform),
    Gui,
    Help,
    Version,
}

#[derive(Debug)]
struct Cli {
    command: Command,
    debug: bool,
    /// Windows only: start in the tray without opening the settings window.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    minimized: bool,
}

/// Holds a named mutex for the lifetime of the process. Returns `None` when
/// another process already holds it.
#[cfg(target_os = "windows")]
fn acquire_instance(name: windows::core::PCWSTR) -> Option<windows::Win32::Foundation::HANDLE> {
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Threading::CreateMutexW;

    unsafe {
        let handle = CreateMutexW(None, false, name).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            None
        } else {
            Some(handle)
        }
    }
}

fn main() -> Result<()> {
    let cli = parse_cli(env::args().skip(1))?;

    match cli.command {
        Command::Help => {
            print_help();
            Ok(())
        }
        Command::Version => {
            println!("mx4-haptic-daemon {VERSION}");
            Ok(())
        }
        Command::ListDevices => list_command(cli.debug),
        Command::Test(waveform) => test_command(waveform, cli.debug),
        Command::Foreground => daemon::run(cli.debug),
        Command::Gui => run_settings_window(),
        Command::Default => run_default(cli),
    }
}

/// Settings window only. On Windows a second one just focuses the first.
fn run_settings_window() -> Result<()> {
    #[cfg(target_os = "windows")]
    let Some(_settings_mutex) = acquire_instance(windows::core::w!("Local\\MX4Haptic.Settings"))
    else {
        tray::focus_settings_window();
        return Ok(());
    };
    gui::run()
}

/// Windows: daemon + tray icon in a lightweight background process; the
/// settings window is opened from the tray as a separate `--gui` process.
#[cfg(target_os = "windows")]
fn run_default(cli: Cli) -> Result<()> {
    let Some(_instance_mutex) =
        acquire_instance(windows::core::w!("Local\\MX4Haptic.Background"))
    else {
        // Already running in the tray: launching the app again (Start menu)
        // opens the settings instead of silently doing nothing.
        if !cli.minimized {
            tray::open_settings_in_background();
        }
        return Ok(());
    };
    spawn_daemon(cli.debug);
    tray::run_background(!cli.minimized)
}

/// macOS: daemon thread + settings window in one process (no tray).
#[cfg(not(target_os = "windows"))]
fn run_default(cli: Cli) -> Result<()> {
    spawn_daemon(cli.debug);
    gui::run()
}

fn spawn_daemon(debug: bool) {
    std::thread::spawn(move || {
        if let Err(e) = daemon::run(debug) {
            eprintln!("daemon error: {e}");
        }
    });
}

fn list_command(debug: bool) -> Result<()> {
    let devices = list_devices()?;
    if devices.is_empty() {
        println!("No Logitech HID++ vendor collections are currently visible.");
        return Ok(());
    }

    for device in devices {
        println!(
            "{}  vid={:04X} pid={:04X}  usage={:04X}/{:04X}  transport={}  target={}",
            device.name,
            device.vendor_id,
            device.product_id,
            device.usage_page,
            device.usage_id,
            device.transport_name(),
            if device.is_mx4_target() { "yes" } else { "no" },
        );
    }

    if debug {
        eprintln!("debug: only target=yes is eligible for haptic output");
    }
    Ok(())
}

fn test_command(waveform: Waveform, debug: bool) -> Result<()> {
    let mut devices = DeviceManager::new(debug);
    devices.play(waveform).with_context(|| {
        format!(
            "could not play {} on the MX Master 4; check Bluetooth, device visibility, and HID permissions",
            waveform
        )
    })
}

fn parse_cli(args: impl IntoIterator<Item = String>) -> Result<Cli> {
    let mut positional = Vec::new();
    let mut debug = false;
    let mut minimized = false;
    for arg in args {
        if arg == "--debug" {
            debug = true;
        } else if arg == "--minimized" {
            minimized = true;
        } else {
            positional.push(arg);
        }
    }

    let command = match positional.as_slice() {
        [] => Command::Default,
        [arg] if arg.starts_with("-psn") => Command::Default,
        [command] if command == "--foreground" => Command::Foreground,
        [command] if command == "--gui" => Command::Gui,
        [command] if command == "--help" || command == "-h" => Command::Help,
        [command] if command == "--version" || command == "-V" => Command::Version,
        [command] if command == "--list-devices" => Command::ListDevices,
        [command, waveform] if command == "--test" => Command::Test(Waveform::parse(waveform)?),
        _ => bail!("invalid arguments; run mx4-haptic-daemon --help"),
    };

    Ok(Cli {
        command,
        debug,
        minimized,
    })
}

fn print_help() {
    println!(
        "mx4-haptic-daemon {VERSION}\n\n\
Native macOS haptic daemon for Logitech MX Master 4 for Mac.\n\n\
USAGE:\n\
  mx4-haptic-daemon [--debug] --foreground\n\
  mx4-haptic-daemon [--debug] --gui\n\
  mx4-haptic-daemon [--debug] --list-devices\n\
  mx4-haptic-daemon [--debug] --test <WAVEFORM>\n\n\
OPTIONS:\n\
  --foreground    Run the event listener in the current process (the default).\n\
  --gui           Open only the settings window.\n\
  --list-devices  List visible Logitech HID++ vendor collections.\n\
  --test WAVEFORM Send one supported haptic waveform and exit.\n\
  --minimized     Start in the system tray without opening settings (Windows).\n\
  --debug         Print device discovery, reconnect, and HID++ errors to stderr.\n\
  --version       Print version.\n\
  --help          Print this help.\n\n\
The foreground listener needs macOS Accessibility permission."
    );
}

#[cfg(test)]
mod tests {
    use super::{Command, parse_cli};
    use crate::config::Waveform;

    #[test]
    fn parses_test_and_debug_in_any_order() {
        let cli = parse_cli(["--test", "KNOCK", "--debug"].map(str::to_owned)).expect("valid CLI");
        assert!(cli.debug);
        assert_eq!(cli.command, Command::Test(Waveform::Knock));
    }
}
