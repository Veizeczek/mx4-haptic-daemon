use std::{
    fs,
    path::Path,
    sync::mpsc::{self, RecvTimeoutError},
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result};

use crate::{
    config::{config_path, load_from_path, load_or_create},
    device::DeviceManager,
    input::{self, Click, ClickSource},
};

/// How often the daemon checks whether the config file was saved again.
const CONFIG_CHECK_INTERVAL: Duration = Duration::from_secs(1);

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

pub fn run(debug: bool) -> Result<()> {
    let mut config = load_or_create()?;
    let path = config_path()?;
    let mut config_mtime = modified(&path);
    let mut last_config_check = Instant::now();
    let (sender, receiver) = mpsc::sync_channel::<Click>(config.queue_size);
    input::subscribe(sender, debug).context(
        "could not start the global mouse observer; check system permissions (e.g. Accessibility on macOS) and restart",
    )?;

    if debug {
        eprintln!(
            "debug: config enabled={}, target={}, only_mx_master={}, cooldown={}ms, queue={}",
            config.enabled,
            config.device,
            config.only_mx_master,
            config.cooldown_ms,
            config.queue_size
        );
    }

    let mut devices = DeviceManager::with_expected_name(debug, config.device.clone());
    devices.reconnect_if_due();
    let mut last_haptic = Instant::now() - Duration::from_millis(config.cooldown_ms);

    loop {
        let event = receiver.recv_timeout(Duration::from_secs(2));

        // Apply settings saved from the GUI without a restart. One stat()
        // per second at most; the file is only parsed when it changed.
        if last_config_check.elapsed() >= CONFIG_CHECK_INTERVAL {
            last_config_check = Instant::now();
            let mtime = modified(&path);
            if mtime != config_mtime {
                config_mtime = mtime;
                match load_from_path(&path) {
                    Ok(new_config) => {
                        if debug {
                            eprintln!("debug: configuration reloaded");
                        }
                        if new_config.device != config.device {
                            devices =
                                DeviceManager::with_expected_name(debug, new_config.device.clone());
                        }
                        config = new_config;
                    }
                    Err(error) => {
                        if debug {
                            eprintln!("debug: keeping previous configuration: {error:#}");
                        }
                    }
                }
            }
        }

        match event {
            Ok(Click { button, source }) => {
                if config.only_mx_master && source == ClickSource::OtherDevice {
                    continue;
                }
                let Some(waveform) = config.waveform_for(button) else {
                    continue;
                };
                if !config.enabled
                    || last_haptic.elapsed() < Duration::from_millis(config.cooldown_ms)
                {
                    continue;
                }

                // Update before I/O: rapid clicks cannot build an unbounded set
                // of late haptic requests when Bluetooth is slow or asleep.
                last_haptic = Instant::now();
                if debug {
                    eprintln!("debug: playing haptic {}", waveform);
                }
                if let Err(error) = devices.play(waveform) {
                    if debug {
                        eprintln!("debug: haptic {waveform} skipped: {error:#}");
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => devices.reconnect_if_due(),
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}
