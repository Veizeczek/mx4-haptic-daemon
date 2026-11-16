use std::{
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{SyncSender, TrySendError},
    },
};

use anyhow::{Result, anyhow};

use crate::config::MouseButton;

/// Which physical device produced a click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickSource {
    /// The MX Master 4 the daemon drives.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    TargetMouse,
    /// Another mouse, a touchpad, or synthetic input (SendInput, remote desktop).
    OtherDevice,
    /// The platform listener cannot tell devices apart (macOS event tap).
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Click {
    pub button: MouseButton,
    pub source: ClickSource,
}

/// Everyone interested in global clicks (the daemon loop, the GUI input
/// tester). A single OS-level listener feeds all of them, so opening the
/// tester does not install a second hook that runs on every mouse move.
static SUBSCRIBERS: Mutex<Vec<SyncSender<Click>>> = Mutex::new(Vec::new());
static LISTENER: OnceLock<Result<(), String>> = OnceLock::new();
static DEBUG: AtomicBool = AtomicBool::new(false);

/// Registers `sender` for global mouse button presses, starting the shared
/// OS listener on first use. Events are delivered with `try_send`, so a slow
/// subscriber drops clicks instead of stalling system input.
pub fn subscribe(sender: SyncSender<Click>, debug: bool) -> Result<()> {
    if debug {
        DEBUG.store(true, Ordering::Relaxed);
    }
    LISTENER
        .get_or_init(|| start_listener().map_err(|error| format!("{error:#}")))
        .clone()
        .map_err(|error| anyhow!(error))?;
    SUBSCRIBERS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(sender);
    Ok(())
}

fn debug_enabled() -> bool {
    DEBUG.load(Ordering::Relaxed)
}

fn dispatch(click: Click) {
    let mut subscribers = SUBSCRIBERS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    subscribers.retain(|sender| !matches!(sender.try_send(click), Err(TrySendError::Disconnected(_))));
}

/// Asks the listener to re-take ownership of raw mouse input. Call after the
/// GUI event loop exists: winit registers raw input for its own window on
/// startup, which would otherwise silently redirect our clicks to it.
#[cfg(target_os = "windows")]
pub fn reclaim() {
    windows_impl::reclaim();
}

#[cfg(not(target_os = "windows"))]
pub fn reclaim() {}

#[cfg(target_os = "macos")]
fn start_listener() -> Result<()> {
    std::thread::Builder::new()
        .name("mx4-haptic-input".to_owned())
        .spawn(move || {
            let result = rdev::listen(move |event| {
                let debug = debug_enabled();
                if debug {
                    eprintln!("debug: received rdev event: {:?}", event.event_type);
                }
                let button = match event.event_type {
                    rdev::EventType::ButtonPress(rdev::Button::Left) => Some(MouseButton::Left),
                    rdev::EventType::ButtonPress(rdev::Button::Right) => {
                        Some(MouseButton::Right)
                    }
                    rdev::EventType::ButtonPress(rdev::Button::Middle) => {
                        Some(MouseButton::Middle)
                    }
                    rdev::EventType::ButtonPress(rdev::Button::Unknown(4)) => {
                        Some(MouseButton::Forward)
                    }
                    rdev::EventType::ButtonPress(rdev::Button::Unknown(5)) => {
                        Some(MouseButton::Back)
                    }
                    rdev::EventType::ButtonPress(rdev::Button::Unknown(u)) => {
                        if debug {
                            eprintln!("debug: unknown button pressed: {}", u);
                        }
                        None
                    }
                    _ => None,
                };
                if let Some(button) = button {
                    // A CGEvent tap does not say which mouse sent the event.
                    dispatch(Click {
                        button,
                        source: ClickSource::Unknown,
                    });
                }
            });
            if let Err(error) = result {
                if debug_enabled() {
                    eprintln!("debug: macOS event tap ended: {error:?}");
                }
            }
        })?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn start_listener() -> Result<()> {
    windows_impl::start()
}

/// Raw Input instead of a `WH_MOUSE_LL` hook: every raw event carries the
/// handle of the device that produced it, so clicks from other mice and
/// touchpads can be told apart from the MX Master 4. It is also cheaper — a
/// low-level hook sits synchronously in the system input path for every
/// mouse move, raw input is delivered asynchronously.
#[cfg(target_os = "windows")]
mod windows_impl {
    use std::{
        cell::RefCell,
        collections::HashMap,
        ffi::c_void,
        mem::size_of,
        sync::atomic::{AtomicIsize, Ordering},
        thread,
    };

    use anyhow::{Context, Result};
    use windows::{
        Win32::{
            Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM},
            System::LibraryLoader::GetModuleHandleW,
            UI::{
                Input::{
                    GetRawInputData, GetRawInputDeviceInfoW, GetRegisteredRawInputDevices,
                    HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT,
                    RIDEV_DEVNOTIFY, RIDEV_INPUTSINK, RIDI_DEVICENAME, RIM_TYPEMOUSE, RegisterRawInputDevices,
                },
                WindowsAndMessaging::{
                    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HWND_MESSAGE,
                    MSG, PostMessageW, RI_MOUSE_BUTTON_4_DOWN, RI_MOUSE_BUTTON_5_DOWN,
                    RI_MOUSE_LEFT_BUTTON_DOWN, RI_MOUSE_MIDDLE_BUTTON_DOWN,
                    RI_MOUSE_RIGHT_BUTTON_DOWN, RegisterClassW, SetTimer, WINDOW_EX_STYLE,
                    WINDOW_STYLE, WM_APP, WM_INPUT, WM_INPUT_DEVICE_CHANGE, WM_TIMER,
                    WNDCLASSW,
                },
            },
        },
        core::w,
    };

    use super::{Click, ClickSource, debug_enabled, dispatch};
    use crate::{config::MouseButton, device::is_mx4_device_path};

    const HID_USAGE_PAGE_GENERIC: u16 = 0x01;
    const HID_USAGE_GENERIC_MOUSE: u16 = 0x02;
    const WM_RECLAIM: u32 = WM_APP + 1;
    const WATCHDOG_TIMER_ID: usize = 1;
    /// How often to verify nobody else in the process took the raw mouse
    /// registration (one cheap call, no work when it is still ours).
    const WATCHDOG_INTERVAL_MS: u32 = 5_000;

    static LISTENER_HWND: AtomicIsize = AtomicIsize::new(0);

    thread_local! {
        /// Device handle -> "is this the MX Master 4". Looking up the device
        /// path is a syscall; handles are stable while the device stays
        /// connected and are only looked up on clicks, never on moves.
        static DEVICE_CACHE: RefCell<HashMap<isize, bool>> = RefCell::new(HashMap::new());
    }

    pub fn start() -> Result<()> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        thread::Builder::new()
            .name("mx4-haptic-input".to_owned())
            .spawn(move || unsafe { run(ready_tx) })?;
        ready_rx
            .recv()
            .context("raw input listener thread exited during startup")?
    }

    pub fn reclaim() {
        let hwnd = LISTENER_HWND.load(Ordering::Acquire);
        if hwnd != 0 {
            unsafe {
                let _ = PostMessageW(HWND(hwnd as *mut c_void), WM_RECLAIM, WPARAM(0), LPARAM(0));
            }
        }
    }

    unsafe fn run(ready: std::sync::mpsc::Sender<Result<()>>) {
        let hwnd = match unsafe { create_message_window() } {
            Ok(hwnd) => hwnd,
            Err(error) => {
                let _ = ready.send(Err(error));
                return;
            }
        };
        if let Err(error) = register(hwnd) {
            let _ = ready.send(Err(error));
            return;
        }
        LISTENER_HWND.store(hwnd.0 as isize, Ordering::Release);
        unsafe { SetTimer(hwnd, WATCHDOG_TIMER_ID, WATCHDOG_INTERVAL_MS, None) };
        let _ = ready.send(Ok(()));

        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            unsafe { DispatchMessageW(&msg) };
        }
    }

    unsafe fn create_message_window() -> Result<HWND> {
        let instance = unsafe { GetModuleHandleW(None) }.context("GetModuleHandleW failed")?;
        let class_name = w!("MX4HapticRawInputListener");
        let class = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        unsafe { RegisterClassW(&class) };
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                w!(""),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                None,
                instance,
                None,
            )
        }
        .context("could not create the raw input message window")
    }

    fn register(hwnd: HWND) -> Result<()> {
        let device = RAWINPUTDEVICE {
            usUsagePage: HID_USAGE_PAGE_GENERIC,
            usUsage: HID_USAGE_GENERIC_MOUSE,
            // INPUTSINK: receive input even when another app has focus.
            // DEVNOTIFY: hear about hot-plugging so cached handles stay valid.
            dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
            hwndTarget: hwnd,
        };
        unsafe { RegisterRawInputDevices(&[device], size_of::<RAWINPUTDEVICE>() as u32) }
            .context("RegisterRawInputDevices failed")
    }

    /// Raw input allows a single target window per usage in a process; winit
    /// claims mice for its own window when the GUI starts.
    fn ensure_registered(hwnd: HWND) {
        let mut devices = [RAWINPUTDEVICE::default(); 4];
        let mut count = devices.len() as u32;
        let found = unsafe {
            GetRegisteredRawInputDevices(
                Some(devices.as_mut_ptr()),
                &mut count,
                size_of::<RAWINPUTDEVICE>() as u32,
            )
        };
        let ours = found != u32::MAX
            && devices[..found as usize].iter().any(|d| {
                d.usUsagePage == HID_USAGE_PAGE_GENERIC
                    && d.usUsage == HID_USAGE_GENERIC_MOUSE
                    && d.hwndTarget == hwnd
            });
        if !ours {
            if debug_enabled() {
                eprintln!("debug: raw mouse input was taken over; re-registering");
            }
            if let Err(error) = register(hwnd) {
                if debug_enabled() {
                    eprintln!("debug: {error:#}");
                }
            }
        }
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_INPUT => handle_raw_input(HRAWINPUT(lparam.0 as *mut c_void)),
            WM_TIMER | WM_RECLAIM => ensure_registered(hwnd),
            // Handles can be reused after a device goes away.
            WM_INPUT_DEVICE_CHANGE => DEVICE_CACHE.with(|cache| cache.borrow_mut().clear()),
            _ => {}
        }
        // DefWindowProc must also see WM_INPUT so the system frees the data.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn handle_raw_input(handle: HRAWINPUT) {
        let mut raw = RAWINPUT::default();
        let mut size = size_of::<RAWINPUT>() as u32;
        let read = unsafe {
            GetRawInputData(
                handle,
                RID_INPUT,
                Some(&mut raw as *mut RAWINPUT as *mut c_void),
                &mut size,
                size_of::<RAWINPUTHEADER>() as u32,
            )
        };
        if read == u32::MAX || raw.header.dwType != RIM_TYPEMOUSE.0 {
            return;
        }
        let flags = u32::from(unsafe { raw.data.mouse.Anonymous.Anonymous.usButtonFlags });
        if flags == 0 {
            // Plain movement: the hot path, no further work.
            return;
        }

        const BUTTONS: [(u32, MouseButton); 5] = [
            (RI_MOUSE_LEFT_BUTTON_DOWN, MouseButton::Left),
            (RI_MOUSE_RIGHT_BUTTON_DOWN, MouseButton::Right),
            (RI_MOUSE_MIDDLE_BUTTON_DOWN, MouseButton::Middle),
            (RI_MOUSE_BUTTON_4_DOWN, MouseButton::Back),
            (RI_MOUSE_BUTTON_5_DOWN, MouseButton::Forward),
        ];
        let mut source = None;
        for (flag, button) in BUTTONS {
            if flags & flag == 0 {
                continue;
            }
            let source = *source.get_or_insert_with(|| classify(raw.header.hDevice));
            if debug_enabled() {
                eprintln!("debug: raw input {button:?} from {source:?}");
            }
            dispatch(Click { button, source });
        }
    }

    fn classify(device: HANDLE) -> ClickSource {
        // Injected input (SendInput, remote desktop) has no device handle.
        if device.is_invalid() || device.0.is_null() {
            return ClickSource::OtherDevice;
        }
        let key = device.0 as isize;
        let is_target = DEVICE_CACHE.with(|cache| {
            *cache
                .borrow_mut()
                .entry(key)
                .or_insert_with(|| device_path(device).is_some_and(|path| {
                    let target = is_mx4_device_path(&path);
                    if debug_enabled() {
                        eprintln!("debug: mouse device {path} -> target={target}");
                    }
                    target
                }))
        });
        if is_target {
            ClickSource::TargetMouse
        } else {
            ClickSource::OtherDevice
        }
    }

    fn device_path(device: HANDLE) -> Option<String> {
        let mut len = 0_u32;
        unsafe { GetRawInputDeviceInfoW(device, RIDI_DEVICENAME, None, &mut len) };
        if len == 0 {
            return None;
        }
        let mut buffer = vec![0_u16; len as usize];
        let written = unsafe {
            GetRawInputDeviceInfoW(
                device,
                RIDI_DEVICENAME,
                Some(buffer.as_mut_ptr() as *mut c_void),
                &mut len,
            )
        };
        if written == u32::MAX {
            return None;
        }
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn start_listener() -> Result<()> {
    anyhow::bail!("mx4-haptic-daemon can only install its global event listener on macOS or Windows")
}
