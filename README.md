# `gdsii`

Fast, zero-copy, streaming GDSII parser and writer for Rust.

Parses [GDSII](https://en.wikipedia.org/wiki/GDSII) binary layout files into a
[SAX](https://en.wikipedia.org/wiki/Simple_API_for_XML)-style event stream with
no heap allocation during parsing. All borrowed data references the original
input buffer. The writer serializes events back to spec-compliant GDSII bytes,
enabling read-transform-write pipelines with no intermediate tree.

## Features

- **Zero-copy parsing**: element data borrows directly from the input `&[u8]`
- **Streaming events**: `GdsParser` implements `Iterator<Item = Result<GdsEvent>>`
- **Lossless float roundtrips**: GDS base-16 reals encode/decode via IEEE 754
  bit extraction
- **Byte-exact writer**: `GdsWriter` produces output identical to the input for
  well-formed files
- **All element types**: Boundary, Path, Sref, Aref, Text, Node, Box, with
  ELFLAGS, PLEX, and properties

## Performance

Compared against [`gds21`](https://crates.io/crates/gds21) v0.2.0, which
allocates an owned tree with `String` and `Vec<GdsPoint>` per element.

| Size    | `gds21` | `gdsii` | Speedup | Throughput |
| ------- | ------- | ------- | ------- | ---------- |
| 1.64 MB | 2.8ms   | 0.9ms   | 3.2x    | 1.901 GB/s |
| 47 MB   | 88ms    | 25ms    | 3.6x    | 1.922 GB/s |
| 472 MB  | 868ms   | 245ms   | 3.5x    | 1.924 GB/s |
| 1.57 GB | 2,968ms | 818ms   | 3.6x    | 1.922 GB/s |

> [!NOTE]
> Benchmarked on Apple M5 Pro (single-threaded, `--release` profile).

To reproduce, generate synthetic files and run the benchmark:

```bash
# Generate test files (roughly 1 MB, 30 MB, 300 MB, 1 GB)
cargo run --release --example gen_large_gds -- bench_1mb.gds 1
cargo run --release --example gen_large_gds -- bench_30mb.gds 30
cargo run --release --example gen_large_gds -- bench_300mb.gds 300
cargo run --release --example gen_large_gds -- bench_1gb.gds 1000
```

Then:

```fish
for file in ./*.gds
    ./target/release/examples/bench_parse "$file"
    printf '\n%s\n\n' '---'
end
```

## Quick start

```rust
use gdsii::parser::{GdsParser, GdsEvent, Element};
use gdsii::writer::GdsWriter;

// Parse
let data = std::fs::read("layout.gds").unwrap();
for event in GdsParser::new(&data) {
    match event.unwrap() {
        GdsEvent::Element(Element::Boundary(b)) => {
            println!("layer={}, points={}", b.layer, b.xy.len() / 2);
        }
        _ => {}
    }
}

// Roundtrip
let events: Vec<_> = GdsParser::new(&data)
    .collect::<Result<_, _>>()
    .unwrap();
let mut out = Vec::new();
let mut writer = GdsWriter::new(&mut out);
for event in &events {
    writer.write_event(event).unwrap();
}
assert_eq!(out.len(), data.len());
```

## Event types

| Event            | Description                                                                    |
| ---------------- | ------------------------------------------------------------------------------ |
| `LibraryBegin`   | `HEADER` through `UNITS` (version, timestamps, name, units)                    |
| `StructureBegin` | `BGNSTR` + `STRNAME` (cell name and timestamps)                                |
| `Element`        | Complete element: `Boundary`, `Path`, `Sref`, `Aref`, `Text`, `Node`, or `Box` |
| `Property`       | `PROPATTR`/`PROPVALUE` pair following its element                              |
| `StructureEnd`   | End of current structure                                                       |
| `LibraryEnd`     | End of library (final event)                                                   |

## License

MIT
