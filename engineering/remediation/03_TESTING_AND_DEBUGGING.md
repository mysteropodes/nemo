<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Testing, regression prevention and debugging

Status: **proposed quality contract**. Nemo currently has Node and Cargo tests. The expanded
stack and commands below are not complete until implemented and accepted.

## Runner decision

Do not build a custom general-purpose test framework. Preserve the existing Node test runner
and Cargo tests during migration. Trial Node and Vitest on the same extracted TypeScript module,
stateful command/undo sequence and browser control:

| Candidate | Best fit | Decision concern |
|---|---|---|
| Node test runner | low dependency surface and direct modules | richer TS/component/browser transforms require other tools |
| Vitest | substantial TS/ESM/component work, watch and browser projects | transformed tests must still match shipped behavior |
| Web Test Runner | small modules that must run in real browsers | still needs separate Node/native orchestration |
| Jest | mature ecosystem | weaker fit for a new ESM-oriented architecture |
| Custom runner | Nemo fixture/replay semantics only | reject custom discovery/assertions/reporting infrastructure |

Select Node or Vitest in an ADR after measuring useful diagnostics, source maps, cold/warm
speed, concurrent worktrees, reports and setup. Type checking remains an independent gate.

## One command surface

| Command | Required result |
|---|---|
| `npm run doctor` | read-only prerequisites and capability report |
| `npm run check` | format, lint, type, architecture, module profile and schema/artifact integrity |
| `npm test` | fast deterministic JS/TS and Rust CPU suites |
| `npm run test:integration` | document, command/history, persistence and backend contracts |
| `npm run test:browser` | Playwright workflows and justified visual assertions |
| `npm run test:desktop` | actual test-build Tauri app with isolated data and native operations |
| `npm run bench` | named workload, seed and hardware profile with structured metrics |
| `npm run verify` | merge profile that emits one machine-readable receipt |

Each job reports `pass`, `fail`, `blocked` or `not-run`, with reason. A missing required
environment is blocked, never silently converted to success.

## Test layers

1. **Pure unit:** time, easing, transforms, parsing, identity and cache policy using production
   imports and independent expected results.
2. **Property/model:** generated identities, serialization and stateful command sequences;
   retain minimized failing seeds.
3. **Document contract:** old/current save, migration, unknown fields, nested components,
   assets and undo/redo.
4. **Backend parity:** declared Rust/native/WASM subset, numeric tolerances and independent
   reference behavior.
5. **Integration:** create → edit → key → undo/redo → frame change → save/reopen → render/export.
6. **Interaction/visual:** real sliders, gestures, masks, text, alpha, mesh and playback
   transitions at fixed font/scale/time/color settings.
7. **Concurrency/fault:** stale revisions, retries, cancellation, disconnect, device loss,
   missing media and interrupted save.
8. **Performance/soak:** p95/p99 latency, missed presentations, CPU/GPU completion, memory
   plateau, cache eviction and export throughput.
9. **Package/client:** installed desktop artifact plus each supported MCP client and platform.

Most unit tests call the application API directly. MCP has protocol and parity suites; it is
not the route for every test. UI wiring has independent interaction tests even when command
handlers pass.

## Fixture and regression rules

- Every reproduced bug gains the smallest useful failing fixture before the fix when practical.
- Use a versioned corpus covering static/keyed/expression properties, held frames, nested
  components, paths/groups, mask/alpha, text, mesh, media, older schemas and export.
- Each fixture records generation, expected invariants, asset hashes, required capabilities,
  backend, tolerance and seed.
- Use fast-check for suitable JS/TS command models and proptest for Rust properties.
- Preserve failing seeds and minimized command sequences.
- Review visual baseline changes; never regenerate goldens merely to obtain green CI.
- Apply targeted mutation/fuzz tests to critical parsers and migrated kernels, not every edit.
- Test the combined integration candidate. Separate green branches do not prove their merge.

## Built-in Diagnostics surface

The Diagnostics panel, CLI, tests and MCP consume one versioned diagnostics API:

- environment: source/build identity, dirty digest, application/WASM versions, platform,
  renderer/device and current document/fixture revision;
- event timeline: command, undo, frame, request and job IDs; JS/native/worker errors; stage
  transitions and cancellation;
- performance: evaluation/prepare/present p50/p95/p99, GPU completion, long tasks, cache and
  bounded resource usage;
- state: stable IDs, revision, dependencies and stored-model versus adapter state;
- reproduction: load a synthetic fixture, record/replay commands with fixed clock/seed,
  capture output and export a redacted diagnostic bundle.

Use bounded ring buffers, sampling and opt-in detailed tracing. Diagnostics must not create a
different evaluator or serialize the full project every frame. Shareable reports redact asset
content and paths by default.

Suggested application operations:

```text
getCapabilities
getSnapshot
loadFixture
dispatchCommand
evaluateFrame
captureFrame
getTrace
exportBundle
```

Every operation carries request, instance, document and revision identity plus cancellation
where applicable. Stale responses cannot update a later document.

## CI design

Run static architecture, fast CPU, document-contract, browser and required native/build jobs
according to changed modules and reverse dependencies. Until global coupling is retired,
selection stays conservative. Required aggregate checks fail if a required child fails,
cancels or is missing. Expensive GPU/native checks use trusted runners or an explicit
maintainer acceptance path; untrusted contributor code must not automatically execute on a
personal privileged runner.
