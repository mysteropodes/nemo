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
no matrix surfaces may declare an empty surface list.

`tasks` lists only tasks whose **every** matrix surface (the surfaces whose own `tasks` name it) is in the
row's `surfaces`. A task the row exercises only in part goes in `partialTasks`, an object
`{ "<task>": ["<matrix surface the row does not exercise>", ...] }` that lists exactly the missing surfaces
(never empty, never a task also in `tasks`). The runner refuses a row that lists a partly exercised task in
`tasks` or states a wrong missing list, before any session is created, and a node test checks every shipped
declaration the same way. The receipt keeps declarations under `expected`; top-level and per-row
`tasks`, `partialTasks`, `surfaces`, and `operatorAttestedSurfaces` show only coverage from rows whose checks
pass, whose scenario permits continuation, and whose owner cleanup and deferred evidence finalizers complete.
Failed, unsafe, blocked, unrun, pending, or cleanup/finalizer-failed rows contribute no actual coverage.
Subset runs retain expected scope while actual coverage is limited to validated selected rows. Read
`partialTasks` as "part of this task was exercised; the listed surfaces were not": a pass is never acceptance
of those. Non-inventory keyboard routes may be separately recorded as `operatorAttestedRoutes`; they do not
count toward task surface coverage. A Ready row needs at least one task in `tasks` or `partialTasks`. Every Ready scenario must
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

`installed-pass` is the issue #2348 outcome: every `Ready` row ran on one candidate and passed. A run
with a `rows` list that leaves any Ready row out gets the distinct verdict **`installed-subset-pass`** when
all its selected rows pass, and a failing or incomplete subset is `fail` or `incomplete` as usual. A subset
run adds a `selection` object right after `verdict` in `installed-results.json`, each progress file and the
stdout line: `{ "scope": "subset", "selected": [...], "omittedReady": [...] }` (`"all-ready"` when the
list names every Ready row, which keeps the `installed-pass` verdict). The omitted Ready rows were not run
and the receipt is not evidence about them.

Exit status of `cs7-run.cjs`: `0` only for `installed-pass`; **`3`** for `installed-subset-pass`, with one
line on stderr (`CS7 subset run passed (installed-subset-pass): N selected row(s) passed, M Ready row(s)
were not run; this is not the every-Ready-row outcome (exit status 3).`); `1` for anything else (a refusal,
a failure, an incomplete run). A script or check that wants the #2348 outcome must test for `0`, not for
"not 1".

### Document reads are made by the harness, not the operator

A row may pass only on observations the harness made itself of the candidate under test. What a person or
agent pastes in is not proof of document state, so the operator's replies carry **no** MCP reads
(`reads`, `sceneLayers` and `kinds` in a reply make it malformed, which stops the batch). After each reply
the scenario calls `session.read(fn)`, the only document-read route a scenario has. It can issue nothing
but read-only `query.*` operations (a `command.*` or any other name is refused before it is sent).

The route is the one any MCP client uses: the `nemo-mcp` binary bundled in the package under test
(`Contents/MacOS/nemo-mcp`, which must match the package proof when the proof lists it) started with
`NEMO_MCP_REGISTRY` set to the candidate's own task-scoped registry (`<tauriDataDir>/mcp`), calling
`nemo_discover` and `nemo_command` (native v2 `query.document.object*`). The route reports no pid or
executable hash, so a reply is bound to the candidate like this (`cs7-read.cjs`), and the batch fails closed
(no pass) if any step differs:

1. the owner identity (pid, source sha, tree, driver sha, executable sha256) is read before and after every
   batch and must equal the identity captured at start;
2. the registry holds exactly one record, the discovered instance is that record (instanceId, buildId whose
   version is the candidate's app version) and its native host is available with a document;
3. the OS listener on the record's port (`lsof`) must be exactly the candidate's pid;
4. every ok reply carries the batch's instanceId and documentId, and the document id, revision, lifecycle
   generation and build id are identical before and after the batch.

A reply from another running Nemo (a daily-driver build, an older candidate) cannot satisfy these. Limits,
stated plainly: the bundled `nemo-mcp` binary is checked against the package proof only if the proof lists
it, and the route itself still has no pid or executable hash of its own (the listener check supplies the
pid); reporting the process id and executable sha256 in the native host status (`NativeHostStatus`) would let the reader compare them directly instead of using the listener check. Each batch's status
tuples are kept privately as `<row>/reads-<phase>.json`; a refused batch as `<row>/read-refused-<phase>.json`
with its fixed reason. The operator must therefore **touch nothing in the app after replying**: the read
happens right after the reply, and a change during the batch is refused.

### Rows in package-h (frame-edit-ui, settings-profile)

Both rows are `Ready` with merged-PR evidence and run with no private packet, so
`"rows": ["frame-edit-ui", "settings-profile"]` selects exactly them. Neither has been run installed.
Run them in that order or separately; both capture with distinct names (`capture-1`, `capture-11`..`14`).

- `frame-edit-ui` (#2248, foreground). A person presses the real F5/F6/F7, `+`, a bound Duplicate
  keyframe key, Cmd+Z / Cmd+Shift+Z and the timeline right-click items, in the scene and inside an
  entered component (PR #2786). After every reply the harness itself makes the **read-only** Nemo MCP
  reads (`query.document.object.history`, one `query.document.object.layer.timing` view `cells` per
  layer, plus the Scene layers while inside a component); the operator attaches nothing. The oracle computes the revision step,
  history depth, total frames and cell runs itself. The saved files are measured by the runner (hash, size, mtime, around the
  Save As step) and compared with the verified layers and key frames; reopening must give a new document, leave the file
  byte-identical and show its name. The layer-row styling takes no operator phase: the runner captures the owned window once,
  reads the PNG once (its sha256 is kept) and takes every rectangle from the helper's accessibility frames (compact means a
  row at most 34 points high and visibility/lock/shy controls at most 28 high and 90 wide). A reply with a
  verdict field or any attached read, or harness reads that are missing, unpinned or non-tiling, stops the batch.
- `settings-profile` (#2172). `session.input` drives the Settings entries, the eight tabs, close and
  backdrop, profile name/email/role with restore, the tool buttons and the empty sync-folder
  open-cancel/check; `observe` supplies the raw accessibility facts. The helper **cannot** drive, so a
  person performs (and the oracle judges the effect of):
  - File > New (the `entry.*` commands need an entry mode this batch does not set);
  - the colour swatch and its alpha (judged from three helper captures; the swatch rectangle is the accessibility
    frame of `#profile-color-well`, the ground is four samples just outside it, and a swatch that moved between
    captures leaves the clause unrun);
  - "Reset all" first, then rebinding a shortcut and "Reset all" again (`shortcuts.reset` is denied; the baseline is
    the tab after the first Reset all, so a binding left by `frame-edit-ui` is not blamed on the product; judged
    by comparing the Shortcuts tab labels before, after rebind and after reset);
  - the eight tabs are judged by the one element only that pane renders (`profile-name`, `app-check-update`,
    `sync-folder-path`, `fb-pull`, `figma-token`, `shortcuts-reset`, `labs-tools-list`, `app-version-txt-about`);
    a tab showing another pane's element fails, one whose element is not in the accessibility read leaves the clause unrun;
  - the empty-folder check passes only when the baseline's completion text ("Aucune mise à jour d'un autre profil.")
    appears after the press and was not on screen before it; otherwise it is `unrun`;
  - the whole `sync-owner-paths` clause: drawing one stroke and a blank keyframe, then Publish with no
    folder, Choose… in the native picker (the exact folder is named in the phase), Publish, Check, a
    Merge row and a Publish into a folder the runner has made read-only. The helper has no Publish,
    Merge or non-empty-folder command (it refuses a non-empty path), so every helper sync step runs first.
  The sync oracle reads the disposable shared folder itself (what the app wrote, and the second profile
  it seeded from the app's own published snapshot), the accessibility read (folder path, update rows,
  Merge/Merged buttons, last toast text, which stays in `#toast` after it fades, so freshness is also
  checked) and MCP cell reads of the document, made by the harness itself, right before and after Merge: the merge needs a fresh
  `Fusion de ... : +1 ajout(s)` toast, a later revision, an undisturbed history, content on frame 4 and the Merged row. The clause is `unrun` with
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

### Rows in package-h, part 3 (component-edit-draw, legacy-import)

Both rows are `Ready`, run with no private packet and have not been run installed;
`"rows": ["component-edit-draw", "legacy-import"]` selects exactly them (the older rows stay omitted).
Neither needs a fixture from the operator: `legacy-import` copies the repository's own baseline files
(`tests/fixtures/legacy-import/animation.json`, `media.json`, and two files derived from `single-frame.json` that
add `exprGlobals` or `trackRoles`) into `<report>/legacy-source/` and asks the operator to open those copies.

- `component-edit-draw` (#2256, foreground; PRs 2765, 2784, 2786, 2800, 2803). A person builds a small
  document (a drawn layer, a component with a blank key on frame 2, an empty "Plain" layer, a "Bake" component),
  draws one gesture per tool inside the entered component (Draw, Pen, Line, Rectangle, Ellipse, Fill Brush, Bitmap
  Brush, each on its own empty cell), draws with Draw, Pen and a rectangle in Motion mode on "Plain", uses
  Cmd+Z / Cmd+Shift+Z, leaves the component, exports two PNGs, breaks the "Bake" component apart, saves and reopens.
  After each reply the harness makes the read-only MCP reads itself (history, per-layer cells, for Break apart
  the scene layers page of `component.read`, and for the Bitmap Brush `query.document.object.brush.bitmap`); the operator
  attaches none. The oracle computes the revision step, the history step, which cell gained content or its own key, that the
  Scene stayed unchanged and that Break apart made a drawing layer holding the component's keys. After every gesture the
  harness also reads the helper's accessibility facts: the one tool button with the page's `active` class must be the tool the
  step names (a different tool is the operator's mistake and stops the batch), the Fill Brush "Above" button must be active,
  Motion steps need the Motion mode button active, and the playhead box (`info-frame`) must show the step's frame; whatever
  cannot be read leaves the clause `unrun` with the reason. The Bitmap Brush stroke must have a bitmap-brush record with ink
  in the native read. A Motion stroke after the first is passed only when the layer rows change; when they do not (the stroke
  went onto the origin key) the clause is `unrun`, since the edit cannot be told from any other. The instance pixels take no
  operator rectangles: the runner captures the owned window before entering and after leaving the component at playhead frame 9,
  reads each PNG once and compares them inside the accessibility frame of `drawing-canvas` (at least 200 pixels changed in at
  most 24 of 48 tiles; the window, canvas and playhead must not have moved). The bitmap export is a baseline PNG
  (`exports/bitmap-baseline.png`, exported before entering the component) and `exports/bitmap-brush.png`, both with the
  playhead on frame 3, neither existing before its step; the runner measures their hash and mtime and compares them. The
  saved file is measured by the runner and compared with the verified scene and component layers and key frames, and
  reopening must give a new document. A uniform capture leaves the pixel clause `unrun` with the reason. Not exercised
  (see the row's `taskEvidence`): Text, the Fill tool, Pen continue, Perspective snap, bitmap re-bake on Select, component camera keys and markers.
- `legacy-import` (#2355; PRs 2772, 2774, 2777). A person opens the copied baseline files through the native picker,
  presses Cmd+S, Save As onto the original, F5, Cmd+Z, Cmd+Shift+Z and opens the saved file. The scenario measures the
  sha256, size and modified time of every old file after every phase, compares the opened document (scene frames,
  component frames, layer names) with what the file declares, reads the saved files from disk and reads the toast
  text from the helper's accessibility observation. A toast it cannot read leaves its clause `unrun` with the reason.
  It does not judge that Motion keys, expressions, pictures or audio render or play as in the old app.

### Refusals

A refusal before or around launch prints one line to stderr:
`CS7 batch refused or incomplete (<reason>); inspect its private receipt. No acceptance claimed.`
`<reason>` is the harness check's fixed message (for example `selected installed row b depends
on a row outside the selection: a`). Only an error raised by the harness `requireCheck` is quoted. Any other error (file system, JSON syntax, scenario
code, even a plain `Error` with a short message) is reduced to its class and code, such as `SyntaxError` or `Error ENOENT`, so paths and
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
`installed-pass` requires all Ready rows, unchanged candidate identity, zero operator
failures and complete cleanup (a passing subset is `installed-subset-pass`, exit status 3). These receipts stay in the private report folder;
they are never inserted into the headless `results/` census as `source-pass`.
