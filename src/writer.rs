//! GDS binary writer — serializes [`GdsEvent`]s to GDSII byte streams.
//!
//! Symmetric with [`crate::parser::GdsParser`]: the parser reads bytes → events,
//! the writer takes events → bytes. Together they enable streaming read-transform-write
//! pipelines with no full-file buffering.

use std::io::Write;

use zerocopy::big_endian::{I16, I32};

use crate::float::{GdsEightByteReal, NotRepresentable};
use crate::parser::{
    Aref, Boundary, Element, GdsBox, GdsEvent, LibraryBegin, Node, Path, Sref,
    Strans, StructureBegin, Text,
};
use crate::types::{DataType, RecordType};

/// Errors that can occur while writing GDS records.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// Underlying I/O failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// An f64 value could not be encoded as a GDS real.
    #[error("f64 not representable as GDS real")]
    Float(#[from] NotRepresentable),
}

// ==============================================================================
// Record-level helpers
// ==============================================================================

fn write_record_header(
    sink: &mut impl Write,
    record_type: RecordType,
    data_type: DataType,
    body_len: usize,
) -> Result<(), WriteError> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "GDS record bodies are always well under 64k"
    )]
    let length = (4 + body_len) as u16;
    sink.write_all(&length.to_be_bytes())?;
    sink.write_all(&[record_type as u8, data_type as u8])?;
    Ok(())
}

fn write_no_data(
    sink: &mut impl Write,
    record_type: RecordType,
) -> Result<(), WriteError> {
    write_record_header(sink, record_type, DataType::NoData, 0)
}

fn write_i16(
    sink: &mut impl Write,
    record_type: RecordType,
    value: i16,
) -> Result<(), WriteError> {
    write_record_header(sink, record_type, DataType::TwoByteSignedInt, 2)?;
    sink.write_all(&value.to_be_bytes())?;
    Ok(())
}

fn write_i32(
    sink: &mut impl Write,
    record_type: RecordType,
    value: i32,
) -> Result<(), WriteError> {
    write_record_header(sink, record_type, DataType::FourByteSignedInt, 4)?;
    sink.write_all(&value.to_be_bytes())?;
    Ok(())
}

fn write_u16(
    sink: &mut impl Write,
    record_type: RecordType,
    value: u16,
) -> Result<(), WriteError> {
    write_record_header(sink, record_type, DataType::BitArray, 2)?;
    sink.write_all(&value.to_be_bytes())?;
    Ok(())
}

fn write_str(
    sink: &mut impl Write,
    record_type: RecordType,
    value: &str,
) -> Result<(), WriteError> {
    let bytes = value.as_bytes();
    let padded = !bytes.len().is_multiple_of(2);
    let body_len = bytes.len() + usize::from(padded);
    write_record_header(sink, record_type, DataType::AsciiString, body_len)?;
    sink.write_all(bytes)?;
    if padded {
        sink.write_all(&[0x00])?;
    }
    Ok(())
}

fn write_i16_slice(
    sink: &mut impl Write,
    record_type: RecordType,
    values: &[I16],
) -> Result<(), WriteError> {
    write_record_header(
        sink,
        record_type,
        DataType::TwoByteSignedInt,
        values.len() * 2,
    )?;
    for v in values {
        sink.write_all(&v.get().to_be_bytes())?;
    }
    Ok(())
}

fn write_i32_slice(
    sink: &mut impl Write,
    record_type: RecordType,
    values: &[I32],
) -> Result<(), WriteError> {
    write_record_header(
        sink,
        record_type,
        DataType::FourByteSignedInt,
        values.len() * 4,
    )?;
    for v in values {
        sink.write_all(&v.get().to_be_bytes())?;
    }
    Ok(())
}

fn write_raw_real(
    sink: &mut impl Write,
    record_type: RecordType,
    value: GdsEightByteReal,
) -> Result<(), WriteError> {
    write_record_header(sink, record_type, DataType::EightByteReal, 8)?;
    sink.write_all(&value.raw())?;
    Ok(())
}

fn write_two_f64s(
    sink: &mut impl Write,
    record_type: RecordType,
    a: f64,
    b: f64,
) -> Result<(), WriteError> {
    let ra = GdsEightByteReal::try_from(a)?;
    let rb = GdsEightByteReal::try_from(b)?;
    write_record_header(sink, record_type, DataType::EightByteReal, 16)?;
    sink.write_all(&ra.raw())?;
    sink.write_all(&rb.raw())?;
    Ok(())
}

// ==============================================================================
// Compound helpers
// ==============================================================================

fn write_elflags_plex(
    sink: &mut impl Write,
    elflags: Option<u16>,
    plex: Option<i32>,
) -> Result<(), WriteError> {
    if let Some(flags) = elflags {
        write_u16(sink, RecordType::Elflags, flags)?;
    }
    if let Some(plex) = plex {
        write_i32(sink, RecordType::Plex, plex)?;
    }
    Ok(())
}

fn write_strans(
    sink: &mut impl Write,
    strans: &Strans,
) -> Result<(), WriteError> {
    let mut flags: u16 = 0;
    if strans.reflection {
        flags |= 0x8000;
    }
    if strans.abs_mag {
        flags |= 0x0004;
    }
    if strans.abs_angle {
        flags |= 0x0002;
    }
    write_u16(sink, RecordType::Strans, flags)?;
    if let Some(mag) = strans.mag {
        write_raw_real(sink, RecordType::Mag, mag)?;
    }
    if let Some(angle) = strans.angle {
        write_raw_real(sink, RecordType::Angle, angle)?;
    }
    Ok(())
}

// ==============================================================================
// Element writers
// ==============================================================================

fn write_boundary(
    sink: &mut impl Write,
    b: &Boundary<'_>,
) -> Result<(), WriteError> {
    write_no_data(sink, RecordType::Boundary)?;
    write_elflags_plex(sink, b.elflags, b.plex)?;
    write_i16(sink, RecordType::Layer, b.layer)?;
    write_i16(sink, RecordType::Datatype, b.datatype)?;
    write_i32_slice(sink, RecordType::Xy, b.xy)?;
    Ok(())
}

fn write_path(sink: &mut impl Write, p: &Path<'_>) -> Result<(), WriteError> {
    write_no_data(sink, RecordType::Path)?;
    write_elflags_plex(sink, p.elflags, p.plex)?;
    write_i16(sink, RecordType::Layer, p.layer)?;
    write_i16(sink, RecordType::Datatype, p.datatype)?;
    if let Some(pt) = p.pathtype {
        write_i16(sink, RecordType::Pathtype, pt)?;
    }
    if let Some(w) = p.width {
        write_i32(sink, RecordType::Width, w)?;
    }
    write_i32_slice(sink, RecordType::Xy, p.xy)?;
    Ok(())
}

fn write_sref(sink: &mut impl Write, s: &Sref<'_>) -> Result<(), WriteError> {
    write_no_data(sink, RecordType::Sref)?;
    write_elflags_plex(sink, s.elflags, s.plex)?;
    write_str(sink, RecordType::Sname, s.sname)?;
    if let Some(ref st) = s.strans {
        write_strans(sink, st)?;
    }
    write_i32_slice(sink, RecordType::Xy, s.xy)?;
    Ok(())
}

fn write_aref(sink: &mut impl Write, a: &Aref<'_>) -> Result<(), WriteError> {
    write_no_data(sink, RecordType::Aref)?;
    write_elflags_plex(sink, a.elflags, a.plex)?;
    write_str(sink, RecordType::Sname, a.sname)?;
    if let Some(ref st) = a.strans {
        write_strans(sink, st)?;
    }
    write_record_header(
        sink,
        RecordType::Colrow,
        DataType::TwoByteSignedInt,
        4,
    )?;
    sink.write_all(&a.colrow.0.to_be_bytes())?;
    sink.write_all(&a.colrow.1.to_be_bytes())?;
    write_i32_slice(sink, RecordType::Xy, a.xy)?;
    Ok(())
}

fn write_text(sink: &mut impl Write, t: &Text<'_>) -> Result<(), WriteError> {
    write_no_data(sink, RecordType::Text)?;
    write_elflags_plex(sink, t.elflags, t.plex)?;
    write_i16(sink, RecordType::Layer, t.layer)?;
    write_i16(sink, RecordType::TextType, t.texttype)?;
    if let Some(p) = t.presentation {
        write_u16(sink, RecordType::Presentation, p)?;
    }
    if let Some(pt) = t.pathtype {
        write_i16(sink, RecordType::Pathtype, pt)?;
    }
    if let Some(w) = t.width {
        write_i32(sink, RecordType::Width, w)?;
    }
    if let Some(ref st) = t.strans {
        write_strans(sink, st)?;
    }
    write_i32_slice(sink, RecordType::Xy, t.xy)?;
    write_str(sink, RecordType::String, t.string)?;
    Ok(())
}

fn write_node(sink: &mut impl Write, n: &Node<'_>) -> Result<(), WriteError> {
    write_no_data(sink, RecordType::Node)?;
    write_elflags_plex(sink, n.elflags, n.plex)?;
    write_i16(sink, RecordType::Layer, n.layer)?;
    write_i16(sink, RecordType::Nodetype, n.nodetype)?;
    write_i32_slice(sink, RecordType::Xy, n.xy)?;
    Ok(())
}

fn write_gds_box(
    sink: &mut impl Write,
    b: &GdsBox<'_>,
) -> Result<(), WriteError> {
    write_no_data(sink, RecordType::Box)?;
    write_elflags_plex(sink, b.elflags, b.plex)?;
    write_i16(sink, RecordType::Layer, b.layer)?;
    write_i16(sink, RecordType::BoxType, b.boxtype)?;
    write_i32_slice(sink, RecordType::Xy, b.xy)?;
    Ok(())
}

// ==============================================================================
// Public API
// ==============================================================================

/// Writes [`GdsEvent`]s as binary GDSII records to an underlying byte sink.
///
/// Tracks element state internally so that ENDEL records are emitted at the
/// correct position — after all [`GdsEvent::Property`] events for an element.
#[derive(Debug)]
pub struct GdsWriter<W: Write> {
    sink: W,
    needs_endel: bool,
}

impl<W: Write> GdsWriter<W> {
    /// Creates a new writer over the given sink.
    pub const fn new(sink: W) -> Self {
        Self { sink, needs_endel: false }
    }

    /// Consumes the writer and returns the underlying sink.
    pub fn into_inner(self) -> W {
        self.sink
    }

    fn flush_endel(&mut self) -> Result<(), WriteError> {
        if self.needs_endel {
            write_no_data(&mut self.sink, RecordType::Endel)?;
            self.needs_endel = false;
        }
        Ok(())
    }

    /// Writes a single event as one or more GDS binary records.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Io`] on I/O failure or [`WriteError::Float`] if a
    /// MAG/ANGLE/UNITS value is not representable as a GDS real.
    pub fn write_event(
        &mut self,
        event: &GdsEvent<'_>,
    ) -> Result<(), WriteError> {
        match event {
            GdsEvent::LibraryBegin(lib) => {
                self.write_library_begin(lib)?;
            }
            GdsEvent::StructureBegin(s) => {
                self.flush_endel()?;
                self.write_structure_begin(s)?;
            }
            GdsEvent::Element(elem) => {
                self.flush_endel()?;
                self.write_element(elem)?;
                self.needs_endel = true;
            }
            GdsEvent::Property(prop) => {
                write_i16(&mut self.sink, RecordType::Propattr, prop.attr)?;
                write_str(&mut self.sink, RecordType::Propvalue, prop.value)?;
            }
            GdsEvent::StructureEnd => {
                self.flush_endel()?;
                write_no_data(&mut self.sink, RecordType::EndStr)?;
            }
            GdsEvent::LibraryEnd => {
                self.flush_endel()?;
                write_no_data(&mut self.sink, RecordType::EndLib)?;
            }
        }
        Ok(())
    }

    fn write_library_begin(
        &mut self,
        lib: &LibraryBegin<'_>,
    ) -> Result<(), WriteError> {
        write_i16(&mut self.sink, RecordType::Header, lib.version)?;
        write_i16_slice(&mut self.sink, RecordType::BgnLib, lib.timestamps)?;
        write_str(&mut self.sink, RecordType::LibName, lib.lib_name)?;
        if let Some(reflibs) = lib.reflibs {
            write_str(&mut self.sink, RecordType::Reflibs, reflibs)?;
        }
        if let Some(fonts) = lib.fonts {
            write_str(&mut self.sink, RecordType::Fonts, fonts)?;
        }
        if let Some(attrtable) = lib.attrtable {
            write_str(&mut self.sink, RecordType::Attrtable, attrtable)?;
        }
        if let Some(r#gen) = lib.generations {
            write_i16(&mut self.sink, RecordType::Generations, r#gen)?;
        }
        write_two_f64s(
            &mut self.sink,
            RecordType::Units,
            lib.db_in_user,
            lib.db_in_meters,
        )?;
        Ok(())
    }

    fn write_structure_begin(
        &mut self,
        s: &StructureBegin<'_>,
    ) -> Result<(), WriteError> {
        write_i16_slice(&mut self.sink, RecordType::BgnStr, s.timestamps)?;
        write_str(&mut self.sink, RecordType::StrName, s.name)?;
        Ok(())
    }

    fn write_element(&mut self, elem: &Element<'_>) -> Result<(), WriteError> {
        match elem {
            Element::Boundary(b) => write_boundary(&mut self.sink, b),
            Element::Path(p) => write_path(&mut self.sink, p),
            Element::Sref(s) => write_sref(&mut self.sink, s),
            Element::Aref(a) => write_aref(&mut self.sink, a),
            Element::Text(t) => write_text(&mut self.sink, t),
            Element::Node(n) => write_node(&mut self.sink, n),
            Element::Box(b) => write_gds_box(&mut self.sink, b),
        }
    }
}

/// Writes an entire event stream to a sink.
///
/// # Errors
///
/// Returns the first I/O or encoding error encountered.
pub fn write_all<'data>(
    sink: impl Write,
    events: impl IntoIterator<Item = GdsEvent<'data>>,
) -> Result<(), WriteError> {
    let mut writer = GdsWriter::new(sink);
    for event in events {
        writer.write_event(&event)?;
    }
    Ok(())
}
