//! Independent production-router/file/actual-owner persistence and refusal controls.
use crate::native_object_bootstrap::bootstrap_raw;
use crate::native_object_bootstrap::reopen::{bootstrap_request, MAX_FILE_BYTES};
use crate::{
    application_mcp::ApplicationMcp,
    native_application::{admit_release_request, complete_release},
    native_application_contract::NativeReleaseRequest,
    native_dispatch::ReleaseAdmission,
};
use serde_json::{json, Value};
use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("nemo-reopen-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn file(&self, bytes: &[u8]) -> PathBuf {
        let path = self.0.join("project.json");
        fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn document() -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases["records"]})
}
fn inline(state: &ApplicationMcp, bytes: &str) -> String {
    json!({"apiVersion":2,"requestId":"bytes","instanceId":state.instance_id(),
        "documentJson":bytes})
    .to_string()
}
fn request(state: &ApplicationMcp, path: &Path) -> Value {
    json!({"apiVersion":2,"requestId":"file","instanceId":state.instance_id(),
        "sourcePath":path.to_str().unwrap()})
}
fn load(state: &ApplicationMcp, request: &Value, root: &Path) -> Value {
    serde_json::to_value(
        bootstrap_request(state, &request.to_string(), |resolved| {
            resolved.starts_with(root)
        })
        .unwrap(),
    )
    .unwrap()
}
fn identity(state: &ApplicationMcp) -> Option<(u64, String, u64)> {
    state
        .native_state()
        .lock()
        .unwrap()
        .active()
        .map(|(g, a)| (g, a.document_id().into(), a.content_revision()))
}
fn refused(state: &ApplicationMcp, raw: &str, scope: impl Fn(&Path) -> bool, code: &str) {
    let before = identity(state);
    assert_eq!(bootstrap_request(state, raw, scope).unwrap_err().code, code);
    assert_eq!(identity(state), before);
}
fn owner(
    state: &ApplicationMcp,
    pins: &Value,
    operation: &str,
    revision: Option<u64>,
    payload: Value,
) -> Value {
    let mut value = json!({"apiVersion":2,"requestId":operation,
        "instanceId":pins["instanceId"],"documentId":pins["documentId"],
        "operation":operation,"payload":payload});
    if let Some(revision) = revision {
        value["expectedRevision"] = json!(revision);
    }
    let request = serde_json::from_value(value).unwrap();
    let native = state.native_state();
    let mut authority = native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    serde_json::to_value(
        authority
            .active_mut(generation)
            .unwrap()
            .dispatch(request)
            .unwrap(),
    )
    .unwrap()
}
fn serialize(state: &ApplicationMcp, pins: &Value, revision: u64, expected: &Value) -> String {
    let response = owner(
        state,
        pins,
        "query.document.object.serialize",
        None,
        json!({"atRevision":revision}),
    );
    assert_eq!(response["ok"], true);
    assert_eq!(response["contentRevision"], revision);
    let bytes = response["result"]["documentJson"].as_str().unwrap();
    assert_eq!(serde_json::from_str::<Value>(bytes).unwrap(), *expected);
    bytes.into()
}
fn release_request(pins: &Value, revision: u64) -> NativeReleaseRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"release",
        "instanceId":pins["instanceId"],"documentId":pins["documentId"],
        "expectedRevision":revision}))
    .unwrap()
}
fn begin_release(state: &ApplicationMcp, request: &NativeReleaseRequest) -> u64 {
    match admit_release_request(&state.native_state(), request).unwrap() {
        ReleaseAdmission::Execute { generation } => generation,
        _ => panic!("unexpected retained release"),
    }
}

#[test]
fn saved_actual_fill_history_bytes_reopen_through_production_router_and_same_owner_factory() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let old = serde_json::to_value(
        bootstrap_raw(&state, &inline(&state, &document().to_string())).unwrap(),
    )
    .unwrap();
    let mut edited = document();
    edited["objects"][0]["fill"] = json!({"kind":"solid","r":0.9,"g":0.1,"b":0.3,"a":0.5});
    for (op, revision, payload, next) in [
        (
            "command.document.object.fill.set",
            0,
            json!({"command":"object.fill.set",
            "stableTarget":document()["objects"][0]["target"],"fill":edited["objects"][0]["fill"]}),
            1,
        ),
        (
            "command.document.object.undo",
            1,
            json!({"command":"object.undo"}),
            2,
        ),
        (
            "command.document.object.redo",
            2,
            json!({"command":"object.redo"}),
            3,
        ),
    ] {
        let response = owner(&state, &old, op, Some(revision), payload);
        assert_eq!(response["ok"], true);
        assert_eq!(response["contentRevision"], next);
    }
    let bytes = serialize(&state, &old, 3, &edited);
    let path = scratch.file(bytes.as_bytes());
    assert_eq!(fs::read(&path).unwrap(), bytes.as_bytes());
    let release = release_request(&old, 3);
    let generation = begin_release(&state, &release);
    complete_release(&state.native_state(), generation, &release, || {
        Ok((vec![], "already_absent"))
    })
    .unwrap();
    let fresh = load(&state, &request(&state, &path), &scratch.0);
    assert_eq!(fresh["apiVersion"], 2);
    assert_eq!(fresh["requestId"], "file");
    assert_eq!(fresh["instanceId"], state.instance_id());
    assert_ne!(fresh["documentId"], old["documentId"]);
    assert_eq!(fresh["contentRevision"], 0);
    assert_eq!(fresh["resourceCount"], 0);
    assert_eq!(fresh["viewportAvailable"], false);
    assert_eq!(serialize(&state, &fresh, 0, &edited), bytes);
    for action in ["undo", "redo"] {
        assert_eq!(
            owner(
                &state,
                &fresh,
                &format!("command.document.object.{action}"),
                Some(0),
                json!({"command":format!("object.{action}")})
            )["error"]["code"],
            "unavailable"
        );
    }
}

#[test]
fn strict_file_headers_and_cancel_refuse_before_permission_or_reservation() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let path = scratch.file(document().to_string().as_bytes());
    let valid = request(&state, &path);
    let raw = valid.to_string();
    let mut invalid = vec!["[]".into(), format!("{raw} trailing")];
    for (key, value) in [
        ("documentJson", json!(document().to_string())),
        ("extra", json!(true)),
        ("documentId", json!("stale-document")),
        ("expectedRevision", json!(0)),
        ("sourcePath", json!("relative.json")),
        ("sourcePath", json!(null)),
        ("requestId", json!("invalid id")),
        ("apiVersion", json!(1)),
        ("instanceId", json!("other-instance")),
    ] {
        let mut bad = valid.clone();
        bad[key] = value;
        invalid.push(bad.to_string());
    }
    for key in [
        "apiVersion",
        "requestId",
        "instanceId",
        "sourcePath",
        "cancelledBeforeDispatch",
    ] {
        let with_cancel = raw.replacen("{", "{\"cancelledBeforeDispatch\":false,", 1);
        for encoded in [
            key.to_string(),
            format!("\\u{:04x}{}", key.as_bytes()[0], &key[1..]),
        ] {
            invalid.push(with_cancel.replacen(
                &format!("\"{key}\":"),
                &format!("\"{encoded}\":null,\"{key}\":"),
                1,
            ));
        }
    }
    for bad in invalid {
        assert!(
            bootstrap_request(&state, &bad, |_| panic!("invalid header reached scope")).is_err()
        );
        assert!(identity(&state).is_none());
    }
    let mut cancelled = valid;
    cancelled["cancelledBeforeDispatch"] = json!(true);
    refused(
        &state,
        &cancelled.to_string(),
        |_| panic!("cancelled before IO"),
        "cancelled_before_dispatch",
    );
}

#[test]
fn exact_file_wrapper_limit_and_original_byte_mode_keep_independent_budgets() {
    let scratch = Scratch::new();
    let path = scratch.file(document().to_string().as_bytes());
    let state = ApplicationMcp::default();
    let raw = request(&state, &path).to_string();
    let exact = format!("{raw}{}", " ".repeat(4096 - raw.len()));
    assert!(bootstrap_request(&state, &exact, |p| p.starts_with(&scratch.0)).is_ok());
    let denied = ApplicationMcp::default();
    let raw = request(&denied, &path).to_string();
    let oversized = format!("{raw}{}", " ".repeat(4097 - raw.len()));
    refused(
        &denied,
        &oversized,
        |_| panic!("oversized file request"),
        "invalid_request",
    );
    let bytes = inline(&denied, &document().to_string());
    assert!(
        bootstrap_request(&denied, &bytes, |_| panic!("byte mode does not read files")).is_ok()
    );
}

#[cfg(unix)]
#[test]
fn resolved_config_scope_denies_outside_file_and_symlink_escape_before_open() {
    use std::os::unix::fs::symlink;
    let allowed = Scratch::new();
    let outside = Scratch::new();
    let state = ApplicationMcp::default();
    let path = outside.file(document().to_string().as_bytes());
    let alias = allowed.0.join("alias.json");
    symlink(&path, &alias).unwrap();
    let calls = Cell::new(0);
    for source in [&path, &alias] {
        refused(
            &state,
            &request(&state, source).to_string(),
            |resolved| {
                calls.set(calls.get() + 1);
                assert_eq!(resolved, path.as_path());
                resolved.starts_with(&allowed.0)
            },
            "unavailable",
        );
    }
    assert_eq!(calls.get(), 2);
    let owned = allowed.file(document().to_string().as_bytes());
    refused(
        &state,
        &request(&state, &owned).to_string(),
        |_| false,
        "unavailable",
    );
}

#[test]
fn file_io_utf8_codec_and_complete_wrapper_failures_never_install_partial_owner() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let mut unsupported = document();
    unsupported["formatVersion"] = json!(99);
    for bytes in [
        vec![0xff],
        b"{}".to_vec(),
        unsupported.to_string().into_bytes(),
        vec![b' '; MAX_FILE_BYTES as usize + 1],
    ] {
        let path = scratch.file(&bytes);
        refused(
            &state,
            &request(&state, &path).to_string(),
            |_| true,
            "invalid_request",
        );
    }
    let text = document().to_string();
    let full = format!("{text}{}", " ".repeat(MAX_FILE_BYTES as usize - text.len()));
    let path = scratch.file(full.as_bytes());
    refused(
        &state,
        &request(&state, &path).to_string(),
        |_| true,
        "invalid_request",
    );
    refused(
        &state,
        &request(&state, &scratch.0).to_string(),
        |_| true,
        "invalid_request",
    );
    refused(
        &state,
        &request(&state, &scratch.0.join("missing")).to_string(),
        |_| true,
        "unavailable",
    );
}

#[cfg(unix)]
#[test]
fn unreadable_regular_file_returns_io_failure_without_owner_effect() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let path = scratch.file(document().to_string().as_bytes());
    fs::set_permissions(&path, fs::Permissions::from_mode(0)).unwrap();
    refused(
        &state,
        &request(&state, &path).to_string(),
        |_| true,
        "unavailable",
    );
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn active_and_releasing_owner_refusals_preserve_real_history_and_revision_subscriber() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let pins = serde_json::to_value(
        bootstrap_raw(&state, &inline(&state, &document().to_string())).unwrap(),
    )
    .unwrap();
    let mut edited = document();
    edited["objects"][0]["fill"] = json!({"kind":"solid","r":0.8,"g":0.2,"b":0.4,"a":0.6});
    let response = owner(
        &state,
        &pins,
        "command.document.object.fill.set",
        Some(0),
        json!({"command":"object.fill.set","stableTarget":document()["objects"][0]["target"],
            "fill":edited["objects"][0]["fill"]}),
    );
    assert_eq!(response["ok"], true);
    assert_eq!(response["contentRevision"], 1);
    let binding = state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap();
    let path = scratch.file(document().to_string().as_bytes());
    refused(
        &state,
        &request(&state, &path).to_string(),
        |_| true,
        "unavailable",
    );
    assert!(state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap_err()
        .contains("occupied"));
    serialize(&state, &pins, 1, &edited);
    let release = release_request(&pins, 1);
    let generation = begin_release(&state, &release);
    refused(
        &state,
        &request(&state, &path).to_string(),
        |_| true,
        "unavailable",
    );
    let terminal = complete_release(&state.native_state(), generation, &release, || {
        Ok((vec![], "already_absent"))
    })
    .unwrap();
    assert_eq!(terminal.document_id, pins["documentId"].as_str().unwrap());
    assert_eq!(
        (
            terminal.content_revision,
            terminal.undo_depth,
            terminal.redo_depth
        ),
        (1, 1, 0)
    );
}
