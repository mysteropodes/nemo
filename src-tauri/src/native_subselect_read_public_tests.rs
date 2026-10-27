//! Actual presented host and public contract for the subselect view, preview and command.
use super::super::tests::{present, viewport};
use super::tests::{host_info, host_read, host_saved, presented_project};
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{compositor::Compositor, desktop_viewport::ViewportStatus};
use serde_json::{json, Value};
fn project() -> Value {
    let mut p = presented_project();
    p["assets"] = json!([]);
    p["assetInventory"] = json!({"assetIds":[],"placements":[]});
    p["drawing"].as_object_mut().unwrap().remove("items");
    p["drawing"].as_object_mut().unwrap().remove("imageMeshes");
    p["drawing"]["objects"]=json!((0..2).map(|frame|json!({"schemaVersion":1,"family":"editable-vector-path","target":{"contextId":"scene-root","layerUid":"L0","strokeId":format!("pixels-{frame}"),"frameScope":{"kind":"authored","frame":frame}},
        "geometry":{"kind":"cubic-path","coordinateSpace":"document-world","handleSpace":"relative","closed":true,"segments":([[4,4],[28,4],[28,28],[4,28]].map(|[x,y]|json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})))},
        "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1},"vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"round","strokeJoin":"round","opacity":1}})).collect::<Vec<_>>());
    p
}
#[test]
fn subselect_presented_host_pages_preview_commit_and_stale_view() {
    let mut owner = NativeObjectHost::from_project_file(
        "subselect-host".into(),
        project().to_string().as_bytes(),
    )
    .unwrap();
    let info = host_info(&owner);
    let saved = host_saved(&owner);
    let mut gpu = Compositor::new().expect("Subselect host requires headless Metal");
    let (mut surface, trace) = viewport(&gpu, 64);
    trace.borrow_mut().pixel = Some([255, 0, 0, 255]);
    let shown = present(&mut owner, &mut gpu, &mut surface).unwrap();
    assert_eq!(shown.status(), &ViewportStatus::Presented);
    let query = json!({"apiVersion":2,"requestId":"subselect-view","instanceId":info["instanceId"],"documentId":info["documentId"],"operation":"query.document.object","payload":{"atRevision":info["contentRevision"],"documentSnapshotId":info["documentSnapshotId"],"view":{"kind":"subselect","contextId":"scene-root","layerUid":"L0","frame":0,"workId":format!("native-work-{}",shown.identity().work_id().value()),"viewGeneration":shown.identity().view_generation().value(),"cursor":0,"limit":8}}});
    let first = host_read(&mut owner, &query);
    assert_eq!(first["ok"], true, "{first}");
    let item = &first["result"]["items"][0];
    assert_eq!(item["count"], 4);
    assert_eq!(first["result"]["preview"], false);
    let intent = json!({"kind":"move","indices":[0,1,2,3],"delta":[24,0]});
    let target = item["stableTarget"].clone();
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
    let command = json!({"apiVersion":2,"requestId":"subselect-commit","instanceId":info["instanceId"],"documentId":info["documentId"],"expectedRevision":0,"operation":"command.document.object.subselect.edit","payload":{"command":"object.subselect.edit","stableTarget":target,"intent":intent}});
    let committed = host_read(&mut owner, &command);
    assert_eq!(committed["ok"], true, "{committed}");
    assert_eq!(committed["result"]["historyEntriesAdded"], 1);
    assert_ne!(host_saved(&owner), saved);
    assert_eq!(host_read(&mut owner, &query)["ok"], false);
    assert_eq!(owner.content_revision(), 1);
}
