//! The reading window's single-key commands (spec §7.2; the Mac's `ReadingKeyRouter`): Qt key
//! events decoded to a [`Key`], the focused item's chain of tokens classified to a
//! [`KeyFocus`], the Native and Antiope maps from both to a [`Command`], which keys the menus never
//! see, and the verb the QML page performs.

use crate::settings::Appearance;

/// The Qt key codes and modifier bits the decoder reads (`Qt::Key`, `Qt::KeyboardModifier`).
pub mod qt {
    pub const KEY_ESCAPE: i32 = 0x0100_0000;
    pub const KEY_BACKSPACE: i32 = 0x0100_0003;
    pub const KEY_LEFT: i32 = 0x0100_0012;
    pub const KEY_UP: i32 = 0x0100_0013;
    pub const KEY_RIGHT: i32 = 0x0100_0014;
    pub const KEY_DOWN: i32 = 0x0100_0015;
    pub const KEY_SPACE: i32 = 0x20;
    pub const KEY_QUESTION: i32 = 0x3F;
    pub const SHIFT: u32 = 0x0200_0000;
    pub const CONTROL: u32 = 0x0400_0000;
    pub const ALT: u32 = 0x0800_0000;
    pub const META: u32 = 0x1000_0000;
    pub const KEYPAD: u32 = 0x2000_0000;
}

/// The tokens the shim puts in a focus chain, one per ancestor of the focused item.
pub mod focus {
    pub const TEXT: &str = "text";
    pub const WEB: &str = "web";
    pub const CONTROL: &str = "control";
    pub const POPUP: &str = "popup";
    pub const ITEM: &str = "item";
    pub const SOURCES_LIST: &str = "sourcesList";
    pub const ENTRIES_LIST: &str = "entriesList";
    pub const READER_PANE: &str = "readerPane";
    pub const READING_PAGE: &str = "readingPage";
    /// Antiope's three columns: the index list, the entries list, the reader column.
    pub const ANTIOPE_INDEX: &str = "antiopeIndex";
    pub const ANTIOPE_ENTRIES: &str = "antiopeEntries";
    pub const ANTIOPE_READER: &str = "antiopeReader";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A letter key, lowercased (Caps Lock reads as the letter).
    Letter(char),
    Space,
    Up,
    Down,
    Left,
    Right,
    /// 0 to 9, from the top row (with or without Shift: AZERTY) or the keypad.
    Digit(u8),
    Delete,
    Escape,
    /// Alt and a letter, lowercased.
    Option(char),
    /// `?`, Shift included.
    Question,
}

/// The part of the reading window that has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyFocus {
    Sidebar,
    EntryList,
    Reader,
    /// No item of the page has the keyboard.
    Window,
    /// The search field, any text or an open menu: every key is typing or menu navigation.
    TextInput,
    /// Another control (a toolbar button).
    Control,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    NextEntry,
    PreviousEntry,
    Advance,
    ToggleRead,
    ToggleStar,
    /// `→` in the sidebar.
    FocusEntryList,
    /// `←` in the list.
    FocusSidebar,
    /// Antiope: a digit of the page number being typed.
    PageDigit(u8),
    /// Antiope: CORRECTION (Backspace) erases the last digit.
    PageCorrect,
    /// Antiope: ANNULATION (Esc) cancels the page number.
    PageCancel,
    /// Antiope: SOMMAIRE (Alt+S), page 100.
    Sommaire,
    /// Antiope: GUIDE (Alt+G, `?`), page 899.
    Guide,
    /// Antiope: RÉPÉTITION (Alt+R) refreshes, or retries the edition.
    Repetition,
    /// Antiope: `i` swaps the article's images between mosaics and originals.
    ToggleImages,
    /// Antiope: `→` moves to the next column.
    NextColumn,
    /// Antiope: `←` moves to the previous column.
    PreviousColumn,
    /// Antiope: `↓` in the index.
    NextIndexRow,
    /// Antiope: `↑` in the index.
    PreviousIndexRow,
}

const HELD: u32 = qt::SHIFT | qt::CONTROL | qt::ALT | qt::META;

/// The key of a press, or `None` for the chords this window leaves to the menus and the
/// views. Only Shift, Control, Alt and Meta count (Keypad is ignored).
pub fn decode(qt_key: i32, text: &str, modifiers: u32) -> Option<Key> {
    let held = modifiers & HELD;
    if held & (qt::CONTROL | qt::META) != 0 {
        return None;
    }
    let plain = held == 0;
    let shift_only = held & !qt::SHIFT == 0;
    match qt_key {
        qt::KEY_DOWN => return plain.then_some(Key::Down),
        qt::KEY_UP => return plain.then_some(Key::Up),
        qt::KEY_LEFT => return plain.then_some(Key::Left),
        qt::KEY_RIGHT => return plain.then_some(Key::Right),
        qt::KEY_BACKSPACE => return plain.then_some(Key::Delete),
        qt::KEY_ESCAPE => return plain.then_some(Key::Escape),
        _ => {}
    }
    if qt_key == qt::KEY_QUESTION || text == "?" {
        return shift_only.then_some(Key::Question);
    }
    let digit = match text.as_bytes() {
        [b @ b'0'..=b'9'] => Some(b - b'0'),
        _ => match qt_key {
            0x30..=0x39 => Some((qt_key - 0x30) as u8),
            _ => None,
        },
    };
    if let Some(digit) = digit {
        return shift_only.then_some(Key::Digit(digit));
    }
    if (0x41..=0x5A).contains(&qt_key) {
        let letter = char::from(qt_key as u8).to_ascii_lowercase();
        return match held {
            0 => Some(Key::Letter(letter)),
            qt::ALT => Some(Key::Option(letter)),
            _ => None,
        };
    }
    (qt_key == qt::KEY_SPACE && plain).then_some(Key::Space)
}

/// Which part of the window has the keyboard, from the focused item's chain of tokens
/// (the item first, then its ancestors).
pub fn classify(chain: &[&str]) -> KeyFocus {
    let has = |token: &str| chain.contains(&token);
    let first = chain.first().copied();
    if first.is_none() {
        KeyFocus::Window
    } else if has(focus::POPUP) || first == Some(focus::TEXT) {
        KeyFocus::TextInput
    } else if has(focus::WEB) || has(focus::READER_PANE) || has(focus::ANTIOPE_READER) {
        KeyFocus::Reader
    } else if has(focus::ENTRIES_LIST) || has(focus::ANTIOPE_ENTRIES) {
        KeyFocus::EntryList
    } else if has(focus::SOURCES_LIST) || has(focus::ANTIOPE_INDEX) {
        KeyFocus::Sidebar
    } else if matches!(first, Some(focus::ITEM | focus::READING_PAGE)) {
        KeyFocus::Window
    } else {
        KeyFocus::Control
    }
}

/// NetNewsWire's rules: letters and Space act wherever the window is not typing; `↓` and `↑`
/// are `j` and `k` in the list and the reader, the sidebar keeps its own; `→` leaves the
/// sidebar for the list and `←` comes back. In Antiope its own keys are tried first.
pub fn command(key: Key, focus: KeyFocus, look: Appearance) -> Option<Command> {
    if focus == KeyFocus::TextInput {
        return None;
    }
    if look == Appearance::Antiope
        && let Some(command) = antiope_command(key, focus)
    {
        return Some(command);
    }
    native_command(key, focus)
}

/// The Antiope keys (the Mac's `ReadingKeyRouter`): page digits, CORRECTION, ANNULATION,
/// SOMMAIRE, GUIDE, RÉPÉTITION, images, the columns and the index rows.
fn antiope_command(key: Key, focus: KeyFocus) -> Option<Command> {
    match key {
        Key::Digit(d) => Some(Command::PageDigit(d)),
        Key::Delete => Some(Command::PageCorrect),
        Key::Escape => Some(Command::PageCancel),
        Key::Option('s') => Some(Command::Sommaire),
        Key::Option('g') | Key::Question => Some(Command::Guide),
        Key::Option('r') => Some(Command::Repetition),
        Key::Letter('i') => Some(Command::ToggleImages),
        Key::Right if focus != KeyFocus::Control => Some(Command::NextColumn),
        Key::Left if focus != KeyFocus::Control => Some(Command::PreviousColumn),
        Key::Down if focus == KeyFocus::Sidebar => Some(Command::NextIndexRow),
        Key::Up if focus == KeyFocus::Sidebar => Some(Command::PreviousIndexRow),
        _ => None,
    }
}

/// Antiope's keys that do their job once per press: a held one is only spent (a held digit
/// never types three). CORRECTION and the index arrows repeat.
pub fn acts_once_per_press(command: Command) -> bool {
    matches!(
        command,
        Command::PageDigit(_)
            | Command::PageCancel
            | Command::Sommaire
            | Command::Guide
            | Command::Repetition
            | Command::ToggleImages
            | Command::NextColumn
            | Command::PreviousColumn
    )
}

const PAGE_DIGIT_VERBS: [&str; 10] = [
    "pageDigit0",
    "pageDigit1",
    "pageDigit2",
    "pageDigit3",
    "pageDigit4",
    "pageDigit5",
    "pageDigit6",
    "pageDigit7",
    "pageDigit8",
    "pageDigit9",
];

fn native_command(key: Key, focus: KeyFocus) -> Option<Command> {
    use KeyFocus::*;
    match (key, focus) {
        (Key::Letter('j'), _) => Some(Command::NextEntry),
        (Key::Letter('k'), _) => Some(Command::PreviousEntry),
        (Key::Letter('m'), _) => Some(Command::ToggleRead),
        (Key::Letter('s'), _) => Some(Command::ToggleStar),
        (Key::Space, Sidebar | EntryList | Reader | Window) => Some(Command::Advance),
        (Key::Down, EntryList | Reader) => Some(Command::NextEntry),
        (Key::Up, EntryList | Reader) => Some(Command::PreviousEntry),
        (Key::Right, Sidebar) => Some(Command::FocusEntryList),
        (Key::Left, EntryList) => Some(Command::FocusSidebar),
        _ => None,
    }
}

/// Whether the window must take the key before any menu shortcut sees it: every letter and
/// Space outside a text field (so a single-key menu label never fires on its own), and any
/// other key that maps to a command. Chords and mnemonics are never claimed.
pub fn claims_override(key: Key, focus: KeyFocus, look: Appearance) -> bool {
    focus != KeyFocus::TextInput
        && (matches!(key, Key::Letter(_) | Key::Space) || command(key, focus, look).is_some())
}

/// The name `ReadingPage.perform` switches on. A held toggle only spends the key, and a held
/// Space never moves on to the next entry; an Antiope key that acts once per press is
/// `spent` (consumed, nothing done).
pub fn verb(command: Command, repeat: bool) -> &'static str {
    if repeat && acts_once_per_press(command) {
        return "spent";
    }
    match (command, repeat) {
        (Command::Advance, true) => "advanceHeld",
        (Command::ToggleRead, true) => "toggleReadHeld",
        (Command::ToggleStar, true) => "toggleStarHeld",
        (Command::NextEntry, _) => "nextEntry",
        (Command::PreviousEntry, _) => "previousEntry",
        (Command::Advance, false) => "advance",
        (Command::ToggleRead, false) => "toggleRead",
        (Command::ToggleStar, false) => "toggleStar",
        (Command::FocusEntryList, _) => "focusEntryList",
        (Command::FocusSidebar, _) => "focusSidebar",
        (Command::PageDigit(d), _) => PAGE_DIGIT_VERBS[usize::from(d.min(9))],
        (Command::PageCorrect, _) => "pageCorrect",
        (Command::PageCancel, _) => "pageCancel",
        (Command::Sommaire, _) => "sommaire",
        (Command::Guide, _) => "guide",
        (Command::Repetition, _) => "repetition",
        (Command::ToggleImages, _) => "toggleImages",
        (Command::NextColumn, _) => "nextColumn",
        (Command::PreviousColumn, _) => "previousColumn",
        (Command::NextIndexRow, _) => "nextIndexRow",
        (Command::PreviousIndexRow, _) => "previousIndexRow",
    }
}

/// What the window does with a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    /// The `ShortcutOverride` is accepted: no menu or `Shortcut` sees the key.
    pub claims_override: bool,
    /// What `ReadingPage.perform` is asked, when the key maps to a command.
    pub verb: Option<&'static str>,
}

/// Decoding, focus and the held-key rules in one call (the shim's entry point).
pub fn route(
    qt_key: i32,
    text: &str,
    modifiers: u32,
    repeat: bool,
    chain: &[&str],
    look: Appearance,
) -> Route {
    let Some(key) = decode(qt_key, text, modifiers) else {
        return Route {
            claims_override: false,
            verb: None,
        };
    };
    let focus = classify(chain);
    Route {
        claims_override: claims_override(key, focus, look),
        verb: command(key, focus, look).map(|c| verb(c, repeat)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: u32 = 0;

    fn k(key: i32, text: &str, mods: u32) -> Option<Key> {
        decode(key, text, mods)
    }

    #[test]
    fn modified_keys_are_left_alone() {
        assert_eq!(k(0x4A, "j", NONE), Some(Key::Letter('j')));
        assert_eq!(k(qt::KEY_SPACE, " ", NONE), Some(Key::Space));
        assert_eq!(k(qt::KEY_DOWN, "", NONE), Some(Key::Down));
        assert_eq!(k(qt::KEY_DOWN, "", qt::KEYPAD), Some(Key::Down));
        for (key, text, mods) in [
            (0x4A, "J", qt::SHIFT),
            (0x4A, "j", qt::CONTROL),
            (0x4A, "j", qt::META),
            (qt::KEY_DOWN, "", qt::SHIFT),
            (0x34, "4", qt::ALT),
            (0x53, "S", qt::ALT | qt::SHIFT),
            (qt::KEY_BACKSPACE, "", qt::ALT),
            (0x3F, "?", qt::CONTROL),
        ] {
            assert_eq!(k(key, text, mods), None, "{key:#x} {mods:#x}");
        }
        assert_eq!(k(0x4A, "J", NONE), Some(Key::Letter('j'))); // Caps Lock
        assert_eq!(k(0x4A, "j", qt::ALT), Some(Key::Option('j')));
        assert_eq!(k(0x34, "4", NONE), Some(Key::Digit(4)));
        assert_eq!(k(0x26, "1", qt::SHIFT), Some(Key::Digit(1))); // AZERTY top row
        assert_eq!(k(0x30, "0", qt::KEYPAD), Some(Key::Digit(0)));
        assert_eq!(k(qt::KEY_BACKSPACE, "\u{8}", NONE), Some(Key::Delete));
        assert_eq!(k(qt::KEY_ESCAPE, "\u{1b}", NONE), Some(Key::Escape));
        assert_eq!(k(0x3F, "?", qt::SHIFT), Some(Key::Question));
    }

    const LOOKS: [Appearance; 2] = [Appearance::Native, Appearance::Antiope];
    const PANES: [KeyFocus; 4] = [
        KeyFocus::Sidebar,
        KeyFocus::EntryList,
        KeyFocus::Reader,
        KeyFocus::Window,
    ];

    #[test]
    fn letters_and_space_act_everywhere_but_in_text_fields() {
        for look in LOOKS {
            for focus in PANES {
                for (key, expected) in [
                    (Key::Letter('j'), Command::NextEntry),
                    (Key::Letter('k'), Command::PreviousEntry),
                    (Key::Letter('m'), Command::ToggleRead),
                    (Key::Letter('s'), Command::ToggleStar),
                    (Key::Space, Command::Advance),
                ] {
                    assert_eq!(
                        command(key, focus, look),
                        Some(expected),
                        "{key:?} {focus:?}"
                    );
                }
            }
            for key in [Key::Letter('j'), Key::Space, Key::Down, Key::Right] {
                assert_eq!(command(key, KeyFocus::TextInput, look), None);
            }
            assert_eq!(command(Key::Letter('x'), KeyFocus::EntryList, look), None);
            assert_eq!(command(Key::Space, KeyFocus::Control, look), None);
        }
    }

    #[test]
    fn arrows_follow_the_focus() {
        let n = Appearance::Native;
        for focus in [KeyFocus::EntryList, KeyFocus::Reader] {
            assert_eq!(command(Key::Down, focus, n), Some(Command::NextEntry));
            assert_eq!(command(Key::Up, focus, n), Some(Command::PreviousEntry));
        }
        assert_eq!(command(Key::Down, KeyFocus::Sidebar, n), None);
        assert_eq!(command(Key::Up, KeyFocus::Sidebar, n), None);
        assert_eq!(
            command(Key::Right, KeyFocus::Sidebar, n),
            Some(Command::FocusEntryList)
        );
        assert_eq!(
            command(Key::Left, KeyFocus::EntryList, n),
            Some(Command::FocusSidebar)
        );
        assert_eq!(command(Key::Left, KeyFocus::Sidebar, n), None);
        assert_eq!(command(Key::Right, KeyFocus::EntryList, n), None);
        assert_eq!(command(Key::Left, KeyFocus::Reader, n), None);
    }

    #[test]
    fn native_never_produces_an_antiope_command() {
        let keys = [
            Key::Digit(0),
            Key::Digit(7),
            Key::Delete,
            Key::Escape,
            Key::Option('s'),
            Key::Option('g'),
            Key::Option('r'),
            Key::Question,
            Key::Letter('i'),
        ];
        for key in keys {
            assert_eq!(
                command(key, KeyFocus::EntryList, Appearance::Native),
                None,
                "{key:?}"
            );
        }
    }

    #[test]
    fn the_focused_item_is_classified_by_its_chain() {
        use focus::*;
        assert_eq!(classify(&[WEB, "reader", READING_PAGE]), KeyFocus::Reader);
        assert_eq!(
            classify(&[ITEM, WEB, READER_PANE, READING_PAGE]),
            KeyFocus::Reader
        );
        assert_eq!(
            classify(&[TEXT, "search", READING_PAGE]),
            KeyFocus::TextInput
        );
        assert_eq!(
            classify(&[ENTRIES_LIST, "entriesPane", READING_PAGE]),
            KeyFocus::EntryList
        );
        assert_eq!(
            classify(&[CONTROL, ITEM, ENTRIES_LIST, "entriesPane"]),
            KeyFocus::EntryList
        );
        assert_eq!(
            classify(&[SOURCES_LIST, "sourcesPane", READING_PAGE]),
            KeyFocus::Sidebar
        );
        assert_eq!(classify(&[CONTROL, READING_PAGE]), KeyFocus::Control);
        assert_eq!(
            classify(&[CONTROL, ITEM, POPUP, READING_PAGE]),
            KeyFocus::TextInput
        );
        assert_eq!(classify(&[READING_PAGE]), KeyFocus::Window);
        assert_eq!(classify(&[ITEM, ITEM]), KeyFocus::Window);
        assert_eq!(classify(&[]), KeyFocus::Window);
    }

    /// Review Focus 3.
    #[test]
    fn menus_never_see_plain_reading_keys_but_keep_their_chords() {
        for focus in [
            KeyFocus::Sidebar,
            KeyFocus::EntryList,
            KeyFocus::Reader,
            KeyFocus::Window,
            KeyFocus::Control,
        ] {
            for key in [
                Key::Letter('j'),
                Key::Letter('m'),
                Key::Letter('x'),
                Key::Space,
            ] {
                assert!(
                    claims_override(key, focus, Appearance::Native),
                    "{key:?} {focus:?}"
                );
            }
        }
        for key in [Key::Letter('j'), Key::Space, Key::Down] {
            assert!(!claims_override(
                key,
                KeyFocus::TextInput,
                Appearance::Native
            ));
        }
        assert!(claims_override(
            Key::Down,
            KeyFocus::EntryList,
            Appearance::Native
        ));
        assert!(!claims_override(
            Key::Down,
            KeyFocus::Sidebar,
            Appearance::Native
        ));
        for key in [
            Key::Option('f'),
            Key::Option('s'),
            Key::Escape,
            Key::Digit(4),
        ] {
            assert!(
                !claims_override(key, KeyFocus::EntryList, Appearance::Native),
                "{key:?}"
            );
        }
    }

    #[test]
    fn held_keys_spend_toggles_and_never_let_space_move_on() {
        assert_eq!(
            [
                verb(Command::ToggleRead, true),
                verb(Command::ToggleStar, true),
                verb(Command::Advance, true)
            ],
            ["toggleReadHeld", "toggleStarHeld", "advanceHeld"]
        );
        assert_eq!(
            [
                verb(Command::NextEntry, true),
                verb(Command::ToggleRead, false),
                verb(Command::FocusEntryList, false)
            ],
            ["nextEntry", "toggleRead", "focusEntryList"]
        );
    }

    #[test]
    fn route_combines_decoding_focus_and_held_rules() {
        use focus::*;
        assert_eq!(
            route(
                qt::KEY_SPACE,
                " ",
                NONE,
                false,
                &[WEB, READER_PANE],
                Appearance::Native
            ),
            Route {
                claims_override: true,
                verb: Some("advance")
            }
        );
        assert_eq!(
            route(
                0x4A,
                "j",
                NONE,
                false,
                &[TEXT, "search"],
                Appearance::Native
            ),
            Route {
                claims_override: false,
                verb: None
            }
        );
        assert_eq!(
            route(
                0x46,
                "f",
                qt::ALT,
                false,
                &[ENTRIES_LIST],
                Appearance::Native
            ),
            Route {
                claims_override: false,
                verb: None
            }
        );
        assert_eq!(
            route(0x4D, "m", NONE, true, &[ENTRIES_LIST], Appearance::Native),
            Route {
                claims_override: true,
                verb: Some("toggleReadHeld")
            }
        );
    }

    #[test]
    fn antiope_keys_map_to_their_commands() {
        use KeyFocus::*;
        for focus in [Sidebar, EntryList, Reader, Window, Control] {
            for d in 0..=9 {
                assert_eq!(
                    command(Key::Digit(d), focus, Appearance::Antiope),
                    Some(Command::PageDigit(d))
                );
            }
            assert_eq!(
                command(Key::Delete, focus, Appearance::Antiope),
                Some(Command::PageCorrect)
            );
            assert_eq!(
                command(Key::Escape, focus, Appearance::Antiope),
                Some(Command::PageCancel)
            );
            assert_eq!(
                command(Key::Option('s'), focus, Appearance::Antiope),
                Some(Command::Sommaire)
            );
            assert_eq!(
                command(Key::Option('g'), focus, Appearance::Antiope),
                Some(Command::Guide)
            );
            assert_eq!(
                command(Key::Question, focus, Appearance::Antiope),
                Some(Command::Guide)
            );
            assert_eq!(
                command(Key::Option('r'), focus, Appearance::Antiope),
                Some(Command::Repetition)
            );
            assert_eq!(
                command(Key::Letter('i'), focus, Appearance::Antiope),
                Some(Command::ToggleImages)
            );
        }
        for focus in [Sidebar, EntryList, Reader, Window] {
            assert_eq!(
                command(Key::Right, focus, Appearance::Antiope),
                Some(Command::NextColumn)
            );
            assert_eq!(
                command(Key::Left, focus, Appearance::Antiope),
                Some(Command::PreviousColumn)
            );
        }
        assert_eq!(command(Key::Right, Control, Appearance::Antiope), None);
        assert_eq!(
            command(Key::Down, Sidebar, Appearance::Antiope),
            Some(Command::NextIndexRow)
        );
        assert_eq!(
            command(Key::Up, Sidebar, Appearance::Antiope),
            Some(Command::PreviousIndexRow)
        );
        assert_eq!(
            command(Key::Down, EntryList, Appearance::Antiope),
            Some(Command::NextEntry)
        );
        assert_eq!(
            command(Key::Up, Reader, Appearance::Antiope),
            Some(Command::PreviousEntry)
        );
        assert_eq!(command(Key::Option('x'), Reader, Appearance::Antiope), None);
    }

    /// Review Focus 2.
    #[test]
    fn digits_in_a_text_field_never_become_page_numbers() {
        for look in [Appearance::Native, Appearance::Antiope] {
            for key in [
                Key::Digit(4),
                Key::Delete,
                Key::Escape,
                Key::Option('s'),
                Key::Option('g'),
                Key::Option('r'),
                Key::Question,
                Key::Letter('i'),
                Key::Left,
                Key::Right,
                Key::Up,
                Key::Down,
            ] {
                assert_eq!(command(key, KeyFocus::TextInput, look), None, "{key:?}");
            }
        }
        assert_eq!(
            route(
                0x34,
                "4",
                0,
                false,
                &[focus::TEXT, "antiopePrompt", focus::READING_PAGE],
                Appearance::Antiope
            ),
            Route {
                claims_override: false,
                verb: None
            }
        );
    }

    #[test]
    fn reading_and_garden_letters_keep_their_meaning_in_antiope() {
        for focus in [KeyFocus::EntryList, KeyFocus::Reader] {
            for l in ['j', 'k', 'm', 's'] {
                assert_eq!(
                    command(Key::Letter(l), focus, Appearance::Antiope),
                    command(Key::Letter(l), focus, Appearance::Native)
                );
            }
            for l in ['r', 'b', 'u', 'o', 'w'] {
                assert_eq!(command(Key::Letter(l), focus, Appearance::Antiope), None);
            }
        }
    }

    #[test]
    fn held_antiope_keys_act_once_per_press() {
        for c in [
            Command::PageDigit(4),
            Command::PageCancel,
            Command::Sommaire,
            Command::Guide,
            Command::Repetition,
            Command::ToggleImages,
            Command::NextColumn,
            Command::PreviousColumn,
        ] {
            assert!(acts_once_per_press(c));
            assert_eq!(verb(c, true), "spent", "{c:?}");
        }
        assert_eq!(
            [
                verb(Command::PageCorrect, true),
                verb(Command::NextIndexRow, true)
            ],
            ["pageCorrect", "nextIndexRow"]
        );
        assert_eq!(
            [
                verb(Command::PageDigit(0), false),
                verb(Command::PageDigit(9), false),
                verb(Command::ToggleImages, false)
            ],
            ["pageDigit0", "pageDigit9", "toggleImages"]
        );
    }

    #[test]
    fn antiope_columns_classify_their_focus() {
        assert_eq!(
            classify(&[focus::ANTIOPE_INDEX, "antiopeView", focus::READING_PAGE]),
            KeyFocus::Sidebar
        );
        assert_eq!(
            classify(&[focus::ITEM, focus::ANTIOPE_ENTRIES, "antiopeView"]),
            KeyFocus::EntryList
        );
        assert_eq!(
            classify(&[focus::ANTIOPE_READER, "antiopeView"]),
            KeyFocus::Reader
        );
        assert_eq!(
            classify(&[focus::WEB, "web", focus::READER_PANE, "antiopeView"]),
            KeyFocus::Reader
        );
        assert_eq!(
            route(
                0x34,
                "4",
                0,
                false,
                &[focus::ANTIOPE_INDEX],
                Appearance::Antiope
            ),
            Route {
                claims_override: true,
                verb: Some("pageDigit4")
            }
        );
        assert_eq!(
            route(
                0x53,
                "s",
                qt::ALT,
                false,
                &[focus::ANTIOPE_ENTRIES],
                Appearance::Antiope
            ),
            Route {
                claims_override: true,
                verb: Some("sommaire")
            }
        );
        assert_eq!(
            route(
                0x34,
                "4",
                0,
                true,
                &[focus::ANTIOPE_INDEX],
                Appearance::Antiope
            ),
            Route {
                claims_override: true,
                verb: Some("spent")
            }
        );
    }
}
