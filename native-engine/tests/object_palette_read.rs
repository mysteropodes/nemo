//! Actual immutable snapshot reads; public dispatcher/client admission remains separate.
use crate::history::NativeObjectHistory;
use crate::object_document::ObjectDocument;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode as Error, OpacityRequest};
use serde_json::{json, Value};
fn owner(extra: Value) -> NativeObjectHistory {
    let mut project = json!({"canvasW":64,"canvasH":48,"fps":24});
    project
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    NativeObjectHistory::from_project_file("palette-read",json!({"format":"nemo.native-project-document","formatVersion":1,"project":project,
        "viewportReference":null,"assets":[],"drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]}}).to_string().as_bytes()).unwrap()
}
fn pin(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let p = pin(o);
    o.serialize_project_file(p.content_revision(), p.snapshot_id())
        .unwrap()
}
fn query(p: &ObjectSnapshot, selector: Value) -> Value {
    json!({"command":"object.project.palette.read","instanceId":p.instance_id(),"documentId":p.document_id(),
    "atRevision":p.content_revision(),"documentSnapshotId":p.snapshot_id(),"contextId":"scene-root","selector":selector})
}
fn read(p: &ObjectSnapshot, selector: Value) -> Result<Value, Error> {
    p.query_palette_metadata_json(query(p, selector).to_string().as_bytes())
}
fn request(p: &ObjectSnapshot, selector: Value) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"r".repeat(128),"instanceId":p.instance_id(),"documentId":p.document_id(),
    "operation":"query.document.object.project.palette.read","payload":{"atRevision":p.content_revision(),"documentSnapshotId":p.snapshot_id(),"contextId":"scene-root","selector":selector}})).unwrap()
}
#[test]
fn palette_settings_distinguish_absent_empty_and_raw_selection_without_writing_defaults() {
    for (extra, present, index) in [
        (json!({}), false, None),
        (json!({"palettes":[]}), true, None),
        (json!({"activePaletteIdx":-0.5}), false, Some(-0.5)),
        (json!({"activePaletteIdx":0.5}), false, Some(0.5)),
    ] {
        let o = owner(extra);
        let before = saved(&o);
        let p = pin(&o);
        let r = read(&p, json!({"kind":"settings"})).unwrap();
        assert_eq!(r["instanceId"], p.instance_id());
        assert_eq!(r["documentSnapshotId"], p.snapshot_id());
        assert_eq!(r["contextId"], "scene-root");
        assert_eq!(r["result"]["palettesPresent"], present);
        assert_eq!(r["result"]["paletteCount"], 0);
        assert_eq!(
            r["result"].get("activePaletteIdx").and_then(Value::as_f64),
            index
        );
        assert_eq!(
            r["result"]["effectiveActivePaletteIdx"].as_f64(),
            Some(index.unwrap_or(0.))
        );
        assert_eq!(saved(&o), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
}
#[test]
fn palette_summary_and_color_pages_preserve_raw_order_duplicates_and_ordinal_identity() {
    let o = owner(
        json!({"palettes":[{"id":"same","name":"Z","colors":["#AbC","#abc","#AbC"]},{"id":"same","name":"A","colors":[""]}],"activePaletteIdx":0.5}),
    );
    let p = pin(&o);
    let r = read(&p, json!({"kind":"palettePage","cursor":0,"limit":16})).unwrap();
    assert_eq!(
        r["result"]["items"],
        json!([{"ordinal":1,"id":"same","name":"Z","colorCount":3},{"ordinal":2,"id":"same","name":"A","colorCount":1}])
    );
    let r = read(
        &p,
        json!({"kind":"colorPage","ordinal":1,"cursor":0,"limit":2}),
    )
    .unwrap();
    assert_eq!(
        r["result"]["items"],
        json!([{"index":1,"color":"#AbC"},{"index":2,"color":"#abc"}])
    );
    assert_eq!(r["result"]["nextCursor"], 2);
    let r = read(
        &p,
        json!({"kind":"colorPage","ordinal":1,"cursor":2,"limit":2}),
    )
    .unwrap();
    assert_eq!(r["result"]["items"], json!([{"index":3,"color":"#AbC"}]));
    assert!(r["result"]["nextCursor"].is_null());
    assert_eq!(
        read(&p, json!({"kind":"settings"})).unwrap()["result"]["effectiveActivePaletteIdx"]
            .as_f64(),
        Some(0.5)
    );
}
#[test]
fn palette_private_wrapper_charges_complete_encoded_messages_and_adapts_without_omitting_items() {
    let colors = vec!["é".repeat(512); 7];
    let palettes:Vec<Value>=(0..7).map(|i|json!({"id":"i".repeat(1024),"name":format!("{i}{}","n".repeat(1023)),"colors":colors})).collect();
    let o = owner(json!({"palettes":palettes}));
    let p = pin(&o);
    for kind in ["palettePage", "colorPage"] {
        let mut cursor = 0;
        let mut received = Vec::new();
        loop {
            let mut selector = json!({"kind":kind,"cursor":cursor,"limit":16});
            if kind == "colorPage" {
                selector["ordinal"] = json!(1);
            }
            let response = p.dispatch_palette_read(&request(&p, selector)).unwrap();
            assert!(response.ok, "{response:?}");
            assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
            let body = response.result.unwrap()["result"].clone();
            let items = body["items"].as_array().unwrap();
            assert!(!items.is_empty());
            assert!(items.len() < 7);
            received.extend(items.clone());
            match body["nextCursor"].as_u64() {
                Some(next) => {
                    assert!(next > cursor);
                    cursor = next;
                }
                None => break,
            }
        }
        assert_eq!(received.len(), 7);
        for (i, item) in received.iter().enumerate() {
            assert_eq!(
                item[if kind == "colorPage" {
                    "index"
                } else {
                    "ordinal"
                }]
                .as_u64(),
                Some((i + 1) as u64)
            );
        }
    }
}
#[test]
fn palette_queries_refuse_stale_foreign_nonroot_and_malformed_requests_without_affecting_snapshots()
{
    let mut o = owner(json!({"palettes":[{"id":"p","name":"Before","colors":[]}]}));
    let old = pin(&o);
    let old_query = query(&old, json!({"kind":"settings"}));
    let delta=json!({"command":"object.project.palette.edit","action":{"kind":"rename","ordinal":1,"name":"After"}}).to_string();
    let candidate = o.prepare_palette_delta_json(delta.as_bytes()).unwrap();
    o.commit_project_settings(candidate).unwrap();
    let current = pin(&o);
    let before = saved(&o);
    assert_eq!(
        current
            .query_palette_metadata_json(old_query.to_string().as_bytes())
            .unwrap_err(),
        Error::StaleRevision
    );
    assert_eq!(
        read(&old, json!({"kind":"palettePage","cursor":0,"limit":16})).unwrap()["result"]["items"]
            [0]["name"],
        "Before"
    );
    for (field, value, code) in [
        ("instanceId", json!("foreign"), Error::WrongInstance),
        ("documentId", json!("foreign"), Error::WrongDocument),
        ("contextId", json!("component"), Error::Unavailable),
        ("documentSnapshotId", json!("other"), Error::StaleRevision),
    ] {
        let mut q = query(&current, json!({"kind":"settings"}));
        q[field] = value;
        assert_eq!(
            current
                .query_palette_metadata_json(q.to_string().as_bytes())
                .unwrap_err(),
            code
        );
    }
    for selector in [
        json!([]),
        json!({"kind":"palettePage","cursor":0,"limit":0}),
        json!({"kind":"palettePage","cursor":2,"limit":16}),
        json!({"kind":"colorPage","ordinal":0,"cursor":0,"limit":16}),
        json!({"kind":"settings","extra":true}),
    ] {
        assert_eq!(read(&current, selector).unwrap_err(), Error::InvalidRequest);
    }
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (1, 0));
}
#[test]
fn escaped_single_record_refuses_instead_of_truncating_and_bare_document_is_unavailable() {
    let o = owner(
        json!({"palettes":[{"id":"p","name":"\u{1}".repeat(1024),"colors":["\u{1}".repeat(1024)]}]}),
    );
    let p = pin(&o);
    for selector in [
        json!({"kind":"palettePage","cursor":0,"limit":16}),
        json!({"kind":"colorPage","ordinal":1,"cursor":0,"limit":16}),
    ] {
        assert_eq!(read(&p, selector.clone()).unwrap_err(), Error::Unavailable);
        let response = p.dispatch_palette_read(&request(&p, selector)).unwrap();
        assert!(!response.ok);
        assert_eq!(
            serde_json::to_value(response).unwrap()["error"]["code"],
            "unavailable"
        );
    }
    let document:ObjectDocument=serde_json::from_value(json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":2,"layers":[{"layerUid":"L"}],"objects":[]})).unwrap();
    let bare = NativeObjectHistory::new("bare-palette-read", document).unwrap();
    assert_eq!(
        read(&pin(&bare), json!({"kind":"settings"})).unwrap_err(),
        Error::Unavailable
    );
}
