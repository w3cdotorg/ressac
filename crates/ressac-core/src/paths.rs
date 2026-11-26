//! Where Ressac keeps its files, in one place.
//!
//! The macOS layout is frozen: the Swift app's `Database.swift` hard-codes the same database
//! path, so those locations never change. On every other target the XDG base directories are
//! used. [`Paths::resolve`] is pure (the environment is injected), so both layouts are tested on
//! any host, apart from the file checks in [`Paths::ticker_state`] and
//! [`Paths::legacy_macos_database`].

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Which directory convention to follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// `~/Library/...`, shared with the Swift app.
    MacOs,
    /// XDG base directories.
    Xdg,
}

impl Layout {
    /// The layout of the platform this binary was built for.
    pub fn current() -> Layout {
        if cfg!(target_os = "macos") {
            Layout::MacOs
        } else {
            Layout::Xdg
        }
    }
}

/// Why the locations could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathsError {
    /// `HOME` is unset, empty or not an absolute path.
    #[error("HOME is not set to an absolute path")]
    NoHome,
}

/// Resolved file locations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    home: PathBuf,
    layout: Layout,
    data: PathBuf,
    config: PathBuf,
    state: PathBuf,
    log: PathBuf,
    cache: PathBuf,
}

/// An XDG variable's value, only when it is non-empty and absolute; otherwise `fallback`.
fn xdg(env: &dyn Fn(&str) -> Option<OsString>, var: &str, fallback: PathBuf) -> PathBuf {
    match env(var) {
        Some(value) if !value.is_empty() && Path::new(&value).is_absolute() => {
            PathBuf::from(value).join("ressac")
        }
        _ => fallback,
    }
}

impl Paths {
    /// Resolve from the process environment on the current platform.
    pub fn from_env() -> Result<Paths, PathsError> {
        Paths::resolve(Layout::current(), &|key| std::env::var_os(key))
    }

    /// Resolve `layout` using `env` to read variables.
    pub fn resolve(
        layout: Layout,
        env: &dyn Fn(&str) -> Option<OsString>,
    ) -> Result<Paths, PathsError> {
        let home = env("HOME")
            .map(PathBuf::from)
            .filter(|h| h.is_absolute())
            .ok_or(PathsError::NoHome)?;
        let paths = match layout {
            Layout::MacOs => {
                let data = home.join("Library/Application Support/Ressac");
                Paths {
                    layout,
                    config: data.clone(),
                    state: data.clone(),
                    log: home.join("Library/Logs/Ressac"),
                    cache: home.join("Library/Caches/Ressac"),
                    data,
                    home,
                }
            }
            Layout::Xdg => {
                let state = xdg(env, "XDG_STATE_HOME", home.join(".local/state/ressac"));
                Paths {
                    layout,
                    data: xdg(env, "XDG_DATA_HOME", home.join(".local/share/ressac")),
                    config: xdg(env, "XDG_CONFIG_HOME", home.join(".config/ressac")),
                    log: state.join("logs"),
                    state,
                    cache: xdg(env, "XDG_CACHE_HOME", home.join(".cache/ressac")),
                    home,
                }
            }
        };
        Ok(paths)
    }

    /// The default SQLite database.
    pub fn database(&self) -> PathBuf {
        self.data.join("ressac.sqlite")
    }

    /// The terminal reader's settings file.
    pub fn terminal_settings(&self) -> PathBuf {
        self.config.join("terminal.json")
    }

    /// The ticker state file for `db`: `ticker.json` for the default database (under any
    /// spelling), a hash-named file for any other.
    pub fn ticker_state(&self, db: &Path) -> PathBuf {
        let db = canonical(db);
        if db == canonical(&self.database()) {
            self.state.join("ticker.json")
        } else {
            let hash = fnv1a64(db.as_os_str().as_encoded_bytes());
            self.state.join(format!("ticker-{hash:016x}.json"))
        }
    }

    /// Where log files go.
    pub fn log_dir(&self) -> PathBuf {
        self.log.clone()
    }

    /// Where disposable cached files go.
    pub fn cache_dir(&self) -> PathBuf {
        self.cache.clone()
    }

    /// The macOS-layout database, when this is the XDG layout and that file exists (a copy
    /// brought over from a Mac).
    pub fn legacy_macos_database(&self) -> Option<PathBuf> {
        if self.layout != Layout::Xdg {
            return None;
        }
        let legacy = self
            .home
            .join("Library/Application Support/Ressac/ressac.sqlite");
        legacy.exists().then_some(legacy)
    }
}

/// Resolve symlinks and `..`; for a file that does not exist yet, canonicalise its parent (an
/// empty parent, as in a bare `x.sqlite`, means the current directory); otherwise keep the path
/// as given.
fn canonical(p: &Path) -> PathBuf {
    if let Ok(real) = p.canonicalize() {
        return real;
    }
    let parent = match p.parent() {
        Some(d) if d.as_os_str().is_empty() => Some(Path::new(".")),
        other => other,
    };
    match (parent.and_then(|d| d.canonicalize().ok()), p.file_name()) {
        (Some(dir), Some(name)) => dir.join(name),
        _ => p.to_path_buf(),
    }
}

/// 64-bit FNV-1a.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    const VARS: [&str; 4] = [
        "XDG_DATA_HOME",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
    ];

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let pairs: Vec<(String, OsString)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |key| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    fn resolve(layout: Layout, pairs: &[(&str, &str)]) -> Result<Paths, PathsError> {
        Paths::resolve(layout, &env(pairs))
    }

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn layout_current_matches_the_build_target() {
        let expected = if cfg!(target_os = "macos") {
            Layout::MacOs
        } else {
            Layout::Xdg
        };
        assert_eq!(Layout::current(), expected);
    }

    #[test]
    fn macos_layout_matches_the_frozen_paths() {
        // The Swift app hard-codes the same database path: these strings never change.
        let paths = resolve(
            Layout::MacOs,
            &[
                ("HOME", "/Users/clement"),
                ("XDG_DATA_HOME", "/x/data"),
                ("XDG_CONFIG_HOME", "/x/config"),
                ("XDG_STATE_HOME", "/x/state"),
                ("XDG_CACHE_HOME", "/x/cache"),
            ],
        )
        .unwrap();
        let dir = "/Users/clement/Library/Application Support/Ressac";
        assert_eq!(paths.database(), p(&format!("{dir}/ressac.sqlite")));
        assert_eq!(
            paths.terminal_settings(),
            p(&format!("{dir}/terminal.json"))
        );
        assert_eq!(
            paths.ticker_state(&paths.database()),
            p(&format!("{dir}/ticker.json"))
        );
        assert_eq!(paths.log_dir(), p("/Users/clement/Library/Logs/Ressac"));
        assert_eq!(paths.cache_dir(), p("/Users/clement/Library/Caches/Ressac"));
        assert_eq!(paths.legacy_macos_database(), None);
    }

    #[test]
    fn xdg_layout_defaults_under_home() {
        let paths = resolve(Layout::Xdg, &[("HOME", "/home/clement")]).unwrap();
        assert_eq!(
            paths.database(),
            p("/home/clement/.local/share/ressac/ressac.sqlite")
        );
        assert_eq!(
            paths.terminal_settings(),
            p("/home/clement/.config/ressac/terminal.json")
        );
        assert_eq!(
            paths.ticker_state(&paths.database()),
            p("/home/clement/.local/state/ressac/ticker.json")
        );
        assert_eq!(paths.log_dir(), p("/home/clement/.local/state/ressac/logs"));
        assert_eq!(paths.cache_dir(), p("/home/clement/.cache/ressac"));
    }

    #[test]
    fn xdg_layout_follows_absolute_xdg_variables() {
        let paths = resolve(
            Layout::Xdg,
            &[
                ("HOME", "/home/clement"),
                ("XDG_DATA_HOME", "/x/data"),
                ("XDG_CONFIG_HOME", "/x/config"),
                ("XDG_STATE_HOME", "/x/state"),
                ("XDG_CACHE_HOME", "/x/cache"),
            ],
        )
        .unwrap();
        assert_eq!(paths.database(), p("/x/data/ressac/ressac.sqlite"));
        assert_eq!(
            paths.terminal_settings(),
            p("/x/config/ressac/terminal.json")
        );
        assert_eq!(
            paths.ticker_state(&paths.database()),
            p("/x/state/ressac/ticker.json")
        );
        assert_eq!(paths.log_dir(), p("/x/state/ressac/logs"));
        assert_eq!(paths.cache_dir(), p("/x/cache/ressac"));
    }

    #[test]
    fn empty_or_relative_xdg_values_are_ignored() {
        // Review Focus 1: some shells export XDG_DATA_HOME="" or a relative value.
        let defaults = resolve(Layout::Xdg, &[("HOME", "/home/clement")]).unwrap();
        for var in VARS {
            for value in ["", "rel/dir", "./rel"] {
                let paths =
                    resolve(Layout::Xdg, &[("HOME", "/home/clement"), (var, value)]).unwrap();
                assert_eq!(paths, defaults, "{var}={value:?}");
                for path in [
                    paths.database(),
                    paths.terminal_settings(),
                    paths.log_dir(),
                    paths.cache_dir(),
                ] {
                    assert!(path.is_absolute(), "{var}={value:?} gave {path:?}");
                }
            }
        }
    }

    #[test]
    fn no_usable_home_is_an_error_on_both_layouts() {
        let all_xdg: Vec<(&str, &str)> = VARS.iter().map(|v| (*v, "/x")).collect();
        for layout in [Layout::MacOs, Layout::Xdg] {
            assert_eq!(
                resolve(layout, &all_xdg),
                Err(PathsError::NoHome),
                "{layout:?}, no HOME"
            );
            for home in ["", "relative/home"] {
                let mut pairs = all_xdg.clone();
                pairs.push(("HOME", home));
                assert_eq!(
                    resolve(layout, &pairs),
                    Err(PathsError::NoHome),
                    "{layout:?}, HOME={home:?}"
                );
            }
        }
    }

    #[test]
    fn fnv1a64_matches_the_reference_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn another_database_gets_a_hashed_file_in_the_state_directory() {
        for layout in [Layout::MacOs, Layout::Xdg] {
            let paths = resolve(layout, &[("HOME", "/home/clement")]).unwrap();
            let state = paths.ticker_state(&paths.database());
            let other = paths.ticker_state(Path::new("/nowhere/r.sqlite"));
            assert_eq!(other.parent(), state.parent(), "{layout:?}");
            assert_eq!(
                other.file_name().unwrap().to_str().unwrap(),
                format!("ticker-{:016x}.json", fnv1a64(b"/nowhere/r.sqlite"))
            );
            assert_ne!(paths.ticker_state(Path::new("/nowhere/s.sqlite")), other);
        }
    }

    #[cfg(unix)]
    #[test]
    fn one_database_named_several_ways_has_one_ticker_state() {
        // Review Focus 2, resolver side (A2 adds the CLI side with a `./` spelling).
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::resolve(Layout::Xdg, &{
            let h = home.path().as_os_str().to_owned();
            move |k: &str| (k == "HOME").then(|| h.clone())
        })
        .unwrap();
        let dir = home.path().join("dbs");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("r.sqlite"), b"").unwrap();
        std::os::unix::fs::symlink(dir.join("r.sqlite"), dir.join("link.sqlite")).unwrap();
        let one = paths.ticker_state(&dir.join("r.sqlite"));
        assert_eq!(paths.ticker_state(&dir.join("sub/../r.sqlite")), one);
        assert_eq!(paths.ticker_state(&dir.join("link.sqlite")), one);
        assert!(
            one.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("ticker-")
        );

        // The default database stays `ticker.json`, also through a symlink.
        let default = paths.database();
        std::fs::create_dir_all(default.parent().unwrap()).unwrap();
        std::fs::write(&default, b"").unwrap();
        std::os::unix::fs::symlink(&default, dir.join("default-link.sqlite")).unwrap();
        let ticker_json = paths.ticker_state(&default);
        assert_eq!(ticker_json.file_name().unwrap(), "ticker.json");
        assert_eq!(
            paths.ticker_state(&dir.join("default-link.sqlite")),
            ticker_json
        );
    }

    #[test]
    fn the_legacy_mac_database_is_reported_only_on_xdg_and_only_when_it_exists() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path().as_os_str().to_owned();
        let env = move |k: &str| (k == "HOME").then(|| h.clone());
        let xdg = Paths::resolve(Layout::Xdg, &env).unwrap();
        let mac = Paths::resolve(Layout::MacOs, &env).unwrap();
        assert_eq!(xdg.legacy_macos_database(), None);
        let legacy = home
            .path()
            .join("Library/Application Support/Ressac/ressac.sqlite");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, b"").unwrap();
        assert_eq!(xdg.legacy_macos_database(), Some(legacy));
        assert_eq!(mac.legacy_macos_database(), None);
    }

    #[test]
    fn a_not_yet_created_database_has_one_ticker_state_relative_or_absolute() {
        let paths = resolve(Layout::Xdg, &[("HOME", "/home/clement")]).unwrap();
        let name = "ressac-paths-test-not-created.sqlite";
        assert!(!Path::new(name).exists());
        let absolute = std::env::current_dir()
            .unwrap()
            .canonicalize()
            .unwrap()
            .join(name);
        let expected = paths.ticker_state(&absolute);
        assert_eq!(paths.ticker_state(Path::new(name)), expected);
        assert_eq!(paths.ticker_state(&Path::new(".").join(name)), expected);
    }
}
