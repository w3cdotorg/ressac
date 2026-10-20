//! The command line: clap's definitions and their conversions to core types.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use ressac_core::edition::{CAP_WEEK, CAP_WIRE};
use ressac_core::model::{CadenceGroup, Decision, FeedId, FolderId};
use ressac_core::store::pages::PageKind;

#[derive(Parser)]
#[command(
    name = "ressac",
    version,
    about = "Tidy your RSS subscriptions, then read them calmly"
)]
pub struct Cli {
    /// Database file [default: ~/Library/Application Support/Ressac/ressac.sqlite]
    #[arg(long, env = "RESSAC_DB", global = true)]
    pub db: Option<PathBuf>,
    /// Print one JSON object per line instead of human-readable output
    #[arg(long, global = true)]
    pub json: bool,
    /// Use ASCII status glyphs instead of emoji
    #[arg(long, global = true)]
    pub ascii: bool,
    /// Also write logs to stderr
    #[arg(short, long, global = true)]
    pub verbose: bool,
    /// What to do; without one, `ressac` opens the reader
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Import feeds and folders from an OPML file
    Import {
        file: PathBuf,
        /// Show what would be imported without writing anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Print the sea chart's data, or import or export its themes and territories
    Chart {
        #[command(subcommand)]
        action: Option<ChartCommand>,
    },
    /// Add a feed by its address, or by its site's: the first feed the page announces
    Add {
        /// A feed address or a site address (https:// is assumed when there is no scheme)
        address: String,
        /// Put the feed in this theme (a theme key or name) instead of its site's
        #[arg(long, value_name = "THEME")]
        theme: Option<String>,
    },
    /// Fetch every feed (or only some) and update its health
    Fetch {
        /// Also fetch feeds fetched in the last 30 minutes
        #[arg(long)]
        force: bool,
        /// Only fetch this feed (repeatable); implies --force
        #[arg(long = "feed", value_name = "ID")]
        feeds: Vec<FeedId>,
        /// Only fetch the feeds of this folder and its subfolders; implies --force
        #[arg(long, value_name = "ID", conflicts_with = "feeds")]
        folder: Option<FolderId>,
    },
    /// Fetch, look up archives of dead feeds, and print a health report
    Check {
        /// Also fetch feeds fetched in the last 30 minutes
        #[arg(long)]
        force: bool,
    },
    /// Fetch an entry's web page and store its full article
    Extract {
        /// The entry's id
        entry_id: i64,
    },
    /// Triage feeds in a full-screen garden, or decide without it
    Garden {
        #[command(subcommand)]
        action: Option<GardenCommand>,
    },
    /// Read your feeds in the terminal (the default when no command is given)
    Read,
    /// Export active feeds (or the cemetery) as OPML
    Export {
        /// Write to this file instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Export buried feeds, with their Wayback links
        #[arg(long)]
        cemetery: bool,
    },
    /// Open the database and apply pending schema migrations, nothing else
    Migrate,
    /// List the teletext page numbers, or give a folder or a feed another one
    Page {
        #[command(subcommand)]
        action: PageCommand,
    },
    /// Build or show the morning edition
    Edition {
        #[command(subcommand)]
        action: EditionCommand,
    },
    /// Change one feed's settings
    Feed {
        #[command(subcommand)]
        action: FeedCommand,
    },
    /// Print one unread headline for the tmux status bar (called every 15 s by tmux)
    Ticker {
        /// Wrap the line in tmux style markup, with a click range
        #[arg(long)]
        tmux: bool,
        /// Maximum width of the line, in terminal columns
        #[arg(long, default_value_t = crate::ticker::DEFAULT_WIDTH)]
        width: usize,
        #[command(subcommand)]
        action: Option<TickerCommand>,
    },
}

#[derive(Subcommand)]
pub enum TickerCommand {
    /// Mark the clicked entry read and open it in the browser
    Open {
        /// The click token from the ticker line (rs followed by the entry id)
        token: String,
    },
}

#[derive(Subcommand)]
pub enum GardenCommand {
    /// Keep, repair or bury one feed (keep restores a buried feed)
    Decide {
        feed_id: FeedId,
        #[arg(value_enum)]
        decision: DecisionArg,
    },
    /// Undo the last decision (the stack is shared with the full-screen garden)
    Undo,
    /// Apply every safe repair (https upgrades and permanent redirects)
    ApplySafe,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum DecisionArg {
    Keep,
    Repair,
    Bury,
}

#[derive(Subcommand)]
pub enum PageCommand {
    /// Every page, reserved ones first: page, kind, id, title
    List,
    /// Give a folder or a feed another page (110 to 799, not reserved, not taken)
    Set {
        #[arg(value_enum)]
        kind: PageKindArg,
        /// The folder's or the feed's id
        id: i64,
        page: u16,
    },
}

#[derive(Subcommand)]
pub enum EditionCommand {
    /// Refresh every feed, then compose this morning's edition (once a day, from 04:00)
    Build {
        /// Entries per weekly feed
        #[arg(long, value_name = "N", default_value_t = CAP_WEEK)]
        cap_week: usize,
        /// Entries per daily feed
        #[arg(long, value_name = "N", default_value_t = CAP_WIRE)]
        cap_wire: usize,
        /// Replace this morning's edition, over the same window
        #[arg(long)]
        rebuild: bool,
    },
    /// Print this morning's edition by section
    Show,
}

#[derive(Subcommand)]
pub enum FeedCommand {
    /// Force a feed's rhythm group, or give it back to the automatic one
    Cadence {
        /// The feed's id
        feed_id: FeedId,
        #[arg(value_enum)]
        group: CadenceArg,
    },
    /// Put a feed in a theme (a sea on the chart), or take it out with "none"
    Theme {
        /// The feed's id
        feed_id: FeedId,
        /// A theme key or name (see ressac chart), or "none"
        key: String,
    },
    /// Move a feed to a folder (last position, lowest free page there), or back to the root
    Move {
        /// The feed's id
        feed_id: FeedId,
        /// A folder id, or "root" for the top level
        folder: String,
    },
}

#[derive(Subcommand)]
pub enum ChartCommand {
    /// Import themes, feed themes and territories from a JSON file
    Import {
        file: PathBuf,
        /// Report what would change without writing anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Write the database's themes, feed themes and territories as an import file
    Export {
        /// Write to this file instead of standard output
        #[arg(long, short, value_name = "FILE")]
        output: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum CadenceArg {
    Auto,
    Daily,
    Weekly,
    Monthly,
    Rare,
    Silent,
}

impl CadenceArg {
    /// `None` for `auto`: the override is cleared.
    pub fn group(self) -> Option<CadenceGroup> {
        match self {
            CadenceArg::Auto => None,
            CadenceArg::Daily => Some(CadenceGroup::Daily),
            CadenceArg::Weekly => Some(CadenceGroup::Weekly),
            CadenceArg::Monthly => Some(CadenceGroup::Monthly),
            CadenceArg::Rare => Some(CadenceGroup::Rare),
            CadenceArg::Silent => Some(CadenceGroup::Silent),
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum PageKindArg {
    Feed,
    Folder,
}

impl From<PageKindArg> for PageKind {
    fn from(k: PageKindArg) -> Self {
        match k {
            PageKindArg::Feed => PageKind::Feed,
            PageKindArg::Folder => PageKind::Folder,
        }
    }
}

impl From<DecisionArg> for Decision {
    fn from(d: DecisionArg) -> Self {
        match d {
            DecisionArg::Keep => Decision::Keep,
            DecisionArg::Repair => Decision::Repair,
            DecisionArg::Bury => Decision::Bury,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    /// `ressac garden` alone still opens the TUI. It cannot run under `cargo test`
    /// (crossterm falls back to /dev/tty), so the parse is checked instead.
    #[test]
    fn bare_garden_still_opens_the_tui() {
        let bare = Cli::try_parse_from(["ressac", "garden"]).unwrap();
        assert!(matches!(
            bare.command,
            Some(Command::Garden { action: None })
        ));
        let decide =
            Cli::try_parse_from(["ressac", "--json", "garden", "decide", "3", "bury"]).unwrap();
        assert!(decide.json);
        assert!(matches!(
            decide.command,
            Some(Command::Garden {
                action: Some(GardenCommand::Decide {
                    feed_id: 3,
                    decision: DecisionArg::Bury
                })
            })
        ));
        let safe = Cli::try_parse_from(["ressac", "garden", "apply-safe"]).unwrap();
        assert!(matches!(
            safe.command,
            Some(Command::Garden {
                action: Some(GardenCommand::ApplySafe)
            })
        ));
    }

    #[test]
    fn feed_is_repeatable_and_exclusive_with_folder() {
        let cli = Cli::try_parse_from(["ressac", "fetch", "--feed", "1", "--feed", "4"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Fetch { ref feeds, folder: None, force: false }) if *feeds == vec![1, 4]
        ));
        let conflict = Cli::try_parse_from(["ressac", "fetch", "--feed", "1", "--folder", "2"])
            .err()
            .unwrap();
        assert_eq!(conflict.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn page_set_parses_its_kind_id_and_page() {
        let set = Cli::try_parse_from(["ressac", "page", "set", "folder", "4", "450"]).unwrap();
        assert!(matches!(
            set.command,
            Some(Command::Page {
                action: PageCommand::Set {
                    kind: PageKindArg::Folder,
                    id: 4,
                    page: 450
                }
            })
        ));
        for bad in [
            &["ressac", "page"][..],
            &["ressac", "page", "set", "entry", "1", "300"],
            &["ressac", "page", "set", "feed", "1", "70000"],
            &["ressac", "page", "set", "feed", "1", "-3"],
        ] {
            assert!(Cli::try_parse_from(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn no_subcommand_and_read_both_open_the_reader() {
        assert!(Cli::try_parse_from(["ressac"]).unwrap().command.is_none());
        let flags =
            Cli::try_parse_from(["ressac", "--db", "/tmp/x.sqlite", "--json", "-v"]).unwrap();
        assert!(flags.command.is_none() && flags.json && flags.verbose);
        assert_eq!(
            flags.db.as_deref(),
            Some(std::path::Path::new("/tmp/x.sqlite"))
        );
        assert!(matches!(
            Cli::try_parse_from(["ressac", "read"]).unwrap().command,
            Some(Command::Read)
        ));
    }

    #[test]
    fn ticker_parses_its_flags_and_the_open_token() {
        match Cli::try_parse_from(["ressac", "ticker", "--tmux"])
            .unwrap()
            .command
        {
            Some(Command::Ticker {
                tmux: true,
                width: 70,
                action: None,
            }) => {}
            _ => panic!("ticker --tmux"),
        }
        match Cli::try_parse_from(["ressac", "ticker", "open", "rs12"])
            .unwrap()
            .command
        {
            Some(Command::Ticker {
                action: Some(TickerCommand::Open { token }),
                ..
            }) => assert_eq!(token, "rs12"),
            _ => panic!("ticker open"),
        }
    }
}
