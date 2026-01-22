use anyhow::{Context, Result};
use rand::TryRngCore;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Vrací cestu k application data adresáři dle OS
pub fn get_app_dir() -> Result<PathBuf> {
    let base_dir = dirs::config_dir()
        .context("Failed to resolve config directory")?
        .join("elastic-explorer");

    Ok(base_dir)
}

/// Vrací cestu k data adresáři
pub fn get_data_dir() -> Result<PathBuf> {
    get_app_dir()
}

/// Vrací cestu k SQLite databázi
pub fn get_db_path() -> Result<PathBuf> {
    let db_path = get_data_dir()?.join("elastic-explorer.db");
    Ok(db_path)
}

/// Vrací cestu k šifrovacímu klíči
pub fn get_key_path() -> Result<PathBuf> {
    let key_path = get_app_dir()?.join("db.key");
    Ok(key_path)
}

/// Načte nebo vytvoří šifrovací klíč
pub fn load_or_create_key() -> Result<Vec<u8>> {
    let key_path = get_key_path()?;

    if key_path.exists() {
        let key_hex = fs::read_to_string(&key_path)
            .context("Failed to read encryption key")?;
        let key_bytes = hex::decode(key_hex.trim())
            .context("Failed to decode encryption key")?;
        if key_bytes.len() != 32 {
            return Err(anyhow::anyhow!("Encryption key must be 32 bytes"));
        }
        return Ok(key_bytes);
    }

    let mut key = [0u8; 32];
    let mut rng = rand::rngs::OsRng;
    rng.try_fill_bytes(&mut key)
        .context("Failed to generate encryption key")?;
    let key_hex = hex::encode(key);

    let mut file = fs::File::create(&key_path)
        .context("Failed to create encryption key file")?;
    file.write_all(key_hex.as_bytes())
        .context("Failed to write encryption key")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&key_path, perms)
            .context("Failed to set encryption key permissions")?;
    }

    tracing::info!("Created encryption key: {}", key_path.display());
    Ok(key.to_vec())
}

fn legacy_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|home| PathBuf::from(home).join(".elastic-explorer"))
}

fn move_legacy_dir(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)
            .context("Failed to create target config directory")?;
    }

    for entry in fs::read_dir(src).context("Failed to read legacy config directory")? {
        let entry = entry.context("Failed to read legacy directory entry")?;
        let src_path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(file_name);

        if let Err(err) = fs::rename(&src_path, &dst_path) {
            if src_path.is_dir() {
                fs::create_dir_all(&dst_path).ok();
                move_legacy_dir(&src_path, &dst_path).with_context(|| {
                    format!("Failed to move legacy directory {}", src_path.display())
                })?;
                fs::remove_dir(&src_path).ok();
            } else {
                fs::copy(&src_path, &dst_path)
                    .with_context(|| format!("Failed to copy {}", src_path.display()))?;
                fs::remove_file(&src_path).ok();
            }
            tracing::debug!("Legacy move fallback for {}: {}", src_path.display(), err);
        }
    }

    Ok(())
}

/// Inicializuje adresáře (vytvoří je pokud neexistují)
pub fn init_directories() -> Result<()> {
    if let Some(legacy_dir) = legacy_dir() {
        let app_dir = get_app_dir()?;
        if legacy_dir.exists() && legacy_dir != app_dir {
            move_legacy_dir(&legacy_dir, &app_dir)?;
            if fs::read_dir(&legacy_dir).map(|mut i| i.next().is_none()).unwrap_or(false) {
                fs::remove_dir(&legacy_dir)
                    .context("Failed to remove legacy config directory")?;
            }
        }
    }

    let data_dir = get_data_dir()?;

    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)
            .context("Failed to create data directory")?;
        tracing::info!("Created data directory: {}", data_dir.display());
    }

    let _ = load_or_create_key()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_dir_path() {
        let app_dir = get_app_dir().unwrap();
        assert!(app_dir.to_string_lossy().contains("elastic-explorer"));
    }

    #[test]
    fn test_data_dir_path() {
        let data_dir = get_data_dir().unwrap();
        assert!(data_dir.to_string_lossy().contains("elastic-explorer"));
    }
}
