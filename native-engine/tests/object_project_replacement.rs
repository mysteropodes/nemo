//! Immutable project preflight and consuming replacement in the original history.
use crate::history::NativeObjectHistory;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use serde_json::{json, Value};

fn file(width: u32) -> Vec<u8> {
    serde_json::to_vec(
        &json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":width,"canvasH":48,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":2,"layers":[{"layerUid":"original-layer"}],"objects":[]}}),
    )
    .unwrap()
}
fn pin(owner: &NativeObjectHistory) -> String {
    owner
        .acquire_snapshot(owner.content_revision())
        .unwrap()
        .snapshot_id()
        .into()
}
fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    owner
        .serialize_project_file(owner.content_revision(), &pin(owner))
        .unwrap()
}
fn attach(owner: &mut NativeObjectHistory) {
    let hex="89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000d49444154789c636060f80f060014f405fbb956037d0000000049454e44ae426082";
    let png: Vec<_> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect();
    let request = OpacityRequest::history_stage(
        "attach",
        owner.instance_id(),
        owner.document_id(),
        Some(owner.content_revision()),
        "command.document.reference.attach",
        json!({"id":"reference","name":"original.png"}),
    );
    assert!(owner
        .dispatch_reference_command(&request, Some(&png))
        .unwrap()
        .is_ok());
}

#[test]
fn cpu_project_replacement_preserves_bytes_and_old_reads_with_fresh_history() {
    let mut owner =
        NativeObjectHistory::from_project_file("replacement-engine", &file(64)).unwrap();
    attach(&mut owner);
    let old = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let saved = bytes(&owner);
    let info = owner.project_file_info().unwrap();
    let address = &owner as *const _;
    let prepared = owner
        .prepare_project_replacement(old.snapshot_id(), &saved)
        .unwrap();
    assert_eq!(owner.document_id(), old.document_id());
    assert_eq!(owner.history_depths(), (1, 0));
    owner.commit_project_replacement(prepared).unwrap();
    assert_eq!(&owner as *const _, address);
    assert_ne!(owner.document_id(), old.document_id());
    assert_eq!(owner.instance_id(), old.instance_id());
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(owner.history_depths(), (0, 0));
    assert_eq!(bytes(&owner), saved);
    assert_eq!(
        owner.project_file_info().unwrap()["documentDigest"],
        info["documentDigest"]
    );
    assert_eq!(old.serialize_project_file().unwrap(), saved);
    assert!(owner.undo(0).is_err());
}

#[test]
fn cpu_invalid_project_preflight_and_foreign_or_stale_origin_leave_owner_unchanged() {
    let mut owner =
        NativeObjectHistory::from_project_file("replacement-engine", &file(64)).unwrap();
    let before = bytes(&owner);
    let before_info = owner.project_file_info().unwrap();
    for invalid in [
        b"{".to_vec(),
        file(0),
        file(64).into_iter().take(12).collect(),
        String::from_utf8(file(64))
            .unwrap()
            .replacen(
                "\"formatVersion\":1",
                "\"formatVersion\":1,\"formatVersion\":1",
                1,
            )
            .into_bytes(),
    ] {
        assert!(owner
            .prepare_project_replacement(&pin(&owner), &invalid)
            .is_err());
        assert_eq!(bytes(&owner), before);
        assert_eq!(owner.project_file_info().unwrap(), before_info);
    }
    assert!(matches!(
        owner.prepare_project_replacement("foreign", &file(80)),
        Err(DispatchErrorCode::StaleRevision)
    ));
    let stale = owner
        .prepare_project_replacement(&pin(&owner), &file(80))
        .unwrap();
    attach(&mut owner);
    let current = bytes(&owner);
    assert!(matches!(
        owner.commit_project_replacement(stale),
        Err(DispatchErrorCode::StaleRevision)
    ));
    assert_eq!(bytes(&owner), current);
    let token = owner
        .prepare_project_replacement(&pin(&owner), &file(80))
        .unwrap();
    let mut foreign = NativeObjectHistory::from_project_file("other-instance", &file(64)).unwrap();
    assert!(matches!(
        foreign.commit_project_replacement(token),
        Err(DispatchErrorCode::WrongInstance)
    ));
    let token = owner
        .prepare_project_replacement(&pin(&owner), &file(80))
        .unwrap();
    let mut other =
        NativeObjectHistory::from_project_file("replacement-engine", &file(64)).unwrap();
    assert!(matches!(
        other.commit_project_replacement(token),
        Err(DispatchErrorCode::WrongDocument)
    ));
    let one = owner
        .prepare_project_replacement(&pin(&owner), &file(80))
        .unwrap();
    let two = owner
        .prepare_project_replacement(&pin(&owner), &file(96))
        .unwrap();
    owner.commit_project_replacement(one).unwrap();
    assert!(matches!(
        owner.commit_project_replacement(two),
        Err(DispatchErrorCode::WrongDocument)
    ));
    let result: Value = serde_json::from_slice(&bytes(&owner)).unwrap();
    assert_eq!(result["project"]["canvasW"], 80);
}

fn current(owner: &NativeObjectHistory) -> crate::revision::ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn save(owner: &NativeObjectHistory, revision: u64) -> Vec<u8> {
    let snapshot = owner.acquire_snapshot(revision).unwrap();
    owner
        .serialize_project_file(revision, snapshot.snapshot_id())
        .unwrap()
}

#[test]
fn blank_project_preserves_explicit_settings_and_five_second_baseline_after_reopen() {
    // Frozen desktop rule: max(1, round(fps * 5)); positive halfway rounds up.
    for (fps, frames) in [
        (24., 120),
        (29.97, 150),
        (60., 300),
        (0.1, 1),
        (0.49, 2),
        (0.5, 3),
        (f64::MIN_POSITIVE, 1),
        (u32::MAX as f64 / 5., u32::MAX),
    ] {
        let mut original = NativeObjectHistory::new_blank_project("new", 640, 360, fps).unwrap();
        let initial = current(&original);
        assert_eq!(
            (original.content_revision(), original.history_depths()),
            (0, (0, 0))
        );
        assert_eq!(original.undo(0), Err(DispatchErrorCode::Unavailable));
        assert_eq!(original.redo(0), Err(DispatchErrorCode::Unavailable));
        let bytes = save(&original, 0);
        let expected = json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":640,"canvasH":360,"fps":fps,
                "canvasBg":"#ffffff","waIn":0.0,"waOut":(frames - 1) as f64},
            "drawing":{"format":"nemo.native-object-document","formatVersion":2,
                "totalFrames":frames,"layers":[{"layerUid":"layer-1","name":"Layer 1","visible":true,
                    "locked":false,"color":"#5FA875","parentLayerUid":null,
                    "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
                        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],
                "objects":[],"compositionOrder":{"format":"nemo.native-object-order-plan",
                    "formatVersion":1,"documentFormatVersion":2,"contexts":[{"contextId":"scene-root",
                        "layers":[{"layerUid":"layer-1","naturalPosition":0,"order":{}}],
                        "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[]}]}],
                    "migration":"explicit-plan-required","referencePolicy":"unresolved"}},
            "viewportReference":null,"assets":[]});
        assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), expected);
        let info = original.project_file_info().unwrap();
        assert_eq!(
            (info["totalFrames"].as_u64(), info["fps"].as_f64()),
            (Some(frames as u64), Some(fps))
        );
        assert_eq!(
            info["layers"],
            json!([{"layerUid":"layer-1","name":"Layer 1","visible":true,
            "locked":false,"color":"#5FA875","parentLayerUid":null}]),
            "file-info publishes native layer defaults; timing keeps its own read"
        );
        let reopened = NativeObjectHistory::from_project_file("new", &bytes).unwrap();
        let another = NativeObjectHistory::new_blank_project("new", 640, 360, fps).unwrap();
        assert_ne!(reopened.document_id(), original.document_id());
        assert_ne!(another.document_id(), original.document_id());
        assert_eq!(
            (reopened.content_revision(), reopened.history_depths()),
            (0, (0, 0))
        );
        assert_eq!(save(&reopened, 0), bytes);
        assert_eq!(save(&another, 0), bytes);
        drop(original);
        assert_eq!(initial.serialize_project_file().unwrap(), bytes);
    }
}

#[test]
fn blank_project_refuses_invalid_intent_and_frame_overflow_without_touching_existing_owner() {
    let existing = NativeObjectHistory::from_project_file("existing", &file(64)).unwrap();
    let before = save(&existing, 0);
    for (width, height, fps) in [
        (0, 360, 24.),
        (640, 0, 24.),
        (640, 360, 0.),
        (640, 360, -1.),
        (640, 360, f64::NAN),
        (640, 360, f64::INFINITY),
        (640, 360, f64::NEG_INFINITY),
        (640, 360, f64::MAX),
        (640, 360, (u32::MAX as f64 + 1.) / 5.),
    ] {
        assert!(matches!(
            NativeObjectHistory::new_blank_project("existing", width, height, fps),
            Err(DispatchErrorCode::InvalidRequest)
        ));
    }
    assert!(matches!(
        NativeObjectHistory::new_blank_project("", 640, 360, 24.),
        Err(DispatchErrorCode::InvalidRequest)
    ));
    assert_eq!(
        (existing.content_revision(), existing.history_depths()),
        (0, (0, 0))
    );
    assert_eq!(save(&existing, 0), before);
}

#[test]
fn blank_project_uses_the_existing_visibility_history_and_retained_snapshot_consumers() {
    let mut original =
        NativeObjectHistory::new_blank_project("new-history", 320, 180, 24.).unwrap();
    let before = save(&original, 0);
    let request = serde_json::from_value::<crate::commands::OpacityRequest>(json!({
        "apiVersion":2,"requestId":"hide-new","instanceId":original.instance_id(),
        "documentId":original.document_id(),"expectedRevision":0,
        "operation":"command.document.object.layer.visibility.set",
        "payload":{"command":"object.layer.visibility.set","layerUid":"layer-1","visible":false}
    }))
    .unwrap();
    assert!(
        original
            .try_dispatch_object_command(&request)
            .unwrap()
            .unwrap()
            .ok
    );
    let after = save(&original, 1);
    assert_ne!(after, before);
    assert_eq!(original.history_depths(), (1, 0));
    assert_eq!(original.undo(1), Ok(2));
    assert_eq!(save(&original, 2), before);
    assert_eq!(original.redo(2), Ok(3));
    assert_eq!(save(&original, 3), after);
    assert_eq!(save(&original, 0), before);
    let reopened = NativeObjectHistory::from_project_file("reopened", &after).unwrap();
    assert_eq!(save(&reopened, 0), after);
    assert!(!current(&reopened).document().layers()[0].visible());
}

#[test]
fn blank_replacement_prepares_immutably_and_commits_in_the_original_history() {
    let mut owner = NativeObjectHistory::from_project_file("new-replace", &file(64)).unwrap();
    attach(&mut owner);
    let original = current(&owner);
    let before = bytes(&owner);
    let address = &owner as *const _;
    let prepared = owner
        .prepare_blank_project_replacement(original.snapshot_id(), 640, 360, 29.97)
        .unwrap();
    assert_eq!(bytes(&owner), before);
    assert_eq!(owner.document_id(), original.document_id());
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    owner.validate_project_replacement(&prepared).unwrap();
    owner.commit_project_replacement(prepared).unwrap();
    assert_eq!(&owner as *const _, address);
    assert_eq!(owner.instance_id(), original.instance_id());
    assert_ne!(owner.document_id(), original.document_id());
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
    let saved = bytes(&owner);
    let expected = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":640,"canvasH":360,"fps":29.97,
            "canvasBg":"#ffffff","waIn":0.0,"waOut":149.0},
        "drawing":{"format":"nemo.native-object-document","formatVersion":2,
            "totalFrames":150,"layers":[{"layerUid":"layer-1","name":"Layer 1","visible":true,
                    "locked":false,"color":"#5FA875","parentLayerUid":null,
                "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
                    "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],
            "objects":[],"compositionOrder":{"format":"nemo.native-object-order-plan",
                "formatVersion":1,"documentFormatVersion":2,"contexts":[{"contextId":"scene-root",
                    "layers":[{"layerUid":"layer-1","naturalPosition":0,"order":{}}],
                    "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[]}]}],
                "migration":"explicit-plan-required","referencePolicy":"unresolved"}},
        "viewportReference":null,"assets":[]});
    assert_eq!(serde_json::from_slice::<Value>(&saved).unwrap(), expected);
    let reopened = NativeObjectHistory::from_project_file("reopened", &saved).unwrap();
    assert_eq!(bytes(&reopened), saved);
    assert_eq!(owner.undo(0), Err(DispatchErrorCode::Unavailable));
    assert_eq!(owner.redo(0), Err(DispatchErrorCode::Unavailable));
    drop(owner);
    assert_eq!(original.serialize_project_file().unwrap(), before);
}

#[test]
fn blank_replacement_refuses_invalid_stale_and_foreign_intent_before_commit() {
    let mut owner = NativeObjectHistory::from_project_file("new-refuse", &file(64)).unwrap();
    let before = owner.project_file_info().unwrap();
    for (w, h, fps) in [
        (0, 360, 24.),
        (640, 0, 24.),
        (640, 360, 0.),
        (640, 360, f64::NAN),
        (640, 360, f64::INFINITY),
        (640, 360, f64::MAX),
    ] {
        assert!(matches!(
            owner.prepare_blank_project_replacement(&pin(&owner), w, h, fps),
            Err(DispatchErrorCode::InvalidRequest)
        ));
        assert_eq!(owner.project_file_info().unwrap(), before);
    }
    assert!(matches!(
        owner.prepare_blank_project_replacement("stale", 0, 0, f64::NAN),
        Err(DispatchErrorCode::StaleRevision)
    ));
    let stale = owner
        .prepare_blank_project_replacement(&pin(&owner), 80, 48, 24.)
        .unwrap();
    attach(&mut owner);
    let changed = owner.project_file_info().unwrap();
    assert_eq!(
        owner.commit_project_replacement(stale),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(owner.project_file_info().unwrap(), changed);
    for (instance, error) in [
        ("other", DispatchErrorCode::WrongInstance),
        ("new-refuse", DispatchErrorCode::WrongDocument),
    ] {
        let token = owner
            .prepare_blank_project_replacement(&pin(&owner), 80, 48, 24.)
            .unwrap();
        let mut foreign = NativeObjectHistory::from_project_file(instance, &file(96)).unwrap();
        let unchanged = foreign.project_file_info().unwrap();
        assert_eq!(foreign.commit_project_replacement(token), Err(error));
        assert_eq!(foreign.project_file_info().unwrap(), unchanged);
    }
    let bare: crate::object_document::ObjectDocument = serde_json::from_value(
        serde_json::from_slice::<Value>(&file(64)).unwrap()["drawing"].clone(),
    )
    .unwrap();
    let bare = NativeObjectHistory::new("bare", bare).unwrap();
    assert!(matches!(
        bare.prepare_blank_project_replacement(&pin(&bare), 80, 48, 24.),
        Err(DispatchErrorCode::Unavailable)
    ));
}

#[path = "object_project_retained_history.rs"]
mod retained_history_tests;

#[test]
fn blank_project_explicit_total_frames_sets_duration_and_work_area() {
    let owner = NativeObjectHistory::new_blank_project_frames("kitsu", 640, 360, 25., Some(37)).unwrap();
    let info = owner.project_file_info().unwrap();
    assert_eq!(info["totalFrames"].as_u64(), Some(37));
    let saved: Value = serde_json::from_slice(&save(&owner, 0)).unwrap();
    assert_eq!(saved["project"]["waOut"].as_f64(), Some(36.));
    let default = NativeObjectHistory::new_blank_project_frames("kitsu", 640, 360, 25., None).unwrap();
    assert_eq!(default.project_file_info().unwrap()["totalFrames"].as_u64(), Some(125));
    assert!(matches!(
        NativeObjectHistory::new_blank_project_frames("kitsu", 640, 360, 25., Some(0)),
        Err(DispatchErrorCode::InvalidRequest)
    ));
    let max = NativeObjectHistory::new_blank_project_frames("kitsu", 64, 48, 24., Some(u32::MAX)).unwrap();
    assert_eq!(max.project_file_info().unwrap()["totalFrames"].as_u64(), Some(u32::MAX as u64));
    let prepared = owner
        .prepare_blank_project_replacement_frames(&pin(&owner), 80, 48, 24., Some(9))
        .unwrap();
    owner.validate_project_replacement(&prepared).unwrap();
    assert!(matches!(
        owner.prepare_blank_project_replacement_frames(&pin(&owner), 80, 48, 24., Some(0)),
        Err(DispatchErrorCode::InvalidRequest)
    ));
}
