use anyhow::{Context, Result};
use std::process::Command;

/// Otevře URL v defaultním prohlížeči podle OS
pub fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .context("Failed to open browser on macOS")?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .context("Failed to open browser on Linux")?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(&["/C", "start", url])
            .spawn()
            .context("Failed to open browser on Windows")?;
    }

    tracing::info!("Opened browser: {}", url);
    Ok(())
}
