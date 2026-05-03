/*!
GDSII types.

NOTE: Underlying byte stream is assumed to be **immutable**.

## Reference
 * Original manual: <https://www.bitsavers.org/pdf/calma/GDS_II_Users_Operating_Manual_Nov78.pdf>
 * [GDSII format](https://boolean.klaasholwerda.nl/interface/bnf/gdsformat.html)
*/
use zerocopy::big_endian::U16;
use zerocopy_derive::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

/// GDSII Record Type (1 byte).
///
/// Records are always an even number of bytes long. The first four bytes of a record are the
/// record header (see `RecordHeader`). If a record contains ASCII string data and the ASCII string
/// is an odd number of bytes long, the data is padded with a null character.
///
/// The following record types are defined but intentionally omitted from this enum because they
/// are unreleased or unused: STYPTABLE (0x25), STRTYPE (0x25), ELKEY (0x27), LINKTYPE (0x28),
/// LINKKEYS (0x29), STRCLASS (0x34), RESERVED (0x35).
#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Clone, Copy, IntoBytes, TryFromBytes, Unaligned, Immutable)]
pub enum RecordType {
    /// Contains two bytes of data representing the Stream version number.
    Header = 0x00,

    /// Contains the last modification time of a library (two bytes each for year, month, day,
    /// hour, minute, and second), the time of last access (same format), and marks the beginning
    /// of a library.
    ///
    /// ```text
    ///          Bit 0-7              Bit 8-15
    /// word1  : 1C (hex) # of bytes in record
    /// word2  : 01 (hex)              02 (hex)
    /// word3  : year (last modification time)
    /// word4  : month
    /// word5  : day
    /// word6  : hour
    /// word7  : minute
    /// word8  : second
    /// word9  : year (last access time)
    /// word10 : month
    /// word11 : day
    /// word12 : hour
    /// word13 : minute
    /// word14 : second
    /// ```
    BgnLib = 0x01,

    /// Contains a string which is the library name. The library name must follow UNIX filename
    /// conventions for length and valid characters. The library name may include the file
    /// extension (.sf or db in most cases).
    LibName = 0x02,

    /// Contains two eight-byte real numbers. The first number is the size of a database unit in
    /// user units. The second number is the size of a database unit in meters. For example, if you
    /// create a library with the default units (user unit = 1 micron and 1000 database units per
    /// user unit), the first number is .001, and the second number is 1E-9. Typically, the first
    /// number is less than 1, since you use more than 1 database unit per user unit. To calculate
    /// the size of a user unit in meters, divide the second number by the first.
    Units = 0x03,

    /// Marks the end of a library.
    EndLib = 0x04,

    /// Contains the creation time and last modification time of a structure (in the same format as
    /// the BGNLIB record), and marks the beginning of a structure.
    BgnStr = 0x05,

    /// Contains a string which is the structure name. A structure name may be up to 32 characters
    /// long. Legal structure name characters are:
    ///
    /// - A through Z
    /// - a through z
    /// - 0 through 9
    /// - Underscore (_)
    /// - Question mark (?)
    /// - Dollar sign ($)
    StrName = 0x06,

    /// Marks the end of a structure.
    EndStr = 0x07,

    /// Marks the beginning of a boundary element.
    Boundary = 0x08,

    /// Marks the beginning of a path element.
    Path = 0x09,

    /// Marks the beginning of an SREF (structure reference) element.
    Sref = 0x0A,

    /// Marks the beginning of an AREF (array reference) element.
    Aref = 0x0B,

    /// Marks the beginning of a text element.
    Text = 0x0C,

    /// Contains two bytes which specify the layer. The value of the layer must be in the range of
    /// 0 to 255.
    Layer = 0x0D,

    /// Contains two bytes which specify the datatype. The value of the datatype must be in the
    /// range of 0 to 255.
    Datatype = 0x0E,

    /// Contains four bytes which specify the width of a path or text lines in database units. A
    /// negative value for width means that the width is absolute, that is, the width is not
    /// affected by the magnification factor of any parent reference. If omitted, zero is assumed.
    Width = 0x0F,

    /// Contains an array of XY coordinates in database units. Each X or Y coordinate is four bytes
    /// long.
    ///
    /// - Path elements may have a minimum of 2 and a maximum of 200 coordinates.
    /// - Boundary and border elements may have a minimum of 4 and a maximum of 200 coordinates.
    ///   The first and last coordinates of a boundary or border must coincide.
    /// - A text, or SREF element may have only one coordinate.
    /// - An AREF has exactly three coordinates. In an AREF, the first coordinate is the array
    ///   reference point (origin point). The other two coordinates are already rotated, reflected
    ///   as specified in the STRANS record (if specified). So in order to calculate the intercolumn
    ///   and interrow spacing, the coordinates must be mapped back to their original position, or
    ///   the vector length (x1,y1 -> x3,y3) must be divided by the number of rows etc. The second
    ///   coordinate locates a position which is displaced from the reference point by the
    ///   inter-column spacing times the number of columns. The third coordinate locates a position
    ///   which is displaced from the reference point by the inter-row spacing times the number of
    ///   rows.
    /// - A node may have from one to 50 coordinates.
    /// - A box must have five coordinates, with the first and last coordinates being the same.
    Xy = 0x10,

    /// Marks the end of an element.
    Endel = 0x11,

    /// Contains the name of a referenced structure.
    Sname = 0x12,

    /// Contains four bytes. The first two bytes contain the number of columns in the array. The
    /// third and fourth bytes contain the number of rows. Neither the number of columns nor the
    /// number of rows may exceed 32,767 (decimal), and both are positive.
    Colrow = 0x13,

    /// Marks the beginning of a node.
    Node = 0x15,

    /// Contains two bytes representing texttype. The value of the texttype must be in the range
    /// 0 to 255.
    TextType = 0x16,

    /// Contains one word (two bytes) of bit flags for text presentation. Bits 10 and 11, taken
    /// together as a binary number, specify the font (00 means font 0, 01 means font 1, 10 means
    /// font 2, and 11 means font 3). Bits 12 and 13 specify the vertical justification (00 means
    /// top, 01 means middle, and 10 means bottom). Bits 14 and 15 specify the horizontal
    /// justification (00 means left, 01 means center, and 10 means right). Bits 0 through 9 are
    /// reserved for future use and must be cleared. If this record is omitted, then top-left
    /// justification and font 0 are assumed.
    ///
    /// ```text
    ///         Bit 0-9      Bit 10-11     Bit 12-13       Bit 14-15
    /// word3 : unused       font number   vertical        horizontal
    ///                                    presentation    presentation
    /// ```
    Presentation = 0x17,

    /// Contains a character string, up to 512 characters long, for text presentation.
    String = 0x19,

    /// Contains two bytes of bit flags for SREF, AREF, and text transformation. Bit 0 (the
    /// leftmost bit) specifies reflection. If bit 0 is set, the element is reflected about the
    /// X-axis before angular rotation. For an AREF, the entire array is reflected, with the
    /// individual array members rigidly attached. Bit 13 flags absolute magnification. Bit 14
    /// flags absolute angle. Bit 15 (the rightmost bit) and all remaining bits are reserved for
    /// future use and must be cleared. If this record is omitted, the element is assumed to have
    /// no reflection, non-absolute magnification, and non-absolute angle.
    ///
    /// ```text
    ///         Bit 0        Bit 1-12   Bit 13       Bit 14     Bit 15
    /// word3 : reflection   unused     absolute     absolute   unused
    ///                                 magnification angle
    /// ```
    Strans = 0x1A,

    /// Contains a double-precision real number (8 bytes), which is the magnification factor. If
    /// this record is omitted, a magnification factor of one is assumed.
    Mag = 0x1B,

    /// Contains a double-precision real number (8 bytes), which is the angular rotation factor.
    /// The angle of rotation is measured in degrees and in the counterclockwise direction. For an
    /// AREF, the ANGLE rotates the entire array (with the individual array members rigidly
    /// attached) about the array reference point. For COLROW record information, the angle of
    /// rotation is already included in the coordinates. If this record is omitted, an angle of
    /// zero degrees is assumed.
    Angle = 0x1C,

    /// Contains the names of the reference libraries. This record must be present if any reference
    /// libraries are bound to the working library. The name of the first reference library starts
    /// at byte 5 (immediately following the record header) and continues for 44 bytes. The next 44
    /// bytes contain the name of the second library. The record is extended by 44 bytes for each
    /// additional library (up to 15) which is bound for reference. The reference library names may
    /// include directory specifiers (separated with "/") and an extension (separated with "."). If
    /// either the first or second library is not named, its place is filled with nulls.
    Reflibs = 0x1F,

    /// Contains the names of the textfont definition files. This record must be present if any of
    /// the four fonts have a corresponding textfont definition file. This record must not be
    /// present if none of the fonts have a textfont definition file. The textfont filename of font
    /// 0 starts the record, followed by the textfont files of the remaining three fonts. Each
    /// filename is 44 bytes long. The filename is padded with nulls if the name is shorter than 44
    /// bytes. The filename is null if no textfont definition corresponds to the font. The textfont
    /// filenames may include directory specifiers (separated with "/") and an extension (separated
    /// with ".").
    Fonts = 0x20,

    /// Contains a value that describes the type of path endpoints. The value is:
    ///
    /// - 0 for square-ended paths that end flush with their endpoints
    /// - 1 for round-ended paths
    /// - 2 for square-ended paths that extend a half-width beyond their endpoints
    ///
    /// If not specified, a Pathtype of 0 is assumed.
    Pathtype = 0x21,

    /// Contains a value to indicate the number of copies of deleted or back-up structures to
    /// retain. This number must be at least 2 and not more than 99. If the GENERATIONS record is
    /// omitted, a value of 3 is assumed.
    Generations = 0x22,

    /// Contains the name of the attribute definition file. This record is present only if an
    /// attribute definition file is bound to the library. The attribute definition filename may
    /// include directory specifiers (separated with "/") and an extension (separated with ".").
    /// Maximum record size is 44 bytes.
    Attrtable = 0x23,

    /// Contains two bytes of bit flags. Bit 15 (the rightmost bit) specifies Template data. Bit
    /// 14 specifies External data (also referred to as Exterior data). All other bits are
    /// currently unused and must be cleared to 0. If this record is omitted, all bits are assumed
    /// to be 0.
    ///
    /// ```text
    ///         Bit 0-13   Bit 14     Bit 15
    /// word3 : unused     external   template
    ///                    data       data
    /// ```
    Elflags = 0x26,

    /// Contains two bytes which specify nodetype. The value of the nodetype must be in the range
    /// of 0 to 255.
    Nodetype = 0x2A,

    /// Contains two bytes which specify the attribute number. The attribute number is an integer
    /// from 1 to 127. Attribute numbers 126 and 127 are reserved for the user integer and user
    /// string (CSD) properties which existed prior to Release 3.0.
    Propattr = 0x2B,

    /// Contains the string value associated with the attribute named in the preceding PROPATTR
    /// record. Maximum length is 126 characters. The attribute-value pairs associated with any one
    /// element must all have distinct attribute numbers. Also, the total amount of property data
    /// that may be associated with any one element is limited: the total length of all the strings,
    /// plus twice the number of attribute-value pairs, must not exceed 128 (or 512 if the element
    /// is an SREF, AREF, contact, node port, or node).
    Propvalue = 0x2C,

    /// Marks the beginning of a box element.
    Box = 0x2D,

    /// Contains two bytes which specify boxtype. The value of the boxtype must be in the range of
    /// 0 to 255.
    BoxType = 0x2E,

    /// A unique positive number which is common to all elements of the plex to which this element
    /// belongs. The head of the plex is flagged by setting the seventh bit; therefore, plex
    /// numbers should be small enough to occupy only the right-most 24 bits. If this record is not
    /// present, the element is not a plex member.
    Plex = 0x2F,

    /// Contains two bytes which specify the number of the current reel of tape for a multi-reel
    /// Stream file. For the first tape, the TAPENUM is 1; for the second tape, the TAPENUM is 2.
    /// For each additional tape, increment the TAPENUM by one.
    TapeNum = 0x32,

    /// Contains 12 bytes. This is a unique 6-integer code which is common to all the reels of a
    /// multi-reel Stream file. It verifies that the correct reels are being read.
    TapeCode = 0x33,

    /// Defines the format of a Stream tape in two bytes. The possible values are:
    ///
    /// 1. for GDSII Archive format
    /// 2. for GDSII Filtered format
    /// 3. for EDSM Archive format
    /// 4. for EDSHI Filtered format
    ///
    /// An Archive Stream file contains elements for all the layers and data types. In an Archive
    /// Stream file, the FORMAT record is followed immediately by the UNITS record. A file which
    /// does not have the FORMAT record is assumed to be an Archive file.
    ///
    /// A Filtered Stream file contains only the elements on the layers and with the datatypes you
    /// specify during creation of the Stream file. The list of layers and datatypes specified
    /// appear in MASK records. At least one MASK record must immediately follow the FORMAT record.
    /// The MASK records are terminated with the ENDMASKS record.
    Format = 0x36,

    /// Required for and present only in Filtered Stream files. Contains the list of layers and
    /// datatypes specified by the user when creating the file. At least one MASK record must
    /// immediately follow the FORMAT record. More than one MASK record may occur. The last MASK
    /// record is followed by the ENDMASKS record. In the MASK list, datatypes are separated from
    /// the layers with a semicolon. Individual layers or datatypes are separated with a space. A
    /// range of layers or datatypes is specified with a dash.
    Mask = 0x37,

    /// Required for and present only in Filtered Stream files. Marks the end of the MASK records.
    /// The ENDMASKS record must follow the last MASK record. ENDMASKS is immediately followed by
    /// the UNITS record.
    EndMasks = 0x38,
}

/// Version of GDS on the source.
///
/// For reference, see <http://www.layouteditor.net/wiki/GDSII>.
#[repr(i16)]
pub enum GdsVersion {
    ///  "Version 3 of the GDS II file format limits the maximum size of polygons/path elements to
    ///  200 vertices."
    V3 = 3,
    /// "Box elements were introduced with version 4 of the GDS II file format."
    V4 = 4,
    /// "The technical limit of the file format structure is 8191 points, which is allowed in
    /// version 7."
    V7 = 7,
}
/// GDSII Data Type (1 byte)
#[repr(u8)]
#[derive(
    Debug, PartialEq, Eq, Clone, Copy, IntoBytes, TryFromBytes, Unaligned, KnownLayout, Immutable,
)]
pub enum DataType {
    NoData = 0x00,
    /// A bit array is a word which uses the value of a particular bit or group of bits to
    /// represent data. A bit array allows oneword to represent a number of simple pieces of
    /// information.
    BitArray = 0x01,
    /// 2-byte integer = 1 word 2s-complement representation. The range of two-byte signed integers
    /// is -32,768 to 32,767.
    ///
    /// The following is a representation of a two-byte integer, where S is the sign and M is the
    /// magnitude:
    ///
    /// ```text
    /// smmmmmmm mmmmmmmm
    /// ```
    ///
    /// The following are examples of two-byte integers:
    ///
    /// ```
    /// 00000000 00000001 = 1
    /// 00000000 00000010 = 2
    /// 00000000 10001001 = 137
    /// 11111111 11111111 = -1
    /// 11111111 11111110 = -2
    /// 11111111 01110111 = -137
    /// ```
    TwoByteSignedInt = 0x02,
    /// 4-byte integer = 2 word 2s-complement representation
    ///
    /// The range of four-byte signed integers is -2,147,483,648 to 2,147,483,647.
    ///
    /// The following is a representation of a four-byte integer, where S is the sign and M is the magnitude.
    ///
    /// ```text
    /// smmmmmmm mmmmmmmm mmmmmmmm mmmmmmmm
    /// ```
    ///
    /// The following are examples of four-byte integers:
    ///
    /// ```text
    /// 00000000 00000000 00000000 00000001 = 1
    /// 00000000 00000000 00000000 00000010 = 2
    /// 00000000 00000000 00000000 10001001 = 137
    /// 11111111 11111111 11111111 11111111 = -1
    /// 11111111 11111111 11111111 11111110 = -2
    /// 11111111 11111111 11111111 01110111 = -137
    /// ```
    FourByteSignedInt = 0x03,
    /// 4-byte real = 2-word floating point representation (See `EightByteReal`).
    ///
    /// NOTE: this is not used in practice.
    #[allow(dead_code, reason = "This data type is not used in practice")]
    FourByteReal = 0x04,
    /// 8-byte real = 4-word floating point representation
    ///
    /// For all non-zero values:
    ///
    /// * A floating point number has three parts: the sign, the exponent, and the mantissa.
    /// * The value of a floating point number is defined as:
    /// * (Mantissa) x (16 raised to the true value of the exponent field).
    /// * The exponent field (bits 1-7) is in Excess-64 representation.
    /// * The 7-bit field shows a number that is 64 greater than the actual exponent.
    /// * The mantissa is always a positive fraction >=1/16 and <1. For a 4-byte real, the mantissa
    ///   is bits 8-31. For an 8-byte real, the mantissa is bits 8-63.
    /// * The binary point is just to the left of bit 8.
    /// * Bit 8 represents the value 1/2, bit 9 represents 1/4, etc.
    /// * In order to keep the mantissa in the range of 1/16 to 1, the results of floating point
    ///   arithmetic are normalized. Normalization is a process where by the mantissa is shifted left
    ///   one hex digit at a time until its left FOUR bits represent a non-zero quantity. For every
    ///   hex digit shifted, the exponent is decreased by one. Since the mantissa is shifted four
    ///   bits at a time, it is possible for the left three bits of the normalized mantissa to be
    ///   zero. A zero value, also called true zero, is represented by a number with all bits zero.
    ///
    /// The following are representations of 4-byte and 8-byte reals, where S is the sign, E is the
    /// exponent, and M is the magnitude. Examples of 4-byte reals are included in the following
    /// pages, but 4-byte reals are not used currently. The representation of the negative values
    /// of real numbers is exactly the same as the positive, except that the highest order bit is
    /// 1, not 0. In the eight-byte real representation, the first four bytes are exactly the same
    /// as in the four-byte real representation. The last four bytes contain additional binary
    /// places for more resolution.
    ///
    /// 4-byte real:
    ///
    /// ```text
    /// SEEEEEEE MMMMMMMM MMMMMMMM MMMMMMMM
    /// ```
    ///
    /// 8-byte real:
    ///
    /// ```text
    /// SEEEEEEE MMMMMMMM MMMMMMMM MMMMMMMM MMMMMMMM MMMMMMMM MMMMMMMM
    /// ```
    ///
    /// Examples of 4-byte real:
    ///
    /// NOTE: In the first six lines of the following example, the 7-bit exponent field = 65. The
    /// actual exponent is 65-64=1.
    ///
    /// ```text
    /// 01000001 00010000 00000000 00000000 = 1
    /// 01000001 00100000 00000000 00000000 = 2
    /// 01000001 00110000 00000000 00000000 = 3
    /// 11000001 00010000 00000000 00000000 = -1
    /// 11000001 00100000 00000000 00000000 = -2
    /// 11000001 00110000 00000000 00000000 = -3
    /// 01000000 10000000 00000000 0000000 = 0 .5
    /// 01000000 10011001 10011001 1001100 = 1 .6
    /// 01000000 10110011 00110011 0011001 = 1 .7
    /// 01000001 00011000 00000000 00000000 = 1.5
    /// 01000001 00011001 10011001 10011001 = 1.6
    /// 01000001 00011011 00110011 00110011 = 1.7
    /// 00000000 00000000 00000000 00000000 = 0
    /// 01000001 00010000 00000000 00000000 = 1
    /// 01000001 10100000 00000000 00000000 = 10
    /// 01000010 01100100 00000000 00000000 = 100
    /// 01000011 00111110 00000001 00000000 = 1000
    /// 01000100 00100111 00010000 00000000 = 10000
    /// 01000101 00011000 01101010 00000000 = 100000
    /// ```
    EightByteReal = 0x05,
    /// A collection of ASCII characters, where each character is represented by one byte. All odd
    /// length strings must be padded with a null character (the number zero), and the byte count
    /// for the record containing the ASCII string must include this null character. Stream read-in
    /// programs must look for the null character and decrease the length of the string by one if
    /// the null character is present.
    AsciiString = 0x06,
}

/// GDSII Record header
///
/// The Stream format output file is composed of variable length records. Record length is measured
/// in bytes. The minimum record length is four bytes. Within the record, two bytes (16 bits) is a
/// word. The 16 bits in a word are numbered 0 to 15, left to right. The first four bytes of a
/// record compose the recordheader. The first two bytes of the recordheader contain a count (in
/// eight-bit bytes) of the total record length, so the maximum length is 65536 (64k). The next
/// record starts immediately after the last byte of the previous record.The third byte of the
/// header is the record type. The fourth byte of the header identifies the type of data contained
/// within the record. The fifth until count bytes of a record contain the data.
#[repr(C)]
#[derive(
    TryFromBytes, PartialEq, Eq, IntoBytes, Debug, Unaligned, KnownLayout, Immutable, Copy, Clone,
)]
pub struct RecordHeader {
    /// Count (in eight-bit bytes) of the total record length, so the maximum length is 65536
    /// (64k).
    length: U16,
    /// Associated record type of header (1 byte).
    record_type: RecordType,
    /// Associated data type of header (1 byte).
    data_type: DataType,
}

impl RecordHeader {
    #[must_use]
    pub const fn new(length: U16, record_type: RecordType, data_type: DataType) -> Self {
        Self {
            length,
            record_type,
            data_type,
        }
    }

    #[must_use]
    pub const fn length(&self) -> U16 {
        self.length
    }

    #[must_use]
    pub const fn record_type(&self) -> RecordType {
        self.record_type
    }

    #[must_use]
    pub const fn data_type(&self) -> DataType {
        self.data_type
    }
}
