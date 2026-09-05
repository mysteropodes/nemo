# Task-runtime isolation (R06)

Work package [R06 / #902](https://github.com/mysteropodes/nemo/issues/902):
"Concurrent tasks cannot overwrite one another's source state, browser data,
desktop data, ports, caches, builds, or reports." Depends on
[R02](../scripts/nemo/README.md) for source/build identity and the receipt
command surface.

This is a **first, reviewable increment**: a library (`scripts/nemo/lib/isolation.cjs`)
and a debug CLI (`scripts/nemo/isolation.cjs`), with behavioral tests
(`scripts/nemo/isolation.test.cjs`). It is intentionally scoped to what can be
proven without editing any file outside those four — no changes to
`package.json`, `scripts/nemo/lib/jobs.cjs`, Tauri config, R04 packaging, or
R03 inventory. Where full isolation needs one of those, this document names
the exact change required and leaves that part of the acceptance unmet rather
than claiming it.

## What a "task" is here

A task instance is one invocation of work that needs its own mutable state —
a dev server, a `verify`/`check` run, a future browser or desktop test
harness. `resolveTaskId()` gives it an id: an explicit `--task`/`NEMO_TASK_ID`
name if given, otherwise `<worktree>-<pid>-<timestamp>-<random>`, unique per
process invocation with no coordination required. Two tasks started without
talking to each other will not collide by construction.

## What is isolated, and how

| Resource | Mechanism | Proven by |
|---|---|---|
| temp / cache / build / report roots | `taskRoots(taskId)` creates `<NEMO_ISOLATION_ROOT>/<taskId>/{tmp,cache,build,reports,browser-profile,tauri-data,ports}` | `isolation.test.cjs`: two task ids produce disjoint, existing directory trees |
| ports | `reservePort(taskId)` actually binds a TCP server; the kernel refuses a second bind on the same port (`EADDRINUSE`), so this can't be gotten wrong the way a lock-file convention could | `isolation.test.cjs`: two tasks get distinct, simultaneously-connectable ports; a same-task double reservation probes past the first; a released port becomes free again |
| build / report roots, in practice | `taskRoots(taskId).reports` is meant to be exported as `NEMO_REPORT_DIR`, which `scripts/nemo/lib/receipt.cjs`'s `reportDir()` **already reads** (R02, unmodified), and `jobBuildWasm()` in `lib/jobs.cjs` already writes wasm build output under `ctx.reportDir/build-wasm`. So isolating reports isolates that build output too, with zero code changes to R02. | `isolation.test.cjs` calls `receipt.reportDir(receipt.create(...))` directly (R02, read-only) under two different `NEMO_REPORT_DIR` values and asserts the resulting paths are disjoint |
| owner/source handshake | `registerLauncher(taskId, {pid, ownerToken?})` writes `launcher.json` (pid, an unguessable `ownerToken`, and the source identity from `identity.cjs` at start: head/branch/dirty). `verifyHandshake(taskId, {ownerToken, checkSource})` checks the pid is alive, the token matches, and — with `checkSource` — that HEAD hasn't moved since the launcher started. | `isolation.test.cjs`: correct token verifies; wrong token and unknown task are refused by name; a monkey-patched `identity.sourceIdentity()` proves the moved-source case is caught |
| non-owner stop | `requestStop(taskId, ownerToken)` only signals the recorded pid if the token matches; otherwise it returns a named refusal and does nothing | `isolation.test.cjs` spawns a real child process, confirms a wrong-token stop leaves it running, and a correct-token stop actually terminates it |
| shared exclusive resources (desktop input, GPU reference slot) | `acquireExclusiveSlot(slot, taskId)` / `releaseExclusiveSlot(slot, ownerToken)`: atomic `O_EXCL` file create, so only one task can hold a named slot at a time; a lock left by a dead process is reclaimed automatically (never a live one) | `isolation.test.cjs`: a second acquire is refused while the first holds it; a non-owner release is refused; a lock from a killed process is reclaimed |

`RUNTIME_ROOT` defaults to `<os.tmpdir()>/nemo-runtime`, not anything under
the repo — this needs no `.gitignore` entry and needs nothing cleaned up by
`git clean`. Override with `NEMO_ISOLATION_ROOT` (tests do this to run in a
throwaway directory).

## Using it

Library, from a long-lived owning process (a dev server bootstrap, a test
runner):

```js
const iso = require('./scripts/nemo/lib/isolation.cjs');
const taskId = iso.resolveTaskId();
const roots = iso.taskRoots(taskId);
const port = await iso.reservePort(taskId);       // holds the bind for this process's lifetime
const launcher = iso.registerLauncher(taskId, { label: 'dev-server' });
process.env.NEMO_REPORT_DIR = roots.reports;       // isolates R02 receipts/build-wasm output
// ... on shutdown: await port.release();
```

CLI, for debugging/scripting (see `node scripts/nemo/isolation.cjs` with no
args for the full command list):

```
node scripts/nemo/isolation.cjs alloc --task my-task --pid <owning-pid>
node scripts/nemo/isolation.cjs handshake --task my-task --owner <token>
node scripts/nemo/isolation.cjs stop --task my-task --owner <token>
```

**CLI caveat on ports:** `alloc --port` binds, records, and immediately
releases a port from a short-lived CLI process — it tells you a port is free
*right now*, it does not hold a standing reservation past the CLI's own exit
(a process can't keep a socket bound after it exits). A real standing
reservation requires calling `reservePort()` from inside the process that
will actually use the port.

## Verified

`node --test scripts/nemo/isolation.test.cjs` — 13/13 passing, including:

- two full task instances allocated concurrently (roots, port, launcher) with
  zero overlapping paths, independent owner tokens, cross-task stop attempts
  refused, and one task's stop/release leaving the other's handshake intact;
- a released port becoming connectable-false and then successfully reused by
  a fresh reservation;
- a stale exclusive-slot lock (owner process killed) being reclaimed by a new
  task rather than wedging the slot forever.

Run against source `e1d2ea760a45d5dd2f587cc98d708326786a7d9d` on branch
`codex/buzz-f8b4d83fa5d4` (worktree dirty from this increment's own new
files; no tracked file was modified). Not run: native desktop or browser
acceptance (see below — no such harness exists yet to exercise).

## Explicitly not covered by this increment (blocked or deferred)

Per the R06 acceptance ("browser, Tauri... state" and "GPU/desktop physical
UI"), three rows of the isolation table in
[07_GITHUB_PROJECT_AND_PARALLEL_WORK.md](remediation/07_GITHUB_PROJECT_AND_PARALLEL_WORK.md#isolation)
need a change outside this increment's owned files:

1. **Tauri app-data-dir isolation.** Tauri derives the OS app-data directory
   from `identifier` in `src-tauri/tauri.conf.json`
   (`~/Library/Application Support/<identifier>` on macOS); there is no
   environment-variable override for it today. The Tauri v2 CLI does support
   `-c/--config <json>` to merge extra configuration at launch, so the exact
   integration patch is: the process that starts a task-scoped Tauri instance
   passes `--config '{"identifier":"<base-identifier>.task-<taskId>"}'` (or
   the packaged binary's equivalent env/arg, if the packaging owner adds one)
   when launching `tauri dev` / the built app. `taskRoots(taskId).tauriDataDir`
   is computed and ready to be handed to that launch step once it exists;
   this increment does not touch `src-tauri/tauri.conf.json` or `lib.rs`
   (out of scope: R04 packaging / Tauri config ownership). **Acceptance
   blocked** on that patch landing.
2. **Browser profile/context isolation.** `taskRoots(taskId).browserProfile`
   is computed and directly usable as a Chromium/Playwright
   `userDataDir`/persistent-context path, but there is no browser automation
   dependency in this repo yet — `@playwright/test` is absent (`test:browser`
   reports `blocked` in `scripts/nemo/lib/jobs.cjs`, and adding the dependency
   is a `package.json` change owned by R03/R07, not this increment). **No
   behavioral test exists for this row until that dependency lands.**
3. **GPU reference-benchmark exclusivity.** `acquireExclusiveSlot()` is a
   generic, already-tested primitive that a future GPU benchmark job could
   call to serialize access to a shared reference machine, but no such job
   exists yet (`npm run bench` is `not-run`, per R03/R19). The primitive is
   ready; nothing in the codebase calls it for GPU yet, so there is no
   consumer to integration-test against.

`build:desktop` (packaged app build) writes into the worktree's own
`src-tauri/target/<triple>/...`, which is already isolated *across*
worktrees (each worktree/branch has its own checkout, per the parallel-work
contract's "separate worktrees per writer" rule) — no change needed there.
It is **not** isolated for two concurrent `build:desktop` runs *inside the
same worktree* (they'd share one `target/` and `Cargo.lock` file lock);
serializing that is a `cargo`/`CARGO_TARGET_DIR` concern that would touch the
build job in `lib/jobs.cjs`, out of this increment's scope.

Not wired into `npm test`, `npm run check`, or the `JOBS` registry in
`scripts/nemo/lib/jobs.cjs` — those are all changes to files outside this
increment's ownership (`package.json`'s `test` script only globs
`tests/*.test.cjs`; `isolation.test.cjs` lives under `scripts/nemo/` on
purpose, matching the path list this task packet was scoped to). Wiring it
in is a small, mechanical follow-up for whoever owns those files.
