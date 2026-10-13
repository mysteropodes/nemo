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
