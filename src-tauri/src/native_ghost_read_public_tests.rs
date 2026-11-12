//! #2358 D23: actual presented host and public contract for the onion / Ghost All view.
use super::super::tests::{present, viewport};
use super::tests::{host_info, host_read, host_saved, presented_project};
use crate::native_dispatch::NativeObjectHost;
use native_engine::compositor::Compositor;
use serde_json::{json, Value};

fn query(info: &Value, work: String, generation: u64, frame: u64, ghost: u64) -> Value {
    json!({"apiVersion":2,"requestId":format!("ghost-read-{ghost}"),"instanceId":info["instanceId"],
        "documentId":info["documentId"],"operation":"query.document.object","payload":{
            "atRevision":info["contentRevision"],"documentSnapshotId":info["documentSnapshotId"],"view":{
                "kind":"ghosts","contextId":"scene-root","frame":frame,"workId":work,"viewGeneration":generation,
                "layerUid":"L0","onion":true,"ghostAll":false,"from":0,"to":1,"previousOpacity":30,"nextOpacity":30,
                "mode":"tinted","ghost":ghost,"cursor":0}}})
}

#[test]
fn gpu_ghost_view_reads_only_through_the_actual_presentation() {
    let mut owner = NativeObjectHost::from_project_file(
        "ghost-host".into(),
        presented_project().to_string().as_bytes(),
    )
    .unwrap();
    let info = host_info(&owner);
    let mut gpu = Compositor::new().expect("The ghost host read requires headless Metal");
    let (mut surface, _trace) = viewport(&gpu, 64);
    let unpresented = query(&info, "native-work-1".into(), 1, 0, 0);
    assert_eq!(
        host_read(&mut owner, &unpresented)["error"]["code"],
        "unavailable"
    );
    let shown = present(&mut owner, &mut gpu, &mut surface).unwrap();
    let work = format!("native-work-{}", shown.identity().work_id().value());
    let generation = shown.identity().view_generation().value();
    let saved = host_saved(&owner);
    let depths = owner.history.history_depths();
    let first = host_read(&mut owner, &query(&info, work.clone(), generation, 0, 0));
    assert_eq!(first["ok"], true, "{first}");
    let result = &first["result"];
    assert_eq!(
        (
            result["ghost"]["kind"].clone(),
            result["ghost"]["frame"].clone(),
            result["ghost"]["count"].clone()
        ),
        (json!("next"), json!(1), json!(1))
    );
    assert_eq!(result["next"], Value::Null);
    // Only the playhead frame the presentation names is admitted; the read neither spends history nor changes the saved bytes.
    for bad in [
        query(&info, "native-work-9999".into(), generation, 0, 0),
        query(&info, work.clone(), generation + 1, 0, 0),
        query(&info, work.clone(), generation, 1, 0),
    ] {
        assert_eq!(
            host_read(&mut owner, &bad)["error"]["code"],
            "stale_revision"
        );
    }
    assert_eq!(
        (host_saved(&owner), owner.history.history_depths()),
        (saved, depths)
    );
}
