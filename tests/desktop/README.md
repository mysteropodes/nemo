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
`nemo_discover` and `nemo_command` (native v2 `query.document.object*`). The route reports the process id (`pid`) but no
executable hash, and only the instance's own report of it, so a reply is bound to the candidate like this (`cs7-read.cjs`), and the batch fails closed
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

## Installed client evidence without input (`installed-evidence/`)

`installed-evidence/run.cjs` lets an agent produce installed-app evidence for the "installed/client acceptance" clause of lanes
#2107 (vectors, parametric shapes, path effects), #2108 (groups, boolean, eraser, layer lifecycle, selection and clipboard),
#2112 (raster and text assets, text layout) and #2113 (typed scene and style producers, bitmap brush, evaluate versus export)
with **no keyboard or mouse input**. It builds nothing: it takes an already built package, starts it in a fresh isolated
session (`scripts/nemo/native.cjs` through `native-harness.cjs`, task roots allocated by the launcher, data removed on stop) and
drives it only through the package's own `Contents/MacOS/nemo-mcp` (stdio JSON-RPC: `nemo_discover`, `nemo_command`).
Run it as a manual, heavy job, under the same lock and local booking as other app runs:

```sh
lockf -k /tmp/nemo-heavy-job.lock node tests/desktop/installed-evidence/run.cjs \
  --app /abs/Nemo.app --build-proof /abs/build-proof.json --report-dir /abs/empty-dir \
  [--lane 2107,2108,2112,2113|all] [--row 2107-path-effects-stack,...] [--task ID] [--source-sha SHA] [--reserve gpu-reference|none]
```

The package comes from `NEMO_TASK_ID=<task> lockf -k /tmp/nemo-heavy-job.lock npm run build:desktop` on the clean, committed
checkout; `--build-proof` is its `build-proof.json`. The window opens on the screen like any run of the app; the runner never
sends input to it. Slots: `gpu-reference` is reserved by default for the PNG exports (`--reserve none` or `--reserve a,b` changes it); no `desktop-input` slot is needed.

**The runner refuses (exit 2) unless the instance it talks to is the candidate.** Before the first row, and before and after
every row, it checks: the clean checkout head equals the proof's source head; the package proof lists the app executable and
the bundled `nemo-mcp` by sha256 and both match the files about to run (`requireMcpPinned`, `verifyPackage`); the registered
build id equals the app version plus the clean source head (full string, no `-dirty`); exactly one application is discovered and
it is the registered instance; the OS listener on the registered port is the pid the launcher started (`lsof`); the executable
bytes and the pid are unchanged. On every request the wire re-discovers and requires that one application to keep the registered
instance id and build id, and a reply that names another instance, document or request id is `foreign` or `malformed` and stops
or fails the row. Cleanup always stops the app and closes `nemo-mcp`; an app process that is still alive, or a stop that is
refused, fails the run.

**What a row proves.** Each row starts from `project.new` (640x360 for the pixel oracle) and asserts the product contract from raw
observations, never from "the command returned ok": for every command the reply (`applied`, `historyEntriesAdded`), then
independent reads: the document revision from `nemo_discover` is exactly +1 and history depth (`query.document.object.history`)
exactly +1 with redo cleared; a no-op or refused command moves neither; undo and redo move one revision and one entry in the right
direction and restore or reapply the same state; `object` reads give the record that was sent; the PNG exported through the app
(`project.export`) is decoded and measured (ink bounds, colours at probe points, coverage ratios, equality of decoded pixels);
`project.saveAs`, `project.open` in the same session reproduces the export and the objects, and a second save is byte-identical.
A row passes only if every assertion passes and at least one is an independent observation of the row's own (a read, a pixel or a
file that is not kit bookkeeping: setup and the per-command revision and history deltas are recorded as `bookkeeping` and never
count), and a step wrapped in `ctx.behavior` fails when its claim rests on replies and bookkeeping alone. Save then open loads the
file from a copy at another path and requires a different `documentId` (and an advanced `lifecycleGeneration` when reported), so
an app that kept the in-memory document cannot pass. A negative control counts only as a typed refusal by the native owner: a
`frontDoor` reply (nemo-mcp's own request-shape check) fails it, and so does `unavailable` or `internal`; a control names the codes
its rule produces (default `invalid_request`, `not_found`, `busy_conflict`; `stale_revision` for the stale-revision control; a
missing image source is `unavailable` with `disposition: notDispatched`). Controls come from the lane texts: stale target, stale
revision, locked layer, a layer with an active path effect, a corrupt PNG, a decodable PNG one pixel past the 8192 size cap, a
missing file, an asset still in use, an absent font, an unknown asset, a wrong-size or unknown bitmap-brush publication. An asset
the catalog does not hold is a no-op by contract (not a refusal). Resource publications (`brush.bitmap.publish`) are judged as
resources: no revision, no history entry. There are no sleeps; every wait is the reply of the operation itself.

**Rows.** #2107: `2107-vector-create-export-reopen`, `2107-stroke-style-pressure-fields`, `2107-vector-replace-delete-undo-redo`,
`2107-parametric-set-translate-scale`, `2107-path-effects-stack`, `2107-canvas-alpha-and-opaque-export`,
`2107-parametric-ellipse-polygon-star`, `2107-parametric-rotated-box`. #2108: `2108-group-combine-flatten`, `2108-boolean-apply-erase`,
`2108-layer-lifecycle`, `2108-layer-reorder` (runs last), `2108-selection-clipboard-remap`, `2108-nested-group-structure`,
`2108-group-stack-reorder`, `2108-nested-group-operand-flatten`, `2108-linked-fill-join`, `2108-flatten-element-motion`. #2112:
`2112-asset-attach-dedup-refusals`, `2112-asset-remove-lifetime`, `2112-raster-layer-delete`, `2112-raster-edit-save-reopen`,
`2112-text-layout-edit-reopen`, `2112-text-layout-box`. #2113: `2113-mixed-scene-one-export`, `2113-mixed-z-order-reorder` (runs last),
`2113-image-mesh-sculpt`, `2113-bitmap-brush-resources`, `2113-evaluate-vs-export-pin`, `2113-bitmap-brush-quota-and-cancel`. The mesh id and
offsets are read through the object query's `imageMesh` view (the `image.mesh.edit` reply is only its receipt); group paths and modes, text
glyph boxes, dynamic-shape intent and fills are read through the hit-test listing (`query.document.object`, view `subselect` with `hitTest`:
role, bounds, groupPath outermost first, paramShape, boxAngle); the anchors of a large record are paged (the whole-object read refuses past
4096 bytes).

**Ink is alpha or colour, never one standing in for the other** (`png-probe.cjs`). `project.export` has no alpha option and fills the project
canvas background, so an opaque export can not tell a white-painted item from nothing (colour distance 0). Rows that assert clear, hole,
no-ink or outside-the-shape therefore run on a transparent canvas (`K.fresh(ctx, { transparent: true })`: one
`command.document.object.project.settings.edit` `setCanvasBackground` `#00000000`, capabilities-v2/native-project-settings.json) and measure
ink as the alpha of the export; colour probes (`probe.over`) are kept for paint colour. `2107-canvas-alpha-and-opaque-export` observes both
on one scene: an opaque default canvas, a transparent one, white paint as alpha 255 with colour distance 0, undo and reopen. The digest of an
export ignores the colour channels of fully transparent pixels (rounding residue, not something an export shows).

**What a lane covers.** A lane total counts the rows that ran, not the lane. `coverage.cjs` assigns ids per lane and the receipt states, per id,
exactly one of **covered** (a passing row names it), **attempted-not-passed** (rows ran, none passed: failing rows are listed with their
reason) or **never attempted** (no row for it ran, or only unrun ones); a covered id still lists its failing rows (`alsoNotPassed`), and
`coverage.notCovered` lists what no row observes with the read that would be needed: glyph identity (a `layout` view of
`query.document.object.text`), the object-project evaluate read (`query.document.evaluate` is refused on an object project; the evaluated
output is observed as export alpha and the authored value as the property read), component and rig producers, component and folder layers.
An id with an unobserved part (G38b glyph identity, I16a the document evaluate read) is `partial` even when its rows pass. A unit test fails when an assigned id is neither covered by a row nor listed. Select by
lane (`--lane`) or row (`--row`); an unknown lane or row refuses before anything starts. A row whose operation the package does not
offer (a front-door `unsupported_capability` or `unavailable` from `nemo-mcp`, or a host that reports no lifecycle generation) is
`unrun` with the reason, never `pass`; a refusal by the application itself is an observation the row judges. Request payloads are built by
`payloads.cjs` only, and a unit test validates every builder against `native-transport-v2.schema.json` and the item and bitmap-brush
schemas.

**Retries are data.** `project.saveAs` right after a change is refused intermittently by the candidate (`unavailable`, "pin-changed-before-picker" or
"publication-response-unavailable") and the next request is accepted, and a hit-test listing right after a command can answer "Subselect
presentation changed". The kit retries (at most four attempts) and records every retry on the row result: `retried`, and per retried operation
`{operation, label, attempts, refused, accepted, refusals: [{attempt, code, message}]}`; `totals.retries` (overall and per lane) counts retried
operations, refused attempts, rows touched, operations by name and `exhausted` retries, and the stdout line lists `retriedRows`. A retry
never turns a failure into a pass: a save that is still refused after four attempts fails the row.

**Receipt and exit status.** `installed-client-evidence.json` in the report directory (and `transcript.jsonl`, the requests and raw
replies, private, mode 0600) records the candidate identity (source head, tree, app version, build id, instance id, pid, task id,
executable, `nemo-mcp` and proof digests), per row the operations sent (request and reply digests, revisions), every assertion with
`observed`, `expected` and kind, a verdict `pass`, `fail` or `unrun`, totals per lane (with the catalog row count and whether the
lane ran in full), the coverage table, and the selection (`selection.omitted`). Exit status: `0` every catalog row ran and passed;
`1` a row failed or cleanup was incomplete; `2` refused before any row; `4` no failure but at least one row is `unrun`; `5`
`subset-pass`: the selected rows all passed but the selection left rows out (a one-row receipt is not a lane receipt; the stdout
line carries the selection and the uncovered ids). The
receipt states that it is not the CS7 batched session (#2348) and that it does not record any lane's installed/client acceptance
disposition; the rows that need real input (drawing-tool strokes, keyboard shortcuts, UI buttons, how text looks in the viewport,
the export modal and video export) stay hands-on and are listed in `residualHandsOn`.

**Pre-flight: a locked screen is refused, not timed out.** A locked macOS login session (or a covered Nemo window) stops the window from
presenting, so the app never completes its first command (`project.new`). `candidate.cjs` therefore reads the session before anything starts
(`session-preflight.cjs`: `ioreg -n Root -d1`, `CGSSessionScreenIsLocked` = Yes in the console user entry) and refuses with exit status `2`
(`refused`) and the reason in the receipt, for `run.cjs` and for `f03-clients.cjs` alike. Only a positive reading refuses; a host without
`ioreg` is not stopped. When the first `nemo_command` after launch still times out, the row reason says the window is probably not
presenting (Mac locked or window covered) with the underlying timeout, instead of a bare transport timeout; a later timeout stays a plain
transport failure. Do not work around either: unlock the Mac, keep the Nemo window on screen, run again.

Known limits, stated so nobody reads more into a pass: the only public read of a text layout is one glyph box per block (the hit-test
listing), compared with the ink of the export, so glyph identity and advances are not observed; the CS7 read route and this runner both bind replies to the candidate through
the registry, `lsof` and the build id because `nemo-mcp` reports no pid of its own; rows cannot show that a UI caller reaches the
owner, only that the owner honours the contract. Unit tests (`node --test tests/desktop/unit/installed-evidence-*.test.cjs`) run the kit, wire
and runner against stub transports only: a real pass, a product failure, a foreign instance, malformed replies and an unavailable
operation, plus one injected fault per kit guard (`installed-evidence-defects.test.cjs`: an unbound mesh, a front-door or wrong-code
refusal, an edit disguised as a publication, a host that keeps the in-memory document on open, a flatten that only bumps the
revision, an export file changed on disk) and `installed-evidence-reads.test.cjs` (the hit-test listing, anchor pages, element property read,
canvas background edit, brush cancellation and quota refusals answered with literal copies of the candidate's replies; coverage that counts
only passing rows; every retry recorded). The stub follows the published reply shapes; it is not what is being proved. The first real runs
(task evidence-1112, receipts kept outside the repository) found four product defects the rows keep reporting rather than work around:
`group.combine` is ignored by the export, `layer.reorder` commits and then answers an indeterminate refusal that leaves the session unusable
(those rows run last), and `layer.delete` refuses a layer holding a raster placement. (An earlier reading, "flatten refuses a group that contains a nested group", was the row's mistake: flatten folds ALL leaves once with the outer mode, so Outer(intersect: big, Inner(unite: two disjoint bars)) has an empty fold and is refused `invalid_request` with the document unchanged, which is the native owner's deliberate, safer deviation from the baseline that silently deleted the members. `2108-nested-group-operand-flatten` observes the paint of a nested combining group as one operand, that refusal, and a case whose fold is non-empty flattening to the baseline's refit of that fold: flatten ends in `path.simplify(2.5)` when no slit is kept, so a plain box fold is a 2-3 anchor curve, not the sharp box, and the result keeps the style member's stroke id. A combined group paints with the topmost member's style, which is why `2108-group-combine-flatten` expects the inner rectangle's colour on the ring.)

## Real client workflow, Codex and Claude Code (`installed-evidence/f03-clients.cjs`, #1049)

Runs the issue's original opacity workflow (static 25, set 40 then 60, save, undo, an out-of-date write, save, replace the project by
the keyed fixture, a write against the replaced project, read keyed frames 0 and 20, reopen the saved file) twice against an already
built package: through the real Codex CLI (`codex exec`) and through the real Claude Code CLI (`claude -p`). Each half has its own fresh
isolated session, bound to the candidate by `candidate.cjs` exactly like `run.cjs`, and a per-invocation MCP configuration naming the
package's own `nemo-mcp` and that session's registry; no user or project client configuration is touched and no input is sent.
The identification rule is in `engineering/remediation/reference/03_TESTING_AND_DEBUGGING.md`.

```sh
lockf -k /tmp/nemo-heavy-job.lock node tests/desktop/installed-evidence/f03-clients.cjs \
  --app /abs/Nemo.app --build-proof /abs/build-proof.json --report-dir /abs/empty-dir [--client codex,claude] [--timeout-min 20] [--task ID] \
  [--claude-model sonnet] [--codex-model NAME]
```

**The model is pinned, not whatever the client defaults to.** `claude -p` is always run with an explicit `--model` (default `sonnet`,
override with `--claude-model`); `codex exec` gets `--model` only when `--codex-model` is given. The receipt records the requested model next to
the client version (`clients[].requestedModel`: the Claude model, or the Codex model / `client default`), and the recorded command line shows the flag.

The prompt names goals and file paths only (no tool, operation or payload names) and says which kind of project it is, so the client
derives requests from the advertised schema. The fixed fixture (`tests/animation/fixtures/curve-workflow.json`, pinned by hash, as the
static and keyed shells of N21) opens as an **object project**, which answers only the native operations (apiVersion 2) plus
`project.open`/`saveAs`/`new`: the opacity-document reads (`query.document.opacity`, `query.document.evaluate`) and the apiVersion 1
`property.*` operations are refused on it. The scenario therefore reads opacity with `query.document.object.property.read` (view
`rawSample`, pinned by the `presentation` view of `query.document.object`), writes it with `command.document.object.property.edit`
(`setValue`) and undoes with `command.document.object.undo`; the declarations are cited in the F03 change note. The application also
refuses to open the project file that is already open, so the prompt reopens the saved file after the keyed project replaced it.
The harness stages the fixtures, opens the static shell itself, runs the client, and after it exits judges only from its own reads:
`saved-60.json` holds 60 and `saved.json` holds 40 (`drawing.propertyHolders`, written by the application), the fixtures are
unchanged, the open project is a different document at revision 0 with no history reading 40, the keyed fixture opened by the harness evaluates
20, 50, 80 at frames 0, 10, 20, reopening `saved.json` reads 40, and the harness itself shows that a write naming the previous revision
is refused (`stale_revision`) and one naming the replaced project is refused (`wrong_document`), then undoes its own write.
Claude Code is given no built-in tools, only the three `nemo_*` tools: the default `nemo_discover` answer is a compact guide (at most 12000
characters) that the client reads inline, and the prompt no longer says where anything is offered. Codex is given `default_tools_approval_mode="approve"` for the `nemo` server,
because under approval policy `never` every MCP call is otherwise refused.
A client that is not installed, not authenticated, over quota or that rejects the command line leaves its half `unrun` with the
reason; the client's last message and tool trace are in the receipt as information and never decide a verdict. The receipt
`f03-clients-receipt.json` (plus `<client>.stream.jsonl`, `<client>.stderr.txt`, `<client>.wire.jsonl`, private) records the candidate
identity, per client name, version, binary SHA-256, exact prompt and command line, every assertion and the limitations. Exit status as
`run.cjs`: 0 both passed, 5 one client was selected and passed, 1 a half failed, 2 refused, 4 a half unrun. Unit tests (`node --test tests/desktop/unit/f03-clients.test.cjs`)
use stubs whose reply shapes are the ones observed from the installed candidate; the first real runs are recorded in the F03 change note.
