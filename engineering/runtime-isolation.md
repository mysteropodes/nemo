# Task-runtime isolation (R06)

[R06 / #902](https://github.com/mysteropodes/nemo/issues/902) requires concurrent
tasks to keep their source, browser/desktop state, ports, caches, builds and
reports separate. This increment supplies a **local coordination library and
debug CLI**, not an integrated application launcher. Full R06 acceptance remains
open until the consumers listed below are implemented and exercised.

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
URL. A real launcher still needs a served endpoint that returns its task, owner,
source and build identity. Browser/Tauri acceptance must check that endpoint or
equivalent process protocol before trusting the application instance.

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

- Wire these helpers into the actual launcher, including source/build endpoint
  verification and tool environment/configuration for temp, cache and build paths.
- Configure and exercise isolated browser profiles/origins. No browser harness is
  integrated here; `test:browser` remains blocked in R02.
- Configure and exercise isolated Tauri data/autosave paths. Merely creating a
  `tauri-data` directory does not make the app use it. The platform owner must
  implement and validate the actual runtime override; no untested Tauri launch
  configuration is prescribed by this document.
- Wire exclusive slots into desktop input and reference GPU benchmark consumers.
- Isolate/serialize simultaneous desktop builds within one worktree. Separate
  worktrees normally separate `src-tauri/target`; same-worktree builds still share it.
- Include the focused suite in normal `npm test`, `check` or `verify` discovery.
  The existing package test glob does not include `scripts/nemo/isolation.test.cjs`.

Those integrations require shared package/job/platform files beyond the four
files owned by this increment. No browser, Tauri or GPU acceptance is claimed.
