//! Actual-owner oracles for project settings, attached to the existing history module.
use super::*;
use serde_json::{json, Value};
fn project_wire() -> Value {
    json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":29.97},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":120,"layers":[{"layerUid":"L"}],"objects":[]},
        "viewportReference":null,"assets":[]})
}
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::from_project_file("settings-test", project_wire().to_string().as_bytes())
        .unwrap()
}
fn wire(o: &NativeObjectHistory) -> Value {
    let snapshot = o.acquire_snapshot(o.content_revision()).unwrap();
    serde_json::from_slice(
        &o.serialize_project_file(o.content_revision(), snapshot.snapshot_id())
            .unwrap(),
    )
    .unwrap()
}
fn apply(o: &mut NativeObjectHistory, action: Value) -> Result<u64, DispatchErrorCode> {
    let bytes = json!({"command":"object.project.settings.edit","action":action}).to_string();
    let candidate = o.prepare_project_settings_json(bytes.as_bytes())?;
    o.commit_project_settings(candidate)
}
#[test]
fn settings_are_atomic_persistent_and_old_snapshot_is_immutable() {
    let mut o = owner();
    let pin = o.acquire_snapshot(0).unwrap();
    let before = o.serialize_project_file(0, pin.snapshot_id()).unwrap();
    assert_eq!(
        apply(
            &mut o,
            json!({"kind":"setCanvasSize","width":1920,"height":1080})
        )
        .unwrap(),
        1
    );
    assert_eq!(
        apply(
            &mut o,
            json!({"kind":"setCanvasBackground","color":"#AbCdEF80"})
        )
        .unwrap(),
        2
    );
    assert_eq!(
        apply(
            &mut o,
            json!({"kind":"setWorkArea","inFrame":2.5,"outFrame":45.25})
        )
        .unwrap(),
        3
    );
    let saved = wire(&o);
    assert_eq!(
        saved["project"],
        json!({"canvasW":1920,"canvasH":1080,"fps":29.97,"canvasBg":"#AbCdEF80","waIn":2.5,"waOut":45.25})
    );
    assert_eq!(saved["drawing"], project_wire()["drawing"]);
    assert_eq!(o.history_depths(), (3, 0));
    assert_eq!(
        o.serialize_project_file(0, pin.snapshot_id()).unwrap(),
        before
    );
    let reopened =
        NativeObjectHistory::from_project_file("reopened", saved.to_string().as_bytes()).unwrap();
    assert_eq!(wire(&reopened), saved);
    assert_eq!(o.undo(3).unwrap(), 4);
    assert!(wire(&o)["project"].get("waIn").is_none());
    assert_eq!(o.redo(4).unwrap(), 5);
    assert_eq!(wire(&o), saved);
}
#[test]
fn fps_command_clamps_without_retiming_and_storage_preserves_fractional_values() {
    let mut source = project_wire();
    source["project"]["fps"] = json!(0.5);
    let mut o =
        NativeObjectHistory::from_project_file("fps-test", source.to_string().as_bytes()).unwrap();
    assert_eq!(wire(&o)["project"]["fps"], json!(0.5));
    for (input, expected) in [(-5., 1.), (61., 60.), (29.97, 29.97)] {
        apply(&mut o, json!({"kind":"setFps","fps":input})).unwrap();
        assert_eq!(wire(&o)["project"]["fps"], json!(expected));
        assert_eq!(wire(&o)["drawing"], source["drawing"]);
    }
}
#[test]
fn no_op_preserves_revision_bytes_and_redo() {
    let mut o = owner();
    apply(&mut o, json!({"kind":"setFps","fps":30})).unwrap();
    o.undo(1).unwrap();
    let before = wire(&o);
    assert_eq!(
        apply(&mut o, json!({"kind":"setFps","fps":29.97})).unwrap(),
        2
    );
    assert_eq!(o.history_depths(), (0, 1));
    assert_eq!(wire(&o), before);
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(wire(&o)["project"]["fps"], json!(30.));
}
#[test]
fn invalid_settings_and_duplicate_raw_members_are_atomic() {
    let mut o = owner();
    let before = wire(&o);
    for action in [
        json!({"kind":"setCanvasSize","width":0,"height":180}),
        json!({"kind":"setCanvasBackground","color":"#ggg"}),
        json!({"kind":"setFps","fps":"24"}),
        json!({"kind":"setWorkArea","inFrame":0,"outFrame":null}),
        json!({"kind":"setTotalFrames","frames":5}),
    ] {
        assert_eq!(
            apply(&mut o, action),
            Err(DispatchErrorCode::InvalidRequest)
        );
        assert_eq!(wire(&o), before);
        assert_eq!(o.content_revision(), 0);
        assert_eq!(o.history_depths(), (0, 0));
    }
    for raw in [
        r#"{"command":"object.project.settings.edit","command":"object.project.settings.edit","action":{"kind":"setFps","fps":24}}"#,
        r#"{"command":"object.project.settings.edit","action":{"kind":"setFps","fps":24,"fps":30}}"#,
    ] {
        assert_eq!(
            o.prepare_project_settings_json(raw.as_bytes()).unwrap_err(),
            DispatchErrorCode::InvalidRequest
        );
    }
}
#[test]
fn stale_and_foreign_prepared_settings_cannot_publish() {
    let mut o = owner();
    let bytes =
        json!({"command":"object.project.settings.edit","action":{"kind":"setFps","fps":30}})
            .to_string();
    let stale = o.prepare_project_settings_json(bytes.as_bytes()).unwrap();
    let foreign = owner()
        .prepare_project_settings_json(bytes.as_bytes())
        .unwrap();
    apply(
        &mut o,
        json!({"kind":"setCanvasSize","width":640,"height":360}),
    )
    .unwrap();
    assert_eq!(
        o.commit_project_settings(stale),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(
        o.commit_project_settings(foreign),
        Err(DispatchErrorCode::WrongDocument)
    );
    assert_eq!(o.content_revision(), 1);
    assert_eq!(o.history_depths(), (1, 0));
}
