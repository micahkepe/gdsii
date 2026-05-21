use std::time::Instant;

use gdsii::parser::GdsParser;

fn main() {
    let path = std::env::args().nth(1).expect("usage: bench_parse <file.gds>");
    let data = std::fs::read(&path).expect("cannot read file");
    eprintln!("file: {path} ({} bytes)\n", data.len());

    let iterations = 10;

    // gds21 requires owned Vec<u8>
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = gds21::GdsLibrary::from_bytes(data.clone())
            .expect("gds21 parse error");
    }
    let gds21_elapsed = start.elapsed() / iterations;
    eprintln!("gds21 parse:    {gds21_elapsed:?}");

    let start = Instant::now();
    let mut event_count = 0;
    for _ in 0..iterations {
        event_count = 0;
        for event in GdsParser::new(&data) {
            let _ = event.expect("parse error");
            event_count += 1;
        }
    }
    let gdsii_elapsed = start.elapsed() / iterations;
    eprintln!("gdsii parse:    {gdsii_elapsed:?} ({event_count} events)");

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
    let roundtrip_elapsed = start.elapsed() / iterations;
    eprintln!("gdsii roundtrip: {roundtrip_elapsed:?}");

    eprintln!();
    let speedup = gds21_elapsed.as_secs_f64() / gdsii_elapsed.as_secs_f64();
    eprintln!("speedup (parse): {speedup:.1}x");
    eprintln!(
        "gdsii throughput: {:.0} MB/s",
        data.len() as f64 / gdsii_elapsed.as_secs_f64() / 1_000_000.0
    );
}
