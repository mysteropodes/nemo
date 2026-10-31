//! Real native project files bypass the small command-response byte envelope.
use crate::application_mcp::ApplicationMcp;
use crate::native_dispatch::{NativeDispatch, NativeObjectHost, PROJECT_INFO_LIMIT_BYTES};
use crate::native_object_bootstrap::{
    bootstrap_raw, reopen::bootstrap_request, save::save_request,
};
use native_engine::{
    object_document::FrameScopeKind, object_frame_packet::ObjectFrameSelector,
    scheduler::OutputSpec,
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
/// #2371 review: every project-info row carries kind, folder, blend, opacity and matte facts; the read must not
/// hard-fail at hundreds of fully populated layers (baseline Release to Layers makes one layer per shape).
fn rich_file(count: usize, name_bytes: usize) -> String {
    let uid = |i: usize| format!("ly_{i:05}_mh3k2j9a4f");
    let layers: Vec<_> = (0..count)
        .map(|i| {
            json!({"layerUid":uid(i),"name":format!("Layer {i} {}", "x".repeat(name_bytes)),"visible":true,
                "locked":false,"shy":false,"color":"#5FA875","parentLayerUid":null,"folderId":"f1",
                "blendMode":"multiply","matteMode":"alphaInverted","matteSourceLayerUid":uid((i + 1) % count),
                "mattesMore":[{"uid":uid((i + 2) % count),"mode":"luma"},{"uid":uid((i + 3) % count),"mode":"alpha"}]})
        })
        .collect();
    let holders: Vec<_> = (0..count)
        .map(|i| json!({"target":{"contextId":"scene-root","layerUid":uid(i)},
            "properties":[{"property":"opacity","static":[50.5]}]}))
        .collect();
    json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4,"layers":layers,
            "layerFolders":{"f1":{"name":"F","collapsed":false}},"propertyHolders":holders,"objects":[]},
        "viewportReference":null,"assets":[]})
    .to_string()
}

#[test]
fn project_info_reads_hundreds_of_rich_rows_and_refuses_only_past_the_hard_cap() {
    let state = ApplicationMcp::default();
    let receipt = open(&state, &rich_file(600, 40));
    let id = receipt["documentId"].as_str().unwrap();
    let info = state.native_project_info(id).unwrap();
    let bytes = serde_json::to_vec(&info).unwrap().len();
    assert_eq!(info["layers"].as_array().unwrap().len(), 600);
    assert_eq!(info["layers"][599]["kind"], "draw");
    assert_eq!(info["layers"][599]["mattesMore"].as_array().unwrap().len(), 2);
    // The retired 64 KiB bound would have refused this answer; the hard cap still holds.
    assert!(bytes > 65536 && bytes <= PROJECT_INFO_LIMIT_BYTES, "{bytes}");
    // Past the cap the read is refused, never truncated.
    let big = ApplicationMcp::default();
    let (document, _, _) = big
        .install_native_project_file(rich_file(3, PROJECT_INFO_LIMIT_BYTES / 3).as_bytes())
        .unwrap();
    let err = big.native_project_info(&document).unwrap_err();
    assert!(err.contains("exceeds UI limit"), "{err}");
}

fn request(state: &ApplicationMcp, document: &str, path: &Path) -> Value {
    let info = state.native_project_info(document).unwrap();
    json!({"apiVersion":2,"requestId":"save","instanceId":state.instance_id(),"documentId":document,
        "atRevision":info["contentRevision"],"documentSnapshotId":info["documentSnapshotId"],
        "destinationPath":path})
}

fn p38_drawing(variant: bool) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut at_open = cases["records"][0].clone();
    at_open["target"]["frameScope"]["frame"] = json!(0);
    at_open["target"]["strokeId"] = json!("ink/path_A");
    let mut authored = cases["records"][0].clone();
    authored["target"]["frameScope"]["frame"] = json!(7);
    authored["target"]["strokeId"] = json!("ink/path_B");
    if variant {
        at_open["fill"]["r"] = json!(0.8);
        authored["geometry"]["segments"][0]["point"]["x"] = json!(15.125);
    }
    let reference = cases["records"][1].clone();
    json!({
        "format": "nemo.native-object-document",
        "formatVersion": 2,
        "totalFrames": 21,
        "layers": [
            {"layerUid": "legacy layer:alpha", "timing": timing_tests::fixture_timing(&[0, 7])},
            {"layerUid": "0", "timing": timing_tests::fixture_timing(&[])}
        ],
        "objects": [at_open, authored, reference],
        "compositionOrder": {
            "format": "nemo.native-object-order-plan",
            "formatVersion": 1,
            "documentFormatVersion": 2,
            "contexts": [{
                "contextId": "scene-root",
                "layers": [
                    {"layerUid": "legacy layer:alpha", "naturalPosition": 0, "order": {"static": [0]}},
                    {"layerUid": "0", "naturalPosition": 1, "order": {}}
                ],
                "scopes": [
                    {"frameScope": {"kind": "authored", "frame": 0}, "elements": [
                        {"layerUid": "legacy layer:alpha", "strokeId": "ink/path_A", "naturalPosition": 0, "order": {}}
                    ]},
                    {"frameScope": {"kind": "authored", "frame": 7}, "elements": [
                        {"layerUid": "legacy layer:alpha", "strokeId": "ink/path_B", "naturalPosition": 0, "order": {}}
                    ]}
                ]
            }],
            "migration": "explicit-plan-required",
            "referencePolicy": "unresolved"
        }
    })
}

fn p38_project_bytes(width: u32, height: u32, fps: u32, variant: bool) -> Vec<u8> {
    let input = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":width,"canvasH":height,"fps":fps},
        "drawing":p38_drawing(variant),"viewportReference":null,"assets":[]});
    let input = serde_json::to_vec(&input).unwrap();
    let owner =
        NativeObjectHost::from_project_file("p38-fixture-producer".to_owned(), &input).unwrap();
    let info = owner.project_file_info().unwrap();
    owner
        .project_file_snapshot(
            info["contentRevision"].as_u64().unwrap(),
            info["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap()
}

fn assert_p38_cpu_paint(bytes: &[u8]) {
    let mut owner = NativeObjectHost::from_project_file("p38-fixture-paint".into(), bytes).unwrap();
    let info = owner.project_file_info().unwrap();
    let instance_id = owner.instance_id().to_owned();
    let document_id = owner.document_id().to_owned();
    let snapshot_id = info["documentSnapshotId"].as_str().unwrap().to_owned();
    let revision = owner.content_revision();
    let width = info["canvasW"].as_u64().unwrap() as u32;
    let height = info["canvasH"].as_u64().unwrap() as u32;
    for frame in [0, 7] {
        let selector = ObjectFrameSelector {
            instance_id: &instance_id,
            document_id: &document_id,
            content_revision: revision,
            document_snapshot_id: &snapshot_id,
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame,
        };
        let output = OutputSpec::new("frame", "rgba8", width, height, "srgb", "straight").unwrap();
        owner
            .with_prepared_object_preview(&selector, output, |_| Ok(()))
            .unwrap();
    }
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
    let fixtures = [
        (
            "A.json",
            include_bytes!("../../tests/fixtures/p38/native-project/A.json").as_slice(),
            320,
            180,
            24,
            false,
        ),
        (
            "B.json",
            include_bytes!("../../tests/fixtures/p38/native-project/B.json").as_slice(),
            640,
            360,
            30,
            true,
        ),
    ];
    let mut digests = Vec::new();
    for (name, bytes, width, height, fps, variant) in fixtures {
        assert_eq!(
            bytes,
            p38_project_bytes(width, height, fps, variant).as_slice(),
            "{name} must be a canonical production project encoder output"
        );
        let drawing: Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(drawing["drawing"]["formatVersion"], 2);
        assert_eq!(drawing["drawing"]["objects"].as_array().unwrap().len(), 3);
        for frame in [0, 7] {
            let authored = drawing["drawing"]["objects"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|object| {
                    object["target"]["frameScope"]["kind"] == "authored"
                        && object["target"]["frameScope"]["frame"] == frame
                })
                .count();
            assert_eq!(authored, 1, "frame {frame} must contain authored content");
            let scope = drawing["drawing"]["compositionOrder"]["contexts"][0]["scopes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|scope| scope["frameScope"]["frame"] == frame)
                .unwrap();
            assert_eq!(scope["elements"].as_array().unwrap().len(), 1);
        }
        assert_p38_cpu_paint(bytes);

        let scratch = Scratch::new();
        let path = scratch.0.join(name);
        fs::write(&path, bytes).unwrap();
        let state = ApplicationMcp::default();
        let raw = json!({"apiVersion":2,"requestId":"p38-fixture-open",
            "instanceId":state.instance_id(),"sourcePath":path,"projectOnly":true})
        .to_string();
        let refused = bootstrap_request(&state, &raw, |_| false).unwrap_err();
        assert_eq!(refused.code, "unavailable");
        assert_eq!(
            refused.message,
            "object file is outside the existing filesystem scope"
        );
        assert_eq!(refused.details.unwrap()["disposition"], "notDispatched"); // #2511: retryable
        assert!(state.native_state().lock().unwrap().active().is_none());
        let opened =
            bootstrap_request(&state, &raw, |candidate| candidate == path.as_path()).unwrap();
        let opened = serde_json::to_value(opened).unwrap();
        let document = opened["documentId"].as_str().unwrap();
        let info = state.native_project_info(document).unwrap();
        assert_eq!(info["canvasW"], width);
        assert_eq!(info["canvasH"], height);
        assert_eq!(info["fps"], f64::from(fps));
        assert_eq!(info["totalFrames"], 21);
        assert_eq!(info["layers"].as_array().unwrap().len(), 2);
        assert_ne!(info["documentDigest"], "");
        digests.push(info["documentDigest"].clone());
    }
    assert_ne!(digests[0], digests[1]);
}

#[test]
#[ignore = "manual deterministic regeneration of the pinned P38 native fixtures"]
fn regenerate_p38_native_project_fixtures() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/p38/native-project");
    fs::write(root.join("A.json"), p38_project_bytes(320, 180, 24, false)).unwrap();
    fs::write(root.join("B.json"), p38_project_bytes(640, 360, 30, true)).unwrap();
}

fn blank_host_snapshot(owner: &NativeObjectHost) -> Vec<u8> {
    let info = owner.project_file_info().unwrap();
    owner
        .project_file_snapshot(
            owner.content_revision(),
            info["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap()
}

#[path = "native_object_project_timing_tests.rs"]
mod timing_tests;

fn blank_host_cpu_preview(owner: &mut NativeObjectHost) {
    let info = owner.project_file_info().unwrap();
    let instance = owner.instance_id().to_owned();
    let document = owner.document_id().to_owned();
    let selector = ObjectFrameSelector {
        instance_id: &instance,
        document_id: &document,
        content_revision: owner.content_revision(),
        document_snapshot_id: info["documentSnapshotId"].as_str().unwrap(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let output = OutputSpec::new(
        "frame",
        "rgba8",
        info["canvasW"].as_u64().unwrap() as u32,
        info["canvasH"].as_u64().unwrap() as u32,
        "srgb",
        "straight",
    )
    .unwrap();
    owner
        .with_prepared_object_preview(&selector, output, |_| Ok(()))
        .unwrap();
}

#[test]
fn blank_host_replacement_requires_cleanup_then_preserves_preview_and_save_reopen() {
    let mut owner =
        NativeObjectHost::new_blank_project("blank-host".into(), 320, 180, 24.).unwrap();
    blank_host_cpu_preview(&mut owner);
    let before = owner.project_file_info().unwrap();
    let original = owner.document_id().to_owned();
    let saved = blank_host_snapshot(&owner);
    let pin = before["documentSnapshotId"].as_str().unwrap();
    let premature = owner
        .prepare_blank_project_replacement(pin, 640, 360, 29.97)
        .unwrap();
    assert_eq!(owner.project_file_info().unwrap(), before);
    assert!(owner
        .commit_project_replacement(premature)
        .unwrap_err()
        .contains("complete owner cleanup"));
    assert_eq!(blank_host_snapshot(&owner), saved);
    let next = owner
        .prepare_blank_project_replacement(pin, 640, 360, 29.97)
        .unwrap();
    owner.validate_project_replacement(&next).unwrap();
    assert!(owner.release_project().unwrap().reconciliation_complete());
    assert!(!owner.object_client_available());
    assert!(owner
        .prepare_blank_project_replacement(pin, 80, 48, 24.)
        .is_err());
    owner.commit_project_replacement(next).unwrap();
    assert!(owner.object_client_available());
    assert!(owner.release_progress().is_none());
    assert_ne!(owner.document_id(), original);
    let info = owner.project_file_info().unwrap();
    assert_eq!(
        (info["canvasW"].as_u64(), info["canvasH"].as_u64()),
        (Some(640), Some(360))
    );
    assert_eq!(info["fps"], 29.97);
    assert_eq!(info["totalFrames"], 150);
    assert_eq!(info["undoDepth"], 0);
    assert_eq!(info["redoDepth"], 0);
    assert_eq!(owner.content_revision(), 0);
    blank_host_cpu_preview(&mut owner);
    let new_bytes = blank_host_snapshot(&owner);
    assert_ne!(new_bytes, saved);
    let mut reopened =
        NativeObjectHost::from_project_file("blank-reopen".into(), &new_bytes).unwrap();
    assert_eq!(blank_host_snapshot(&reopened), new_bytes);
    blank_host_cpu_preview(&mut reopened);
}

#[test]
fn blank_host_rejects_invalid_identity_intent_and_snapshot_without_mutating_owner() {
    for instance in ["", "has space", "?invalid"] {
        assert!(NativeObjectHost::new_blank_project(instance.into(), 64, 48, 24.).is_err());
    }
    let owner =
        NativeObjectHost::from_project_file("blank-refuse".into(), file().as_bytes()).unwrap();
    let before = owner.project_file_info().unwrap();
    let saved = blank_host_snapshot(&owner);
    let pin = before["documentSnapshotId"].as_str().unwrap();
    for (w, h, fps) in [
        (0, 48, 24.),
        (64, 0, 24.),
        (64, 48, f64::NAN),
        (64, 48, 0.),
        (64, 48, f64::MAX),
    ] {
        assert!(NativeObjectHost::new_blank_project("valid".into(), w, h, fps).is_err());
        assert!(owner
            .prepare_blank_project_replacement(pin, w, h, fps)
            .is_err());
        assert_eq!(owner.project_file_info().unwrap(), before);
    }
    assert!(owner
        .prepare_blank_project_replacement("stale", 64, 48, 24.)
        .is_err());
    assert_eq!(blank_host_snapshot(&owner), saved);
    assert!(owner.release_progress().is_none());
    assert!(owner.object_client_available());
}
