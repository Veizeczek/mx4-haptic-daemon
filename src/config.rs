use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Deserialize;
use thiserror::Error;

use crate::i18n::Language;

const DEFAULT_CONFIG: &str = "# MX Master 4 haptic feedback configuration\n\
enabled = true\n\
device = \"MX Master 4\"\n\
only_mx_master = true\n\
# \"auto\" follows the system language; or \"en\" / \"pl\"\n\
language = \"auto\"\n\
\n\
left = \"KNOCK\"\n\
right = \"SUBTLE COLLISION\"\n\
middle = \"\"\n\
\n\
cooldown_ms = 30\n\
queue_size = 4\n\
autostart = false\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub enabled: bool,
    pub device: String,
    /// Ignore clicks from other mice/touchpads where the OS can tell them
    /// apart (Windows). On macOS clicks carry no device and always count.
    pub only_mx_master: bool,
    /// UI language; `None` follows the system language.
    pub language: Option<Language>,
    pub left: Option<Waveform>,
    pub right: Option<Waveform>,
    pub middle: Option<Waveform>,
    pub forward: Option<Waveform>,
    pub back: Option<Waveform>,
    pub cooldown_ms: u64,
    pub queue_size: usize,
    pub autostart: bool,
}

impl Config {
    /// The language the UI should use right now.
    pub fn ui_language(&self) -> Language {
        self.language.unwrap_or_else(Language::system)
    }

    pub fn waveform_for(&self, button: MouseButton) -> Option<Waveform> {
        match button {
            MouseButton::Left => self.left,
            MouseButton::Right => self.right,
            MouseButton::Middle => self.middle,
            MouseButton::Forward => self.forward,
            MouseButton::Back => self.back,
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        let left_str = self.left.map_or("".to_owned(), |w| w.to_string());
        let right_str = self.right.map_or("".to_owned(), |w| w.to_string());
        let middle_str = self.middle.map_or("".to_owned(), |w| w.to_string());
        let forward_str = self.forward.map_or("".to_owned(), |w| w.to_string());
        let back_str = self.back.map_or("".to_owned(), |w| w.to_string());
        let content = format!(
            "# MX Master 4 haptic feedback configuration\n\
enabled = {}\n\
device = \"{}\"\n\
only_mx_master = {}\n\
# \"auto\" follows the system language; or \"en\" / \"pl\"\n\
language = \"{}\"\n\
\n\
left = \"{}\"\n\
right = \"{}\"\n\
middle = \"{}\"\n\
forward = \"{}\"\n\
back = \"{}\"\n\
\n\
cooldown_ms = {}\n\
queue_size = {}\n\
autostart = {}\n",
            self.enabled,
            self.device,
            self.only_mx_master,
            self.language.map_or("auto", Language::code),
            left_str,
            right_str,
            middle_str,
            forward_str,
            back_str,
            self.cooldown_ms,
            self.queue_size,
            self.autostart
        );
        fs::write(&path, content).with_context(|| format!("could not write {}", path.display()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Forward,
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Waveform {
    SharpStateChange = 0x00,
    DampStateChange = 0x01,
    SharpCollision = 0x02,
    DampCollision = 0x03,
    SubtleCollision = 0x04,
    HappyAlert = 0x05,
    AngryAlert = 0x06,
    Completed = 0x07,
    Square = 0x08,
    Wave = 0x09,
    Firework = 0x0A,
    Mad = 0x0B,
    Knock = 0x0C,
    Jingle = 0x0D,
    Ringing = 0x0E,
    WhisperCollision = 0x1B,
}

impl Waveform {
    pub fn parse(value: &str) -> Result<Self, ConfigError> {
        let normalized = value.trim().to_ascii_uppercase().replace('_', " ");
        let waveform = match normalized.as_str() {
            "SHARP STATE CHANGE" => Self::SharpStateChange,
            "DAMP STATE CHANGE" => Self::DampStateChange,
            "SHARP COLLISION" => Self::SharpCollision,
            "DAMP COLLISION" => Self::DampCollision,
            "SUBTLE COLLISION" => Self::SubtleCollision,
            "HAPPY ALERT" => Self::HappyAlert,
            "ANGRY ALERT" => Self::AngryAlert,
            "COMPLETED" => Self::Completed,
            "SQUARE" => Self::Square,
            "WAVE" => Self::Wave,
            "FIREWORK" => Self::Firework,
            "MAD" => Self::Mad,
            "KNOCK" => Self::Knock,
            "JINGLE" => Self::Jingle,
            "RINGING" => Self::Ringing,
            "WHISPER COLLISION" => Self::WhisperCollision,
            _ => return Err(ConfigError::InvalidWaveform(value.to_owned())),
        };
        Ok(waveform)
    }

    pub const fn id(self) -> u8 {
        self as u8
    }

    pub const ALL: &'static [Waveform] = &[
        Waveform::SharpStateChange,
        Waveform::DampStateChange,
        Waveform::SharpCollision,
        Waveform::DampCollision,
        Waveform::SubtleCollision,
        Waveform::HappyAlert,
        Waveform::AngryAlert,
        Waveform::Completed,
        Waveform::Square,
        Waveform::Wave,
        Waveform::Firework,
        Waveform::Mad,
        Waveform::Knock,
        Waveform::Jingle,
        Waveform::Ringing,
        Waveform::WhisperCollision,
    ];

    pub fn is_advertised_by(self, supported_mask: u32) -> bool {
        supported_mask & (1_u32 << self.id()) != 0
    }
}

impl fmt::Display for Waveform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::SharpStateChange => "SHARP STATE CHANGE",
            Self::DampStateChange => "DAMP STATE CHANGE",
            Self::SharpCollision => "SHARP COLLISION",
            Self::DampCollision => "DAMP COLLISION",
            Self::SubtleCollision => "SUBTLE COLLISION",
            Self::HappyAlert => "HAPPY ALERT",
            Self::AngryAlert => "ANGRY ALERT",
            Self::Completed => "COMPLETED",
            Self::Square => "SQUARE",
            Self::Wave => "WAVE",
            Self::Firework => "FIREWORK",
            Self::Mad => "MAD",
            Self::Knock => "KNOCK",
            Self::Jingle => "JINGLE",
            Self::Ringing => "RINGING",
            Self::WhisperCollision => "WHISPER COLLISION",
        };
        f.write_str(name)
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("unsupported waveform {0:?}; use one of the documented MX Master 4 waveforms")]
    InvalidWaveform(String),
    #[error("unsupported language {0:?}; use \"auto\", \"en\" or \"pl\"")]
    InvalidLanguage(String),
    #[error("{field} must be between {min} and {max}, got {value}")]
    OutOfRange {
        field: &'static str,
        min: u64,
        max: u64,
        value: u64,
    },
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default = "default_device")]
    device: String,
    #[serde(default = "default_enabled")]
    only_mx_master: bool,
    #[serde(default)]
    language: String,
    #[serde(default)]
    left: String,
    #[serde(default)]
    right: String,
    #[serde(default)]
    middle: String,
    #[serde(default)]
    forward: String,
    #[serde(default)]
    back: String,
    #[serde(default = "default_cooldown_ms")]
    cooldown_ms: u64,
    #[serde(default = "default_queue_size")]
    queue_size: usize,
    #[serde(default = "default_autostart")]
    autostart: bool,
}

fn default_enabled() -> bool {
    true
}
fn default_device() -> String {
    "MX Master 4".to_owned()
}
fn default_cooldown_ms() -> u64 {
    30
}
fn default_queue_size() -> usize {
    4
}
fn default_autostart() -> bool {
    false
}

impl TryFrom<RawConfig> for Config {
    type Error = ConfigError;

    fn try_from(raw: RawConfig) -> Result<Self, Self::Error> {
        if raw.cooldown_ms > 5_000 {
            return Err(ConfigError::OutOfRange {
                field: "cooldown_ms",
                min: 0,
                max: 5_000,
                value: raw.cooldown_ms,
            });
        }
        if raw.queue_size == 0 || raw.queue_size > 64 {
            return Err(ConfigError::OutOfRange {
                field: "queue_size",
                min: 1,
                max: 64,
                value: raw.queue_size as u64,
            });
        }

        Ok(Self {
            enabled: raw.enabled,
            device: raw.device,
            only_mx_master: raw.only_mx_master,
            language: parse_language(&raw.language)?,
            left: parse_optional_waveform(&raw.left)?,
            right: parse_optional_waveform(&raw.right)?,
            middle: parse_optional_waveform(&raw.middle)?,
            forward: parse_optional_waveform(&raw.forward)?,
            back: parse_optional_waveform(&raw.back)?,
            cooldown_ms: raw.cooldown_ms,
            queue_size: raw.queue_size,
            autostart: raw.autostart,
        })
    }
}

fn parse_language(value: &str) -> Result<Option<Language>, ConfigError> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("auto") {
        Ok(None)
    } else {
        Language::parse(value)
            .map(Some)
            .ok_or_else(|| ConfigError::InvalidLanguage(value.to_owned()))
    }
}

fn parse_optional_waveform(value: &str) -> Result<Option<Waveform>, ConfigError> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        Waveform::parse(value).map(Some)
    }
}

pub fn config_path() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").context("HOME is not set; cannot locate Application Support")?;
        Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("MX4Haptic")
            .join("config.toml"))
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not set")?;
        Ok(PathBuf::from(appdata).join("MX4Haptic").join("config.toml"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        anyhow::bail!("unsupported operating system for config path")
    }
}

pub fn load_or_create() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        write_default(&path)?;
    }
    load_from_path(&path)
}

fn write_default(path: &Path) -> Result<()> {
    let parent = path.parent().context("config path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("could not create {}", parent.display()))?;
    fs::write(path, DEFAULT_CONFIG)
        .with_context(|| format!("could not write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("could not set safe permissions on {}", path.display()))?;
    }
    Ok(())
}

pub fn load_from_path(path: &Path) -> Result<Config> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    let raw: RawConfig =
        toml::from_str(&contents).with_context(|| format!("invalid TOML in {}", path.display()))?;
    Config::try_from(raw).with_context(|| format!("invalid configuration in {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{Config, ConfigError, MouseButton, RawConfig, Waveform};

    #[test]
    fn parses_documented_waveforms_only() {
        assert_eq!(
            Waveform::parse("whisper collision").expect("documented waveform"),
            Waveform::WhisperCollision
        );
        assert!(matches!(
            Waveform::parse("BUZZ"),
            Err(ConfigError::InvalidWaveform(_))
        ));
    }

    #[test]
    fn parses_button_mapping_and_empty_value() {
        let config = Config::try_from(RawConfig {
            enabled: true,
            device: "MX Master 4".to_owned(),
            only_mx_master: true,
            language: String::new(),
            left: "KNOCK".to_owned(),
            right: "SUBTLE COLLISION".to_owned(),
            middle: String::new(),
            forward: String::new(),
            back: String::new(),
            cooldown_ms: 30,
            queue_size: 4,
            autostart: false,
        })
        .expect("valid config");

        assert_eq!(
            config.waveform_for(MouseButton::Left),
            Some(Waveform::Knock)
        );
        assert_eq!(
            config.waveform_for(MouseButton::Right),
            Some(Waveform::SubtleCollision)
        );
        assert_eq!(config.waveform_for(MouseButton::Middle), None);
    }

    #[test]
    fn rejects_unbounded_queue() {
        let result = Config::try_from(RawConfig {
            enabled: true,
            device: "MX Master 4".to_owned(),
            only_mx_master: true,
            language: String::new(),
            left: String::new(),
            right: String::new(),
            middle: String::new(),
            forward: String::new(),
            back: String::new(),
            cooldown_ms: 30,
            queue_size: 0,
            autostart: false,
        });
        assert!(matches!(
            result,
            Err(ConfigError::OutOfRange {
                field: "queue_size",
                ..
            })
        ));
    }
}
