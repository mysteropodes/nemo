# Nemo MCP installation

The packaged desktop application contains one production Rust executable,
`nemo-mcp`, beside the application executable. Tauri selects the target named
binary from `src-tauri/binaries/nemo-mcp-<target-triple>` (and the `.exe`
suffix on Windows) during the build; the application then invokes that bundled
executable. The build script uses the existing `src-tauri/nemo-mcp` crate and
`cargo build --locked`, so an installed client has no Node, Python, download,
or source checkout dependency.

Codex and Claude should each register the installed executable using their
normal MCP command configuration, for example:

```json
{
  "mcpServers": {
    "nemo": {
      "command": "/Applications/Nemo.app/Contents/MacOS/nemo-mcp"
    }
  }
}
```

Use the corresponding installed application path on Windows or Linux. Keep
the `nemo` registry entry local to each client; do not share or overwrite the
other client's registry. This package does not edit either user's Codex or
Claude configuration.

Validation of the builder and its controlled fixtures only proves target naming,
locked Cargo invocation, stale-artifact cleanup, and Tauri wiring. A future
acceptance pass must launch a freshly installed package and exercise both
Codex and Claude against the installed executable separately.
