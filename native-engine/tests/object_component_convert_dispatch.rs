//! The staged dispatcher envelope of the lane-4 setup / convert operations
//! (L04-M): compact results, exact request replay, and typed refusals that publish
//! nothing and claim no raw ingress.
use super::fixture::{decode, document, layer, s1_document, s1_with_animated_item, s2_document};
use super::*;
use crate::object_codec::encode_storage_project;
use serde_json::{json, Value};

const CONVERT: &str = "command.document.object.component.convert";

fn owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new("component-dispatch", decode(value)).unwrap()
}
fn value(owner: &NativeObjectHistory) -> Value {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    serde_json::from_slice(&encode_storage_project(snapshot.document()).unwrap()).unwrap()
}
fn request(owner: &NativeObjectHistory, id: &str, operation: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":operation,"payload":payload})
}
fn convert(layers: &[&str]) -> Value {
    json!({"command":"object.component.convert","contextId":"scene-root","layerUids":layers})
}
fn send(owner: &mut NativeObjectHistory, id: &str, operation: &str, payload: Value) -> Value {
    let envelope = request(owner, id, operation, payload);
    dispatch(owner, &envelope)
}
fn dispatch(owner: &mut NativeObjectHistory, request: &Value) -> Value {
    serde_json::to_value(
        owner
            .try_dispatch_object_command(&serde_json::from_value(request.clone()).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn compact_results_report_the_layer_only_when_one_was_created() {
    let mut single = owner(&s1_document(json!({})));
    let reply = send(&mut single, "one", CONVERT, convert(&["ly_hand"]));
    assert_eq!(reply["ok"], true);
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    let mut merged = owner(&s2_document());
    let reply = send(&mut merged, "many", CONVERT, convert(&["ly_a", "ly_b"]));
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:1:0"})
    );
    assert_eq!(
        value(&merged)["layers"][1]["layerUid"],
        reply["result"]["layerUid"]
    );
}

#[test]
fn exact_replay_does_not_execute_twice_even_after_an_undo() {
    let mut owner = owner(&s2_document());
    let req = request(&owner, "convert", CONVERT, convert(&["ly_a", "ly_b"]));
    let first = dispatch(&mut owner, &req);
    assert_eq!(first["ok"], true);
    let after = value(&owner);
    assert_eq!(dispatch(&mut owner, &req), first);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    let mut altered = req.clone();
    altered["payload"] = convert(&["ly_a", "ly_c"]);
    assert_eq!(dispatch(&mut owner, &altered)["ok"], false);
    assert_eq!(value(&owner), after);
    owner.undo(1).unwrap();
    assert_eq!(
        dispatch(&mut owner, &req),
        first,
        "replay cannot execute after undo"
    );
    assert_eq!(owner.history_depths(), (0, 1));
}

#[test]
fn typed_dispatch_refusals_publish_nothing() {
    let mut owner = owner(&s1_document(json!({})));
    let original = value(&owner);
    let cases = [
        (
            "cancel",
            json!({"cancelledBeforeDispatch":true}),
            "cancelled_before_dispatch",
        ),
        ("stale", json!({"expectedRevision":99}), "stale_revision"),
        (
            "foreign-instance",
            json!({"instanceId":"foreign"}),
            "wrong_instance",
        ),
        (
            "foreign-document",
            json!({"documentId":"foreign"}),
            "wrong_document",
        ),
        ("wrong-api", json!({"apiVersion":1}), "invalid_request"),
        (
            "chosen-symbol",
            json!({"payload":{"command":"object.component.convert","contextId":"scene-root",
                "layerUids":["ly_hand"],"symbolId":"chosen"}}),
            "invalid_request",
        ),
        (
            "nested",
            json!({"payload":{"command":"object.component.convert","contextId":"sym_open",
                "layerUids":["ly_hand"]}}),
            "unavailable",
        ),
        (
            "unknown-layer",
            json!({"payload":{"command":"object.component.convert","contextId":"scene-root",
                "layerUids":["nope"]}}),
            "not_found",
        ),
    ];
    for (id, patch, code) in cases {
        let mut req = request(&owner, id, CONVERT, convert(&["ly_hand"]));
        for (key, val) in patch.as_object().unwrap() {
            req[key] = val.clone();
        }
        let reply = dispatch(&mut owner, &req);
        assert_eq!(reply["ok"], false, "{id}");
        assert_eq!(reply["error"]["code"], code, "{id}");
        assert_eq!(value(&owner), original, "{id}");
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (0, (0, 0)),
            "{id}"
        );
    }
}

#[test]
fn a_refused_source_reports_the_baseline_reason_in_the_receipt() {
    let mut owner = owner(&s1_document(json!({})));
    assert_eq!(
        send(&mut owner, "first", CONVERT, convert(&["ly_hand"]))["ok"],
        true
    );
    let reply = send(&mut owner, "again", CONVERT, convert(&["ly_hand"]));
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["error"]["code"], "invalid_request");
    assert_eq!(
        reply["error"]["message"],
        "Impossible de convertir : un composant"
    );
}

#[test]
fn every_staged_operation_name_is_routed_and_unrelated_names_are_not() {
    for operation in [
        "command.document.object.component.convert",
        "command.document.object.component.expose",
        "command.document.object.component.place",
        "command.document.object.duplicator.set",
        "command.document.object.duplicator.objects",
        "command.document.object.duplicator.path",
        "command.document.object.duplicator.config",
        "command.document.object.component.instance.set",
    ] {
        assert!(
            NativeObjectHistory::component_setup_operation(operation),
            "{operation}"
        );
    }
    for operation in [
        "command.document.object.group.create",
        "command.document.object.component",
        "",
    ] {
        assert!(
            !NativeObjectHistory::component_setup_operation(operation),
            "{operation}"
        );
    }
    let mut owner = owner(&s1_document(json!({})));
    let place = json!({"command":"object.component.place","symbolId":"nope","name":"x"});
    let reply = send(
        &mut owner,
        "place",
        "command.document.object.component.place",
        place,
    );
    assert_eq!(reply["error"]["code"], "not_found");
}

#[test]
fn an_effect_layer_and_a_layer_with_items_are_refused_in_typed_receipts_that_publish_nothing() {
    let layers = [
        layer("ly_fx", "Effects", &[]).with(json!({"isEffectLayer":true,"effects":[]})),
        layer("ly_plain", "Plain", &[(0, "pa")]),
    ];
    let mut owner = owner(&document(&layers, json!({})));
    let original = value(&owner);
    for (id, uids) in [
        ("fx", vec!["ly_fx"]),
        ("fx-in-selection", vec!["ly_plain", "ly_fx"]),
    ] {
        let reply = send(&mut owner, id, CONVERT, convert(&uids));
        assert_eq!(reply["ok"], false, "{id}");
        assert_eq!(reply["error"]["code"], "invalid_request", "{id}");
        assert_eq!(
            reply["error"]["message"], "Impossible de convertir : un calque d'effet",
            "{id}"
        );
    }
    let mut owner_items = owner_for_items();
    let original_items = value(&owner_items);
    for (id, uids) in [
        ("single", vec!["ly_hand"]),
        ("merge", vec!["ly_other", "ly_hand"]),
    ] {
        let reply = send(&mut owner_items, id, CONVERT, convert(&uids));
        assert_eq!(reply["ok"], false, "{id}");
        assert_eq!(reply["error"]["code"], "unavailable", "{id}");
        assert_eq!(
            reply["error"]["message"],
            "component-convert-items: animated text, brush and mesh-bound items cannot enter a component yet.",
            "{id}"
        );
    }
    assert_eq!(value(&owner), original);
    assert_eq!(value(&owner_items), original_items);
    for held in [&owner, &owner_items] {
        assert_eq!(
            (held.content_revision(), held.history_depths()),
            (0, (0, 0))
        );
    }
}

fn owner_for_items() -> NativeObjectHistory {
    owner(&s1_with_animated_item("ly_hand", 3))
}
