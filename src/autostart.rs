//! Start at login via the per-user Run registry key (Windows only).

use winreg::enums::*;
use winreg::RegKey;

pub fn set_autostart(enable: bool) -> anyhow::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    let (key, _) = hkcu.create_subkey(path)?;
    
    let app_name = "MX4HapticDaemon";
    
    if enable {
        let exe_path = std::env::current_exe()?;
        let exe_path_str = exe_path.to_str().unwrap_or_default().to_string();
        // Quote the path in case it has spaces; start in the tray at login.
        let value = format!("\"{}\" --minimized", exe_path_str);
        key.set_value(app_name, &value)?;
    } else {
        let _ = key.delete_value(app_name); // Ignore if doesn't exist
    }
    
    Ok(())
}

/// Whether the login Run entry exists (also when created by install.ps1).
pub fn is_enabled() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
        .and_then(|key| key.get_value::<String, _>("MX4HapticDaemon"))
        .is_ok()
}
