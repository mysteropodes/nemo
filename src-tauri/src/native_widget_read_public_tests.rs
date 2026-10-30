//! #2255: actual presented host and public contract for the widget presentation view.
use super::super::tests::{present, viewport};
use super::tests::{host_info, host_read, host_saved, presented_project};
use crate::native_dispatch::NativeObjectHost;
use native_engine::compositor::Compositor;
use serde_json::{json, Value};

fn query(info: &Value, work: String, generation: u64, cursor: u64) -> Value {
    json!({"apiVersion":2,"requestId":format!("widget-read-{cursor}"),"instanceId":info["instanceId"],
        "documentId":info["documentId"],"operation":"query.document.object","payload":{
            "atRevision":info["contentRevision"],"documentSnapshotId":info["documentSnapshotId"],"view":{
                "kind":"widget","contextId":"scene-root","frame":0,"workId":work,"viewGeneration":generation,
                "cursor":cursor,"limit":1}}})
}
fn create(owner: &mut NativeObjectHost, id: &str, kind: &str) {
    let info = host_info(owner);
    let reply = host_read(
        owner,
        &json!({"apiVersion":2,"requestId":id,"instanceId":info["instanceId"],
        "documentId":info["documentId"],"expectedRevision":info["contentRevision"],
        "operation":"command.document.object.widget.create","payload":{"command":"object.widget.create",
            "kind":kind,"name":id,"pos":[32,20]}}),
    );
    assert_eq!(reply["ok"], true, "{reply}");
}

#[test]
fn gpu_widget_view_reads_only_through_the_actual_presentation() {
    let mut owner = NativeObjectHost::from_project_file(
        "widget-host".into(),
        presented_project().to_string().as_bytes(),
    )
    .unwrap();
    create(&mut owner, "stick", "joystick");
    create(&mut owner, "fader", "slider");
    let info = host_info(&owner);
    let mut gpu = Compositor::new().expect("The widget host read requires headless Metal");
    let (mut surface, _trace) = viewport(&gpu, 64);
    let unpresented = query(&info, "native-work-1".into(), 1, 0);
    assert_eq!(
        host_read(&mut owner, &unpresented)["error"]["code"],
        "unavailable"
    );
    let shown = present(&mut owner, &mut gpu, &mut surface).unwrap();
    let work = format!("native-work-{}", shown.identity().work_id().value());
    let generation = shown.identity().view_generation().value();
    let saved = host_saved(&owner);
    let first = host_read(&mut owner, &query(&info, work.clone(), generation, 0));
    assert_eq!(first["ok"], true, "{first}");
    let result = &first["result"];
    assert_eq!(
        (result["total"].clone(), result["nextCursor"].clone()),
        (json!(2), json!(1))
    );
    let stick = &result["items"][0];
    assert_eq!(
        (stick["kind"].clone(), stick["unavailable"].clone()),
        (json!("joystick"), Value::Null)
    );
    assert_eq!(stick["center"], json!([32.0, 20.0]));
    assert_eq!(stick["puck"], json!([0.0, 0.0]));
    let second = host_read(&mut owner, &query(&info, work.clone(), generation, 1));
    assert_eq!(second["result"]["items"][0]["kind"], "slider");
    assert_eq!(second["result"]["nextCursor"], Value::Null);
    // The read neither spends history nor changes the saved bytes; a foreign presentation is refused.
    assert_eq!(
        (host_saved(&owner), owner.history.history_depths()),
        (saved, (2, 0))
    );
    for bad in [
        query(&info, "native-work-9999".into(), generation, 0),
        query(&info, work.clone(), generation + 1, 0),
    ] {
        assert_eq!(
            host_read(&mut owner, &bad)["error"]["code"],
            "stale_revision"
        );
    }
    // A commit invalidates the presentation: the same query no longer reads until it is presented again.
    create(&mut owner, "third", "slider");
    let current = query(&host_info(&owner), work, generation, 0);
    assert_ne!(host_read(&mut owner, &current)["ok"], json!(true));
}
