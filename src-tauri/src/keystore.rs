//! Database key storage in the OS keychain (NFR-BIZ-01):
//! Windows Credential Manager, macOS Keychain. Linux (development and CI only)
//! falls back to a key file next to the database when no keyring is available.

use std::path::Path;

use lunda_core::CoreError;

const SERVICE: &str = "hu.lunda.temeto";
const ACCOUNT: &str = "database-key";

fn entry() -> Result<keyring::Entry, CoreError> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| CoreError::new("keychain", e.to_string()))
}

fn fallback_allowed() -> bool {
    cfg!(target_os = "linux") || std::env::var("LUNDA_KEY_FILE").is_ok()
}

fn key_file(dir: &Path) -> std::path::PathBuf {
    dir.join(".dev-db-key")
}

pub fn get_key(dir: &Path) -> Result<Option<String>, CoreError> {
    if fallback_allowed() && key_file(dir).exists() {
        return Ok(Some(std::fs::read_to_string(key_file(dir))?.trim().to_string()));
    }
    match entry()?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) if fallback_allowed() => {
            log::warn!("keychain unavailable ({e}); using development key file");
            Ok(None)
        }
        Err(e) => Err(CoreError::new("keychain", e.to_string())),
    }
}

pub fn set_key(dir: &Path, key: &str) -> Result<(), CoreError> {
    if fallback_allowed() {
        // Linux is not a release target: keyutils keys do not survive a reboot,
        // so development builds keep the key in a private file.
        std::fs::write(key_file(dir), key)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(key_file(dir), std::fs::Permissions::from_mode(0o600))?;
        }
        return Ok(());
    }
    entry()?.set_password(key).map_err(|e| CoreError::new("keychain", e.to_string()))
}
