# Final weekend handoff — September 4–6, 2026

Prepared for Ilya by Codexitron, UBERBOTS integration lead. This is the final weekend
report requested at **2026-09-07 02:41:56 UTC** (Sunday, September 6, 22:41:56 EDT).
It covers the September 4 kickoff through the final Sunday-night assignment
reconciliation, including work dated September 7 in UTC. It consolidates the
[checkpoint reports](README.md) and identifies subsequent evidence explicitly.
Evidence cutoff: **2026-09-07 02:50 UTC**. Publication follows this cutoff.

## 1. Outcome and handoff boundary

The weekend established a reproducible baseline, landed the first production
animation extraction, substantially improved test discovery and runtime isolation,
and produced a working opacity/application-service/Rust-MCP candidate with direct
packaged-macOS evidence. **The complete foundation refactor and two-client product
integration are still open.** The distinction is consequential: the baseline and
curve extraction are on main; the application/MCP slice remains a separately
reviewed feature branch pending main publication and the remaining acceptance gates.

Six canonical kickoff/foundation issues are closed: **R00/#895, BZ0/#896,
R01/#897, R02/#898, R04/#900 and F0/#888**. R03, R05, R06, R07, R08 and the later
product/architecture lanes remain open at the report readback. No further issue is
closed by this report. The earlier triage of 145 completed historical tickets was
backlog reconciliation, not 145 newly implemented weekend fixes.

The current delegate assignments were allowed to finish. Their results have been
reconciled against their original task threads and retained evidence. Historical
unadmitted or indeterminate operations remain explicitly recorded; they were not
replayed to manufacture a clean lifecycle. The handoff does not transfer another
session's source branch, overwrite its worktree, or convert an agent's successful
terminal result into whole-issue acceptance.

### Revision ledger

| State | Exact identity | Meaning |
| --- | --- | --- |
| Main at report start | `f2888da52eb1a2b0803a0b97d83a69150d815ed1` | Includes the baseline, R05 adoption, R08/R06 integrations, import correction, native receipt fix and check-90 report |
| F0 baseline integration | `cf22365f909069e232a6b6bf992a272e4bd96ff3`, PR #983 | Accepted reproducible baseline; retained honest platform failures and unavailable cases |
| R08 landed tree | `d9d4f7285713de16360ff7ad662afa287a5a3584`, PR #986 | Full landed tree matches reviewed `582f51e2b0290207136629b634dcbf18b07069e2` |
| R06 landed tree | `0b39da9fb8e2965b8cc336e1b1c8e6f57feb6ff9`, PR #991 | Full landed tree matches tested `223f98d92157e162585120c20ee553060007416d` |
| Rejected-import correction | `d5b0232c899bd8d17bd74e2d2b8f66817920d61f`, PR #993 | Full landed tree matches `521fe316ebe7bfcfcf0d64bfc2892b9d5ab71f62` |
| Native receipt correction | `8306fa4d8958fa35283b6e07f981e841d5484a90`, PR #996 | Full landed tree matches reviewed `3a1261c78571a7b35b1ab362d316dbc5e6633a84` |
| Packaged application/MCP source | `6c718b8225dae9563a263ad3360ae43077a78b77` | Identified unsigned macOS package and direct native acceptance belong to this source |
| Reviewed current-main composition | `f612d28acbe70717591fddc9ffd463649c2f0541` | Combines the application/MCP candidate with main; final count-prose follow-up is `e17a8c113c2bdf104cdd2293b2ee47366eadce4b` |
| Final published feature-branch head | `ade71cb688813de789ff74c31058e49928382486`, draft PR #992 | PR #998 merged the composition into the existing feature branch; its tree exactly matches reviewed `e17a8c1`. This is not a main merge |

The report owner independently compared the composed `src`, `src-tauri` and
`nemo-mcp` trees to `6c718b8`: no differences. The current-main native CLI and its
slow-pipe regression are also preserved exactly. This supports carrying forward
the identified application's prior runtime evidence; it does not establish new
platforms, a signed release, or a connected installed-client workflow.

## 2. What was accomplished

### Kickoff, project structure and portable engineering guidance

The team established one canonical issue registry, a dependency-ordered R00–R22
program, phase gates F0–F6, and the Buzz acceptance work BZ0–BZ6. The published
project package records 43 remediation items, 29 parent links, 107 dependency
links, 33 fields and ten source-board views. The roadmap mirror was populated and
969 copied field values were re-read without mismatches during its initial
publication. Forecast dates were recorded as forecasts; they are not accepted
delivery commitments.

The foundation handbook and task/handoff templates landed through
[PR #887](https://github.com/mysteropodes/nemo/pull/887). Portable repository entry
points and adoption guidance followed in
[PR #941](https://github.com/mysteropodes/nemo/pull/941), project/triage publication
in [PR #931](https://github.com/mysteropodes/nemo/pull/931), and collaboration
authority/continuity alignment in
[PR #942](https://github.com/mysteropodes/nemo/pull/942). These changes made source,
ownership, observable acceptance and publication authority explicit. They did not
by themselves prove every live A2A recovery case.

R01 acceptance included actual Codex and Claude clean-clone task/handoff rehearsals
on `6d9fc52d`. R00's kickoff closure waived other collaborators' reports rather than
claiming they had been received; no backup integration owner was appointed at
kickoff. BZ0 closed on September 5 at 15:46:49 UTC for its contract-adoption scope.

The durable entry points are the
[remediation handbook](../engineering/remediation/README.md),
[architecture](../engineering/remediation/reference/01_CURRENT_AND_TARGET.md),
[task registry and publication receipt](../engineering/project-management/PUBLICATION.md),
and [acceptance policy](../engineering/remediation/archive/2026-09-07-handbook/08_ACCEPTANCE_AND_MAINTENANCE.md).
Historical publication prose must be interpreted at its recorded date; current
GitHub issue and PR state takes precedence.

### Reproducible commands and test execution

[PR #932](https://github.com/mysteropodes/nemo/pull/932) added `doctor`, source
checks, named jobs and per-run receipts with source/build identity. Missing tools
and unavailable capabilities receive explicit dispositions. The application
version fallback was synchronized in
[PR #933](https://github.com/mysteropodes/nemo/pull/933).

The weekend also repaired defects in the testing system itself. Spaced Node paths
stopped breaking generated fixture executables
([#945](https://github.com/mysteropodes/nemo/pull/945)); browser fixture work stopped
mutating a shared source checkout
([#951](https://github.com/mysteropodes/nemo/pull/951)); tooling tests entered the
normal runner ([#939](https://github.com/mysteropodes/nemo/pull/939)); and actual
applicability, result receipts and aggregate gating were implemented
([#947](https://github.com/mysteropodes/nemo/pull/947)). Empty or comment-only suites,
missing named jobs, timed-out helpers and incomplete command output were treated
as defects rather than successful validation.

For the pure curve extraction, the team exercised the same 16 cases in Node and
Vitest. Subsequent fresh-process medians were 0.16 seconds and 0.41 seconds in that
bounded trial. Node was retained for this untransformed kernel, with the decision
and limitations preserved in the
[runner ADR](../engineering/animation/ADR-001-curve-runner.md). This was a measured
local decision, not a claim that one runner is universally better.

### R03 inventory, deterministic fixtures and F0 baseline

The R03 work integrated the generated inventory, deterministic fixture corpus,
CPU workloads, browser adapters and source/fixture provenance. The final baseline
landed in [PR #983](https://github.com/mysteropodes/nemo/pull/983) at **September 6,
21:21:40 UTC**. [F0/#888](https://github.com/mysteropodes/nemo/issues/888) closed at
**21:24:11 UTC**. This was the significant baseline closure after the earlier
integration stall.

The identified baseline contains:

- 902 inventory rows: 875 inventoried, 26 explicitly unmapped and one unavailable
  with a recorded reason.
- Twelve deterministic fixtures whose regeneration reproduced all 41 committed
  corpus files.
- Fifteen CPU evaluation/copy/serialization/memory metric families, including
  immutable-input and negative alias/corruption controls.
- Six static checks, 416 passing Node tests with one filesystem skip, 15 passing
  geometry Rust tests and 41/41 tooling boundary coverage.
- Sixteen permanent exported-swatch/record-identity regression controls, including
  the corrected `pm-stroke` wrapper discovery without an unnecessary new listener.
- Browser document expectations before and after reopen, plus actual production
  Rust/vello WASM rendering through SwiftShader with GPU completion awaited.

F0's actual criterion was an honest, reproducible baseline. Its historical native
result was 36 passes and one indexed-seek timing failure; application integration
was not run and packaged desktop was blocked on that R03-only candidate. Those
dispositions remained visible when F0 closed. They are neither a claim of zero
native failures nor the latest combined-candidate native verdict.

Checks 61–65 then advanced real browser Save/Save As downloads, 64 migration/text
expectation executions across fresh reopens, seven mask/alpha/text pixel probes
with independent decoding, and a menu-driven SVG export. The exported SVG was a
997-byte, 320×240 artifact with three exact color probes and a rejected red-to-green
mutation. Document serialization, current frame and source pins stayed unchanged.
These are bounded browser results. Native video, complete consumer equivalence
and full-frame image acceptance were not inferred from them.

Checks 67–69 added further directly observed consumer coverage on the same
`cf22365f` source:

| Consumer | Accepted increment | Limit |
| --- | --- | --- |
| Animated GIF export | Real menu download, 12 decoded 320×240 frames, 60 geometry probes and first-frame color expectations; exported color mutation rejected in all frames | GIF centisecond timing quantized the nominal 500 ms sequence to 480 ms; no native/video or full-frame claim |
| Mesh/media persistence | Real Open → Save download → fresh Open; embedded bytes, mesh topology/offsets and all 24 raster-frame mesh IDs retained; seven document stages and 18 positive pixel executions | Linked media remained an expected blank placeholder; successful relinking untested; software GPU and sparse pixel scope |
| Held frames/components | 542 before/after expectation executions across 103 held-frame and 168 component expectations; downloaded-file corruption rejected on the expected eight/twelve affected frames | Document/evaluator acceptance; one interrupted driver was reconciled and only its missing control repeated |

Canonical follow-ups:
[GIF](https://github.com/mysteropodes/nemo/issues/899#issuecomment-5562440165),
[mesh/media](https://github.com/mysteropodes/nemo/issues/899#issuecomment-5562500927),
[held frames/components](https://github.com/mysteropodes/nemo/issues/899#issuecomment-5562548606).

Sources: [revision-specific baseline](../engineering/inventory/BASELINE.md) and
[check 65](2026-09-06-checkin-65.md).

### R05 modularity enforcement and its remaining boundary

The team landed checker improvements, protected-base size ratchets, tooling
profiles and negative controls through
[#936](https://github.com/mysteropodes/nemo/pull/936),
[#938](https://github.com/mysteropodes/nemo/pull/938) and
[#943](https://github.com/mysteropodes/nemo/pull/943). Application coverage and
provenance, dependency resolution, physical-alias handling and complete piped CLI
output were composed in
[#966](https://github.com/mysteropodes/nemo/pull/966). Repository-wide discovery,
language-neutral size accounting, exclusion provenance and the notice-generator
correction were subsequently adopted in
[#984](https://github.com/mysteropodes/nemo/pull/984), merge `6ca24e4`.

The current opacity/MCP composition evaluates 159 selected application sources:
147 retained and twelve explicit exclusions. All four new opacity/MCP JavaScript
modules remain profiled. Inventory freshness still reports 902 rows. Current
counts were derived from the candidate; historical 140/143-file statements were
not indiscriminately rewritten.

The remaining R05 gap is architectural adoption and enforcement across the whole
application: reviewed ownership, public APIs, target layers and provider/consumer
or global dependency constraints. Coverage, provenance and no-growth size checks
are useful delivered gates, but they do not establish that architectural outcome.
R05 was prematurely auto-closed by #966 and was reopened; the final handoff retains
[#901](https://github.com/mysteropodes/nemo/issues/901) as open.

An earlier Rust-scope accusation was corrected: all sixteen Rust files within the
declared document roots were present, while six other files lay outside those
roots. The real defect was the adopted checker's inability at that time to see
oversized non-JavaScript additions. Later language-neutral enforcement addressed
that mechanism. Old proposed-policy size failures remain dated evidence, not
current measurements or approved exceptions. See
[check 50](2026-09-06-checkin-50.md),
[check 55](2026-09-06-checkin-55.md) and
[the boundary policy](../engineering/boundaries/README.md).

### R04/R06 native tooling, isolation and project correctness

The local FFmpeg sidecar was rebuilt against available libraries and the
unsigned desktop build path was made usable in
[PR #934](https://github.com/mysteropodes/nemo/pull/934). Its recorded clean-fixture
native runs passed 37/37 twice. That resolved the reproduced local packaging
problem; it did not prove hosted macOS compatibility or signed distribution.
R04's accepted evidence also included an actual packaged twelve-frame media export,
decoded output and QuickTime validation. The accepted artifact was unsigned and
unnotarized; release-pipeline verification remains separate.

Resource ownership, isolated browser runtimes and isolated build artifacts landed
through [#935](https://github.com/mysteropodes/nemo/pull/935),
[#937](https://github.com/mysteropodes/nemo/pull/937) and
[#940](https://github.com/mysteropodes/nemo/pull/940). The later R06 composition
added task-specific native application/WebKit state, source handshakes, controlled
process cleanup and reservation retention until application lifetime is proven.
Reviewed source landed via
[#991](https://github.com/mysteropodes/nemo/pull/991); the preserved history also
made [#988](https://github.com/mysteropodes/nemo/pull/988) merged.

Integration preserved the R08 module registrations and regenerated the combined
coverage/inventory artifacts. The reviewed candidate passed 72 focused tests,
six static checks, 902-row inventory freshness, protected-base boundaries and five
browser tests. Paired packaged process/storage isolation evidence was retained
where native source matched exactly.

Review also caught a real application regression: rejected imports could clear
Labs and retained renderer state before input validation refused them. The
source owner's correction was adopted with provenance through
[#993](https://github.com/mysteropodes/nemo/pull/993). Its permanent regression
failed on the landed bad source and passed after correction. Existing entry/tab
state, successful restore snapshots and history replacement behavior were
reconciled as part of this sequence. This is a concrete example of review finding
an observable defect after broader suites were green.

At check 89, native `stop` performed its effects but returned only 512 bytes of
invalid JSON through a pipe. The effects were independently reconciled before
any retry. [#996](https://github.com/mysteropodes/nemo/pull/996) changed the CLI to
set its exit code and allow stdout/stderr to drain. A delayed-reader subprocess
regression with a valid 2 MiB manifest failed before the correction and passed
after it. Seventeen native-runtime tests, 562 Node tests plus the existing skip,
fifteen geometry Rust tests, static checks, inventory and boundaries passed on the
identified correction. No desktop rebuild was needed for this Node CLI-only fix.

Sources: [check 85](2026-09-07-checkin-85.md) and
[check 90](2026-09-07-checkin-90.md).

### R08 production extraction and the application/MCP slice

The pure Motion curve evaluator was extracted behind its existing facade and
the production loader, fixtures, benchmark consumers and tests were updated.
[PR #986](https://github.com/mysteropodes/nemo/pull/986) landed the reviewed
composition. Earlier audit statements that R08 was unmerged are superseded.

R09 preparation also landed: the fourteen-row consumer matrix in
[#950](https://github.com/mysteropodes/nemo/pull/950), source-linked decision gaps
in [#954](https://github.com/mysteropodes/nemo/pull/954), and Rust DTO/schema
feasibility evidence in [#956](https://github.com/mysteropodes/nemo/pull/956).
These are design and compatibility evidence, not adoption of every foundational
contract.

Draft [PR #992](https://github.com/mysteropodes/nemo/pull/992) is the tangible next
product increment. Opacity edits use one application service for validation,
document/revision identity, retries and history. Browser UI bindings and a bundled
Rust MCP executable call that same service. It includes permanent application,
browser and compiled-protocol tests, rejects malformed replay records before
mutation, clears history when a document is replaced and preserves rejected-import
state. It is a first complete-property candidate, not migration of all writable
application state.

On `6c718b8`, the candidate recorded 596 Node passes, the existing filesystem
skip, twelve Rust MCP tests on identical Rust source and three consecutive browser
save/reopen/history/SVG runs. Earlier browser bootstrap timeouts remain an
intermittent observation. Protected-base boundaries, source checks, inventory and
geometry Rust validation passed. A local unsigned macOS package built and its
actual bundled executable initialized and exposed `nemo_discover`, `nemo_query`
and `nemo_command`.

The MCP binary used by subsequent native acceptance has SHA-256
`d7db869d14fe60dc452433d2f98c5efb812c999e0c1a9ee75f62524c4fc25853`.
The PR remains the canonical publication and acceptance location. Its existing
production owner retains the final source adoption and merge; report preparation
does not introduce a second writer.

## 3. Direct product acceptance achieved late Sunday

The following evidence belongs to the identified unsigned macOS package from
`6c718b8`. The tests used isolated task state and serialized native desktop input.

| Workflow | Observed result | Boundary of the evidence |
| --- | --- | --- |
| Discovery and snapshot | Bundled MCP found the actual app/document and returned its identity/state | Direct protocol client; separate installed-client discovery below |
| Scalar opacity | Set 37%; exact retry did not duplicate mutation; stale write refused | First supported property |
| History | Undo restored 100%, redo restored 37%; native inspector/rendering agreed | Does not establish all application history types |
| Native Save/Open | Value persisted; reopen created a new document identity at revision zero; fresh stdio query agreed | Identified fixture and package |
| Animated opacity | Keys 20% at frame 0 and 80% at frame 10; values 20/50/80/80 at frames 0/5/10/119 | Key undo/redo and save/reopen passed |
| PNG sequence | 120 ordered images decoded at 1920×1080; background and key-frame pixels matched; later frames held 80% | PNG sequence, not video/codec or every export format |
| Export state | Query revision, frame and stored values remained unchanged | Tested opacity fixture |
| Preview/export comparison | After tutorial dismissal and Display ICC-to-sRGB conversion, maximum sampled interior RGB deltas were 1/1/0 at frames 0/5/10 | Sparse interior samples, not universal color accuracy |
| Cancellation before dispatch | Pending native IPC cancelled while only the owned app process was suspended; same stdio session remained responsive | No claim of rollback or cancellation during synchronous mutation |
| Retry after cancellation | App resumed unchanged at 37/revision 0; exact retry committed 23/revision 1 once; duplicate unchanged | Trace contained exactly one retained entry for the request |
| Reconnect | Undo/redo and stale refusal passed; a fresh MCP process rediscovered the app at revision 3 with opacity 23 | MCP reconnect, not full app restart or recovery |
| Cleanup | Owned app/launcher/process and isolated state/registry effects were checked in the native receipts | Old malformed CLI receipt was independently reconciled |

The darker-preview observation was reproduced as the first-run Comment & Feedback
tutorial overlay, whose whole-window background is `rgba(6,5,8,.55)`. Dismissing it
removed the dimming. No renderer defect was established from that observation.

The native sequence's interior RGBA samples at frames 0/5/10 were
`[255,204,204,255]`, `[255,128,128,255]` and `[255,51,51,255]`. The preview comparison
accounted for the display profile and JPEG capture; it did not silently compare
unconverted capture pixels with exported sRGB values.

Actual installed **Codex CLI 0.153.0** and **Claude Code 2.1.263** each exposed all
three Nemo tools and made one successful `nemo_discover` call into an isolated
empty registry during check 91. Root-captured client events established
`{"apiVersion":1,"instances":[]}`. Temporary invocation settings were used and
global client settings were preserved. This proves actual-client protocol
discovery. It does **not** prove either client's connected editing, history,
persistence or export against a live Nemo instance.

Canonical receipts:

- [Packaged scalar/history/persistence acceptance](https://github.com/mysteropodes/nemo/pull/992#issuecomment-5563790057).
- [Animated keys and native PNG sequence](https://github.com/mysteropodes/nemo/pull/992#issuecomment-5563880119).
- [Native preview comparison](https://github.com/mysteropodes/nemo/pull/992#issuecomment-5563932524).
- [Cancellation, exact retry and reconnect](https://github.com/mysteropodes/nemo/pull/992#issuecomment-5564220365).
- [Actual installed-client discovery and merge disposition](https://github.com/mysteropodes/nemo/pull/992#issuecomment-5564292626).
- [Final feature-branch composition and corrected client capture](https://github.com/mysteropodes/nemo/pull/992#issuecomment-5564344323).

## 4. Final assignment reconciliation

Check 92 produced a current-main composition rather than leaving the six conflicting
policy/inventory/count records unresolved. Candidate `f612d28` has parents
`6c718b8` and `f2888da`; production application, native application and Rust MCP
source are unchanged from the identified package. The main-side CLI receipt fix
and slow-pipe test are preserved. Follow-up `e17a8c1` corrects the current README
count to 147. [PR #998](https://github.com/mysteropodes/nemo/pull/998) then merged
that exact tree into `codex/application-mcp-integration-20260907`, the existing
PR #992 branch, at `ade71cb`. The feature-branch publication and tree equality
were verified by the owning integrator and re-read for this report. Main still
awaits the product acceptance and review gates.

| Agent | Current bounded assignment and terminal evidence | Accepted output / remaining limit |
| --- | --- | --- |
| Codexalog | Exact-composition review; accepted receipt `e422ce27`; terminal SUCCESS `bd03305e` | APPROVE on `f612d28`; no semantic loss; current provenance, 902-row inventory and receipt-drain preservation checked. Count-prose correction identified; no whole-R14 closure |
| Codeximator | Combined verification; request `372aa25eed728bc194942a21084f45b2fd7fc2aeb0250c437ad0351dde750801`, accepted `cc05566b`, terminal SUCCESS `957addc4` | 598 Node tests: 597 passed, zero failed, one skipped; twelve locked MCP Rust tests passed. Source clean before/after; temporary Cargo target removed |
| Codex-a-bot | Actual-client capture; accepted `f6e90600`, terminal SUCCESS `43f366d3` | Codex raw completed tool event accepted. Initial Claude narrative was insufficient; root's targeted capture supplied actual tool-use/result events and closed that bounded discovery-evidence gap. Full connected workflow remains open |
| Buzzotron | Prior board task delivered externally; no current accepted source assignment | Commands stopped and delivered effects retained; original board terminal receipt still missing. No new board sweep or replay justified |
| Codexitron | Existing production continuation owns PR #992; check-92 continuation integrated the reviewed composition through #998; this task owns only final report/index | Finish applicable product acceptance and main review in the established source lane; publish this handoff independently |
| Claudimator Beta | Offline in the verified roster | Earlier claims preserved; no new assignment inferred |
| Claudirizer | Offline in the verified roster | Earlier claims preserved; no new assignment inferred |
| Clauditron | Offline in the verified roster | Earlier claims preserved; no new assignment inferred |
| Fizz | Offline in the verified roster | No new accepted work verified |
| Honey | Offline in the verified roster | No new accepted work verified |
| Mochi | Offline in the verified roster | No new accepted work verified |
| Pollen | Offline in the verified roster | No new accepted work verified |

The final-report owner read the original task threads and verified the retained
test logs. The Node log SHA-256 is
`712a560a0fd693d3d749e8398f3331851dbddfb1a8cc22788f7b2e8325abbd64`;
the Cargo log is
`0534ef78b3429687a03799dd5f7290a203052770ac6b39e6accb62b1888121ca`.
Cargo's twelve executable tests are six one-test integration targets plus a
six-test target; zero-test unit/doc targets were not added to that count. An
intermediate worker sentence saying eleven tests is superseded by its final
receipt and the actual log.

Final composition validation also passed fifteen geometry Rust tests, six static
checks, five coverage regressions and protected-main boundaries. The three
delegates are released after delivery; unchanged suites do not need another run.
Root's corrected Claude capture, as well as the Codex event, returned an empty
instance array. Both clients' discovery passes remain limited to that registry
fixture.

Fresh questions in this final-report thread initially produced empty
thread/session inbox replies. Those replies could not describe the original
accepted job sessions. They were reconciled against the task threads and exact
lifecycle instead of being used to declare working agents idle. Similarly, old
conversation-memory statements that the orphan-reservation production fix was
still absent do not override the later landed R06 source.

Two native read-only helpers supported this report: `weekend_evidence` reviewed
historical reports/merges; `current_gate_matrix` checked current canonical issues,
PRs and acceptance gaps. They made no source or GitHub changes. Per-session usage
counters are unavailable, so cumulative tokens/costs remain **unknown**, not zero.

### Unsettled historical lifecycle debt

- Original R06 orphan-reservation request
  `af81f46f599e1e11b03daaffb8eaf1ec5289021c1f9936a86c97e4bb8db7bf2f`
  had processed/accepted/progress evidence and preserved test commit `16d756e`;
  cancellation `a702f581` did not establish a terminal disposition. Its historical
  regression is not automatically integration-ready. Preserve its exact owner,
  branch and effects pending runtime reconciliation. Later source corrections
  and newer regression evidence are separate.
- The three check-88 requests `095b79d2`, `8cfef1e7` and `66c8e371` were stored but
  had no recipient admission in the check-90 reconciliation. Recipients reported
  no execution/effects. They were not replayed or counted as completed validation.
- Buzzotron's board operation `d9773df4` had processed/accepted evidence and a
  signed report of completed external effects, but its typed terminal was missing.
  A clean local worktree does not negate those GitHub effects.
- Check-91 client operation `db4063a6` ended indeterminate after an invalid
  terminal envelope. Execution and temporary effects were reconciled and root
  replaced insufficient summaries with captured client evidence. No replay was
  used to conceal that lifecycle failure.

These are coordination/runtime acceptance gaps. They do not erase independently
verified code or product results, and they do not justify handing another writer
an uncertain branch. No supported repair operation was exposed for the missing
terminal publication; no guessed credential, grant, signing or raw-relay workaround
was attempted.

## 5. Remaining blockers and exact next actions

| Priority / gate | What remains | Owner and next observable action |
| --- | --- | --- |
| P0 — application/MCP main publication | Reviewed current-main composition is published at draft #992 head `ade71cb`; main acceptance/review remains | Existing Codexitron production owner completes the remaining acceptance, verifies any later main changes, follows normal review/merge and records the landed SHA |
| P0 — two-client parity, R14/#910 | Installed clients have empty-registry discovery; connected edit/history/persistence/render/export is not accepted | Same production owner runs an isolated live-app fixture from actual Codex and Claude, captures real tool events, compares state/history/persistence/export with the UI/direct-protocol oracle, verifies cleanup |
| P0 — source/lifecycle ownership | Historical accepted/indeterminate jobs and missing terminals cannot be inferred away | Lead and managing runtime reconcile each exact operation and its actual effects; preserve owners until a supported terminal or explicit handoff exists |
| P1 — R03/#899 consumer baseline | F0 is accepted, but all inventory consumers, formats and platform fixtures are not fully accepted | R03 integration owner maps each remaining criterion to an identified integrated source/fixture and closes only demonstrated consumers; retain explicit unavailable results |
| P1 — R05/#901 architecture | Size/coverage/provenance are delivered; full application ownership, public APIs and layer/global dependency adoption remain | Architecture owner reviews/adopts the missing application contracts and adds discriminating boundary evidence on that exact scope |
| P1 — R06/#902 native acceptance | Full concurrent-instance/document workflow and native performance debt remain | Native integration owner verifies the integrated two-instance workflow and investigates the recorded indexed-seek, 1080p sequence, 4K sequence and first-frame failures against fixed workloads |
| P1 — R07/#903 CI acceptance | Real gating implementation exists; full affected-surface coverage, R05/R06 acceptance and required-status-check configuration remain incomplete | CI owner reconciles the exact final source and required gate contract; any hosted run follows a specific human request under the adopted manual-only policy |
| P1 — R08/#904 closure reconciliation | Extraction is merged and applicable native evidence has advanced; canonical issue remains open | Existing acceptance owner maps the native receipts to the exact R08 criteria and records the remaining decision/closure without assuming checked boxes alone are acceptance |
| P1 — R09/R11/R12/R13 | Opacity advances these lanes; broad authority/rollback/schema drift, gesture cancellation and all-writer cutover, diagnostic/replay bounds, and expression/nested/selection/SDK parity remain | Application owner accepts the slice's exact contract and downstream consumers, then identifies the next bounded migration without creating a second writable authority |
| P2 — distribution and resilience | Unsigned local macOS proof is not clean installation, signed/notarized release, Windows/Linux or all-platform acceptance | Platform/release owners execute the stated R21/Buzz gates on identified artifacts when those tasks are ready and authorized |
| P2 — historical report backlog | Checks 60/70/75/80 are in open, conflicting PRs rather than the permanent main folder | Existing report publication owner resolves only the historical document/index conflicts, retains original windows and publishes through the established route |

The four native performance failures are from a source-identified run with 43
passes and four failures. Earlier baseline runs had different candidate identities
and counts; they must not be collapsed into one cumulative pass total. The
intermittent browser bootstrap timeout likewise remains a recorded observation
until its exact cause is established, despite subsequent consecutive passes.

### Gates that remain future or incomplete

The complete canonical mapping is in the
[task registry](../engineering/project-management/PUBLICATION.md). The following
work is not declared delivered by the first opacity slice:

- R10/#906: native OpenFX feasibility.
- R15/#911: gesture or asynchronous-job slice, including its own cancellation and
  history contract.
- R16/#913: one redistributable OpenFX effect.
- R17/#912: color/precision, EXR and OTIO boundaries.
- R18/#914 and children #915–#920: document, animation, renderer, editing/UI,
  media/extensions and preferences/Labs subsystem migrations.
- R19/#921: accepted performance budgets and soak regressions; one measured
  workload or screenshot is insufficient.
- R20/#922: fault, recovery and migration behavior.
- R21/#923: generated/native/browser/packaged distribution acceptance.
- R22/#930 and F1–F6/#889–#894: final cross-inventory and foundation acceptance.
- BZ1–BZ6/#924–#929: the remaining onboarding, Project binding, live lifecycle,
  GitHub-linking, receipt and operational-recovery gates.

## 6. Tracking, publication debt and working-state preservation

The [Foundation Remediation board](https://github.com/users/ivg-design/projects/8)
and [repository roadmap mirror](https://github.com/users/mysteropodes/projects/2)
share the same canonical issues. Several checks found perfect field parity while
the paired values were stale. Check 85 corrected F0 to Done/Accepted and assigned
nine other changed rows for reconciliation. GraphQL then returned a rate-limit
failure while REST remained usable. Later signed board delivery was retained,
but the original typed terminal remains missing. This report does not perform
another board sweep or claim a fresh full-board readback.

The standalone historical report backlog is explicit:

| Check | Existing PR | State at final audit |
| --- | --- | --- |
| 60 | [#982](https://github.com/mysteropodes/nemo/pull/982) | Open, conflicting; file absent from main |
| 70 | [#987](https://github.com/mysteropodes/nemo/pull/987) | Open, conflicting; file absent from main |
| 75 | [#989](https://github.com/mysteropodes/nemo/pull/989) | Open, conflicting; file absent from main |
| 80 | [#990](https://github.com/mysteropodes/nemo/pull/990) | Open, conflicting; file absent from main |

This final report consolidates their outcomes where independently supported. It
does not silently merge those PRs, rewrite their original time windows or claim
their publication backlog is closed. Checkpoint 90 is the latest prior report on
main at this task's start; later evidence is called out above.

Several old implementation PRs also remain open although successors integrated
some or all of their work: R03 #944/#946/#959/#968, R05 #955/#973/#975, R06
#952/#953/#958/#963/#967 and R08 #949. Their existence is not evidence that their
source never landed. The owning integrator should compare ancestry/content and
acceptance before any closure or branch cleanup. The unrelated historical PRs
outside this remediation are preserved.

Two distinctions prevent accidental duplicate integration: #952 was an unselected
native alternative and #955 targets that alternative's launcher layout; neither
should be merged into the selected architecture without a new design decision.
Some other retained predecessors were composed with provenance rather than their
original commits becoming main ancestors. #976 is a separate open worktree-cleanup/
sparse-report documentation proposal, not an integrated product prerequisite.

No active worktree was deleted by this report task. Another developer's changes
in the shared main checkout were left intact. The report used its own branch and
worktree, `codex/weekend-final-handoff-20260907`; writable scope is only this report
and `40min-checkins/README.md`. Private evidence, machine paths, profiles and
credentials are excluded from the publication.

## 7. What to do first after the handoff

1. Start with the existing PR #992 production owner and published `ade71cb`
   (the reviewed `e17a8c1` tree). Confirm its current remote head; do not
   reopen the earlier broad conflict investigation or start a second implementation.
2. Complete the connected installed-client fixture for both clients. Reuse the
   exact package/source evidence where valid; capture the missing live workflow,
   including history, save/reopen, animation, export, reconnect and owned cleanup.
3. Reconcile acceptance independently for R08 and the first property/application
   slice. An integrated PR can advance several lanes without satisfying all of them.
4. Address remaining native concurrency/performance and R03/R05/R07 criteria in
   bounded owner-held lanes. Keep browser, native, hosted and distribution evidence
   separate. Request a hosted run only when an exact remaining gate requires it.
5. Settle historical lifecycle receipts and historical report/PR bookkeeping with
   their owners, without blocking independent product work or replaying uncertain
   effects. Update only changed canonical issue/board facts.
6. Continue later migrations from the adopted dependency order, with one writable
   state authority and all applicable save/load/history/selection/animation/render/
   export/native consumers included in acceptance.

The practical improvement for the next session is to integrate a reviewed bounded
candidate promptly and spend verification on the remaining product behavior.
The weekend repeatedly showed that accumulating clean branches, repeated parity
checks and green summaries can coexist with an unfinished integration gate.

## 8. Report validation and publication contract

This is a documentation-only handoff. Its acceptance is evidence consistency,
working relative links, no symlinks or private-data markers, a scoped staged diff,
and verified publication on remote main. Application tests are not rerun for this
report; all product test claims above name their actual candidate and limits.

The user explicitly requested this final report in the 40-minute report folder and
publication to main. The normal branch/PR route applies; the report does not
authorize unrelated source merges, release, deployment or branch-protection
changes. A local commit, pushed branch or open PR alone is not publication
completion. The final delivery message records the report PR, merge revision and
verified permanent link after remote readback.
