# Animated opacity application/MCP slice

Owner: Codexitron (contract, Rust adapter, native integration); Codeximator
(application/domain extraction and real consumers). Reviewer: Codexalog.
Issues: #905, #907, #908, #909, #910. Base: retained R08 `582f51e`.

The live application document remains the sole writable authority. The Rust
adapter validates and transports requests to `NemoApplication.handle`; it does
not mirror document state or evaluate scripts. UI, timeline, direct tests,
diagnostic replay and MCP share that application service and history.

Rust declarations in `nemo-mcp/src/contract.rs` generate the versioned transport
schema with `cargo run --manifest-path nemo-mcp/Cargo.toml --bin nemo-mcp-schema`.
Each write requires instance, document, expected revision and request identity.
The application checks revision and idempotency at the actual mutation boundary.
Reusing an identical request returns its retained result; changing its body fails.
Queries return identity/revision without granting a later stale write.

The bundled `nemo-mcp` executable uses the official Rust SDK, pinned at 3.2.0 in
Cargo and locked dependencies. Its compact tools discover running instances,
query capabilities/snapshots/properties/trace, and dispatch typed commands.
Desktop attachment is local; discovery secrets are excluded from client results.
Disconnect/cancellation does not prove that an already committed command rolled
back. Clients query state and retry only the original identified command.

The first aggregate is layer Motion opacity, including static and keyed values,
UI/timeline selection, history, save/reload, evaluation, rendering and export.
New modules obey enforced application/domain/adapter size and dependency limits.
Permanent independent-expectation regressions accompany extraction.

Final acceptance requires identified installed executable bytes and actual
Codex/Claude discovery, edit, undo/redo, persistence, render/export, reconnect,
cancellation and stale-write evidence. A compiled crate alone is not acceptance.
Browser and desktop results are recorded separately. The fixed F0 PR #983
baseline retains its known failures; this slice changes no baseline expectations.
