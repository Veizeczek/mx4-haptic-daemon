//! User interface translations.
//!
//! Every user-visible string of the settings window and the tray menu lives
//! here, once per language. To add a language: add a `Language` variant, a
//! `Strings` table and extend the `match`es below.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Polish,
    English,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::English, Language::Polish];

    /// Code stored in config.toml.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Polish => "pl",
            Self::English => "en",
        }
    }

    pub fn parse(code: &str) -> Option<Self> {
        match code.trim().to_ascii_lowercase().as_str() {
            "pl" | "polski" | "polish" => Some(Self::Polish),
            "en" | "english" | "angielski" => Some(Self::English),
            _ => None,
        }
    }

    /// The language's own name, shown in the picker.
    pub const fn native_name(self) -> &'static str {
        match self {
            Self::Polish => "Polski",
            Self::English => "English",
        }
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Self::Polish => &POLISH,
            Self::English => &ENGLISH,
        }
    }

    /// The operating system's UI language if we have a translation for it,
    /// English otherwise.
    pub fn system() -> Self {
        static SYSTEM: std::sync::OnceLock<Language> = std::sync::OnceLock::new();
        *SYSTEM.get_or_init(|| {
            if system_is_polish() { Self::Polish } else { Self::English }
        })
    }
}

#[cfg(target_os = "windows")]
fn system_is_polish() -> bool {
    const LANG_POLISH: u16 = 0x15;
    let langid = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    langid & 0x3ff == LANG_POLISH
}

#[cfg(not(target_os = "windows"))]
fn system_is_polish() -> bool {
    let from_env = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty() && value != "C" && value != "POSIX");
    if let Some(locale) = from_env {
        return locale.to_ascii_lowercase().starts_with("pl");
    }
    // Apps started from Finder get no LANG; ask macOS for the user locale.
    #[cfg(target_os = "macos")]
    if let Ok(output) = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
    {
        return String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_ascii_lowercase()
            .starts_with("pl");
    }
    false
}

// Some strings are only shown on one platform (e.g. macOS permissions).
#[allow(dead_code)]
pub struct Strings {
    // Header and tabs
    pub subtitle: &'static str,
    pub connecting: &'static str,
    pub connected: &'static str,
    pub disconnected: &'static str,
    pub reconnect: &'static str,
    pub tab_settings: &'static str,
    pub tab_tester: &'static str,
    pub tab_language: &'static str,

    // Settings: general
    pub card_general: &'static str,
    pub enabled: &'static str,
    pub enabled_hint: &'static str,
    pub only_mx: &'static str,
    pub only_mx_hint: &'static str,
    pub autostart: &'static str,
    pub autostart_hint: &'static str,

    // Settings: buttons
    pub card_buttons: &'static str,
    pub button_left: &'static str,
    pub button_right: &'static str,
    pub button_middle: &'static str,
    pub button_forward: &'static str,
    pub button_back: &'static str,
    pub waveform_none: &'static str,
    pub play_hint: &'static str,

    // Settings: advanced
    pub card_advanced: &'static str,
    pub cooldown: &'static str,
    pub cooldown_hint: &'static str,
    pub queue: &'static str,
    pub queue_hint: &'static str,

    // Settings: macOS permissions
    pub card_permissions: &'static str,
    pub permissions_hint: &'static str,
    pub accessibility: &'static str,
    pub input_monitoring: &'static str,

    // Input tester
    pub tester_title: &'static str,
    pub tester_hint: &'static str,
    pub recent_clicks: &'static str,
    pub waiting_for_click: &'static str,
    pub source_mx: &'static str,
    pub source_other: &'static str,
    /// Appended to a number of seconds: "5 s temu" / "5 s ago".
    pub seconds_ago: &'static str,
    pub clear: &'static str,

    // Language tab
    pub language_title: &'static str,
    pub language_hint: &'static str,
    pub language_auto: &'static str,

    // Footer
    pub save: &'static str,
    pub cancel: &'static str,
    pub restart: &'static str,
    pub restart_hint: &'static str,
    pub saved_done: &'static str,
    pub restarting: &'static str,
    pub restart_done: &'static str,
    pub failed: &'static str,

    // Error banner titles (the error detail is shown below)
    pub save_failed: &'static str,
    pub load_failed: &'static str,
    pub listener_failed: &'static str,
    pub autostart_failed: &'static str,

    // Tray menu
    pub tray_open: &'static str,
    pub tray_quit: &'static str,
}

pub static POLISH: Strings = Strings {
    subtitle: "Wibracje przy kliknięciach",
    connecting: "Łączenie…",
    connected: "Połączono",
    disconnected: "Brak połączenia",
    reconnect: "Połącz ponownie",
    tab_settings: "Ustawienia",
    tab_tester: "Tester wejścia",
    tab_language: "Język",

    card_general: "Ogólne",
    enabled: "Wibracje włączone",
    enabled_hint: "Globalny wyłącznik haptyki",
    only_mx: "Tylko kliknięcia MX Master",
    only_mx_hint: "Inne myszy i touchpad nie wywołują wibracji",
    autostart: "Uruchamiaj z systemem",
    autostart_hint: "Startuje w zasobniku systemowym",

    card_buttons: "Przyciski",
    button_left: "Lewy",
    button_right: "Prawy",
    button_middle: "Środkowy",
    button_forward: "Boczny — dalej",
    button_back: "Boczny — wstecz",
    waveform_none: "— brak —",
    play_hint: "Zagraj na myszce",

    card_advanced: "Zaawansowane",
    cooldown: "Odstęp między wibracjami",
    cooldown_hint: "Kliknięcia szybsze niż ten odstęp nie wywołują kolejnej wibracji",
    queue: "Rozmiar kolejki",
    queue_hint: "Ile kliknięć może czekać, gdy mysz się wybudza (działa po ponownym uruchomieniu)",

    card_permissions: "Uprawnienia systemowe",
    permissions_hint: "Jeśli kliknięcia lub test nie działają, nadaj uprawnienia:",
    accessibility: "Dostępność",
    input_monitoring: "Monitorowanie wejścia",

    tester_title: "Tester wejścia",
    tester_hint: "Kliknij dowolnym przyciskiem myszy w dowolnym miejscu ekranu. \
                  Jeśli nasłuch działa, kliknięcie pojawi się poniżej.",
    recent_clicks: "Ostatnie kliknięcia",
    waiting_for_click: "Czekam na kliknięcie…",
    source_mx: "MX Master",
    source_other: "inne urządzenie",
    seconds_ago: "s temu",
    clear: "Wyczyść",

    language_title: "Język interfejsu",
    language_hint: "Zmiana działa od razu i jest zapisywana automatycznie.",
    language_auto: "Automatycznie (język systemu)",

    save: "Zapisz",
    cancel: "Anuluj",
    restart: "Uruchom ponownie",
    restart_hint: "Uruchamia ponownie usługę w tle (np. po zmianie rozmiaru kolejki)",
    saved_done: "Zapisano!",
    restarting: "Uruchamianie…",
    restart_done: "Gotowe!",
    failed: "Błąd",

    save_failed: "Błąd zapisu",
    load_failed: "Nie udało się wczytać konfiguracji",
    listener_failed: "Błąd nasłuchu myszy",
    autostart_failed: "Błąd autostartu",

    tray_open: "Otwórz ustawienia",
    tray_quit: "Wyjdź",
};

pub static ENGLISH: Strings = Strings {
    subtitle: "Haptic feedback on clicks",
    connecting: "Connecting…",
    connected: "Connected",
    disconnected: "Not connected",
    reconnect: "Reconnect",
    tab_settings: "Settings",
    tab_tester: "Input tester",
    tab_language: "Language",

    card_general: "General",
    enabled: "Haptics enabled",
    enabled_hint: "Global on/off switch",
    only_mx: "MX Master clicks only",
    only_mx_hint: "Other mice and touchpads do not vibrate",
    autostart: "Start with the system",
    autostart_hint: "Starts in the system tray",

    card_buttons: "Buttons",
    button_left: "Left",
    button_right: "Right",
    button_middle: "Middle",
    button_forward: "Side — forward",
    button_back: "Side — back",
    waveform_none: "— none —",
    play_hint: "Play on the mouse",

    card_advanced: "Advanced",
    cooldown: "Time between pulses",
    cooldown_hint: "Clicks faster than this do not trigger another pulse",
    queue: "Queue size",
    queue_hint: "How many clicks may wait while the mouse wakes up (applies after a restart)",

    card_permissions: "System permissions",
    permissions_hint: "If clicks or tests do not work, grant these permissions:",
    accessibility: "Accessibility",
    input_monitoring: "Input Monitoring",

    tester_title: "Input tester",
    tester_hint: "Click any mouse button anywhere on the screen. \
                  If the listener works, the click shows up below.",
    recent_clicks: "Recent clicks",
    waiting_for_click: "Waiting for a click…",
    source_mx: "MX Master",
    source_other: "other device",
    seconds_ago: "s ago",
    clear: "Clear",

    language_title: "Interface language",
    language_hint: "Takes effect immediately and is saved automatically.",
    language_auto: "Automatic (system language)",

    save: "Save",
    cancel: "Cancel",
    restart: "Restart",
    restart_hint: "Restarts the background service (e.g. after changing the queue size)",
    saved_done: "Saved!",
    restarting: "Restarting…",
    restart_done: "Done!",
    failed: "Failed",

    save_failed: "Could not save",
    load_failed: "Could not load the configuration",
    listener_failed: "Mouse listener error",
    autostart_failed: "Autostart error",

    tray_open: "Open settings",
    tray_quit: "Quit",
};

#[cfg(test)]
mod tests {
    use super::Language;

    #[test]
    fn language_codes_round_trip() {
        for language in Language::ALL {
            assert_eq!(Language::parse(language.code()), Some(language));
        }
        assert_eq!(Language::parse("auto"), None);
    }
}
