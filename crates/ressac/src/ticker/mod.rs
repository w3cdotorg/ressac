//! `ressac ticker`: one unread entry at a time for the tmux status bar.

mod line;
mod pick;
pub mod state_file;

pub use line::{DEFAULT_WIDTH, line, parse_token};
pub use pick::{Rng, TickerState, next, slot};
