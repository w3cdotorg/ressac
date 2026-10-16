//! The terminal's own settings (spec §5, The look setting): the look and whether images show,
//! in `terminal.json` next to the database. A missing or unreadable file means the defaults;
//! saving never blocks reading (the caller ignores a failed save, beyond a message).

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

    /// `~/Library/Application Support/Ressac/terminal.json`, next to the default database.
    pub fn default_path() -> PathBuf {
        ressac_core::store::default_path().with_file_name("terminal.json")
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

    #[test]
    fn the_default_path_sits_next_to_the_database() {
        let path = TerminalSettings::default_path();
        assert!(
            path.ends_with("Library/Application Support/Ressac/terminal.json"),
            "{path:?}"
        );
    }
}
