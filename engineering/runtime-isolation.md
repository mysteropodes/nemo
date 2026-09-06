# Task-runtime isolation (R06)

[R06 / #902](https://github.com/mysteropodes/nemo/issues/902) requires concurrent
tasks to keep their source, browser/desktop state, ports, caches, builds and
reports separate. The coordination library, the browser preview launcher and the
native app launcher provide local ownership checks, distinct task origins,
per-task native state and a served or written source/build identity. Full R06
acceptance remains open for the live two-instance desktop evidence and the
remaining consumers below.

## API and guarantees

Use `scripts/nemo/lib/isolation.cjs` from a long-lived owning process. Its focused
regressions live in `scripts/nemo/isolation.test.cjs`.

| Resource | Behavior |
|---|---|
| Task IDs and directories | Explicit IDs must contain 1–120 ASCII letters, digits, dots, underscores or hyphens, starting with a letter or digit. Invalid IDs are rejected, never silently normalized. Exact IDs map to SHA-256 directory names under `<runtime>/tasks/`, keeping case variants distinct on case-insensitive filesystems and separating task names from slot/guard namespaces. Generated IDs retain their random suffix even for long worktree names. |
| Mutable roots | Each ID gets `tmp`, `cache`, `build`, `reports`, `browser-profile`, `tauri-data` and `ports` directories. Callers must actually configure their tools to use these paths. Reusing an explicit ID intentionally addresses the same roots. |
| Ports | `reservePort()` holds a real TCP bind in the calling process. Simultaneous reservations cannot hold the same address/port. The caller must integrate the returned server/socket with its service; releasing it and rebinding elsewhere creates a reservation gap. |
| Launcher ownership | `registerLauncher()` atomically refuses an already-registered task, including a stale or unreadable record. Concurrent registrations have one winner. Root release and launcher record changes use the same per-task mutation guard. |
| Local owner/source check | `verifyHandshake()` requires the owner token and a live positive PID. With `checkSource:true`, it requires available source identity and compares the **complete R02 `sourceIdentity()` snapshot**, including HEAD, branch, worktree and dirty digest. Missing identity fails closed. |
| Stop | `await requestStop(taskId, ownerToken, {signal, timeoutMs})` signals only an owner-matching record, then polls for exit. `stopped:true` means exit was observed. A timeout returns `stopped:false` and retains the owner record for a retry. Allowed signals are SIGTERM, SIGINT and SIGKILL; default timeout is 3 seconds, maximum 60 seconds. |
| Root release | `releaseTask(taskId, ownerToken)` refuses a live launcher, including its owner, and refuses mismatched or unreadable records. It removes that validated task's roots after exit and reconciles only its verifiably dead exclusive-slot records; live, foreign or uncertain records remain untouched. Release port reservations separately before removing their metadata. |
| Exclusive resources | Slot acquire/release and stale-holder replacement all take the same atomic per-slot mutation guard. An ordinary record belonging to a dead PID can be replaced; two reclaimers cannot delete each other's live ownership. |

The default runtime root is `<os.tmpdir()>/nemo-runtime`; override it with
`NEMO_ISOLATION_ROOT` **before importing the module**. Reports can use
`NEMO_REPORT_DIR=taskRoots(taskId).reports`: R02's existing `receipt.cjs` and
`jobBuildWasm()` already honor this root. No R02 files are changed here.

## Ownership limits and interrupted mutations

These are cooperative local-process controls for tools using this library, not a
security boundary against another process with access to the same user account.
Owner tokens reside in local records. A live PID is not a process-birth identity;
PID reuse and processes bypassing the library remain outside this increment's
proof. The source comparison inherits R02's fingerprint definition, including
its current limitation that untracked-file contents are not hashed separately.

Despite its historical name, `verifyHandshake()` reads local launcher metadata;
it does **not** challenge a running server or establish which checkout serves a
URL. The browser launcher adds the served endpoint described below. The native app launcher adds the
manifest the running instance writes for itself, which is what identifies the
application instance rather than the launcher's own bookkeeping.

Each mutation creates an atomic directory under `<runtime>/mutations/`, with an
`owner.json` identifying the mutating PID. Normal completion removes the guard.
Contention returns a named busy refusal (registration/root release throw
`EBUSY`; slot operations and stop return a refusal object). A caller may retry
once the current operation finishes.

If a process dies while holding a mutation guard, the guard is deliberately
**not** stolen automatically. Stop dependent work, reconcile the affected
launcher/slot and actual processes, then remove that exact abandoned guard when
no operation can still own it. A missing `owner.json` is also an interrupted
state requiring reconciliation. Do not delete guards based solely on age or
rerun an indeterminate task blindly. This availability tradeoff prevents a
stale reclaimer from deleting a successor's lock.

## Use

```js
const iso = require('./scripts/nemo/lib/isolation.cjs');
const taskId = iso.resolveTaskId();
const roots = iso.taskRoots(taskId);
const launcher = iso.registerLauncher(taskId, { label: 'test-runner' });
process.env.NEMO_REPORT_DIR = roots.reports;
const port = await iso.reservePort(taskId);
// Integrate port.server with the service; release it during shutdown.
await port.release();
```

From a separate controlling process, retain the returned owner token and owning
PID. The one-shot CLI defaults `alloc` and `slot-acquire` to its parent PID;
pass `--pid` explicitly when the actual task is a different process.

```sh
node scripts/nemo/isolation.cjs alloc --task my-task --pid <owning-pid>
node scripts/nemo/isolation.cjs handshake --task my-task --owner <token> --check-source
node scripts/nemo/isolation.cjs stop --task my-task --owner <token> --timeout-ms 3000
node scripts/nemo/isolation.cjs release --task my-task
```

`alloc --port` immediately releases its temporary bind before returning. It is
a momentary availability probe, not a lasting reservation. The CLI exposes
`slot-acquire` and `slot-release` for explicit resource coordination as well.

## Browser preview launcher

`node scripts/nemo/browser.cjs start --task preview-a` starts a long-lived HTTP
server bound to loopback on an OS-assigned port. Its first stdout line is JSON
with the actual origin, PID, task roots and owner token. Keep that token in the
controlling process. Start a second task with a different ID to obtain a distinct
origin and mutable roots; the server holds its listening socket until shutdown.

The server serves this checkout's `src/` files with no-store responses and
appropriate JavaScript/WASM MIME types. It rejects writes, traversal and symlinks
that escape that source directory. It does not expose the rest of the checkout.

Open the returned `identityUrl` (`/.well-known/nemo-runtime.json`) before using
an instance. The response identifies task/PID/origin and compares startup and
current R02 source/build snapshots. It never includes the owner token or Git
origin URL. A changed source/build or invalid owner record returns HTTP 409;
static requests also fail closed once the mismatch is observed. Static requests
cache the check for up to one second; identity requests refresh it immediately.
The source fingerprint retains R02's documented untracked-content limitation.

Browser launch is explicit:

```sh
node scripts/nemo/browser.cjs start --task preview-a --browser auto
node scripts/nemo/browser.cjs start --task preview-b --browser auto --headless
```

`--browser auto` finds an installed Chromium-family browser, or use an absolute
executable path. The launched process receives this task's `--user-data-dir` and
`--disk-cache-dir`, along with isolated temporary/cache/report environment paths.
Without that option, only the server runs: opening its URL in an existing browser
does not configure a separate profile. Inspect `browser.integrated`, `active`
and `error` in the identity response; a healthy HTTP server alone does not prove
that a browser started or that Nemo's application workflow passed.

From a separate controlling process:

```sh
node scripts/nemo/browser.cjs status --task preview-a --owner <token>
node scripts/nemo/browser.cjs stop --task preview-a --owner <token>
```

`status` checks local ownership/source metadata; use the served identity endpoint
for the build and actual URL identity. `stop` refuses a mismatched owner, waits
for its launcher to exit, then removes that task's mutable roots, including its
browser profile. These are disposable task profiles: retain any wanted artifacts
before stopping. Graceful launcher shutdown closes its browser and HTTP server;
forced termination and interrupted records still require the reconciliation
rules above. No unrelated browser or task is selected by name.

## Desktop build launcher

`node scripts/nemo/build.cjs start --task build-a` invokes the installed local
Tauri CLI as `tauri build --target <host> -b app --no-sign`. It writes Cargo
output under that task's `build/tauri-target`, routes temporary, cache and
report paths through the task roots, and holds an exclusive slot derived from
the exact worktree path until the complete owned build process group exits.
Different worktrees have different slots. A second build in the same worktree
is refused before it starts, avoiding concurrent writes by Tauri/Cargo to shared
generated source. Direct build-leader exit does not establish group completion:
remaining descendants are terminated and verified absent before slot release.
If that cannot be established, the launcher and slot remain live with a
`reconciliation-required` result.

The first stdout line is JSON containing the task roots, owner token, source and
build identities. Build stdout/stderr go to the reported log files. The launcher
stays alive after success or failure so its ownership handshake and artifacts
remain addressable; completion releases the build slot. Inspect or stop it from
a separate controlling process:

```sh
node scripts/nemo/build.cjs status --task build-a --owner <token>
node scripts/nemo/build.cjs stop --task build-a --owner <token>
```

`status` compares the current complete R02 source/build identity with startup
and reports the child state and result. `stop` refuses a mismatched token,
terminates an active build process group, waits for launcher exit and removes
that task's disposable roots. Copy wanted artifacts before stopping. Forced or
external launcher termination can still leave child processes or stale records
that require the reconciliation procedure above.

The native default currently supports macOS only. Other platforms are rejected
before task roots, slots or launcher records are created because their bundle
arguments and process-tree ownership have not been implemented or validated.

This serializes same-worktree builds and isolates Cargo output; it does not make
the checkout immutable. Tauri or its build scripts can still update generated
files under `src-tauri/gen/`. Treat those as shared-writer paths: inspect the
source diff after a real build and coordinate any expected regeneration with
its owner. Actual paired native-build evidence and integration with the R02/R03
`build:desktop` job remain separate work because that job surface is currently
owned by R03/R04.

The focused non-native regressions run with
`node --test scripts/nemo/build-runtime.test.cjs`. They use executable stubs to
cover disjoint paths, same-worktree slot refusal/release, nonzero build results,
owner-only status/stop, active process-group reaping, leader-before-descendant
exit and retained artifacts without starting a Tauri, desktop or GPU process.
The core isolation suite already covers source-identity drift without adding a
second concurrent worktree mutation to normal test discovery.

## Native app launcher

`node scripts/nemo/native.cjs start --task app-a` starts a built Nemo with that
task's isolated environment, owns the resulting process group, and reports what
the running instance disclosed about itself. It resolves the app from an
explicit `--app`/`--executable`, else this task's own build output, else the
worktree's default target, and reads the bundle's `CFBundleExecutable` rather
than guessing the binary name. A missing build is a named blocker listing every
path it looked in.

Isolation has two halves, because macOS keeps the two kinds of state in two
different places:

| State | Mechanism |
|---|---|
| App data, config, cache, logs | every `app_*_dir()` in Tauri resolves as `<platform dir>/<config identifier>`, so the bootstrap rewrites the identifier to `com.strokemotion.app.nemo-task-<16 hex>` before the app runs. Everything that goes through the path resolver — the fs plugin scope, `appDataDir()` in JS, the feedback and history folders — follows without knowing this exists. |
| WebKit `localStorage`/IndexedDB | not under those directories at all; they follow the webview's data store, so the configured windows are created in `setup` with an explicit `data_store_identifier` (macOS >= 14). Window labels are untouched, so `capabilities/default.json` keeps applying to the same window. |

Both derive from the task key — the SHA-256 of the exact task id that
`isolation.cjs` already computes (`idKey`, exported for this). The native side
takes it as input instead of recomputing it: a second derivation would be one
more pair of functions that has to stay identical by hand.

| Variable | Meaning |
|---|---|
| `NEMO_TAURI_DATA_DIR` | absolute path to this task's `tauri-data` root. **Its presence is what turns isolation on**; nothing else in the repository sets it. |
| `NEMO_TASK_ID` | the task id, echoed in the manifest. |
| `NEMO_TASK_KEY` | 64 lowercase hex characters, `idKey(taskId)`. |
| `NEMO_TASK_OWNER_TOKEN` | optional. Without it the instance refuses every release request; a missing token is never read as "no check required". |

With `NEMO_TAURI_DATA_DIR` absent nothing changes: the app resolves exactly the
paths it always has. With it set and anything else missing or invalid, the app
exits 2 naming the variable instead of starting on the shared production
identifier — a silent fallback would write an isolated run's state into the
user's real app data, which is the failure this work package exists to prevent.
`NEMO_TASK_ID` alone is deliberately not enough, since the build launcher
already exports it for every desktop build.

The app writes `native-runtime.json` (schema `nemo.native-runtime/1`) into that
root and prints the same document as its first stdout line: task id and key, the
identifier and data store identifier actually in use, the app directories read
back from Tauri's own path resolver, pid, executable and app version. It never
contains the owner token. The `nemo_task_runtime` command returns the same
document to the page, so an in-app check can assert which task's state it is
looking at before writing anything. `nemo_task_runtime_release` takes the owner
token, marks the manifest released and exits — the stop path that stays
available when process-group termination does not, because an instance started
through macOS `open` is reparented by launchd.

```sh
node scripts/nemo/native.cjs start --task app-a --reserve desktop-input
node scripts/nemo/native.cjs status --task app-a --owner <token>
node scripts/nemo/native.cjs stop --task app-a --owner <token>
```

`status` is `ok` only when all of the owner token, the complete R02 source and
build identities, and a live app manifest naming this task agree. A manifest
left by an earlier or different run is reported as such, never accepted as this
instance's identity. `stop` refuses a mismatched owner, terminates the owned
process group, waits for launcher exit and then removes that task's roots.
Copy wanted artifacts first. `stop --retain-data` instead releases the launcher
registration and reservations while preserving task roots, app directories and
the WebKit store for a later launch with the same task id. Both modes require
confirmed app-process-group exit before releasing ownership; an orphan whose
process identity or group exit cannot be proved retains its state and reports
that explicit reconciliation is required. A stale manifest cannot satisfy a
new launch.

Removing the task root is not enough on its own: the isolated app directories
are `<platform dir>/<identifier>`, so they live outside it and
`isolation.releaseTask` never sees them, as does the WebKit store under
`~/Library/WebKit/<bundle identifier>/WebsiteDataStore/<uuid>`. `stop` therefore
reads the manifest the instance wrote, and removes each directory **whose last
component carries this task's own identifier suffix** — recomputed locally from
the task id, never trusted from the manifest. A manifest naming the shared
`com.strokemotion.app` directory, another task's directory or a relative path is
refused by name and reported in `appState.refused`. A start that fails does not
release anything it did not acquire. Reuse the task id after an explicit
`--retain-data` stop to read its saved state; the default stop removes it.

`--reserve` takes the shared resources this instance needs exclusively, before
it starts: `desktop-input` for a run that drives the one keyboard and mouse,
`gpu-reference` for a reference measurement. A second launcher asking for a held
slot is refused by name. Running two isolated instances at once is **not** what a
slot restricts — that is the point of the roots above; a second instance that
does not ask for the shared resource still starts.

Focused regressions run with `node --test scripts/nemo/native-runtime.test.cjs`
and reach normal discovery through `tests/nemo-native-runtime.test.cjs`. They
use an executable stub that reproduces only the app's observable contract (the
environment it is handed, the manifest it writes), so they cover two concurrent
instances on disjoint roots, per-task keys, handshake refusal for a silent or
foreign manifest, owner-only stop with root removal, explicit reservation
refusal and release, the missing-build blocker, derivation-bound removal of the
isolated app directories (refusing the production directory, another task's
directory and a relative path), and a refused start leaving an earlier
instance's state intact — without a desktop, a GPU or a window. The environment contract itself is asserted against
`src-tauri/src/task_runtime.rs`, so renaming a variable on one side only fails
the suite instead of silently returning the app to shared state. The native
resolution rules (activation, fail-closed refusals, identifier and data-store
derivation) are proved by `cargo test task_runtime` in `src-tauri`.


## Validation and remaining integration

Run `node --test scripts/nemo/isolation.test.cjs`. The 20 behavioral tests cover
two task roots and bound ports, R02 report routing, independent stop authority,
invalid/case-sensitive IDs, long generated IDs, duplicate/concurrent launcher
registration, full source mismatch and unavailable identity, ignored SIGTERM
with a later successful owner retry, simultaneous stale-slot reclamation, and
an interrupted guard refusing takeover. These exercise real spawned processes
and sockets; they do not launch Nemo's browser or desktop application.

Still required for the full acceptance contract in
[the parallel-work specification](remediation/07_GITHUB_PROJECT_AND_PARALLEL_WORK.md#isolation):

- Complete automated browser/application workflow coverage and standard harness
  integration. The preview launcher supplies real origins, explicit Chromium
  profiles and the served identity, while R02 `test:browser` still requires its
  separate Playwright harness. A browser preview does not build WASM or the app.
- Two real packaged instances were run at once on 2026-09-05 and the isolated
  roots hold on the live desktop. Both apps reported distinct identifiers and
  app directories from Tauri's own path resolver; WebKit materialised two
  separate stores (`~/Library/WebKit/com.strokemotion.app/WebsiteDataStore/
  99377ca9-…` and `80a136b6-…`); both autosaved real project JSON into their own
  `history/untitled-autosave` directory with **zero shared filenames** (9 files
  against 24); one instance was driven through its real UI (New Project dialog,
  named project, Create) through the accessibility tree; and the shared
  production state was byte-for-byte untouched — `com.strokemotion.app` stayed at
  25 files / 464K with its newest mtime predating the run, and the production
  WebKit and Caches directories likewise. A stopped instance's state survived
  its app exit. Still open: a restart of the same task id reading back its own
  saved document through the UI, which was interrupted when a parallel session
  took the `desktop-input` slot.
- `--reserve` gives desktop input and reference GPU measurements an explicit
  exclusive reservation with a real consumer. Benchmark and input-driving
  harnesses still have to ask for those slots from their own entry points.
- Validate two real native builds through the standalone isolated launcher,
  inspect their source diffs and then integrate it with the standard R02/R03
  `build:desktop` job after that shared surface is available.
- Production profile/launcher adoption remains separate from test discovery.
  Normal `npm test`/`verify` discovery reaches the six focused suites through
  isolated entry processes under `tests/nemo-{boundaries,boundaries-ratchet,
  isolation,browser-runtime,build-runtime,native-runtime}.test.cjs`; each suite
  can still be invoked directly while developing it.

Those remaining integrations require shared package/job/platform files. The
preview launcher is a bounded browser increment; it does not establish Tauri,
GPU, export or complete R06 acceptance. Its focused tests are run with
`node --test scripts/nemo/browser-runtime.test.cjs`. They exercise concurrent
real servers, served source bytes and identities, source drift, ownership, stale
process recovery, failed startup, duplicate launch and static path confinement.

## Integrated packaged-native validation

`npm run native -- start --task <id> --app <Nemo.app>` uses the isolated native
launcher. Reserve `desktop-input` when driving a window and `gpu-reference` for
reference measurements. Owner-authorized `stop --retain-data` ends the process
and releases its reservations while retaining the task's app/WebKit state for
a subsequent start with the same task id. Ordinary `stop` also releases that
state, after proving the application process group has exited. Uncertain
orphan-process identity blocks cleanup for explicit reconciliation.

`NEMO_DESKTOP_APP=/absolute/path/Nemo.app npm run test:desktop` executes the
Node tests in `tests/desktop` serially. The named job rejects failed, cancelled,
skipped and incomplete test output; its receipt identifies the chosen package
and report directory. The app runtime manifest includes the child's observed
temporary, cache and report environment so the harness can compare actual
values against launcher intent. Only those directory variables are disclosed.

These automated process, ownership and storage checks are one R06 gate. Actual
UI save/reload and rendering must be exercised separately on the identified
package; launcher source identity does not establish the provenance of an
arbitrary prebuilt executable. Record the clean build receipt and package
hashes when running acceptance. Signing and release acceptance remain separate.
