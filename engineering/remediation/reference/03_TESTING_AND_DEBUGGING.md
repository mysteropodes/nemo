<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Testing, regression prevention and debugging

> **Current execution guidance — 2026-09-07.** The
> [English checklist](../EXECUTION_PLAN.en.md) / [French copy](../EXECUTION_PLAN.fr.md)
> governs required tests and acceptance. The runner comparison has already selected
> Node for the extracted seam ([ADR 001](../../animation/ADR-001-curve-runner.md)); do not
> repeat that trial. New c8, Rust coverage and expanded diagnostics are
> planned until their named leaves land. Preserve exact baseline failures; do not fix
> unrelated product defects to obtain a globally green baseline.

Status: **quality reference with partial implementation**. Source reviewed at
`66ece0641708122eb8447e85ad8dd7e3402aaf6c` has Node/Cargo tests, browser and desktop
harnesses, structured receipts and the opacity trace/replay seed. General coverage producers,
expanded diagnostics and the remaining contract checks are planned; their presence in this
reference is not a pass or installed-runtime claim.

## Recorded runner decision

Keep Node's `node:test` and `node:assert`, Playwright and Cargo tests. The completed
[Node/Vitest comparison](../../animation/ADR-001-curve-runner.md) retains Node for the extracted
seam. Do not repeat it, migrate runners or build a custom discovery/assertion framework for
this remediation. Reconsider a runner only for a demonstrated later requirement. Type
checking and coverage remain independent from a passing test count.

## Existing command surface and its limits

Read [the local command documentation](../../../scripts/nemo/README.md), current job source
and help before choosing a validation profile. These commands have different scopes; none
implicitly proves all test, architecture, coverage and installed-client obligations.

| Command | Current scope / interpretation |
|---|---|
| `npm run doctor` | read-only identity/prerequisite/capability report; zero exit does not mean every capability is available |
| `npm run check` | version, JSON, JS syntax, script references, private-labs and committed-artifact integrity; not a complete lint/type/coverage gate |
| `npm test` | Node suites in `tests/*.test.cjs` and `tests/animation/*.test.cjs`; not all Rust crates |
| `npm run test:rust` | geometry-wasm Cargo tests; the MCP crate needs its own explicit Cargo command |
| `npm run test:integration` | existing integration directory; a job name does not prove complete document/history/persistence coverage |
| `npm run test:browser` | Playwright workflows and justified visual assertions |
| `npm run test:desktop` | actual test-build Tauri app with isolated data and native operations |
| `npm run bench` | existing named workloads and structured metrics; inspect blocked/not-run cases and separate future performance budgets |
| `npm run verify` | one machine-readable receipt; defaults to the quick profile, not all acceptance surfaces |

The [local boundary lane](../../ci/README.md) checks adopted profiles against a reviewed
protected base. Extend its missing language/architecture/registration coverage through the
named checklist leaves. T01–T04 add JavaScript c8 and Rust coverage/report producers;
T05–T08 extend diagnostics. Do not claim these additions merely from the table above.

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

## The installed candidate and the clients that test it

"The installed Nemo" is never a name, a path, an app found in `/Applications`, or the newest package on disk. It is **one
package**, identified by the items below, recorded together. An installed-app or MCP-client result that does not carry all of them
does not count as installed or client evidence, and a package whose proof names another source head is evidence for that head only.

| Item | What it is | Where it comes from |
| --- | --- | --- |
| Source head and tree | 40-hex commit and tree of the clean committed checkout under acceptance | `git rev-parse HEAD`, `HEAD^{tree}`; must equal `details.source.head` of the package proof, with `dirty: false` |
| Build id | `<app version>:<source head>`, the whole string, no `-dirty` | the registry record of the running app and `buildId` in `nemo_discover` |
| Executable SHA-256 | `Contents/MacOS/nemo` (the app) and `Contents/MacOS/nemo-mcp` (the bundled MCP) | `shasum -a 256` of the files that will run, equal to the `artifacts[]` entries of `build-proof.json` |
| Client versions | per agent client: name, `--version` output, resolved path and SHA-256 of the binary, the exact prompt and command line | `codex --version`, `claude --version`; recorded by the harness |

**How a fresh session finds the candidate.** It is the package built from the head being accepted: the clean checkout's head is
named by the task or issue, and `npm run build:desktop` (run under `lockf -k /tmp/nemo-heavy-job.lock`, one build at a time)
prints a report directory holding `Nemo.app` and its `build-proof.json` side by side. Do not rebuild to refresh a package, do not
adopt a retained older package for a newer head, and do not infer anything from an app that merely launches. If no package with a
passing proof for the head exists, say the candidate is unbound and stop; the orchestrator builds it.

**Binding at run time.** The harness starts the package once in a fresh isolated session (`scripts/nemo/native.cjs`, data removed on
stop) and refuses unless: the proof lists the app and the bundled `nemo-mcp` by those SHA-256 values and they match the files about
to run; the registered build id is the one above; exactly one application is discovered and it is the registered instance; the OS
listener on the registered port is the process the harness started; and, before and after every row, the head, the executable
bytes and the pid are unchanged (`tests/desktop/installed-evidence/candidate.cjs`, shared by every runner of that folder).

**Client routes.** A client reaches the candidate only through a configuration that exists for one invocation and names the
package's own `nemo-mcp` and the isolated session's registry: Codex through `-c mcp_servers.nemo.command=...` and
`-c mcp_servers.nemo.env.NEMO_MCP_REGISTRY=...`, Claude Code through `--mcp-config <file> --strict-mcp-config`. User, project and
repository client configuration is never read or edited for this evidence, and a client cannot be pointed at a Nemo it was not
given. The route sends no keyboard or mouse input.

**What counts as a result.** The verdict comes from the harness's own reads of the application and from files the application wrote,
taken after the client has exited; what the client says it did, and its tool trace, are recorded as information only. A client that
is missing, unauthenticated or over quota makes its run **unrun**, with the reason, never a pass. The original opacity workflow
(#1049) is run this way by `tests/desktop/installed-evidence/f03-clients.cjs`; its receipt records the identity above, per client
name, version, binary SHA-256 and prompt, every assertion with observed and expected values, and what it cannot show.

## Fixture and regression rules

- Every reproduced bug gains the smallest useful failing fixture before the fix when practical.
- A pre-existing defect may remain characterized and unfixed. Record it against the exact
  baseline and distinguish it from a regression introduced by the current extraction.
- Use a versioned corpus covering static/keyed/expression properties, held frames, nested
  components, paths/groups, mask/alpha, text, mesh, media, older schemas and export.
- Each fixture records generation, expected invariants, asset hashes, required capabilities,
  backend, tolerance and seed.
- Add generated/property cases where the named leaf benefits; fast-check/proptest are
  possible tools, not already installed requirements or a reason for unrelated dependency work.
- Preserve failing seeds and minimized command sequences.
- Review visual baseline changes; never regenerate goldens merely to obtain green CI.
- Apply targeted mutation/fuzz tests to critical parsers and migrated kernels, not every edit.
- Test the combined integration candidate. Separate green branches do not prove their merge.

On `codex/native-remediation`, classify affected tests explicitly: keep independent
behavior/parity oracles and native correctness; adapt tests coupled only to old call
structure; retire tests solely of removed old/new ownership handoff; and keep a visible
pending-migration record for required temporarily unavailable features. A green native
subset is not full baseline acceptance. Test the available workflow at each integration,
then require the complete applicable browser, installed-client and visual matrix at the
identified final promotion candidate. Never turn a required pending test into a silent skip.

## Target built-in Diagnostics surface

Extend the existing opacity trace/replay seed under T05–T08. The proposed general panel,
CLI, tests and MCP consume one versioned diagnostics API; this target is not yet a claim
that every operation below exists:

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

**Current execution policy (2026-09-06): builds and validation run locally.** Hosted
Actions builds require an explicit human request for the specific run; commits, pushes,
PRs, merges and tags do not authorize one. See [the CI policy](../../ci/README.md).

Run static architecture, fast CPU, document-contract, browser and required native/build jobs locally
according to changed modules and reverse dependencies. Until global coupling is retired,
selection stays conservative. Required aggregate checks fail if a required child fails,
cancels or is missing. Expensive GPU/native checks use trusted runners or an explicit
maintainer acceptance path; untrusted contributor code must not automatically execute on a
personal privileged runner.
