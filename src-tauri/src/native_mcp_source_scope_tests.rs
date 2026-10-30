//! #2242: which image paths an MCP request admits into the fs scope, and where they must lie.
use crate::application_mcp::source_scope::{admissible, named_sources};
use nemo_mcp::contract::NativeApplicationRequest;
use serde_json::json;
use std::path::PathBuf;

fn request(operation: &str, payload: serde_json::Value) -> NativeApplicationRequest {
    serde_json::from_value(
        json!({"apiVersion": 2, "requestId": "r", "instanceId": "i", "documentId": "d",
        "expectedRevision": 0, "operation": operation, "payload": payload}),
    )
    .unwrap()
}

fn temp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nemo-source-scope-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

#[test]
fn each_image_route_names_its_file_sources_and_fonts_or_other_commands_name_none() {
    let file = json!({"kind": "file", "path": "/a/one.png"});
    let attach = request(
        "command.document.object.asset.attach",
        json!({"source": file}),
    );
    assert_eq!(named_sources(&attach), vec!["/a/one.png"]);
    let stroke = request(
        "command.document.object.brush.bitmap.stroke",
        json!({"source": file}),
    );
    assert_eq!(named_sources(&stroke), vec!["/a/one.png"]);
    let batch = request(
        "command.document.object.import.batch",
        json!({"sources": [
            {"sourceId": "a", "mediaType": "image/png", "source": {"kind": "file", "path": "/a/two.png"}},
            {"sourceId": "b", "mediaType": "font/ttf", "source": {"kind": "file", "path": "/a/f.ttf"}}]}),
    );
    assert_eq!(named_sources(&batch), vec!["/a/two.png"]);
    assert!(named_sources(&request(
        "command.document.object.item.edit",
        json!({"source": file})
    ))
    .is_empty());
    let other = request(
        "command.document.object.asset.attach",
        json!({"source": {"kind": "url", "path": "/a"}}),
    );
    assert!(named_sources(&other).is_empty());
}

#[test]
fn only_an_existing_absolute_regular_file_strictly_inside_a_root_is_admitted() {
    let dir = temp();
    let file = dir.join("still.png");
    std::fs::write(&file, b"png").unwrap();
    let roots = [dir.clone()];
    assert_eq!(
        admissible(file.to_str().unwrap(), &roots),
        Some(file.clone())
    );
    assert_eq!(
        admissible(dir.to_str().unwrap(), &roots),
        None,
        "never a folder or a whole root"
    );
    assert_eq!(
        admissible(dir.join("absent.png").to_str().unwrap(), &roots),
        None
    );
    assert_eq!(admissible("still.png", &roots), None, "relative");
    assert_eq!(
        admissible(file.to_str().unwrap(), &[PathBuf::from("/elsewhere")]),
        None,
        "outside every root"
    );
}
