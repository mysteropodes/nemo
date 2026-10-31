//! #2510: which path a project request admits, and where it must lie.
use crate::application_mcp::project_scope::{
    admit_open_file_with, inside, refusal, target, ApplicationRequest, Target,
};
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

// #2269: a recent entry or a recovery generation is not dialog-picked, so the open routes (which consult only the
// dynamic scope) refused it. It is admitted as one file, inside the static roots, exactly like an MCP project.open.
#[test]
fn an_unpicked_project_file_inside_the_roots_is_admitted_alone() {
    let dir = temp();
    let file = dir.join("Recent.json");
    let sibling = dir.join("Sibling.json");
    std::fs::write(&file, "{}").unwrap();
    std::fs::write(&sibling, "{}").unwrap();
    let app = tauri::test::mock_app();
    let scope = tauri::fs::Scope::new(&app, &Default::default()).unwrap();
    assert!(!scope.is_allowed(&file), "not granted before admission");
    let roots = [dir.parent().unwrap().to_path_buf()];
    let admitted = admit_open_file_with(file.to_str().unwrap(), &roots, |path| {
        scope.allow_file(path).map_err(|error| error.to_string())
    })
    .unwrap();
    assert_eq!(admitted, file);
    assert!(scope.is_allowed(&file));
    assert!(!scope.is_allowed(&sibling), "only the named file is granted");
}

#[test]
fn an_unpicked_project_path_that_is_missing_relative_outside_or_not_a_file_is_not_admitted() {
    let dir = temp();
    let file = dir.join("Inside.json");
    std::fs::write(&file, "{}").unwrap();
    let roots = [dir.clone()];
    let refuse = |raw: &str, roots: &[PathBuf]| {
        let mut granted = false;
        let result = admit_open_file_with(raw, roots, |_| {
            granted = true;
            Ok(())
        });
        assert!(result.is_err(), "{raw}");
        assert!(!granted, "nothing is granted for {raw}");
    };
    refuse(dir.join("absent.json").to_str().unwrap(), &roots);
    refuse("relative/Recent.json", &roots);
    refuse(&format!("{}/../x.json", dir.display()), &roots);
    refuse(dir.to_str().unwrap(), &roots);
    refuse(file.to_str().unwrap(), &[PathBuf::from("/nonexistent-root")]);
    refuse("/etc/hosts", &roots);
    refuse("/tmp/a\0b.json", &roots);
}
