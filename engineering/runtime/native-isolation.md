# Native app-data and WebKit isolation (R06)

[R06 / #902](https://github.com/mysteropodes/nemo/issues/902). This closes the
one item [`engineering/runtime-isolation.md`](../runtime-isolation.md) left open
for the desktop app:

> Configure and exercise isolated Tauri data/autosave paths. Merely creating a
> `tauri-data` directory does not make the app use it. The platform owner must
> implement and validate the actual runtime override.

`scripts/nemo/lib/isolation.cjs` has created a per-task `tauri-data` directory
since the first R06 increment, and nothing pointed the app at it. Two pieces
close that gap and share exactly one thing — an environment contract:

| Half | File | Role |
|---|---|---|
| App | `src-tauri/src/task_runtime.rs` | Resolves the isolated roots, applies the WebKit website-data identity, widens the fs scope, writes an instance record, and serves `nemo_task_runtime` / `nemo_task_runtime_release`. |
| Launcher | `scripts/nemo/native-runtime.cjs` | Allocates a task's roots and website-data UUID, spawns the app with that environment, and owns status/stop/cleanup plus the exclusive desktop-input slot. |

## Why the WebKit half is not optional

Redirecting `appDataDir()` alone would look like isolation and not be it.
`nemo-auto` (autosave), `nemo-recents`, the sync-folder setting and the
feedback fallback store all live in `localStorage`. On macOS that is WKWebView
website data, keyed by the **bundle** identifier — nothing `appDataDir()`
controls. Two instances redirected only on the filesystem would still share one
`localStorage`, so instance B's autosave would overwrite instance A's.

`WebviewWindowBuilder::data_store_identifier([u8; 16])` gives the window its own
`WKWebsiteDataStore`. It needs the window to be built in Rust, and Tauri creates
`tauri.conf.json`'s windows itself *before* the setup hook runs. So on the
isolated path only, `run()` clears each window config's `create` flag — the one
field Tauri's own creation loop filters on — and hands the configs to
`install()`, which rebuilds them with the identity and the same labels. The
entries stay in the config, so anything else reading `app.windows` still sees
them. On the default path nothing is touched and Tauri creates the window
exactly as it does today.

## The environment contract

| Variable | Required | Meaning |
|---|---|---|
| `NEMO_TASK_DATA_ROOT` | activation trigger | Absolute path, no `.`/`..` segment, at least two directories deep. Absent ⇒ the app resolves its normal install paths. |
| `NEMO_TASK_ID` | when isolated | 1–120 ASCII letters, digits, `.`, `_`, `-`, starting alphanumeric — the same grammar as `isolation.cjs`'s `validId`. |
| `NEMO_TASK_WEB_DATA_UUID` | when isolated, on macOS | 16-byte hex UUID, dashed or not, never nil. |
| `NEMO_TASK_OWNER_TOKEN` | optional | Required to later call `nemo_task_runtime_release`. |
| `NEMO_TASK_SOURCE_IDENTITY` | optional | R02 `sourceIdentity()` JSON, echoed back verbatim by the app. |

**The data root is the sole trigger, deliberately.** `build-runtime.cjs`
already exports `NEMO_TASK_ID` into every process it spawns, so keying off the
task id could flip an unrelated launch into isolation by inheritance. A
regression covers exactly that (`a_task_id_alone_never_switches_a_normal_launch_into_isolation`).

**A malformed isolation request exits; it does not fall back.** `run()` prints
the named reason and exits 78 (`EX_CONFIG`). Falling back to the shared install
would write a task instance's history, autosave and preferences into the real
user profile — the exact failure the feature exists to prevent.

## Roots

```
<NEMO_TASK_DATA_ROOT>/
  instance.json     written by the app: task id, pid, version, identifier, executable, uuid
  data/             replaces appDataDir() for every JS consumer
    history/        version snapshots (src/js/project.js)
    autosave/
    preferences/
  cache/
  output/
  webkit/           records this instance's WebKit identity (see below)
```

`data/` keeps the layout `project.js` already writes, so `historyDir()` stays
`<base>/history/<projectKey>` and only the base moves. The launcher points
`NEMO_TASK_DATA_ROOT` at `taskRoots(taskId).tauriDataDir`, the directory
`isolation.cjs` already creates and already reaps in `releaseTask()` — no
second lifecycle to reconcile.

`webkit/` is where this instance *records* its identity. WebKit itself stores
under an OS-managed location for that UUID; `webDataStore.observedPaths` in the
profile reports any directory carrying the UUID that the app could actually see
from its own process, and an empty list is reported as *not observed*, never as
proof of either outcome.

## Frontend

`SMProject.appDataBase()` (src/js/project.js) is the single resolver: it calls
`nemo_task_runtime` and uses `roots.data` when the instance is isolated,
otherwise `appDataDir()`. `feedback-bridge.js` calls the same function.

Both readers going through one resolver is the point — `CLAUDE.md` §1's
first-family bug is exactly "a new path handled in one reader but not the
others", and version history following the task root while feedback stayed in
the shared install would be that bug. A binary without the command (an older
build) falls through to `appDataDir()`, which is also what a normal launch
resolves to.

Note that the static fs capability scopes `$APPDATA`, `$TEMP`, `$HOME` and a
few user directories. An isolated root can sit outside all of them, so
`install()` widens the scope at runtime with `FsExt::fs_scope().allow_directory`
for `data/`, `cache/` and `output/`. Without that the frontend's own `fs` calls
are denied by the plugin, with nothing visible in the UI.

## Ownership and cleanup

`nemo_task_runtime_release(ownerToken, purgeWebData)` refuses unless the
presented token matches the one this instance started with, compared without
early exit. It re-validates the root immediately before removing it, so a record
mutated after startup cannot aim a recursive delete at a shallow path. An
instance started without `NEMO_TASK_OWNER_TOKEN` refuses every release.

`purgeWebData` additionally asks WebKit to drop this identity's store via
`AppHandle::remove_data_store`. Its outcome is reported as observed, never
assumed: removing a store the running webview still holds is not something this
code can prove.

From the launcher side, `stop` refuses a mismatched owner, waits for the app to
exit, then hands the root to `isolation.releaseTask` and removes the OS-side
website-data store for this instance's identity.

### The website-data identity belongs to the task, not to one launch

`<task-root>/tauri-data/webkit/identity.json` holds the UUID, allocated once per
task and reused by every later launch of it. The first version generated a fresh
UUID per launch, which is wrong for the reason the WebKit half exists at all:
`nemo-auto`, `nemo-recents`, the sync folder and the feedback fallback live in
`localStorage`, so a new store per launch hands a relaunched task an *empty*
`localStorage` while its on-disk history is still there — its own autosave
becomes invisible to it, and every relaunch leaves another orphan store in the
user profile. Confirmed by launching the same task id twice and comparing the
two configs, before any GUI was involved.

The identity file lives *inside* the task roots, so `releaseTask` takes it with
them: a task id reused after a release starts on a store of its own rather than
inheriting the previous tenant's website data. The store WebKit itself keeps —
`~/Library/WebKit/<bundle-identifier>/WebsiteDataStore/<uuid>/`, observed on
macOS 15 — is removed by `stop` after the app has exited, using the bundle
identifier from the instance record the app wrote. Only a directory whose final
component is exactly that UUID, directly under a known per-identifier parent, is
ever a candidate; a missing identifier or an invalid identity is reported as a
named refusal and removes nothing.

## Use

```sh
# Two isolated instances. Keep each token in its controlling process.
node scripts/nemo/native-runtime.cjs start --task native-a
node scripts/nemo/native-runtime.cjs start --task native-b

node scripts/nemo/native-runtime.cjs paths  --task native-a          # no launch
node scripts/nemo/native-runtime.cjs status --task native-a --owner <token>
node scripts/nemo/native-runtime.cjs stop   --task native-a --owner <token>
```

`start` resolves the built `Nemo.app` under `src-tauri/target/**/bundle/macos`
and names every path it looked in when there is none. `--app` takes an explicit
bundle or executable; `--command` takes a stub, which is how the tests run
without a desktop process.

`status` is `ok` only when the local owner token matches, the R02 source and
build identities still match startup, **and** the app wrote an instance record
matching this task and website-data identity. A launcher record proves the
launcher; only the instance record proves the app resolved the isolated root.

Two isolated instances may run at the same time — that is the point. Exclusive
**human input** is a separate resource, because only one of them can own the
keyboard and pointer during a paired validation run:

```sh
node scripts/nemo/native-runtime.cjs input-acquire  --task native-a --pid <owning-pid>
node scripts/nemo/native-runtime.cjs input-release  --owner <slot-token>
```

## Validation

- `cd src-tauri && cargo test --lib task_runtime` — 15 tests: the default path
  stays default, a task id alone never activates isolation, two instances get
  non-overlapping roots, roots hang off the declared base, and every malformed
  or hostile request is refused rather than normalized (relative path, `..`,
  `.`, filesystem root, one-level-deep root, empty, blank, NUL byte, bad task
  ids, nil/short/non-hex UUID). Plus owner-only release, a refused release
  leaving state intact, a tokenless instance refusing every release, and
  re-validation before deletion.
- `node --test scripts/nemo/native-runtime.test.cjs` — 13 tests: disjoint roots
  and distinct website-data identities, the emitted environment satisfying the
  app-side rules, valid non-nil v4 UUIDs, `.app` bundle resolution, two live
  concurrent instances each writing their record into their own root only,
  independent stop authority, owner-refused status/stop leaving the app and its
  state untouched, an unconfirmed-isolation handshake, the input slot, one
  website-data identity per task across relaunches (and a fresh one after a
  release), and a release that removes exactly this instance's store while a
  peer task's store survives.

Both suites reach normal discovery through `tests/nemo-native-runtime.test.cjs`
and `npm run test:rust`.

## Limitations and open gates

- **The paired two-instance desktop run is the remaining gate.** Everything
  above is pure-logic and launcher-level. That a *real* Nemo resolves these
  roots, and that two WKWebView website-data stores are genuinely separate on
  disk, needs a built app, two launched instances and the exclusive input slot.
  Browser evidence cannot substitute for it.
- `data_store_identifier` needs **macOS ≥ 14**, and Tauri returns no result for
  it. `webDataStore.applied` therefore means "the webview was built with this
  identity", not "WebKit accepted it". `observedPaths` is the only app-side
  evidence, and it is reported as observed-or-not, never inferred.
- `declaredSource` is echoed verbatim from the launcher. It identifies the
  launcher that configured the instance, not the checkout the binary was
  compiled from; the app's own build identity (version, identifier, executable
  path and size) is reported separately.
- Non-macOS is refused before any task state is created. The `.app` layout and
  the website-data identity are macOS-specific and unvalidated elsewhere.
- Owner tokens are cooperative local process bookkeeping, inheriting the limits
  [`runtime-isolation.md`](../runtime-isolation.md) already documents. They are
  not a boundary against another process running as the same user.
- No npm script or `lib/jobs.cjs` entry points at this launcher yet, and
  `engineering/boundaries/profiles/scripts-nemo.profile.json` has no module
  entry for it. Both files belong to other owners; the wiring is requested, not
  applied here.
