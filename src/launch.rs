//! Launch controller skeleton. Mirrors launcher/LaunchController, launch/*.
//! Full implementation requires component updates, asset download, auth.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Default)]
pub struct LaunchProfile {
    pub java_path: PathBuf,
    pub min_mem_mb: u32,
    pub max_mem_mb: u32,
    pub game_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub main_class: String,
    pub classpath: Vec<PathBuf>,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
}

impl LaunchProfile {
    pub fn jvm_command(&self) -> Command {
        let mut cmd = Command::new(&self.java_path);
        cmd.arg(format!("-Xms{}M", self.min_mem_mb));
        cmd.arg(format!("-Xmx{}M", self.max_mem_mb));
        for a in &self.jvm_args {
            cmd.arg(a);
        }
        let cp = self
            .classpath
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(":");
        cmd.arg("-cp").arg(cp);
        cmd.arg(&self.main_class);
        cmd.args(&self.game_args);
        cmd.current_dir(&self.game_dir);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd
    }
}

/// Build a minimal launch profile for a vanilla instance (placeholder).
pub fn build_profile(inst_dir: &Path, java_path: &Path) -> LaunchProfile {
    LaunchProfile {
        java_path: java_path.to_path_buf(),
        min_mem_mb: 512,
        max_mem_mb: 4096,
        game_dir: inst_dir.join("minecraft"),
        assets_dir: inst_dir.join("minecraft").join("assets"),
        main_class: "net.minecraft.client.main.Main".to_string(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builds_command() {
        let p = LaunchProfile {
            java_path: "/usr/bin/java".into(),
            min_mem_mb: 512,
            max_mem_mb: 1024,
            main_class: "Main".into(),
            ..Default::default()
        };
        let cmd = p.jvm_command();
        let dbg = format!("{:?}", cmd);
        assert!(dbg.contains("java"));
    }
}
