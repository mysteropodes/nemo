//! Real native project files bypass the small command-response byte envelope.
use crate::application_mcp::ApplicationMcp;
use crate::native_object_bootstrap::{
    bootstrap_raw, reopen::bootstrap_request, save::save_request,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("nemo-project-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn file() -> String {
    let layers: Vec<_> = (0..400)
        .map(|n| json!({"layerUid":format!("drawing-{n}")}))
        .collect();
    json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":29.97},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":21,"layers":layers,"objects":[]},"viewportReference":null,"assets":[]})
    .to_string()
}
fn open(state: &ApplicationMcp, bytes: &str) -> Value {
    serde_json::to_value(
        bootstrap_raw(
            state,
            &json!({"apiVersion":2,"requestId":"open",
        "instanceId":state.instance_id(),"documentJson":bytes})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn request(state: &ApplicationMcp, document: &str, path: &Path) -> Value {
    let info = state.native_project_info(document).unwrap();
    json!({"apiVersion":2,"requestId":"save","instanceId":state.instance_id(),"documentId":document,
        "atRevision":info["contentRevision"],"documentSnapshotId":info["documentSnapshotId"],
        "destinationPath":path})
}

#[test]
fn real_large_project_publishes_exact_native_bytes_and_reopens_from_disk() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let source = file();
    assert!(source.len() > 4096);
    let installed = open(&state, &source);
    let document = installed["documentId"].as_str().unwrap();
    let info = state.native_project_info(document).unwrap();
    assert_eq!(info["layers"].as_array().unwrap().len(), 400);
    assert_eq!(info["fps"], 29.97);
    assert_eq!(info["undoDepth"], 0);
    let destination = scratch.0.join("saved.json");
    let save = request(&state, document, &destination);
    let receipt = save_request(
        &state,
        &save.to_string(),
        |p| p.parent() == Some(scratch.0.as_path()),
        |_, _| Ok(()),
    )
    .unwrap();
    let receipt = serde_json::to_value(receipt).unwrap();
    assert_eq!(receipt["published"], true);
    assert_eq!(receipt["cleanupPending"], false);
    let expected = state
        .native_project_file_snapshot(document, 0, info["documentSnapshotId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(fs::read(&destination).unwrap(), expected);
    assert_eq!(
        receipt["byteCount"].as_u64().unwrap() as usize,
        expected.len()
    );
    assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 1);
    let fresh = ApplicationMcp::default();
    let reopened = bootstrap_request(
        &fresh,
        &json!({"apiVersion":2,"requestId":"file-open",
        "instanceId":fresh.instance_id(),"sourcePath":destination})
        .to_string(),
        |p| p == destination,
    )
    .unwrap();
    let reopened = serde_json::to_value(reopened).unwrap();
    assert_ne!(reopened["documentId"], installed["documentId"]);
    let view = fresh
        .native_project_info(reopened["documentId"].as_str().unwrap())
        .unwrap();
    assert_eq!(view["documentDigest"], info["documentDigest"]);
    assert_eq!(view["layers"], info["layers"]);
    assert_eq!(view["byteCount"], info["byteCount"]);
}

#[test]
fn stale_scope_cancel_duplicate_and_write_failure_do_not_publish_or_damage_owner() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let installed = open(&state, &file());
    let document = installed["documentId"].as_str().unwrap();
    let before = state.native_project_info(document).unwrap();
    let destination = scratch.0.join("refused.json");
    let valid = request(&state, document, &destination);
    let mut stale = valid.clone();
    stale["documentSnapshotId"] = "wrong".into();
    let mut cancelled = valid.clone();
    cancelled["cancelledBeforeDispatch"] = true.into();
    for raw in [
        stale.to_string(),
        cancelled.to_string(),
        valid
            .to_string()
            .replacen("\"apiVersion\":2", "\"apiVersion\":2,\"apiVersion\":2", 1),
    ] {
        assert!(save_request(&state, &raw, |_| true, |_, _| Ok(())).is_err());
    }
    assert!(save_request(&state, &valid.to_string(), |_| false, |_, _| Ok(())).is_err());
    assert!(save_request(
        &state,
        &valid.to_string(),
        |_| true,
        |phase, _| if phase == "write" {
            Err(std::io::Error::other("injected"))
        } else {
            Ok(())
        }
    )
    .is_err());
    assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 0);
    assert_eq!(state.native_project_info(document).unwrap(), before);
    fs::write(&destination, b"existing data").unwrap();
    assert!(save_request(&state, &valid.to_string(), |_| true, |_, _| Ok(())).is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"existing data");
    assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 1);
}

#[test]
fn invalid_project_or_occupied_install_never_replaces_existing_native_authority() {
    let state = ApplicationMcp::default();
    let installed = open(&state, &file());
    let document = installed["documentId"].as_str().unwrap();
    let before = state.native_project_info(document).unwrap();
    for bytes in [
        file(),
        file().replacen(
            "\"formatVersion\":1",
            "\"formatVersion\":1,\"formatVersion\":1",
            1,
        ),
        "null".into(),
    ] {
        let raw=json!({"apiVersion":2,"requestId":"refused","instanceId":state.instance_id(),"documentJson":bytes}).to_string();
        assert!(bootstrap_raw(&state, &raw).is_err());
        assert_eq!(state.native_project_info(document).unwrap(), before);
    }
}

#[test]
fn project_only_file_selection_refuses_other_formats_before_owner_installation() {
    let scratch = Scratch::new();
    let path = scratch.0.join("selected.json");
    let state = ApplicationMcp::default();
    let raw = json!({"apiVersion":2,"requestId":"ordinary-open","instanceId":state.instance_id(),
        "sourcePath":path,"projectOnly":true})
    .to_string();
    for bytes in [
        json!({"layers":[],"canvasW":320}).to_string(),
        json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
            "layers":[{"layerUid":"drawing"}],"objects":[]})
        .to_string(),
    ] {
        fs::write(&path, bytes).unwrap();
        assert_eq!(
            bootstrap_request(&state, &raw, |_| true).unwrap_err().code,
            "unsupported_format"
        );
        assert!(state.native_state().lock().unwrap().active().is_none());
    }
    // A malformed claimed project is an error, never permission to fall back.
    fs::write(
        &path,
        file().replace("\"formatVersion\":1", "\"formatVersion\":99"),
    )
    .unwrap();
    assert_ne!(
        bootstrap_request(&state, &raw, |_| true).unwrap_err().code,
        "unsupported_format"
    );
    assert!(state.native_state().lock().unwrap().active().is_none());
    fs::write(&path, file()).unwrap();
    assert!(bootstrap_request(&state, &raw, |_| true).is_ok());
}

#[test]
fn checked_in_p38_native_fixtures_open_through_project_only_file_route() {
    for (name, bytes) in [
        (
            "A.json",
            include_bytes!("../../tests/fixtures/p38/native-project/A.json").as_slice(),
        ),
        (
            "B.json",
            include_bytes!("../../tests/fixtures/p38/native-project/B.json").as_slice(),
        ),
    ] {
        let scratch = Scratch::new();
        let path = scratch.0.join(name);
        fs::write(&path, bytes).unwrap();
        let state = ApplicationMcp::default();
        let raw = json!({"apiVersion":2,"requestId":"p38-fixture-open",
            "instanceId":state.instance_id(),"sourcePath":path,"projectOnly":true})
        .to_string();
        let opened =
            bootstrap_request(&state, &raw, |candidate| candidate == path.as_path()).unwrap();
        let opened = serde_json::to_value(opened).unwrap();
        let document = opened["documentId"].as_str().unwrap();
        let info = state.native_project_info(document).unwrap();
        let (width, height, fps) = if name == "A.json" {
            (320, 180, 24.0)
        } else {
            (640, 360, 30.0)
        };
        assert_eq!(info["canvasW"], width);
        assert_eq!(info["canvasH"], height);
        assert_eq!(info["fps"], fps);
        assert_eq!(info["totalFrames"], 21);
        assert_eq!(info["layers"].as_array().unwrap().len(), 2);
        assert_ne!(info["documentDigest"], "");
    }
}
