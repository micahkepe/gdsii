//! Fast, zero-copy, streaming [GDSII] parser and writer.
//!
//! Parses GDSII binary layout files into a SAX-style event stream with no heap
//! allocation during parsing. All borrowed data references the original input
//! buffer. The writer serializes events back to spec-compliant GDSII bytes.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use gdsii::parser::{GdsParser, GdsEvent, Element};
//!
//! let data = std::fs::read("layout.gds").unwrap();
//! for event in GdsParser::new(&data) {
//!     match event.unwrap() {
//!         GdsEvent::Element(Element::Boundary(b)) => {
//!             println!("layer={}, points={}", b.layer, b.xy.len() / 2);
//!         }
//!         _ => {}
//!     }
//! }
//! ```
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
//! [GDSII]: <https://en.wikipedia.org/wiki/GDSII>
pub mod float;
pub mod parser;
pub mod reader;
pub mod types;
pub mod writer;

/* Re-exports */
pub use float::*;
pub use types::*;
pub use zerocopy::big_endian::{I16, I32, U16};
