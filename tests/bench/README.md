# Nemo bench (R03 initial workloads)

```
npm run bench                                   # the `bench` job: writes <run>/bench.json next to the receipt
node --expose-gc tests/bench/run.cjs            # human-readable summary
node --expose-gc tests/bench/run.cjs --json     # the receipt on stdout
node tests/bench/run.cjs --quick --iterations 1 # what the unit test runs (one workload document)
```

A run records what each workload costs **on this machine, for this source
revision**: the receipt carries the source identity (head, branch, dirty digest),
the platform (OS, arch, CPU model and count, memory), Node/V8 versions and the
backend behind every number. It sets **no budgets** (`budgets: null`): thresholds
are R19's job and must be derived from receipts like these, not invented.

| Workload | Backend | What is measured |
|---|---|---|
| `evaluation.valueAtFrame` | the real `motion.js`, loaded whole in a vm sandbox (`tests/fixtures/lib/sandbox.cjs`) | ns per property evaluation over the `keyed-props` fixture plus a 200-key synthetic track |
| `evaluation.evalCurvePoints` | same | ns per ease-curve evaluation on `DEFAULT_CURVE` |
| `evaluation.expression` | same, with the shipped expression compiler | ns per evaluation of `frame * 3` and `[value[0] + frame * 2, value[1]]` over the `expression-props` fixture |
| `copy.undoClone.<workload>` | the production `_cloneLayersForUndo` (`src/js/tweens.js`, lifted with `_walkStrokes` and app.js `_HEAVY_STROKE_FIELDS`) | ms for the real undo snapshot of the document's layers (heavy string fields detached, native clone, reattached — CLAUDE.md §5bis) |
| `copy.jsonClone.<workload>` | `JSON.parse(JSON.stringify(doc))` | ms for the plain JSON round trip the snapshot is built on |
| `copy.serialize.<workload>` | `JSON.stringify` | ms to serialize the same document |
| `memory.parsedDocument.<workload>` | V8 heap in an **isolated child process** (`--expose-gc`, only the JSON string alive at the baseline) | bytes held by the parsed document, and per stroke; repeating or reordering workloads does not change it |
| `render.engine.export`, `export.mp4.export` | none here | declared with the `export` fixture and recorded `not-run`: they need the WebGPU engine in a browser or the packaged app (`test:browser` / `test:desktop`, R21) |

Workload documents (`bench-vectors-40x24`, `bench-vectors-8x24`, `bench-images-20x24`) come from
`tests/fixtures/lib/corpus.cjs benchDocument(params)` with the parameters in
`tests/fixtures/generate.cjs`; the run checks their SHA-256 against
`tests/fixtures/manifest.json` and refuses any other document, so two receipts
are comparable only when they measured byte-identical input. `--quick` measures `bench-vectors-8x24` only.

Timing is wall-clock per iteration (median, p90, min, max, mean over
`--iterations`, default 5). Numbers from a loaded machine are still valid
receipts; they are simply receipts of a loaded machine, which is why the
platform block exists.
