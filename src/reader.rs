//! GDSII reader for parsing GDS bytes into intermediary types.
//!
//! See [`crate::types`] for more details on the over-the-wire raw formats.

use zerocopy::{
    TryFromBytes,
    big_endian::{I16, I32, U16},
};

use crate::{
    float::{GdsEightByteReal, GdsFourByteReal},
    types::{DataType, RecordHeader},
};

/// Parsed GDS record, including its header and associated data.
#[derive(Debug)]
pub struct Record<'data> {
    /// The record's header.
    ///
    /// See:
    /// <https://boolean.klaasholwerda.nl/interface/bnf/gdsformat.html#recordheader>.
    pub header: RecordHeader,
    /// The record's body.
    pub body: RecordBody<'data>,
}

/// Payload data contained within a `Record`.
#[derive(Debug, PartialEq, Eq)]
pub enum RecordBody<'data> {
    /// Header-only record, carrying no body.
    NoData,
    /// Two-byte bit fields, e.g. ELFLAGS, STRANS, PRESENTATION.
    BitArray(&'data [U16]),
    /// Signed 16-bit integers, e.g. LAYER, DATATYPE, timestamps.
    TwoByteSignedInt(&'data [I16]),
    /// Signed 32-bit integers, e.g. XY coordinates, WIDTH, PLEX.
    FourByteSignedInt(&'data [I32]),
    /// GDS base-16 four-byte reals.
    ///
    /// NOTE: raw bytes here because needs custom conversion to `f32`.
    FourByteReal(&'data [GdsFourByteReal]),
    /// GDS base-16 eight-byte reals, e.g. UNITS, MAG, ANGLE.
    ///
    /// NOTE: raw bytes here because needs custom conversion to `f64`.
    EightByteReal(&'data [GdsEightByteReal]),
    /// ASCII text with any trailing NUL padding already trimmed.
    AsciiString(&'data str),
}

/// Error encountered while reading a GDS record.
#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    /// Data/ datatype mismatch.
    #[error(
        "data does not match the expected datatype: expected {expected:?}, found: {found:?}"
    )]
    Invalid {
        /// Data type the record header declared.
        expected: DataType,
        /// Body bytes that could not be read as that type.
        found: Vec<u8>,
    },
    /// NOTE: Length field must be **at least** 4 since the length field of the record includes the
    /// 4-byte header.
    #[error(
        "record at offset {offset} declares {length} bytes, which is less than the 4-byte header"
    )]
    InvalidLength {
        /// Byte offset of the record header in the input.
        offset: usize,
        /// The impossible length the header declared.
        length: u16,
    },
    /// The length field runs past the end of the input.
    #[error(
        "record at offset {offset} declares length of {length} bytes, but only {available} bytes remain"
    )]
    Truncated {
        /// Byte offset of the record header in the input.
        offset: usize,
        /// Total record length the header declared, header included.
        length: usize,
        /// Bytes actually remaining from `offset` to the end of the input.
        available: usize,
    },
}

impl<'data> TryFrom<(DataType, &'data [u8])> for RecordBody<'data> {
    type Error = RecordError;

    fn try_from(
        (data_type, body): (DataType, &'data [u8]),
    ) -> Result<Self, Self::Error> {
        match data_type {
            DataType::NoData => Ok(Self::NoData),
            DataType::BitArray => <[U16]>::try_ref_from_bytes(body)
                .ok()
                .map(Self::BitArray)
                .ok_or_else(|| RecordError::Invalid {
                    expected: DataType::BitArray,
                    found: body.to_vec(),
                }),
            DataType::TwoByteSignedInt => <[I16]>::try_ref_from_bytes(body)
                .ok()
                .map(Self::TwoByteSignedInt)
                .ok_or_else(|| RecordError::Invalid {
                    expected: DataType::TwoByteSignedInt,
                    found: body.to_vec(),
                }),
            DataType::FourByteSignedInt => <[I32]>::try_ref_from_bytes(body)
                .ok()
                .map(Self::FourByteSignedInt)
                .ok_or_else(|| RecordError::Invalid {
                    expected: DataType::FourByteSignedInt,
                    found: body.to_vec(),
                }),
            DataType::FourByteReal => {
                <[GdsFourByteReal]>::try_ref_from_bytes(body)
                    .ok()
                    .map(Self::FourByteReal)
                    .ok_or_else(|| RecordError::Invalid {
                        expected: DataType::FourByteReal,
                        found: body.to_vec(),
                    })
            }
            DataType::EightByteReal => {
                <[GdsEightByteReal]>::try_ref_from_bytes(body)
                    .ok()
                    .map(Self::EightByteReal)
                    .ok_or_else(|| RecordError::Invalid {
                        expected: DataType::EightByteReal,
                        found: body.to_vec(),
                    })
            }
            DataType::AsciiString => std::str::from_utf8(body)
                .map(|s| s.trim_end_matches('\0'))
                .map_or_else(
                    |_| {
                        Err(RecordError::Invalid {
                            expected: DataType::AsciiString,
                            found: body.to_vec(),
                        })
                    },
                    |s| Ok(Self::AsciiString(s)),
                ),
        }
    }
}

/// An iterator over a GDS file that yields parsed records.
#[derive(Debug)]
pub struct RecordIter<'data> {
    /// Reference to the source GDS bytes.
    input: &'data [u8],
    /// Current offset into the source bytes.
    offset: usize,
}

impl<'data> RecordIter<'data> {
    /// Constructs a new iterator over the referenced GDS bytes.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use gdsii::reader::RecordIter;
    ///
    /// let data = std::fs::read("layout.gds").unwrap();
    /// for record in RecordIter::new(&data) {
    ///     let record = record.unwrap();
    ///     println!("{:?}: {:?}", record.header.record_type(), record.header.data_type());
    /// }
    /// ```
    #[must_use]
    pub fn new<B>(input: &'data B) -> Self
    where
        B: AsRef<[u8]> + ?Sized,
    {
        Self { input: input.as_ref(), offset: 0 }
    }
}

impl<'data> Iterator for RecordIter<'data> {
    type Item = Result<Record<'data>, RecordError>;

    fn next(&mut self) -> Option<Self::Item> {
        let Ok((header, rest)) =
            RecordHeader::try_ref_from_prefix(&self.input[self.offset..])
        else {
            return None;
        };
        let offset = self.offset;
        let length = header.length().get();

        // On malformed records, fuse the records.
        let Some(body_len) = usize::from(length).checked_sub(4) else {
            self.offset = self.input.len();
            return Some(Err(RecordError::InvalidLength { offset, length }));
        };
        if body_len > rest.len() {
            self.offset = self.input.len();
            return Some(Err(RecordError::Truncated {
                offset,
                length: usize::from(length),
                available: self.input.len() - offset,
            }));
        }

        let body =
            match RecordBody::try_from((header.data_type(), &rest[..body_len]))
            {
                Ok(body) => body,
                Err(e) => {
                    self.offset = self.input.len();
                    return Some(Err(e));
                }
            };
        self.offset += usize::from(length);
        Some(Ok(Record { header: *header, body }))
    }
}

#[cfg(test)]
mod tests {
    use crate::types::{DataType, RecordType};

    use super::*;

    #[test]
    fn iter_two_records() {
        // HEADER: 6 bytes total (4 header + 2 body), version = 6
        // ENDLIB: 4 bytes total (header only, no body)
        let bytes: &[u8] = &[
            0x00, 0x06, 0x00,
            0x02, // HEADER: length=6, type=0x00, datatype=0x02
            0x00, 0x06, // body: version 6
            0x00, 0x04, 0x04,
            0x00, // ENDLIB: length=4, type=0x04, datatype=0x00
        ];

        let mut iter = RecordIter::new(bytes);

        let record = iter
            .next()
            .expect("Couldn't get record")
            .expect("body parse failed");
        assert_eq!(record.header.record_type(), RecordType::Header);
        assert_eq!(record.header.data_type(), DataType::TwoByteSignedInt);
        assert_eq!(record.body, RecordBody::TwoByteSignedInt(&[0x06.into()]));

        let record = iter
            .next()
            .expect("Couldn't get record")
            .expect("body parse failed");
        assert_eq!(record.header.record_type(), RecordType::EndLib);
        assert_eq!(record.header.data_type(), DataType::NoData);
        assert_eq!(record.body, RecordBody::NoData);

        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_empty_input() {
        assert!(RecordIter::new(&[]).next().is_none());
    }

    #[test]
    fn iter_short_header() {
        // Header must be at least 4 bytes
        assert!(RecordIter::new(&[0x00, 0x00, 0x00]).next().is_none());
    }

    #[test]
    fn iter_rejects_length_below_header() {
        // A length field of 0 is what NUL padding after ENDLIB looks like, and
        // is also the shape that used to underflow `length - 4`.
        let mut iter = RecordIter::new(&[0x00, 0x00, 0x00, 0x00]);
        assert!(matches!(
            iter.next(),
            Some(Err(RecordError::InvalidLength { offset: 0, length: 0 }))
        ));
        // Fused: a bogus length leaves no safe distance to skip ahead by.
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_rejects_truncated_record() {
        // HEADER declaring 6 bytes with only 5 present.
        let mut iter = RecordIter::new(&[0x00, 0x06, 0x00, 0x02, 0x00]);
        assert!(matches!(
            iter.next(),
            Some(Err(RecordError::Truncated {
                offset: 0,
                length: 6,
                available: 5
            }))
        ));
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_fuses_after_a_body_type_error() {
        // LAYER claiming AsciiString with invalid UTF-8. Before fusing, the
        // offset never advanced past a body error, so `next` repeated it
        // forever.
        let mut iter = RecordIter::new(&[0x00, 0x06, 0x0D, 0x06, 0xFF, 0xFF]);
        assert!(matches!(iter.next(), Some(Err(RecordError::Invalid { .. }))));
        assert!(iter.next().is_none());
    }
}
