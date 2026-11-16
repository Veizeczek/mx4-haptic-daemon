//! Background mode with a system tray icon (Windows only).
//!
//! The background process runs only the daemon, the tray icon and a plain
//! Win32 message loop — no window, no OpenGL context. The settings window is
//! a separate `--gui` process started from the tray and fully closed again.
//!
//! An earlier version kept the eframe window alive and hid it. That cost
//! ~80 MB and, worse, could burn a whole CPU core: while the window is hidden
//! any repaint request switches winit to `ControlFlow::Poll` and waits for a
//! `WM_PAINT` that Windows never sends to an invisible window, so the event
//! loop spins until some unrelated event arrives.

/// Title of the settings window, also used to find and focus it.
pub const WINDOW_TITLE: &str = "MX Master 4 Haptic";

/// RGBA pixels of the app icon (blue disc with "vibration" rings), used for
/// the tray and the window. Drawn in code so no image assets are needed.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let scale = size as f32 / 32.0;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    let center = (size as f32 - 1.0) / 2.0;
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 - center, y as f32 - center);
            let dist = (fx * fx + fy * fy).sqrt() / scale;
            // Anti-aliased disc edge.
            let alpha = ((15.5 - dist) * scale).clamp(0.0, 1.0);
            // Two concentric rings and a solid core.
            let ring = |r: f32| ((1.6 - (dist - r).abs()) * scale).clamp(0.0, 1.0);
            let white = ring(10.0).max(ring(5.5)).max(((2.6 - dist) * scale).clamp(0.0, 1.0));
            let base = [37.0_f32, 99.0, 235.0];
            let px = |c: f32| (c + (255.0 - c) * white).round() as u8;
            rgba.extend_from_slice(&[
                px(base[0]),
                px(base[1]),
                px(base[2]),
                (alpha * 255.0).round() as u8,
            ]);
        }
    }
    rgba
}

#[cfg(target_os = "windows")]
pub use windows_impl::{
    focus_settings_window, open_settings_in_background, restart_background, run_background,
};

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::{
        cell::RefCell,
        process::{Child, Command},
        sync::Mutex,
        time::{Duration, Instant},
    };

    use anyhow::{Context, Result};
    use tray_icon::{
        Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
        menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
    };
    use windows::{
        Win32::{
            Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WAIT_OBJECT_0, WPARAM},
            System::{
                LibraryLoader::GetModuleHandleW,
                Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
            },
            UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowExW, FindWindowW,
                GetMessageW, GetWindowThreadProcessId, HWND_MESSAGE, IsIconic, MSG,
                PostMessageW, PostQuitMessage, RegisterClassW, SW_RESTORE, SW_SHOW,
                SetForegroundWindow, ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE,
                WM_APP, WNDCLASSW,
            },
        },
        core::{HSTRING, PCWSTR, w},
    };

    use super::WINDOW_TITLE;
    use crate::{config::load_or_create, i18n::Language};

    const MENU_OPEN: &str = "open";
    const MENU_QUIT: &str = "quit";

    /// Message-only window of the background process, so other instances can
    /// ask it to open the settings or to exit for a restart.
    const CONTROL_CLASS: PCWSTR = w!("MX4HapticBackground");
    const WM_OPEN_SETTINGS: u32 = WM_APP + 1;
    const WM_EXIT_FOR_RESTART: u32 = WM_APP + 2;

    /// The settings process we started, so "Quit" can close it as well.
    static SETTINGS: Mutex<Option<Child>> = Mutex::new(None);

    thread_local! {
        // Menu items live on the UI thread; re-labelled when the language changes.
        static MENU_ITEMS: RefCell<Option<(MenuItem, MenuItem)>> = const { RefCell::new(None) };
    }

    /// Shows the tray icon and pumps Win32 messages until "Quit".
    /// The daemon runs on its own thread; this thread only sleeps in
    /// `GetMessageW` between tray clicks.
    pub fn run_background(open_settings_now: bool) -> Result<()> {
        create_control_window()?;

        let open_item = MenuItem::with_id(MenuId::new(MENU_OPEN), "", true, None);
        let quit_item = MenuItem::with_id(MenuId::new(MENU_QUIT), "", true, None);
        let menu = Menu::new();
        menu.append_items(&[&open_item, &PredefinedMenuItem::separator(), &quit_item])
            .map_err(|e| anyhow::anyhow!("tray menu error: {e:?}"))?;
        MENU_ITEMS.with(|items| *items.borrow_mut() = Some((open_item, quit_item)));
        relabel_menu();

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("MX Master 4 Haptic")
            .with_icon(Icon::from_rgba(super::icon_rgba(32), 32, 32)?)
            .build()
            .context("could not create the tray icon")?;

        // Handlers run on this thread, from inside DispatchMessageW below.
        // tray-icon reports the right click before it shows the menu.
        TrayIconEvent::set_event_handler(Some(|event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => open_settings(),
            TrayIconEvent::Click {
                button: MouseButton::Right,
                button_state: MouseButtonState::Up,
                ..
            } => relabel_menu(),
            _ => {}
        }));
        MenuEvent::set_event_handler(Some(|event: MenuEvent| match event.id.0.as_str() {
            MENU_OPEN => open_settings(),
            MENU_QUIT => {
                if let Some(mut child) = lock_settings().take() {
                    let _ = child.kill();
                }
                unsafe { PostQuitMessage(0) }
            }
            _ => {}
        }));

        if open_settings_now {
            open_settings();
        }

        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // Remove the icon explicitly, otherwise Windows leaves a dead icon in
        // the tray until the user hovers over it.
        let _ = tray.set_visible(false);
        drop(tray);
        Ok(())
    }

    /// Menu labels follow the configured language; reading the small config
    /// file only when the menu opens costs nothing in the background.
    fn relabel_menu() {
        let language = load_or_create().map_or_else(|_| Language::system(), |c| c.ui_language());
        let strings = language.strings();
        MENU_ITEMS.with(|items| {
            if let Some((open, quit)) = items.borrow().as_ref() {
                open.set_text(strings.tray_open);
                quit.set_text(strings.tray_quit);
            }
        });
    }

    fn create_control_window() -> Result<()> {
        unsafe {
            let instance = GetModuleHandleW(None).context("GetModuleHandleW failed")?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(control_proc),
                hInstance: instance.into(),
                lpszClassName: CONTROL_CLASS,
                ..Default::default()
            };
            RegisterClassW(&class);
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                CONTROL_CLASS,
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
            .context("could not create the background control window")?;
        }
        Ok(())
    }

    unsafe extern "system" fn control_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_OPEN_SETTINGS => open_settings(),
            // Leave the settings window running: it asked for the restart.
            WM_EXIT_FOR_RESTART => unsafe { PostQuitMessage(0) },
            _ => {}
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn find_control_window() -> Option<HWND> {
        unsafe { FindWindowExW(HWND_MESSAGE, None, CONTROL_CLASS, PCWSTR::null()) }
            .ok()
            .filter(|hwnd| !hwnd.0.is_null())
    }

    /// Called by a second launch: lets the running background process open
    /// (and keep track of) the settings window.
    pub fn open_settings_in_background() {
        match find_control_window() {
            Some(hwnd) => unsafe {
                let _ = PostMessageW(hwnd, WM_OPEN_SETTINGS, WPARAM(0), LPARAM(0));
            },
            None => open_settings(),
        }
    }

    /// Stops the background process (if any) and starts a fresh one in the
    /// tray. Used by the "Restart" button of the settings window.
    pub fn restart_background() -> Result<()> {
        if let Some(hwnd) = find_control_window() {
            let mut pid = 0_u32;
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }
                .context("could not open the background process")?;
            unsafe { PostMessageW(hwnd, WM_EXIT_FOR_RESTART, WPARAM(0), LPARAM(0)) }
                .context("could not signal the background process")?;
            let exited = unsafe { WaitForSingleObject(process, 5_000) } == WAIT_OBJECT_0;
            unsafe {
                let _ = CloseHandle(process);
            }
            anyhow::ensure!(exited, "the background process did not exit in time");
        }
        let exe = std::env::current_exe()?;
        Command::new(exe)
            .arg("--minimized")
            .spawn()
            .context("could not start the background process")?;

        // Wait until the new instance is up, so the button reports reality.
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if find_control_window().is_some() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        anyhow::bail!("the background process did not start")
    }

    /// Brings the settings window to the front, starting it if needed.
    fn open_settings() {
        if focus_settings_window() {
            return;
        }
        let mut settings = lock_settings();
        // A previous settings process may still be starting up.
        if let Some(child) = settings.as_mut() {
            if matches!(child.try_wait(), Ok(None)) {
                return;
            }
        }
        match std::env::current_exe().and_then(|exe| Command::new(exe).arg("--gui").spawn()) {
            Ok(child) => *settings = Some(child),
            Err(error) => eprintln!("could not start the settings window: {error}"),
        }
    }

    /// Focuses an open settings window; returns false if there is none.
    pub fn focus_settings_window() -> bool {
        let title = HSTRING::from(WINDOW_TITLE);
        let Ok(hwnd) = (unsafe { FindWindowW(PCWSTR::null(), &title) }) else {
            return false;
        };
        if hwnd.0.is_null() {
            return false;
        }
        unsafe {
            let cmd = if IsIconic(hwnd).as_bool() { SW_RESTORE } else { SW_SHOW };
            let _ = ShowWindow(hwnd, cmd);
            let _ = SetForegroundWindow(hwnd);
        }
        true
    }

    fn lock_settings() -> std::sync::MutexGuard<'static, Option<Child>> {
        SETTINGS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
