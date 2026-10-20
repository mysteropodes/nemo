//! Real immutable-owner reads: original selectors, raw presence, pinned bounded pages.
use crate::history::NativeObjectHistory;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode as Error, OpacityRequest};
use serde_json::{json, Value};

fn owner(markers: Option<Value>) -> NativeObjectHistory {
    let mut metadata = json!({"canvasW":320,"canvasH":180,"fps":24});
    if let Some(markers) = markers {
        metadata["markers"] = markers;
    }
    let wire = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":metadata, "drawing":{"format":"nemo.native-object-document",
        "formatVersion":1,"totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]},
        "viewportReference":null,"assets":[]});
    NativeObjectHistory::from_project_file("composition-read", wire.to_string().as_bytes()).unwrap()
}
fn query(pin: &ObjectSnapshot, selector: Value) -> Value {
    json!({"command":"object.project.composition.read", "instanceId":pin.instance_id(),
        "documentId":pin.document_id(), "atRevision":pin.content_revision(),
        "documentSnapshotId":pin.snapshot_id(), "contextId":"scene-root", "selector":selector})
}
fn read(pin: &ObjectSnapshot, selector: Value) -> Result<Value, Error> {
    pin.query_composition_metadata_json(query(pin, selector).to_string().as_bytes())
}
fn envelope(pin: &ObjectSnapshot, selector: Value) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"r".repeat(128),
        "instanceId":pin.instance_id(),"documentId":pin.document_id(),
        "operation":"query.document.object.project.composition.read",
        "payload":{"atRevision":pin.content_revision(),"documentSnapshotId":pin.snapshot_id(),
            "contextId":"scene-root","selector":selector}}))
    .unwrap()
}

#[test]
fn marker_pages_preserve_import_order_duplicates_and_raw_optional_presence() {
    let markers = json!([{"frame":30.5},{"frame":-2,"name":"","color":""},
        {"frame":30.5,"name":"X","color":"blue"},{"frame":0,"name":"X"}]);
    let o = owner(Some(markers.clone()));
    let pin = o.acquire_snapshot(0).unwrap();
    let first = read(&pin, json!({"kind":"markerPage","cursor":0,"limit":2})).unwrap();
    assert_eq!(
        first["result"],
        json!({"kind":"markerPage","markersPresent":true,
        "markerCount":4,"cursor":0,"nextCursor":2,"items":[
        {"ordinal":1,"frame":30.5},{"ordinal":2,"frame":-2.0,"name":"","color":""}]})
    );
    let second = read(&pin, json!({"kind":"markerPage","cursor":2,"limit":2})).unwrap();
    assert_eq!(
        second["result"]["items"],
        json!([
        {"ordinal":3,"frame":30.5,"name":"X","color":"blue"},
        {"ordinal":4,"frame":0.0,"name":"X"}])
    );
    assert!(second["result"]["nextCursor"].is_null());
    assert_eq!(
        read(&pin, json!({"kind":"markerNamed","name":"X"})).unwrap()["result"]["index"],
        3
    );
    assert_eq!(
        read(&pin, json!({"kind":"markerNearest","frame":30.5})).unwrap()["result"]["index"],
        1
    );
    let old = read(&pin, json!({"kind":"markerAt","index":1})).unwrap();
    assert_eq!(
        old["result"],
        json!({"index":1,"frame":30.5,"time":30.5/24.0,"name":"","color":null})
    );
}

#[test]
fn absent_and_explicit_empty_are_distinct_without_persisting_effective_defaults() {
    for (markers, present) in [(None, false), (Some(json!([])), true)] {
        let o = owner(markers);
        let pin = o.acquire_snapshot(0).unwrap();
        let before = o.serialize_project_file(0, pin.snapshot_id()).unwrap();
        let page = read(&pin, json!({"kind":"markerPage","cursor":0,"limit":16})).unwrap();
        assert_eq!(
            page["result"],
            json!({"kind":"markerPage","markersPresent":present,
            "markerCount":0,"cursor":0,"items":[],"nextCursor":null})
        );
        let settings = read(&pin, json!({"kind":"settings"})).unwrap();
        assert_eq!(
            settings["result"],
            json!({"markersPresent":present,"markerCount":0,
            "effectiveBpm":120.0,"effectiveBpmOffset":0.0,"effectiveBpmShow":false})
        );
        assert_eq!(
            o.serialize_project_file(0, pin.snapshot_id()).unwrap(),
            before
        );
    }
}

#[test]
fn complete_public_envelope_budget_pages_forward_without_dropping_records() {
    let markers: Vec<Value> = (0..19)
        .map(|i| json!({"frame":i,"name":"é".repeat(512)}))
        .collect();
    let o = owner(Some(json!(markers)));
    let pin = o.acquire_snapshot(0).unwrap();
    let mut cursor = 0;
    let mut actual = Vec::new();
    loop {
        let request = envelope(
            &pin,
            json!({"kind":"markerPage","cursor":cursor,"limit":16}),
        );
        let response = pin.dispatch_composition_read(&request).unwrap();
        let bytes = serde_json::to_vec(&response).unwrap();
        assert!(bytes.len() <= 4096);
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["ok"], true);
        let page = &value["result"]["result"];
        let items = page["items"].as_array().unwrap();
        assert!(!items.is_empty() && items.len() < 16);
        for item in items {
            actual.push(item.clone());
        }
        match page["nextCursor"].as_u64() {
            Some(next) => {
                assert!(next > cursor);
                assert_eq!(next as usize, actual.len());
                cursor = next;
            }
            None => break,
        }
    }
    assert_eq!(actual.len(), 19);
    for (i, item) in actual.iter().enumerate() {
        assert_eq!(
            item,
            &json!({"ordinal":i+1,"frame":i as f64,"name":"é".repeat(512)})
        );
    }
}

#[test]
fn escaped_single_record_over_budget_refuses_instead_of_empty_success_or_loop() {
    let o = owner(Some(json!([{"frame":1,"name":"\u{1}".repeat(1024)}])));
    let pin = o.acquire_snapshot(0).unwrap();
    assert_eq!(
        read(&pin, json!({"kind":"markerPage","cursor":0,"limit":1})),
        Err(Error::Unavailable)
    );
    let request = envelope(&pin, json!({"kind":"markerPage","cursor":0,"limit":16}));
    let response = serde_json::to_value(pin.dispatch_composition_read(&request).unwrap()).unwrap();
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "unavailable");
}

#[test]
fn malformed_selectors_and_wrong_pins_refuse_without_mutation() {
    let o = owner(Some(json!([{"frame":1}])));
    let pin = o.acquire_snapshot(0).unwrap();
    let before = o.serialize_project_file(0, pin.snapshot_id()).unwrap();
    for selector in [
        json!([]),
        json!({"kind":"markerPage","cursor":0,"limit":0}),
        json!({"kind":"markerPage","cursor":0,"limit":17}),
        json!({"kind":"markerPage","cursor":2,"limit":1}),
        json!({"kind":"markerPage","cursor":0,"limit":1,"unknown":true}),
        json!({"kind":"markerPage","cursor":-1,"limit":1}),
    ] {
        assert_eq!(read(&pin, selector), Err(Error::InvalidRequest));
    }
    for (field, value, error) in [
        ("instanceId", json!("other"), Error::WrongInstance),
        ("documentId", json!("other"), Error::WrongDocument),
        ("atRevision", json!(1), Error::StaleRevision),
        ("documentSnapshotId", json!("other"), Error::StaleRevision),
        ("contextId", json!("component"), Error::Unavailable),
    ] {
        let mut raw = query(&pin, json!({"kind":"settings"}));
        raw[field] = value;
        assert_eq!(
            pin.query_composition_metadata_json(raw.to_string().as_bytes()),
            Err(error)
        );
    }
    let raw = query(&pin, json!({"kind":"markerPage","cursor":0,"limit":1})).to_string();
    let duplicate = raw.replace("\"limit\":1", "\"limit\":1,\"limit\":2");
    assert_ne!(raw, duplicate);
    assert_eq!(
        pin.query_composition_metadata_json(duplicate.as_bytes()),
        Err(Error::InvalidRequest)
    );
    assert_eq!(
        o.serialize_project_file(0, pin.snapshot_id()).unwrap(),
        before
    );
    assert_eq!(o.history_depths(), (0, 0));
}
