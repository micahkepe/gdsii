//! Zero-copy, streaming GDS event parser.
//!
//! Wraps [`RecordIter`] and yields high-level [`GdsEvent`]s -- one per logical GDS unit
//! (library header, structure header, complete element, property, structural delimiters).
//! All borrowed data references the original input buffer; no allocations occur during parsing.

use zerocopy::big_endian::{I16, I32};

use crate::reader::{BodyParseError, Record, RecordBody, RecordIter};
use crate::types::RecordType;

/// Errors that can occur during GDS stream parsing.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// Record type does not match what the grammar expects at this position.
    #[error("unexpected record {found:?} in {context}")]
    UnexpectedRecord {
        found: RecordType,
        context: &'static str,
    },
    /// Record stream ended before a required record was found.
    #[error("unexpected end of records in {context}")]
    UnexpectedEof { context: &'static str },
    /// Record body data type does not match the expected variant.
    #[error("{record_type:?} body: expected {expected}")]
    WrongBodyType {
        record_type: RecordType,
        expected: &'static str,
    },
    /// Underlying record body could not be parsed.
    #[error(transparent)]
    Body(#[from] BodyParseError),
}

// ==============================================================================
// AST types
// ==============================================================================

/// High-level event emitted by [`GdsParser`].
///
/// Events follow a strict nesting order: `LibraryBegin` is always first,
/// followed by zero or more structure/element groups, then `LibraryEnd`.
#[derive(Debug)]
pub enum GdsEvent<'data> {
    /// Accumulated library header (HEADER through UNITS).
    LibraryBegin(LibraryBegin<'data>),
    /// Start of a structure (cell).
    StructureBegin(StructureBegin<'data>),
    /// Complete geometric or reference element.
    Element(Element<'data>),
    /// Property attached to the preceding element.
    Property(Property<'data>),
    /// End of the current structure.
    StructureEnd,
    /// End of the library (final event).
    LibraryEnd,
}

/// Library-level metadata from HEADER, BGNLIB, LIBNAME, and UNITS records.
#[derive(Debug)]
pub struct LibraryBegin<'data> {
    pub version: i16,
    /// Twelve i16 values: modification time then access time
    /// (year, month, day, hour, minute, second × 2).
    pub timestamps: &'data [I16],
    pub lib_name: &'data str,
    pub db_in_user: f64,
    pub db_in_meters: f64,
    pub reflibs: Option<&'data str>,
    pub fonts: Option<&'data str>,
    pub attrtable: Option<&'data str>,
    pub generations: Option<i16>,
}

/// Structure (cell) header from BGNSTR and STRNAME records.
#[derive(Debug)]
pub struct StructureBegin<'data> {
    pub timestamps: &'data [I16],
    pub name: &'data str,
}

/// PROPATTR/PROPVALUE pair attached to an element.
#[derive(Debug)]
pub struct Property<'data> {
    pub attr: i16,
    pub value: &'data str,
}

/// Transformation flags from STRANS, MAG, and ANGLE records.
#[derive(Debug, Clone, Copy)]
pub struct Strans {
    pub reflection: bool,
    pub abs_mag: bool,
    pub abs_angle: bool,
    pub mag: Option<f64>,
    pub angle: Option<f64>,
}

/// Parsed GDS element (geometry or reference).
#[derive(Debug)]
pub enum Element<'data> {
    Boundary(Boundary<'data>),
    Path(Path<'data>),
    Sref(Sref<'data>),
    Aref(Aref<'data>),
    Text(Text<'data>),
    Node(Node<'data>),
    Box(GdsBox<'data>),
}

/// Filled polygon element.
#[derive(Debug)]
pub struct Boundary<'data> {
    pub elflags: Option<u16>,
    pub plex: Option<i32>,
    pub layer: i16,
    pub datatype: i16,
    pub xy: &'data [I32],
}

/// Wire-like path element with optional width and endpoint style.
#[derive(Debug)]
pub struct Path<'data> {
    pub elflags: Option<u16>,
    pub plex: Option<i32>,
    pub layer: i16,
    pub datatype: i16,
    pub pathtype: Option<i16>,
    pub width: Option<i32>,
    pub xy: &'data [I32],
}

/// Structure reference (instance placement).
#[derive(Debug)]
pub struct Sref<'data> {
    pub elflags: Option<u16>,
    pub plex: Option<i32>,
    pub sname: &'data str,
    pub strans: Option<Strans>,
    pub xy: &'data [I32],
}

/// Array reference (repeated instance placement in a grid).
#[derive(Debug)]
pub struct Aref<'data> {
    pub elflags: Option<u16>,
    pub plex: Option<i32>,
    pub sname: &'data str,
    pub strans: Option<Strans>,
    pub colrow: (i16, i16),
    pub xy: &'data [I32],
}

/// Text annotation element.
#[derive(Debug)]
pub struct Text<'data> {
    pub elflags: Option<u16>,
    pub plex: Option<i32>,
    pub layer: i16,
    pub texttype: i16,
    pub presentation: Option<u16>,
    pub pathtype: Option<i16>,
    pub width: Option<i32>,
    pub strans: Option<Strans>,
    pub xy: &'data [I32],
    pub string: &'data str,
}

/// Electrical net node element.
#[derive(Debug)]
pub struct Node<'data> {
    pub elflags: Option<u16>,
    pub plex: Option<i32>,
    pub layer: i16,
    pub nodetype: i16,
    pub xy: &'data [I32],
}

/// Rectangular box element. Named `GdsBox` to avoid shadowing `std::boxed::Box`.
#[derive(Debug)]
pub struct GdsBox<'data> {
    pub elflags: Option<u16>,
    pub plex: Option<i32>,
    pub layer: i16,
    pub boxtype: i16,
    pub xy: &'data [I32],
}

// ==============================================================================
// Body extraction helpers
// ==============================================================================

fn extract_i16(body: &RecordBody, record_type: RecordType) -> Result<i16, ParseError> {
    match body {
        RecordBody::TwoByteSignedInt(s) if !s.is_empty() => Ok(s[0].get()),
        _ => Err(ParseError::WrongBodyType {
            record_type,
            expected: "TwoByteSignedInt with ≥1 element",
        }),
    }
}

fn extract_i32(body: &RecordBody, record_type: RecordType) -> Result<i32, ParseError> {
    match body {
        RecordBody::FourByteSignedInt(s) if !s.is_empty() => Ok(s[0].get()),
        _ => Err(ParseError::WrongBodyType {
            record_type,
            expected: "FourByteSignedInt with ≥1 element",
        }),
    }
}

fn extract_u16(body: &RecordBody, record_type: RecordType) -> Result<u16, ParseError> {
    match body {
        RecordBody::BitArray(s) if !s.is_empty() => Ok(s[0].get()),
        _ => Err(ParseError::WrongBodyType {
            record_type,
            expected: "BitArray with ≥1 element",
        }),
    }
}

fn extract_f64(body: &RecordBody, record_type: RecordType) -> Result<f64, ParseError> {
    match body {
        RecordBody::EightByteReal(s) if !s.is_empty() => Ok(f64::from(s[0])),
        _ => Err(ParseError::WrongBodyType {
            record_type,
            expected: "EightByteReal with ≥1 element",
        }),
    }
}

fn extract_two_f64s(body: &RecordBody, record_type: RecordType) -> Result<(f64, f64), ParseError> {
    match body {
        RecordBody::EightByteReal(s) if s.len() >= 2 => Ok((f64::from(s[0]), f64::from(s[1]))),
        _ => Err(ParseError::WrongBodyType {
            record_type,
            expected: "EightByteReal with ≥2 elements",
        }),
    }
}

const fn extract_i16_slice<'a>(
    body: &RecordBody<'a>,
    record_type: RecordType,
) -> Result<&'a [I16], ParseError> {
    match body {
        RecordBody::TwoByteSignedInt(s) => Ok(s),
        _ => Err(ParseError::WrongBodyType {
            record_type,
            expected: "TwoByteSignedInt",
        }),
    }
}

const fn extract_i32_slice<'a>(
    body: &RecordBody<'a>,
    record_type: RecordType,
) -> Result<&'a [I32], ParseError> {
    match body {
        RecordBody::FourByteSignedInt(s) => Ok(s),
        _ => Err(ParseError::WrongBodyType {
            record_type,
            expected: "FourByteSignedInt",
        }),
    }
}

const fn extract_str<'a>(
    body: &RecordBody<'a>,
    record_type: RecordType,
) -> Result<&'a str, ParseError> {
    match body {
        RecordBody::AsciiString(s) => Ok(s),
        _ => Err(ParseError::WrongBodyType {
            record_type,
            expected: "AsciiString",
        }),
    }
}

/// Transition states for the parser state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Start,
    Library,
    Structure,
    /// Emitting PROPATTR/PROPVALUE pairs after an element, before ENDEL.
    Properties,
    Done,
}

/// Zero-copy, streaming event parser over GDS record bytes.
///
/// Implements [`Iterator`] yielding `Result<GdsEvent, ParseError>`.
/// Fuses on first error so that subsequent calls to `next` return `None`.
#[derive(Debug)]
pub struct GdsParser<'data> {
    records: RecordIter<'data>,
    state: State,
    peeked: Option<Record<'data>>,
}

impl<'data> GdsParser<'data> {
    /// Creates a parser over the given GDS byte buffer.
    #[must_use]
    pub fn new<B: AsRef<[u8]> + ?Sized>(data: &'data B) -> Self {
        Self {
            records: RecordIter::new(data),
            state: State::Start,
            peeked: None,
        }
    }

    fn next_record(&mut self, context: &'static str) -> Result<Record<'data>, ParseError> {
        match self.peeked.take().map(Ok).or_else(|| self.records.next()) {
            Some(r) => Ok(r?),
            None => Err(ParseError::UnexpectedEof { context }),
        }
    }

    fn unpeek(&mut self, record: Record<'data>) {
        debug_assert!(self.peeked.is_none(), "double unpeek");
        self.peeked = Some(record);
    }

    fn expect_record(
        &mut self,
        expected: RecordType,
        context: &'static str,
    ) -> Result<Record<'data>, ParseError> {
        let r = self.next_record(context)?;
        if r.header.record_type() == expected {
            Ok(r)
        } else {
            Err(ParseError::UnexpectedRecord {
                found: r.header.record_type(),
                context,
            })
        }
    }

    fn try_record(&mut self, expected: RecordType) -> Result<Option<Record<'data>>, ParseError> {
        match self.peeked.take().map(Ok).or_else(|| self.records.next()) {
            Some(r) => {
                let r = r?;
                if r.header.record_type() == expected {
                    Ok(Some(r))
                } else {
                    self.unpeek(r);
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    fn parse_elflags_plex(&mut self) -> Result<(Option<u16>, Option<i32>), ParseError> {
        let elflags = self
            .try_record(RecordType::Elflags)?
            .map(|r| extract_u16(&r.body, RecordType::Elflags))
            .transpose()?;
        let plex = self
            .try_record(RecordType::Plex)?
            .map(|r| extract_i32(&r.body, RecordType::Plex))
            .transpose()?;
        Ok((elflags, plex))
    }

    fn parse_strans(&mut self) -> Result<Option<Strans>, ParseError> {
        let Some(strans_rec) = self.try_record(RecordType::Strans)? else {
            return Ok(None);
        };
        let flags = extract_u16(&strans_rec.body, RecordType::Strans)?;
        let mag = self
            .try_record(RecordType::Mag)?
            .map(|r| extract_f64(&r.body, RecordType::Mag))
            .transpose()?;
        let angle = self
            .try_record(RecordType::Angle)?
            .map(|r| extract_f64(&r.body, RecordType::Angle))
            .transpose()?;
        Ok(Some(Strans {
            reflection: flags & 0x8000 != 0,
            abs_mag: flags & 0x0004 != 0,
            abs_angle: flags & 0x0002 != 0,
            mag,
            angle,
        }))
    }

    /// Consume PROPATTR/PROPVALUE pairs and ENDEL, transitioning to Structure state.
    /// If the next record is PROPATTR, transition to Properties state for streaming.
    /// Otherwise expect ENDEL directly.
    fn finish_element(&mut self) -> Result<(), ParseError> {
        let rec = self.next_record("element end")?;
        match rec.header.record_type() {
            RecordType::Endel => {
                self.state = State::Structure;
                Ok(())
            }
            RecordType::Propattr => {
                self.unpeek(rec);
                self.state = State::Properties;
                Ok(())
            }
            other => Err(ParseError::UnexpectedRecord {
                found: other,
                context: "element end (expected ENDEL or PROPATTR)",
            }),
        }
    }

    fn parse_library_begin(&mut self) -> Result<GdsEvent<'data>, ParseError> {
        let header_rec = self.expect_record(RecordType::Header, "library begin")?;
        let version = extract_i16(&header_rec.body, RecordType::Header)?;

        let bgnlib_rec = self.expect_record(RecordType::BgnLib, "library begin")?;
        let timestamps = extract_i16_slice(&bgnlib_rec.body, RecordType::BgnLib)?;

        let libname_rec = self.expect_record(RecordType::LibName, "library begin")?;
        let lib_name = extract_str(&libname_rec.body, RecordType::LibName)?;

        // Consume optional library-level records before UNITS.
        let mut reflibs = None;
        let mut fonts = None;
        let mut attrtable = None;
        let mut generations = None;
        loop {
            let rec = self.next_record("library begin")?;
            match rec.header.record_type() {
                RecordType::Units => {
                    let (db_in_user, db_in_meters) =
                        extract_two_f64s(&rec.body, RecordType::Units)?;
                    self.state = State::Library;
                    return Ok(GdsEvent::LibraryBegin(LibraryBegin {
                        version,
                        timestamps,
                        lib_name,
                        db_in_user,
                        db_in_meters,
                        reflibs,
                        fonts,
                        attrtable,
                        generations,
                    }));
                }
                RecordType::Reflibs => {
                    reflibs = Some(extract_str(&rec.body, RecordType::Reflibs)?);
                }
                RecordType::Fonts => {
                    fonts = Some(extract_str(&rec.body, RecordType::Fonts)?);
                }
                RecordType::Attrtable => {
                    attrtable = Some(extract_str(&rec.body, RecordType::Attrtable)?);
                }
                RecordType::Generations => {
                    generations = Some(extract_i16(&rec.body, RecordType::Generations)?);
                }
                RecordType::Format | RecordType::Mask | RecordType::EndMasks => {}
                other => {
                    return Err(ParseError::UnexpectedRecord {
                        found: other,
                        context: "library begin (before UNITS)",
                    });
                }
            }
        }
    }

    fn parse_structure_begin(
        &mut self,
        bgnstr: &Record<'data>,
    ) -> Result<GdsEvent<'data>, ParseError> {
        let timestamps = extract_i16_slice(&bgnstr.body, RecordType::BgnStr)?;
        let strname_rec = self.expect_record(RecordType::StrName, "structure begin")?;
        let name = extract_str(&strname_rec.body, RecordType::StrName)?;
        self.state = State::Structure;
        Ok(GdsEvent::StructureBegin(StructureBegin {
            timestamps,
            name,
        }))
    }

    fn parse_boundary(&mut self) -> Result<GdsEvent<'data>, ParseError> {
        let (elflags, plex) = self.parse_elflags_plex()?;
        let layer = extract_i16(
            &self.expect_record(RecordType::Layer, "Boundary")?.body,
            RecordType::Layer,
        )?;
        let datatype = extract_i16(
            &self.expect_record(RecordType::Datatype, "Boundary")?.body,
            RecordType::Datatype,
        )?;
        let xy = extract_i32_slice(
            &self.expect_record(RecordType::Xy, "Boundary")?.body,
            RecordType::Xy,
        )?;
        self.finish_element()?;
        Ok(GdsEvent::Element(Element::Boundary(Boundary {
            elflags,
            plex,
            layer,
            datatype,
            xy,
        })))
    }

    fn parse_path(&mut self) -> Result<GdsEvent<'data>, ParseError> {
        let (elflags, plex) = self.parse_elflags_plex()?;
        let layer = extract_i16(
            &self.expect_record(RecordType::Layer, "Path")?.body,
            RecordType::Layer,
        )?;
        let datatype = extract_i16(
            &self.expect_record(RecordType::Datatype, "Path")?.body,
            RecordType::Datatype,
        )?;
        let pathtype = self
            .try_record(RecordType::Pathtype)?
            .map(|r| extract_i16(&r.body, RecordType::Pathtype))
            .transpose()?;
        let width = self
            .try_record(RecordType::Width)?
            .map(|r| extract_i32(&r.body, RecordType::Width))
            .transpose()?;
        let xy = extract_i32_slice(
            &self.expect_record(RecordType::Xy, "Path")?.body,
            RecordType::Xy,
        )?;
        self.finish_element()?;
        Ok(GdsEvent::Element(Element::Path(Path {
            elflags,
            plex,
            layer,
            datatype,
            pathtype,
            width,
            xy,
        })))
    }

    fn parse_sref(&mut self) -> Result<GdsEvent<'data>, ParseError> {
        let (elflags, plex) = self.parse_elflags_plex()?;
        let sname = extract_str(
            &self.expect_record(RecordType::Sname, "Sref")?.body,
            RecordType::Sname,
        )?;
        let strans = self.parse_strans()?;
        let xy = extract_i32_slice(
            &self.expect_record(RecordType::Xy, "Sref")?.body,
            RecordType::Xy,
        )?;
        self.finish_element()?;
        Ok(GdsEvent::Element(Element::Sref(Sref {
            elflags,
            plex,
            sname,
            strans,
            xy,
        })))
    }

    fn parse_aref(&mut self) -> Result<GdsEvent<'data>, ParseError> {
        let (elflags, plex) = self.parse_elflags_plex()?;
        let sname = extract_str(
            &self.expect_record(RecordType::Sname, "Aref")?.body,
            RecordType::Sname,
        )?;
        let strans = self.parse_strans()?;
        let colrow_rec = self.expect_record(RecordType::Colrow, "Aref")?;
        let colrow_slice = extract_i16_slice(&colrow_rec.body, RecordType::Colrow)?;
        if colrow_slice.len() < 2 {
            return Err(ParseError::WrongBodyType {
                record_type: RecordType::Colrow,
                expected: "TwoByteSignedInt with ≥2 elements",
            });
        }
        let colrow = (colrow_slice[0].get(), colrow_slice[1].get());
        let xy = extract_i32_slice(
            &self.expect_record(RecordType::Xy, "Aref")?.body,
            RecordType::Xy,
        )?;
        self.finish_element()?;
        Ok(GdsEvent::Element(Element::Aref(Aref {
            elflags,
            plex,
            sname,
            strans,
            colrow,
            xy,
        })))
    }

    fn parse_text(&mut self) -> Result<GdsEvent<'data>, ParseError> {
        let (elflags, plex) = self.parse_elflags_plex()?;
        let layer = extract_i16(
            &self.expect_record(RecordType::Layer, "Text")?.body,
            RecordType::Layer,
        )?;
        let texttype = extract_i16(
            &self.expect_record(RecordType::TextType, "Text")?.body,
            RecordType::TextType,
        )?;
        let presentation = self
            .try_record(RecordType::Presentation)?
            .map(|r| extract_u16(&r.body, RecordType::Presentation))
            .transpose()?;
        let pathtype = self
            .try_record(RecordType::Pathtype)?
            .map(|r| extract_i16(&r.body, RecordType::Pathtype))
            .transpose()?;
        let width = self
            .try_record(RecordType::Width)?
            .map(|r| extract_i32(&r.body, RecordType::Width))
            .transpose()?;
        let strans = self.parse_strans()?;
        let xy = extract_i32_slice(
            &self.expect_record(RecordType::Xy, "Text")?.body,
            RecordType::Xy,
        )?;
        let string = extract_str(
            &self.expect_record(RecordType::String, "Text")?.body,
            RecordType::String,
        )?;
        self.finish_element()?;
        Ok(GdsEvent::Element(Element::Text(Text {
            elflags,
            plex,
            layer,
            texttype,
            presentation,
            pathtype,
            width,
            strans,
            xy,
            string,
        })))
    }

    fn parse_node(&mut self) -> Result<GdsEvent<'data>, ParseError> {
        let (elflags, plex) = self.parse_elflags_plex()?;
        let layer = extract_i16(
            &self.expect_record(RecordType::Layer, "Node")?.body,
            RecordType::Layer,
        )?;
        let nodetype = extract_i16(
            &self.expect_record(RecordType::Nodetype, "Node")?.body,
            RecordType::Nodetype,
        )?;
        let xy = extract_i32_slice(
            &self.expect_record(RecordType::Xy, "Node")?.body,
            RecordType::Xy,
        )?;
        self.finish_element()?;
        Ok(GdsEvent::Element(Element::Node(Node {
            elflags,
            plex,
            layer,
            nodetype,
            xy,
        })))
    }

    fn parse_gds_box(&mut self) -> Result<GdsEvent<'data>, ParseError> {
        let (elflags, plex) = self.parse_elflags_plex()?;
        let layer = extract_i16(
            &self.expect_record(RecordType::Layer, "Box")?.body,
            RecordType::Layer,
        )?;
        let boxtype = extract_i16(
            &self.expect_record(RecordType::BoxType, "Box")?.body,
            RecordType::BoxType,
        )?;
        let xy = extract_i32_slice(
            &self.expect_record(RecordType::Xy, "Box")?.body,
            RecordType::Xy,
        )?;
        self.finish_element()?;
        Ok(GdsEvent::Element(Element::Box(GdsBox {
            elflags,
            plex,
            layer,
            boxtype,
            xy,
        })))
    }

    fn parse_property(&mut self) -> Result<GdsEvent<'data>, ParseError> {
        let rec = self.next_record("properties")?;
        match rec.header.record_type() {
            RecordType::Propattr => {
                let attr = extract_i16(&rec.body, RecordType::Propattr)?;
                let val_rec = self.expect_record(RecordType::Propvalue, "property value")?;
                let value = extract_str(&val_rec.body, RecordType::Propvalue)?;
                Ok(GdsEvent::Property(Property { attr, value }))
            }
            RecordType::Endel => {
                self.state = State::Structure;
                self.advance()
            }
            other => Err(ParseError::UnexpectedRecord {
                found: other,
                context: "properties (expected PROPATTR or ENDEL)",
            }),
        }
    }

    fn advance(&mut self) -> Result<GdsEvent<'data>, ParseError> {
        match self.state {
            State::Start => self.parse_library_begin(),
            State::Library => {
                let rec = self.next_record("library")?;
                match rec.header.record_type() {
                    RecordType::BgnStr => self.parse_structure_begin(&rec),
                    RecordType::EndLib => {
                        self.state = State::Done;
                        Ok(GdsEvent::LibraryEnd)
                    }
                    other => Err(ParseError::UnexpectedRecord {
                        found: other,
                        context: "library (expected BGNSTR or ENDLIB)",
                    }),
                }
            }
            State::Structure => {
                let rec = self.next_record("structure")?;
                match rec.header.record_type() {
                    RecordType::Boundary => self.parse_boundary(),
                    RecordType::Path => self.parse_path(),
                    RecordType::Sref => self.parse_sref(),
                    RecordType::Aref => self.parse_aref(),
                    RecordType::Text => self.parse_text(),
                    RecordType::Node => self.parse_node(),
                    RecordType::Box => self.parse_gds_box(),
                    RecordType::EndStr => {
                        self.state = State::Library;
                        Ok(GdsEvent::StructureEnd)
                    }
                    other => Err(ParseError::UnexpectedRecord {
                        found: other,
                        context: "structure (expected element or ENDSTR)",
                    }),
                }
            }
            State::Properties => self.parse_property(),
            State::Done => unreachable!("advance called in Done state"),
        }
    }
}

impl<'data> Iterator for GdsParser<'data> {
    type Item = Result<GdsEvent<'data>, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.state == State::Done {
            return None;
        }
        let result = self.advance();
        if result.is_err() {
            self.state = State::Done;
        }
        Some(result)
    }
}
