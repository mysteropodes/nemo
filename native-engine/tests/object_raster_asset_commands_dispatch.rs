//! The raster / text transactions through the existing object dispatch: cancellation, stale and foreign pins,
//! request replay and the response envelope. The ingress bound of 4096 bytes is the existing one, unchanged.
use super::super::support::{
    bytes, dispatch, drawing, head, items_of, owner, raster_item, request, state, target, OP,
};
use crate::history::object_history::NativeObjectHistory;
use crate::request_receipts::OpacityRequest;
use serde_json::{json, Value};

fn create_payload(asset: &str) -> Value {
    json!({"command":"object.raster.create","target":target("L0",""),"item":raster_item(asset, 1, 2, 3, 4)})
}

fn opacity(id: &str, value: f64) -> Value {
    json!({"command":"object.raster.opacity","stableTarget":target("L0",id),"opacity":value})
}

fn history_request(
    o: &NativeObjectHistory,
    id: &str,
    operation: &str,
    payload: Value,
) -> OpacityRequest {
    assert_ne!(operation, OP, "history operations only");
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":operation,"payload":payload}),
    )
    .unwrap()
}

#[test]
fn a_create_through_dispatch_reports_the_minted_id_and_replays_without_executing_twice() {
    let mut o = owner(&drawing(json!({})));
    let r = request(&o, "item-1", &create_payload("img-1"));
    let first = dispatch(&mut o, &r);
    assert_eq!(first["ok"], true);
    assert_eq!(first["contentRevision"], 1);
    assert_eq!(
        first["result"],
        json!({"applied":true,"historyEntriesAdded":1,"operation":"raster.create","strokeId":"native-raster:1:0"})
    );
    let after = state(&o);
    assert_eq!(
        dispatch(&mut o, &r),
        first,
        "the retained response is replayed"
    );
    assert_eq!(state(&o), after, "replay does not execute twice");
    let mut changed = r.clone();
    changed.payload["item"]["x"] = json!(99);
    assert_eq!(
        dispatch(&mut o, &changed)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), after);
    // The retained response is also replayed after later history moved on.
    let next = request(&o, "item-2", &opacity("native-raster:1:0", 0.5));
    assert_eq!(dispatch(&mut o, &next)["ok"], true);
    let moved = state(&o);
    assert_eq!(dispatch(&mut o, &r), first);
    assert_eq!(state(&o), moved);
}

#[test]
fn an_exact_noop_through_dispatch_is_applied_false_and_keeps_the_redo_entry() {
    let mut o = owner(&drawing(json!({})));
    let id = "native-raster:1:0";
    let created = request(&o, "c", &create_payload("img-1"));
    dispatch(&mut o, &created);
    let dimmed = request(&o, "d", &opacity(id, 0.5));
    dispatch(&mut o, &dimmed);
    assert_eq!(o.undo(2), Ok(3));
    assert_eq!(o.history_depths(), (1, 1), "one undo entry, one redo entry");
    let before = state(&o);
    let noop = request(&o, "n", &opacity(id, 1.0));
    let reply = dispatch(&mut o, &noop);
    assert_eq!(reply["ok"], true);
    assert_eq!(reply["result"]["applied"], false);
    assert_eq!(reply["result"]["historyEntriesAdded"], 0);
    assert_eq!(reply["result"]["strokeId"], id);
    assert_eq!(
        state(&o),
        before,
        "no revision, no history entry, redo preserved"
    );
    let real = request(&o, "r", &opacity(id, 0.25));
    assert_eq!(dispatch(&mut o, &real)["result"]["applied"], true);
    assert_eq!(o.history_depths(), (2, 0), "a changed edit clears redo");
}

#[test]
fn cancel_stale_missing_revision_and_foreign_pins_publish_nothing_and_refusals_are_retained() {
    let mut o = owner(&drawing(json!({})));
    let before = state(&o);
    let mut cancelled = request(&o, "cancel", &create_payload("img-1"));
    cancelled.cancelled_before_dispatch = true;
    assert_eq!(
        dispatch(&mut o, &cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    let mut stale = request(&o, "stale", &create_payload("img-1"));
    stale.expected_revision = Some(7);
    assert_eq!(dispatch(&mut o, &stale)["error"]["code"], "stale_revision");
    let mut missing = request(&o, "missing", &create_payload("img-1"));
    missing.expected_revision = None;
    assert_eq!(
        dispatch(&mut o, &missing)["error"]["code"],
        "invalid_request"
    );
    let mut foreign_instance = request(&o, "fi", &create_payload("img-1"));
    foreign_instance.instance_id = "foreign".into();
    assert_eq!(
        dispatch(&mut o, &foreign_instance)["error"]["code"],
        "wrong_instance"
    );
    let mut foreign_document = request(&o, "fd", &create_payload("img-1"));
    foreign_document.document_id = "foreign".into();
    assert_eq!(
        dispatch(&mut o, &foreign_document)["error"]["code"],
        "wrong_document"
    );
    assert_eq!(state(&o), before);
    let refused = request(&o, "refused", &create_payload("nope"));
    let first = dispatch(&mut o, &refused);
    assert_eq!(first["ok"], false);
    assert_eq!(first["error"]["code"], "not_found");
    assert_eq!(state(&o), before, "a refused create publishes nothing");
    assert_eq!(
        dispatch(&mut o, &refused),
        first,
        "the refusal is retained and replayed"
    );
}

#[test]
fn undo_and_redo_run_through_the_same_dispatch_and_restore_the_exact_items() {
    let mut o = owner(&drawing(json!({})));
    let start = bytes(&o);
    let r = request(&o, "c", &create_payload("img-1"));
    dispatch(&mut o, &r);
    let created = (bytes(&o), items_of(&o));
    let undo = history_request(
        &o,
        "u",
        "command.document.object.undo",
        json!({"command":"object.undo"}),
    );
    assert_eq!(dispatch(&mut o, &undo)["ok"], true);
    assert_eq!(bytes(&o), start);
    let redo = history_request(
        &o,
        "r",
        "command.document.object.redo",
        json!({"command":"object.redo"}),
    );
    assert_eq!(dispatch(&mut o, &redo)["ok"], true);
    assert_eq!((bytes(&o), items_of(&o)), created);
    assert_eq!(
        head(&o)
            .document()
            .item_records()
            .unwrap()
            .raster_entries()
            .len(),
        1
    );
}
