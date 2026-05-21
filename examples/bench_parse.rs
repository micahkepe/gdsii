#![allow(clippy::cast_precision_loss)]
use std::time::{Duration, Instant};

use gdsii::parser::GdsParser;

/// Patch zero-valued GDS date fields so gds21/chrono don't panic.
fn patch_gds_dates(data: &[u8]) -> Vec<u8> {
    const BGNLIB: u16 = 0x0102;
    const BGNSTR: u16 = 0x0502;
    const VALID_DATE: [u8; 12] = [
        0x07, 0xe9, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let mut bytes = data.to_vec();
    let mut offset = 0;
    while offset + 4 <= bytes.len() {
        let rlen =
            usize::from(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]));
        let rtype = u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]]);
        if rlen < 4 || offset + rlen > bytes.len() {
            break;
        }
        if (rtype == BGNLIB || rtype == BGNSTR) && rlen >= 28 {
            for base in [offset + 4, offset + 16] {
                if base + 12 <= bytes.len()
                    && bytes[base..base + 12].iter().all(|&b| b == 0)
                {
                    bytes[base..base + 12].copy_from_slice(&VALID_DATE);
                }
            }
        }
        offset += rlen;
    }
    bytes
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: bench_parse <file.gds>");
    let data = std::fs::read(&path).expect("cannot read file");
    eprintln!("file: {path} ({} bytes)\n", data.len());

    let iterations: i32 = if data.len() > 100_000_000 { 3 } else { 10 };

    // gds21 requires owned Vec<u8>; patch zero timestamps to avoid chrono panic
    let patched = patch_gds_dates(&data);
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = gds21::GdsLibrary::from_bytes(patched.clone())
            .expect("gds21 parse error");
    }
    let gds21_elapsed = start.elapsed() / iterations.cast_unsigned();
    eprintln!("gds21 parse:\t\t{gds21_elapsed:?}");

    // gdsii: zero-copy streaming parse
    let start = Instant::now();
    let mut event_count = 0;
    for _ in 0..iterations {
        event_count = 0;
        for event in GdsParser::new(&data) {
            let _ = event.expect("parse error");
            event_count += 1;
        }
    }
    let gdsii_elapsed = start.elapsed() / iterations.cast_unsigned();
    eprintln!("gdsii parse:\t\t{gdsii_elapsed:?} ({event_count} events)");

    // gdsii: parse + write roundtrip
    let start = Instant::now();
    for _ in 0..iterations {
        let events: Vec<_> = GdsParser::new(&data)
            .collect::<Result<_, _>>()
            .expect("parse error");
        let mut buf = Vec::with_capacity(data.len());
        let mut writer = gdsii::writer::GdsWriter::new(&mut buf);
        for event in &events {
            writer.write_event(event).expect("write error");
        }
    }
    let roundtrip_elapsed: Duration =
        start.elapsed() / iterations.cast_unsigned();
    eprintln!("gdsii roundtrip:\t{roundtrip_elapsed:?}");

    eprintln!();
    let speedup = gds21_elapsed.as_secs_f64() / gdsii_elapsed.as_secs_f64();
    eprintln!("speedup (parse):\t{speedup:.1}x");
    eprintln!(
        "gdsii throughput:\t{:.0} MB/s",
        data.len() as f64 / gdsii_elapsed.as_secs_f64() / 1_000_000.0
    );
}
