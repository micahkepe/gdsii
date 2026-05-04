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
pub struct Record<'data> {
    pub header: RecordHeader,
    pub body: RecordBody<'data>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RecordBody<'data> {
    NoData,
    BitArray(&'data [U16]),
    TwoByteSignedInt(&'data [I16]),
    FourByteSignedInt(&'data [I32]),
    /// NOTE: raw bytes here because needs custom conversion to `f32`.
    FourByteReal(&'data [GdsFourByteReal]),
    /// NOTE: raw bytes here because needs custom conversion to `f64`.
    EightByteReal(&'data [GdsEightByteReal]),
    AsciiString(&'data str),
}

#[derive(Debug, thiserror::Error)]
pub enum BodyParseError {
    #[error("data does not match the expected datatype: expected {expected:?}, found: {found:?}")]
    Invalid { expected: DataType, found: Vec<u8> },
}

impl<'data> TryFrom<(DataType, &'data [u8])> for RecordBody<'data> {
    type Error = BodyParseError;

    fn try_from((data_type, body): (DataType, &'data [u8])) -> Result<Self, Self::Error> {
        match data_type {
            DataType::NoData => Ok(Self::NoData),
            DataType::BitArray => <[U16]>::try_ref_from_bytes(body)
                .ok()
                .map(Self::BitArray)
                .ok_or_else(|| BodyParseError::Invalid {
                    expected: DataType::BitArray,
                    found: body.to_vec(),
                }),
            DataType::TwoByteSignedInt => <[I16]>::try_ref_from_bytes(body)
                .ok()
                .map(Self::TwoByteSignedInt)
                .ok_or_else(|| BodyParseError::Invalid {
                    expected: DataType::TwoByteSignedInt,
                    found: body.to_vec(),
                }),
            DataType::FourByteSignedInt => <[I32]>::try_ref_from_bytes(body)
                .ok()
                .map(Self::FourByteSignedInt)
                .ok_or_else(|| BodyParseError::Invalid {
                    expected: DataType::FourByteSignedInt,
                    found: body.to_vec(),
                }),
            DataType::FourByteReal => <[GdsFourByteReal]>::try_ref_from_bytes(body)
                .ok()
                .map(Self::FourByteReal)
                .ok_or_else(|| BodyParseError::Invalid {
                    expected: DataType::FourByteReal,
                    found: body.to_vec(),
                }),
            DataType::EightByteReal => <[GdsEightByteReal]>::try_ref_from_bytes(body)
                .ok()
                .map(Self::EightByteReal)
                .ok_or_else(|| BodyParseError::Invalid {
                    expected: DataType::EightByteReal,
                    found: body.to_vec(),
                }),
            DataType::AsciiString => std::str::from_utf8(body)
                .map(|s| s.trim_end_matches('\0'))
                .map_or_else(
                    |_| {
                        Err(BodyParseError::Invalid {
                            expected: DataType::AsciiString,
                            found: body.to_vec(),
                        })
                    },
                    |s| Ok(Self::AsciiString(s)),
                ),
        }
    }
}

/// An iterator over a GDS file that returns file that returns `(RecordHeader, <data>)`.
pub struct RecordIter<'data> {
    /// Reference to the source GDS bytes.
    input: &'data [u8],
    /// Current offset into the source bytes.
    offset: usize,
}

impl<'data> RecordIter<'data> {
    #[must_use]
    pub fn new<B>(input: &'data B) -> Self
    where
        B: AsRef<[u8]> + ?Sized,
    {
        Self {
            input: input.as_ref(),
            offset: 0,
        }
    }
}

impl<'data> Iterator for RecordIter<'data> {
    type Item = Record<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        match RecordHeader::try_ref_from_prefix(&self.input[self.offset..]) {
            Ok((header, rest)) => {
                let length = usize::from(header.length().get());
                // Strip the 4 byte header from the body
                let body_bytes = &rest[..length - 4];
                let body = RecordBody::try_from((header.data_type(), body_bytes)).ok()?;
                self.offset += length;
                Some(Record {
                    header: *header,
                    body,
                })
            }
            _ => None,
        }
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
            0x00, 0x06, 0x00, 0x02, // HEADER: length=6, type=0x00, datatype=0x02
            0x00, 0x06, // body: version 6
            0x00, 0x04, 0x04, 0x00, // ENDLIB: length=4, type=0x04, datatype=0x00
        ];

        let mut iter = RecordIter::new(bytes);

        let record = iter.next().expect("Couldn't get record");
        assert_eq!(record.header.record_type(), RecordType::Header);
        assert_eq!(record.header.data_type(), DataType::TwoByteSignedInt);
        assert_eq!(record.body, RecordBody::TwoByteSignedInt(&[0x06.into()]));

        let record = iter.next().expect("Couldn't get record");
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
}
