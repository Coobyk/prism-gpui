//! Desktop integration. Mirrors launcher/DesktopServices.

use std::path::Path;

/// Open a file or folder in the system file manager / default app.
pub fn open_path(path: &Path) -> Result<(), String> {
    let path_str = path.to_string_lossy().to_string();
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("explorer").arg(&path_str).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(&path_str).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(&path_str).spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Open a URL in the default browser.
pub fn open_url(url: &str) -> Result<(), String> {
    open_path(Path::new(url))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn open_missing_program_is_err_not_panic() {
        // Passing a bogus path should not panic; result may be Ok(spawn) or Err.
        let _ = open_path(Path::new("/nonexistent/xyz"));
    }
}
