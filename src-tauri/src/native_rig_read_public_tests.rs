//! #2255: actual presented host and public contract for the rig presentation view.
use super::super::tests::{present, viewport};
use super::tests::{host_info, host_read, host_saved, presented_project};
use crate::native_dispatch::NativeObjectHost;
use native_engine::compositor::Compositor;
use serde_json::{json, Value};

fn query(
    info: &Value,
    work: String,
    generation: u64,
    part: &str,
    cursor: u64,
    limit: u64,
) -> Value {
    json!({"apiVersion":2,"requestId":format!("rig-read-{part}-{cursor}"),"instanceId":info["instanceId"],
        "documentId":info["documentId"],"operation":"query.document.object","payload":{
            "atRevision":info["contentRevision"],"documentSnapshotId":info["documentSnapshotId"],"view":{
                "kind":"rig","contextId":"scene-root","frame":0,"workId":work,"viewGeneration":generation,
                "layerUid":"L0","part":part,"bind":0,"vertex":0,"cursor":cursor,"limit":limit}}})
}
fn draw(owner: &mut NativeObjectHost, id: &str) {
    let info = host_info(owner);
    let reply = host_read(
        owner,
        &json!({"apiVersion":2,"requestId":id,"instanceId":info["instanceId"],
        "documentId":info["documentId"],"expectedRevision":info["contentRevision"],
        "operation":"command.document.object.rig.edit","payload":{"command":"object.rig.edit","layerUid":"L0",
            "action":{"kind":"drawBone","closed":false,"segments":[
                {"point":[4,6],"handleIn":[0,0],"handleOut":[8,0]},
                {"point":[20,6],"handleIn":[-8,0],"handleOut":[0,0]}]}}}),
    );
    assert_eq!(reply["ok"], true, "{reply}");
}

#[test]
fn gpu_rig_view_reads_only_through_the_actual_presentation() {
    let mut owner = NativeObjectHost::from_project_file(
        "rig-host".into(),
        presented_project().to_string().as_bytes(),
    )
    .unwrap();
    draw(&mut owner, "bone-a");
    let info = host_info(&owner);
    let mut gpu = Compositor::new().expect("The rig host read requires headless Metal");
    let (mut surface, _trace) = viewport(&gpu, 64);
    assert_eq!(
        host_read(
            &mut owner,
            &query(&info, "native-work-1".into(), 1, "bones", 0, 8)
        )["error"]["code"],
        "unavailable"
    );
    let shown = present(&mut owner, &mut gpu, &mut surface).unwrap();
    let work = format!("native-work-{}", shown.identity().work_id().value());
    let generation = shown.identity().view_generation().value();
    let saved = host_saved(&owner);
    let bones = host_read(
        &mut owner,
        &query(&info, work.clone(), generation, "bones", 0, 8),
    );
    assert_eq!(bones["ok"], true, "{bones}");
    let bone = &bones["result"]["items"][0];
    assert_eq!(
        (
            bone["boneId"].clone(),
            bone["count"].clone(),
            bone["closed"].clone()
        ),
        (json!("bone1"), json!(2), json!(false))
    );
    assert_eq!(bone["center"], json!([12.0, 6.0]));
    assert_eq!(bones["result"]["unavailable"], Value::Null);
    let vertices = host_read(
        &mut owner,
        &query(&info, work.clone(), generation, "vertices", 1, 16),
    );
    assert_eq!(vertices["result"]["total"], 2);
    assert_eq!(vertices["result"]["items"][0]["point"], json!([20, 6]));
    assert_eq!(
        vertices["result"]["items"][0]["handleIn"],
        json!([-8.0, 0.0])
    );
    // The read neither spends history nor changes the saved bytes; a foreign presentation is refused.
    assert_eq!(host_saved(&owner), saved);
    for bad in [
        query(&info, "native-work-9999".into(), generation, "bones", 0, 8),
        query(&info, work.clone(), generation + 1, "bones", 0, 8),
    ] {
        assert_eq!(
            host_read(&mut owner, &bad)["error"]["code"],
            "stale_revision"
        );
    }
    // A commit invalidates the presentation: the same query no longer reads until it is presented again.
    draw(&mut owner, "bone-b");
    let current = query(&host_info(&owner), work, generation, "bones", 0, 8);
    assert_ne!(host_read(&mut owner, &current)["ok"], json!(true));
}
