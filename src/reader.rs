//! GDSII reader

use zerocopy::TryFromBytes;

use crate::types::RecordHeader;

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
    type Item = (RecordHeader, &'data [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        match RecordHeader::try_ref_from_prefix(&self.input[self.offset..]) {
            Ok((header, rest)) => {
                // Yield the length described in the header
                let length = usize::from(header.length());
                self.offset += usize::from(header.length());
                Some((*header, &rest[..length - 4])) // Strip the 4 byte header from the body
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

        let (hdr, body) = iter.next().expect("HEADER record");
        assert_eq!(hdr.record_type(), RecordType::Header);
        assert_eq!(hdr.data_type(), DataType::TwoByteSignedInt);
        assert_eq!(body, &[0x00, 0x06]);

        let (hdr, body) = iter.next().expect("ENDLIB record");
        assert_eq!(hdr.record_type(), RecordType::EndLib);
        assert_eq!(hdr.data_type(), DataType::NoData);
        assert_eq!(body, &[]);

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
