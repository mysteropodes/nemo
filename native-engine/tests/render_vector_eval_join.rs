//! The join as a consumer sees it: immutability across frames, detached output, the packet entry the scene producer calls,
//! the holder-table predicate and the typed refusals. Documents are oracle rows (real codec, real snapshot), edited as JSON.
use super::super::{
    evaluate_record, evaluate_vector, holders_fully_consumed, params, VectorEvalError,
};
use super::{bits, check, row, rows, snapshot, Row};
use crate::evaluation::object_vector_kernels::object_parametric_geometry::ParamGeomError;
use crate::evaluation::object_vector_kernels::object_path_effects_kernel::PathFxError;
use crate::object_codec::{decode_project, encode_project};
use crate::object_document::{FrameScopeKind, Presence};
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn value_of(row: &Row) -> Value {
    serde_json::from_str(&row.doc).expect("oracle document is json")
}

fn snap(value: &Value) -> ObjectSnapshot {
    snapshot(&value.to_string())
}

fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}

fn consumed(value: &Value) -> bool {
    let document = decode_project(value.to_string().as_bytes()).expect("variant is admitted");
    holders_fully_consumed(&document)
}

#[test]
fn repeated_frames_leave_the_snapshot_and_the_first_result_untouched() {
    let (_, all) = rows();
    let source = row(&all, "rect-roughen-stack-then-rebuild@12");
    let snapshot = snap(&value_of(source));
    let document_before = snapshot.document().clone();
    let bytes_before = encode_project(snapshot.document()).expect("encodes");
    let first = evaluate_vector(&snapshot, 0, 0).expect("frame 0");
    let middle = evaluate_vector(&snapshot, 0, 10).expect("frame 10");
    let again = evaluate_vector(&snapshot, 0, 0).expect("frame 0 again");
    assert_eq!(
        bits(&first),
        bits(&again),
        "frame 0 is byte-identical after frame 10"
    );
    assert_ne!(
        bits(&first),
        bits(&middle),
        "roughen evolves, the animated corner moves"
    );
    assert_eq!(snapshot.document(), &document_before);
    assert_eq!(
        encode_project(snapshot.document()).expect("encodes"),
        bytes_before
    );
    // the stored rest geometry is still the authored rounded rectangle (anchor 0 at x 10)
    let rest = &snapshot.document().objects()[0].geometry.segments[0];
    assert_eq!(rest.point.x.as_f64(), Some(10.0));
    assert_eq!(rest.point.y.as_f64(), Some(0.0));
}

#[test]
fn detached_output_cannot_mutate_rest_geometry() {
    let (_, all) = rows();
    let source = row(&all, "rect-zigzag-then-rebuild-uses-post-effect-bounds@5");
    let snapshot = snap(&value_of(source));
    let mut taken = evaluate_vector(&snapshot, 0, 5).expect("evaluates");
    check(&taken, source).expect("matches the oracle");
    taken.segments.clear();
    taken.closed = false;
    let fresh = evaluate_vector(&snapshot, 0, 5).expect("evaluates again");
    check(&fresh, source).expect("a second evaluation is unaffected by the caller's edits");
    assert_eq!(fresh.segments.len(), 8);
    assert_eq!(
        snapshot.document().objects()[0].geometry.segments.len(),
        8,
        "rest geometry keeps the authored anchors"
    );
}

#[test]
fn the_packet_entry_gives_every_consumer_the_snapshot_entry_value() {
    let (_, all) = rows();
    let mut checked = 0;
    for source in all.iter().filter(|r| !r.doc.contains("propertyHolders")) {
        // v1 selects the records authored AT the selector frame, so the record is authored at the evaluated frame
        let mut value = value_of(source);
        value["objects"][0]["target"]["frameScope"]["frame"] = json!(source.frame);
        let snapshot = snap(&value);
        let packet = prepare_object_frame(&snapshot, &selector(&snapshot, source.frame))
            .unwrap_or_else(|e| panic!("{}: {e:?}", source.id));
        let record = packet.records().next().expect("one record");
        let direct = evaluate_vector(&snapshot, 0, source.frame).expect("snapshot entry");
        let via_packet = evaluate_record(&packet, record).expect("packet entry");
        assert_eq!(via_packet, direct, "{}", source.id);
        // scene, preview and export all retain a clone of this packet
        let retained = packet.clone();
        let again = evaluate_record(&retained, retained.records().next().expect("record"));
        assert_eq!(again.as_ref(), Ok(&direct), "{}", source.id);
        check(&via_packet, source).expect("matches the oracle");
        checked += 1;
    }
    assert_eq!(checked, 17);
}

#[test]
fn the_holder_predicate_admits_only_what_the_join_consumes() {
    let (_, all) = rows();
    let rect = value_of(row(&all, "rect-corner-tl-animated@5"));
    assert!(consumed(&rect));
    assert!(consumed(&value_of(row(&all, "star-corner-animated@5"))));
    assert!(
        consumed(&value_of(row(&all, "plain-closed-zigzag@0"))),
        "no table"
    );
    let mut empty = rect.clone();
    empty["propertyHolders"] = json!([]);
    assert!(consumed(&empty), "an empty table");

    let mut other = rect.clone();
    other["propertyHolders"][0]["properties"]
        .as_array_mut()
        .expect("properties")
        .push(json!({"property": "position", "static": [1, 2]}));
    assert!(
        !consumed(&other),
        "a Motion transform property is not consumed here"
    );

    let mut foreign = rect.clone();
    foreign["propertyHolders"][0]["properties"][0]["property"] = json!("arcSweep");
    assert!(!consumed(&foreign), "a key of another kind is not consumed");

    let mut live = rect.clone();
    live["propertyHolders"][0]["properties"][0]["expression"] =
        json!({"enabled": true, "code": "1"});
    assert!(
        !consumed(&live),
        "an enabled expression needs the evaluator"
    );
    live["propertyHolders"][0]["properties"][0]["expression"]["enabled"] = json!(false);
    assert!(consumed(&live), "a disabled expression is stored data only");

    let mut layer = rect.clone();
    layer["propertyHolders"]
        .as_array_mut()
        .expect("holders")
        .push(
            json!({"target": {"contextId": "scene-root", "layerUid": "L1"},
               "properties": [{"property": "rotation", "static": [50]}]}),
        );
    assert!(
        !consumed(&layer),
        "unsupported layer rotation is not consumed"
    );

    let mut controls = rect.clone();
    controls["propertyHolders"][0]["controls"] =
        json!([{"key": "xc_point", "name": "Point", "type": "point", "default": [12, 34]}]);
    assert!(!consumed(&controls), "expression controls are not consumed");

    let mut no_shape = rect.clone();
    no_shape["objects"][0]
        .as_object_mut()
        .expect("record")
        .remove("vectorExt");
    assert!(
        !consumed(&no_shape),
        "a vector without paramShape has no shape to rebuild"
    );

    let mut legacy = rect.clone();
    let record = legacy["objects"][0].as_object_mut().expect("record");
    record.remove("vectorExt");
    record.remove("vector");
    record.insert("family".into(), json!("closed-solid-fill-cubic-path"));
    assert!(
        !consumed(&legacy),
        "the legacy family has no parametric rebuild"
    );
}

#[test]
fn refusals_are_typed_and_publish_nothing() {
    let (_, all) = rows();
    let rect = value_of(row(&all, "rect-corner-tl-animated@5"));
    let base = snap(&rect);
    let baseline = evaluate_vector(&base, 0, 5).expect("base evaluates");

    assert_eq!(
        evaluate_vector(&base, 7, 5),
        Err(VectorEvalError::MissingRecord)
    );

    let mut legacy = value_of(row(&all, "plain-closed-zigzag@0"));
    let record = legacy["objects"][0].as_object_mut().expect("record");
    record.remove("vector");
    record.insert("family".into(), json!("closed-solid-fill-cubic-path"));
    assert_eq!(
        evaluate_vector(&snap(&legacy), 0, 0),
        Err(VectorEvalError::NotVectorFamily)
    );

    // an enabled expression on a KEYED parameter needs the expression evaluator; the same expression disabled, or on an
    // unkeyed parameter (the baseline never calls valueAtFrame for it), changes nothing
    let mut live = rect.clone();
    live["propertyHolders"][0]["properties"][0]["expression"] =
        json!({"enabled": true, "code": "1"});
    assert_eq!(
        evaluate_vector(&snap(&live), 0, 5),
        Err(VectorEvalError::ExpressionUnavailable {
            property: "cornerTL"
        })
    );
    live["propertyHolders"][0]["properties"][0]["expression"]["enabled"] = json!(false);
    assert_eq!(evaluate_vector(&snap(&live), 0, 5).as_ref(), Ok(&baseline));
    let mut unkeyed = rect.clone();
    unkeyed["propertyHolders"][0]["properties"]
        .as_array_mut()
        .expect("properties")
        .push(json!({"property": "cornerBL", "expression": {"enabled": true, "code": "1"}}));
    assert_eq!(
        evaluate_vector(&snap(&unkeyed), 0, 5).as_ref(),
        Ok(&baseline)
    );

    // an unknown kind cannot be admitted, so it is tried on a detached copy of the stored record
    let mut stored = base.document().objects()[0].clone();
    if let Presence::Present(ext) = &mut stored.vector_ext {
        if let Presence::Present(shape) = &mut ext.param_shape {
            shape.kind = "hexagon".to_string();
        }
    }
    assert!(matches!(
        params::shape_of(&stored),
        Err(VectorEvalError::UnknownShapeKind)
    ));

    // an unbounded sample count is refused by the kernel and surfaces as the join's error
    let mut budget = value_of(row(&all, "plain-closed-zigzag@0"));
    budget["layers"][0]["pathFx"][0]["p2"] = json!(1.0e9);
    assert_eq!(
        evaluate_vector(&snap(&budget), 0, 0),
        Err(VectorEvalError::PathFx(PathFxError::SampleBudget))
    );
    let mut points = value_of(row(&all, "star-inner-animated@10"));
    points["objects"][0]["vectorExt"]["paramShape"]["pointCount"] = json!(1.0e9);
    assert_eq!(
        evaluate_vector(&snap(&points), 0, 5),
        Err(VectorEvalError::Rebuild(ParamGeomError::StarPointBudget))
    );
}
