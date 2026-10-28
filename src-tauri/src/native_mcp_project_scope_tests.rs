//! #2510: which path a project request admits, and where it must lie.
use crate::application_mcp::project_scope::{inside, refusal, target, ApplicationRequest, Target};
use serde_json::json;
use std::path::PathBuf;

fn request(operation: &str, payload: serde_json::Value) -> ApplicationRequest {
    serde_json::from_value(
        json!({"apiVersion": 1, "requestId": "r", "instanceId": "i", "documentId": "d",
        "expectedRevision": 0, "operation": operation, "payload": payload}),
    )
    .unwrap()
}

fn temp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nemo-project-scope-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

#[test]
fn each_project_request_names_its_exact_file_or_folder() {
    let dir = temp();
    let file = dir.join("A.json");
    std::fs::write(&file, "{}").unwrap();
    let open = target(&request("project.open", json!({"path": file})))
        .unwrap()
        .unwrap();
    assert_eq!(open, Target::File(file.clone()));
    let save = target(&request(
        "project.saveAs",
        json!({"path": dir.join("copy.json")}),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        save,
        Target::File(dir.join("copy.json")),
        "a new file is placed under its existing parent"
    );
    let png = target(&request(
        "project.export",
        json!({"format": "png", "destination": dir.join("frames")}),
    ));
    assert_eq!(png.unwrap().unwrap(), Target::Directory(dir.join("frames")));
    let lottie = target(&request(
        "project.export",
        json!({"format": "lottie", "destination": dir.join("out.json")}),
    ));
    assert_eq!(lottie.unwrap().unwrap(), Target::File(dir.join("out.json")));
    assert!(target(&request(
        "project.new",
        json!({"canvasW": 320, "canvasH": 180, "fps": 24})
    ))
    .is_none());
    assert!(target(&request("history.undo", json!({}))).is_none());
}

#[test]
fn missing_relative_and_traversing_paths_refuse() {
    let dir = temp();
    for path in [
        dir.join("absent.json"),
        dir.join("..").join("x.json"),
        dir.join("missing-dir").join("x.json"),
    ] {
        assert!(
            target(&request("project.open", json!({"path": path})))
                .unwrap()
                .is_err(),
            "{path:?}"
        );
    }
    assert!(target(&request(
        "project.saveAs",
        json!({"path": dir.join("..").join("x.json")})
    ))
    .unwrap()
    .is_err());
}

#[test]
fn only_paths_strictly_inside_a_root_are_admitted() {
    let roots = [PathBuf::from("/Users/example"), PathBuf::from("/tmp")];
    assert!(inside(
        &Target::File(PathBuf::from("/Users/example/Documents/A.json")),
        &roots
    ));
    assert!(inside(
        &Target::Directory(PathBuf::from("/tmp/frames")),
        &roots
    ));
    assert!(
        !inside(&Target::Directory(PathBuf::from("/Users/example")), &roots),
        "never a whole root"
    );
    assert!(!inside(&Target::File(PathBuf::from("/etc/passwd")), &roots));
    assert!(
        !inside(
            &Target::File(PathBuf::from("/Users/examples/x.json")),
            &roots
        ),
        "a sibling prefix is not inside"
    );
}

#[test]
fn a_refused_path_answers_a_typed_reply_with_the_request_identity() {
    let reply = refusal(
        &request("project.open", json!({"path": "/a.json"})),
        "outside".into(),
    );
    let value = serde_json::to_value(&reply).unwrap();
    assert_eq!(value["requestId"], "r");
    assert_eq!(value["instanceId"], "i");
    assert_eq!(value["documentId"], "d");
    assert_eq!(value["ok"], false);
    assert_eq!(
        value["error"],
        json!({"code": "invalid_request", "message": "outside"})
    );
}
