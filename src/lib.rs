// The README is the crate-level documentation, so `cargo test` compiles its
// examples and they cannot drift from the API.
#![doc = include_str!("../README.md")]
//!
//! # Modules
//!
//! - [`parser`]: streaming event parser ([`GdsParser`](parser::GdsParser),
//!   [`GdsEvent`](parser::GdsEvent), element types)
//! - [`writer`]: event-to-bytes writer ([`GdsWriter`](writer::GdsWriter))
//! - [`reader`]: low-level record iterator ([`RecordIter`](reader::RecordIter))
//! - [`types`]: wire-format type definitions ([`RecordType`], [`DataType`],
//!   [`GdsPoint`])
//! - [`float`]: GDS base-16 float encoding ([`GdsEightByteReal`],
//!   [`GdsFourByteReal`])
//!
//! Elements whose vertices span several XY records are the one place parsing
//! allocates; see [`XyCoords`](parser::XyCoords).
pub mod float;
pub mod parser;
pub mod reader;
pub mod types;
pub mod writer;

/* Re-exports */
pub use float::*;
pub use types::*;
pub use zerocopy::big_endian::{I16, I32, U16};
