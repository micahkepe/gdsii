//! Zero-copy, streaming GDS event parser.
//!
//! Wraps [`RecordIter`] and yields high-level [`GdsEvent`]s -- one per logical GDS unit
//! (library header, structure header, complete element, property, structural delimiters).
//! All borrowed data references the original input buffer; no allocations occur during parsing.

use zerocopy::big_endian::{I16, I32};

use crate::float::GdsEightByteReal;
use crate::reader::{BodyParseError, Record, RecordBody, RecordIter};
use crate::types::RecordType;

/// Errors that can occur during GDS stream parsing.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// Record type does not match what the grammar expects at this position.
    #[error("unexpected record {found:?} in {context}")]
    UnexpectedRecord { found: RecordType, context: &'static str },
    /// Record stream ended before a required record was found.
    #[error("unexpected end of records in {context}")]
    UnexpectedEof { context: &'static str },
    /// Record body data type does not match the expected variant.
    #[error("{record_type:?} body: expected {expected}")]
    WrongBodyType { record_type: RecordType, expected: &'static str },
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
    /// Stream format version (typically 3, 4, 5, or 7).
    pub version: i16,
    /// Twelve i16 values: modification time then access time
    /// (year, month, day, hour, minute, second × 2).
    pub timestamps: &'data [I16],
    /// Library name string.
    pub lib_name: &'data str,
    /// Size of a database unit in user units (e.g. 0.001 for 1nm DB, 1um user).
    pub db_in_user: f64,
    /// Size of a database unit in meters (e.g. 1e-9 for 1nm).
    pub db_in_meters: f64,
    /// Reference library names (44 bytes each, up to 15).
    pub reflibs: Option<&'data str>,
    /// Text font definition file names.
    pub fonts: Option<&'data str>,
    /// Attribute definition file name.
    pub attrtable: Option<&'data str>,
    /// Number of deleted/backup structure copies to retain (2--99).
    pub generations: Option<i16>,
}

/// Structure (cell) header from BGNSTR and STRNAME records.
#[derive(Debug)]
pub struct StructureBegin<'data> {
    /// Twelve i16 timestamp values (same format as [`LibraryBegin::timestamps`]).
    pub timestamps: &'data [I16],
    /// Structure name (up to 32 characters).
    pub name: &'data str,
}

/// PROPATTR/PROPVALUE pair attached to an element.
#[derive(Debug)]
pub struct Property<'data> {
    /// Attribute number (1--127).
    pub attr: i16,
    /// Attribute value string (up to 126 characters).
    pub value: &'data str,
}

/// Transformation flags from STRANS, MAG, and ANGLE records.
///
/// MAG and ANGLE are stored as raw [`GdsEightByteReal`] to preserve the original
/// encoding for byte-exact roundtrips. Use `f64::from(real)` to decode.
#[derive(Debug, Clone, Copy)]
pub struct Strans {
    /// Reflect about the X-axis before rotation.
    pub reflection: bool,
    /// Magnification is absolute (not affected by parent).
    pub abs_mag: bool,
    /// Angle is absolute (not affected by parent).
    pub abs_angle: bool,
    /// Magnification factor as raw GDS real. Decode with `f64::from(mag)`.
    pub mag: Option<GdsEightByteReal>,
    /// Rotation angle in degrees (counterclockwise) as raw GDS real.
    pub angle: Option<GdsEightByteReal>,
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
    /// Element flags (bit 14 = external data, bit 15 = template data).
    pub elflags: Option<u16>,
    /// Plex group membership ID.
    pub plex: Option<i32>,
    /// Layer number (0 - 255).
    pub layer: i16,
    /// Datatype number (0 - 255).
    pub datatype: i16,
    /// Flat coordinate pairs `[x0, y0, x1, y1, ...]` in database units.
    pub xy: &'data [I32],
}

/// Wire-like path element with optional width and endpoint style.
#[derive(Debug)]
pub struct Path<'data> {
    /// Element flags.
    pub elflags: Option<u16>,
    /// Plex group membership ID.
    pub plex: Option<i32>,
    /// Layer number (0-255).
    pub layer: i16,
    /// Datatype number (0-255).
    pub datatype: i16,
    /// Endpoint style: 0 = flush, 1 = round, 2 = extended half-width, 4 = custom extensions.
    pub pathtype: Option<i16>,
    /// Width in database units. Negative means absolute (unaffected by magnification).
    pub width: Option<i32>,
    /// Start-point extension in database units. Meaningful only for `pathtype` 4.
    pub bgn_extn: Option<i32>,
    /// End-point extension in database units. Meaningful only for `pathtype` 4.
    pub end_extn: Option<i32>,
    /// Flat coordinate pairs in database units.
    pub xy: &'data [I32],
}

/// Structure reference (instance placement).
#[derive(Debug)]
pub struct Sref<'data> {
    /// Element flags.
    pub elflags: Option<u16>,
    /// Plex group membership ID.
    pub plex: Option<i32>,
    /// Name of the referenced structure.
    pub sname: &'data str,
    /// Transformation (reflection, magnification, rotation).
    pub strans: Option<Strans>,
    /// Origin point as `[x, y]` in database units.
    pub xy: &'data [I32],
}

/// Array reference (repeated instance placement in a grid).
#[derive(Debug)]
pub struct Aref<'data> {
    /// Element flags.
    pub elflags: Option<u16>,
    /// Plex group membership ID.
    pub plex: Option<i32>,
    /// Name of the referenced structure.
    pub sname: &'data str,
    /// Transformation (reflection, magnification, rotation).
    pub strans: Option<Strans>,
    /// `(columns, rows)` in the array grid.
    pub colrow: (i16, i16),
    /// Three points as `[ref_x, ref_y, col_end_x, col_end_y, row_end_x, row_end_y]`.
    /// Points are post-STRANS (already transformed).
    pub xy: &'data [I32],
}

/// Text annotation element.
#[derive(Debug)]
pub struct Text<'data> {
    /// Element flags.
    pub elflags: Option<u16>,
    /// Plex group membership ID.
    pub plex: Option<i32>,
    /// Layer number (0--255).
    pub layer: i16,
    /// Text type number (0--255).
    pub texttype: i16,
    /// Justification flags (bits 10--11: font, 12--13: vertical, 14--15: horizontal).
    pub presentation: Option<u16>,
    /// Endpoint style for the text path.
    pub pathtype: Option<i16>,
    /// Width of text lines in database units.
    pub width: Option<i32>,
    /// Transformation (reflection, magnification, rotation).
    pub strans: Option<Strans>,
    /// Origin point as `[x, y]` in database units.
    pub xy: &'data [I32],
    /// The text string content (up to 512 characters).
    pub string: &'data str,
}

/// Electrical net node element.
#[derive(Debug)]
pub struct Node<'data> {
    /// Element flags.
    pub elflags: Option<u16>,
    /// Plex group membership ID.
    pub plex: Option<i32>,
    /// Layer number (0--255).
    pub layer: i16,
    /// Node type number (0--255).
    pub nodetype: i16,
    /// Coordinate pairs (1--50 points) in database units.
    pub xy: &'data [I32],
}

/// Rectangular box element. Named `GdsBox` to avoid shadowing `std::boxed::Box`.
#[derive(Debug)]
pub struct GdsBox<'data> {
    /// Element flags.
    pub elflags: Option<u16>,
    /// Plex group membership ID.
    pub plex: Option<i32>,
    /// Layer number (0--255).
    pub layer: i16,
    /// Box type number (0--255).
    pub boxtype: i16,
    /// Five coordinate pairs (closed rectangle) in database units.
    pub xy: &'data [I32],
}

// ==============================================================================
// Body extraction helpers
// ==============================================================================

fn extract_i16(
    body: &RecordBody,
    record_type: RecordType,
) -> Result<i16, ParseError> {
    match body {
        RecordBody::TwoByteSignedInt(s) if !s.is_empty() => Ok(s[0].get()),
        _ => Err(ParseError::WrongBodyType {
            record_type,
            expected: "TwoByteSignedInt with ≥1 element",
        }),
    }
}

fn extract_i32(
    body: &RecordBody,
    record_type: RecordType,
) -> Result<i32, ParseError> {
    match body {
        RecordBody::FourByteSignedInt(s) if !s.is_empty() => Ok(s[0].get()),
        _ => Err(ParseError::WrongBodyType {
            record_type,
            expected: "FourByteSignedInt with ≥1 element",
        }),
    }
}

fn extract_u16(
    body: &RecordBody,
    record_type: RecordType,
) -> Result<u16, ParseError> {
    match body {
        RecordBody::BitArray(s) if !s.is_empty() => Ok(s[0].get()),
        _ => Err(ParseError::WrongBodyType {
            record_type,
            expected: "BitArray with ≥1 element",
        }),
    }
}

fn extract_real(
    body: &RecordBody,
    record_type: RecordType,
) -> Result<GdsEightByteReal, ParseError> {
    match body {
        RecordBody::EightByteReal(s) if !s.is_empty() => Ok(s[0]),
        _ => Err(ParseError::WrongBodyType {
            record_type,
            expected: "EightByteReal with ≥1 element",
        }),
    }
}

fn extract_two_f64s(
    body: &RecordBody,
    record_type: RecordType,
) -> Result<(f64, f64), ParseError> {
    match body {
        RecordBody::EightByteReal(s) if s.len() >= 2 => {
            Ok((f64::from(s[0]), f64::from(s[1])))
        }
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

    fn next_record(
        &mut self,
        context: &'static str,
    ) -> Result<Record<'data>, ParseError> {
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

    fn try_record(
        &mut self,
        expected: RecordType,
    ) -> Result<Option<Record<'data>>, ParseError> {
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

    fn parse_elflags_plex(
        &mut self,
    ) -> Result<(Option<u16>, Option<i32>), ParseError> {
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
            .map(|r| extract_real(&r.body, RecordType::Mag))
            .transpose()?;
        let angle = self
            .try_record(RecordType::Angle)?
            .map(|r| extract_real(&r.body, RecordType::Angle))
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
        let header_rec =
            self.expect_record(RecordType::Header, "library begin")?;
        let version = extract_i16(&header_rec.body, RecordType::Header)?;

        let bgnlib_rec =
            self.expect_record(RecordType::BgnLib, "library begin")?;
        let timestamps =
            extract_i16_slice(&bgnlib_rec.body, RecordType::BgnLib)?;

        let libname_rec =
            self.expect_record(RecordType::LibName, "library begin")?;
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
                    reflibs =
                        Some(extract_str(&rec.body, RecordType::Reflibs)?);
                }
                RecordType::Fonts => {
                    fonts = Some(extract_str(&rec.body, RecordType::Fonts)?);
                }
                RecordType::Attrtable => {
                    attrtable =
                        Some(extract_str(&rec.body, RecordType::Attrtable)?);
                }
                RecordType::Generations => {
                    generations =
                        Some(extract_i16(&rec.body, RecordType::Generations)?);
                }
                RecordType::Format
                | RecordType::Mask
                | RecordType::EndMasks => {}
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
        let strname_rec =
            self.expect_record(RecordType::StrName, "structure begin")?;
        let name = extract_str(&strname_rec.body, RecordType::StrName)?;
        self.state = State::Structure;
        Ok(GdsEvent::StructureBegin(StructureBegin { timestamps, name }))
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
        let bgn_extn = self
            .try_record(RecordType::BgnExtn)?
            .map(|r| extract_i32(&r.body, RecordType::BgnExtn))
            .transpose()?;
        let end_extn = self
            .try_record(RecordType::EndExtn)?
            .map(|r| extract_i32(&r.body, RecordType::EndExtn))
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
            bgn_extn,
            end_extn,
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
        let colrow_slice =
            extract_i16_slice(&colrow_rec.body, RecordType::Colrow)?;
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
                let val_rec = self
                    .expect_record(RecordType::Propvalue, "property value")?;
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

#[cfg(test)]
mod tests {

    use crate::DataType;

    use super::*;

    #[expect(
        clippy::cast_possible_truncation,
        reason = "test-only, lengths always small"
    )]
    const fn header(
        length: u16,
        record_type: RecordType,
        data_type: DataType,
    ) -> [u8; 4] {
        [(length >> 8) as u8, length as u8, record_type as u8, data_type as u8]
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "test-only, lengths always small"
    )]
    fn gds_record(
        record_type: RecordType,
        data_type: DataType,
        body: &[u8],
    ) -> Vec<u8> {
        let length = (4 + body.len()) as u16;
        let mut buf = Vec::with_capacity(length as usize);
        buf.extend_from_slice(&header(length, record_type, data_type));
        buf.extend_from_slice(body);
        buf
    }

    fn no_data_record(record_type: RecordType) -> Vec<u8> {
        gds_record(record_type, DataType::NoData, &[])
    }

    fn i16_record(record_type: RecordType, value: i16) -> Vec<u8> {
        gds_record(
            record_type,
            DataType::TwoByteSignedInt,
            &value.to_be_bytes(),
        )
    }

    fn i32_record(record_type: RecordType, value: i32) -> Vec<u8> {
        gds_record(
            record_type,
            DataType::FourByteSignedInt,
            &value.to_be_bytes(),
        )
    }

    fn string_record(record_type: RecordType, s: &str) -> Vec<u8> {
        let mut body: Vec<u8> = s.bytes().collect();
        if !body.len().is_multiple_of(2) {
            body.push(0x00);
        }
        gds_record(record_type, DataType::AsciiString, &body)
    }

    fn xy_record(coords: &[i32]) -> Vec<u8> {
        let body: Vec<u8> =
            coords.iter().flat_map(|c| c.to_be_bytes()).collect();
        gds_record(RecordType::Xy, DataType::FourByteSignedInt, &body)
    }

    fn bitarray_record(record_type: RecordType, value: u16) -> Vec<u8> {
        gds_record(record_type, DataType::BitArray, &value.to_be_bytes())
    }

    /// Builds a minimal valid library wrapper around inner structure bytes.
    fn minimal_library(inner: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend(i16_record(RecordType::Header, 5));
        buf.extend(gds_record(
            RecordType::BgnLib,
            DataType::TwoByteSignedInt,
            &[0u8; 24],
        ));
        buf.extend(string_record(RecordType::LibName, "TEST"));
        buf.extend(gds_record(
            RecordType::Units,
            DataType::EightByteReal,
            &[
                0x3E, 0x41, 0x89, 0x37, 0x4B, 0xC6, 0xA7, 0xEF, // 0.001
                0x39, 0x44, 0xB8, 0x2F, 0xA0, 0x9B, 0x5A, 0x54, // 1e-9
            ],
        )); // UNITS
        buf.extend_from_slice(inner);
        buf.extend(no_data_record(RecordType::EndLib));
        buf
    }

    /// Builds a minimal structure wrapper around inner element bytes.
    fn minimal_structure(name: &str, inner: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend(gds_record(
            RecordType::BgnStr,
            DataType::TwoByteSignedInt,
            &[0u8; 24],
        )); // BGNSTR
        buf.extend(string_record(RecordType::StrName, name));
        buf.extend_from_slice(inner);
        buf.extend(no_data_record(RecordType::EndStr));
        buf
    }

    #[test]
    fn extract_i16_wrong_body_type() {
        let body = RecordBody::NoData;
        assert!(extract_i16(&body, RecordType::Layer).is_err());
    }

    #[test]
    fn extract_str_from_ascii() {
        let body = RecordBody::AsciiString("hello");
        assert_eq!(extract_str(&body, RecordType::LibName).unwrap(), "hello");
    }

    #[test]
    fn parse_empty_library() {
        let data = minimal_library(&[]);
        let events: Vec<_> = GdsParser::new(&data)
            .collect::<Result<_, _>>()
            .expect("parse failed");
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], GdsEvent::LibraryBegin(_)));
        assert!(matches!(events[1], GdsEvent::LibraryEnd));
    }

    #[test]
    fn parse_empty_structure() {
        let structure = minimal_structure("CELL", &[]);
        let data = minimal_library(&structure);
        let events: Vec<_> = GdsParser::new(&data)
            .collect::<Result<_, _>>()
            .expect("parse failed");
        assert_eq!(events.len(), 4);
        let GdsEvent::StructureBegin(s) = &events[1] else {
            panic!("expected StructureBegin");
        };
        assert_eq!(s.name, "CELL");
        assert!(matches!(events[2], GdsEvent::StructureEnd));
    }

    #[test]
    fn parse_boundary_element() {
        let mut element = Vec::new();
        element.extend(no_data_record(RecordType::Boundary));
        element.extend(i16_record(RecordType::Layer, 5));
        element.extend(i16_record(RecordType::Datatype, 3));
        element.extend(xy_record(&[0, 0, 100, 0, 100, 100, 0, 100, 0, 0]));
        element.extend(no_data_record(RecordType::Endel));

        let structure = minimal_structure("TOP", &element);
        let data = minimal_library(&structure);
        let events: Vec<_> = GdsParser::new(&data)
            .collect::<Result<_, _>>()
            .expect("parse failed");

        let GdsEvent::Element(Element::Boundary(b)) = &events[2] else {
            panic!("expected Boundary");
        };
        assert_eq!(b.layer, 5);
        assert_eq!(b.datatype, 3);
        assert_eq!(b.xy.len(), 10);
    }

    #[test]
    fn parse_path_with_optional_fields() {
        let mut element = Vec::new();
        element.extend(no_data_record(RecordType::Path));
        element.extend(i16_record(RecordType::Layer, 1));
        element.extend(i16_record(RecordType::Datatype, 0));
        element.extend(i16_record(RecordType::Pathtype, 2));
        element.extend(i32_record(RecordType::Width, 500));
        element.extend(xy_record(&[0, 0, 1000, 0]));
        element.extend(no_data_record(RecordType::Endel));

        let structure = minimal_structure("TOP", &element);
        let data = minimal_library(&structure);
        let events: Vec<_> = GdsParser::new(&data)
            .collect::<Result<_, _>>()
            .expect("parse failed");

        let GdsEvent::Element(Element::Path(p)) = &events[2] else {
            panic!("expected Path");
        };
        assert_eq!(p.pathtype, Some(2));
        assert_eq!(p.width, Some(500));
        assert_eq!(p.bgn_extn, None);
        assert_eq!(p.end_extn, None);
    }

    #[test]
    fn parse_path_with_custom_extensions() {
        let mut element = Vec::new();
        element.extend(no_data_record(RecordType::Path));
        element.extend(i16_record(RecordType::Layer, 1));
        element.extend(i16_record(RecordType::Datatype, 0));
        element.extend(i16_record(RecordType::Pathtype, 4));
        element.extend(i32_record(RecordType::Width, 500));
        element.extend(i32_record(RecordType::BgnExtn, 250));
        element.extend(i32_record(RecordType::EndExtn, -100));
        element.extend(xy_record(&[0, 0, 1000, 0]));
        element.extend(no_data_record(RecordType::Endel));

        let structure = minimal_structure("TOP", &element);
        let data = minimal_library(&structure);
        let events: Vec<_> = GdsParser::new(&data)
            .collect::<Result<_, _>>()
            .expect("parse failed");

        let GdsEvent::Element(Element::Path(p)) = &events[2] else {
            panic!("expected Path");
        };
        assert_eq!(p.pathtype, Some(4));
        assert_eq!(p.width, Some(500));
        assert_eq!(p.bgn_extn, Some(250));
        assert_eq!(p.end_extn, Some(-100));
    }

    #[test]
    fn parse_path_without_optional_fields() {
        let mut element = Vec::new();
        element.extend(no_data_record(RecordType::Path));
        element.extend(i16_record(RecordType::Layer, 1));
        element.extend(i16_record(RecordType::Datatype, 0));
        element.extend(xy_record(&[0, 0, 1000, 0]));
        element.extend(no_data_record(RecordType::Endel));

        let structure = minimal_structure("TOP", &element);
        let data = minimal_library(&structure);
        let events: Vec<_> = GdsParser::new(&data)
            .collect::<Result<_, _>>()
            .expect("parse failed");

        let GdsEvent::Element(Element::Path(p)) = &events[2] else {
            panic!("expected Path");
        };
        assert_eq!(p.pathtype, None);
        assert_eq!(p.width, None);
    }

    #[test]
    fn parse_sref_with_strans() {
        let mut element = Vec::new();
        element.extend(no_data_record(RecordType::Sref));
        element.extend(string_record(RecordType::Sname, "CHILD"));
        element.extend(bitarray_record(RecordType::Strans, 0x8000));
        element.extend(xy_record(&[100, 200]));
        element.extend(no_data_record(RecordType::Endel));

        let structure = minimal_structure("TOP", &element);
        let data = minimal_library(&structure);
        let events: Vec<_> = GdsParser::new(&data)
            .collect::<Result<_, _>>()
            .expect("parse failed");

        let GdsEvent::Element(Element::Sref(s)) = &events[2] else {
            panic!("expected Sref");
        };
        assert_eq!(s.sname, "CHILD");
        let strans = s.strans.expect("strans should be present");
        assert!(strans.reflection);
        assert!(!strans.abs_mag);
        assert_eq!(strans.mag, None);
    }

    #[test]
    fn parse_element_with_elflags_and_plex() {
        let mut element = Vec::new();
        element.extend(no_data_record(RecordType::Boundary));
        element.extend(bitarray_record(RecordType::Elflags, 0x0002));
        element.extend(i32_record(RecordType::Plex, 42));
        element.extend(i16_record(RecordType::Layer, 0));
        element.extend(i16_record(RecordType::Datatype, 0));
        element.extend(xy_record(&[0, 0, 1, 0, 1, 1, 0, 1, 0, 0]));
        element.extend(no_data_record(RecordType::Endel));

        let structure = minimal_structure("TOP", &element);
        let data = minimal_library(&structure);
        let events: Vec<_> = GdsParser::new(&data)
            .collect::<Result<_, _>>()
            .expect("parse failed");

        let GdsEvent::Element(Element::Boundary(b)) = &events[2] else {
            panic!("expected Boundary");
        };
        assert_eq!(b.elflags, Some(0x0002));
        assert_eq!(b.plex, Some(42));
    }

    #[test]
    fn parse_element_with_properties() {
        let mut element = Vec::new();
        element.extend(no_data_record(RecordType::Boundary));
        element.extend(i16_record(RecordType::Layer, 0));
        element.extend(i16_record(RecordType::Datatype, 0));
        element.extend(xy_record(&[0, 0, 1, 0, 1, 1, 0, 1, 0, 0]));
        element.extend(i16_record(RecordType::Propattr, 1));
        element.extend(string_record(RecordType::Propvalue, "net_name"));
        element.extend(i16_record(RecordType::Propattr, 2));
        element.extend(string_record(RecordType::Propvalue, "signal"));
        element.extend(no_data_record(RecordType::Endel));

        let structure = minimal_structure("TOP", &element);
        let data = minimal_library(&structure);
        let events: Vec<_> = GdsParser::new(&data)
            .collect::<Result<_, _>>()
            .expect("parse failed");

        assert_eq!(events.len(), 7);
        assert!(matches!(events[2], GdsEvent::Element(Element::Boundary(_))));

        let GdsEvent::Property(p1) = &events[3] else {
            panic!("expected Property");
        };
        assert_eq!(p1.attr, 1);
        assert_eq!(p1.value, "net_name");

        let GdsEvent::Property(p2) = &events[4] else {
            panic!("expected Property");
        };
        assert_eq!(p2.attr, 2);
        assert_eq!(p2.value, "signal");
    }

    #[test]
    fn unexpected_record_in_library_state() {
        let mut data = Vec::new();
        data.extend(i16_record(RecordType::Header, 5));
        data.extend(gds_record(
            RecordType::BgnLib,
            DataType::TwoByteSignedInt,
            &[0u8; 24],
        ));
        data.extend(string_record(RecordType::LibName, "LIB"));
        data.extend(gds_record(
            RecordType::Units,
            DataType::EightByteReal,
            &[0u8; 16],
        ));
        data.extend(i16_record(RecordType::Layer, 0)); // LAYER — wrong context

        let mut parser = GdsParser::new(&data);
        let first = parser.next().unwrap();
        assert!(first.is_ok());
        let second = parser.next().unwrap();
        assert!(second.is_err());
        assert!(parser.next().is_none(), "should be fused after error");
    }

    #[test]
    fn unexpected_record_in_structure_state() {
        let bad_inner =
            gds_record(RecordType::Units, DataType::EightByteReal, &[0u8; 16]); // UNITS — invalid inside structure
        let structure = minimal_structure("TOP", &bad_inner);
        let data = minimal_library(&structure);

        let events: Vec<_> = GdsParser::new(&data).collect::<Vec<_>>();
        assert_eq!(events.len(), 3);
        assert!(events[0].is_ok());
        assert!(events[1].is_ok());
        assert!(events[2].is_err());
    }

    #[test]
    fn fused_after_error() {
        let bytes: &[u8] = &[0x00, 0x04, 0x0D, 0x02];
        let mut parser = GdsParser::new(bytes);
        let first = parser.next().expect("should yield one event");
        assert!(first.is_err());
        assert!(parser.next().is_none());
        assert!(parser.next().is_none());
    }
}
