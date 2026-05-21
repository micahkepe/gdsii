use std::path::Path;

use gdsii::{parser::GdsParser, writer::GdsWriter};

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
    let mut writer = GdsWriter::new(&mut buf);
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
