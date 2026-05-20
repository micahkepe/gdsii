use gdsii::parser::{Element, GdsEvent, GdsParser};

const EXAMPLE: &[u8] = include_bytes!("data/example.cal");

#[test]
fn parse_example_cal_full_event_sequence() {
    let events: Vec<_> = GdsParser::new(EXAMPLE)
        .collect::<Result<Vec<_>, _>>()
        .expect("parse failed");

    // Expected 15 events verified against hex dump of example.cal:
    // LibraryBegin → StructureBegin("AAP") → Boundary(1) → StructureEnd
    // → StructureBegin("LAYOUT") → Boundary(0) → Box(2) → Sref("AAP")
    // → Path(3,width=100000) → Text("Boundary") → Text("Path")
    // → Text("Sref") → Text("Box") → StructureEnd → LibraryEnd
    assert_eq!(events.len(), 15, "event count mismatch: {events:#?}");

    // Library header
    let GdsEvent::LibraryBegin(lib) = &events[0] else {
        panic!("expected LibraryBegin, got {:?}", events[0]);
    };
    assert_eq!(lib.version, 5);
    assert_eq!(lib.lib_name, "TEMPEGS.DB");
    assert_eq!(lib.timestamps.len(), 12);

    // Structure "AAP"
    let GdsEvent::StructureBegin(s) = &events[1] else {
        panic!("expected StructureBegin");
    };
    assert_eq!(s.name, "AAP");

    // Boundary in AAP: layer=1, datatype=0, 5 coordinate pairs
    let GdsEvent::Element(Element::Boundary(b)) = &events[2] else {
        panic!("expected Boundary");
    };
    assert_eq!(b.layer, 1);
    assert_eq!(b.datatype, 0);
    assert_eq!(b.xy.len(), 10);

    assert!(matches!(events[3], GdsEvent::StructureEnd));

    // Structure "LAYOUT"
    let GdsEvent::StructureBegin(s) = &events[4] else {
        panic!("expected StructureBegin");
    };
    assert_eq!(s.name, "LAYOUT");

    // Boundary: layer=0
    let GdsEvent::Element(Element::Boundary(b)) = &events[5] else {
        panic!("expected Boundary");
    };
    assert_eq!(b.layer, 0);
    assert_eq!(b.datatype, 0);
    assert_eq!(b.xy.len(), 10);

    // Box: layer=2
    let GdsEvent::Element(Element::Box(bx)) = &events[6] else {
        panic!("expected Box");
    };
    assert_eq!(bx.layer, 2);
    assert_eq!(bx.boxtype, 0);
    assert_eq!(bx.xy.len(), 10);

    // Sref: sname="AAP"
    let GdsEvent::Element(Element::Sref(sr)) = &events[7] else {
        panic!("expected Sref");
    };
    assert_eq!(sr.sname, "AAP");
    assert_eq!(sr.xy.len(), 2);

    // Path: layer=3, width=100000
    let GdsEvent::Element(Element::Path(p)) = &events[8] else {
        panic!("expected Path");
    };
    assert_eq!(p.layer, 3);
    assert_eq!(p.datatype, 0);
    assert_eq!(p.pathtype, None);
    assert_eq!(p.width, Some(100_000));
    assert_eq!(p.xy.len(), 8);

    // Four text elements
    let expected_strings = ["Boundary", "Path", "Sref", "Box"];
    for (i, expected) in expected_strings.iter().enumerate() {
        let GdsEvent::Element(Element::Text(t)) = &events[9 + i] else {
            panic!("expected Text at index {}", 9 + i);
        };
        assert_eq!(t.layer, 3);
        assert_eq!(t.texttype, 0);
        assert_eq!(t.presentation, Some(0x0008));
        assert_eq!(t.pathtype, Some(1));
        assert!(t.strans.is_some());
        assert_eq!(t.string, *expected);
        assert_eq!(t.xy.len(), 2);
    }

    assert!(matches!(events[13], GdsEvent::StructureEnd));
    assert!(matches!(events[14], GdsEvent::LibraryEnd));
}

#[test]
fn empty_input_yields_eof_error() {
    let mut parser = GdsParser::new(&[] as &[u8]);
    let first = parser.next().expect("should yield one error");
    assert!(first.is_err());
    assert!(parser.next().is_none(), "should be fused after error");
}
