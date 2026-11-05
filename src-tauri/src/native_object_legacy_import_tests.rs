//! A baseline project opens as a new native document through the one bootstrap command, never touching the legacy file.
use super::{reopen, replace, replace::Admission};
use crate::application_mcp::ApplicationMcp;
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

const ORIGINAL: &str = include_str!("../../tests/fixtures/legacy-import/original.json");
const MOTION: &str = include_str!("../../tests/fixtures/migration/project.json");

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("nemo-legacy-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, text).unwrap();
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn open_blank() -> (ApplicationMcp, Value) {
    let state = ApplicationMcp::default();
    let (id, _, _) = state.install_native_blank_project(64, 48, 24.0).unwrap();
    let info = state.native_project_info(&id).unwrap();
    (state, info)
}
fn replacing(state: &ApplicationMcp, info: &Value, path: &PathBuf) -> Value {
    json!({"apiVersion":2,"requestId":"legacy-open","instanceId":state.instance_id(),"sourcePath":path,"projectOnly":true,
        "replace":{"documentId":info["documentId"],"expectedRevision":info["contentRevision"],
            "documentSnapshotId":info["documentSnapshotId"],"lifecycleGeneration":info["lifecycleGeneration"]}})
}

#[test]
fn a_baseline_project_replaces_the_open_document_as_a_new_untitled_native_one() {
    let (state, before) = open_blank();
    let scratch = Scratch::new();
    let path = scratch.file("old-film.json", ORIGINAL);
    let request = replacing(&state, &before, &path);
    let admission = replace::prepare(
        &state,
        reopen::file_request(&request.to_string()).unwrap(),
        |p| p.starts_with(&scratch.0),
    )
    .unwrap();
    let Admission::Execute {
        generation,
        request_id,
        prepared,
        legacy,
        ..
    } = admission
    else {
        panic!("expected one execution")
    };
    let report = legacy.expect("a baseline project carries its report");
    assert_eq!(
        report["counts"],
        json!({"layers": 2, "keyframes": 4, "strokes": 4})
    );
    assert_eq!(report["warningsTotal"], 0);
    let receipt = replace::complete(
        &state.native_state(),
        generation,
        &request_id,
        prepared,
        || Ok((vec![], "already_absent")),
    )
    .unwrap();
    let receipt = serde_json::to_value(receipt).unwrap();
    assert!(
        receipt.get("legacyImport").is_none(),
        "the retained replay receipt never holds the report"
    );
    let after = state
        .native_project_info(receipt["documentId"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        (
            after["canvasW"].clone(),
            after["canvasH"].clone(),
            after["totalFrames"].clone(),
            after["contentRevision"].clone(),
            after["undoDepth"].clone()
        ),
        (json!(640), json!(360), json!(24), json!(0), json!(0))
    );
    assert_eq!(after["layers"].as_array().unwrap().len(), 2);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        ORIGINAL,
        "the legacy file is never modified"
    );
}

#[test]
fn a_refused_baseline_project_names_its_field_and_leaves_the_open_document_untouched() {
    let (state, before) = open_blank();
    let scratch = Scratch::new();
    let path = scratch.file("with-motion.json", MOTION);
    let request = replacing(&state, &before, &path);
    let error = replace::prepare(
        &state,
        reopen::file_request(&request.to_string()).unwrap(),
        |p| p.starts_with(&scratch.0),
    )
    .err()
    .unwrap();
    assert_eq!(error.code, "replacement_refused");
    let details = error.details.unwrap();
    assert_eq!(
        (
            details["disposition"].clone(),
            details["reasonCode"].clone()
        ),
        (json!("notDispatched"), json!("legacy_unsupported_field"))
    );
    assert!(
        error.message.contains("layers[0].motion"),
        "{}",
        error.message
    );
    assert_eq!(
        state
            .native_project_info(before["documentId"].as_str().unwrap())
            .unwrap(),
        before
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), MOTION);
}

#[test]
fn the_first_open_installs_a_baseline_project_and_other_json_stays_unsupported() {
    let state = ApplicationMcp::default();
    let scratch = Scratch::new();
    let allowed = |p: &std::path::Path| p.starts_with(&scratch.0);
    let path = scratch.file("old-film.json", ORIGINAL);
    let request = json!({"apiVersion":2,"requestId":"first","instanceId":state.instance_id(),"sourcePath":path,"projectOnly":true});
    let receipt = reopen::bootstrap_request(&state, &request.to_string(), allowed).unwrap();
    let report = serde_json::to_value(&receipt).unwrap()["legacyImport"].clone();
    assert_eq!(report["counts"]["strokes"], 4);
    let info = state
        .native_project_info(
            &serde_json::to_value(&receipt).unwrap()["documentId"]
                .as_str()
                .unwrap()
                .to_owned(),
        )
        .unwrap();
    assert_eq!(
        (info["canvasW"].clone(), info["undoDepth"].clone()),
        (json!(640), json!(0))
    );
    let other = scratch.file("other.json", r#"{"hello":"world"}"#);
    let again = json!({"apiVersion":2,"requestId":"second","instanceId":state.instance_id(),"sourcePath":other,"projectOnly":true});
    assert_eq!(
        reopen::bootstrap_request(&state, &again.to_string(), allowed)
            .unwrap_err()
            .code,
        "unsupported_format"
    );
}

#[test]
fn a_baseline_shaped_file_without_layers_is_a_typed_legacy_refusal_and_installs_nothing() {
    let state = ApplicationMcp::default();
    let scratch = Scratch::new();
    let path = scratch.file("empty.json", r#"{"layers":[],"canvasW":320}"#);
    let request = json!({"apiVersion":2,"requestId":"empty","instanceId":state.instance_id(),"sourcePath":path,"projectOnly":true});
    let error =
        reopen::bootstrap_request(&state, &request.to_string(), |p| p.starts_with(&scratch.0))
            .unwrap_err();
    assert_eq!(error.code, "legacy_invalid");
    assert!(state.native_state().lock().unwrap().active().is_none());
}
