//! Frozen independent full-document bytes and real private write/reopen controls.
use crate::native_object_bootstrap::{
    bootstrap_raw,
    reopen::bootstrap_request,
    save::{save_request, save_selected_request},
};
use crate::{
    application_mcp::ApplicationMcp,
    native_application::{admit_release_request, complete_release},
    native_application_contract::NativeReleaseRequest,
    native_dispatch::{ReleaseAdmission, TerminalPump},
};
use serde_json::{json, Value};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const ORIGINAL: &str = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,"layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":[{"schemaVersion":1,"family":"closed-solid-fill-cubic-path","target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},"layerUid":"legacy layer:alpha","strokeId":"ink/path_A"},"geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":[{"point":{"x":10.125,"y":20},"handleIn":{"x":-2,"y":0},"handleOut":{"x":3.25,"y":-1}},{"point":{"x":40,"y":20},"handleIn":{"x":-3,"y":2},"handleOut":{"x":0,"y":6}},{"point":{"x":25,"y":50},"handleIn":{"x":4,"y":-2},"handleOut":{"x":-5,"y":-3}}]},"fill":{"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.75}},{"schemaVersion":1,"family":"closed-solid-fill-cubic-path","target":{"contextId":"scene-root","frameScope":{"kind":"reference","frame":12},"layerUid":"0","strokeId":"0001"},"geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":[{"point":{"x":-10,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},{"point":{"x":10,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},{"point":{"x":0,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]},"fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}}]}"#;
const EDITED: &str = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,"layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":[{"schemaVersion":1,"family":"closed-solid-fill-cubic-path","target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},"layerUid":"legacy layer:alpha","strokeId":"ink/path_A"},"geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":[{"point":{"x":10.125,"y":20},"handleIn":{"x":-2,"y":0},"handleOut":{"x":3.25,"y":-1}},{"point":{"x":40,"y":20},"handleIn":{"x":-3,"y":2},"handleOut":{"x":0,"y":6}},{"point":{"x":25,"y":50},"handleIn":{"x":4,"y":-2},"handleOut":{"x":-5,"y":-3}}]},"fill":{"kind":"solid","r":0.9,"g":0.1,"b":0.3,"a":0.5}},{"schemaVersion":1,"family":"closed-solid-fill-cubic-path","target":{"contextId":"scene-root","frameScope":{"kind":"reference","frame":12},"layerUid":"0","strokeId":"0001"},"geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":[{"point":{"x":-10,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},{"point":{"x":10,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},{"point":{"x":0,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]},"fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}}]}"#;

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("nemo-save-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn files(&self) -> usize {
        fs::read_dir(&self.0).unwrap().count()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn install(state: &ApplicationMcp, bytes: &str) -> Value {
    serde_json::to_value(
        bootstrap_raw(
            state,
            &json!({"apiVersion":2,"requestId":"save-install",
        "instanceId":state.instance_id(),"documentJson":bytes})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn dispatch(
    state: &ApplicationMcp,
    pins: &Value,
    op: &str,
    revision: u64,
    payload: Value,
) -> Value {
    let mut request = json!({"apiVersion":2,"requestId":format!("{op}-{revision}"),
        "instanceId":pins["instanceId"],"documentId":pins["documentId"],
        "operation":op,"payload":payload});
    if op.starts_with("command.") {
        request["expectedRevision"] = json!(revision);
    }
    serde_json::to_value(
        state
            .serialize_object_file_snapshot(&request.to_string())
            .unwrap(),
    )
    .unwrap()
}
fn serialized(state: &ApplicationMcp, pins: &Value, rev: u64) -> Value {
    dispatch(
        state,
        pins,
        "query.document.object.serialize",
        rev,
        json!({"atRevision":rev}),
    )
}
fn request(pins: &Value, rev: u64, path: &Path) -> Value {
    json!({"apiVersion":2,"requestId":"save-new","instanceId":pins["instanceId"],
        "documentId":pins["documentId"],"atRevision":rev,
        "documentSnapshotId":format!("native-object:{}:{rev}",pins["documentId"].as_str().unwrap()),
        "destinationPath":path.to_str().unwrap()})
}
fn save(state: &ApplicationMcp, request: &Value, root: &Path) -> Value {
    serde_json::to_value(
        save_request(
            state,
            &request.to_string(),
            |p| p.starts_with(root),
            |_, _| Ok(()),
        )
        .unwrap(),
    )
    .unwrap()
}
fn advance(state: &ApplicationMcp, pins: &Value, rev: u64, action: &str) {
    let payload = if action == "fill.set" {
        json!({"command":"object.fill.set","stableTarget":
            serde_json::from_str::<Value>(ORIGINAL).unwrap()["objects"][0]["target"],
            "fill":{"kind":"solid","r":0.9,"g":0.1,"b":0.3,"a":0.5}})
    } else {
        json!({"command":format!("object.{action}")})
    };
    let result = dispatch(
        state,
        pins,
        &format!("command.document.object.{action}"),
        rev,
        payload,
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["contentRevision"], rev + 1);
}
fn release(state: &ApplicationMcp, pins: &Value, rev: u64) {
    let request: NativeReleaseRequest = serde_json::from_value(json!({"apiVersion":2,
        "requestId":"save-release","instanceId":pins["instanceId"],
        "documentId":pins["documentId"],"expectedRevision":rev}))
    .unwrap();
    let native = state.native_state();
    let generation = match admit_release_request(&native, &request).unwrap() {
        ReleaseAdmission::Execute { generation } => generation,
        _ => panic!("retained release"),
    };
    complete_release(&native, generation, &request, || {
        Ok((vec![], "already_absent"))
    })
    .unwrap();
}
fn fail() -> io::Error {
    io::Error::from_raw_os_error(13)
}

#[test]
fn actual_fill_undo_redo_exact_disk_bytes_and_production_file_reopen() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let pins = install(&state, ORIGINAL);
    for (rev, action, expected) in [
        (1, "fill.set", EDITED),
        (2, "undo", ORIGINAL),
        (3, "redo", EDITED),
    ] {
        advance(&state, &pins, rev - 1, action);
        let oracle = serialized(&state, &pins, rev);
        assert_eq!(
            oracle["result"]["documentJson"], expected,
            "frozen serializer oracle"
        );
        let path = scratch.0.join(format!("revision-{rev}.json"));
        let result = save(&state, &request(&pins, rev, &path), &scratch.0);
        assert_eq!(result["published"], true);
        assert_eq!(result["cleanupPending"], false);
        assert_eq!(result["byteCount"], expected.len());
        assert_eq!(result["atRevision"], rev);
        for key in [
            "apiVersion",
            "requestId",
            "instanceId",
            "documentId",
            "documentSnapshotId",
        ] {
            assert_eq!(result[key], request(&pins, rev, &path)[key]);
        }
        assert_eq!(fs::read(&path).unwrap(), expected.as_bytes());
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
            serde_json::from_str::<Value>(expected).unwrap()
        );
        assert_eq!(serialized(&state, &pins, rev), oracle);
        assert_eq!(scratch.files(), rev as usize);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    release(&state, &pins, 3);
    let raw = json!({"apiVersion":2,"requestId":"save-reopen","instanceId":state.instance_id(),
        "sourcePath":scratch.0.join("revision-3.json")})
    .to_string();
    let fresh = serde_json::to_value(
        bootstrap_request(&state, &raw, |p| p.starts_with(&scratch.0)).unwrap(),
    )
    .unwrap();
    assert_ne!(fresh["documentId"], pins["documentId"]);
    assert_eq!(fresh["contentRevision"], 0);
    assert_eq!(
        serialized(&state, &fresh, 0)["result"]["documentJson"],
        EDITED
    );
    for action in ["undo", "redo"] {
        assert_eq!(
            dispatch(
                &state,
                &fresh,
                &format!("command.document.object.{action}"),
                0,
                json!({"command":format!("object.{action}")})
            )["ok"],
            false
        );
    }
    assert_eq!(
        serialized(&state, &pins, 3)["error"]["code"],
        "wrong_document"
    );
    assert!(state
        .dispatch_native(
            serde_json::from_value(json!({"apiVersion":2,"requestId":"public-save",
        "instanceId":fresh["instanceId"],"documentId":fresh["documentId"],
        "operation":"query.document.object.serialize","payload":{"atRevision":0}}))
            .unwrap()
        )
        .is_err());
}

#[test]
fn strict_original_raw_envelope_and_pins_refuse_with_zero_disk_effect() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let pins = install(&state, ORIGINAL);
    let valid = request(&pins, 0, &scratch.0.join("new.json"));
    let raw = valid.to_string();
    let before = serialized(&state, &pins, 0);
    let mut bad = vec![
        "[]".into(),
        "null".into(),
        format!("{raw} trailing"),
        format!("{raw}{}", " ".repeat(4097 - raw.len())),
    ];
    for (key, value) in [
        ("apiVersion", json!(1)),
        ("requestId", json!("bad id")),
        ("instanceId", json!("other")),
        ("documentId", json!("other")),
        ("documentSnapshotId", json!("other")),
        ("atRevision", json!(1)),
        ("atRevision", json!(-1)),
        ("atRevision", json!(0.5)),
        ("atRevision", json!(9_007_199_254_740_992u64)),
        ("extra", json!(true)),
        ("cancelledBeforeDispatch", json!(true)),
        ("cancelledBeforeDispatch", json!(1)),
    ] {
        let mut value_request = valid.clone();
        value_request[key] = value;
        bad.push(value_request.to_string());
    }
    for key in [
        "apiVersion",
        "requestId",
        "instanceId",
        "documentId",
        "atRevision",
        "documentSnapshotId",
        "destinationPath",
        "cancelledBeforeDispatch",
    ] {
        let mut present = valid.clone();
        present["cancelledBeforeDispatch"] = json!(false);
        for encoded in [
            key.to_owned(),
            format!("\\u{:04x}{}", key.as_bytes()[0], &key[1..]),
        ] {
            bad.push(present.to_string().replacen(
                &format!("\"{key}\":"),
                &format!("\"{encoded}\":null,\"{key}\":"),
                1,
            ));
        }
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(key);
        if key != "cancelledBeforeDispatch" {
            bad.push(missing.to_string());
        }
    }
    for path in [
        "relative.json",
        "/",
        "/tmp/.",
        "/tmp/..",
        "/tmp/ending/",
        "/tmp/bad\0name",
    ] {
        let mut request = valid.clone();
        request["destinationPath"] = json!(path);
        bad.push(request.to_string());
    }
    for raw in bad {
        assert!(
            save_request(
                &state,
                &raw,
                |_| true,
                |_, _| panic!("invalid reached effect")
            )
            .is_err(),
            "{raw}"
        );
        assert_eq!(scratch.files(), 0);
        assert_eq!(serialized(&state, &pins, 0), before);
    }
    let exact = format!("{raw}{}", " ".repeat(4096 - raw.len()));
    assert!(save_request(
        &state,
        &exact,
        |_| false,
        |_, _| panic!("scope reached effect")
    )
    .is_err());
}

#[test]
fn selected_file_dialog_scope_publishes_without_global_sibling_authority() {
    let scratch = Scratch::new();
    let app = tauri::test::mock_app();
    let scope = tauri::fs::Scope::new(&app, &Default::default()).unwrap();
    let destination = scratch.0.join("selected.json");
    let unrelated = scratch.0.join("unrelated.json");
    scope.allow_file(&destination).unwrap();
    assert!(scope.is_allowed(&destination));
    assert!(!scope.is_allowed(&unrelated));
    let state = ApplicationMcp::default();
    let pins = install(&state, ORIGINAL);
    let receipt = save_selected_request(
        &app,
        &state,
        &request(&pins, 0, &destination).to_string(),
        |path| scope.is_allowed(path),
        |_, _| Ok(()),
    )
    .unwrap();
    let receipt = serde_json::to_value(receipt).unwrap();
    assert_eq!(receipt["published"], true);
    assert_eq!(receipt["cleanupPending"], false);
    assert_eq!(fs::read_to_string(&destination).unwrap(), ORIGINAL);
    assert_eq!(scratch.files(), 1);
    assert!(!scope.is_allowed(&unrelated));
    let changed = request(&pins, 0, &unrelated);
    assert!(save_selected_request(
        &app,
        &state,
        &changed.to_string(),
        |path| scope.is_allowed(path),
        |_, _| panic!("a changed destination reached a filesystem effect"),
    )
    .is_err());
    assert_eq!(scratch.files(), 1);
}

#[test]
fn selected_file_revocation_before_create_or_publish_leaves_no_output() {
    for revoke_at in ["create", "link"] {
        let scratch = Scratch::new();
        let app = tauri::test::mock_app();
        let scope = tauri::fs::Scope::new(&app, &Default::default()).unwrap();
        let destination = scratch.0.join("selected.json");
        scope.allow_file(&destination).unwrap();
        let state = ApplicationMcp::default();
        let pins = install(&state, ORIGINAL);
        let result = save_selected_request(
            &app,
            &state,
            &request(&pins, 0, &destination).to_string(),
            |path| scope.is_allowed(path),
            |phase, _| {
                if phase == revoke_at {
                    scope.forbid_file(&destination).unwrap();
                }
                Ok(())
            },
        );
        assert!(result.is_err(), "{revoke_at}");
        assert!(!destination.exists(), "{revoke_at}");
        assert_eq!(scratch.files(), 0, "{revoke_at}: staging cleanup");
        assert!(!scope.is_allowed(&destination));
        assert_eq!(
            serialized(&state, &pins, 0)["result"]["documentJson"],
            ORIGINAL
        );
    }
}

#[test]
fn selected_file_collision_preserves_foreign_destination_and_cleans_staging() {
    let scratch = Scratch::new();
    let app = tauri::test::mock_app();
    let scope = tauri::fs::Scope::new(&app, &Default::default()).unwrap();
    let destination = scratch.0.join("selected.json");
    scope.allow_file(&destination).unwrap();
    let state = ApplicationMcp::default();
    let pins = install(&state, ORIGINAL);
    assert!(save_selected_request(
        &app,
        &state,
        &request(&pins, 0, &destination).to_string(),
        |path| scope.is_allowed(path),
        |phase, _| {
            if phase == "link" {
                fs::write(&destination, "foreign destination")?;
            }
            Ok(())
        },
    )
    .is_err());
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "foreign destination"
    );
    assert_eq!(scratch.files(), 1);
}

#[test]
fn in_place_replace_overwrites_an_existing_regular_file_atomically_and_leaves_no_stage() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let pins = install(&state, ORIGINAL);
    let destination = scratch.0.join("titled.json");
    fs::write(&destination, "previous project bytes").unwrap();
    // Without the replace flag an existing destination is still refused and untouched.
    assert!(save_request(
        &state,
        &request(&pins, 0, &destination).to_string(),
        |p| p.starts_with(&scratch.0),
        |_, _| Ok(())
    )
    .is_err());
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "previous project bytes"
    );
    assert_eq!(scratch.files(), 1);
    let mut replace = request(&pins, 0, &destination);
    replace["replace"] = json!(true);
    let result = save(&state, &replace, &scratch.0);
    assert_eq!(result["published"], true);
    assert_eq!(result["cleanupPending"], false);
    assert_eq!(fs::read_to_string(&destination).unwrap(), ORIGINAL);
    assert_eq!(scratch.files(), 1, "the staged file was renamed, not left behind");
    // A replace whose target vanished still publishes the file.
    fs::remove_file(&destination).unwrap();
    let result = save(&state, &replace, &scratch.0);
    assert_eq!(result["published"], true);
    assert_eq!(fs::read_to_string(&destination).unwrap(), ORIGINAL);
    assert_eq!(scratch.files(), 1);
}

#[test]
fn in_place_replace_refuses_a_directory_destination_and_cleans_staging() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let pins = install(&state, ORIGINAL);
    let destination = scratch.0.join("a-directory");
    fs::create_dir(&destination).unwrap();
    let mut replace = request(&pins, 0, &destination);
    replace["replace"] = json!(true);
    assert!(save_request(
        &state,
        &replace.to_string(),
        |p| p.starts_with(&scratch.0),
        |_, _| Ok(())
    )
    .is_err());
    assert!(destination.is_dir());
    assert_eq!(scratch.files(), 1, "only the directory remains");
    replace["replace"] = json!("yes");
    assert!(save_request(
        &state,
        &replace.to_string(),
        |_| true,
        |_, _| panic!("a non-boolean replace reached an effect")
    )
    .is_err());
}

#[path = "native_object_save_failure_tests.rs"]
mod failure_tests;
