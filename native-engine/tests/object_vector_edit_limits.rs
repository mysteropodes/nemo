//! L01-F request-envelope bound: 64 KiB for the vector edit operation only (identifiers and metadata
//! included, counted in UTF-8 bytes), 4096 unchanged for every other object operation. A refusal is a typed
//! error with no mutation, no history entry and an intact receipt store.
use super::support::*;
use crate::history::NativeObjectHistory;
use crate::request_receipts::OpacityRequest;
use serde_json::{json, Value};

const VECTOR_LIMIT: usize = 65_536;
const OLD_LIMIT: usize = 4096;

/// Exact byte length of the request as the dispatcher serializes it (the typed struct, not the Value).
fn wire_len(request: &Value) -> usize {
    let typed: OpacityRequest = serde_json::from_value(request.clone()).unwrap();
    serde_json::to_vec(&typed).unwrap().len()
}
/// Pads the free string produced by `build` with 3-byte characters and ASCII so the request is exactly
/// `total` bytes; returns the request and the pad's character count.
fn padded(build: &dyn Fn(&str) -> Value, total: usize) -> (Value, usize) {
    let base = wire_len(&build(""));
    assert!(base < total);
    let need = total - base;
    let pad = format!("{}{}", "\u{20ac}".repeat(need / 3), "x".repeat(need % 3));
    let request = build(&pad);
    assert_eq!(wire_len(&request), total);
    (request, pad.chars().count())
}
// The over-limit probe MUST use another identity. Otherwise a widened gate
// can hit ChangedBody for the retained inclusive probe and mimic its gate error.
fn distinct_request_id(request: &mut Value) {
    let id = request["requestId"].as_str().unwrap();
    assert!(!id.is_empty() && id.is_ascii());
    let suffix = if id.ends_with('x') { 'y' } else { 'x' };
    request["requestId"] = json!(format!("{}{}", &id[..id.len() - 1], suffix));
}

fn message(response: &Value) -> String {
    response["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

#[test]
fn the_vector_bound_is_inclusive_and_counts_utf8_bytes() {
    let mut owner = owner_of(&document(1, seeds(), None));
    let before = bytes(&owner);
    let build = |pad: &str| {
        let mut record = record_at("A", "L1", 0, "");
        record["target"]["contextId"] = json!(pad);
        request(&owner, "limit", create(record))
    };
    let (inside, chars) = padded(&build, VECTOR_LIMIT);
    assert!(
        chars < VECTOR_LIMIT - 1000,
        "the pad is multibyte: characters are fewer than bytes"
    );
    let (mut over, _) = padded(&build, VECTOR_LIMIT + 1);
    distinct_request_id(&mut over);
    assert_eq!(wire_len(&over), VECTOR_LIMIT + 1);
    let accepted = dispatch(&mut owner, &inside);
    // Past the envelope gate the request fails on its own (non-existent context): the payload message.
    assert_eq!(accepted["error"]["code"], "invalid_request");
    assert_eq!(
        message(&accepted),
        "Vector edit payload or target is invalid or unavailable."
    );
    let refused = dispatch(&mut owner, &over);
    assert_eq!(refused["error"]["code"], "invalid_request");
    assert_eq!(message(&refused), "Vector edit request refused.");
    assert_eq!(
        (
            bytes(&owner),
            owner.content_revision(),
            owner.history_depths()
        ),
        (before, 0, (0, 0))
    );
}

fn gate_check(
    name: &str,
    owner: &mut NativeObjectHistory,
    build: &dyn Fn(&NativeObjectHistory, &str) -> Value,
    gate_message: &str,
) {
    let make = |pad: &str| build(owner, pad);
    let (inside, _) = padded(&make, OLD_LIMIT);
    let (mut over, _) = padded(&make, OLD_LIMIT + 1);
    distinct_request_id(&mut over);
    assert_eq!(wire_len(&over), OLD_LIMIT + 1);
    let passed = dispatch(owner, &inside);
    assert_ne!(
        message(&passed),
        gate_message,
        "{name}: 4096 bytes passes the gate"
    );
    let refused = dispatch(owner, &over);
    assert_eq!(refused["ok"], false, "{name}");
    assert_eq!(refused["error"]["code"], "invalid_request", "{name}");
    assert_eq!(
        message(&refused),
        gate_message,
        "{name}: 4097 bytes is refused by the 4096 gate"
    );
}

#[test]
fn every_existing_object_operation_keeps_the_4096_byte_bound() {
    let mut owner = owner_of(&document(1, seeds(), None));
    gate_check(
        "fill",
        &mut owner,
        &|o, pad| {
            let mut req = request(
                o,
                "fill",
                json!({"command":"object.fill.set","stableTarget":target(pad, 0, "s-a"),
                    "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}}),
            );
            req["operation"] = json!("command.document.object.fill.set");
            req
        },
        "Invalid bounded object fill request.",
    );
    let visibility = |o: &NativeObjectHistory, pad: &str| {
        let mut req = request(
            o,
            "vis",
            json!({"command":"object.layer.visibility.set","layerUid":pad,"visible":false}),
        );
        req["operation"] = json!("command.document.object.layer.visibility.set");
        req
    };
    gate_check(
        "visibility",
        &mut owner,
        &visibility,
        "Layer visibility request refused.",
    );
    let layer_create = |o: &NativeObjectHistory, pad: &str| {
        let mut req = request(o, "mk", json!({"command":"object.layer.create","name":pad}));
        req["operation"] = json!("command.document.object.layer.create");
        req
    };
    gate_check(
        "layer create",
        &mut owner,
        &layer_create,
        "Layer creation request refused.",
    );
    for (name, operation, payload, gate, semantic) in remaining_operations() {
        closed_payload_gate_check(name, operation, payload, gate, semantic);
    }
}

// Every other operation recognized by the current object_command_dispatch.rs.
// Each payload is its real closed command shape plus one deliberate free padding
// member: at 4096 it must reach that command's semantic refusal; at 4097 it must
// fail the envelope gate first. This distinguishes both sides without publishing.
fn remaining_operations() -> Vec<(
    &'static str,
    &'static str,
    Value,
    &'static str,
    &'static str,
)> {
    vec![
        (
            "policy",
            "command.document.object.layer.policy.edit",
            json!({"command":"object.layer.policy.edit","layerUid":"L1","flag":"locked","value":true}),
            "Layer policy request refused.",
            "Layer policy payload is invalid or target is absent.",
        ),
        (
            "settings",
            "command.document.object.project.settings.edit",
            json!({"command":"object.project.settings.edit","action":{"kind":"setFps","fps":30}}),
            "Project settings request refused.",
            "Project settings payload is invalid or project is absent.",
        ),
        (
            "property",
            "command.document.object.property.edit",
            json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L1"},
                "property":"opacity","action":{"kind":"setValue","frame":0,"values":[50]}}),
            "Property edit request refused.",
            "Property edit payload or target is invalid or unavailable.",
        ),
        (
            "track",
            "command.document.object.property.track.edit",
            json!({"command":"object.property.track.edit","selections":[{"target":{"contextId":"scene-root", "layerUid":"L1"},
                "property":"opacity","frame":0}],"action":{"kind":"subdivide"}}),
            "Property edit request refused.",
            "Property edit payload or target is invalid or unavailable.",
        ),
        (
            "undo",
            "command.document.object.undo",
            json!({"command":"object.undo"}),
            "Invalid bounded object history request.",
            "Object history payload is invalid.",
        ),
        (
            "redo",
            "command.document.object.redo",
            json!({"command":"object.redo"}),
            "Invalid bounded object history request.",
            "Object history payload is invalid.",
        ),
    ]
}

fn closed_payload_gate_check(
    name: &str,
    operation: &str,
    payload: Value,
    gate: &str,
    semantic: &str,
) {
    let mut owner = owner_of(&document(1, seeds(), None));
    // Both stacks are nonempty: an accidental undo, redo, or redo clearing is observable.
    assert_eq!(
        send(
            &mut owner,
            "seed-one",
            replace(record_at("D2", "L2", 0, "s-b"))
        )["ok"],
        true
    );
    assert_eq!(
        send(
            &mut owner,
            "seed-two",
            replace(record_at("D2", "L1", 0, "s-a"))
        )["ok"],
        true
    );
    assert_eq!(owner.undo(2), Ok(3));
    assert_eq!(owner.history_depths(), (1, 1));
    let before = bytes(&owner);
    let old = owner.acquire_snapshot(3).unwrap();
    let snapshot_id = old.snapshot_id().to_owned();
    let source = serde_json::to_value(old.document()).unwrap();
    let build = |pad: &str| {
        let mut padded_payload = payload.clone();
        padded_payload["padding"] = json!(pad);
        let mut req = request(&owner, name, padded_payload);
        req["operation"] = json!(operation);
        req
    };
    let (inside, chars) = padded(&build, OLD_LIMIT);
    let (mut over, _) = padded(&build, OLD_LIMIT + 1);
    distinct_request_id(&mut over);
    assert_eq!(wire_len(&over), OLD_LIMIT + 1);
    assert!(chars < OLD_LIMIT / 2, "UTF-8 bytes, not character count");
    let passed = dispatch(&mut owner, &inside);
    assert_eq!(passed["ok"], false, "{name}");
    assert_eq!(passed["error"]["code"], "invalid_request", "{name}");
    assert_eq!(
        message(&passed),
        semantic,
        "{name}: inclusive gate reached the actual closed payload validator"
    );
    let refused = dispatch(&mut owner, &over);
    assert_eq!(refused["ok"], false, "{name}");
    assert_eq!(refused["error"]["code"], "invalid_request", "{name}");
    assert_eq!(
        message(&refused),
        gate,
        "{name}: 4097 is refused before payload/receipt lookup"
    );
    assert_eq!(
        dispatch(&mut owner, &inside),
        passed,
        "{name}: semantic receipt replay"
    );
    assert_eq!(owner.content_revision(), 3, "{name}");
    assert_eq!(owner.history_depths(), (1, 1), "{name}");
    assert_eq!(bytes(&owner), before, "{name}");
    assert_eq!(
        owner.acquire_snapshot(3).unwrap().snapshot_id(),
        snapshot_id
    );
    assert_eq!(serde_json::to_value(old.document()).unwrap(), source);
}

#[test]
fn a_maximal_vector_dispatches_where_4096_bytes_could_not_and_257_anchors_cannot() {
    let mut owner = owner_of(&document(2, seeds(), None));
    let mut record = record_at("D", "L1", 0, "");
    let anchor = record["geometry"]["segments"][0].clone();
    record["geometry"]["segments"] = Value::Array(vec![anchor; 256]);
    let big = request(&owner, "big", create(record.clone()));
    let size = wire_len(&big);
    assert!(size > OLD_LIMIT && size <= VECTOR_LIMIT, "{size}");
    let made = dispatch(&mut owner, &big);
    assert_eq!(made["ok"], true, "{made}");
    assert_eq!(
        state(&owner)["objects"][3]["geometry"]["segments"]
            .as_array()
            .unwrap()
            .len(),
        256
    );
    // The bound is not a way around the anchor cap: 257 anchors fit the envelope and are refused by admission.
    let anchor = record["geometry"]["segments"][0].clone();
    record["geometry"]["segments"] = Value::Array(vec![anchor; 257]);
    let before = bytes(&owner);
    let refused = send(&mut owner, "257", create(record));
    assert_eq!(refused["error"]["code"], "invalid_request");
    assert_eq!(
        message(&refused),
        "Vector edit payload or target is invalid or unavailable."
    );
    assert_eq!((bytes(&owner), owner.history_depths()), (before, (1, 0)));
}

#[test]
fn an_oversized_refusal_leaves_the_receipt_store_usable() {
    let mut owner = owner_of(&document(1, seeds(), None));
    let build = |pad: &str| {
        let mut record = record_at("A", "L1", 0, "");
        record["target"]["contextId"] = json!(pad);
        request(&owner, "same-id", create(record))
    };
    let (over, _) = padded(&build, VECTOR_LIMIT + 1);
    let refused = dispatch(&mut owner, &over);
    assert_eq!(message(&refused), "Vector edit request refused.");
    // A well-formed request is still executed once and replays exactly.
    let ok = request(&owner, "next", create(record_at("B", "L1", 0, "")));
    let first = dispatch(&mut owner, &ok);
    assert_eq!(first["ok"], true);
    assert_eq!(dispatch(&mut owner, &ok), first);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    // The same oversized body is refused again (the gate precedes receipts), never executed.
    let again = dispatch(&mut owner, &over);
    assert_eq!(
        (message(&again), &again["error"]["code"]),
        (message(&refused), &refused["error"]["code"])
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
}
