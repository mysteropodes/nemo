# Nemo MCP installation

The packaged desktop application contains one production Rust executable,
`nemo-mcp`, beside the application executable. Tauri selects the target named
binary from `src-tauri/binaries/nemo-mcp-<target-triple>` (and the `.exe`
suffix on Windows) during the build; MCP clients invoke that bundled
executable. The build script uses the `nemo-mcp` crate and
`cargo build --locked`, so an installed client has no Node, Python, download,
or source checkout dependency.

Claude's JSON MCP configuration can point directly at the installed executable:

```json
{
  "mcpServers": {
    "nemo": {
      "command": "/Applications/Nemo.app/Contents/MacOS/nemo-mcp"
    }
  }
}
```

Use the corresponding installed application path on Windows or Linux. Configure
Codex with the same executable in its native MCP configuration format. This
package does not edit either user's client configuration.

Nemo and the MCP executable discover each other through per-user local application
data under `com.strokemotion.app/mcp`. To validate an isolated candidate, set
`NEMO_MCP_REGISTRY` to the same dedicated directory for that app and both clients.
Do not share its connection records; they contain local attachment secrets.
Queries return instance/build/document/revision identity without those secrets.

Validation of the builder and its controlled fixtures only proves target naming,
locked Cargo invocation, stale-artifact cleanup, and Tauri wiring. A future
acceptance pass must launch a freshly installed package and exercise both
Codex and Claude against the installed executable separately.
