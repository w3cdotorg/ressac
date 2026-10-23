//! The Ressac desktop app's logic, without Qt: every model, state machine and pure renderer
//! the KDE app (`apps/linux`) draws. No I/O except through `ressac-core`; time and time zone
//! are always passed in.

pub mod entries;
pub mod launch;
pub mod settings;

/// The chrono the app's types use, so the KDE bridges pass the clock in with the same chrono.
pub use chrono;

#[cfg(test)]
mod test_db;
