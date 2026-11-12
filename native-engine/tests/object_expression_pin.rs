//! nemo-test-harness: test-object_expression_pin => object_expression_pin_tests
//! Frozen request literals through the real, non-Clone native history owner.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_expression_pin::{prepare_expression_request, ExpressionSelector};
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameError, ObjectFrameSelector};
use serde_json::{json, Value};

fn source() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":21,
        "layers":[{"layerUid":"L0"}],
        "objects":[{"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
            "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},"layerUid":"L0","strokeId":"s0"},
            "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
                "segments":[{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":1,"y":0}},
                {"point":{"x":10,"y":10},"handleIn":{"x":0,"y":-1},"handleOut":{"x":0,"y":0}}]},
            "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":0.75}}],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"L0","naturalPosition":0,
                    "order":{"static":[3.25],"expression":{"enabled":true,"code":"layer(value)"}}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":7},"elements":[
                    {"layerUid":"L0","strokeId":"s0","naturalPosition":0,
                        "order":{"track":{"keys":[{"frame":0,"v":[2]},
                            {"frame":10,"v":[10]}]},"expression":{"enabled":true,"code":"stroke(value)"}}}]}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}})
}

fn make_owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "expression-owner",
        decode_storage_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap()
}

fn frame<'a>(
    snapshot: &'a crate::revision::ObjectSnapshot,
    evaluated: u32,
) -> ObjectFrameSelector<'a> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: evaluated,
    }
}

fn select<'a>(
    snapshot: &'a crate::revision::ObjectSnapshot,
    evaluated: u32,
) -> ExpressionSelector<'a> {
    ExpressionSelector {
        frame: frame(snapshot, evaluated),
        source_frame: 7,
        layer_uid: "L0",
        stroke_id: Some("s0"),
    }
}

fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(
        owner
            .acquire_snapshot(owner.content_revision())
            .unwrap()
            .document(),
    )
    .unwrap()
}

#[test]
fn pins_real_owner_source_scope_evaluated_frame_code_and_frozen_numeric_without_c1_activation() {
    let owner = make_owner(&source());
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let before = (
        bytes(&owner),
        owner.content_revision(),
        owner.history_depths(),
    );
    let request = prepare_expression_request(&owner, &snapshot, &select(&snapshot, 5)).unwrap();
    assert_eq!(request.snapshot().instance_id(), owner.instance_id());
    assert_eq!(request.snapshot().document_id(), owner.document_id());
    assert_eq!(request.snapshot().content_revision(), 0);
    assert_eq!(request.snapshot().snapshot_id(), snapshot.snapshot_id());
    assert_eq!(request.context_id(), "scene-root");
    assert_eq!(request.scope_kind(), &FrameScopeKind::Authored);
    assert_eq!((request.source_frame(), request.evaluated_frame()), (7, 5));
    assert_eq!(
        (request.layer_uid(), request.stroke_id()),
        ("L0", Some("s0"))
    );
    assert_eq!(request.code(), "stroke(value)");
    assert_eq!(request.raw_numeric(), 6.0); // frozen 2 -> 10 midpoint
    assert_eq!(request.revalidate(&owner), Ok(()));
    assert_eq!(
        prepare_object_frame(&snapshot, &frame(&snapshot, 7)).unwrap_err(),
        ObjectFrameError::Unavailable
    );
    let layer = ExpressionSelector {
        stroke_id: None,
        ..select(&snapshot, 5)
    };
    let layer_request = prepare_expression_request(&owner, &snapshot, &layer).unwrap();
    assert_eq!(
        (layer_request.code(), layer_request.raw_numeric()),
        ("layer(value)", 3.25)
    );
    assert_eq!(
        (
            bytes(&owner),
            owner.content_revision(),
            owner.history_depths()
        ),
        before
    );
}

#[test]
fn raw_numeric_uses_c1_held_default_empty_curve_and_authored_nonlinear_paths() {
    let cases = [
        (
            json!({"track":{"keys":[{"frame":0,"v":[2],"hold":true},{"frame":10,"v":[10]}]}}),
            2.0,
        ),
        (
            json!({"track":{"keys":[{"frame":0,"v":[2]},{"frame":10,"v":[10]}]}}),
            6.0,
        ),
        (
            json!({"track":{"keys":[{"frame":0,"v":[2],"curvePoints":[]},{"frame":10,"v":[10]}]}}),
            6.0,
        ),
        (
            json!({"track":{"keys":[{"frame":0,"v":[2],"curvePoints":[{"x":0,"y":0,"tx":1,"ty":0},{"x":1,"y":1,"tx":1,"ty":2}]},{"frame":10,"v":[10]}]}}),
            4.0, // frozen cubic midpoint y=.25, then 2 + (10-2)*.25
        ),
    ];
    for (mut order, expected) in cases {
        order["expression"] = json!({"enabled":true,"code":"value + 1"});
        let mut value = source();
        value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"] = order;
        let owner = make_owner(&value);
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let request = prepare_expression_request(&owner, &snapshot, &select(&snapshot, 5)).unwrap();
        assert!(
            (request.raw_numeric() - expected).abs() < 1e-12,
            "expected {expected}"
        );
    }
}

#[test]
fn wrong_pins_inactive_code_reference_v1_and_invalid_targets_refuse_without_writing() {
    let owner = make_owner(&source());
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let other = make_owner(&source()).acquire_snapshot(0).unwrap();
    let before = (
        bytes(&owner),
        owner.content_revision(),
        owner.history_depths(),
    );
    assert!(matches!(
        prepare_expression_request(&owner, &other, &select(&other, 5)),
        Err(ObjectFrameError::WrongDocument)
    ));
    let mut selector = select(&snapshot, 5);
    selector.frame.instance_id = "wrong";
    assert!(matches!(
        prepare_expression_request(&owner, &snapshot, &selector),
        Err(ObjectFrameError::WrongInstance)
    ));
    selector = select(&snapshot, 5);
    selector.frame.document_id = "wrong";
    assert!(matches!(
        prepare_expression_request(&owner, &snapshot, &selector),
        Err(ObjectFrameError::WrongDocument)
    ));
    selector = select(&snapshot, 5);
    selector.frame.content_revision = 1;
    assert!(matches!(
        prepare_expression_request(&owner, &snapshot, &selector),
        Err(ObjectFrameError::WrongRevision)
    ));
    selector = select(&snapshot, 5);
    selector.frame.document_snapshot_id = "wrong";
    assert!(matches!(
        prepare_expression_request(&owner, &snapshot, &selector),
        Err(ObjectFrameError::WrongSnapshot)
    ));
    selector = select(&snapshot, 5);
    selector.frame.context_id = "other";
    assert!(matches!(
        prepare_expression_request(&owner, &snapshot, &selector),
        Err(ObjectFrameError::InvalidContext)
    ));
    selector = select(&snapshot, 5);
    selector.frame.scope_kind = FrameScopeKind::Reference;
    assert!(matches!(
        prepare_expression_request(&owner, &snapshot, &selector),
        Err(ObjectFrameError::Unavailable)
    ));
    selector = select(&snapshot, 21);
    assert!(matches!(
        prepare_expression_request(&owner, &snapshot, &selector),
        Err(ObjectFrameError::FrameOutOfRange)
    ));
    selector = select(&snapshot, 5);
    selector.source_frame = 21;
    assert!(matches!(
        prepare_expression_request(&owner, &snapshot, &selector),
        Err(ObjectFrameError::FrameOutOfRange)
    ));
    selector = select(&snapshot, 5);
    selector.stroke_id = Some("absent");
    assert!(matches!(
        prepare_expression_request(&owner, &snapshot, &selector),
        Err(ObjectFrameError::InvalidSource)
    ));
    selector = select(&snapshot, 5);
    selector.layer_uid = "absent";
    assert!(matches!(
        prepare_expression_request(&owner, &snapshot, &selector),
        Err(ObjectFrameError::InvalidSource)
    ));
    for expression in [
        json!({"enabled":false,"code":"x"}),
        json!({"enabled":true,"code":""}),
    ] {
        let mut value = source();
        value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"]
            ["expression"] = expression;
        let owner = make_owner(&value);
        let snapshot = owner.acquire_snapshot(0).unwrap();
        assert!(matches!(
            prepare_expression_request(&owner, &snapshot, &select(&snapshot, 5)),
            Err(ObjectFrameError::Unavailable)
        ));
    }
    let mut value = source();
    value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"]
        .as_object_mut()
        .unwrap()
        .remove("expression");
    let owner_without_code = make_owner(&value);
    let snapshot_without_code = owner_without_code.acquire_snapshot(0).unwrap();
    assert!(matches!(
        prepare_expression_request(
            &owner_without_code,
            &snapshot_without_code,
            &select(&snapshot_without_code, 5)
        ),
        Err(ObjectFrameError::Unavailable)
    ));
    let v1 = json!({"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":21,"layers":[{"layerUid":"L0"}],"objects":source()["objects"]});
    let v1_owner = make_owner(&v1);
    let v1_snapshot = v1_owner.acquire_snapshot(0).unwrap();
    assert!(matches!(
        prepare_expression_request(&v1_owner, &v1_snapshot, &select(&v1_snapshot, 5)),
        Err(ObjectFrameError::Unavailable)
    ));
    assert_eq!(
        (
            bytes(&owner),
            owner.content_revision(),
            owner.history_depths()
        ),
        before
    );
}

#[test]
fn real_fill_and_undo_stale_old_request_without_touching_intervening_owner() {
    let initial = source();
    let mut owner = make_owner(&initial);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let request = prepare_expression_request(&owner, &snapshot, &select(&snapshot, 5)).unwrap();
    let payload = json!({"command":"object.fill.set","stableTarget":initial["objects"][0]["target"],
        "fill":{"kind":"solid","r":0.5,"g":0.1,"b":0.3,"a":0.5}});
    let candidate = owner
        .prepare_fill_json(&serde_json::to_vec(&payload).unwrap())
        .unwrap();
    assert_eq!(owner.commit_fill(candidate), Ok(1));
    let after_fill = (
        bytes(&owner),
        owner.content_revision(),
        owner.history_depths(),
    );
    assert_eq!(
        request.revalidate(&owner),
        Err(ObjectFrameError::WrongRevision)
    );
    assert_eq!(
        (
            bytes(&owner),
            owner.content_revision(),
            owner.history_depths()
        ),
        after_fill
    );
    let one = owner.acquire_snapshot(1).unwrap();
    let newer = prepare_expression_request(&owner, &one, &select(&one, 5)).unwrap();
    assert_eq!(owner.undo(1), Ok(2));
    let after_undo = (
        bytes(&owner),
        owner.content_revision(),
        owner.history_depths(),
    );
    assert_eq!(
        newer.revalidate(&owner),
        Err(ObjectFrameError::WrongRevision)
    );
    assert_eq!(
        (
            bytes(&owner),
            owner.content_revision(),
            owner.history_depths()
        ),
        after_undo
    );
}

#[cfg(feature = "object_expression_runtime")]
fn bounded_runtime() -> crate::object_expression_runtime::ExpressionLimits {
    // Exact accepted scratch-control envelope, supplied explicitly by this
    // test caller. No production workload default is inferred from it.
    crate::object_expression_runtime::ExpressionLimits::new(4096, 8_388_608, 262_144, 32).unwrap()
}

#[cfg(feature = "object_expression_runtime")]
#[test]
fn real_owner_to_retired_runtime_to_final_pin_yields_only_detached_scalar() {
    use crate::object_expression_runtime::evaluate_for_owner;
    let mut value = source();
    value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"]["expression"]
        ["code"] = json!("value+frame/5");
    let owner = make_owner(&value);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let before = (
        bytes(&owner),
        owner.content_revision(),
        owner.history_depths(),
    );
    assert_eq!(
        evaluate_for_owner(&owner, &snapshot, &select(&snapshot, 5), bounded_runtime()),
        Ok(7.0) // C1 raw 6 at evaluated frame 5, plus frame/5
    );
    assert_eq!(
        (
            bytes(&owner),
            owner.content_revision(),
            owner.history_depths()
        ),
        before
    );
}

#[cfg(feature = "object_expression_runtime")]
#[test]
fn real_owner_early_input_refusal_preserves_history_and_recovers() {
    use crate::object_expression_runtime::{
        evaluate_for_owner, ExpressionLimits, ExpressionRefusal,
    };
    let mut value = source();
    value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"]["expression"]
        ["code"] = json!("value+frame/5");
    let owner = make_owner(&value);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let before = (
        bytes(&owner),
        owner.content_revision(),
        owner.history_depths(),
    );
    let tiny_input = ExpressionLimits::new(1, 8_388_608, 262_144, 32).unwrap();
    assert_eq!(
        evaluate_for_owner(&owner, &snapshot, &select(&snapshot, 5), tiny_input),
        Err(ExpressionRefusal::InputLimit)
    );
    assert_eq!(
        (
            bytes(&owner),
            owner.content_revision(),
            owner.history_depths()
        ),
        before
    );
    assert_eq!(
        evaluate_for_owner(&owner, &snapshot, &select(&snapshot, 5), bounded_runtime()),
        Ok(7.0)
    );
}

#[cfg(feature = "object_expression_runtime")]
#[test]
fn real_fill_and_undo_between_detached_evaluation_and_finalization_refuse_stale() {
    use crate::object_expression_runtime::{evaluate_detached, ExpressionRefusal};
    let mut value = source();
    value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"]["expression"]
        ["code"] = json!("value+frame/5");
    let mut owner = make_owner(&value);
    let zero = owner.acquire_snapshot(0).unwrap();
    let pinned = prepare_expression_request(&owner, &zero, &select(&zero, 5)).unwrap();
    let candidate = evaluate_detached(&pinned, bounded_runtime()).unwrap();
    let payload = json!({"command":"object.fill.set","stableTarget":value["objects"][0]["target"],
        "fill":{"kind":"solid","r":0.5,"g":0.1,"b":0.3,"a":0.5}});
    let prepared = owner
        .prepare_fill_json(&serde_json::to_vec(&payload).unwrap())
        .unwrap();
    assert_eq!(owner.commit_fill(prepared), Ok(1));
    let after_fill = (
        bytes(&owner),
        owner.content_revision(),
        owner.history_depths(),
    );
    assert_eq!(
        pinned.finalize(&owner, candidate),
        Err(ExpressionRefusal::Pin(ObjectFrameError::WrongRevision))
    );
    assert_eq!(
        (
            bytes(&owner),
            owner.content_revision(),
            owner.history_depths()
        ),
        after_fill
    );
    let one = owner.acquire_snapshot(1).unwrap();
    let pinned = prepare_expression_request(&owner, &one, &select(&one, 5)).unwrap();
    let candidate = evaluate_detached(&pinned, bounded_runtime()).unwrap();
    assert_eq!(owner.undo(1), Ok(2));
    let after_undo = (
        bytes(&owner),
        owner.content_revision(),
        owner.history_depths(),
    );
    assert_eq!(
        pinned.finalize(&owner, candidate),
        Err(ExpressionRefusal::Pin(ObjectFrameError::WrongRevision))
    );
    assert_eq!(
        (
            bytes(&owner),
            owner.content_revision(),
            owner.history_depths()
        ),
        after_undo
    );
}
