//! The command line: clap's definitions and their conversions to core types.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use ressac_core::model::{Decision, FeedId, FolderId};
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
    #[command(subcommand)]
    pub command: Command,
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
        assert!(matches!(bare.command, Command::Garden { action: None }));
        let decide =
            Cli::try_parse_from(["ressac", "--json", "garden", "decide", "3", "bury"]).unwrap();
        assert!(decide.json);
        assert!(matches!(
            decide.command,
            Command::Garden {
                action: Some(GardenCommand::Decide {
                    feed_id: 3,
                    decision: DecisionArg::Bury
                })
            }
        ));
        let safe = Cli::try_parse_from(["ressac", "garden", "apply-safe"]).unwrap();
        assert!(matches!(
            safe.command,
            Command::Garden {
                action: Some(GardenCommand::ApplySafe)
            }
        ));
    }

    #[test]
    fn feed_is_repeatable_and_exclusive_with_folder() {
        let cli = Cli::try_parse_from(["ressac", "fetch", "--feed", "1", "--feed", "4"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Fetch { ref feeds, folder: None, force: false } if *feeds == vec![1, 4]
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
            Command::Page {
                action: PageCommand::Set {
                    kind: PageKindArg::Folder,
                    id: 4,
                    page: 450
                }
            }
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
}
