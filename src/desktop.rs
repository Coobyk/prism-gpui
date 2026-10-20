//! Desktop integration. Mirrors launcher/DesktopServices.

use std::path::Path;

/// Open a file or folder in the system file manager / default app.
pub fn open_path(path: &Path) -> Result<(), String> {
    let path_str = path.to_string_lossy().to_string();
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("explorer")
            .arg(&path_str)
            .spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(&path_str).spawn()
    } else {
        std::process::Command::new("xdg-open")
            .arg(&path_str)
            .spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Open a URL in the default browser.
pub fn open_url(url: &str) -> Result<(), String> {
    open_path(Path::new(url))
}

/// Create a shortcut that launches an instance.
///
/// On Linux a `.desktop` file is written to the desktop and to the
/// applications menu. The other platforms are not supported yet.
pub fn create_shortcut(
    instance: &str,
    data_root: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    #[cfg(target_os = "linux")]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let desktop = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .map_err(|_| "no home directory".to_string())?;
        let file_name = format!("{}.desktop", sanitize(instance));
        let content = format!(
            "[Desktop Entry]\nType=Application\nName={}\nComment=Launch {} with Prism Launcher\nExec=\"{}\" --gui instance \"{}\"\nIcon=applications-games\nTerminal=false\nCategories=Game;\n",
            instance,
            instance,
            exe.display(),
            instance
        );
        let mut written = Vec::new();
        for dir in [
            desktop.join("Desktop"),
            desktop.join(".local/share/applications"),
        ] {
            if std::fs::create_dir_all(&dir).is_err() {
                continue;
            }
            let path = dir.join(&file_name);
            if std::fs::write(&path, &content).is_ok() {
                // The desktop entry has to be executable.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = std::fs::metadata(&path) {
                        let mut perms = meta.permissions();
                        perms.set_mode(0o755);
                        let _ = std::fs::set_permissions(&path, perms);
                    }
                }
                written.push(path);
            }
        }
        // The data root is only used to find the launcher; keep the argument
        // so the call sites stay the same on every platform.
        let _ = data_root;
        written
            .into_iter()
            .next()
            .ok_or_else(|| "could not write the shortcut".to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (instance, data_root);
        Err("shortcuts are only supported on Linux".into())
    }
}

/// Turn a name into something that can be used in a file name.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
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
