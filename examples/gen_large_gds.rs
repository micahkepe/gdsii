#![allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_precision_loss)]
/*!
Generate a synthetic large GDS file for benchmarking.

Usage:

```text
cargo run --release --example gen_large_gds -- <output.gds> [size_mb]
```

Default size: 300MB. Generates a realistic IC layout structure with:
- Leaf cells containing dense polygon arrays (standard cells)
- Mid-level cells instancing leaf cells in grids (blocks)
- A top cell instancing blocks in a chip-scale grid
*/

use gdsii::parser::{
    Aref, Boundary, Element, GdsEvent, LibraryBegin, Sref, StructureBegin,
};
use gdsii::writer::GdsWriter;
use gdsii::{I16, I32};

#[allow(clippy::too_many_lines)]
fn main() {
    let output =
        std::env::args().nth(1).unwrap_or_else(|| "large_bench.gds".to_owned());
    let target_mb: usize =
        std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(300);

    eprintln!("Generating ~{target_mb}MB GDS file: {output}");

    let mut buf = Vec::with_capacity(target_mb * 1024 * 1024);
    let mut writer = GdsWriter::new(&mut buf);

    let timestamps = [I16::new(0); 12];

    writer
        .write_event(&GdsEvent::LibraryBegin(LibraryBegin {
            version: 5,
            timestamps: &timestamps,
            lib_name: "BENCH",
            db_in_user: 0.001,
            db_in_meters: 1e-9,
            reflibs: None,
            fonts: None,
            attrtable: None,
            generations: None,
        }))
        .unwrap();

    // Leaf cells: small standard-cell-like polygons.
    // Each leaf has ~20 boundaries (transistors, metals, vias).
    let leaf_count = 50;
    let polys_per_leaf = 20;

    for leaf_idx in 0..leaf_count {
        let name = format!("stdcell_{leaf_idx}");
        writer
            .write_event(&GdsEvent::StructureBegin(StructureBegin {
                timestamps: &timestamps,
                name: &name,
            }))
            .unwrap();

        for poly_idx in 0..polys_per_leaf {
            let x0 = poly_idx * 500;
            let y0 = 0i32;
            let x1 = x0 + 400;
            let y1 = 2000;
            let layer = (poly_idx % 6) as i16 + 1;
            let xy = [
                I32::new(x0),
                I32::new(y0),
                I32::new(x1),
                I32::new(y0),
                I32::new(x1),
                I32::new(y1),
                I32::new(x0),
                I32::new(y1),
                I32::new(x0),
                I32::new(y0),
            ];
            writer
                .write_event(&GdsEvent::Element(Element::Boundary(Boundary {
                    elflags: None,
                    plex: None,
                    layer,
                    datatype: 0,
                    xy: &xy,
                })))
                .unwrap();
        }

        writer.write_event(&GdsEvent::StructureEnd).unwrap();
    }

    // Mid-level cells: arrays of leaf cells (logic blocks).
    // Each block is a grid of standard cells.
    let block_count = 20;
    let cols_per_block: i16 = 100;
    let rows_per_block: i16 = 50;

    for block_idx in 0..block_count {
        let name = format!("block_{block_idx}");
        let leaf_name = format!("stdcell_{}", block_idx % leaf_count);
        writer
            .write_event(&GdsEvent::StructureBegin(StructureBegin {
                timestamps: &timestamps,
                name: &name,
            }))
            .unwrap();

        let col_spacing = 10_000i32;
        let row_spacing = 3_000i32;
        let xy = [
            I32::new(0),
            I32::new(0),
            I32::new(i32::from(cols_per_block) * col_spacing),
            I32::new(0),
            I32::new(0),
            I32::new(i32::from(rows_per_block) * row_spacing),
        ];
        writer
            .write_event(&GdsEvent::Element(Element::Aref(Aref {
                elflags: None,
                plex: None,
                sname: &leaf_name,
                strans: None,
                colrow: (cols_per_block, rows_per_block),
                xy: &xy,
            })))
            .unwrap();

        // Each block also has some direct geometry (power rails, fill).
        // Scale boundary count to reach target file size.
        let fill_polys = (target_mb * 1024 * 24 / block_count) as usize;
        for fill_idx in 0..fill_polys {
            let x0 = ((fill_idx % 500) * 200) as i32;
            let y0 = ((fill_idx / 500) * 100) as i32;
            let x1 = x0 + 180;
            let y1 = y0 + 80;
            let xy = [
                I32::new(x0),
                I32::new(y0),
                I32::new(x1),
                I32::new(y0),
                I32::new(x1),
                I32::new(y1),
                I32::new(x0),
                I32::new(y1),
                I32::new(x0),
                I32::new(y0),
            ];
            writer
                .write_event(&GdsEvent::Element(Element::Boundary(Boundary {
                    elflags: None,
                    plex: None,
                    layer: 10 + (fill_idx % 4) as i16,
                    datatype: 0,
                    xy: &xy,
                })))
                .unwrap();
        }

        writer.write_event(&GdsEvent::StructureEnd).unwrap();
    }

    // Top cell: grid of blocks.
    writer
        .write_event(&GdsEvent::StructureBegin(StructureBegin {
            timestamps: &timestamps,
            name: "TOP",
        }))
        .unwrap();

    for (i, block_idx) in (0..block_count).enumerate() {
        let block_name = format!("block_{block_idx}");
        let x = ((i % 5) as i32)
            .checked_mul(2_000_000)
            .expect("unable to get x coord");
        let y = (i / 5) as i32 * 500_000;
        let xy = [I32::new(x), I32::new(y)];
        writer
            .write_event(&GdsEvent::Element(Element::Sref(Sref {
                elflags: None,
                plex: None,
                sname: &block_name,
                strans: None,
                xy: &xy,
            })))
            .unwrap();
    }

    writer.write_event(&GdsEvent::StructureEnd).unwrap();
    writer.write_event(&GdsEvent::LibraryEnd).unwrap();

    let size_mb = buf.len() as f64 / (1024.0 * 1024.0);
    eprintln!("Generated {size_mb:.1}MB ({} bytes)", buf.len());

    std::fs::write(&output, &buf).unwrap();
    eprintln!("Written to {output}");
}
