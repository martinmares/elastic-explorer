use anyhow::{Context, Result};
use std::path::PathBuf;

/// Vrací cestu k application data adresáři dle OS
pub fn get_app_dir() -> Result<PathBuf> {
    let base_dir = if cfg!(target_os = "windows") {
        // Windows: %APPDATA%\elastic-explorer
        std::env::var("APPDATA")
            .context("APPDATA environment variable not found")?
            .into()
    } else {
        // Linux/macOS: ~/.elastic-explorer
        let home = std::env::var("HOME")
            .context("HOME environment variable not found")?;
        PathBuf::from(home).join(".elastic-explorer")
    };

    Ok(base_dir)
}

/// Vrací cestu k data adresáři
pub fn get_data_dir() -> Result<PathBuf> {
    let data_dir = get_app_dir()?.join("data");
    Ok(data_dir)
}

/// Vrací cestu k SQLite databázi
pub fn get_db_path() -> Result<PathBuf> {
    let db_path = get_data_dir()?.join("elastic-explorer.db");
    Ok(db_path)
}

/// Inicializuje adresáře (vytvoří je pokud neexistují)
pub fn init_directories() -> Result<()> {
    let data_dir = get_data_dir()?;

    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)
            .context("Failed to create data directory")?;
        tracing::info!("Created data directory: {}", data_dir.display());
    }

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
        assert!(data_dir.to_string_lossy().contains("data"));
    }
}
