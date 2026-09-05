<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Shared capabilities, Rust MCP and open standards

Status: **proposed product architecture**. The Nemo Rust MCP and shared capability registry are
not current product features. They are built during remediation from application contracts,
not as a DOM-control wrapper.

## One definition, many consumers

Every feature owns a namespaced capability module. Rust declarations generate transport
schemas, TypeScript types and declarative descriptors. UI, SDK, tests and MCP call the same
query/command/job handlers.

Each descriptor defines:

- stable ID, namespace, owner and schema/API version;
- scope: document, object, component, app preference, UI session or job;
- input/output types, defaults, bounds, units, enum values and applicability;
- getter/query and mutation handler; static, keyed, expression or computed behavior;
- preconditions, document revision, transaction/undo and invalidation rules;
- UI binding plus optional presentation metadata;
- native/browser availability, permissions and runtime/diagnostic/developer mode;
- fixtures, invariants, diagnostics and performance-sensitive contracts.

Dynamic extensions register compatible descriptors and validated handlers at runtime. Avoid
a central switch or a new general-purpose DSL.

## Complete surface inventory

Maintain:

```text
surface -> capability -> handler -> UI/SDK/MCP exposure -> fixtures -> platform status
```

Inventory project lifecycle, drawing/selection/gestures, layers/components, animation,
expressions/rigs, timeline/graph, effects/masks, media/import/export, playback, preferences,
plugins, diagnostics and integrations. Include contextual/modal/dynamic controls and custom
effect parameters. A button count is not coverage.

States: `inventoried`, `characterized`, `legacy-adapter`, `migrated`, `validated`,
`unavailable-with-reason`, `unmapped`. CI rejects an actionable shipped surface that is
silently unmapped. Generated registration tests do not invent semantic or visual oracles.

## Application command contract

Persistent writes use one dispatcher with validation, transaction and history boundaries.
Long work uses bounded jobs with progress and cancellation. Canvas interactions use explicit
begin/update/commit/cancel commands so one gesture creates one undo action.

Requests identify app instance, document, revision, request and optional job. Queries can use
immutable snapshots. Writes serialize per document with optimistic revision checks.
Byte-equivalent retries use explicit idempotency; late results cannot mutate a newer revision.

## Rust MCP adapter

Use the official Rust MCP SDK at a pinned released protocol version. Keep transport outside
domain/application modules. Default to a compact tool family:

```text
discover instances and capabilities
query snapshot/property/diagnostic resources
dispatch a typed command or transaction
start, observe and cancel a bounded job
retrieve report/artifact references
```

Expose the complete schema through paginated discovery/resources and list-change notifications
without flooding the default context with thousands of per-widget tools. Keep pixels, video
frames and geometry arrays off ordinary JSON tool calls; exchange handles or bulk artifacts.

Multiple clients may attach to an explicitly selected instance. Surface build identity,
connection state and granted mode. A runtime app connection is not source-workspace authority.
A browser needs a separate local companion or hosted adapter because it cannot expose native
stdio by itself; accept desktop attachment first and name browser support separately.

## Modes

| Mode | Allowed capability |
|---|---|
| Runtime | authorized editing/query/jobs, bounded diagnostics, build identity and reports |
| Diagnostic | opt-in trace, fixture session, capture, replay and bounded benchmark on the production path |
| Developer workspace | issue-to-source/test/build loop in an explicitly granted isolated checkout |

Never expose unrestricted remote shell/eval from release builds. A production binary does not
patch itself; a fix becomes a reviewed build and normal update. Developer mode uses explicit
operations and least privilege.

## Open standards from the foundation

| Standard | Required boundary |
|---|---|
| OpenFX | native effect-host port, parameter mapping, image contract, scheduler/lifecycle and fault containment |
| OpenColorIO | working space/config identity, input/output and display transforms |
| OpenEXR | half/float precision, alpha/channel naming and real I/O adapter |
| OpenTimelineIO | precise time/media references and an explicit unsupported-information report |

OpenFX affects images, scheduling, cache and time, not only a plugin menu. Start with a
controlled CPU float subset and one real redistributable plugin. Map supported parameter
descriptors into ordinary Nemo properties so UI, animation, SDK and MCP use the same handlers.

Required early proof:

1. discover/load/describe one real plugin and record its version/supported suites;
2. edit and key a parameter through UI and MCP, then undo and save/reload;
3. render known inputs against independent results;
4. cancel work and inject a host crash without corrupting the document;
5. preserve plugin IDs/parameters when missing;
6. record platform, architecture, image precision and CPU/GPU path.

Unmodified native OpenFX binaries do not run in browser WASM. Browser projects preserve the
effect and report it unavailable or use an explicitly baked proxy. GPU interop and zero-copy
claims require a measured prototype.
