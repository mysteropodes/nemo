//! MCP clients read the lifecycle generation from the host status they already poll.
use super::tests::Fixture;
use crate::application_mcp::ApplicationMcp;
use serde_json::{json, Value};
use std::fs;

fn reported_generation(state: &ApplicationMcp) -> u64 {
    let status = state
        .native_status(
            serde_json::from_value(json!({"apiVersion":2,"requestId":"lifecycle-status",
                "instanceId":state.instance_id()}))
            .unwrap(),
        )
        .unwrap();
    status.validate().unwrap();
    assert!(status.available);
    status.lifecycle_generation.unwrap()
}

fn attach(state: &ApplicationMcp, root: &std::path::Path, generation: u64) -> Value {
    let id = state
        .native_state()
        .lock()
        .unwrap()
        .active()
        .unwrap()
        .1
        .document_id()
        .to_owned();
    let pin = state.native_project_info(&id).unwrap();
    json!({"apiVersion":2,"requestId":format!("attach-{generation}"),
        "instanceId":state.instance_id(),"documentId":pin["documentId"],
        "expectedRevision":pin["contentRevision"],"operation":"command.document.object.asset.attach",
        "payload":{"documentSnapshotId":pin["documentSnapshotId"],"lifecycleGeneration":generation,
            "name":"still.png","mediaType":"image/png",
            "source":{"kind":"file","path":root.join("still.png")},"placement":{"kind":"newLayer"}}})
}

/// MCP clients read the lifecycle generation from the status they already poll; after a
/// project replacement it must be exactly what the image attach route requires.
#[tokio::test]
async fn mcp_status_lifecycle_generation_follows_replacement_and_is_what_attach_requires() {
    let f = Fixture::new();
    let first = reported_generation(&f.state);
    assert_eq!(f.request["replace"]["lifecycleGeneration"], first);
    let (generation, id, _fingerprint, prepared) = f.execute();
    super::complete(&f.state.native_state(), generation, &id, prepared, || {
        Ok((vec![], "already_absent"))
    })
    .unwrap();
    let second = reported_generation(&f.state);
    assert_eq!(second, generation);
    assert!(second > first, "{first} -> {second}");

    let corpus =
        include_str!("../../native-engine/tests/fixtures/object-raster-asset-attach/oracle.txt");
    let row = corpus
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["name"] == "blue-2x1")
        .unwrap();
    let hex = row["hex"].as_str().unwrap();
    let png: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    fs::write(f.root.join("still.png"), png).unwrap();
    let root = f.root.clone();
    let allow = move || {
        let root = root.clone();
        std::sync::Arc::new(move |path: &std::path::Path| path.starts_with(&root)) as _
    };

    let stale = f
        .state
        .dispatch_native_scoped(
            serde_json::from_value(attach(&f.state, &f.root, first)).unwrap(),
            allow(),
        )
        .await
        .unwrap();
    assert!(!stale.ok);
    assert!(stale
        .error
        .unwrap()
        .message
        .contains("lifecycle generation is stale"));

    let accepted = f
        .state
        .dispatch_native_scoped(
            serde_json::from_value(attach(&f.state, &f.root, second)).unwrap(),
            allow(),
        )
        .await
        .unwrap();
    assert!(accepted.ok, "{:?}", accepted.error);
}
