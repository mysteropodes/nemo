# Nemo bench (R03 initial workloads)

```
npm run bench                                   # the `bench` job: writes <run>/bench.json next to the receipt
node --expose-gc tests/bench/run.cjs            # human-readable summary
node --expose-gc tests/bench/run.cjs --json     # the receipt on stdout
node tests/bench/run.cjs --quick --iterations 1 # what the unit test runs
```

A run records what each workload costs **on this machine, for this source
revision**: the receipt carries the source identity (head, branch, dirty digest),
the platform (OS, arch, CPU model and count, memory), Node/V8 versions and the
backend behind every number. It sets **no budgets** (`budgets: null`): thresholds
are R19's job and must be derived from receipts like these, not invented.

| Workload | Backend | What is measured |
|---|---|---|
| `evaluation.rawValueAtFrame` | the real `motion.js` evaluator, extracted by `motion-eval.cjs` | ns per property evaluation over the `keyed-position-default-ease` fixture plus a 200-key synthetic track |
| `evaluation.evalCurvePoints` | same | ns per ease-curve evaluation on `DEFAULT_CURVE` |
| `copy.undoClone.<n>` | `JSON.parse(JSON.stringify(doc))` | ms to clone a generated document of *n* strokes (the undo snapshot path, CLAUDE.md §5bis) |
| `copy.serialize.<n>` | `JSON.stringify` | ms to serialize the same document |
| `memory.parsedDocument.<n>` | V8 heap (`--expose-gc` for stable numbers; `gcAvailable` is recorded) | bytes held by the parsed document, and per stroke |
| `render.engine.export-12f-320x240`, `export.mp4.export-12f-320x240` | none here | declared with their fixture and recorded `not-run`: they need the WebGPU engine in a browser or the packaged app (`test:browser` / `test:desktop`, R21) |

Scale documents come from `tests/fixtures/lib.cjs scaleDocument(n, seed)`; the
run checks their SHA-256 against `tests/fixtures/manifest.json` so two receipts
are comparable only when they measured byte-identical input.

Timing is wall-clock per iteration (median, p90, min, max, mean over
`--iterations`, default 5). Numbers from a loaded machine are still valid
receipts; they are simply receipts of a loaded machine, which is why the
platform block exists.
