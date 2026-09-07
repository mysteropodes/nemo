use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // stdout belongs exclusively to the MCP protocol.
    let server = nemo_mcp::server::NemoServer::new(nemo_mcp::registry::registry_root()?);
    server
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}
