//! L10-H: the plan -> record conversion against the baseline `serP` oracle. The expected native
//! record of each oracle row is derived in `rows.rs` from the baseline serP column by a mapping
//! independent of the production code, then compared number by number; hand-written literals pin
//! the same facts again.
use super::rows::{
    expected_from_serp, oracle, production_input, same_json, spelled_like_json_stringify, target,
};
use super::support::*;
use super::svg::{js_number, path_record, r3, StrokeExtras, MAX_SEGMENTS, MIN_SEGMENTS};
use super::ImportError;
use crate::history::object_history::object_shot_kernels::native_import_svg_explode::{
    Contour, ExplodedPath, LeafStyle, PathOrigin, Seg,
};
use crate::object_document::ObjectLayer;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn seg(x: f64, y: f64) -> Seg {
    Seg {
        point: [x, y],
        handle_in: [0.0, 0.0],
        handle_out: [0.0, 0.0],
    }
}

#[test]
fn every_oracle_pair_converts_to_the_record_the_baseline_serp_persisted() {
    let (meta, _, pairs) = oracle();
    assert_eq!(pairs.len(), 17);
    assert!(meta.contains(&("pairs".into(), "17".into())));
    assert!(meta.contains(&("baseline".into(), BASELINE.into())));
    for pair in &pairs {
        let (path, extras) = production_input(&pair.input);
        let record = path_record(&path, &extras, target()).unwrap();
        let produced = serde_json::to_value(&record).unwrap();
        let expected = expected_from_serp(&pair.serp);
        assert!(
            produced.get("vectorExt").is_none(),
            "{}: an SVG path carries no pressure/paramShape/boxAngle (L01-E vectorExt stays absent)",
            pair.id
        );
        assert!(
            same_json(&produced, &expected),
            "{}:\n{produced}\n{expected}",
            pair.id
        );
        assert!(
            spelled_like_json_stringify(&produced),
            "{}: {produced}",
            pair.id
        );
    }
}

#[test]
fn literal_pins_of_three_rows_survive_independently_of_the_derived_mapping() {
    let (_, _, pairs) = oracle();
    let record = |id: &str| {
        let pair = pairs.iter().find(|p| p.id == id).unwrap();
        let (path, extras) = production_input(&pair.input);
        serde_json::to_value(path_record(&path, &extras, target()).unwrap()).unwrap()
    };
    let alpha = record("alpha-fill-and-stroke");
    assert_eq!(
        alpha["fill"],
        json!({"kind":"solid","r":0.06666666666666667,
        "g":0.13333333333333333,"b":0.2,"a":0.5019607843137255})
    );
    assert_eq!(
        alpha["vector"]["stroke"],
        json!({"kind":"solid","r":0.2,
        "g":0.26666666666666666,"b":0.3333333333333333,"a":0.4})
    );
    assert_eq!(alpha["vector"]["hasRealStroke"], true);
    let fill_only = record("closed-rect-fill-only");
    assert_eq!(
        fill_only["vector"]["stroke"],
        Value::Null,
        "the #ffffff fallback is not a stroke"
    );
    assert_eq!(fill_only["vector"]["hasRealStroke"], false);
    assert_eq!(
        fill_only["fill"],
        json!({"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":1})
    );
    let rounded = record("coordinates-rounded-to-three");
    assert_eq!(
        rounded["geometry"]["segments"],
        json!([
        {"point":{"x":1.235,"y":-3},"handleIn":{"x":0,"y":-0.001},"handleOut":{"x":2.001,"y":1.001}},
        {"point":{"x":-0.001,"y":0.001},"handleIn":{"x":0,"y":123456.79},"handleOut":{"x":-2.5,"y":100}}])
    );
    let dashed = record("dashed-with-offset");
    assert_eq!(dashed["vector"]["dashArray"], json!([4, 2]));
    assert_eq!(dashed["vector"]["dashOffset"], json!(1.5));
    assert_eq!(dashed["vector"]["miterLimit"], json!(10));
    let plain = record("paper-default-line");
    assert!(
        plain["vector"].get("dashArray").is_none(),
        "an empty dash array is absent, as in serP"
    );
    assert!(plain["vector"].get("paintOrder").is_none());
    assert_eq!(plain["vector"]["dashOffset"], json!(0));
}

#[test]
fn number_helpers_follow_json_stringify_and_the_baseline_rounding() {
    assert_eq!(r3(-3.0005), -3.0);
    assert_eq!(r3(1.0005), 1.001);
    assert_eq!(r3(0.0005), 0.001);
    assert!(
        r3(-0.0004).is_sign_positive(),
        "a negative zero persists as 0"
    );
    assert_eq!(r3(-0.0015), -0.001, "ties go toward +infinity");
    assert_eq!(
        (r3(f64::NAN), r3(f64::INFINITY), r3(f64::NEG_INFINITY)),
        (0.0, 0.0, 0.0)
    );
    let ten = js_number(10.0).unwrap();
    assert!(ten.is_u64() && !ten.is_f64());
    assert!(js_number(-0.0).unwrap().is_u64());
    assert!(js_number(-5.0).unwrap().is_i64());
    assert!(js_number(2.5).unwrap().is_f64());
    assert!(js_number(9_007_199_254_740_992.0).unwrap().is_f64());
    assert_eq!(js_number(f64::NAN), None);
    assert_eq!(js_number(f64::INFINITY), None);
}

#[test]
fn the_baseline_facts_the_design_relies_on_are_pinned_and_tied_to_the_real_providers() {
    let (meta, facts, _) = oracle();
    let fact = |name: &str| facts.iter().find(|f| f.0 == name).map(|f| f.1.as_str());
    for (name, value) in [
        ("importSVGString.pushUndo", "1"),
        ("importSVGString.ensureKeyframe", "1"),
        ("importSVGString.insertAtEnd", "1"),
        ("importSVGString.refusesLocked", "1"),
        ("ensureKeyframe.copiesHeldStrokes", "1"),
        ("ensureKeyframe.skipsInterpolated", "1"),
        ("createUserLayer.keyOnlyAtFrameZero", "1"),
        ("figma.createUserLayerPerFrame", "1"),
        ("figma.importStringNoFit", "1"),
    ] {
        assert_eq!(fact(name), Some(value), "{name}");
    }
    assert_eq!(facts.len(), 9);
    let serp = meta.iter().find(|m| m.0 == "slice-sha256-serP").unwrap();
    assert_eq!(
        serp.1,
        "c71e9d4fbae1837b3f50bec603808a701ed629412eb050c64b9a62b8710e635e"
    );
    // createUserLayer: exactly one key, at frame 0 -- the native default layer the transaction creates.
    let layer =
        serde_json::to_value(ObjectLayer::blank_drawing("x".into(), "n".into(), 0)).unwrap();
    assert_eq!(layer["timing"]["keys"], json!([{"frame":0}]));
    assert_eq!(
        layer["locked"], false,
        "a new layer is unlocked, as createUserLayer makes it"
    );
    assert_eq!(layer["parentLayerUid"], Value::Null);
}

fn anchors(n: usize) -> ExplodedPath {
    ExplodedPath {
        origin: PathOrigin::Passthrough,
        contour: Contour {
            segments: (0..n).map(|i| seg(i as f64, (i * i) as f64)).collect(),
            closed: false,
        },
        style: LeafStyle {
            fill: None,
            stroke: Some("#000000".to_owned()),
            stroke_width: 1.0,
            opacity: 1.0,
        },
    }
}

#[test]
fn the_anchor_bounds_of_the_conversion_agree_with_the_real_storage_admission() {
    assert_eq!((MIN_SEGMENTS, MAX_SEGMENTS), (2, 256));
    for n in [1, 2, 3, 255, 256, 257, 0] {
        let converted = path_record(&anchors(n), &StrokeExtras::paper_default(), target());
        let by_conversion = converted.is_ok();
        assert_eq!(by_conversion, (2..=256).contains(&n), "{n}");
        // The same anchor count through the REAL admission, bypassing the conversion's own check.
        let mut record = record("L0", 0, 1);
        let one = record.geometry.segments[0].clone();
        record.geometry.segments = vec![one; n];
        let batch = super::ImportBatch {
            records: vec![super::ImportRecord {
                source_id: "n".into(),
                record,
            }],
            ..super::ImportBatch::default()
        };
        let by_admission = history(1).prepare_import_batch(&batch);
        assert_eq!(by_admission.is_ok(), by_conversion, "{n} anchors");
        if !by_conversion {
            assert_eq!(
                by_admission.unwrap_err(),
                ImportError::Dispatch(DispatchErrorCode::InvalidRequest)
            );
        }
    }
}
