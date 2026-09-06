//! Protocol regression against the compiled executable, with isolated discovery.
//! Installed Codex/Claude acceptance is a separate gate.
use rmcp::{model::CallToolRequestParams, transport::TokioChildProcess, ServiceExt};
use serde_json::json;

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
        Some(json!({"apiVersion":1,"instances":[]}))
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
