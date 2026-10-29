//! Preview is a detached read: no document/history write and no caller-provided geometry.
#[path = "object_vector_edit_support.rs"]
#[allow(dead_code)]
mod support;
use serde_json::{json, Value};
use support::{bytes, document, owner_of, record_at, target};
#[test]
fn subselect_read_pages_and_preview_never_publish() {
    let owner = owner_of(&document(2, vec![record_at("A", "L1", 0, "s-a")], None));
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let before = bytes(&owner);
    let mut request = json!({"apiVersion":2,"requestId":"read-subselect","instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "operation":"query.document.object","payload":{"atRevision":0,"documentSnapshotId":snapshot.snapshot_id(),
        "stableTarget":target("L1",0,"s-a"),"subselect":{"cursor":0,"limit":1}}});
    let read = |v: &Value| {
        serde_json::to_value(
            snapshot
                .dispatch(serde_json::from_value(v.clone()).unwrap())
                .unwrap(),
        )
        .unwrap()
    };
    let raw = read(&request);
    assert_eq!(raw["ok"], true, "{raw}");
    assert_eq!(raw["result"]["subselect"]["nextCursor"], 1);
    request["payload"]["subselect"]["preview"] = json!({"kind":"move","indices":[0],"delta":[4,7]});
    let preview = read(&request);
    assert_eq!(preview["ok"], true, "{preview}");
    assert_eq!(preview["result"]["subselect"]["preview"], true);
    for (axis, delta) in [(0, 4.0), (1, 7.0)] {
        assert_eq!(
            preview["result"]["subselect"]["segments"][0]["point"][axis]
                .as_f64()
                .unwrap(),
            raw["result"]["subselect"]["segments"][0]["point"][axis]
                .as_f64()
                .unwrap()
                + delta
        );
    }
    assert_eq!(bytes(&owner), before);
    assert_eq!(owner.history_depths(), (0, 0));
    request["payload"]["documentSnapshotId"] = "replaced".into();
    assert_eq!(read(&request)["error"]["code"], "stale_revision");
    request["documentId"] = "replacement-document".into();
    assert_eq!(read(&request)["error"]["code"], "wrong_document");
    assert_eq!(bytes(&owner), before);
}

#[test]
fn a_hit_test_read_pages_painted_geometry_outside_the_editable_scope() {
    // A stroke on a 3D layer is outside the editable selection, but Select and the knife must still be able to hit it.
    let mut doc = document(2, vec![record_at("A", "L1", 0, "s-a")], None);
    doc["layers"][0]["threeD"] = json!(true);
    let owner = owner_of(&doc);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let stored = snapshot.document().objects[0].geometry.segments.len();
    let read = |page: Value| {
        let v = json!({"apiVersion":2,"requestId":"hit","instanceId":owner.instance_id(),"documentId":owner.document_id(),
            "operation":"query.document.object","payload":{"atRevision":0,"documentSnapshotId":snapshot.snapshot_id(),
            "stableTarget":target("L1",0,"s-a"),"subselect":page}});
        serde_json::to_value(snapshot.dispatch(serde_json::from_value(v).unwrap()).unwrap()).unwrap()
    };
    assert_eq!(read(json!({"cursor":0,"limit":4}))["ok"], false, "the editable read refuses it");
    let hit = read(json!({"cursor":0,"limit":16,"hitTest":true}));
    assert_eq!(hit["ok"], true, "{hit}");
    let s = &hit["result"]["subselect"];
    assert_eq!((s["total"].as_u64(), s["hitTest"].as_bool()), (Some(stored as u64), Some(true)));
    let first = &snapshot.document().objects[0].geometry.segments[0];
    assert_eq!((s["segments"][0]["point"][0].as_f64(), s["segments"][0]["point"][1].as_f64()), (first.point.x.as_f64(), first.point.y.as_f64()));
    for bad in [json!({"cursor":0,"limit":4,"hitTest":false}), json!({"cursor":0,"limit":4,"hitTest":true,"paint":true}),
        json!({"cursor":0,"limit":4,"hitTest":true,"preview":{"kind":"move","indices":[0],"delta":[1,1]}})] {
        assert_eq!(read(bad.clone())["ok"], false, "{bad}");
    }
}

#[test]
fn a_centre_read_pages_a_ribbons_centreline_and_widths() {
    // Sculpt deforms a ribbon's centreline, even one with a linked fill that the editable read refuses.
    let mut ribbon = record_at("A", "L1", 0, "s-r");
    ribbon["geometry"]["closed"] = json!(true);
    ribbon["fill"] = json!({"kind":"solid","r":0.1,"g":0.1,"b":0.1,"a":1});
    ribbon["vector"]["stroke"] = Value::Null;
    ribbon["vector"]["hasRealStroke"] = json!(false);
    let anchor = |x: f64, w: f64| json!({"point":{"x":x,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":w});
    ribbon["vectorExt"] = json!({"linkedFillId":"lf","pressure":{"centerSegments":[anchor(0., 2.), anchor(20., 3.), anchor(40., 4.)]}});
    let mut fill = record_at("A", "L1", 0, "s-f");
    fill["geometry"]["closed"] = json!(true);
    fill["fill"] = ribbon["fill"].clone();
    fill["vectorExt"] = json!({"linkedFillId":"lf","isLinkedFillCompanion":true});
    let owner = owner_of(&document(2, vec![fill, ribbon, record_at("A", "L1", 0, "s-a")], None));
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let read = |id: &str, page: Value| {
        let v = json!({"apiVersion":2,"requestId":"centre","instanceId":owner.instance_id(),"documentId":owner.document_id(),
            "operation":"query.document.object","payload":{"atRevision":0,"documentSnapshotId":snapshot.snapshot_id(),
            "stableTarget":target("L1",0,id),"subselect":page}});
        serde_json::to_value(snapshot.dispatch(serde_json::from_value(v).unwrap()).unwrap()).unwrap()
    };
    assert_eq!(read("s-r", json!({"cursor":0,"limit":4}))["ok"], false, "the editable read refuses a ribbon with a linked fill");
    let page = read("s-r", json!({"cursor":1,"limit":16,"hitTest":true,"center":true}));
    assert_eq!(page["ok"], true, "{page}");
    let s = &page["result"]["subselect"];
    assert_eq!((s["total"].as_u64(), s["center"].as_bool(), s["closed"].as_bool()), (Some(3), Some(true), Some(false)));
    assert_eq!(s["segments"][0]["point"], json!([20.0, 10.0]));
    assert_eq!(s["widths"], json!([3.0, 4.0]), "widths page with their anchors");
    for (id, bad) in [("s-a", json!({"cursor":0,"limit":4,"hitTest":true,"center":true})), ("s-r", json!({"cursor":0,"limit":4,"center":true})),
        ("s-r", json!({"cursor":0,"limit":4,"hitTest":true,"center":false}))] {
        assert_eq!(read(id, bad.clone())["ok"], false, "{id} {bad}");
    }
}
