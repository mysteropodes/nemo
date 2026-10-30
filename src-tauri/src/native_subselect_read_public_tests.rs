//! Actual presented host and public contract for the subselect view, preview and command.
use super::super::tests::{output, selector, snapshot, viewport};
use super::tests::{host_info, host_read, host_saved, presented_project};
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{compositor::Compositor, desktop_viewport::ViewportStatus};
use serde_json::{json, Value};
fn project(parametric: bool, held: bool) -> Value {
    let mut p = presented_project();
    p["assets"] = json!([]);
    p["assetInventory"] = json!({"assetIds":[],"placements":[]});
    p["drawing"].as_object_mut().unwrap().remove("items");
    p["drawing"].as_object_mut().unwrap().remove("imageMeshes");
    p["drawing"]["objects"]=json!((0..2).map(|frame|json!({"schemaVersion":1,"family":"editable-vector-path","target":{"contextId":"scene-root","layerUid":"L0","strokeId":format!("pixels-{frame}"),"frameScope":{"kind":"authored","frame":frame}},
        "geometry":{"kind":"cubic-path","coordinateSpace":"document-world","handleSpace":"relative","closed":true,"segments":([[4,4],[28,4],[28,28],[4,28]].map(|[x,y]|json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})))},
        "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1},"vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"round","strokeJoin":"round","opacity":1}})).collect::<Vec<_>>());
    if parametric {
        for record in p["drawing"]["objects"].as_array_mut().unwrap() {
            record["vectorExt"] = json!({"paramShape":{"kind":"rect","box":{"x1":4,"y1":4,"x2":28,"y2":28},"tl":0,"tr":0,"br":0,"bl":0}});
        }
    }
    if held {
        p["drawing"]["objects"].as_array_mut().unwrap().truncate(1);
        p["drawing"]["compositionOrder"]["contexts"][0]["scopes"]
            .as_array_mut()
            .unwrap()
            .truncate(1);
        p["drawing"]["layers"][0]["timing"]["keys"] = json!([{"frame":0}]);
    }
    p
}
#[test]
fn subselect_presented_host_pages_preview_commit_and_stale_view() {
    for parametric in [false, true] {
        for held in [false, true] {
            check_presented_host(parametric, held);
        }
    }
}
fn check_presented_host(parametric: bool, held: bool) {
    let mut owner = NativeObjectHost::from_project_file(
        "subselect-host".into(),
        project(parametric, held).to_string().as_bytes(),
    )
    .unwrap();
    let info = host_info(&owner);
    let saved = host_saved(&owner);
    let mut gpu = Compositor::new().expect("Subselect host requires headless Metal");
    let (mut surface, trace) = viewport(&gpu, 64);
    trace.borrow_mut().pixel = Some([255, 0, 0, 255]);
    let frame = u32::from(held);
    let snap = snapshot(&owner);
    let shown = owner
        .present_object_preview(&selector(&snap, frame), output(), &mut gpu, &mut surface)
        .unwrap();
    assert_eq!(shown.status(), &ViewportStatus::Presented);
    assert!(owner.preview.reference_cache.is_none());
    assert!(owner.preview.presented_vectors.is_some());
    let query = json!({"apiVersion":2,"requestId":"subselect-view","instanceId":info["instanceId"],"documentId":info["documentId"],"operation":"query.document.object","payload":{"atRevision":info["contentRevision"],"documentSnapshotId":info["documentSnapshotId"],"view":{"kind":"subselect","contextId":"scene-root","layerUid":"L0","frame":frame,"workId":format!("native-work-{}",shown.identity().work_id().value()),"viewGeneration":shown.identity().view_generation().value(),"cursor":0,"limit":8}}});
    let first = host_read(&mut owner, &query);
    assert_eq!(first["ok"], true, "{first}");
    let item = &first["result"]["items"][0];
    assert_eq!(item["count"], 4);
    assert_eq!(first["result"]["preview"], false);
    // #2362 hit-test listing: the same presented records with their bounds and role, never their editability.
    let mut hit = query.clone();
    hit["requestId"] = json!("subselect-hit");
    hit["payload"]["view"]["hitTest"] = json!(true);
    let listed = host_read(&mut owner, &hit);
    assert_eq!(listed["ok"], true, "{listed}");
    assert_eq!(listed["result"]["hitTest"], true);
    let hit_item = &listed["result"]["items"][0];
    assert_eq!((hit_item["count"].as_u64(), hit_item["closed"].as_bool()), (Some(4), Some(true)));
    assert_eq!(hit_item["role"], if parametric { "shape" } else { "plain" });
    assert_eq!(hit_item["bounds"], json!([4.0, 4.0, 28.0, 28.0]));
    assert!(item.get("role").is_none() && item.get("bounds").is_none(), "the editable listing is unchanged");
    hit["payload"]["view"]["preview"] = json!({"stableTarget":item["stableTarget"],"intent":{"kind":"move","indices":[0],"delta":[1,0]}});
    assert_eq!(host_read(&mut owner, &hit)["ok"], false, "a hit-test listing has no preview");
    let intent = json!({"kind":"move","indices":[0,1,2,3],"delta":[24,0]});
    let target = item["stableTarget"].clone();
    assert_eq!(target["frameScope"]["frame"], 0);
    let mut preview = query.clone();
    preview["requestId"] = json!("subselect-preview");
    preview["payload"]["view"]["preview"] = json!({"stableTarget":target,"intent":intent});
    let r = host_read(&mut owner, &preview);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["preview"], true);
    assert_eq!(host_saved(&owner), saved);
    assert_eq!(owner.history.history_depths(), (0, 0));
    let mut foreign = query.clone();
    foreign["payload"]["view"]["workId"] = json!("native-work-9999");
    assert_eq!(host_read(&mut owner, &foreign)["ok"], false);
    let command = json!({"apiVersion":2,"requestId":"subselect-commit","instanceId":info["instanceId"],"documentId":info["documentId"],"expectedRevision":0,"operation":"command.document.object.subselect.edit","payload":{"command":"object.subselect.edit","stableTarget":target,"atFrame":frame,"intent":intent}});
    let committed = host_read(&mut owner, &command);
    assert_eq!(committed["ok"], true, "{committed}");
    assert_eq!(committed["result"]["historyEntriesAdded"], 1);
    assert_ne!(host_saved(&owner), saved);
    assert_eq!(host_read(&mut owner, &query)["ok"], false);
    assert_eq!(owner.content_revision(), 1);
    assert!(owner.preview.presented_vectors.is_none());
}
#[test]
fn the_hit_test_listing_flags_fill_paint_per_record() {
    // #2372: a filled closed record is hit inside, an unfilled closed outline and an open path only on their line.
    let mut doc = project(false, true);
    let stroke = json!({"kind":"solid","r":0,"g":0,"b":0,"a":1});
    let objects = doc["drawing"]["objects"].as_array_mut().unwrap();
    for (id, closed) in [("outline", true), ("open", false)] {
        let mut r = objects[0].clone();
        r["target"]["strokeId"] = json!(id);
        r["geometry"]["closed"] = json!(closed);
        r["fill"] = Value::Null;
        r["vector"]["stroke"] = stroke.clone();
        r["vector"]["hasRealStroke"] = json!(true);
        objects.push(r);
    }
    let elements = doc["drawing"]["compositionOrder"]["contexts"][0]["scopes"][0]["elements"].as_array_mut().unwrap();
    for (n, id) in ["outline", "open"].into_iter().enumerate() {
        elements.push(json!({"layerUid":"L0","strokeId":id,"naturalPosition":n + 1,"order":{}}));
    }
    let mut owner = NativeObjectHost::from_project_file("subselect-fill".into(), doc.to_string().as_bytes()).unwrap();
    let info = host_info(&owner);
    let mut gpu = Compositor::new().expect("Subselect host requires headless Metal");
    let (mut surface, trace) = viewport(&gpu, 64);
    trace.borrow_mut().pixel = Some([255, 0, 0, 255]);
    let snap = snapshot(&owner);
    let shown = owner.present_object_preview(&selector(&snap, 0), output(), &mut gpu, &mut surface).unwrap();
    assert_eq!(shown.status(), &ViewportStatus::Presented);
    let query = json!({"apiVersion":2,"requestId":"subselect-fill","instanceId":info["instanceId"],"documentId":info["documentId"],"operation":"query.document.object","payload":{"atRevision":info["contentRevision"],"documentSnapshotId":info["documentSnapshotId"],"view":{"kind":"subselect","contextId":"scene-root","layerUid":"L0","frame":0,"workId":format!("native-work-{}",shown.identity().work_id().value()),"viewGeneration":shown.identity().view_generation().value(),"cursor":0,"limit":8,"hitTest":true}}});
    let listed = host_read(&mut owner, &query);
    assert_eq!(listed["ok"], true, "{listed}");
    let flags: Vec<(String, bool, bool)> = listed["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| (i["stableTarget"]["strokeId"].as_str().unwrap().to_string(), i["closed"].as_bool().unwrap(), i["filled"].as_bool().unwrap()))
        .collect();
    assert_eq!(
        flags,
        [("pixels-0".to_string(), true, true), ("outline".to_string(), true, false), ("open".to_string(), false, false)]
    );
}
