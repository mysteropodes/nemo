use crate::native_object_bootstrap::reference::reference_request;
use crate::{application_mcp::ApplicationMcp, native_object_bootstrap::bootstrap_raw};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

const BLUE:&str="89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000d49444154789c636060f80f060014f405fbb956037d0000000049454e44ae426082";
const RED:&str="89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000e49444154789c63f8cfc0f01f040110f803fd4e95c16f0000000049454e44ae426082";
struct Fixture {
    state: ApplicationMcp,
    document: String,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nemo-ref-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let state = ApplicationMcp::default();
        let source = json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":320,"canvasH":180,"fps":24},
            "drawing":{"format":"nemo.native-object-document","formatVersion":1,
                "totalFrames":21,"layers":[{"layerUid":"drawing"}],"objects":[]},
            "viewportReference":null,"assets":[]});
        let opened = bootstrap_raw(
            &state,
            &json!({"apiVersion":2,"requestId":"open",
            "instanceId":state.instance_id(),"documentJson":source.to_string()})
            .to_string(),
        )
        .unwrap();
        let opened = serde_json::to_value(opened).unwrap();
        Self {
            state,
            document: opened["documentId"].as_str().unwrap().into(),
            root,
        }
    }
    fn info(&self) -> Value {
        self.state.native_project_info(&self.document).unwrap()
    }
    fn png(&self, name: &str, hex: &str) -> PathBuf {
        let bytes: Vec<_> = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
            .collect();
        let path = self.root.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
    fn request(&self, action: Value) -> Value {
        let info = self.info();
        json!({"apiVersion":2,"requestId":uuid::Uuid::new_v4().to_string(),
            "instanceId":self.state.instance_id(),"documentId":self.document,
            "lifecycleGeneration":info["lifecycleGeneration"],"expectedRevision":info["contentRevision"],
            "documentSnapshotId":info["documentSnapshotId"],"action":action})
    }
    fn run(&self, action: Value) -> Value {
        let request = self.request(action);
        let response = reference_request(&self.state, &request.to_string(), |p| {
            p.parent() == Some(self.root.as_path())
        })
        .unwrap();
        assert_eq!(response["ok"], true, "{response}");
        response
    }
    fn history(&self, action: &str) {
        let info = self.info();
        let request = json!({"apiVersion":2,"requestId":uuid::Uuid::new_v4().to_string(),
            "instanceId":self.state.instance_id(),"documentId":self.document,
            "expectedRevision":info["contentRevision"],"operation":format!("command.document.object.{action}"),
            "payload":{"command":format!("object.{action}")}});
        assert!(
            self.state
                .serialize_object_file_snapshot(&request.to_string())
                .unwrap()
                .ok
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn actual_file_reference_controls_history_and_inspect_share_original_owner() {
    let f = Fixture::new();
    let original = f.info();
    let blue = f.png("blue.png", BLUE);
    let red = f.png("red.png", RED);
    f.run(json!({"kind":"attach","id":"blue","sourcePath":blue}));
    let first = f.info();
    assert_eq!(first["reference"]["opacity"], 0.5);
    assert_eq!(first["reference"]["name"], "blue.png");
    assert_eq!(first["documentId"], original["documentId"]);
    f.run(json!({"kind":"attach","id":"red","sourcePath":red}));
    let second = f.info();
    assert_eq!(second["contentRevision"], 2);
    f.history("undo");
    assert_eq!(f.info()["documentDigest"], first["documentDigest"]);
    f.history("redo");
    assert_eq!(f.info()["documentDigest"], second["documentDigest"]);
    f.run(json!({"kind":"settings","name":"red.png","visible":false,"opacity":0.25,"offsetFrames":-3}));
    let changed = f.info();
    assert_eq!(changed["reference"]["visible"], false);
    f.run(json!({"kind":"remove"}));
    assert!(f.info()["reference"].is_null());
    f.history("undo");
    assert_eq!(f.info()["documentDigest"], changed["documentDigest"]);
    let inspection = reference_request(
        &f.state,
        &json!({"apiVersion":2,"requestId":"inspect",
        "instanceId":f.state.instance_id(),"documentId":f.document,"action":{"kind":"inspect"}})
        .to_string(),
        |_| panic!("inspect has no filesystem effects"),
    )
    .unwrap();
    assert_eq!(inspection["requestId"], "inspect");
    assert_eq!(inspection["documentDigest"], changed["documentDigest"]);
    assert!(inspection.get("documentJson").is_none());
    assert!(inspection["reference"].get("encodedPng").is_none());
}

#[test]
fn stale_cancel_scope_bad_png_unknown_and_duplicate_members_have_zero_document_effects() {
    let f = Fixture::new();
    let png = f.png("blue.png", BLUE);
    let attach = f.request(json!({"kind":"attach","id":"ref","sourcePath":png}));
    let mut cancelled = attach.clone();
    cancelled["cancelledBeforeDispatch"] = true.into();
    let mut stale = attach.clone();
    stale["documentSnapshotId"] = "stale".into();
    let mut extra = attach.clone();
    extra["action"]["encodedPng"] = "injected".into();
    let before = f.info();
    for raw in [
        cancelled.to_string(),
        stale.to_string(),
        extra.to_string(),
        attach
            .to_string()
            .replacen("\"apiVersion\":2", "\"apiVersion\":2,\"apiVersion\":2", 1),
        attach.to_string().replacen(
            "\"kind\":\"attach\"",
            "\"kind\":\"attach\",\"kind\":\"attach\"",
            1,
        ),
    ] {
        assert!(reference_request(&f.state, &raw, |_| panic!(
            "invalid request reached file scope"
        ))
        .is_err());
        assert_eq!(f.info(), before);
    }
    assert!(reference_request(&f.state, &attach.to_string(), |_| false).is_err());
    assert_eq!(f.info(), before);
    fs::write(&png, b"not PNG").unwrap();
    let refused = reference_request(&f.state, &attach.to_string(), |_| true).unwrap();
    assert_eq!(refused["ok"], false);
    assert_eq!(f.info(), before);
    let directory = f.request(json!({"kind":"attach","id":"ref","sourcePath":f.root}));
    assert!(reference_request(&f.state, &directory.to_string(), |_| true).is_err());
    assert_eq!(f.info(), before);
}

#[test]
fn original_revision_barrier_blocks_reference_commit_until_external_history_is_acknowledged() {
    let f = Fixture::new();
    let png = f.png("blue.png", BLUE);
    f.run(json!({"kind":"attach","id":"ref","sourcePath":png}));
    let binding = f
        .state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    f.state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap();
    let undo = json!({"apiVersion":2,"requestId":"external-undo","instanceId":f.state.instance_id(),
        "documentId":f.document,"expectedRevision":1,"operation":"command.document.object.undo",
        "payload":{"command":"object.undo"}});
    let (response, event) = f
        .state
        .external_object_preview_for_test(&undo.to_string())
        .unwrap();
    assert!(response.ok);
    assert!(event.is_some());
    let pending = f.info();
    let request = f.request(json!({"kind":"attach","id":"ref","sourcePath":png}));
    assert!(reference_request(&f.state, &request.to_string(), |_| true).is_err());
    assert_eq!(f.info(), pending);
    assert_eq!(pending["contentRevision"], 2);
    assert!(pending["reference"].is_null());
}

#[test]
fn file_alias_scope_uses_resolved_path_and_wrong_generation_cannot_commit() {
    let f = Fixture::new();
    let png = f.png("blue.png", BLUE);
    #[cfg(unix)]
    {
        let alias = f.root.join("alias.png");
        std::os::unix::fs::symlink(&png, &alias).unwrap();
        let request = f.request(json!({"kind":"attach","id":"ref","sourcePath":alias}));
        let result = reference_request(&f.state, &request.to_string(), |p: &Path| {
            assert_eq!(p, png);
            true
        })
        .unwrap();
        assert_eq!(result["ok"], true);
    }
    let before = f.info();
    let mut request = f.request(json!({"kind":"remove"}));
    request["lifecycleGeneration"] = json!(99999);
    assert!(reference_request(&f.state, &request.to_string(), |_| true).is_err());
    assert_eq!(f.info(), before);
}
