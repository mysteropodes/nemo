//! Original workspace capture through public native dispatch and sealed history.
//! This explicit intent defines no UI capture schedule or installed acceptance.
use crate::history::NativeObjectHistory;
use crate::request_receipts::{DispatchErrorCode as Error, OpacityRequest};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn owner(extra: Value) -> NativeObjectHistory {
    let mut project = json!({"canvasW":100,"canvasH":80,"fps":24});
    project
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let wire = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":project,"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]}});
    NativeObjectHistory::from_project_file("public-workspace", wire.to_string().as_bytes()).unwrap()
}
fn pin(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let p = pin(o);
    o.serialize_project_file(p.content_revision(), p.snapshot_id())
        .unwrap()
}
fn captured() -> Value {
    // Six fields always supplied by original capture(), motionColumns optional.
    json!({"layerPanelWidth":null,"toolsPanelWidth":"","propsPanelWidth":"280px",
        "propsPanelCollapsed":false,"timelineHeight":"19.5vh",
        "panelOrder":["transform","effects","transform",""]})
}
fn payload(workspace: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"command":"object.project.workspace.edit",
        "action":{"kind":"capture","workspace":workspace}}))
    .unwrap()
}
fn command(o: &NativeObjectHistory, id: &str, workspace: Value) -> OpacityRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":"command.document.object.project.workspace.edit",
        "payload":{"command":"object.project.workspace.edit",
            "action":{"kind":"capture","workspace":workspace}}}),
    )
    .unwrap()
}
fn dispatch(o: &mut NativeObjectHistory, request: OpacityRequest) -> Value {
    let response = o
        .dispatch_object_command(&request)
        .unwrap()
        .expect("registered workspace edit");
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    serde_json::to_value(response).unwrap()
}
fn capture(o: &mut NativeObjectHistory, id: &str, workspace: Value) -> Value {
    let request = command(o, id, workspace);
    dispatch(o, request)
}
fn request(p: &ObjectSnapshot, id: &str) -> OpacityRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":id,"instanceId":p.instance_id(),
        "documentId":p.document_id(),"operation":"query.document.object.project.workspace.read",
        "payload":{"atRevision":p.content_revision(),"documentSnapshotId":p.snapshot_id(),
            "contextId":"scene-root","selector":{"kind":"record"}}}),
    )
    .unwrap()
}
fn read(p: &ObjectSnapshot, request: OpacityRequest) -> Value {
    let response = p.dispatch(request).unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    serde_json::to_value(response).unwrap()
}

#[test]
fn capture_is_one_checkpoint_and_preserves_comments_allocation_metadata_old_pin_and_fresh_reopen() {
    let mut o = owner(
        json!({"workspace":{"panelOrder":null,"motionColumns":"old"},
        "comments":[{"id":"c1","authorId":"author","authorName":"Writer",
            "authorColor":"#AbC","text":"Keep\n日本語","createdAt":123.5,"resolved":false}],
        "customBrushPresets":{"brush":{"label":"Preset","nibSize":0.5}},
        "guides":{"h":[-0.25]},"perspectiveMode":"custom","perspectiveDensity":-3.5,
        "shadowPalette":[{"id":"same","color":"#AbC"},{"id":"same","color":"#abc"}]}),
    );
    let old = pin(&o);
    let before = saved(&o);
    let comment_ptr = old
        .project()
        .unwrap()
        .project()
        .annotations()
        .1
        .unwrap()
        .as_ptr();
    let q = command(&o, "capture", captured());
    let response = dispatch(&mut o, q.clone());
    assert_eq!(
        response["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(o.content_revision(), 1);
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(dispatch(&mut o, q), response);
    assert_eq!(o.history_depths(), (1, 0));
    let current = pin(&o);
    assert_eq!(
        current
            .project()
            .unwrap()
            .project()
            .annotations()
            .1
            .unwrap()
            .as_ptr(),
        comment_ptr
    );
    let changed = saved(&o);
    let mut next: Value = serde_json::from_slice(&changed).unwrap();
    let initial: Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(next["project"]["workspace"], captured());
    next["project"]["workspace"] = initial["project"]["workspace"].clone();
    assert_eq!(next, initial);
    assert_eq!(
        read(&old, request(&old, "old-workspace-pin"))["result"]["result"]["workspace"],
        json!({"panelOrder":null,"motionColumns":"old"})
    );
    o.undo(1).unwrap();
    assert_eq!(saved(&o), before);
    o.redo(2).unwrap();
    assert_eq!(saved(&o), changed);
    let fresh = NativeObjectHistory::from_project_file("workspace-reopen", &changed).unwrap();
    assert_eq!(saved(&fresh), changed);
}
#[test]
fn identical_capture_preserves_redo_and_optional_motion_capture_distinguishes_missing_null_and_empty(
) {
    let mut o = owner(json!({"workspace":captured()}));
    let initial = saved(&o);
    let mut null_motion = captured();
    null_motion["motionColumns"] = Value::Null;
    capture(&mut o, "null-motion", null_motion.clone());
    let changed = saved(&o);
    o.undo(1).unwrap();
    assert_eq!(saved(&o), initial);
    assert_eq!(
        capture(&mut o, "noop", captured())["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(o.content_revision(), 2);
    assert_eq!(o.history_depths(), (0, 1));
    o.redo(2).unwrap();
    assert_eq!(saved(&o), changed);
    let mut empty_motion = captured();
    empty_motion["motionColumns"] = json!("");
    capture(&mut o, "empty-motion", empty_motion.clone());
    assert_eq!(
        read(&pin(&o), request(&pin(&o), "empty"))["result"]["result"]["workspace"],
        empty_motion
    );
    capture(&mut o, "missing-motion", captured());
    assert!(
        read(&pin(&o), request(&pin(&o), "missing"))["result"]["result"]["workspace"]
            .get("motionColumns")
            .is_none()
    );
}
#[test]
fn invalid_capture_shapes_and_raw_duplicate_fields_are_atomic() {
    let mut o = owner(json!({}));
    let before = saved(&o);
    let mut invalid = vec![Value::Null, json!([]), json!({})];
    for field in [
        "layerPanelWidth",
        "toolsPanelWidth",
        "propsPanelWidth",
        "propsPanelCollapsed",
        "timelineHeight",
        "panelOrder",
    ] {
        let mut missing = captured();
        missing.as_object_mut().unwrap().remove(field);
        invalid.push(missing);
    }
    for (field, value) in [
        ("propsPanelCollapsed", Value::Null),
        ("layerPanelWidth", json!(12)),
        ("panelOrder", json!(["one", 1])),
        ("motionColumns", json!([])),
        ("toolbarOrder", json!([])),
        ("railMode", json!("wide")),
        ("viewZoom", json!(2)),
        ("comments", json!([])),
    ] {
        let mut workspace = captured();
        workspace[field] = value;
        invalid.push(workspace);
    }
    for (index, workspace) in invalid.into_iter().enumerate() {
        assert_eq!(
            capture(&mut o, &format!("bad-{index}"), workspace)["error"]["code"],
            "invalid_request"
        );
        assert_eq!(saved(&o), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
    let raw = String::from_utf8(payload(captured())).unwrap();
    let duplicate = raw.replace(
        "\"propsPanelCollapsed\":false",
        "\"propsPanelCollapsed\":false,\"propsPanelCollapsed\":true",
    );
    assert_ne!(raw, duplicate);
    assert_eq!(
        o.prepare_workspace_capture_json(duplicate.as_bytes())
            .unwrap_err(),
        Error::InvalidRequest
    );
    for raw in [
        r#"[]"#,
        r#"{"command":"object.project.workspace.edit","action":[]}"#,
        r#"{"command":"object.project.workspace.edit","action":{"kind":"replace","workspace":{}}}"#,
        r#"{"command":"object.project.workspace.edit","command":"object.project.workspace.edit","action":{"kind":"capture","workspace":{}}}"#,
    ] {
        assert_eq!(
            o.prepare_workspace_capture_json(raw.as_bytes())
                .unwrap_err(),
            Error::InvalidRequest
        );
    }
    assert_eq!(
        o.prepare_workspace_capture_json(&vec![b' '; 4097])
            .unwrap_err(),
        Error::InvalidRequest
    );
    assert_eq!(saved(&o), before);
}
#[test]
fn stale_and_foreign_prepared_captures_and_public_envelopes_cannot_publish() {
    let mut o = owner(json!({}));
    let before = saved(&o);
    let candidate = o
        .prepare_workspace_capture_json(&payload(captured()))
        .unwrap();
    let mut foreign = NativeObjectHistory::from_project_file("foreign-workspace", &before).unwrap();
    assert_eq!(
        foreign.commit_project_settings(candidate.clone()),
        Err(Error::WrongInstance)
    );
    let mut other_doc =
        NativeObjectHistory::from_project_file("public-workspace", &before).unwrap();
    assert_eq!(
        other_doc.commit_project_settings(candidate.clone()),
        Err(Error::WrongDocument)
    );
    assert_eq!(saved(&foreign), before);
    assert_eq!(saved(&other_doc), before);
    for (field, value, code) in [
        ("instanceId", json!("foreign"), "wrong_instance"),
        ("documentId", json!("foreign"), "wrong_document"),
        ("expectedRevision", json!(1), "stale_revision"),
    ] {
        let mut raw = serde_json::to_value(command(&o, field, captured())).unwrap();
        raw[field] = value;
        assert_eq!(
            dispatch(&mut o, serde_json::from_value(raw).unwrap())["error"]["code"],
            code
        );
    }
    let mut cancelled = command(&o, "cancelled", captured());
    cancelled.cancelled_before_dispatch = true;
    assert_eq!(
        dispatch(&mut o, cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    capture(&mut o, "advance", captured());
    let changed = saved(&o);
    assert_eq!(
        o.commit_project_settings(candidate),
        Err(Error::StaleRevision)
    );
    assert_eq!(saved(&o), changed);
    assert_eq!(o.history_depths(), (1, 0));
}

#[path = "object_workspace_read.rs"]
mod read_tests;
#[test]
fn bounded_capture_cannot_publish_a_candidate_the_actual_project_encoder_cannot_save() {
    const LIMIT: usize = 32 * 1024 * 1024;
    const TARGET: usize = LIMIT - 64;
    let colors = vec!["x".repeat(1024); (LIMIT - 4096) / 1027];
    let source = owner(json!({"palettes":[{"id":"filler","name":"Filler","colors":colors}]}));
    let mut wire: Value = serde_json::from_slice(&saved(&source)).unwrap();
    let mut remaining = TARGET
        .checked_sub(serde_json::to_vec(&wire).unwrap().len())
        .unwrap();
    let colors = wire["project"]["palettes"][0]["colors"]
        .as_array_mut()
        .unwrap();
    while remaining > 0 {
        if remaining < 3 {
            let last = colors.last_mut().unwrap();
            let value = last.as_str().unwrap();
            *last = json!(&value[..value.len() - (3 - remaining)]);
            remaining = 3;
        }
        let bytes = (remaining - 3).min(1024);
        colors.push(json!("x".repeat(bytes)));
        remaining -= bytes + 3;
    }
    let encoded = serde_json::to_vec(&wire).unwrap();
    assert_eq!(encoded.len(), TARGET);
    let o = NativeObjectHistory::from_project_file("workspace-budget", &encoded).unwrap();
    let before = saved(&o);
    assert_eq!(before.len(), TARGET);
    let bytes = payload(captured());
    assert!(bytes.len() < 4096);
    assert_eq!(
        o.prepare_workspace_capture_json(&bytes).unwrap_err(),
        Error::Unavailable
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.content_revision(), 0);
    assert_eq!(o.history_depths(), (0, 0));
}
