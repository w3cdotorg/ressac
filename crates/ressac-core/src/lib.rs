//! Ressac core: storage, fetching, diagnosis and OPML for the Ressac feed reader.

pub mod clock;
pub mod discover;
pub mod edition;
pub mod error;
pub mod extract;
pub mod garden;
pub mod health;
pub mod logging;
pub mod model;
pub mod opml;
pub mod parse;
pub mod paths;
pub mod service;
pub mod source;
pub mod store;
pub mod url_key;
pub mod wayback;

pub use error::{Error, Result};
