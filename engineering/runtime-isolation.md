# Task-runtime isolation (R06)

[R06 / #902](https://github.com/mysteropodes/nemo/issues/902) requires concurrent
tasks to keep their source, browser/desktop state, ports, caches, builds and
reports separate. The coordination library and browser preview launcher provide
local ownership checks, distinct task origins and a served source/build identity.
Full R06 acceptance remains open for the desktop and remaining consumers below.

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
| Root release | `releaseTask(taskId, ownerToken)` refuses a live launcher, including its owner, and refuses mismatched or unreadable records. It removes only that validated task's roots after exit. Release port reservations separately before removing their metadata. |
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
URL. The browser launcher adds the served endpoint described below. Tauri still
requires an equivalent runtime handshake before trusting its application instance.

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
- Configure and exercise isolated Tauri data/autosave paths. Merely creating a
  `tauri-data` directory does not make the app use it. The platform owner must
  implement and validate the actual runtime override; no untested Tauri launch
  configuration is prescribed by this document.
- Wire exclusive slots into desktop input and reference GPU benchmark consumers.
- Isolate/serialize simultaneous desktop builds within one worktree. Separate
  worktrees normally separate `src-tauri/target`; same-worktree builds still share it.
- Include the focused suite in normal `npm test`, `check` or `verify` discovery.
  The existing package test glob does not include `scripts/nemo/isolation.test.cjs`.

Those remaining integrations require shared package/job/platform files. The
preview launcher is a bounded browser increment; it does not establish Tauri,
GPU, export or complete R06 acceptance. Its focused tests are run with
`node --test scripts/nemo/browser-runtime.test.cjs`. They exercise concurrent
real servers, served source bytes and identities, source drift, ownership, stale
process recovery, failed startup, duplicate launch and static path confinement.
