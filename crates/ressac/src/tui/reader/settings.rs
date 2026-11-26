//! The terminal's own settings (spec §5, The look setting): the look and whether images show,
//! in `terminal.json` (`Paths::terminal_settings`: next to the database on macOS,
//! `$XDG_CONFIG_HOME/ressac/` on Linux). A missing or unreadable file means the defaults;
//! saving never blocks reading (the caller ignores a failed save, beyond a message). Without a
//! usable home there is no file: the choices last for the session (spec §5.1).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::layout::Look;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalSettings {
    pub look: Look,
    pub images: bool,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        TerminalSettings {
            look: Look::Native,
            images: true,
        }
    }
}

impl TerminalSettings {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Written to a temporary file then renamed, so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, path)
    }

    /// `terminal.json` from the layout: `~/Library/Application Support/Ressac/` (next to the
    /// default database) on macOS, `$XDG_CONFIG_HOME/ressac/` on Linux. `None` without a usable
    /// home (`HOME` unset or relative, reachable with `--db`): never a path relative to the
    /// working directory.
    pub fn default_path() -> Option<PathBuf> {
        ressac_core::paths::Paths::from_env()
            .ok()
            .map(|p| p.terminal_settings())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_means_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let settings = TerminalSettings::load(&dir.path().join("terminal.json"));
        assert_eq!(
            settings,
            TerminalSettings {
                look: Look::Native,
                images: true
            }
        );
    }

    #[test]
    fn an_unreadable_file_means_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("terminal.json");
        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(TerminalSettings::load(&path), TerminalSettings::default());
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep/er/terminal.json");
        let settings = TerminalSettings {
            look: Look::Antiope,
            images: false,
        };
        settings.save(&path).unwrap();
        assert_eq!(TerminalSettings::load(&path), settings);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"look\": \"antiope\""), "{raw}");
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("terminal.json");
        std::fs::write(&path, br#"{"look":"antiope"}"#).unwrap();
        assert_eq!(
            TerminalSettings::load(&path),
            TerminalSettings {
                look: Look::Antiope,
                images: true
            }
        );
    }

    /// `default_path` reads the process environment: the check runs in a child test process
    /// with `HOME` unset, then relative, so no other test sees the change (spec §5.1: no
    /// usable home, never a path relative to the working directory).
    #[test]
    fn without_a_usable_home_there_is_no_settings_file() {
        const CHILD: &str = "RESSAC_TEST_SETTINGS_CHILD";
        const NAME: &str =
            "tui::reader::settings::tests::without_a_usable_home_there_is_no_settings_file";
        if std::env::var_os(CHILD).is_some() {
            assert_eq!(TerminalSettings::default_path(), None);
            return;
        }
        for home in [None, Some("relative/home")] {
            let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
            cmd.args(["--exact", NAME, "--test-threads=1"])
                .env(CHILD, "1");
            for var in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "RESSAC_DB"] {
                cmd.env_remove(var);
            }
            match home {
                None => cmd.env_remove("HOME"),
                Some(home) => cmd.env("HOME", home),
            };
            let out = cmd.output().unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(out.status.success(), "HOME={home:?}: {stdout}");
            assert!(
                stdout.contains("1 passed"),
                "the child ran the check: {stdout}"
            );
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_default_path_is_in_the_config_directory() {
        let path = TerminalSettings::default_path().unwrap();
        assert!(path.ends_with("ressac/terminal.json"), "{path:?}");
        assert_ne!(
            path.parent(),
            ressac_core::store::default_path().parent(),
            "{path:?}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_default_path_sits_next_to_the_database() {
        let path = TerminalSettings::default_path().unwrap();
        assert!(
            path.ends_with("Library/Application Support/Ressac/terminal.json"),
            "{path:?}"
        );
    }
}
