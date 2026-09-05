# Reviewed profile: `scripts/nemo/**` (R05 adoption increment)

Status: **source-reviewed candidate**, not an R05-closing adoption. Produced against
`scripts/nemo/lib/boundaries.cjs`/`boundaries-ratchet.cjs` as they exist on `origin/main`
(commit `6d9fc52`), without modifying that checker, `scripts/nemo/lib/jobs.cjs`, `package.json`,
or any R03 inventory/fixture path. Scope is the Nemo build-tooling CLI itself
(`scripts/nemo/**`), **not** the `src/js` application the tooling builds/tests — profiling
application consumers is explicitly out of scope for this packet.

- Files declared: 23 of 24 files physically present under `scripts/nemo/**`. See
  "Coverage limits" for the one exclusion (`scripts/nemo/lib/receipt.cjs`) and its exact cause.
- Candidate: [`scripts-nemo.profile.json`](./scripts-nemo.profile.json).
- Baseline: [`scripts-nemo.baseline.json`](./scripts-nemo.baseline.json) — a byte-identical
  copy of the candidate, checked in as the first ratchet comparison point (no prior reviewed
  profile existed to diff against; see "Validation evidence").
- Owner/reviewer: no per-path `CODEOWNERS` exists in this repository. Accountable human owner
  for `scripts/nemo/**` is Ilya (`ivg-design`), per issue #901's "Accountable human /
  integration owner" field; recorded here rather than fabricated per-module ownership.
  Reviewer of this profile: the agent session that authored it (this packet), pending
  maintainer/integration review named in the packet's acceptance.

## Logical layers (project-specific extension of `04_MODULARITY_POLICY.md`)

`scripts/nemo/**` is Node CLI/build tooling, not the Paper/WebGPU application the policy's
`domain/application/features/ports/adapters/bootstrap/shared` table was written for. None of
those names fit a build-tooling CLI cleanly (there is no UI "features" layer, no `ports`
interfaces), so this profile declares a **project-specific extension** — permitted explicitly
by `profile.schema.json`'s `module.layer` description ("...or a project-specific extension"):

| Layer | Role | `allowedLayers` |
|---|---|---|
| `tooling-shared` | Domain-independent helpers only, no internal deps | `[]` |
| `tooling-adapters` | Direct OS/fs/process/network/subprocess I/O | `tooling-shared`, `tooling-adapters` |
| `tooling-application` | Orchestration/use-case logic over adapters | `tooling-shared`, `tooling-adapters`, `tooling-application` |
| `tooling-bootstrap` | Thin CLI entry points that wire lib pieces into `node scripts/nemo/X.cjs` | `*` |
| `tooling-test` | `node:test` suites | `tooling-shared`, `tooling-adapters`, `tooling-application`, `tooling-test` |

This intentionally departs from the app's own layer rule ("application imports domain/ports/
shared", not adapters) because `scripts/nemo/lib/jobs.cjs` and `cli.cjs` genuinely orchestrate
adapter-layer modules (`receipt` — excluded, see below —, `capabilities`) as their real job;
forcing the app's exact matrix onto build tooling would either be dishonest about the real
dependency graph or require inventing a `ports` indirection this codebase does not have. This
divergence is deliberate and stated here rather than silently reused.

⚠️ **`global-state`'s adapters/bootstrap exemption is hardcoded to the literal layer names
`"adapters"`/`"bootstrap"`** (`scripts/nemo/lib/boundaries.cjs:393`), not to whatever a profile
names its `layerRules` keys. None of `scripts/nemo/**`'s real source touches `window` (verified:
`grep -rn "window\." scripts/nemo/*.cjs scripts/nemo/lib/*.cjs` matches nothing outside
`boundaries.test.cjs`'s own literal fixture strings, which test the checker's global-state
detection and are not real `window` access), so this naming mismatch has no practical effect on
this profile today — recorded as a coverage limit in case tooling ever gains `window` access.

## Modules

One module per file (each `.cjs` file is its own cohesive unit with its own `module.exports`;
`publicApi` is the whole file where anything imports it, empty where nothing does).

| Module id | File | Layer | `sizeProfile` | Exported public API | Depends on (real `require` edges) |
|---|---|---|---|---|---|
| `nemo.lib.util` | `lib/util.cjs` | tooling-shared | Domain/application JS or TS | `ROOT, run, which, probeTool, sha256File, sha256Text, exists, readJson, nowIso, compactStamp, fileInfo` | — (only `node:*`) |
| `nemo.lib.identity` | `lib/identity.cjs` | tooling-adapters | Platform/engine adapter | `sourceIdentity, buildIdentity, platformIdentity, hostTriple` | `util` |
| `nemo.lib.capabilities` | `lib/capabilities.cjs` | tooling-adapters | Platform/engine adapter | `collect, findBuiltApp, resolvable, localBin` | `util` |
| `nemo.lib.isolation` | `lib/isolation.cjs` | tooling-adapters | Platform/engine adapter | `RUNTIME_ROOT, PORT_RANGE, resolveTaskId, taskRoot, taskRoots, reservePort, registerLauncher, readLauncher, verifyHandshake, requestStop, releaseTask, pidAlive, acquireExclusiveSlot, releaseExclusiveSlot` | `util, identity` |
| `nemo.lib.browserRuntime` | `lib/browser-runtime.cjs` | tooling-adapters | Platform/engine adapter | `IDENTITY_PATH, SCHEMA, browserLaunchConfig, startBrowserRuntime` | `isolation, identity, util` |
| `nemo.lib.buildRuntime` | `lib/build-runtime.cjs` | tooling-adapters | Platform/engine adapter | `SCHEMA, STATUS_FILE, worktreeBuildSlot, localTauriExecutable, tauriBuildArgs, assertNativePlatform, buildLaunchConfig, readBuildStatus, buildHandshake, buildProcessTreeAlive, stopBuild, runBuildLauncher` | `isolation, identity, util` |
| `nemo.lib.boundariesChecker` | `lib/boundaries.cjs` | tooling-adapters | Platform/engine adapter | `checkProfile, validateProfile, extractImports, countNonBlankLines, resolveSpecifier, findCycles` | — (only `node:fs`/`node:path`; classified as adapter for its real filesystem I/O and size trajectory, not domain-pure logic) |
| `nemo.lib.jobs` | `lib/jobs.cjs` | tooling-application | Domain/application JS or TS | `JOBS, PROFILES, execute` | `util, receipt (excluded, see below), capabilities` |
| `nemo.lib.cli` | `lib/cli.cjs` | tooling-application | Domain/application JS or TS | `parseArgs, runJobs` | `receipt (excluded, see below), jobs` |
| `nemo.lib.boundariesRatchet` | `lib/boundaries-ratchet.cjs` | tooling-application | Domain/application JS or TS | `compareSizeBaseline` | `boundariesChecker` |
| `nemo.cli.boundaries` | `boundaries.cjs` | tooling-bootstrap | Handwritten config/bootstrap | (none; entry point, never imported) | `boundariesChecker, boundariesRatchet` |
| `nemo.cli.browser` | `browser.cjs` | tooling-bootstrap | Handwritten config/bootstrap | (none) | `isolation, browserRuntime` |
| `nemo.cli.build` | `build.cjs` | tooling-bootstrap | Handwritten config/bootstrap | (none) | `isolation, buildRuntime` |
| `nemo.cli.check` | `check.cjs` | tooling-bootstrap | Handwritten config/bootstrap | (none) | `cli` |
| `nemo.cli.doctor` | `doctor.cjs` | tooling-bootstrap | Handwritten config/bootstrap | (none) | `cli` |
| `nemo.cli.isolation` | `isolation.cjs` | tooling-bootstrap | Handwritten config/bootstrap | (none) | `isolation` (lib) |
| `nemo.cli.job` | `job.cjs` | tooling-bootstrap | Handwritten config/bootstrap | (none) | `cli, jobs` |
| `nemo.cli.verify` | `verify.cjs` | tooling-bootstrap | Handwritten config/bootstrap | (none) | `cli, jobs` |
| `nemo.test.boundaries` | `boundaries.test.cjs` | tooling-test | Test file | (none; `node:test` suite) | `boundariesChecker` |
| `nemo.test.boundariesRatchet` | `boundaries-ratchet.test.cjs` | tooling-test | Test file | (none) | `boundariesRatchet` |
| `nemo.test.browserRuntime` | `browser-runtime.test.cjs` | tooling-test | Test file | (none) | `isolation, browserRuntime` |
| `nemo.test.buildRuntime` | `build-runtime.test.cjs` | tooling-test | Test file | (none) | `isolation, buildRuntime` |
| `nemo.test.isolation` | `isolation.test.cjs` | tooling-test | Test file | (none) | `isolation, identity, receipt (excluded, see below)` |

**State owner**: none of these modules hold document/session state (this is build tooling, not
the Paper/vello document model `CLAUDE.md` §1 governs) — their persisted state is
job-run receipts (`lib/receipt.cjs` — excluded) and on-disk task-isolation directories under
`RUNTIME_ROOT` (`lib/isolation.cjs`), which are the sole writers/readers of those paths.

## Lifecycle

- `npm run doctor` → `nemo.cli.doctor` → `nemo.lib.cli` → `nemo.lib.jobs` (`doctor`/`check` jobs).
- `npm run check` → `nemo.cli.check` (same path, `check` job).
- `npm run verify` → `nemo.cli.verify` → `PROFILES.quick`/`full` job sequences.
- `npm run test:rust|test:integration|test:browser|test:desktop|bench|build:wasm|build:desktop`
  → `nemo.cli.job` → named entries in `JOBS`.
- `nemo.cli.boundaries`, `nemo.cli.browser`, `nemo.cli.build`, `nemo.cli.isolation` are **not**
  registered in `JOBS`/`package.json` — standalone developer CLIs invoked directly
  (`node scripts/nemo/boundaries.cjs …`), confirmed by `grep -n "nemo" package.json` and by
  `lib/jobs.cjs`'s `JOBS` registry (neither name appears there).
- Behavioral suites (`tooling-test` layer) run under normal `npm test`/`verify` via the
  `tests/nemo-*.test.cjs` re-exports added in `#939` (`6e356a3`) — confirmed present at
  `tests/nemo-boundaries.test.cjs`, `tests/nemo-boundaries-ratchet.test.cjs`,
  `tests/nemo-browser-runtime.test.cjs`, `tests/nemo-build-runtime.test.cjs`,
  `tests/nemo-isolation.test.cjs`.

## Fixtures / invariants

- `nemo.lib.boundariesChecker`/`nemo.lib.boundariesRatchet` invariant: `checkProfile` must
  accept the real, unedited `scripts/nemo/**` source and report **zero violations**; a
  deliberate cycle/private-import/layer-violation/global-state/size/expired-exception fixture
  must report exactly those violations. Verified this session (see "Validation evidence");
  their own regressions in `boundaries.test.cjs`/`boundaries-ratchet.test.cjs` cover the same
  invariant in depth and are unmodified by this packet.
- `nemo.lib.isolation`/`nemo.cli.isolation` invariant: exclusive task slots have exactly one
  live owner under concurrent acquisition; covered by `isolation.test.cjs`'s multi-process
  fixtures (spawned real `node` subprocesses, not mocks) — unmodified by this packet.
- `nemo.lib.browserRuntime`/`nemo.lib.buildRuntime` invariant: a launched runtime is isolated
  per task root and its process tree is fully reaped on stop; covered by
  `browser-runtime.test.cjs`/`build-runtime.test.cjs`, which spawn real child processes and
  poll actual liveness — unmodified by this packet.
- This profile's own module/layer/size assignments were exercised by a disposable temp-root
  fixture set (six modules, one per rule) created and deleted within this session — not
  committed, matching `04_MODULARITY_POLICY.md`'s "Prove the checker with temporary deliberate
  violations... Remove the fixtures after verifying."

## Validation evidence

Run against `origin/main`@`6d9fc52` (`scripts/nemo/lib/boundaries.cjs` unmodified):

- `node scripts/nemo/boundaries.cjs engineering/boundaries/profiles/scripts-nemo.profile.json`
  → exit 0, `23 module(s), 0 violation(s), 3 warning(s)`. The three warnings are `size` warn-zone
  (not hardMax) on `lib/isolation.cjs` (307/300), `lib/build-runtime.cjs` (340/300) and
  `lib/boundaries.cjs` (455/300, 91% of its 500 hardMax — closest to its ceiling of anything in
  this profile; flagged for the accountable owner, not something this packet can change since
  editing the checker implementation is out of scope).
- `node scripts/nemo/boundaries.cjs …profile.json --baseline …baseline.json --json` → exit 0,
  `"ratchet": {"ok": true, "baselinePathCount": 23, "candidatePathCount": 23, "violations": [],
  "reductions": [], "removals": []}` — the baseline/candidate pair is self-consistent as the
  first ratchet checkpoint for this profile.
- Disposable fixture set (temp root, deleted after the run) reproducing this profile's exact
  layer names/rules: a `cycle` (two mutually requiring files), a `private-import` (reaching a
  file outside the target module's `publicApi`), a `layer-violation` (tooling-application
  reaching tooling-adapters, which this profile's `layerRules` forbids), a `global-state`
  (`window.SMProject` access outside `adapters`/`bootstrap`), an oversized file (261 nonblank
  lines against a 50-line fixture ceiling) and an already-expired size exception all fired as
  violations in one run (`ok: false`, exit 1, 7 violations, one per rule category plus the
  hardMax breach) — confirming both the "actual source passes" and "deliberate violations fail"
  halves of acceptance criterion 1 against this specific profile's structure, not just the
  checker's own unit tests.
- `validateProfile` (invoked internally by `checkProfile` before any source is read) accepted
  `scripts-nemo.profile.json` and `scripts-nemo.baseline.json` without error — this exercises
  every relationship the JSON Schema (`profile.schema.json`) cannot itself express (unique
  module/file ownership, `publicApi` ⊆ `files`, known `sizeProfile` references, `warn ≤
  hardMax`, real exception dates), which is a stronger check than shape-only schema validation.
  A standalone JSON Schema (draft-07) validator (e.g. `ajv`) was not available in this
  environment to additionally check raw shape in isolation; not required since `checkProfile`
  performs strictly more validation than the schema alone and both profiles pass it.

## Coverage limits (explicit, per task instructions)

- **`scripts/nemo/lib/receipt.cjs` is excluded from this profile.** Its real source (not a
  fixture) contains, at line 118, `${((j.durationMs || 0) / 1000).toFixed(1)...}` inside a
  template-literal substitution. The checker's lexical scanner deliberately refuses any `/`
  immediately after a `)` or `}` as regex/division-ambiguous
  (`scripts/nemo/lib/boundaries.cjs:163`, "requires an AST inventory") and exits 2
  (`unsupported-import`-class failure, run-level, not a reportable per-rule violation) rather
  than guess. Reproduced directly: `node -e "require('./scripts/nemo/lib/boundaries.cjs')
  .extractImports(require('fs').readFileSync('scripts/nemo/lib/receipt.cjs','utf8'))"` →
  `ambiguous slash after ) at line 118; requires an AST inventory`. This is the "exact
  actionable blocker" acceptance criterion 1 allows in place of a passing profile for that one
  file: fixing it requires either rewriting `receipt.cjs` (outside this packet's owned paths,
  `engineering/boundaries/**` only) or extending the checker's tokenizer past its documented v1
  scope cut (`README.md` "Deliberately unsupported lexical ambiguity fails the run", also
  outside this packet's owned paths — checker implementation is explicitly not to be edited
  here). Left for the accountable owner/checker maintainer to resolve; not silently worked
  around by editing tooling source to dodge the scanner.
- **Consequence: two real edges are unmodeled**, not flagged, not verified: `nemo.lib.jobs` →
  `receipt.cjs` and `nemo.lib.cli` → `receipt.cjs` (both real `require` calls, confirmed present
  in source) and `nemo.test.isolation` → `receipt.cjs`. A file excluded from `modules[]` is
  invisible to the checker per its own documented v1 scope (no filesystem walking, `README.md`
  "No filesystem walking"); this profile's absence of violations on `nemo.lib.jobs`/`nemo.lib.cli`
  does **not** certify their edges into `receipt.cjs` are private-import/layer-clean — only that
  every *modeled* edge is clean. Re-adding `receipt.cjs` once the tokenizer or the file's syntax
  changes is a small follow-up, not a redesign of this profile.
- **Lexical scanner, not AST/binding analysis**, inherited from the checker itself (unchanged
  here): bare specifiers, `node:` built-ins and indirect/computed loaders are outside the
  modeled graph; `isolation.test.cjs`'s dynamic `require(modulePath)`/`require(process.argv[1])`
  calls are spawned-subprocess script text embedded inside `String.raw` template literals with
  no `${}` substitution around them, so they are opaque string content to the tokenizer (never
  tokenized as real `require` calls) and contribute **no** edges to this profile's graph —
  verified by the full-profile run above completing without an `unsupported-import` diagnostic
  from that file. This is a real blind spot (those subprocess scripts do, at runtime, load
  `lib/isolation.cjs` a second way) inherent to lexical scanning of test harnesses that spawn
  code as strings; not something a profile authoring choice can close.
- **No filesystem walking**: this profile's 23 `files` entries are the authoritative and
  complete list of every `.cjs` file physically present under `scripts/nemo/**` except the one
  documented exclusion above — verified by `find scripts/nemo -name '*.cjs' | wc -l` (24) against
  23 declared files + 1 excluded. A future file added to `scripts/nemo/**` without updating this
  profile is invisible to the checker; this is inherent to the checker's v1 design, not a gap
  introduced here.
- **No `CODEOWNERS`, no per-module reviewer sign-off tooling** exists in this repository; owner
  attribution above is the issue-level accountable owner, not a verified per-file reviewer.
- **This profile does not cover `src/js`, `src-tauri`, `geometry-wasm`, `vectorize-*`,
  `worker*`, or any application consumer of this tooling** — explicitly out of scope per this
  packet's task summary ("Do not profile application consumers"). Producing that remains R01/R03's
  job per `engineering/boundaries/README.md`'s "What's pending after this increment".
- **Adoption/wiring is out of scope**: this packet does not add a `package.json` script, does
  not register a `scripts/nemo/lib/jobs.cjs` job, and does not touch CI — matching the task's
  "adoption increment, not R05 closure" framing and the README's own "What's pending" list.
