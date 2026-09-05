<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Current architecture and target direction

Status date: **2026-09-04**. Revalidate against the implementation branch before work starts.

## Current

Nemo is an alpha motion, animation and compositing application. Its usable core combines
Paper.js document objects and serialized JavaScript state with Rust geometry and a vello/WebGPU
renderer. Tauri supplies desktop filesystem, media, updater and input capabilities. The web
application is still loaded as many ordered classic scripts, and several large modules share
mutable globals.

Existing strengths to preserve:

- working vector editing and motion features;
- Rust geometry, WebGPU rendering and worker-based vectorization;
- Node and Cargo tests, shader checks and focused regression fixtures;
- retained renderer resources, caches and some timing/statistics hooks;
- browser and desktop delivery surfaces.

Confirmed architectural risks:

- live editor state, stored frame data, undo, components, render input and exports have
  multiple readers/writers and explicit allowlists;
- rendering and offscreen export can materialize or mutate live Paper state;
- high-frequency scene data crosses a JSON boundary before Rust rendering;
- much evaluation and scene preparation runs on the UI thread;
- decoded media can traverse CPU RGBA buffers, which does not scale well to many UHD streams;
- separate caches, snapshot undo and embedded media lack one resource/ownership model;
- RGBA8 intermediates are insufficient as a universal finishing/color pipeline;
- large global modules make concurrent feature work collision-prone.

JavaScript is suitable for UI, scripting and orchestration. It is not the sole problem.
TypeScript improves contracts, not frame throughput. Professional-scale performance requires
retained data, immutable evaluation, bounded jobs, lower-copy media paths and explicit GPU/
resource ownership. Rewriting the UI or translating identical data flow to Rust would not fix
those constraints.

## Target

Use one modular application with:

```text
UI / SDK / MCP / tests
        |
versioned application API
        |
queries + commands + transactions + jobs
        |
authoritative document + stable IDs + history
        |
immutable revision snapshots
        |
dependency-aware evaluation and scheduling
        |
vector rendering + image composition + media + encoding
```

Recommended ownership:

- Rust for reusable document/evaluation kernels where beneficial, geometry, scheduling,
  native media, diagnostics transport and the MCP adapter.
- TypeScript/ESM for UI, inspectors, tools and SDK bindings.
- Paper.js as an editing/hit-test adapter during migration, not the sole render-time model.
- Native desktop and worker/WASM browser adapters with explicit capability differences.

Each migrated aggregate has exactly one writable authority. A legacy adapter may bridge old
callers, but JavaScript and Rust must not become concurrent editable mirrors.

## Performance direction

Measure before moving kernels. First establish fixed workloads and p50/p95/p99 timings for
evaluation, scene preparation, GPU completion, decode, copies, memory and export. Then:

1. introduce headless evaluation from `document revision + time + quality`;
2. replace frequent full-scene transport with retained updates and typed bulk data;
3. move heavy ownership away from the UI thread;
4. prototype decoded-surface-to-effect-to-preview/encode paths;
5. coordinate RAM, VRAM, disk cache, jobs, cancellation and recovery.

AE/Resolve-class breadth is a future product program. This remediation establishes a credible,
measurable foundation; it does not itself prove performance parity.
