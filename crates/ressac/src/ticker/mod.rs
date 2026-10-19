//! `ressac ticker`: one unread entry at a time for the tmux status bar.

mod line;
mod pick;

pub use line::{DEFAULT_WIDTH, line, parse_token, token};
pub use pick::{Rng, SLOT_SECONDS, TickerState, next, slot};
