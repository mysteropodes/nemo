//! Baseline-oracle and real-plan tests for the native SVG frame serializer.
//! Expected strings are literals or oracle rows; the reader recomputes the
//! geometry and paint from the raw plan records, never from the serializer.
use super::fixture::{
    default_document, empty_document, ordered_document, owner, plan, plan_reds,
    single_curve_document,
};
use super::reader::{parse, verify};
use super::*;
use crate::scheduler::OutputSpec;
use serde_json::json;
use vello::kurbo::PathEl;

const SINGLE_CURVE_LEN: usize = 287;
const SINGLE_CURVE_DOC: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"48\" viewBox=\"0 0 64 48\">\n<path d=\"M1,2 C1.5,2.25 11,2 11.5,2 C11.5,3.25 7.25,13.25 6.25,12.75 C5.25,12.25 1,2 1,2 Z\" fill=\"rgb(20%,40%,60%)\" fill-opacity=\"0.75\" fill-rule=\"nonzero\"/>\n</svg>";
const EMPTY_DOC: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"48\" viewBox=\"0 0 64 48\">\n\n</svg>";

fn size(spec_w: u32, spec_h: u32) -> SvgFrameSpec {
    SvgFrameSpec::new(spec_w, spec_h, 1 << 20).unwrap()
}

#[test]
fn single_curve_document_is_byte_exact_and_matches_the_compositor_contour() {
    let owner = owner("svg-single", &single_curve_document());
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let frame = serialize_svg_frame(&input, size(64, 48)).unwrap();
    assert_eq!(frame.bytes.len(), SINGLE_CURVE_LEN);
    assert_eq!(
        String::from_utf8(frame.bytes.clone()).unwrap(),
        SINGLE_CURVE_DOC
    );
    assert_eq!(frame.path_count, 1);
    let path_line = SINGLE_CURVE_DOC.lines().nth(2).unwrap();
    assert_eq!(svg_document(path_line, 64, 48), SINGLE_CURVE_DOC);
    let parsed = parse(&frame.bytes).unwrap();
    verify(&parsed, &input, (64, 48)).unwrap();
    // The painted contour the PNG compositor fills is the same curve.
    let elements = input
        .commands()
        .next()
        .unwrap()
        .contour()
        .commands()
        .to_vec();
    let path = &parsed.paths[0];
    assert_eq!(elements.len(), path.curves.len() + 2);
    let xy = |p: vello::kurbo::Point| [p.x, p.y];
    assert_eq!(
        elements[0],
        PathEl::MoveTo((path.start[0], path.start[1]).into())
    );
    for (element, curve) in elements[1..].iter().zip(&path.curves) {
        match element {
            PathEl::CurveTo(a, b, c) => assert_eq!([xy(*a), xy(*b), xy(*c)], *curve),
            other => panic!("unexpected element {other:?}"),
        }
    }
    assert_eq!(elements[elements.len() - 1], PathEl::ClosePath);
}

#[test]
fn default_document_keeps_unquantized_fractional_straight_rgba() {
    let owner = owner("svg-default", &default_document());
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let frame = serialize_svg_frame(&input, size(320, 180)).unwrap();
    assert_eq!(frame.path_count, 5);
    let text = String::from_utf8(frame.bytes.clone()).unwrap();
    assert_eq!(text.matches("fill=\"rgb(20.000000000000004%,40.00000000000001%,60.00000000000001%)\" fill-opacity=\"0.7500000000000001\" fill-rule=\"nonzero\"/>").count(), 5);
    verify(&parse(&frame.bytes).unwrap(), &input, (320, 180)).unwrap();
}

#[test]
fn path_order_follows_the_plan_not_storage_order() {
    for (displaced, expected) in [
        (false, ["10", "20", "30", "40", "50"]),
        (true, ["10", "20", "30", "50", "40"]),
    ] {
        let value = ordered_document(displaced);
        let stored: Vec<f64> = value["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["fill"]["r"].as_f64().unwrap())
            .collect();
        let owner = owner("svg-order", &value);
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let input = plan(&snapshot, 7);
        let reds = plan_reds(&input);
        assert_ne!(stored, reds, "storage order is not paint order");
        let frame = serialize_svg_frame(&input, size(64, 48)).unwrap();
        let parsed = parse(&frame.bytes).unwrap();
        let seen: Vec<&str> = parsed
            .paths
            .iter()
            .map(|p| p.percent_text[0].as_str())
            .collect();
        assert_eq!(seen, expected, "displaced={displaced}");
        verify(&parsed, &input, (64, 48)).unwrap();
    }
}

#[test]
fn dimensions_and_pins_come_from_the_output_spec_and_the_snapshot() {
    let owner = owner("svg-pins", &single_curve_document());
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    for format in ["svg", "rgba8"] {
        let output = OutputSpec::new("frame", format, 64, 48, "srgb", "straight").unwrap();
        let frame =
            serialize_svg_frame(&input, SvgFrameSpec::from_output(&output, 4096).unwrap()).unwrap();
        assert_eq!(frame.dimensions, (64, 48));
        let parsed = parse(&frame.bytes).unwrap();
        assert_eq!((parsed.width, parsed.height), (64, 48));
        assert_eq!(frame.pins.instance_id, snapshot.instance_id());
        assert_eq!(frame.pins.document_id, snapshot.document_id());
        assert_eq!(frame.pins.document_snapshot_id, snapshot.snapshot_id());
        assert_eq!(frame.pins.content_revision, snapshot.content_revision());
        assert_eq!(
            (
                frame.pins.content_revision,
                frame.pins.frame,
                frame.pins.context_id.as_str()
            ),
            (0, 7, "scene-root")
        );
    }
    let wide = OutputSpec::new("frame", "svg", 1920, 1080, "srgb", "straight").unwrap();
    let frame =
        serialize_svg_frame(&input, SvgFrameSpec::from_output(&wide, 4096).unwrap()).unwrap();
    assert_eq!(
        (parse(&frame.bytes).unwrap().width, frame.dimensions),
        (1920, (1920, 1080))
    );
}

#[test]
fn output_is_detached_from_the_returned_buffer_and_later_document_edits() {
    let mut original = owner("svg-detached", &single_curve_document());
    let snapshot = original.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let mut first = serialize_svg_frame(&input, size(64, 48)).unwrap();
    let pristine = first.bytes.clone();
    first.bytes.fill(b'x');
    first.pins.document_id.clear();
    let again = serialize_svg_frame(&input, size(64, 48)).unwrap();
    assert_eq!(again.bytes, pristine);
    assert_eq!(again.pins.document_id, snapshot.document_id());
    let fill = original
        .prepare_fill_json(
            &serde_json::to_vec(&json!({"command":"object.fill.set",
            "stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},
                "layerUid":"L0","strokeId":"a"},
            "fill":{"kind":"solid","r":1,"g":1,"b":0,"a":1}}))
            .unwrap(),
        )
        .unwrap();
    assert_eq!(original.commit_fill(fill), Ok(1));
    assert_eq!(
        serialize_svg_frame(&input, size(64, 48)).unwrap().bytes,
        pristine
    );
    let edited = original.acquire_snapshot(1).unwrap();
    let after = serialize_svg_frame(&plan(&edited, 7), size(64, 48)).unwrap();
    let text = String::from_utf8(after.bytes).unwrap();
    assert!(text.contains("fill=\"rgb(100%,100%,0%)\" fill-opacity=\"1\""));
    assert_eq!(after.pins.content_revision, 1);
    assert_ne!(after.pins.document_snapshot_id, snapshot.snapshot_id());
}

#[test]
fn byte_cap_is_exact_and_refuses_before_output_exists() {
    let owner = owner("svg-cap", &single_curve_document());
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let spec = |cap| SvgFrameSpec::new(64, 48, cap).unwrap();
    assert_eq!(
        measure_svg_frame(&input, spec(MAX_SVG_BYTES)),
        Ok(SINGLE_CURVE_LEN)
    );
    let exact = serialize_svg_frame(&input, spec(SINGLE_CURVE_LEN)).unwrap();
    assert_eq!(exact.bytes.len(), SINGLE_CURVE_LEN);
    for cap in [SINGLE_CURVE_LEN - 1, 100, 1] {
        let refused = SvgExportError::OutputTooLarge { limit: cap };
        assert_eq!(serialize_svg_frame(&input, spec(cap)).unwrap_err(), refused);
        assert_eq!(measure_svg_frame(&input, spec(cap)), Err(refused));
    }
}

#[test]
fn invalid_specs_are_refused() {
    for (w, h, cap, want) in [
        (0, 48, 10, SvgExportError::InvalidSize),
        (64, 0, 10, SvgExportError::InvalidSize),
        (64, 48, 0, SvgExportError::InvalidByteCap),
        (64, 48, MAX_SVG_BYTES + 1, SvgExportError::InvalidByteCap),
    ] {
        assert_eq!(SvgFrameSpec::new(w, h, cap), Err(want));
    }
    assert!(SvgFrameSpec::new(1, 1, MAX_SVG_BYTES).is_ok());
    assert_eq!(MAX_SVG_BYTES, 67_108_864);
}

#[test]
fn unsupported_records_are_typed_and_name_the_source_object_index() {
    let good = RecordFacts {
        family: "closed-solid-fill-cubic-path",
        geometry_kind: "cubic-path",
        closed: true,
        coordinate_space: "document-world",
        handle_space: "relative",
        fill_kind: "solid",
    };
    assert_eq!(admit_record(3, &good), Ok(()));
    let cases = [
        RecordFacts {
            family: "open-stroke-path",
            ..good
        },
        RecordFacts {
            geometry_kind: "polyline",
            ..good
        },
        RecordFacts {
            closed: false,
            ..good
        },
        RecordFacts {
            coordinate_space: "layer-local",
            ..good
        },
        RecordFacts {
            handle_space: "absolute",
            ..good
        },
        RecordFacts {
            fill_kind: "linear-gradient",
            ..good
        },
    ];
    for (index, facts) in cases.iter().enumerate() {
        match admit_record(40 + index, facts) {
            Err(SvgExportError::Unsupported {
                source_object_index,
                reason,
            }) => {
                assert_eq!(source_object_index, 40 + index);
                assert!(!reason.is_empty());
            }
            other => panic!("case {index}: {other:?}"),
        }
    }
}

#[test]
fn channels_outside_the_unit_interval_cannot_become_svg_paint() {
    for bad in [
        Some(1.5),
        Some(-0.1),
        Some(f64::NAN),
        Some(f64::INFINITY),
        None,
    ] {
        assert_eq!(unit_interval(bad), None, "{bad:?}");
    }
    for (good, kept) in [(0.0, 0.0), (1.0, 1.0), (0.25, 0.25), (-0.0, 0.0)] {
        assert_eq!(unit_interval(Some(good)), Some(kept));
    }
}

#[test]
fn empty_plan_is_the_bare_wrapper() {
    let owner = owner("svg-empty", &empty_document());
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let frame = serialize_svg_frame(&input, size(64, 48)).unwrap();
    assert_eq!(String::from_utf8(frame.bytes.clone()).unwrap(), EMPTY_DOC);
    assert_eq!((frame.path_count, frame.bytes.len()), (0, 130));
    assert!(parse(&frame.bytes).unwrap().paths.is_empty());
    verify(&parse(&frame.bytes).unwrap(), &input, (64, 48)).unwrap();
}

#[test]
fn actual_stored_masks_refuse_svg_measurement_and_serialization_before_subset_output() {
    use crate::object_codec::encode_storage_project;
    let mut value = default_document();
    for record in value["objects"].as_array_mut().unwrap() {
        if record["target"]["layerUid"] != "L0" {
            continue;
        }
        record["family"] = json!("editable-vector-path");
        record["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,
            "strokeCap":"butt","strokeJoin":"miter","miterLimit":10,"dashOffset":0,"opacity":0});
        record["vectorExt"] = json!({"isMask":true,"maskMode":"intersect"});
    }
    let original = owner("svg-real-mask-exit", &value);
    let snapshot = original.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(snapshot.document()).unwrap();
    let input = plan(&snapshot, 7);
    assert_eq!(input.mask_inputs().layer(1).unwrap().masks().len(), 3);
    assert_eq!(
        input.commands().len(),
        2,
        "never serialize the remaining unmasked subset"
    );
    let expected = SvgExportError::Unsupported {
        source_object_index: 2,
        reason: "native vector masks have no SVG representation",
    };
    assert_eq!(measure_svg_frame(&input, size(64, 48)), Err(expected));
    assert_eq!(
        serialize_svg_frame(&input, size(64, 48)).unwrap_err(),
        expected
    );
    // Even a one-byte cap must see mask admission before counting any header.
    let tiny = SvgFrameSpec::new(64, 48, 1).unwrap();
    assert_eq!(measure_svg_frame(&input, tiny), Err(expected));
    assert_eq!(serialize_svg_frame(&input, tiny).unwrap_err(), expected);
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    assert_eq!(
        (original.content_revision(), original.history_depths()),
        (0, (0, 0))
    );
}

#[test]
fn actual_zero_mask_document_keeps_the_original_svg_bytes_and_measurement() {
    let original = owner("svg-zero-mask-exit", &single_curve_document());
    let snapshot = original.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    assert!(input.mask_inputs().layers().is_empty());
    assert_eq!(
        measure_svg_frame(&input, size(64, 48)),
        Ok(SINGLE_CURVE_LEN)
    );
    let frame = serialize_svg_frame(&input, size(64, 48)).unwrap();
    assert_eq!(frame.bytes.as_slice(), SINGLE_CURVE_DOC.as_bytes());
    assert_eq!(frame.path_count, 1);
    assert_eq!(
        (original.content_revision(), original.history_depths()),
        (0, (0, 0))
    );
}
