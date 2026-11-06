# Packaged native acceptance prerequisite

Run from a clean, committed integration checkout on macOS with its built app:

```sh
NEMO_DESKTOP_APP=/absolute/path/Nemo.app node --test tests/desktop/*.test.cjs
```

The standard `test:desktop` job owns discovery and receipt wiring. Optional
`NEMO_DESKTOP_REPORT_DIR` receives a sanitized `packaged-native.json` result.
The supplied bundle must contain an executable Mach-O binary. Missing app,
unsupported platform, dirty candidate, unavailable reservations or incomplete
runtime evidence fail this direct invocation; no green skips are used.

This gate uses the current checkout's `native.cjs` and `native-runtime.cjs`,
including `stop --retain-data` and the native manifest's `processEnvironment`.
It launches two actual native processes, reserves `desktop-input` and
`gpu-reference` explicitly, and proves:

- task, process, owner, launcher source/build and app-manifest identity;
- actual child temp/cache/report environment and distinct application roots;
- distinct WebKit data stores materialized by the packaged app;
- foreign owner status/stop refusal and exclusive reservation refusal/release;
- same-task stop/relaunch preserving storage sentinels;
- process-group exit before default cleanup, while the other instance survives.

Use the same `NEMO_ISOLATION_ROOT` as other coordinated machine tasks (normally
the runtime default). A private alternate root would test an independent slot
namespace. The gate never searches for, stops or deletes other task instances.
It retains state and fails if cleanup authority or process exit is uncertain;
reconcile that exact owned run before retrying. Raw launcher responses, owner
tokens and host paths are consumed privately rather than copied into reports.

The sentinels are harness-written filesystem data. Their survival does **not**
prove Nemo document save/reload through the UI, WebKit storage APIs, rendering,
GPU output, or final R06 acceptance. The launcher identity and executable hash
also do not prove embedded app source provenance: the integration owner must
build the identified clean candidate, retain the build receipt, then execute the
remaining real UI workflow separately.

Test harness failure handling without a desktop or live slots:

```sh
node --test tests/desktop/unit/native-harness.test.cjs
```

These controls use disposable Node processes only; they cannot substitute for
the packaged-app invocation above.

## CS7 installed batch driver

`cs7-run.cjs` is an explicit manual entry point. Book the Local app/input slot
in #2804 and run the entire command under `lockf -k /tmp/nemo-heavy-job.lock`.
Build the normal app and the UI32 helper from the same clean, committed candidate.
The private JSON config names `sourceSha`, `app`, `executableSha256`, `buildProof`,
`helper`, `helperProof`, `taskId`, `grant` (`2804/<comment-id>`, a booking in the current log; #1062 is full and cannot hold a new one), an existing empty
`reportDirectory`, and an optional supported `fixtureKind`.

```sh
lockf -k /tmp/nemo-heavy-job.lock node tests/desktop/cs7-run.cjs /absolute/private-config.json
```

The driver reads **explicit** declarations from
`engineering/inventory/parity/installed/*.json`. Their schema is
`nemo.installed-parity/1`; each `rows` entry declares `id`, `issue`, `status`
(`Ready` or `Blocked`), existing `tasks` and `surfaces`, `dependsOn`, `scenario`,
and `checks` containing an `id` and family-acceptance `covers` kinds. Tasks with
no matrix surfaces may declare an empty surface list. Every Ready scenario must
exist as a reviewed `cs7-scenarios/<scenario>.cjs` module exporting
`run({session,row})`. It returns every declared assertion's `pass`, `fail` or
`unrun` status and an explicit `safeToContinue` disposition. The scenario must
assert the original product contract from actual observations; successful input
posting is never a passing assertion. `session.input` uses the existing closed
UI32 semantic command protocol and fresh runtime identity checks.

### Running a subset of Ready rows

By default every `Ready` row is loaded, prepared and run, so the existing rows' `prepare()`
steps (observer package, keyed fixture, private Text20 packet) are all required. An optional
`rows` array in the private config limits the batch to the named row ids:

```json
{ "rows": ["play-resume"], "sourceSha": "...", "reportDirectory": "..." }
```

Only the listed rows are loaded, prepared and run; the other rows' scenarios are never
`require`d. `rows` must be a non-empty list of unique ids of rows that are `Ready` in the
declarations. A row's `dependsOn` rows must be listed too; they are never added for you.
An empty or malformed list, an unknown or `Blocked` id, or a missing dependency refuses
**before any session is created or the app is launched**. With no `rows` key nothing changes:
same rows, same receipt, same stdout.

`installed-pass` still means every *selected* row passed. A subset run adds a `selection`
object right after `verdict` in `installed-results.json`, each progress file and the stdout
line: `{ "scope": "subset", "selected": [...], "omittedReady": [...] }` (`"all-ready"` when the
list names every Ready row). Read a subset `installed-pass` as "these rows only"; the omitted
Ready rows were not run and the receipt is not evidence about them.

### Rows in package-h (frame-edit-ui, settings-profile)

Both rows are `Ready` with merged-PR evidence and run with no private packet, so
`"rows": ["frame-edit-ui", "settings-profile"]` selects exactly them. Neither has been run installed.
Run them in that order or separately; both capture with distinct names (`capture-1`, `capture-11`..`14`).

- `frame-edit-ui` (#2248, foreground). A person presses the real F5/F6/F7, `+`, a bound Duplicate
  keyframe key, Cmd+Z / Cmd+Shift+Z and the timeline right-click items, in the scene and inside an
  entered component (PR #2786). After every gesture they attach the unedited **read-only** Nemo MCP
  replies (`query.document.object.history`, one `query.document.object.layer.timing` view `cells` per
  layer, plus the Scene layers while inside a component). The oracle computes the revision step,
  history depth, total frames and cell runs itself, reads the saved files from disk (hash, total
  frames) and judges the layer-row styling from the helper's own owned-window capture. A reply with a
  verdict field, missing/unpinned/non-tiling reads, or a `command.*` call is refused as operator input.
- `settings-profile` (#2172). `session.input` drives the Settings entries, the eight tabs, close and
  backdrop, profile name/email/role with restore, the tool buttons and the empty sync-folder
  open-cancel/check; `observe` supplies the raw accessibility facts. The helper **cannot** drive, so a
  person performs (and the oracle judges the effect of):
  - File > New (the `entry.*` commands need an entry mode this batch does not set);
  - the colour swatch and its alpha (judged from the helper capture of the swatch the person located);
  - rebinding a shortcut and "Reset all" (`shortcuts.reset` is denied; judged by comparing the Shortcuts
    tab labels before, after rebind and after reset);
  - the whole `sync-owner-paths` clause: drawing one stroke and a blank keyframe, then Publish with no
    folder, Choose… in the native picker (the exact folder is named in the phase), Publish, Check, a
    Merge row and a Publish into a folder the runner has made read-only. The helper has no Publish,
    Merge or non-empty-folder command (it refuses a non-empty path), so every helper sync step runs first.
  The sync oracle reads the disposable shared folder itself (what the app wrote, and the second profile
  it seeded from the app's own published snapshot), the accessibility read (folder path, update rows,
  Merge/Merged buttons, last toast text, which stays in `#toast` after it fades, so freshness is also
  checked) and unedited MCP cell reads of the document before and after Merge. The clause is `unrun` with
  its reason when a toast, the update list or the published snapshot shape is not readable, and `fail`
  when the baseline contract is contradicted (for example the merge toast says `+1 ajout(s)` but frame 4
  still has no content). #2355 (D06) migrates publish/pull/merge to native snapshots, so a first-run
  failure there is a product finding. Things only the first installed run can show are never guessed: the
  Producer tool state is trusted only after Brush then Hand moved the buttons' `selected` state (else
  only the literal baseline toast speaks), a helper refusal of the backdrop press leaves that part of
  `settings-entries-tabs` unrun, and MCP replies not shaped as read are kept privately as
  `frame-edit-ui/mismatch-<phase>.json` with the reason. Profile persistence across an app restart needs
  a second lifetime and is not claimed.

The first operator phase of both asks for the window at the top of display 2 (position 2880,-270, size
1620x1420, raised).

### Refusals

A refusal before or around launch prints one line to stderr:
`CS7 batch refused or incomplete (<reason>); inspect its private receipt. No acceptance claimed.`
`<reason>` is the harness check's fixed message (for example `selected installed row b depends
on a row outside the selection: a`). Any other error (file system, JSON syntax, scenario
code) is reduced to its class and code, such as `SyntaxError` or `Error ENOENT`, so paths and
evidence content never reach the terminal. Failures after launch are classified in the private
receipt as before.

Missing declarations or scripts refuse **before app launch**. The existing
matrix's issue status and headless `source-pass` do not establish readiness.
This infrastructure does not itself declare the B3 cohort Ready or implement
its original acceptance scripts; those remain required before CS7 acceptance.

One runtime spans the batch. Input/precondition failures stop it without replay;
product failures may continue only with the scenario's explicit safe disposition.
Dependent rows remain blocked, and every unrun clause remains in the receipt.
Cleanup runs even after partial startup, input timeout or evidence-write failure.
Results retain the family result fields in an explicit
`nemo.installed-family-result/1` extension with `format: nemo.parity-family-result/1`.
`installed-pass` requires all rows, unchanged candidate identity, zero operator
failures and complete cleanup. These receipts stay in the private report folder;
they are never inserted into the headless `results/` census as `source-pass`.
