# MX Master 4 Haptic Daemon

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform: Windows | macOS](https://img.shields.io/badge/Platform-Windows%20%7C%20macOS-lightgrey.svg)](#-requirements)
[![Rust](https://img.shields.io/badge/Rust-2024%20Edition-orange.svg)](https://www.rust-lang.org/)

A lightweight native app for **Windows** and **macOS** that plays physical haptic feedback on the **Logitech MX Master 4** when you click its buttons (Left, Right, Middle, Forward, Back).

No Logitech Options+, Electron or Python. A single Rust binary talking HID++ directly to the mouse.

---

## ✨ Features

- ⚡ **Lightweight:** in the background only the daemon and the tray icon run (~12 MB RAM, ~0% CPU); the settings window is a separate process that exits when you close it.
- 🎯 **Only your MX Master vibrates (Windows):** clicks from other mice, touchpads or remote-desktop input are ignored. Can be turned off in settings.
- 🗂️ **System tray (Windows):** left-click the icon to open settings, right-click → *Quit* to exit. Launching the app again from the Start menu also opens the settings.
- 🌍 **English and Polish UI:** follows the system language by default, switchable in the *Language* tab.
- 🎛️ **Settings window:** per-button waveform mapping with a ▶ test button, connection status, and a live input tester that shows which device each click came from.
- 💾 **Changes apply instantly:** saving settings is picked up by the running daemon within a second, no restart needed.
- 🚀 **Autostart:** starts minimized to the tray at login (Windows) or as a LaunchAgent (macOS).
- 🌊 **16 built-in waveforms** advertised by the mouse firmware (Knock, Subtle Collision, Sharp State Change, …); only the ones your mouse reports are offered.
- 🛡️ **Rate limiting:** a cooldown prevents a backlog of haptics when clicking fast or while Bluetooth wakes up.
- 🔁 **Automatic reconnection** after sleep / Bluetooth drops, with exponential backoff.

---

## 📋 Requirements

| | Windows | macOS |
| :--- | :--- | :--- |
| OS | Windows 10 / 11 | macOS 12+ (Apple Silicon or Intel) |
| Mouse | MX Master 4 **for Mac** (`046D:B042`) over **Bluetooth** — not the Logi Bolt receiver | same |
| Build tools | [Rust](https://rustup.rs) + Visual Studio **C++ Build Tools** | [Rust](https://rustup.rs) (Xcode Command Line Tools) |
| Permissions | none (no admin needed) | **Accessibility** (and possibly **Input Monitoring**) |

The Rust version is pinned in `rust-toolchain.toml`; `rustup` installs it automatically on the first build.

---

## 🪟 Installation on Windows

### 1. Install the build tools (once)

1. **Visual Studio C++ Build Tools** – download *Build Tools for Visual Studio* from <https://visualstudio.microsoft.com/visual-cpp-build-tools/> and select the **“Desktop development with C++”** workload.
2. **Rust** – download and run `rustup-init.exe` from <https://rustup.rs> and accept the defaults.
3. **Git** – <https://git-scm.com/download/win> (or download the repository as a ZIP).

### 2. Pair the mouse over Bluetooth

*Settings → Bluetooth & devices → Add device → Bluetooth*, then hold the Easy-Switch button on the mouse until it blinks. The Logi Bolt receiver does **not** expose the haptic interface.

### 3. Build and install

Open **PowerShell** in the folder where you want the sources and run:

```powershell
git clone https://github.com/Veizeczek/mx4-haptic-daemon.git
cd mx4-haptic-daemon
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

The script:

- builds the release binary (`cargo build --release`, a few minutes the first time),
- stops a running copy, installs the exe to `%LOCALAPPDATA%\Programs\MX4Haptic\`,
- adds **MX4 Haptic** to the Start menu,
- enables start at login (minimized to the tray),
- starts the app in the tray.

No administrator rights are required. Options: `-NoAutostart` skips start at login, `-Binary <path>` installs an already built exe instead of building.

### 4. First run

1. Click the **blue round icon** in the tray (it may be hidden under the **^** arrow — drag it onto the taskbar to keep it visible).
2. Check that the header says **Connected**.
3. Pick a waveform for each button, try it with **▶**, then click **Save**. Changes apply immediately; **Cancel** discards unsaved edits.
4. Open **Input tester** and click with the MX Master — clicks should be labelled *MX Master*; clicks from other devices are labelled *other device* and do not vibrate.
5. The UI follows your system language; pick English or Polish in the **Language** tab.

Closing the settings window closes only the settings; haptics keep working in the background. To quit completely, right-click the tray icon → **Quit**. **Restart** in the settings window restarts the background service (needed only after changing the queue size).

### Updating

```powershell
cd mx4-haptic-daemon
git pull
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

### Uninstalling

```powershell
powershell -ExecutionPolicy Bypass -File .\uninstall.ps1               # keeps your settings
powershell -ExecutionPolicy Bypass -File .\uninstall.ps1 -RemoveConfig # removes settings too
```

---

## 🍎 Installation on macOS

### Option A: App bundle (recommended)

```sh
git clone https://github.com/Veizeczek/mx4-haptic-daemon.git
cd mx4-haptic-daemon
chmod +x build_app.sh
./build_app.sh
open "MX4 Haptic.app"
```

You can drag `MX4 Haptic.app` into `/Applications`. On macOS there is no tray icon: closing the window quits the app.

### Option B: Background service (LaunchAgent)

```sh
chmod +x install.sh uninstall.sh
./install.sh
```

- Installs the binary to `~/Library/Application Support/MX4Haptic/bin/mx4-haptic-daemon`.
- Registers and starts the LaunchAgent `~/Library/LaunchAgents/com.local.mx4-haptic.plist`.
- Logs to `~/Library/Application Support/MX4Haptic/daemon.log`.

Remove it with `./uninstall.sh`. The GUI (`mx4-haptic-daemon --gui`) can still be used to edit settings; the service picks up saved changes automatically, and **Restart** in the window restarts the LaunchAgent.

### Permissions

macOS protects global input events, so the app needs permission to see mouse clicks:

1. Open **System Settings → Privacy & Security → Accessibility**.
2. Click **+** and add the binary you run:
   - app bundle: `/Applications/MX4 Haptic.app`,
   - LaunchAgent: `~/Library/Application Support/MX4Haptic/bin/mx4-haptic-daemon`,
   - terminal: your terminal emulator (Terminal, iTerm, Ghostty, …).
3. If prompted, also allow it under **Input Monitoring**.
4. Relaunch the app / restart the service.

> [!NOTE]
> macOS event taps do not report which mouse sent a click, so on macOS clicks from any mouse trigger haptics on the MX Master.

---

## ⚙️ Configuration

Settings are edited in the app and stored in `config.toml`:

| Platform | Path |
| :--- | :--- |
| Windows | `%LOCALAPPDATA%\MX4Haptic\config.toml` |
| macOS | `~/Library/Application Support/MX4Haptic/config.toml` |

```toml
# MX Master 4 haptic feedback configuration
enabled = true
device = "MX Master 4"
only_mx_master = true
language = "auto"

left = "KNOCK"
right = "SUBTLE COLLISION"
middle = ""
forward = "SHARP STATE CHANGE"
back = "DAMP STATE CHANGE"

cooldown_ms = 30
queue_size = 4
autostart = true
```

| Option | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` | Global on/off switch for haptics. |
| `device` | string | `"MX Master 4"` | Substring of the device name to connect to. |
| `only_mx_master` | bool | `true` | Windows: ignore clicks from other mice, touchpads and injected input. |
| `language` | string | `"auto"` | UI language: `"auto"` (system language), `"en"` or `"pl"`. |
| `left` / `right` / `middle` | string | `"KNOCK"` / `"SUBTLE COLLISION"` / `""` | Waveform per button, `""` = none. |
| `forward` / `back` | string | `""` | Waveform for the side buttons. |
| `cooldown_ms` | int | `30` | Minimum time between two haptic pulses (0–5000). |
| `queue_size` | int | `4` | Pending clicks kept while the mouse wakes up (1–64). Needs a restart. |
| `autostart` | bool | `false` | Windows: start at login. The app reads the actual state from the registry. |

The running daemon reloads the file within a second of it being saved.

---

## 📳 Supported Waveforms

| Waveform | ID | Description |
| :--- | :--- | :--- |
| `SHARP STATE CHANGE` | `0x00` | Crisp transition pulse |
| `DAMP STATE CHANGE` | `0x01` | Soft transition pulse |
| `SHARP COLLISION` | `0x02` | Solid, immediate click |
| `DAMP COLLISION` | `0x03` | Muted collision |
| `SUBTLE COLLISION` | `0x04` | Very gentle impact |
| `HAPPY ALERT` | `0x05` | Ascending alert |
| `ANGRY ALERT` | `0x06` | Rapid warning buzz |
| `COMPLETED` | `0x07` | Dual-tone success |
| `SQUARE` | `0x08` | Square wave pulse |
| `WAVE` | `0x09` | Smooth sine wave |
| `FIREWORK` | `0x0A` | Burst pattern |
| `MAD` | `0x0B` | Intense multi-step vibration |
| `KNOCK` | `0x0C` | Natural mechanical tap |
| `JINGLE` | `0x0D` | Playful chime |
| `RINGING` | `0x0E` | Sustained ringing |
| `WHISPER COLLISION` | `0x1B` | Ultra-quiet tap |

---

## 🛠️ CLI Reference

```text
mx4-haptic-daemon [OPTIONS]

  (no options)    Windows: daemon + tray icon, and open the settings window
                  macOS:   daemon + settings window in one process
  --minimized     Windows: daemon + tray icon only (used by autostart)
  --gui           Settings window only (opened by the tray icon)
  --foreground    Daemon only, no window (used by the macOS LaunchAgent)
  --list-devices  List detected Logitech HID++ collections
  --test <NAME>   Play one waveform and exit, e.g. --test KNOCK
  --debug         Verbose diagnostics on stderr
  --version       Print version
  --help          Print help
```

On Windows the release exe has no console window, so CLI output is only visible from a debug build (`cargo run -- --list-devices`).

---

## ❓ Troubleshooting

<details>
<summary><b>The header says “Not connected”</b></summary>

1. The mouse must be paired over **Bluetooth**, not the Logi Bolt receiver.
2. Move the mouse to wake it up and press **⟳** next to the status.
3. Hover over the red status to see the exact error.
4. Close Logitech Options+ if it is running and try again.
5. Check visibility with a debug build: `cargo run -- --list-devices` should show `vid=046D pid=B042 usage=FF43/0202 target=yes`.

</details>

<details>
<summary><b>Clicks do not vibrate</b></summary>

1. Make sure **Haptics enabled** is on and the button has a waveform other than *— none —*, then **Save**.
2. Open **Input tester** and click with the MX Master:
   - nothing appears → the click listener is not working (macOS: check permissions),
   - it appears as *other device* → the mouse was not recognised as the MX Master; turn off **MX Master clicks only** and please open an issue with your mouse model.
3. A very high **cooldown** swallows fast clicks.

</details>

<details>
<summary><b>I cannot find the tray icon (Windows)</b></summary>

Click the **^** arrow in the taskbar's notification area. Drag the blue icon onto the taskbar to keep it visible. If the app is not running, start **MX4 Haptic** from the Start menu.

</details>

<details>
<summary><b>macOS LaunchAgent logs</b></summary>

```sh
tail -f ~/Library/Application\ Support/MX4Haptic/daemon.log
```

</details>

---

## 🔬 How it works

### Processes (Windows)

- **Background process** (`--minimized`, or no arguments): daemon thread, click listener and the tray icon on a plain Win32 message loop. No window and no OpenGL context, so nothing can redraw in the background.
- **Settings process** (`--gui`): started from the tray, closed with the window. Saved settings reach the daemon through the config file, which it reloads on change.

### Click detection

- **Windows:** [Raw Input](https://learn.microsoft.com/windows/win32/inputdev/raw-input) on a hidden message window (`RIDEV_INPUTSINK`). Each event carries the handle of the device that produced it; its device path is matched against `VID 046D / PID B042` (cached per handle). Mouse moves are discarded immediately, the device lookup only happens on clicks.
- **macOS:** a CGEvent tap via `rdev` (no per-device information).
- A single listener feeds both the daemon and the GUI input tester.

### HID++ 4.5 protocol

The app opens the mouse's Bluetooth LE HID++ vendor collection (`Usage Page 0xFF43, Usage 0x0202`) in shared mode, next to the system mouse driver:

1. **Root / GetFeature (`0x00`)** on device index `0xFF` resolves the index of feature `0x19B0` (`HapticFeedback`).
2. **GetCapabilities (function `0x00`)** reads the 32-bit waveform bitmap, so only supported waveforms are offered and played.
3. **PlayWaveform (function `0x04`)** sends a 20-byte long report:

```text
[ 0x11 | 0xFF | <feature index> | 0x41 | <waveform id> | 0x00 ... zero padded to 20 bytes ]
```

A dedicated reader thread receives responses so writes never wait behind a read timeout; it exits as soon as the connection is closed.

---

## 🌍 Translations

All UI strings live in [`src/i18n.rs`](src/i18n.rs), one table per language. To add a language, add a `Language` variant, copy the `ENGLISH` table, translate it and extend the `match`es in that file. Pull requests are welcome.

---

## 🧪 Development

```sh
cargo test                      # unit tests
cargo run -- --debug            # Windows: tray + settings with diagnostics
cargo run -- --list-devices     # visible Logitech HID++ collections
```

---

## 📄 License

[MIT](LICENSE)
