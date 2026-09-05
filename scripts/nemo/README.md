# `scripts/nemo` — doctor, check, named jobs and receipts

The one command surface proposed in
[engineering/remediation/03_TESTING_AND_DEBUGGING.md](../../engineering/remediation/03_TESTING_AND_DEBUGGING.md),
implemented with Node only (no new dependencies). Work package R02.

| Command | What it does | Exit |
|---|---|---|
| `npm run doctor` | Read-only: source identity (HEAD, branch, dirty digest), build identity (versions, wasm/sidecar hashes), platform, tool prerequisites, capabilities. Never installs or writes outside `reports/`. | always 0 |
| `npm run check` | Static integrity: version strings in sync (package.json, tauri.conf.json, index.html fallback), JSON validity, `src/js` syntax (ES modules checked as such), `index.html` script references resolve, private-labs guard, committed artifacts present. | 0 pass / 1 fail / 2 blocked |
| `npm run inventory` | Regenerates `engineering/inventory/{surfaces.json,surfaces.csv,SURFACES.md}` from `src/` (R03): every actionable control, shortcut, menu item, Labs prototype and script API member, bound to its handler and the document consumers it reaches, with unbound controls kept as explicit `unmapped` rows. The `inventory` job runs `npm run inventory -- --check` and fails when the committed files are stale; it is in both verify profiles. | 0/1 |
| `npm test` | Existing Node unit tests (`tests/*.test.cjs`). Unchanged. | node |
| `npm run test:rust` | `cargo test` for `geometry-wasm` (CPU, native host). | 0/1/2 |
| `npm run test:integration` | `tests/integration` when it exists; `not-run` until R12/R13 define it. | 0/1/2 |
| `npm run test:browser` | Playwright specs under `tests/browser`; `blocked` while `@playwright/test` is absent. | 0/1/2 |
| `npm run test:desktop` | Packaged-app harness under `tests/desktop`; `blocked` without a built `Nemo.app`. | 0/1/2 |
| `npm run bench` | `tests/bench/run.cjs` (R03): evaluation (the real Motion evaluator), copy and memory workloads over generated scale documents, plus render/export workloads declared with their fixture and recorded `not-run` without a WebGPU backend. Writes `bench.json` next to the receipt; records source, hardware and backend, sets no budget (R19). | 0/1/2 |
| `npm run build:wasm` | `wasm-pack build` into the run directory and compare with the committed `src/wasm`; `blocked` without wasm-pack. | 0/1/2 |
| `npm run build:desktop` | `tauri build -b app` for the host triple, then `scripts/bundle-ffmpeg-dylibs.py`. Local, unsigned. | 0/1/2 |
| `npm run verify` | Runs a profile (`--profile quick` default: doctor, check, test:unit, test:rust; `--profile full` adds the rest) or `--jobs a,b,c`, and emits **one receipt**. | 0 pass / 1 any fail / 2 required job blocked |

Any job can be run alone: `node scripts/nemo/job.cjs test:rust,build:wasm`. Add `--json` to
print the receipt instead of the summary.

## Result vocabulary

| Status | Meaning |
|---|---|
| `pass` | the job ran and its own success criterion held |
| `fail` | the job ran and it did not |
| `blocked` | a tool, target, suite or artifact the job needs is absent. Named precisely. **Never** downgraded to a skip: a required blocked job fails `verify` with exit 2 |
| `not-run` | intentionally not attempted, with the work package that will define it |

## Receipts

Every run writes `reports/<runId>/receipt.json` (schema `nemo.receipt/1`), a
`receipt.md` rendering, and one `<job>.log` per job that produced output. `reports/` is
git-ignored; paste `receipt.md` into the issue or PR as the verification table. `runId` is
`<UTC stamp>-<short SHA>[-dirty]`. Set `NEMO_REPORT_DIR` to write elsewhere (per-worktree
isolation for concurrent runs).

A receipt records, in this order: source (`head`, `branch`, `describe`, `dirty`,
`dirtyDigest` = SHA-256 of `git diff HEAD --binary` plus the porcelain list, changed paths),
build (four version strings, crate versions, host triple, SHA-256 of the committed
`geometry_wasm_bg.wasm`, `vectorize_wasm_bg.wasm` and host ffmpeg sidecar), platform (OS,
arch, CPU, memory, node; no hostname or user), tools and capabilities (from `doctor`), then
`jobs[]` with `status`, `reason`, `exitCode`, `durationMs`, `artifacts[]` (path, bytes,
sha256), `limitations[]`, `details`, and `summary` (`overall`, `exitCode`, `counts`).

## Rules the code enforces

- `doctor` only runs `--version`-style commands, reads files and, on macOS, `system_profiler`
  and `otool -L` on the committed sidecar. It never installs, upgrades or writes to the
  workstation or a user project.
- Missing environment is `blocked` with the missing thing named. Nothing converts it to
  `pass`.
- Builds write into the run directory (`build:wasm`) or the worktree's own `src-tauri/target`
  (`build:desktop`); they never overwrite the committed `src/wasm` bundle.
- Historical counts are never treated as current proof: every receipt is bound to the SHA
  and dirty digest it was measured on.
