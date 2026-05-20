//! GDSII toolkit.
//!
//! Provides a high-level toolkit for efficiently working with [GDSII] data.
//!
//! [GDSII]: <https://en.wikipedia.org/wiki/GDSII>
pub mod float;
pub mod parser;
pub mod reader;
pub mod types;

/* Re-exports */
pub use float::*;
pub use types::*;
