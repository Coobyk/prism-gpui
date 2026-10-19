//! Global Prism settings (`prismlauncher.cfg`).
//!
//! Mirrors: `launcher/settings/INIFile`, `INISettingsObject`, `SettingsObject`,
//! and the key/default registry built in `launcher/Application.cpp`.
//!
//! The on-disk file is a sectioned INI (`[General]`, `[UI]`, …) with keys
//! sorted alphabetically inside each section. Unknown keys and unknown sections
//! round-trip untouched.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Kind of a known setting, used by the settings UI to pick a widget and to
/// validate input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Str,
    Bool,
    Int,
    /// A filesystem path.
    Path,
    /// One of a fixed set of strings.
    Enum(&'static [&'static str]),
    /// A secret (API token / password); rendered masked.
    Secret,
}

/// A known setting: its section, default, kind, and where it shows in the UI.
#[derive(Debug, Clone, Copy)]
pub struct SettingDef {
    pub key: &'static str,
    pub section: &'static str,
    pub default: &'static str,
    pub kind: ValueKind,
    pub label: &'static str,
    /// Tab/page in the settings window.
    pub page: &'static str,
}

/// Registry of settings the launcher knows about, grouped for the UI.
///
/// Kept close to Prism's `Application.cpp` so `prismlauncher.cfg` is
/// interchangeable with the C++ launcher.
pub const SETTINGS: &[SettingDef] = &[
    // ---- Appearance / UI ----
    SettingDef { key: "ApplicationTheme", section: "General", default: "", kind: ValueKind::Enum(&["", "dark", "light", "system"]), label: "Theme", page: "Appearance" },
    SettingDef { key: "IconTheme", section: "General", default: "", kind: ValueKind::Str, label: "Icon theme", page: "Appearance" },
    SettingDef { key: "Language", section: "General", default: "en_US", kind: ValueKind::Str, label: "Language", page: "Appearance" },
    SettingDef { key: "UseSystemLocale", section: "General", default: "false", kind: ValueKind::Bool, label: "Use system locale", page: "Appearance" },
    SettingDef { key: "StatusBarVisible", section: "General", default: "true", kind: ValueKind::Bool, label: "Show status bar", page: "Appearance" },
    SettingDef { key: "MenuBarInsteadOfToolBar", section: "General", default: "false", kind: ValueKind::Bool, label: "Menu bar instead of tool bar", page: "Appearance" },
    SettingDef { key: "ToolbarsLocked", section: "General", default: "false", kind: ValueKind::Bool, label: "Lock toolbars", page: "Appearance" },
    SettingDef { key: "InstSortMode", section: "General", default: "Name", kind: ValueKind::Enum(&["Name", "LastLaunchTime", "CreationDate"]), label: "Sort instances by", page: "Appearance" },
    SettingDef { key: "EditInstanceOnDoubleClick", section: "General", default: "false", kind: ValueKind::Bool, label: "Edit instance on double-click", page: "Appearance" },
    SettingDef { key: "ShowGameTime", section: "General", default: "true", kind: ValueKind::Bool, label: "Show game time", page: "Appearance" },
    SettingDef { key: "ShowGameTimeWithoutDays", section: "General", default: "false", kind: ValueKind::Bool, label: "Show game time without days", page: "Appearance" },
    SettingDef { key: "RecordGameTime", section: "General", default: "true", kind: ValueKind::Bool, label: "Record game time", page: "Appearance" },
    // ---- Java ----
    SettingDef { key: "JavaPath", section: "General", default: "", kind: ValueKind::Path, label: "Java path", page: "Java" },
    SettingDef { key: "JavaDir", section: "General", default: "java", kind: ValueKind::Path, label: "Java directory", page: "Java" },
    SettingDef { key: "AutomaticJavaSwitch", section: "General", default: "true", kind: ValueKind::Bool, label: "Automatic Java switch", page: "Java" },
    SettingDef { key: "AutomaticJavaDownload", section: "General", default: "true", kind: ValueKind::Bool, label: "Automatic Java download", page: "Java" },
    SettingDef { key: "IgnoreJavaCompatibility", section: "General", default: "false", kind: ValueKind::Bool, label: "Ignore Java compatibility", page: "Java" },
    SettingDef { key: "IgnoreJavaWizard", section: "General", default: "false", kind: ValueKind::Bool, label: "Skip Java wizard", page: "Java" },
    SettingDef { key: "JvmArgs", section: "General", default: "", kind: ValueKind::Str, label: "JVM arguments", page: "Java" },
    // ---- Memory ----
    SettingDef { key: "MinMemAlloc", section: "General", default: "512", kind: ValueKind::Int, label: "Minimum memory (MB)", page: "Memory" },
    SettingDef { key: "MaxMemAlloc", section: "General", default: "4096", kind: ValueKind::Int, label: "Maximum memory (MB)", page: "Memory" },
    SettingDef { key: "PermGen", section: "General", default: "128", kind: ValueKind::Int, label: "PermGen (MB)", page: "Memory" },
    SettingDef { key: "LowMemWarning", section: "General", default: "true", kind: ValueKind::Bool, label: "Warn on low memory", page: "Memory" },
    // ---- Console ----
    SettingDef { key: "ShowConsole", section: "General", default: "false", kind: ValueKind::Bool, label: "Show console", page: "Console" },
    SettingDef { key: "AutoCloseConsole", section: "General", default: "false", kind: ValueKind::Bool, label: "Auto-close console", page: "Console" },
    SettingDef { key: "ShowConsoleOnError", section: "General", default: "true", kind: ValueKind::Bool, label: "Show console on error", page: "Console" },
    SettingDef { key: "LogPrePostOutput", section: "General", default: "true", kind: ValueKind::Bool, label: "Log pre/post output", page: "Console" },
    SettingDef { key: "ConsoleFont", section: "General", default: "", kind: ValueKind::Str, label: "Console font", page: "Console" },
    SettingDef { key: "ConsoleFontSize", section: "General", default: "11", kind: ValueKind::Int, label: "Console font size", page: "Console" },
    SettingDef { key: "ConsoleMaxLines", section: "General", default: "100000", kind: ValueKind::Int, label: "Console max lines", page: "Console" },
    SettingDef { key: "ConsoleOverflowStop", section: "General", default: "true", kind: ValueKind::Bool, label: "Stop on console overflow", page: "Console" },
    SettingDef { key: "CloseAfterLaunch", section: "General", default: "false", kind: ValueKind::Bool, label: "Close launcher after launch", page: "Console" },
    // ---- Network / proxy ----
    SettingDef { key: "ProxyType", section: "General", default: "None", kind: ValueKind::Enum(&["None", "Default", "HTTP", "SOCKS5"]), label: "Proxy type", page: "Network" },
    SettingDef { key: "ProxyAddr", section: "General", default: "127.0.0.1", kind: ValueKind::Str, label: "Proxy address", page: "Network" },
    SettingDef { key: "ProxyPort", section: "General", default: "8080", kind: ValueKind::Int, label: "Proxy port", page: "Network" },
    SettingDef { key: "ProxyUser", section: "General", default: "", kind: ValueKind::Str, label: "Proxy user", page: "Network" },
    SettingDef { key: "ProxyPass", section: "General", default: "", kind: ValueKind::Secret, label: "Proxy password", page: "Network" },
    SettingDef { key: "NumberOfConcurrentTasks", section: "General", default: "10", kind: ValueKind::Int, label: "Concurrent tasks", page: "Network" },
    SettingDef { key: "NumberOfConcurrentDownloads", section: "General", default: "6", kind: ValueKind::Int, label: "Concurrent downloads", page: "Network" },
    SettingDef { key: "NumberOfManualRetries", section: "General", default: "1", kind: ValueKind::Int, label: "Manual retries", page: "Network" },
    SettingDef { key: "RequestTimeout", section: "General", default: "60", kind: ValueKind::Int, label: "Request timeout (s)", page: "Network" },
    SettingDef { key: "MetaRefreshOnLaunch", section: "General", default: "true", kind: ValueKind::Bool, label: "Refresh meta on launch", page: "Network" },
    // ---- Minecraft ----
    SettingDef { key: "LaunchMaximized", section: "General", default: "false", kind: ValueKind::Bool, label: "Launch maximized", page: "Minecraft" },
    SettingDef { key: "MinecraftWinWidth", section: "General", default: "854", kind: ValueKind::Int, label: "Window width", page: "Minecraft" },
    SettingDef { key: "MinecraftWinHeight", section: "General", default: "480", kind: ValueKind::Int, label: "Window height", page: "Minecraft" },
    SettingDef { key: "EnableFeralGamemode", section: "General", default: "false", kind: ValueKind::Bool, label: "Feral GameMode", page: "Minecraft" },
    SettingDef { key: "EnableMangoHud", section: "General", default: "false", kind: ValueKind::Bool, label: "MangoHud", page: "Minecraft" },
    SettingDef { key: "UseDiscreteGpu", section: "General", default: "false", kind: ValueKind::Bool, label: "Use discrete GPU", page: "Minecraft" },
    SettingDef { key: "UseZink", section: "General", default: "false", kind: ValueKind::Bool, label: "Use Zink", page: "Minecraft" },
    SettingDef { key: "OnlineFixes", section: "General", default: "false", kind: ValueKind::Bool, label: "Online fixes", page: "Minecraft" },
    SettingDef { key: "UseNativeOpenAL", section: "General", default: "false", kind: ValueKind::Bool, label: "Native OpenAL", page: "Minecraft" },
    SettingDef { key: "UseNativeGLFW", section: "General", default: "false", kind: ValueKind::Bool, label: "Native GLFW", page: "Minecraft" },
    SettingDef { key: "UseNativeSDL", section: "General", default: "false", kind: ValueKind::Bool, label: "Native SDL", page: "Minecraft" },
    SettingDef { key: "ModMetadataDisabled", section: "General", default: "false", kind: ValueKind::Bool, label: "Disable mod metadata", page: "Minecraft" },
    SettingDef { key: "ModDependenciesDisabled", section: "General", default: "false", kind: ValueKind::Bool, label: "Disable mod dependencies", page: "Minecraft" },
    SettingDef { key: "ShowModIncompat", section: "General", default: "false", kind: ValueKind::Bool, label: "Show mod incompatibilities", page: "Minecraft" },
    // ---- APIs / tools ----
    SettingDef { key: "ModrinthToken", section: "General", default: "", kind: ValueKind::Secret, label: "Modrinth token", page: "APIs" },
    SettingDef { key: "CurseForgeAPIKey", section: "General", default: "", kind: ValueKind::Secret, label: "CurseForge API key", page: "APIs" },
    SettingDef { key: "FlameKeyOverride", section: "General", default: "", kind: ValueKind::Secret, label: "CurseForge key override", page: "APIs" },
    SettingDef { key: "MSAClientIDOverride", section: "General", default: "", kind: ValueKind::Str, label: "Microsoft client ID override", page: "APIs" },
    SettingDef { key: "JsonEditor", section: "General", default: "", kind: ValueKind::Path, label: "JSON editor", page: "APIs" },
    SettingDef { key: "MCEditPath", section: "General", default: "", kind: ValueKind::Path, label: "MCEdit path", page: "Tools" },
    SettingDef { key: "JProfilerPath", section: "General", default: "", kind: ValueKind::Path, label: "JProfiler path", page: "Tools" },
    SettingDef { key: "JVisualVMPath", section: "General", default: "", kind: ValueKind::Path, label: "JVisualVM path", page: "Tools" },
    // ---- Custom commands ----
    SettingDef { key: "WrapperCommand", section: "General", default: "", kind: ValueKind::Str, label: "Wrapper command", page: "Custom commands" },
    SettingDef { key: "PreLaunchCommand", section: "General", default: "", kind: ValueKind::Str, label: "Pre-launch command", page: "Custom commands" },
    SettingDef { key: "PostExitCommand", section: "General", default: "", kind: ValueKind::Str, label: "Post-exit command", page: "Custom commands" },
    // ---- Updates ----
    SettingDef { key: "UpdateChannel", section: "General", default: "stable", kind: ValueKind::Enum(&["stable", "beta"]), label: "Update channel", page: "Updates" },
    SettingDef { key: "AutoUpdate", section: "General", default: "false", kind: ValueKind::Bool, label: "Automatically download updates", page: "Updates" },
    SettingDef { key: "CheckUpdateOnLaunch", section: "General", default: "true", kind: ValueKind::Bool, label: "Check for updates on launch", page: "Updates" },
    SettingDef { key: "LastUpdateCheck", section: "General", default: "", kind: ValueKind::Str, label: "Last update check", page: "Updates" },
    SettingDef { key: "SkippedUpdateVersion", section: "General", default: "", kind: ValueKind::Str, label: "Skipped version", page: "Updates" },
    // ---- Paths ----
    SettingDef { key: "InstanceDir", section: "General", default: "instances", kind: ValueKind::Path, label: "Instances directory", page: "Paths" },
    SettingDef { key: "CentralModsDir", section: "General", default: "mods", kind: ValueKind::Path, label: "Central mods directory", page: "Paths" },
    SettingDef { key: "IconsDir", section: "General", default: "icons", kind: ValueKind::Path, label: "Icons directory", page: "Paths" },
    SettingDef { key: "DownloadsDir", section: "General", default: "", kind: ValueKind::Path, label: "Downloads directory", page: "Paths" },
    SettingDef { key: "SkinsDir", section: "General", default: "skins", kind: ValueKind::Path, label: "Skins directory", page: "Paths" },
    SettingDef { key: "DownloadsDirWatchRecursive", section: "General", default: "false", kind: ValueKind::Bool, label: "Watch downloads recursively", page: "Paths" },
    SettingDef { key: "MoveModsFromDownloadsDir", section: "General", default: "false", kind: ValueKind::Bool, label: "Move mods from downloads", page: "Paths" },
];

/// Look up a known setting definition by key.
pub fn setting_def(key: &str) -> Option<&'static SettingDef> {
    SETTINGS.iter().find(|d| d.key == key)
}

/// All distinct settings pages, in registry order.
pub fn pages() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for d in SETTINGS {
        if !out.contains(&d.page) {
            out.push(d.page);
        }
    }
    out
}

/// A sectioned INI document that preserves section order and unknown keys.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IniFile {
    /// Section names in the order they should be written.
    pub order: Vec<String>,
    pub sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl IniFile {
    pub fn load(path: &Path) -> Self {
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Self {
        let mut order: Vec<String> = Vec::new();
        let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut current: Option<String> = None;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }
            if let Some(name) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                let name = name.trim().to_string();
                if !order.contains(&name) {
                    order.push(name.clone());
                }
                sections.entry(name.clone()).or_default();
                current = Some(name);
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                let k = k.trim().to_string();
                let v = v.trim().to_string();
                let section = current.clone().unwrap_or_else(|| "General".to_string());
                if !order.contains(&section) {
                    order.push(section.clone());
                }
                sections.entry(section).or_default().insert(k, v);
            }
        }
        if order.is_empty() {
            order.push("General".to_string());
            sections.entry("General".to_string()).or_default();
        }
        IniFile { order, sections }
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections.get(section)?.get(key).map(|s| s.as_str())
    }

    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        if !self.order.iter().any(|s| s == section) {
            self.order.push(section.to_string());
        }
        self.sections
            .entry(section.to_string())
            .or_default()
            .insert(key.to_string(), value.to_string());
    }

    pub fn remove(&mut self, section: &str, key: &str) {
        if let Some(sec) = self.sections.get_mut(section) {
            sec.remove(key);
        }
    }

    /// Find a key in any section (General first).
    pub fn find(&self, key: &str) -> Option<&str> {
        if let Some(v) = self.get("General", key) {
            return Some(v);
        }
        for s in &self.order {
            if let Some(v) = self.get(s, key) {
                return Some(v);
            }
        }
        None
    }

    /// Section that currently holds `key`, if any.
    pub fn section_of(&self, key: &str) -> Option<String> {
        self.order
            .iter()
            .find(|s| self.sections.get(*s).map(|m| m.contains_key(key)).unwrap_or(false))
            .cloned()
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for (i, name) in self.order.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push('[');
            out.push_str(name);
            out.push_str("]\n");
            if let Some(sec) = self.sections.get(name) {
                for (k, v) in sec {
                    out.push_str(k);
                    out.push('=');
                    out.push_str(v);
                    out.push('\n');
                }
            }
        }
        out
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(path, self.render())
    }
}

/// Global Prism settings bound to a data root.
#[derive(Debug, Clone)]
pub struct GlobalSettings {
    pub data_root: PathBuf,
    pub ini: IniFile,
}

impl GlobalSettings {
    pub fn load(data_root: &Path) -> Self {
        let ini = IniFile::load(&data_root.join("prismlauncher.cfg"));
        let mut s = GlobalSettings {
            data_root: data_root.to_path_buf(),
            ini,
        };
        s.apply_defaults();
        s
    }

    /// Fill in any known setting that is missing, so the file has a stable
    /// shape on first run (mirrors Prism's `registerSetting` defaults).
    pub fn apply_defaults(&mut self) {
        for def in SETTINGS {
            if self.ini.find(def.key).is_none() && !def.default.is_empty() {
                self.ini
                    .set(def.section, def.key, def.default);
            }
        }
    }

    /// Read a raw value by key, searching every section.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.ini.find(key)
    }

    pub fn get_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        match self.get(key) {
            Some(v) if !v.is_empty() => v,
            _ => {
                let def = setting_def(key).map(|d| d.default).unwrap_or("");
                if def.is_empty() {
                    default
                } else {
                    def
                }
            }
        }
    }

    /// Write a value into the section it belongs to (or `General`).
    pub fn set(&mut self, key: &str, value: &str) {
        let section = setting_def(key)
            .map(|d| d.section.to_string())
            .or_else(|| self.ini.section_of(key))
            .unwrap_or_else(|| "General".to_string());
        self.ini.set(&section, key, value);
    }

    pub fn get_bool(&self, key: &str) -> bool {
        matches!(
            self.get_or(key, "false").to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "on"
        )
    }

    pub fn get_int(&self, key: &str) -> i64 {
        self.get_or(key, "0").trim().parse().unwrap_or(0)
    }

    pub fn set_bool(&mut self, key: &str, value: bool) {
        self.set(key, if value { "true" } else { "false" });
    }

    pub fn set_int(&mut self, key: &str, value: i64) {
        self.set(key, &value.to_string());
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.ini.save(&self.data_root.join("prismlauncher.cfg"))
    }

    // ---- Frequently used typed accessors ----

    pub fn theme(&self) -> &str {
        let def = setting_def("ApplicationTheme").map(|d| d.default).unwrap_or("");
        self.get("ApplicationTheme").unwrap_or(def)
    }

    pub fn language(&self) -> &str {
        self.get_or("Language", "en_US")
    }

    pub fn min_mem(&self) -> u32 {
        self.get_int("MinMemAlloc").clamp(0, u32::MAX as i64) as u32
    }

    pub fn max_mem(&self) -> u32 {
        self.get_int("MaxMemAlloc").max(512).min(u32::MAX as i64) as u32
    }

    pub fn java_path(&self) -> &str {
        self.get("JavaPath").unwrap_or("")
    }

    pub fn java_dir(&self) -> PathBuf {
        let raw = self.get("JavaDir").unwrap_or("java");
        let p = PathBuf::from(raw);
        if p.is_absolute() {
            p
        } else {
            self.data_root.join(p)
        }
    }

    pub fn instance_dir(&self) -> PathBuf {
        let raw = self.get("InstanceDir").unwrap_or("instances");
        let p = PathBuf::from(raw);
        if p.is_absolute() {
            p
        } else {
            self.data_root.join(p)
        }
    }

    /// Custom JVM args, split on whitespace (quotes respected crudely).
    pub fn jvm_args(&self) -> Vec<String> {
        self.get("JvmArgs")
            .unwrap_or("")
            .split_whitespace()
            .map(|s| s.to_string())
            .collect()
    }

    pub fn console_max_lines(&self) -> usize {
        self.get_int("ConsoleMaxLines").clamp(100, 10_000_000) as usize
    }

    pub fn request_timeout_secs(&self) -> u64 {
        self.get_int("RequestTimeout").clamp(1, 600) as u64
    }

    pub fn wrapper_command(&self) -> &str {
        self.get("WrapperCommand").unwrap_or("")
    }

    pub fn pre_launch_command(&self) -> &str {
        self.get("PreLaunchCommand").unwrap_or("")
    }

    pub fn post_exit_command(&self) -> &str {
        self.get("PostExitCommand").unwrap_or("")
    }

    /// CurseForge API key: explicit setting, then the `CF_API_KEY` environment
    /// variable (which the upstream launcher also honours).
    pub fn curseforge_api_key(&self) -> Option<String> {
        for key in ["CurseForgeAPIKey", "FlameKeyOverride"] {
            if let Some(v) = self.get(key) {
                if !v.trim().is_empty() {
                    return Some(v.trim().to_string());
                }
            }
        }
        std::env::var("CF_API_KEY")
            .ok()
            .filter(|v| !v.trim().is_empty())
    }

    pub fn modrinth_token(&self) -> Option<String> {
        self.get("ModrinthToken")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Build a proxy URL from the proxy settings, if one is configured.
    ///
    /// `ProxyType` may be `None`, `Default` (use the environment), `HTTP`, or
    /// `SOCKS5`.
    pub fn proxy_url(&self) -> Option<String> {
        let kind = self.get("ProxyType").unwrap_or("None");
        match kind {
            "None" => None,
            "Default" => {
                for key in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"] {
                    if let Ok(v) = std::env::var(key) {
                        if !v.trim().is_empty() {
                            return Some(v);
                        }
                    }
                }
                None
            }
            "HTTP" | "SOCKS5" => {
                let scheme = if kind == "SOCKS5" { "socks5" } else { "http" };
                let addr = self.get("ProxyAddr").unwrap_or("127.0.0.1");
                let port = self.get_int("ProxyPort").max(0);
                let user = self.get("ProxyUser").unwrap_or("");
                let pass = self.get("ProxyPass").unwrap_or("");
                let auth = if user.is_empty() {
                    String::new()
                } else if pass.is_empty() {
                    format!("{user}@")
                } else {
                    format!("{user}:{pass}@")
                };
                Some(format!("{scheme}://{auth}{addr}:{port}"))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_reload() {
        let td = tempfile::tempdir().unwrap();
        let mut s = GlobalSettings::load(td.path());
        s.set("ApplicationTheme", "dark");
        s.set("MaxMemAlloc", "2048");
        s.save().unwrap();
        let s2 = GlobalSettings::load(td.path());
        assert_eq!(s2.theme(), "dark");
        assert_eq!(s2.get("MaxMemAlloc"), Some("2048"));
    }

    #[test]
    fn preserves_unknown_sections_and_keys() {
        let td = tempfile::tempdir().unwrap();
        let cfg = "[General]\nApplicationTheme=dark\n";
        fs::write(td.path().join("prismlauncher.cfg"), cfg).unwrap();

        let mut s = GlobalSettings::load(td.path());
        s.ini.set("UI", "mods_Page\\ColumnsVisibility", "{\"Image\":true}");
        s.save().unwrap();

        let text = fs::read_to_string(td.path().join("prismlauncher.cfg")).unwrap();
        assert!(text.contains("[UI]"), "{text}");
        assert!(text.contains("mods_Page\\ColumnsVisibility={\"Image\":true}"), "{text}");

        let s2 = GlobalSettings::load(td.path());
        assert_eq!(s2.ini.get("UI", "mods_Page\\ColumnsVisibility"), Some("{\"Image\":true}"));
    }

    #[test]
    fn keys_are_sorted_within_section_and_sections_ordered() {
        let ini = IniFile::parse("[General]\nz=1\na=2\n\n[UI]\nq=3\n");
        let rendered = ini.render();
        let a = rendered.find("a=2").unwrap();
        let z = rendered.find("z=1").unwrap();
        assert!(a < z, "keys should be alphabetical: {rendered}");
        assert!(rendered.find("[General]").unwrap() < rendered.find("[UI]").unwrap());
    }

    #[test]
    fn typed_accessors_and_defaults() {
        let td = tempfile::tempdir().unwrap();
        let mut s = GlobalSettings::load(td.path());
        assert_eq!(s.max_mem(), 4096); // registry default
        assert_eq!(s.console_max_lines(), 100_000);
        s.set_bool("ShowConsole", true);
        assert!(s.get_bool("ShowConsole"));
        s.set_int("MinMemAlloc", 1024);
        assert_eq!(s.min_mem(), 1024);
        assert_eq!(s.proxy_url(), None);
    }

    #[test]
    fn proxy_url_from_settings() {
        let td = tempfile::tempdir().unwrap();
        let mut s = GlobalSettings::load(td.path());
        s.set("ProxyType", "HTTP");
        s.set("ProxyAddr", "proxy.local");
        s.set_int("ProxyPort", 3128);
        s.set("ProxyUser", "u");
        s.set("ProxyPass", "p");
        assert_eq!(s.proxy_url().as_deref(), Some("http://u:p@proxy.local:3128"));
        s.set("ProxyType", "SOCKS5");
        s.set("ProxyUser", "");
        s.set("ProxyPass", "");
        assert_eq!(s.proxy_url().as_deref(), Some("socks5://proxy.local:3128"));
    }

    #[test]
    fn instance_dir_is_absolute() {
        let td = tempfile::tempdir().unwrap();
        let s = GlobalSettings::load(td.path());
        assert_eq!(s.instance_dir(), td.path().join("instances"));
    }

    #[test]
    fn pages_are_unique() {
        let p = pages();
        let mut sorted = p.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(p.len(), sorted.len());
        assert!(p.contains(&"Java"));
        assert!(p.contains(&"Network"));
    }
}
