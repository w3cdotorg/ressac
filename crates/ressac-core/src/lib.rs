//! Ressac core: storage, fetching, diagnosis and OPML for the Ressac feed reader.

pub mod error;
pub mod model;
pub mod opml;
pub mod parse;
pub mod store;
pub mod url_key;

pub use error::{Error, Result};
