use std::path::Path;

use gdsii::parser::{
    Boundary, Element, GdsEvent, GdsParser, LibraryBegin, StructureBegin,
};
use gdsii::reader::RecordIter;
use gdsii::writer::GdsWriter;
use gdsii::{I16, I32, MAX_XY_POINTS_PER_RECORD, RecordType};

const DATA_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data");

/// Semantic roundtrip for every `.gds` and `.cal` file in `tests/data/`.
///
/// For each file: read -> parse -> write -> parse again -> compare events.
/// Non-canonical encodings (e.g., non-standard GDS zero for angles) are
/// normalized by the writer, so byte-exact equality is not guaranteed.
#[test]
fn roundtrip_all_data_files() {
    let mut tested = 0;
    for entry in std::fs::read_dir(DATA_DIR).expect("cannot read tests/data/") {
        let entry = entry.expect("directory entry error");
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "gds" && ext != "cal" {
            continue;
        }
        roundtrip_file(&path);
        tested += 1;
    }
    assert!(tested > 0, "no .gds/.cal files found in tests/data/");
}

fn roundtrip_file(path: &Path) {
    let name = path.file_name().unwrap().to_string_lossy();
    let input = std::fs::read(path)
        .unwrap_or_else(|e| panic!("{name}: read failed: {e}"));

    let original: Vec<_> = GdsParser::new(&input)
        .collect::<Result<_, _>>()
        .unwrap_or_else(|e| panic!("{name}: parse failed: {e}"));

    let mut buf = Vec::with_capacity(input.len());
    // Byte-preserving a fixture whose polygons span several XY records requires
    // re-emitting that form, so enable it. Elements that fit in one record are
    // unaffected.
    let mut writer = GdsWriter::new(&mut buf).with_multi_xy(true);
    for event in &original {
        writer
            .write_event(event)
            .unwrap_or_else(|e| panic!("{name}: write failed: {e}"));
    }

    // Compare only the parsed portion — some files have trailing data after ENDLIB
    // (e.g., non-standard record types) that the parser rightfully ignores.
    assert!(
        buf.len() <= input.len(),
        "{name}: output ({}) longer than input ({})",
        buf.len(),
        input.len()
    );
    assert!(
        buf == input[..buf.len()],
        "{name}: output bytes differ from input (first diff at byte {})",
        buf.iter().zip(input.iter()).position(|(a, b)| a != b).unwrap_or(0)
    );
    if buf.len() < input.len() {
        eprintln!(
            "{name}: {trailing} trailing bytes after ENDLIB ignored",
            trailing = input.len() - buf.len()
        );
    }
}

/// A polygon with more vertices than one XY record can hold must be written as
/// several consecutive XY records, and reading that back must reproduce the
/// original element.
///
/// Exercises the whole path in one go: write the split form, parse it into a
/// single element, write it again, and require the bytes to match. This is the
/// shape real `KLayout` exports take.
#[test]
fn roundtrip_multi_xy_boundary_is_byte_identical() {
    // Past what a single XY record holds, so a continuation record is required.
    let points = MAX_XY_POINTS_PER_RECORD + 61;
    // Coordinates encode their own index, so a vertex dropped, duplicated or
    // reordered at a record boundary changes the result.
    let coords: Vec<I32> = (0..points)
        .flat_map(|i| {
            let i = i32::try_from(i).expect("point index fits i32");
            [I32::new(i), I32::new(-i)]
        })
        .collect();
    let timestamps = [I16::new(0); 12];

    let events = vec![
        GdsEvent::LibraryBegin(LibraryBegin {
            version: 600,
            timestamps: &timestamps,
            lib_name: "MULTIXY",
            db_in_user: 0.001,
            db_in_meters: 1e-9,
            reflibs: None,
            fonts: None,
            attrtable: None,
            generations: None,
        }),
        GdsEvent::StructureBegin(StructureBegin {
            timestamps: &timestamps,
            name: "TOP",
        }),
        GdsEvent::Element(Element::Boundary(Boundary {
            elflags: None,
            plex: None,
            layer: 1,
            datatype: 0,
            xy: coords[..].into(),
        })),
        GdsEvent::StructureEnd,
        GdsEvent::LibraryEnd,
    ];

    let mut first = Vec::new();
    let mut writer = GdsWriter::new(&mut first).with_multi_xy(true);
    for event in &events {
        writer.write_event(event).expect("first write failed");
    }

    // The written file really is in the split form.
    let xy_records = RecordIter::new(&first)
        .map(|r| r.expect("record parse failed"))
        .filter(|r| r.header.record_type() == RecordType::Xy)
        .count();
    assert_eq!(xy_records, 2, "expected a split XY body");

    // Reading it back yields one element carrying every vertex.
    let reparsed: Vec<_> = GdsParser::new(&first)
        .collect::<Result<_, _>>()
        .expect("reparse failed");
    let GdsEvent::Element(Element::Boundary(b)) = &reparsed[2] else {
        panic!("expected a Boundary element");
    };
    assert_eq!(b.xy.num_points(), points);
    assert_eq!(b.xy.as_slice(), &coords[..]);

    // And writing that back out reproduces the same bytes.
    let mut second = Vec::new();
    let mut writer = GdsWriter::new(&mut second).with_multi_xy(true);
    for event in &reparsed {
        writer.write_event(event).expect("second write failed");
    }
    assert_eq!(second, first, "roundtrip was not byte-identical");
}
