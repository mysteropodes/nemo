use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::json;
fn public_read(q: &Value, r: &Value) -> bool {
    crate::native_contract::validate_result("query.document.object", "document", q, r)
}
fn target() -> Value {
    json!({"contextId":"scene-root","layerUid":"layer-a","frameScope":{"kind":"authored","frame":0},"strokeId":"stroke-a"})
}
fn query() -> Value {
    json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","stableTarget":target(),"subselect":{"cursor":0,"limit":16}})
}
fn page() -> Value {
    json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","subselect":{"stableTarget":target(),"closed":false,"total":2,"cursor":0,"nextCursor":null,"preview":false,"segments":[{"point":[0,0],"handleIn":[0,0],"handleOut":[0,0]},{"point":[10,0],"handleIn":[0,0],"handleOut":[0,0]}]}})
}
#[test]
fn subselect_closed_request_and_geometry_correlation() {
    let q = query();
    let r = page();
    assert!(request(&q));
    assert!(public_read(&q, &r));
    assert!(crate::native_contract::validate_request(
        "query.document.object",
        &q
    ));
    for (path, value) in [
        ("/subselect/stableTarget/strokeId", json!("other")),
        ("/subselect/cursor", json!(1)),
        ("/subselect/total", json!(3)),
        ("/subselect/preview", json!(true)),
        ("/documentSnapshotId", json!("foreign")),
    ] {
        let mut bad = r.clone();
        *bad.pointer_mut(path).unwrap() = value;
        assert!(!public_read(&q, &bad), "{path}");
    }
    let mut mixed = q.clone();
    mixed["view"] = json!({"kind":"presentation"});
    assert!(!crate::native_contract::validate_request(
        "query.document.object",
        &mixed
    ));
    let mut null = q.clone();
    null["subselect"]["preview"] = Value::Null;
    assert!(!request(&null));
}
#[test]
fn subselect_command_uses_shared_owner_and_revision_correlator() {
    let mut q:NativeApplicationRequest=serde_json::from_value(json!({"apiVersion":2,"requestId":"edit-1","instanceId":"instance","documentId":"document","expectedRevision":7,"operation":EDIT,"payload":{"command":"object.subselect.edit","stableTarget":target(),"intent":{"kind":"handle","index":0,"handle":"handleOut","point":[5,4],"breakPair":false}}})).unwrap();
    assert!(q.validate_input().is_ok());
    assert!(q.validate().is_ok());
    q.payload["atFrame"] = json!(5);
    assert!(q.validate_input().is_ok());
    assert!(q.validate().is_ok());
    for value in [
        json!(-1),
        json!(1.5),
        json!(4294967296u64),
        Value::Null,
        json!("5"),
    ] {
        let mut bad = q.clone();
        bad.payload["atFrame"] = value;
        assert!(bad.validate().is_err());
    }
    q.payload["intent"]["breakPair"] = json!(true);
    q.payload["intent"]["pairedPoint"] = json!([3, 2]);
    assert!(q.validate_input().is_ok());
    assert!(q.validate().is_ok());
    for value in [Value::Null, json!([1]), json!(["x", 2])] {
        let mut bad = q.clone();
        bad.payload["intent"]["pairedPoint"] = value;
        assert!(bad.validate().is_err());
    }
    let mut bad = q.clone();
    bad.payload["intent"]["breakPair"] = json!(false);
    assert!(bad.validate().is_err());
    let r:NativeApplicationResponse=serde_json::from_value(json!({"apiVersion":2,"requestId":"edit-1","instanceId":"instance","documentId":"document","contentRevision":8,"ok":true,"result":{"applied":true,"historyEntriesAdded":1}})).unwrap();
    assert!(crate::native_contract::object_edit::response_matches(
        &q, &r
    ));
    for (key, value) in [
        ("instanceId", json!("foreign")),
        ("documentId", json!("foreign")),
        ("contentRevision", json!(9)),
    ] {
        let mut bad = serde_json::to_value(&r).unwrap();
        bad[key] = value;
        let bad = serde_json::from_value(bad).unwrap();
        assert!(!crate::native_contract::object_edit::response_matches(
            &q, &bad
        ));
    }
    q.expected_revision = None;
    assert!(q.validate_input().is_err());
}
#[test]
fn subselect_view_is_correlated_to_native_presentation_and_preview() {
    let view = json!({"kind":"subselect","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"limit":8});
    let mut q = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":view});
    let mut r = json!({"kind":"subselect","atRevision":7,"documentSnapshotId":"native-object:document:7","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"total":1,"nextCursor":null,"preview":false,"items":[{"stableTarget":target(),"closed":false,"count":2,"matrix":[1,0.2,0.3,1,8,9]}]});
    assert!(view_request(&q));
    assert!(public_read(&q, &r));
    for (key, value) in [
        ("workId", json!("native-work-2")),
        ("frame", json!(1)),
        ("preview", json!(true)),
        ("total", json!(2)),
    ] {
        let mut bad = r.clone();
        bad[key] = value;
        assert!(!public_read(&q, &bad), "{key}");
    }
    q["view"]["preview"] =
        json!({"stableTarget":target(),"intent":{"kind":"move","indices":[0],"delta":[1,2]}});
    r["preview"] = json!(true);
    assert!(view_request(&q));
    assert!(public_read(&q, &r));
    r["items"][0]["matrix"] = json!([1, 2, 2, 4, 0, 0]);
    assert!(!public_read(&q, &r));
}

#[test]
fn subselect_paint_pages_require_exact_opt_in_marker_and_closed_geometry() {
    let mut q = query();
    q["subselect"]["paint"] = json!(true);
    let mut r = page();
    r["subselect"]["paint"] = json!(true);
    r["subselect"]["closed"] = json!(true);
    assert!(request(&q));
    assert!(crate::native_contract::validate_request(
        "query.document.object",
        &q
    ));
    assert!(public_read(&q, &r));
    q["subselect"]["preview"] = json!({"kind":"move","indices":[0],"delta":[1,2]});
    r["subselect"]["preview"] = json!(true);
    assert!(public_read(&q, &r));
    for value in [json!(false), Value::Null, json!("true"), json!(1)] {
        let mut bad = q.clone();
        bad["subselect"]["paint"] = value;
        assert!(!request(&bad));
    }
    let mut missing = r.clone();
    missing["subselect"]
        .as_object_mut()
        .unwrap()
        .remove("paint");
    assert!(!public_read(&q, &missing));
    let mut no_opt_in = q.clone();
    no_opt_in["subselect"]
        .as_object_mut()
        .unwrap()
        .remove("paint");
    assert!(!public_read(&no_opt_in, &r));
    for (key, value) in [
        ("paint", json!(false)),
        ("paint", Value::Null),
        ("closed", json!(false)),
    ] {
        let mut bad = r.clone();
        bad["subselect"][key] = value;
        assert!(!public_read(&q, &bad));
    }
}

#[test]
fn subselect_pressure_view_paint_count_is_optional_bounded_and_centerline_only() {
    let q = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":{"kind":"subselect","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"limit":8}});
    let mut r = json!({"kind":"subselect","atRevision":7,"documentSnapshotId":"native-object:document:7","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"total":1,"nextCursor":null,"preview":false,"items":[{"stableTarget":target(),"closed":false,"count":2,"paintCount":20,"matrix":[1,0,0,1,0,0]}]});
    assert!(public_read(&q, &r));
    for value in [
        json!(0),
        json!(1),
        json!(257),
        json!(-1),
        json!(2.5),
        Value::Null,
        json!(false),
        json!("20"),
    ] {
        let mut bad = r.clone();
        bad["items"][0]["paintCount"] = value;
        assert!(!public_read(&q, &bad));
    }
    r["items"][0]["closed"] = json!(true);
    assert!(!public_read(&q, &r));
    r["items"][0].as_object_mut().unwrap().remove("paintCount");
    assert!(
        public_read(&q, &r),
        "ordinary closed paths keep their old response shape"
    );
}

#[test]
fn a_hit_test_listing_and_geometry_carry_bounds_and_role_without_preview() {
    let view = json!({"kind":"subselect","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"limit":8,"hitTest":true});
    let q = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":view});
    let r = json!({"kind":"subselect","atRevision":7,"documentSnapshotId":"native-object:document:7","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"total":1,"nextCursor":null,"preview":false,"hitTest":true,
        "items":[{"stableTarget":target(),"closed":true,"count":4,"matrix":[1,0,0,1,0,0],"bounds":[0.0,0.0,10.0,10.0],"role":"fill"}]});
    assert!(view_request(&q));
    assert!(public_read(&q, &r), "a listed linked fill is legal in a hit-test listing");
    let mut owned = r.clone();
    owned["items"][0]["owner"] = json!("ribbon-a");
    assert!(public_read(&q, &owned), "a linked fill names its ribbon");
    let mut stray = owned.clone();
    stray["items"][0]["role"] = json!("plain");
    assert!(!public_read(&q, &stray), "only a fill has an owner");
    for (key, value) in [("role", json!("ink")), ("bounds", json!([0, 0, 1])), ("paintCount", json!(4))] {
        let mut bad = r.clone();
        bad["items"][0][key] = value;
        assert!(!public_read(&q, &bad), "{key}");
    }
    let mut plain = r.clone();
    plain.as_object_mut().unwrap().remove("hitTest");
    assert!(!public_read(&q, &plain), "the listing must say it is a hit-test listing");
    let mut with_preview = q.clone();
    with_preview["view"]["preview"] = json!({"stableTarget":target(),"intent":{"kind":"move","indices":[0],"delta":[1,2]}});
    assert!(!view_request(&with_preview));
    let mut geometry = query();
    geometry["subselect"]["hitTest"] = json!(true);
    assert!(request(&geometry));
    let mut answer = page();
    answer["subselect"]["hitTest"] = json!(true);
    assert!(public_read(&geometry, &answer));
    assert!(!public_read(&geometry, &page()), "hit-test geometry is marked");
    let mut centre = geometry.clone();
    centre["subselect"]["center"] = json!(true);
    assert!(request(&centre), "a sculpt reads a ribbon's centreline");
    let mut widths = answer.clone();
    widths["subselect"]["center"] = json!(true);
    widths["subselect"]["widths"] = json!(vec![2.5; widths["subselect"]["segments"].as_array().unwrap().len()]);
    assert!(public_read(&centre, &widths));
    assert!(!public_read(&centre, &answer), "a centreline page is marked and carries its widths");
    widths["subselect"]["widths"].as_array_mut().unwrap().pop();
    assert!(!public_read(&centre, &widths), "one width per anchor");
    let mut no_hit = query();
    no_hit["subselect"]["center"] = json!(true);
    assert!(!request(&no_hit), "only on a hit-test read");
    geometry["subselect"]["paint"] = json!(true);
    assert!(!request(&geometry), "never with the paint mode");
}
