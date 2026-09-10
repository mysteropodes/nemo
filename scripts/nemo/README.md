# `scripts/nemo` — doctor, check, named jobs and receipts

Current command implementation, reviewed on **2026-09-07** at
`66ece0641708122eb8447e85ad8dd7e3402aaf6c`. The orchestration scripts use Node; the jobs
they invoke have their own dependencies and runtime requirements. The
[execution checklist](../../engineering/remediation/EXECUTION_PLAN.en.md) governs current
scope and acceptance; [testing guidance](../../engineering/remediation/reference/03_TESTING_AND_DEBUGGING.md)
distinguishes existing commands from planned coverage and diagnostics work.

| Command | What it does | Exit |
|---|---|---|
| `npm run doctor` | Read-only: source identity (HEAD, branch, dirty digest), build identity (versions, wasm/sidecar hashes), platform, tool prerequisites, capabilities. Never installs or writes outside `reports/`. | always 0 |
| `npm run check` | Static integrity: version strings in sync (package.json, tauri.conf.json, index.html fallback), JSON validity, `src/js` syntax (ES modules checked as such), `index.html` script references resolve, private-labs guard, committed artifacts present. | 0 pass / 1 fail / 2 blocked |
| `npm run inventory` | Regenerates `engineering/inventory/{surfaces.json,surfaces.csv,SURFACES.md}` through static `src/` discovery and handler/consumer binding analysis. Unbound rows remain `unmapped`; this does not prove runtime behavior or exhaust the fixed source/consumer census. The `inventory` job runs `npm run inventory -- --check` and fails on stale committed output; it is in both verify profiles. | 0/1 |
| `npm test` | Node unit tests in `tests/*.test.cjs` and `tests/animation/*.test.cjs`; the named `test:unit` job uses the same scope. | node |
| `npm run test:rust` | `cargo test` for `geometry-wasm` (CPU, native host). | 0/1/2 |
| `npm run test:integration` | Runs test files in `tests/integration`, currently the R06 browser-runtime isolation suite. This is not complete document/history/persistence coverage. | 0/1/2 |
| `npm run test:browser` | Playwright specs under `tests/browser`; the runner is declared in devDependencies, but installed dependency/browser and graphics prerequisites still need verification. Reports `blocked` if the runner cannot be resolved. | 0/1/2 |
| `npm run test:desktop` | Runs `tests/desktop/*.test.cjs` serially against `NEMO_DESKTOP_APP` or the local packaged app; required `blocked` without a package or harness; empty files and skipped tests fail. Native process/storage checks are separate from UI workflow acceptance. | 0/1/2 |
| `npm run native -- start/status/stop ...` | Owner-controlled isolated native launcher. See [runtime isolation](../../engineering/runtime-isolation.md#native-app-launcher). | 0/1 |
| `npm run bench` | `tests/bench/run.cjs` (R03): evaluation workloads on the real `motion.js` loaded whole in a vm sandbox, copy and memory workloads over the workload documents the fixture corpus generates (`tests/fixtures/lib/corpus.cjs`, hashes pinned in `tests/fixtures/manifest.json`), plus render/export workloads declared with the `export` fixture and recorded `not-run` without a WebGPU backend. Writes `bench.json` next to the receipt; records source, hardware and backend, sets no budget (R19). | 0/1/2 |
| `npm run build:wasm` | `wasm-pack build` into the run directory and compare with the committed `src/wasm`; `blocked` without wasm-pack. | 0/1/2 |
| `npm run build:desktop` | Isolated Tauri build under a task/worktree reservation; preserves its exact package and logs in the report directory, bundles ffmpeg dylibs, then performs owned cleanup. Local, unsigned. | 0/1/2 |
| `npm run verify` | Runs a profile (`--profile quick` default: doctor, check, inventory, test:unit, test:rust; `--profile full` adds the rest) or `--jobs a,b,c`, and emits **one receipt**. | 0 pass / 1 any fail / 2 required job blocked |

Any job can be run alone: `node scripts/nemo/job.cjs test:rust,build:wasm`. Add `--json` to
print the receipt instead of the summary.

The named `test:rust-tauri` job runs the native crate in release mode with serial test
threads and a checked native fixture sidecar. It is part of the full verifier profile,
not `npm test` or the quick profile. The `nemo-mcp` crate is not selected by either verifier
profile: use its applicable `cargo test --manifest-path nemo-mcp/Cargo.toml` command
explicitly. Read the candidate's job source/help when selecting checks.

The named `test:coverage-rust` job (T04) runs the same `nemo-mcp` suites under
`cargo-llvm-cov`, writes LCOV/HTML/JSON for the crate's production source into the run's
report directory, and compares each file against
[`engineering/coverage/rust-mcp.baseline.json`](../../engineering/coverage/rust-mcp.baseline.json)
— the coverage observed at a reviewed SHA. It is in no profile and is not required,
because `cargo-llvm-cov` is not yet a declared prerequisite; run it with
`node scripts/nemo/job.cjs test:coverage-rust`. Without the tool or the `llvm-tools`
component it reports `blocked` and names what is missing; it never reports `pass`.
`node scripts/nemo/coverage-rust.cjs` runs the same comparison directly, and `--update`
re-records the baseline. Floors are per file: there is no invented global target, and
**branch coverage is deliberately absent** — the stable toolchain does not instrument
branches, so a branch floor would compare 0 against 0 and could never fail.

`npm run check` does not run all architecture/type/coverage checks. The adopted boundary
lane is described in [the local CI reference](../../engineering/ci/README.md). Its `quick`
lane explicitly selects doctor/check/unit/geometry tests, whereas the verifier's default
quick profile also includes inventory. JS coverage (`test:coverage`, c8) and Rust coverage
(`test:coverage-rust`) exist as explicitly selected, non-required jobs; neither is in a
profile, so a green `verify` run does not mean either one ran. General feature-registration
enforcement remains a planned leaf, not a capability delivered by this command list.

The [R03 baseline](../../engineering/inventory/BASELINE.md) records the retained
CPU, browser, native, and packaged-desktop evidence and their separate limitations.

## Result vocabulary

| Status | Meaning |
|---|---|
| `pass` | the job ran and its own success criterion held |
| `fail` | the job ran and it did not |
| `blocked` | a tool, target, suite or artifact the job needs is absent. Named precisely. **Never** downgraded to a skip: a required blocked job fails `verify` with exit 2 |
| `not-run` | intentionally not attempted, with the work package that will define it |

Required blocked jobs fail verification. Some optional jobs can remain `blocked`/`not-run`
while the overall local receipt exits zero; inspect every selected job and its limitations.
The CI lane requires every selected job to pass. `doctor` always exits zero and reports
missing capabilities rather than establishing that the environment can run every job.

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
- Builds write into the run directory (`build:wasm`) or a fresh task's isolated Cargo
  target (`build:desktop`); they never overwrite the committed `src/wasm` bundle.
  A successful desktop build passes its preserved package to later desktop tests
  in the same runner. Desktop build and test prerequisites are required when selected.
- Historical counts are never treated as current proof: every receipt is bound to the SHA
  and dirty digest it was measured on.
