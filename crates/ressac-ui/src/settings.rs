//! The app's settings: keys, legal values, defaults, and the rules that keep a hand-edited
//! file harmless. The values live in a [`SettingsStore`] (QSettings in the app, a map in
//! tests); this module never touches a file.

use std::collections::BTreeMap;

/// The keys, spelled as the Mac app spells them.
pub mod keys {
    pub const APPEARANCE: &str = "appearance";
    pub const CRT: &str = "antiope.crt";
    pub const READER_FONT: &str = "reader.font";
    pub const READER_SIZE: &str = "reader.size";
    pub const READER_SIZE_ANTIOPE: &str = "reader.size.antiope";
    pub const READER_WIDTH: &str = "reader.width";
    pub const DATABASE_PATH: &str = "databasePath";
    pub const CHART_VIEW: &str = "chart.view";
    pub const CHART_INK: &str = "chart.ink";
    pub const CHART_ANIMATE: &str = "chart.animate";
}

pub const READER_SIZE_DEFAULT: f64 = 17.0;
pub const READER_SIZE_MIN: f64 = 12.0;
pub const READER_SIZE_MAX: f64 = 28.0;
pub const ANTIOPE_SIZES: [f64; 4] = [15.0, 20.0, 25.0, 30.0];
pub const ANTIOPE_SIZE_DEFAULT: f64 = 20.0;

/// Declares a setting's enum with its stored spelling, once.
macro_rules! stored_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub enum $name { $($(#[$vmeta])* $variant),+ }

        impl $name {
            /// Every value, in the order the settings page lists them.
            pub const ALL: [Self; [$($text),+].len()] = [$(Self::$variant),+];

            /// The stored spelling.
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }

            /// The value stored as `s` (exact spelling only).
            pub fn parse(s: &str) -> Option<Self> {
                match s { $($text => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

stored_enum!(
    /// The look of the app: the system's, or the teletext one.
    Appearance { #[default] Native => "native", Antiope => "antiope" }
);
stored_enum!(
    /// The reading font.
    ReaderFont {
        #[default] SystemSans => "systemSans",
        SystemSerif => "systemSerif",
        Newsreader => "newsreader",
        Atkinson => "atkinson",
    }
);
stored_enum!(
    /// The reading column's width.
    LineWidth { Narrow => "narrow", #[default] Medium => "medium", Wide => "wide", Forty => "forty" }
);
stored_enum!(
    /// What the chart page shows.
    ChartView { #[default] Harbour => "harbour", Themes => "themes" }
);
stored_enum!(
    /// The chart's ink.
    ChartInk { #[default] Bleu => "bleu", Nuit => "nuit" }
);

impl LineWidth {
    /// The column's maximum width, in characters.
    pub fn max_ch(self) -> u32 {
        match self {
            Self::Narrow => 58,
            Self::Medium => 68,
            Self::Wide => 80,
            Self::Forty => 40,
        }
    }
}

impl Appearance {
    /// The look actually drawn: never Antiope in a fallback font.
    pub fn effective(stored: Appearance, bedstead_available: bool) -> Appearance {
        if stored == Appearance::Antiope && !bedstead_available {
            Appearance::Native
        } else {
            stored
        }
    }
}

/// View ▸ Bigger / Smaller / Actual Size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSizeCommand {
    Bigger,
    Smaller,
    ActualSize,
}

/// A native text size: whole points in 12…28; anything not finite is the default.
pub fn clamped_size(size: f64) -> f64 {
    if size.is_finite() {
        size.round().clamp(READER_SIZE_MIN, READER_SIZE_MAX)
    } else {
        READER_SIZE_DEFAULT
    }
}

/// The index of the Antiope step nearest to `size` (ties go up; not finite is the default).
fn antiope_index(size: f64) -> usize {
    if !size.is_finite() {
        return ANTIOPE_SIZES
            .iter()
            .position(|&s| s == ANTIOPE_SIZE_DEFAULT)
            .unwrap_or(0);
    }
    let mut best = 0;
    for (i, step) in ANTIOPE_SIZES.iter().enumerate() {
        if (step - size).abs() <= (ANTIOPE_SIZES[best] - size).abs() {
            best = i;
        }
    }
    best
}

/// An Antiope text size: the nearest step (ties go up; not finite is 20).
pub fn snapped_antiope(size: f64) -> f64 {
    ANTIOPE_SIZES[antiope_index(size)]
}

/// The step `delta` places from `size`'s snapped step, stopping at the ends.
pub fn step_antiope(size: f64, delta: i32) -> f64 {
    let last = ANTIOPE_SIZES.len() as i64 - 1;
    let index = (antiope_index(size) as i64 + i64::from(delta)).clamp(0, last);
    ANTIOPE_SIZES[index as usize]
}

/// The size a look really draws at, whatever is stored.
pub fn effective_size(size: f64, look: Appearance) -> f64 {
    match look {
        Appearance::Native => clamped_size(size),
        Appearance::Antiope => snapped_antiope(size),
    }
}

/// The look's Actual Size.
pub fn actual_size(look: Appearance) -> f64 {
    match look {
        Appearance::Native => READER_SIZE_DEFAULT,
        Appearance::Antiope => ANTIOPE_SIZE_DEFAULT,
    }
}

/// The key holding the look's text size.
pub fn size_key(look: Appearance) -> &'static str {
    match look {
        Appearance::Native => keys::READER_SIZE,
        Appearance::Antiope => keys::READER_SIZE_ANTIOPE,
    }
}

/// Where the values live.
pub trait SettingsStore {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&mut self, key: &str, value: &str);
    fn remove(&mut self, key: &str);
}

/// The test store.
impl SettingsStore for BTreeMap<String, String> {
    fn get(&self, key: &str) -> Option<String> {
        BTreeMap::get(self, key).cloned()
    }

    fn set(&mut self, key: &str, value: &str) {
        self.insert(key.to_string(), value.to_string());
    }

    fn remove(&mut self, key: &str) {
        BTreeMap::remove(self, key);
    }
}

/// The chart page's preferences.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartPrefs {
    pub view: ChartView,
    pub ink: ChartInk,
    pub animate: bool,
}

impl Default for ChartPrefs {
    fn default() -> Self {
        ChartPrefs {
            view: ChartView::default(),
            ink: ChartInk::default(),
            animate: true,
        }
    }
}

/// Every setting, as read from a store.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub appearance: Appearance,
    pub crt: bool,
    pub reader_font: ReaderFont,
    pub reader_size: f64,
    pub reader_size_antiope: f64,
    pub reader_width: LineWidth,
    /// As stored (a leading `~` is expanded at launch); `None` is the default location.
    pub database_path: Option<String>,
    pub chart: ChartPrefs,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            appearance: Appearance::default(),
            crt: false,
            reader_font: ReaderFont::default(),
            reader_size: READER_SIZE_DEFAULT,
            reader_size_antiope: ANTIOPE_SIZE_DEFAULT,
            reader_width: LineWidth::default(),
            database_path: None,
            chart: ChartPrefs::default(),
        }
    }
}

fn read_bool(store: &dyn SettingsStore, key: &str) -> Option<bool> {
    match store.get(key)?.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn read_f64(store: &dyn SettingsStore, key: &str) -> Option<f64> {
    store.get(key)?.trim().parse().ok()
}

fn read_enum<T>(store: &dyn SettingsStore, key: &str, parse: fn(&str) -> Option<T>) -> Option<T> {
    parse(&store.get(key)?)
}

/// A path as stored: trimmed, and `None` when blank.
fn non_blank(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

impl Settings {
    /// Reads the store. A missing or unparsable value is the default; sizes are clamped
    /// or snapped; a retired chart ink (`pop`, `pirate`) reads as `bleu`.
    pub fn load(store: &dyn SettingsStore) -> Settings {
        let default = Settings::default();
        Settings {
            appearance: read_enum(store, keys::APPEARANCE, Appearance::parse)
                .unwrap_or(default.appearance),
            crt: read_bool(store, keys::CRT).unwrap_or(default.crt),
            reader_font: read_enum(store, keys::READER_FONT, ReaderFont::parse)
                .unwrap_or(default.reader_font),
            reader_size: read_f64(store, keys::READER_SIZE)
                .map_or(default.reader_size, clamped_size),
            reader_size_antiope: read_f64(store, keys::READER_SIZE_ANTIOPE)
                .map_or(default.reader_size_antiope, snapped_antiope),
            reader_width: read_enum(store, keys::READER_WIDTH, LineWidth::parse)
                .unwrap_or(default.reader_width),
            database_path: store
                .get(keys::DATABASE_PATH)
                .and_then(|path| non_blank(&path)),
            chart: ChartPrefs {
                view: read_enum(store, keys::CHART_VIEW, ChartView::parse)
                    .unwrap_or(default.chart.view),
                ink: read_enum(store, keys::CHART_INK, ChartInk::parse)
                    .unwrap_or(default.chart.ink),
                animate: read_bool(store, keys::CHART_ANIMATE).unwrap_or(default.chart.animate),
            },
        }
    }

    pub fn set_appearance(&mut self, store: &mut dyn SettingsStore, value: &str) -> bool {
        let Some(value) = Appearance::parse(value) else {
            return false;
        };
        store.set(keys::APPEARANCE, value.as_str());
        replace(&mut self.appearance, value)
    }

    pub fn set_crt(&mut self, store: &mut dyn SettingsStore, value: bool) -> bool {
        store.set(keys::CRT, &value.to_string());
        replace(&mut self.crt, value)
    }

    pub fn set_reader_font(&mut self, store: &mut dyn SettingsStore, value: &str) -> bool {
        let Some(value) = ReaderFont::parse(value) else {
            return false;
        };
        store.set(keys::READER_FONT, value.as_str());
        replace(&mut self.reader_font, value)
    }

    /// Clamped to 12…28.
    pub fn set_reader_size(&mut self, store: &mut dyn SettingsStore, value: f64) -> bool {
        let value = clamped_size(value);
        store.set(keys::READER_SIZE, &value.to_string());
        replace(&mut self.reader_size, value)
    }

    /// Snapped to the nearest Antiope step.
    pub fn set_reader_size_antiope(&mut self, store: &mut dyn SettingsStore, value: f64) -> bool {
        let value = snapped_antiope(value);
        store.set(keys::READER_SIZE_ANTIOPE, &value.to_string());
        replace(&mut self.reader_size_antiope, value)
    }

    pub fn set_reader_width(&mut self, store: &mut dyn SettingsStore, value: &str) -> bool {
        let Some(value) = LineWidth::parse(value) else {
            return false;
        };
        store.set(keys::READER_WIDTH, value.as_str());
        replace(&mut self.reader_width, value)
    }

    /// A blank path removes the key: the default location.
    pub fn set_database_path(&mut self, store: &mut dyn SettingsStore, value: &str) -> bool {
        let value = non_blank(value);
        match &value {
            Some(path) => store.set(keys::DATABASE_PATH, path),
            None => store.remove(keys::DATABASE_PATH),
        }
        replace(&mut self.database_path, value)
    }

    pub fn set_chart_view(&mut self, store: &mut dyn SettingsStore, value: &str) -> bool {
        let Some(value) = ChartView::parse(value) else {
            return false;
        };
        store.set(keys::CHART_VIEW, value.as_str());
        replace(&mut self.chart.view, value)
    }

    pub fn set_chart_ink(&mut self, store: &mut dyn SettingsStore, value: &str) -> bool {
        let Some(value) = ChartInk::parse(value) else {
            return false;
        };
        store.set(keys::CHART_INK, value.as_str());
        replace(&mut self.chart.ink, value)
    }

    pub fn set_chart_animate(&mut self, store: &mut dyn SettingsStore, value: bool) -> bool {
        store.set(keys::CHART_ANIMATE, &value.to_string());
        replace(&mut self.chart.animate, value)
    }

    /// The size the look shown really draws at.
    pub fn shown_size(&self, shown: Appearance) -> f64 {
        match shown {
            Appearance::Native => effective_size(self.reader_size, shown),
            Appearance::Antiope => effective_size(self.reader_size_antiope, shown),
        }
    }

    /// View ▸ Bigger / Smaller / Actual Size on the look shown: writes only that look's
    /// key (always, so a hand-edited value is rewritten as a legal one) and returns its
    /// new size and whether the stored value changed.
    pub fn resize(
        &mut self,
        store: &mut dyn SettingsStore,
        shown: Appearance,
        command: TextSizeCommand,
    ) -> (f64, bool) {
        let slot = match shown {
            Appearance::Native => &mut self.reader_size,
            Appearance::Antiope => &mut self.reader_size_antiope,
        };
        let current = effective_size(*slot, shown);
        let delta = match command {
            TextSizeCommand::Bigger => 1,
            TextSizeCommand::Smaller => -1,
            TextSizeCommand::ActualSize => 0,
        };
        let size = match (command, shown) {
            (TextSizeCommand::ActualSize, _) => actual_size(shown),
            (_, Appearance::Native) => clamped_size(current + f64::from(delta)),
            (_, Appearance::Antiope) => step_antiope(current, delta),
        };
        store.set(size_key(shown), &size.to_string());
        (size, replace(slot, size))
    }
}

/// Stores `value` in `slot`; true when it differs from what was there.
fn replace<T: PartialEq>(slot: &mut T, value: T) -> bool {
    let changed = *slot != value;
    *slot = value;
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn store(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn keys_are_the_mac_app_s() {
        assert_eq!(
            [
                keys::APPEARANCE,
                keys::CRT,
                keys::READER_FONT,
                keys::READER_SIZE,
                keys::READER_SIZE_ANTIOPE,
                keys::READER_WIDTH,
                keys::DATABASE_PATH,
                keys::CHART_VIEW,
                keys::CHART_INK,
                keys::CHART_ANIMATE
            ],
            [
                "appearance",
                "antiope.crt",
                "reader.font",
                "reader.size",
                "reader.size.antiope",
                "reader.width",
                "databasePath",
                "chart.view",
                "chart.ink",
                "chart.animate"
            ]
        );
    }

    #[test]
    fn values_are_spelled_once_and_parse_back() {
        assert_eq!(
            Appearance::ALL.map(Appearance::as_str),
            ["native", "antiope"]
        );
        assert_eq!(
            ReaderFont::ALL.map(ReaderFont::as_str),
            ["systemSans", "systemSerif", "newsreader", "atkinson"]
        );
        assert_eq!(
            LineWidth::ALL.map(LineWidth::as_str),
            ["narrow", "medium", "wide", "forty"]
        );
        assert_eq!(LineWidth::ALL.map(LineWidth::max_ch), [58, 68, 80, 40]);
        assert_eq!(ChartView::ALL.map(ChartView::as_str), ["harbour", "themes"]);
        assert_eq!(ChartInk::ALL.map(ChartInk::as_str), ["bleu", "nuit"]);
        for font in ReaderFont::ALL {
            assert_eq!(ReaderFont::parse(font.as_str()), Some(font));
        }
        assert_eq!(Appearance::parse("teletext"), None);
        assert_eq!(Appearance::parse("Antiope"), None); // exact spelling only
    }

    #[test]
    fn an_empty_store_gives_the_defaults() {
        let s = Settings::load(&BTreeMap::<String, String>::new());
        assert_eq!(s, Settings::default());
        assert_eq!(
            (s.appearance, s.crt, s.reader_font),
            (Appearance::Native, false, ReaderFont::SystemSans)
        );
        assert_eq!(
            (s.reader_size, s.reader_size_antiope, s.reader_width),
            (17.0, 20.0, LineWidth::Medium)
        );
        assert_eq!(s.database_path, None);
        assert_eq!(
            s.chart,
            ChartPrefs {
                view: ChartView::Harbour,
                ink: ChartInk::Bleu,
                animate: true
            }
        );
    }

    #[test]
    fn load_reads_the_store_and_ignores_garbage() {
        let good = store(&[
            ("appearance", "antiope"),
            ("antiope.crt", "true"),
            ("reader.font", "newsreader"),
            ("reader.size", "21"),
            ("reader.size.antiope", "25"),
            ("reader.width", "wide"),
            ("databasePath", "~/Feeds/other.sqlite"),
            ("chart.view", "themes"),
            ("chart.ink", "nuit"),
            ("chart.animate", "false"),
        ]);
        let s = Settings::load(&good);
        assert_eq!(
            (s.appearance, s.crt, s.reader_font, s.reader_size),
            (Appearance::Antiope, true, ReaderFont::Newsreader, 21.0)
        );
        assert_eq!(
            (s.reader_size_antiope, s.reader_width),
            (25.0, LineWidth::Wide)
        );
        assert_eq!(s.database_path.as_deref(), Some("~/Feeds/other.sqlite")); // raw: launch expands it
        assert_eq!(
            s.chart,
            ChartPrefs {
                view: ChartView::Themes,
                ink: ChartInk::Nuit,
                animate: false
            }
        );

        let bad = store(&[
            ("appearance", "teletext"),
            ("antiope.crt", "yes"),
            ("reader.font", "comic-sans"),
            ("reader.size", "99"),
            ("reader.size.antiope", "17"),
            ("reader.width", "huge"),
            ("databasePath", "   "),
            ("chart.view", "map"),
            ("chart.ink", "pop"),
            ("chart.animate", "0"),
        ]);
        let s = Settings::load(&bad);
        assert_eq!(
            (s.appearance, s.crt, s.reader_font),
            (Appearance::Native, false, ReaderFont::SystemSans)
        );
        assert_eq!(
            (s.reader_size, s.reader_size_antiope, s.reader_width),
            (28.0, 15.0, LineWidth::Medium)
        );
        assert_eq!(s.database_path, None);
        assert_eq!(s.chart, ChartPrefs::default()); // "pop" is a retired ink: bleu
        assert_eq!(
            Settings::load(&store(&[
                ("reader.size", "nan"),
                ("reader.size.antiope", "inf")
            ]))
            .reader_size,
            17.0
        );
        assert_eq!(
            Settings::load(&store(&[("reader.size.antiope", "inf")])).reader_size_antiope,
            20.0
        );
    }

    #[test]
    fn text_size_steps_and_clamps() {
        assert_eq!(
            [
                clamped_size(40.0),
                clamped_size(3.0),
                clamped_size(16.6),
                clamped_size(f64::NAN)
            ],
            [28.0, 12.0, 17.0, 17.0]
        );
    }

    #[test]
    fn antiope_sizes_snap_to_the_nearest_step_and_ties_go_up() {
        let cases = [
            (12.0, 15.0),
            (15.0, 15.0),
            (17.0, 15.0),
            (17.5, 20.0),
            (18.0, 20.0),
            (20.0, 20.0),
            (22.0, 20.0),
            (22.5, 25.0),
            (23.0, 25.0),
            (27.0, 25.0),
            (27.5, 30.0),
            (28.0, 30.0),
            (40.0, 30.0),
            (-3.0, 15.0),
            (f64::NAN, 20.0),
            (f64::INFINITY, 20.0),
        ];
        for (size, expected) in cases {
            assert_eq!(snapped_antiope(size), expected, "{size}");
        }
    }

    #[test]
    fn antiope_steps_move_from_the_snapped_size_and_stop_at_the_ends() {
        assert_eq!(step_antiope(17.0, 1), 20.0); // 17 draws at 15
        assert_eq!(step_antiope(18.0, 1), 25.0); // 18 draws at 20
        assert_eq!(
            [
                step_antiope(20.0, -1),
                step_antiope(30.0, 1),
                step_antiope(15.0, -1),
                step_antiope(15.0, 7),
                step_antiope(25.0, 0)
            ],
            [15.0, 30.0, 15.0, 30.0, 25.0]
        );
        assert_eq!(
            [
                effective_size(17.0, Appearance::Antiope),
                effective_size(17.0, Appearance::Native)
            ],
            [15.0, 17.0]
        );
        assert_eq!(
            [
                effective_size(f64::NAN, Appearance::Antiope),
                effective_size(40.0, Appearance::Native)
            ],
            [20.0, 28.0]
        );
        assert_eq!(
            [
                actual_size(Appearance::Native),
                actual_size(Appearance::Antiope)
            ],
            [17.0, 20.0]
        );
        assert_eq!(
            [size_key(Appearance::Native), size_key(Appearance::Antiope)],
            ["reader.size", "reader.size.antiope"]
        );
    }

    /// Decision of 2026-10-14 (Mac `TextSizePerLookTests`): each look keeps its own size.
    #[test]
    fn resizing_acts_on_the_look_shown_and_never_on_the_other() {
        let mut st: BTreeMap<String, String> = BTreeMap::new();
        let mut s = Settings::load(&st);
        assert_eq!(
            s.resize(&mut st, Appearance::Antiope, TextSizeCommand::Bigger),
            (25.0, true)
        );
        assert_eq!(
            st.get("reader.size.antiope").map(String::as_str),
            Some("25")
        );
        assert_eq!(st.get("reader.size"), None);
        assert_eq!(
            s.resize(&mut st, Appearance::Native, TextSizeCommand::Smaller),
            (16.0, true)
        );
        assert_eq!(
            (
                st["reader.size"].as_str(),
                st["reader.size.antiope"].as_str()
            ),
            ("16", "25")
        );
        assert_eq!(
            s.resize(&mut st, Appearance::Native, TextSizeCommand::Bigger),
            (17.0, true)
        );
        assert_eq!(
            s.resize(&mut st, Appearance::Antiope, TextSizeCommand::ActualSize),
            (20.0, true)
        );
        assert_eq!(
            s.resize(&mut st, Appearance::Antiope, TextSizeCommand::ActualSize),
            (20.0, false)
        );
        assert_eq!(s.shown_size(Appearance::Native), 17.0);
        assert_eq!(st["reader.size"].as_str(), "17");
        for stored in ["17", "0", "-3", "22.5", "99"] {
            for command in [
                TextSizeCommand::Bigger,
                TextSizeCommand::Smaller,
                TextSizeCommand::ActualSize,
            ] {
                let mut st = store(&[("reader.size.antiope", stored)]);
                let mut s = Settings::load(&st);
                let (size, _) = s.resize(&mut st, Appearance::Antiope, command);
                assert!(ANTIOPE_SIZES.contains(&size), "{stored} {command:?}");
                assert_eq!(st["reader.size.antiope"], size.to_string());
            }
        }
    }

    #[test]
    fn setters_normalise_write_and_report_changes() {
        let mut st: BTreeMap<String, String> = BTreeMap::new();
        let mut s = Settings::load(&st);
        assert!(s.set_reader_size(&mut st, 40.0));
        assert_eq!((s.reader_size, st["reader.size"].as_str()), (28.0, "28"));
        assert!(!s.set_reader_size(&mut st, 28.4)); // same normalised value: no change
        assert!(s.set_reader_size_antiope(&mut st, 23.0));
        assert_eq!(st["reader.size.antiope"].as_str(), "25");
        assert!(!s.set_appearance(&mut st, "teletext"));
        assert_eq!(st.get("appearance"), None);
        assert!(
            s.set_appearance(&mut st, "antiope")
                && s.set_crt(&mut st, true)
                && s.set_reader_font(&mut st, "atkinson")
        );
        assert!(
            s.set_reader_width(&mut st, "forty")
                && s.set_chart_view(&mut st, "themes")
                && s.set_chart_ink(&mut st, "nuit")
        );
        assert!(s.set_chart_animate(&mut st, false));
        assert_eq!(Settings::load(&st), s); // what is stored reads back the same
        assert!(s.set_database_path(&mut st, "/data/r.sqlite"));
        assert_eq!(st["databasePath"].as_str(), "/data/r.sqlite");
        assert!(s.set_database_path(&mut st, "  "));
        assert_eq!(
            (st.get("databasePath"), s.database_path.as_deref()),
            (None, None)
        );
        assert!(!s.set_database_path(&mut st, ""));
    }

    /// Mac `antiopeFallsBackToNativeWithoutBedstead`: never Antiope in a fallback font.
    #[test]
    fn antiope_only_with_bedstead() {
        assert_eq!(
            Appearance::effective(Appearance::Antiope, true),
            Appearance::Antiope
        );
        assert_eq!(
            Appearance::effective(Appearance::Antiope, false),
            Appearance::Native
        );
        assert_eq!(
            Appearance::effective(Appearance::Native, true),
            Appearance::Native
        );
    }
}
