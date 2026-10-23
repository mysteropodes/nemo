//! L10-H: the real `plan_svg_import` -> batch -> history -> reopen chain, with literal expected
//! records, ids and receipts (the donut is the L10-D keyhole merge of an exterior and its hole).
use super::support::*;
use super::svg::{svg_plan_batch, RecordError, StrokeExtras, SvgBatchError, SvgPlacement};
use super::SkipKind;
use crate::history::object_history::object_shot_kernels::native_import_svg_explode::plan::{
    plan_svg_import, ActiveLayer, FitRecord, ImportNode, ParseOutcome, SvgImportOutcome,
    SvgImportPlan, SvgImportRequest,
};
use crate::history::object_history::object_shot_kernels::native_import_svg_explode::{
    Bounds, CompoundChild, Contour, ImportLeaf, LeafStyle, Seg,
};
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

fn seg(x: f64, y: f64) -> Seg {
    Seg {
        point: [x, y],
        handle_in: [0.0, 0.0],
        handle_out: [0.0, 0.0],
    }
}
fn contour(points: &[(f64, f64)]) -> Contour {
    Contour {
        segments: points.iter().map(|p| seg(p.0, p.1)).collect(),
        closed: true,
    }
}
fn square(x: f64, y: f64, s: f64, clockwise: bool) -> CompoundChild {
    let points = if clockwise {
        [(x, y), (x + s, y), (x + s, y + s), (x, y + s)]
    } else {
        [(x, y), (x, y + s), (x + s, y + s), (x + s, y)]
    };
    CompoundChild {
        contour: contour(&points),
        clockwise,
        bounds: Bounds {
            x,
            y,
            width: s,
            height: s,
        },
    }
}
fn style(fill: Option<&str>, stroke: Option<&str>, width: f64, opacity: f64) -> LeafStyle {
    LeafStyle {
        fill: fill.map(str::to_owned),
        stroke: stroke.map(str::to_owned),
        stroke_width: width,
        opacity,
    }
}

/// Pre-order node indexes: 0 group, 1 triangle, 2 donut, 3 raster (skipped), 4 all-hole compound.
pub(super) fn svg_plan() -> SvgImportPlan {
    let tree = ImportNode::Node {
        class_name: Some("Group".into()),
        children: vec![
            ImportNode::Leaf(ImportLeaf::Path {
                contour: contour(&[(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)]),
                style: style(Some("#336699"), None, 1.0, 1.0),
            }),
            ImportNode::Leaf(ImportLeaf::Compound {
                children: vec![
                    square(0.0, 0.0, 100.0, true),
                    square(40.0, 40.0, 20.0, false),
                ],
                style: style(Some("#ff0000"), Some("#000000"), 2.0, 0.5),
            }),
            ImportNode::Node {
                class_name: Some("Raster".into()),
                children: vec![],
            },
            ImportNode::Leaf(ImportLeaf::Compound {
                children: vec![
                    square(200.0, 200.0, 10.0, false),
                    square(300.0, 200.0, 10.0, false),
                ],
                style: style(Some("#00ff00"), None, 1.0, 1.0),
            }),
        ],
    };
    let request = SvgImportRequest {
        parse: ParseOutcome::Parsed(tree),
        layer: Some(ActiveLayer {
            locked: false,
            has_symbol: false,
        }),
        fit: FitRecord {
            no_fit: true,
            scale: 1.0,
            center: [0.0, 0.0],
            translate: [0.0, 0.0],
        },
    };
    match plan_svg_import(&request) {
        SvgImportOutcome::Ready(plan) => plan,
        other => panic!("the fixture tree must plan: {other:?}"),
    }
}
pub(super) fn placement(layer_ref: &str) -> SvgPlacement {
    SvgPlacement {
        layer_ref: layer_ref.into(),
        context_id: "scene-root".into(),
        frame: 0,
        id_prefix: "svg".into(),
    }
}
fn defaults(plan: &SvgImportPlan) -> Vec<StrokeExtras> {
    plan.paths
        .iter()
        .map(|_| StrokeExtras::paper_default())
        .collect()
}

#[test]
fn a_real_svg_plan_becomes_a_batch_that_accounts_for_every_node_and_hole() {
    let plan = svg_plan();
    assert_eq!(
        (plan.paths.len(), plan.leaves.len(), plan.skipped.len()),
        (2, 3, 1)
    );
    let batch = svg_plan_batch(&plan, &placement("L0"), &defaults(&plan)).unwrap();
    let ids: Vec<&str> = batch.records.iter().map(|r| r.source_id.as_str()).collect();
    assert_eq!(ids, ["svg:1:0", "svg:2:0"]);
    let skips: Vec<(&str, &str)> = batch
        .skipped
        .iter()
        .map(|s| (s.source_id.as_str(), s.reason.as_str()))
        .collect();
    assert_eq!(
        skips,
        [
            ("svg:4:hole:0", "unclaimed-hole"),
            ("svg:4:hole:1", "unclaimed-hole"),
            ("svg:3", "unsupported-element:Raster"),
        ]
    );
    assert!(batch.skipped.iter().all(|s| s.kind == SkipKind::Element));
    // Every produced path, skipped node and unclaimed hole of the plan is in the batch.
    let holes: usize = plan.leaves.iter().map(|l| l.unclaimed_holes.len()).sum();
    assert_eq!(batch.records.len(), plan.paths.len());
    assert_eq!(batch.skipped.len(), plan.skipped.len() + holes);
}

#[test]
fn the_svg_chain_publishes_literal_records_with_ids_and_survives_reopen() {
    let plan = svg_plan();
    let batch = svg_plan_batch(&plan, &placement("L0"), &defaults(&plan)).unwrap();
    let mut h = history(1);
    let prepared = h.prepare_import_batch(&batch).unwrap();
    let (revision, receipt) = h.commit_import_batch(prepared).unwrap();
    assert_eq!(revision, 1);
    assert_eq!(h.history_depths(), (1, 0), "N paths, one undo step");
    let triangle_vector = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,
        "strokeCap":"butt","strokeJoin":"miter","miterLimit":10,"dashOffset":0,"opacity":1});
    let donut_vector = json!({"stroke":{"kind":"solid","r":0,"g":0,"b":0,"a":1},
        "hasRealStroke":true,"strokeWidth":2,"strokeCap":"butt","strokeJoin":"miter",
        "miterLimit":10,"dashOffset":0,"opacity":0.5});
    let zero = json!({"x":0,"y":0});
    let flat = |x: i64, y: i64| json!({"point":{"x":x,"y":y},"handleIn":zero,"handleOut":zero});
    let after = state(&h);
    assert_eq!(
        after["objects"][1],
        json!({
        "schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
            "layerUid":"L0","strokeId":"native-stroke:1:0"},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":[flat(0, 0), flat(10, 0), flat(0, 10)]},
        "fill":{"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":1},
        "vector":triangle_vector})
    );
    // The donut: exterior + hole keyhole-merged by the L10-D kernel into ONE closed path of 10 anchors.
    let merged = [
        (0, 0),
        (40, 40),
        (40, 60),
        (60, 60),
        (60, 40),
        (40, 40),
        (0, 0),
        (100, 0),
        (100, 100),
        (0, 100),
    ];
    let merged_segments: Vec<Value> = merged.iter().map(|p| flat(p.0, p.1)).collect();
    assert_eq!(
        after["objects"][2],
        json!({
        "schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
            "layerUid":"L0","strokeId":"native-stroke:1:1"},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":merged_segments},
        "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1},
        "vector":donut_vector})
    );
    assert_eq!(after["objects"].as_array().unwrap().len(), 3);
    assert_eq!(
        receipt.skipped.len(),
        3,
        "the raster and both unclaimed holes are reported, not dropped"
    );
    assert_eq!(
        receipt.objects,
        [
            ("svg:1:0".to_owned(), "native-stroke:1:0".to_owned()),
            ("svg:2:0".to_owned(), "native-stroke:1:1".to_owned()),
        ]
    );
    let reopened = decode_storage_project(&bytes(&h)).unwrap();
    assert_eq!(serde_json::to_value(&reopened).unwrap(), after);
}

#[test]
fn a_figma_frame_shape_new_layer_plus_nofit_plan_lands_in_the_created_layer() {
    let plan = svg_plan();
    let mut batch = svg_plan_batch(&plan, &placement("figma:1"), &defaults(&plan)).unwrap();
    batch.layers.push(layer("figma:1", "Frame 1"));
    let mut h = history(2);
    let prepared = h.prepare_import_batch(&batch).unwrap();
    assert_eq!(
        prepared.result().layers,
        [("figma:1".to_owned(), "native-layer:1:0".to_owned())]
    );
    h.commit_import_batch(prepared).unwrap();
    let after = state(&h);
    assert_eq!(
        after["objects"][1]["target"]["layerUid"],
        "native-layer:1:0"
    );
    assert_eq!(
        after["objects"][2]["target"]["layerUid"],
        "native-layer:1:0"
    );
    let scope = &after["compositionOrder"]["contexts"][0]["scopes"][0]["elements"];
    assert_eq!(
        scope.as_array().unwrap().len(),
        3,
        "the order plan covers both new elements"
    );
}

#[test]
fn unplaceable_paths_and_mismatched_inputs_are_typed_errors() {
    let plan = svg_plan();
    let good = defaults(&plan);
    assert_eq!(
        svg_plan_batch(&plan, &placement("L0"), &good[..1]).unwrap_err(),
        SvgBatchError::ExtrasMismatch {
            paths: 2,
            extras: 1
        }
    );
    let mut broken = plan.clone();
    broken.leaves[0].produced += 1;
    assert_eq!(
        svg_plan_batch(&broken, &placement("L0"), &good).unwrap_err(),
        SvgBatchError::LeafReportMismatch
    );
    let mut gradient = plan.clone();
    gradient.paths[1].style.fill = Some("gradient:linear".into());
    assert_eq!(
        svg_plan_batch(&gradient, &placement("L0"), &good).unwrap_err(),
        SvgBatchError::Record {
            index: 1,
            error: RecordError::UnsupportedPaint {
                field: "fill",
                token: "gradient:linear".into()
            }
        }
    );
    for bad in [
        "#12",
        "#12345",
        "#1234567",
        "#gg0000",
        "123456",
        "rgb(1,2,3)",
        "#+12345",
    ] {
        let mut odd = plan.clone();
        odd.paths[0].style.stroke = Some(bad.into());
        assert!(
            matches!(
                svg_plan_batch(&odd, &placement("L0"), &good),
                Err(SvgBatchError::Record {
                    index: 0,
                    error: RecordError::UnsupportedPaint { .. }
                })
            ),
            "{bad}"
        );
    }
    let mut nan = plan.clone();
    nan.paths[0].style.stroke_width = f64::NAN;
    assert_eq!(
        svg_plan_batch(&nan, &placement("L0"), &good).unwrap_err(),
        SvgBatchError::Record {
            index: 0,
            error: RecordError::NonFinite("strokeWidth")
        }
    );
}
