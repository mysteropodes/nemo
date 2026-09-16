//! Protocol regression against the compiled executable, with isolated discovery.
//! Installed Codex/Claude acceptance is a separate gate.
use rmcp::{model::CallToolRequestParams, transport::TokioChildProcess, ServiceExt};
use serde_json::{json, Value};

/// Expected set, READ from the crate's declaration rather than copied.
/// A copied list drifts silently: #1310 added one descriptor and four
/// hand-maintained copies failed at once, in a crate no job ran (#1318).
/// The assertion is unchanged — the spawned executable must advertise exactly
/// what the crate declares — but there is no longer a second list to forget.
fn registered_capabilities() -> Value {
    Value::Array(
        nemo_mcp::capabilities::CAPABILITY_SOURCES
            .iter()
            .map(|source| serde_json::from_str::<Value>(source).unwrap())
            .collect(),
    )
}

#[tokio::test]
async fn executable_discovers_tools_and_reports_no_running_app() {
    let root = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
    command.env("NEMO_MCP_REGISTRY", root.path());
    let transport = TokioChildProcess::new(command).unwrap();
    let client = ().serve(transport).await.unwrap();
    let mut tools = client
        .list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect::<Vec<_>>();
    tools.sort();
    assert_eq!(tools, ["nemo_command", "nemo_discover", "nemo_query"]);
    let discover = client
        .call_tool(CallToolRequestParams::new("nemo_discover"))
        .await
        .unwrap();
    assert_eq!(
        discover.structured_content,
        Some(json!({
            "apiVersion": 1,
            "registeredCapabilities": registered_capabilities(),
            "instances": []
        }))
    );
    let unavailable = client
        .call_tool(
            CallToolRequestParams::new("nemo_query").with_arguments(
                json!({"instance_id":"missing", "operation":"snapshot"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(unavailable.is_error, Some(true));
    assert_eq!(
        unavailable.structured_content.unwrap()["error"]["code"],
        "unavailable"
    );
    client.cancel().await.unwrap();
}
