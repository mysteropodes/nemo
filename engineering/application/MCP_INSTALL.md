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

### What `nemo_discover` answers

The default answer is compact (at most 12,000 characters for one instance with one open
document): the running instances (`instanceId`, `pid`, `buildId`, open `documentId`,
`documentKind`, `revision`) and a `startHere` guide for the open document's kind, with one
request to copy for each step: take the snapshot pin, read the document, objects and
properties, edit a property, undo and redo, history, save, open and export. Detail is
requested through arguments of the same tool, all additive:

| Arguments | Answer |
|---|---|
| `{}` | instances and the start-here guide |
| `{"operation": "<name>"}` (plus `apiVersion` for `history.undo`/`history.redo`, which exist on both routes) | that operation's full contract: request schema, example with the live identity, result shape, refusal codes, availability for the open document |
| `{"detail": "operations", "family": "<family>", "page": <n>}` | operation names with a one-line purpose and availability for the open document, 60 per page, optionally by family |
| `{"detail": "full"}` | every registered declaration verbatim (about 280,000 characters) |
| `{"instanceId": "<id>"}` | the same, for another running instance |

Availability is truthful for the open document's kind. Each native capability declares the
kinds it applies to (`documentKinds`), a one-line `purposes` per operation and, for operations
that exist on another kind only, `useInstead`; the desktop host reports the kind of the open
document in its native status and an operation declared for another kind is refused by the
owner with the operation to use instead. The apiVersion 1 list comes from the instance's own
`capabilities` answer plus the `project.*` operations, which every kind accepts. A desktop
test sends every operation reported available for an object document through the shared
application route and requires that none is refused as unsupported.

Validation of the builder and its controlled fixtures only proves target naming,
locked Cargo invocation, stale-artifact cleanup, and Tauri wiring. A future
acceptance pass must launch a freshly installed package and exercise both
Codex and Claude against the installed executable separately.
