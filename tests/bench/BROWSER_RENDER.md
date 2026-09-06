# Browser render workload

`browser-render.cjs` is an opt-in R03 benchmark of the production browser renderer.
It uses the existing `bench-vectors-8x24` generator and rejects bytes that differ
from `tests/fixtures/manifest.json`. It does not regenerate fixtures or change the
global benchmark runner.

Supply Playwright from an existing external installation or a scratch prefix:

```sh
npm install --prefix /tmp/nemo-render-deps --no-package-lock playwright
NEMO_PLAYWRIGHT_MODULE=/tmp/nemo-render-deps/node_modules/playwright \
  node tests/bench/browser-render.cjs --out /tmp/nemo-render-receipt
```

The harness finds installed Chrome/Chromium, or uses `NEMO_CHROME_EXECUTABLE`.
It launches a fresh headless browser with explicit SwiftShader flags. Dependencies
are not installed automatically. This is a software-GPU measurement on a shared
host, with no exclusive native GPU reservation. The actual device used by the
Rust renderer must identify SwiftShader and a fallback adapter.

Options are `--out DIRECTORY`, `--iterations N` (default 1, maximum 100),
`--warmup N` (default 1, maximum 24), and `--timeout-ms N` (default 180000,
maximum 600000). Each iteration visits all 24 frames in order. Warmup runs the
same path and is excluded from metrics. Initial software shader compilation can
take over a minute.

## What runs

The isolated production browser runtime serves the checkout on a fresh loopback
port. All classic scripts referenced by `src/index.html`, the entry HTML, and
the actual geometry WASM glue/binary are checked against served SHA-256 bytes.
The runtime identity must match the checkout before and after measurement.
The receipt also identifies the harness, fixture generator, manifest, input bytes,
source HEAD/dirty digest, browser/Playwright, CPU, OS, and actual GPU device.

The pinned document enters through `SM.importJSON`. Each sample calls the
production `SM.goToFrame(frame)` and `SMEngineBridge.renderNow()`. The Rust
renderer must be enabled, its scene time must equal the requested frame time,
and all 200 workload paths must be materialized in the eight Paper layers.
The production bridge's `suspend()` stops its background render tick, and its
preview scale is fixed at 1. These are measurement controls, recorded in the
receipt. The viewport is 1400 by 1000 CSS pixels with device scale 1; the receipt
records the renderer canvas dimensions. The document remains 1920 by 1080.

Transparent observation wrappers call the original `GPUAdapter.requestDevice`,
`GPUQueue.submit`, and the **same cache-busted module instance's**
`VelloEngine.render`. Only queue submissions made inside observed production
Rust render calls enter a sample. Previously queued work is drained before the
clock starts. Every observed render call must submit work to a captured device's
queue; extra render calls during the completion wait fail the run. No separate
probe adapter, mocked engine, Paper render fallback, or synthetic timing is used.

## Completion and metrics

`GPUQueue.onSubmittedWorkDone()` must resolve for every queue used by the sampled
Rust render calls. `completedFrames` counts those successful frame barriers;
queue submissions and Rust render calls are separate counts, since navigation
can invoke rendering itself. GPU errors, device loss, wrong frame identity,
missing submissions, and completion timeouts fail the run.

Browser `performance.now()` records these wall-clock durations in milliseconds:

| Metric | Interval |
|---|---|
| `navigationMs` | production frame navigation call |
| `renderCallCpuMs` | sum of synchronous production Rust render calls, including calls inside navigation |
| `navigationAndSubmitMs` | navigation plus explicit renderNow through return from submission |
| `queueWaitMs` | submission return through the observed queues' completion promises |
| `frameToQueueCompleteMs` | navigation start through queue completion |

Raw samples and nearest-rank p50/p95/p99, minimum, maximum, and mean accompany
the identity. These are serial frame measurements. Queue completion does not
establish compositor presentation, scanout, GPU timestamp duration, missed
presentations, or realtime playback FPS. This harness sets no performance budgets,
does not assert visual equivalence, and does not close native export or Tauri gates.

The process writes `receipt.json` and one concise stdout summary. Exit 0 means a
complete benchmark, exit 2 means a missing browser/WebGPU/Rust capability, and
exit 1 means invalid input or failed validation. Missing dependencies also produce
a blocked receipt. Measured samples from a failed run never become pass metrics.
Only the owned browser, server, and exact runtime scratch root are closed/removed;
the supplied dependency prefix and the output receipt directory are retained.
