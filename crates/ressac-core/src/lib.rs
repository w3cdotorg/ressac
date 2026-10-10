//! Ressac core: storage, fetching, diagnosis and OPML for the Ressac feed reader.

pub mod error;
pub mod model;
pub mod store;
pub mod url_key;

pub use error::{Error, Result};
