//! `ressac ticker`: one unread entry at a time for the tmux status bar.

mod pick;

pub use pick::{Rng, SLOT_SECONDS, TickerState, next, slot};
