//! L01-G: every way a parameter edit must refuse. A refusal publishes nothing: the revision, the bytes, both
//! history stacks and the retained replay are exactly what they were, and the typed code is the pinned one.
use super::support::*;
#[cfg_attr(not(feature = "evaluation"), allow(unused_imports))]
use super::vsupport::{bytes, dispatch, document, objects, owner_of, state, target};
use super::{rect_ext, rect_owner, shape_doc, t1};
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

/// Dispatch `payload`, expect `code`, and prove nothing moved (including an exact replay of the refusal).
fn assert_refused(owner: &mut NativeObjectHistory, id: &str, payload: Value, code: &str) {
    let revision = owner.content_revision();
    let before = bytes(owner);
    let depths = owner.history_depths();
    let envelope = request(owner, id, OP_PARAM, payload);
    let response = dispatch(owner, &envelope);
    assert_eq!(refusal(&response), code, "{id}: {response}");
    assert_eq!(response["contentRevision"], revision, "{id}");
    assert_eq!(dispatch(owner, &envelope), response, "{id}: exact replay");
    assert_eq!(owner.content_revision(), revision, "{id}");
    assert_eq!(owner.history_depths(), depths, "{id}");
    assert_eq!(bytes(owner), before, "{id}");
}

fn all_edits() -> Vec<(&'static str, Value)> {
    vec![
        ("set", set(t1(), json!({"kind": "rect", "tl": 8}))),
        ("translate", translate(t1(), 1.0, 2.0)),
        ("scale", scale(t1(), [2.0, 2.0], [0.0, 0.0])),
    ]
}

#[test]
fn unknown_and_reference_targets_refuse_with_their_typed_codes() {
    let mut owner = rect_owner();
    for (id, payload) in all_edits() {
        let mut missing = payload.clone();
        missing["stableTarget"] = target("L1", 0, "nope");
        assert_refused(&mut owner, &format!("{id}-stroke"), missing, "not_found");
        let mut layer = payload.clone();
        layer["stableTarget"] = target("L9", 0, "s-1");
        assert_refused(&mut owner, &format!("{id}-layer"), layer, "not_found");
        let mut late = payload.clone();
        late["stableTarget"] = target("L1", 99, "s-1");
        assert_refused(&mut owner, &format!("{id}-frame"), late, "invalid_request");
        let mut reference = payload;
        reference["stableTarget"]["frameScope"]["kind"] = json!("reference");
        assert_refused(
            &mut owner,
            &format!("{id}-reference"),
            reference,
            "unavailable",
        );
    }
}

#[test]
fn only_a_vector_record_with_a_parametric_shape_and_no_pressure_ribbon_is_editable() {
    let stroke = straight_rect();
    let mut legacy = record("s-1", "L1", &stroke, true, None);
    legacy["family"] = json!("closed-solid-fill-cubic-path");
    legacy
        .as_object_mut()
        .expect("record object")
        .remove("vector");
    let plain_shape = ext_json("R:!:!:!:!", None, 0.0);
    let pressure = json!({
        "paramShape": {"kind": "rect", "box": {"x1": 0, "y1": 0, "x2": 100, "y2": 60}},
        "pressure": {"strokeProfile": "even"}
    });
    let cases = [
        ("legacy", legacy),
        ("plain-vector", record("s-1", "L1", &stroke, true, None)),
        (
            "pressure",
            record("s-1", "L1", &stroke, true, Some(pressure)),
        ),
        (
            "box-angle-only",
            record("s-1", "L1", &stroke, true, Some(json!({"boxAngle": 10}))),
        ),
    ];
    for (name, shape) in cases {
        let other = record("s-2", "L2", &stroke, true, None);
        let mut owner = owner_of(&document(1, vec![shape, other], None));
        for (id, payload) in all_edits() {
            assert_refused(&mut owner, &format!("{name}-{id}"), payload, "unavailable");
        }
    }
    // Control: the same record with a parametric shape and no ribbon is admitted as a document.
    let editable = record("s-1", "L1", &stroke, true, Some(plain_shape));
    assert_eq!(
        owner_of(&document(1, vec![editable], None)).content_revision(),
        0
    );
}

#[test]
fn locked_layers_and_layers_with_active_path_effects_refuse_drawing_edits() {
    let active = [
        json!([{"type": "zigzag"}]),
        json!([{"type": "roughen", "enabled": true}]),
        json!([{"type": "zigzag", "enabled": false}, {"type": "roughen"}]),
    ];
    for (index, stack) in active.into_iter().enumerate() {
        let mut doc = shape_doc(1, Some(rect_ext()), &straight_rect());
        doc["layers"][0]["pathFx"] = stack;
        let mut owner = owner_of(&doc);
        for (id, payload) in all_edits() {
            assert_refused(
                &mut owner,
                &format!("fx{index}-{id}"),
                payload,
                "unavailable",
            );
        }
    }
    let mut doc = shape_doc(1, Some(rect_ext()), &straight_rect());
    doc["layers"][0]["locked"] = json!(true);
    let mut owner = owner_of(&doc);
    for (id, payload) in all_edits() {
        assert_refused(&mut owner, &format!("locked-{id}"), payload, "unavailable");
    }
}

#[cfg(feature = "evaluation")]
#[test]
fn disabled_or_empty_path_effect_stacks_do_not_block_a_parameter_edit() {
    let quiet = [
        json!([]),
        json!([{"type": "zigzag", "enabled": false}]),
        json!([{"type": "zigzag", "enabled": false}, {"type": "roughen", "enabled": false}]),
    ];
    for (index, stack) in quiet.into_iter().enumerate() {
        let mut doc = shape_doc(1, Some(rect_ext()), &straight_rect());
        doc["layers"][0]["pathFx"] = stack.clone();
        let mut owner = owner_of(&doc);
        let response = send(
            &mut owner,
            &format!("q{index}"),
            OP_PARAM,
            translate(t1(), 1.0, 1.0),
        );
        assert_eq!(response["ok"], true, "{stack}: {response}");
        assert_eq!(
            state(&owner)["layers"][0]["pathFx"],
            stack,
            "the stack is untouched"
        );
    }
}

#[test]
fn malformed_payloads_are_invalid_requests_before_any_kernel_runs() {
    let mut owner = rect_owner();
    let t = t1();
    let box_json = json!({"x1": 0, "y1": 0, "x2": 10, "y2": 10});
    let cases = vec![
        ("kind-change", set(t.clone(), json!({"kind": "star"}))),
        (
            "box-in-set",
            set(t.clone(), json!({"kind": "rect", "box": box_json})),
        ),
        ("unknown-kind", set(t.clone(), json!({"kind": "hexagon"}))),
        (
            "set-with-delta",
            json!({"command": "object.parametric.set", "stableTarget": t, "paramShape": {"kind": "rect"}, "delta": [1, 1]}),
        ),
        (
            "set-without-shape",
            json!({"command": "object.parametric.set", "stableTarget": t}),
        ),
        (
            "translate-without-delta",
            json!({"command": "object.parametric.translate", "stableTarget": t}),
        ),
        (
            "translate-with-scale",
            json!({"command": "object.parametric.translate", "stableTarget": t, "delta": [1, 1], "scale": [2, 2]}),
        ),
        (
            "scale-without-anchor",
            json!({"command": "object.parametric.scale", "stableTarget": t, "scale": [2, 2]}),
        ),
        (
            "scale-without-factors",
            json!({"command": "object.parametric.scale", "stableTarget": t, "anchor": [0, 0]}),
        ),
        (
            "short-delta",
            json!({"command": "object.parametric.translate", "stableTarget": t, "delta": [1]}),
        ),
        (
            "long-delta",
            json!({"command": "object.parametric.translate", "stableTarget": t, "delta": [1, 2, 3]}),
        ),
        (
            "string-delta",
            json!({"command": "object.parametric.translate", "stableTarget": t, "delta": ["1", "2"]}),
        ),
        (
            "unknown-command",
            json!({"command": "object.parametric.rotate", "stableTarget": t, "delta": [1, 1]}),
        ),
        (
            "unknown-member",
            json!({"command": "object.parametric.translate", "stableTarget": t, "delta": [1, 1], "extra": 1}),
        ),
        (
            "missing-target",
            json!({"command": "object.parametric.translate", "delta": [1, 1]}),
        ),
        ("empty-object", json!({})),
    ];
    for (id, payload) in cases {
        assert_refused(&mut owner, id, payload, "invalid_request");
    }
}

#[test]
fn duplicate_members_and_oversized_bodies_are_rejected_at_preparation() {
    let owner = rect_owner();
    let target_json = t1().to_string();
    let duplicate = format!(
        r#"{{"command":"object.parametric.translate","command":"object.parametric.translate","stableTarget":{target_json},"delta":[1,1]}}"#
    );
    let positional = br#"["object.parametric.translate"]"#.to_vec();
    let padding = vec![b' '; 5000];
    for (name, body) in [
        ("duplicate", duplicate.into_bytes()),
        ("positional", positional),
        ("oversized", padding),
    ] {
        let prepared = owner.owner.prepare_parametric_edit_json(&body);
        assert_eq!(
            prepared.err(),
            Some(crate::request_receipts::DispatchErrorCode::InvalidRequest),
            "{name}"
        );
    }
    assert_eq!(owner.content_revision(), 0);
}

#[cfg(feature = "evaluation")]
#[test]
fn overflow_anchor_budget_and_foreign_members_refuse_the_whole_request() {
    let mut owner = rect_owner();
    // 100 * 1e308 overflows to infinity: a typed non-finite refusal, nothing published.
    let overflow = scale(t1(), [1e308, 1e308], [0.0, 0.0]);
    assert_refused(&mut owner, "overflow", overflow, "invalid_request");
    // `sweep` belongs to an ellipse: storage admission refuses the whole edit, the intent is not half written.
    let foreign = set(t1(), json!({"kind": "rect", "sweep": 10}));
    assert_refused(&mut owner, "foreign-member", foreign, "invalid_request");
    let ext =
        json!({"paramShape": {"kind": "star", "box": {"x1": 0, "y1": 0, "x2": 100, "y2": 100}}});
    let triangle: Segs = vec![
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [10.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [10.0, 10.0, 0.0, 0.0, 0.0, 0.0],
    ];
    let mut stars = owner_of(&shape_doc(1, Some(ext), &triangle));
    // 200 points with rounded corners are 800 anchors: over the 256-anchor storage admission.
    let rounded = json!({"kind": "star", "pointCount": 200, "innerRatio": 0.5, "cornerRadius": 2});
    assert_refused(
        &mut stars,
        "anchor-budget",
        set(t1(), rounded),
        "invalid_request",
    );
    // More points than the native star budget (65536).
    let beyond = json!({"kind": "star", "pointCount": 70000});
    assert_refused(
        &mut stars,
        "point-budget",
        set(t1(), beyond),
        "invalid_request",
    );
    assert_eq!(objects(&state(&stars)).len(), 2);
}

#[test]
fn envelope_guards_apply_to_parameter_edits_like_to_every_object_edit() {
    let mut owner = rect_owner();
    let before = bytes(&owner);
    let good = request(&owner, "env", OP_PARAM, translate(t1(), 1.0, 1.0));
    let mut stale = good.clone();
    stale["expectedRevision"] = json!(7);
    assert_eq!(refusal(&dispatch(&mut owner, &stale)), "stale_revision");
    let mut instance = good.clone();
    instance["instanceId"] = json!("another-instance");
    assert_eq!(refusal(&dispatch(&mut owner, &instance)), "wrong_instance");
    let mut document_pin = good.clone();
    document_pin["documentId"] = json!("another-document");
    assert_eq!(
        refusal(&dispatch(&mut owner, &document_pin)),
        "wrong_document"
    );
    let mut cancelled = good.clone();
    cancelled["requestId"] = json!("env-cancelled");
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        refusal(&dispatch(&mut owner, &cancelled)),
        "cancelled_before_dispatch"
    );
    let mut missing = good;
    missing["requestId"] = json!("env-missing");
    missing
        .as_object_mut()
        .expect("envelope")
        .remove("expectedRevision");
    assert_eq!(refusal(&dispatch(&mut owner, &missing)), "invalid_request");
    assert_eq!(bytes(&owner), before);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
}
