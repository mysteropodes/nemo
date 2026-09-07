# Nemo remediation: two human leads, local agents

English edition · 7 September 2026 · v1.0 · [French edition](PLAYBOOK.fr.md)

**Purpose:** let Ilya and Mysteropodes direct their own local agents concurrently, with stable ownership, useful sprint handoffs and visible progress toward the same remediation goals: a modular application, shared application commands and bundled Rust MCP, stronger regression coverage, and reproducible browser/desktop behavior.

**Status: proposed operating instructions for the two leads to adopt.** The review below is factual at its stated snapshot. The task allocations below are recommendations, not claims that agents have been assigned. This preparation did not post messages, edit issues or boards, launch agents, or merge code. Existing assignments continue until their owners explicitly hand them over.

## 1. What the review establishes

The review fetched GitHub `main` at **`66ece0641708122eb8447e85ad8dd7e3402aaf6c`**, inspected the implementation and handbook, all 43 remediation issues, the open PR list, relevant recent issue comments, and both complete Project item lists: 58 origin items and 220 roadmap items. It also inspected PRs #1001 and #1002. This is a source and coordination review; it does not claim a new execution of the product test suites or independent reproduction of the PR authors' runtime results.

| Work | Current evidence | Consequence for assignment |
|---|---|---|
| Baseline and kickoff | F0/#888, R00/#895, BZ0/#896, R01/#897, R02/#898 and R04/#900 are closed. The reproducible baseline landed through #983. | Reuse these foundations. Do not restart baseline tooling or the handbook. A historical R04 closure does not establish every later package's media health. |
| R03 inventory and fixtures | Source includes the inventory, deterministic corpus and browser tests. #899 remains Review / Needs validation on both boards. | Complete missing consumer evidence; do not regenerate all fixtures or repeat accepted work without a relevant source change. |
| R05 boundaries | Discovery, language-neutral text sizing, provenance and no-growth checks landed through #984. The opacity slice has JS and MCP Rust profiles. #901 remains Review. | Complete a bounded part of native Rust classification/enforcement. Whole-source dependency policy is still incomplete; do not treat size coverage as architecture coverage. |
| R06 isolation | #991 integrated isolation; #993 corrected rejected-import side effects; #996 fixed truncated native CLI receipts. #902 remains In progress / Needs validation. | Validate outstanding concurrent desktop/build behavior against current source. Do not reimplement already integrated fixes. |
| R08 animation seam | The production curve extraction and Node runner decision landed through #986. #904 is Validate / Needs validation. | Reuse the extracted kernel; finish its remaining acceptance rather than create another extraction. |
| R09/R11/R12/R13/R14 slice | #992 is merged in the reviewed main. Application/domain/bootstrap/adapters, opacity consumers and compiled Rust MCP are present. #905/#907/#908/#909 remain In progress. | Ilya's team owns subsequent core contract and application changes, subject to explicitly reserved Mysteropodes lanes. Broad remediation remains open. |
| Mysteropodes Lane F | Draft [#1002](https://github.com/mysteropodes/nemo/pull/1002), `8c1dd9bdcc3b82d65fe963c8d7d95d30c48fb8c5`, repairs advertised payload shape and query instance naming. | Preserve the Fizz lane and its exact files. Its PR explicitly still requires validation evidence. |
| Mysteropodes Lane H | [#1001](https://github.com/mysteropodes/nemo/pull/1001), `a99d718685f0515e2e5e949dd6a076eb46feda38`, contains packaging/clean-attach evidence only. | Preserve the Honey lane. Its author reports macOS arm64 attach proof, plus separate FFmpeg and registry-lifecycle findings. It does not establish all-platform packaging or export acceptance. |

**An immediate tracking correction needs a human decision.** [#910](https://github.com/mysteropodes/nemo/issues/910) was closed as completed at 20:22:52 UTC with merge commit `66ece06`. Both remediation boards still say In progress / Needs validation. Later [Claude client findings](https://github.com/mysteropodes/nemo/issues/910#issuecomment-5575427772) and [Codex client findings](https://github.com/mysteropodes/nemo/issues/910#issuecomment-5575438157) report a real advertised-schema defect. Main still declares `payload: Value` while rejecting non-object payloads. This supports keeping client acceptance open, even though transport and application code are merged. The leads should explicitly reopen #910 or link an open acceptance follow-up with equivalent coverage. Do not silently call the gate Done. Saved views filtered with `is:open` can hide this unfinished acceptance.

The older [weekend handoff](../../../40min-checkins/2026-09-07-weekend-final-handoff.md) remains useful evidence, but its statement that #992 is unmerged is superseded. Several issue bodies and handbook descriptions also predate installed test dependencies and current integrations. Use source, recent receipts and current PR state together; neither an old paragraph nor matching board values establishes freshness.

## 2. The operating agreement

### Two leads, one remediation program

1. **Ilya is the remediation and integration lead.** He sequences shared contracts, maintains the global dependency order, decides cross-team scope changes and accepts phase gates.
2. **Mysteropodes directs his own local agents.** Once the leads agree a lane pool and its boundaries, he may assign, review and continue those packets locally without waiting for Ilya at every step.
3. Each task has **one accountable human, one active writer and one named reviewer**. An agent/session identifier belongs in the issue, not in place of the human GitHub assignee.
4. Ilya's agents receive instructions from Ilya; Mysteropodes's agents receive instructions from Mysteropodes. Cross-team requests go into the relevant issue and to the responsible human. No remote agent dispatch is required for ordinary work.
5. Both teams use the same issue/PR/Project workflow. Buzz may carry an optional link or discussion; a Buzz connection, timer, relay receipt or remote-agent repair is not a prerequisite for these local packets.
6. The [existing handbook](../README.md), [task packet](../templates/TASK_PACKET.md), [handoff receipt](../templates/HANDOFF_RECEIPT.md), `AGENTS.md`, `CONTRIBUTING.md` and relevant `CLAUDE.md` sections continue to apply. These instructions adapt coordination to human-directed local execution.

### Autonomy inside an agreed scope

A grant lasts through the agreed packet or milestone, including its sprints. A checkpoint is a reporting and integration opportunity, not an automatic interruption or loss of ownership. Agents may implement, test and correct their own packet continuously. Record new requests in the existing issue queue; do not switch to them unless the responsible human changes priority.

Scope changes are needed for another owner's files, a new schema or state authority, shared build/test wiring, changed acceptance, or an external action outside the packet's authority. Ordinary debugging and fixes inside the accepted scope do not require repeated permission.

An unavailable or silent agent does not make its files free. Its human preserves dirty work, local commits, unpushed branches, runtime state and the next action before acknowledging a handover. Historical indeterminate remote operations are reconciled from their actual effects once; they are not replayed to restart local work.

## 3. First joint setup: one short planning session

1. **Refresh the evidence.** Read current `main`, the latest #910/#1001/#1002 updates and all PRs touching proposed paths. Record the new full base SHA; the snapshot in this document is not a permanent pin.
2. **Record the current owners.** Each human lists active tasks, agent sessions, branches/heads, dirty or unpushed work, whole reserved legacy files, resource reservations and next checkpoints in the corresponding GitHub issues. A local queue can point to them; do not create another shared backlog.
3. **Resolve the R14 mismatch.** Record whether #910 is reopened or which open issue now owns the outstanding acceptance. Preserve the existing F and H work. Review #1001's FFmpeg finding as a separate package defect; do not erase it because #900 is closed.
4. **Agree the initial lane pool.** Retain F and H; select additional packets below to fit actual local capacity. A practical starting limit is two implementation writers per human plus an independent reviewer or acceptance lane when resources allow. Finish review backlog before adding writers.
5. **Split independently deliverable work into existing or new child issues.** Search for an equivalent issue first. Use the existing R-parent; create a child only when the leads authorize it. Record real child URLs in its parent and both Projects. Labels such as M1 below are planning packet IDs, not already-created GitHub issues.
6. **Fill the packet and grant once.** Name the human, writer, reviewer, exact paths, base, dependencies, acceptance, resource slots, next checkpoint and publication authority. Mysteropodes may then issue sub-packets within his granted lane pool; crossing the pool boundary returns to Ilya.
7. **Confirm GitHub access from each machine.** Use the correct authenticated human account. Verify repository and both Project reads separately; an existing repository role alone does not demonstrate Project write access. Use existing approved access. If access is missing, the human handles it while the agent continues independent local work and prepares the precise update.
8. **Choose a board steward and backup.** Normally the task's assigned agent updates its own item and both mirrors; its human is the backup. Ilya owns aggregate/phase updates. One person or agent writes a given item at a time; no second polling bot rewrites it.
9. **Set the cadence.** Suggested: 60–90 minute working sprints, a small update at each sprint end, and a 10–15 minute joint review after two sprints or at a convenient agreed time. These are adjustable starting intervals, not deadlines that cancel work.

## 4. Recommended task pool for Mysteropodes

F and H are retained work. M1–M4 are additional bounded packets to admit after ownership reconciliation. Their listed new paths are proposed destinations and must be checked for collisions before the grant. Evidence is stored under a packet-specific receipt directory; raw data stays in isolated ignored report roots until sanitized.

### F — Finish the MCP client contract correction already in #1002

**Human:** Mysteropodes. **Existing lane:** Fizz / Lane F. **Priority:** P0. **Goal:** reliable client discovery and commands through the shared application API. **Parent:** R14/#910 or its explicitly chosen acceptance follow-up; related R09/#905.

**Reserved files:** `nemo-mcp/src/contract.rs`, `nemo-mcp/src/server.rs`, `nemo-mcp/tests/stdio_contract.rs`, `engineering/application/transport-v1.schema.json`, `engineering/application/OPACITY_SLICE.md`. These are a specific exception to Ilya's shared-contract ownership. Other agents treat them as read-only until release.

1. Continue the current branch; reconcile its latest head rather than create a parallel fix.
2. Verify generated schemas describe the accepted payload keys and types, and malformed requests receive useful errors. Leave semantic range/identity/history authority in the application service.
3. Run the relevant compiled Rust/stdio and schema checks, including old supported instance naming and malformed-input controls.
4. From fresh installed-client sessions, ask each client to discover, select the intended instance and perform the same edit using the advertised contract. Preserve actual tool-call payloads and independent document readback. A manually constructed protocol request does not replace this client test.
5. Coordinate the corrected binary with H; record source and installed bytes. Repeat affected undo/redo, retry, stale-write, reconnect and cancellation cases. Route missing application capabilities to Ilya with a bounded reproducer.
6. Deliver the PR, test evidence and a per-client acceptance matrix. F is Review/Validate until its declared installed-client evidence is accepted; the full R14 gate also requires the other listed workflows.

### H — Finish installed-package evidence already in #1001

**Human:** Mysteropodes. **Existing lane:** Honey / Lane H. **Priority:** P0. **Parent:** R14/#910; packaging residuals relate to R21/#923.

**Reserved paths:** `engineering/remediation/receipts/R14-packaging/**`. Source, build scripts and configuration remain outside this evidence-only packet.

1. Review the current summary and corrected transcript interpretation already in #1001.
2. Identify app, MCP, OS/architecture, source SHA and artifact hashes; distinguish unsigned test packaging from a distributable signed release.
3. Verify the installed binary and intended-instance attachment with a clean resolution environment. Preserve working user installations and projects.
4. After F's correction is available, repeat only affected attachment/client cases against those installed bytes. Coordinate desktop/GPU slots with M1.
5. Keep separate findings for FFmpeg dependency drift, stale registry behavior and build-path-sensitive hashes. Do not silently change the bundler or weaken checks. F already addresses query naming; link that dependency rather than duplicate it.
6. Deliver an accepted evidence summary, exact remaining platform claims and linked residual tasks. A passing MCP attachment does not imply working media export or all-platform distribution.

### M1 — Close a bounded concurrent-desktop isolation acceptance slice

**Human:** Mysteropodes after Ilya grants this R06 sub-scope. **Priority:** P0. **Parent:** R06/#902. **Suggested size:** two sprints, adjusted after the first reproducible result. **Dependency:** identified runnable package and a reserved desktop slot; no need to wait for new MCP payload semantics.

**Proposed writable paths:** `tests/desktop/local-isolation-acceptance.test.cjs` and `engineering/remediation/receipts/M1-isolation/**`. Read the existing harness and `engineering/runtime-isolation.md` first. Production launcher, storage, `project.js`, Rust sources and test registry remain read-only.

1. Map the remaining #902 criteria against current tests and check-90 evidence; select genuinely missing cases.
2. Start two task-owned instances with distinct data roots, origins/ports and artifact locations through the documented launcher. Verify the running source/instance identities.
3. Create different disposable documents, save/reopen and undo separately; check state does not bleed between instances. Exercise the required isolated-build output case using separately reserved build roots.
4. Check foreign-stop refusal, owned stop, retained-data relaunch and launcher-exit behavior without disturbing unrelated processes. Reuse existing regressions where they already cover a case.
5. Retain results and artifact identities. If a production defect appears, send a minimized reproducer to Ilya; do not fix the shared launcher in this packet. Continue other independent acceptance cases.
6. Deliver a permanent missing regression where warranted and an acceptance matrix. Report build/process checks and actual UI save/reopen separately. This packet closes only its declared R06 criteria.

### M2 — Enforce a reviewed native Rust boundary subset

**Human:** Mysteropodes after Ilya grants this R05 sub-scope. **Priority:** P1. **Parent:** R05/#901. **Suggested size:** two sprints. **Dependency:** agreement on the selected Rust roots and the single integration owner for checker wiring.

**Proposed writable paths:** `scripts/nemo/lib/boundaries-rust.cjs`, `tests/nemo-rust-boundaries.test.cjs`, `engineering/boundaries/profiles/native-rust.profile.json`, `engineering/remediation/receipts/M2-rust-boundaries/**`. Confirm these are unused. Existing checker, `ci.cjs`, package/lockfiles and production Rust are read-only. Exclude the active MCP contract files and `src-tauri/src/application_mcp.rs` from this new ownership grant; retain their existing profile treatment.

1. Compare current discovery and adopted profiles with actual tracked Rust files in the granted roots, initially `geometry-wasm/src/**` and the agreed remainder of `src-tauri/src/**`.
2. Reuse language-neutral sizing. Identify precisely which files lack adopted limits or dependency rules; do not call an intentionally excluded root an accidental omission.
3. Propose per-path module ownership, limits, exceptions and dependency direction for this subset. Record unsupported graph analysis explicitly; do not pass Rust through a JS lexer or claim regex matching is a complete Rust dependency graph.
4. Add the smallest missing enforceable check using available Rust/Cargo evidence and the existing checker contract. Have the leads review architectural dispositions; do not raise legacy ceilings to make the result pass.
5. Prove clean controls and applicable deliberate failures: an oversized `.rs` file in scope, an unclassified in-scope file, an expired exception, and a forbidden edge when graph enforcement is part of the packet. Remove temporary violations afterward.
6. Hand the integration owner the exact call-site change and expected results for the existing boundary lane. Final acceptance requires the check to run through the normal local entry point on the combined commit. An unused helper is not delivered enforcement.

### M3 — Add missing browser consumer regressions around the merged opacity slice

**Human:** Mysteropodes. **Priority:** P1. **Parents:** R03/#899 and R13/#909, with one primary child issue. **Suggested size:** one characterization sprint plus one implementation sprint. **Dependency:** merged opacity handlers and a stable fixture/consumer contract from Ilya.

**Proposed writable paths:** `tests/browser/local-opacity-consumers.spec.cjs` and `engineering/remediation/receipts/M3-opacity-consumers/**`. Existing `tests/browser/opacity-consumers.spec.cjs`, fixtures, bootstrap, production code and Playwright configuration remain read-only.

1. Read the existing tests and R03 receipts. Build a small matrix of covered versus missing requirements; do not duplicate previously accepted static/save/SVG tests.
2. Choose one missing family: nested/component context, keyed/frame-change behavior, or selection/history after save/reopen. Confirm the intended behavior with the source and local documentation.
3. Drive real UI controls and File Open/Save in separate browser contexts using the current harness. Assert independent expected values before and after reopening.
4. For affected visual/export consumers, compare actual rendered/exported output with a declared expectation. Preserve the fixture and include one relevant corruption control; never update a golden merely to pass.
5. Keep application fixes with Ilya. Submit failing evidence immediately and continue unrelated cases within the same packet.
6. Deliver discoverable browser tests, source/fixture pins, artifact hashes and limitations. This is browser acceptance only; M1 and H cover different native/install claims.

### M4 — Measure a comparable baseline for the next extraction

**Human:** Mysteropodes. **Priority:** P1. **Parents:** R19/#921, inputs from R03/#899 and the implemented R12 slice. **Suggested size:** one sprint. **Start classification:** bounded measurement preparation, not full R19 acceptance.

**Writable paths:** only `engineering/remediation/receipts/M4-performance/**`; raw output uses an isolated ignored report directory. Production code, fixture corpus and benchmark runner remain read-only.

1. Read `tests/bench/README.md` and existing workload manifests. Select byte-identical evaluation/copy/memory fixtures relevant to the next planned extraction.
2. Record source, fixture hashes, machine class, backend, runtime versions, warm/cold protocol and concurrent load. Reserve the machine for comparable timing; do not compete with H/M1 for GPU measurements.
3. Run the existing benchmark command on the agreed baseline and candidate, retaining raw samples. Change one comparison variable at a time and verify output equivalence.
4. Report only statistics actually supported by the retained samples. The current CPU harness reports median/p90 and related statistics with a small default sample count; that is not an established p95/p99 product budget. If more samples or instrumentation are needed, specify them for the owner.
5. Recommend budgets with observed variance and rationale for human adoption. Unavailable GPU/export measurements remain explicitly not-run or blocked.
6. Deliver a compact comparison and the exact next measurement. Completing this preparation does not close R19's performance/soak program.

### Queue next, after the relevant decisions

| Packet | Why it waits | Useful next work |
|---|---|---|
| R10/#906 native OpenFX feasibility | R09 image/parameter port is not fully adopted; the older Pollen request needs disposition reconciliation. | After the port is agreed, authorize one disposable load/describe/CPU-float-render proof. Retire or integrate it explicitly; do not invent a second product backend. |
| R18.1/#915 document identity/codec extraction | Shared persistence/history boundaries still require an exact contract and whole-file reservation. | Ilya selects one pure leaf and its consumers; Mysteropodes can then own that child end to end. |
| R18.2/#916 next animation extraction | Needs accepted current seam and one agreed API. | Choose one remaining pure responsibility; preserve curve/opacity authority and avoid concurrent `motion.js` rewrites. |
| R20/#922 and full R21/#923 | Broad migration, media and platform prerequisites remain open. | Admit individual fault/platform subcases when their specific inputs exist; do not label the full programs Ready. |

## 5. Ilya's lanes and the collision boundary

Ilya's team concentrates on the combined candidate, R09/R11 state and command contracts, R12 diagnostics/replay, R13 authority/consumer fixes, R07 integration of real local gates, and final cross-surface acceptance. It reviews M1/M3 findings and wires accepted M2 enforcement. It also owns reconciliation of obsolete PRs against already merged content; an open historical PR is not automatically safe to merge or an active grant.

**One writer at a time for shared files:** `package.json`, lockfiles, `src/index.html`, shared test configuration/registry, `scripts/nemo/ci.cjs`, shared schema generators, native bootstrap/build configuration, generated inventory and policy manifests. Reserve whole legacy monoliths such as `app.js`, `motion.js`, `project.js`, `tweens.js` and `engine-bridge.js` when editing them. Two disjoint line ranges do not provide durable isolation.

The F packet is an explicit reserved exception for its five current files. H is evidence-only. M1/M3 use separate test files; M2 owns its new checker/profile files; M4 is measurement-only. The integration owner applies shared wiring changes once. Evidence directories are packet-specific; agents never append to a single shared sprint report file concurrently.

## 6. Every agent's sprint workflow

### Start or resume

1. Read the assigned issue, current packet, latest handoff and linked PRs before exploring. Check the next action and tests already performed.
2. Read repository entry rules, relevant remediation chapter, local module/node documentation and implementation. Do not advise node settings from memory.
3. Verify origin, branch, full HEAD, dirty status and related open branches/PRs. Fetch current main; preserve the existing checkout and index. Start writable work in a dedicated branch/worktree, using `codex/` or the team's established agent prefix.
4. Reconcile overlaps against both teams' current claims. Record the exact whole files and resource slots reserved. A Project status is not a filesystem lock.
5. Acknowledge the packet once, including outcome, scope, reviewer and next checkpoint; set the item In progress using section 8.
6. Configure separate runtime roots, browser profile/origin, ports, app data, caches, build outputs and report paths using the documented isolation tools. Verify the running source identity before UI assertions. Serialize physical desktop input and reference-machine/GPU benchmarks.

### Work without unnecessary interruption

7. Characterize the requested behavior and reproduce a real defect before changing it. Reuse prior accepted evidence if source, fixture and applicable environment remain equivalent, recording the basis.
8. Make one coherent change; keep a single writable application authority and compatibility facade where required. Persistent changes check every applicable save/load, undo/redo, selection, animation, render/export and native consumer.
9. Run focused checks early. Before review, run the relevant normal local verification entry points on the candidate. `npm run verify` defaults to a quick profile; it does not prove every browser/native/MCP job. Check receipts, not just process exit: `doctor` itself always exits zero.
10. Use `pass`, `fail`, `blocked` or `not-run` with a reason. Existing commands include `npm run check`, `npm test`, `npm run test:integration`, `npm run test:browser`, `npm run test:desktop` and `npm run bench`; select according to actual scope. For MCP, include the crate's applicable Cargo tests. Read current command documentation first.
11. Builds and validation run locally. No enabling, dispatching, rerunning or adding automatic GitHub Actions builds without an explicit human request for that specific hosted run. Preserve review protection. A normal task/PR/merge instruction does not supply hosted-run authority.
12. On an out-of-scope defect, put a bounded reproducer and proposed owner in the issue. Continue independent in-scope work. On a genuine blocker, preserve state and move only that packet to Blocked. Start another already-granted Ready packet only with a separate scope/worktree; never silently replace the blocked task.

### End a sprint, pause, or finish

13. Check changed paths and the staged diff; commit coherent work when the grant permits. Do not stage unrelated changes or hand-edit generated outputs. Preserve unfinished work explicitly rather than claiming a clean candidate.
14. Post the small sprint receipt below as one issue comment, with evidence links. Update the linked PR when its reviewable behavior or validation changes. Do not open a new report PR for every checkpoint.
15. Update both Project items and read them back. Record a partial synchronization honestly if only one update succeeded. The current agent/human owns repair of that update.
16. Record the next exact action, remaining tests, reserved paths/resources and whether ownership is retained, offered for handoff or released. Send a human-facing notification only for a decision, blocker, review-ready result or material change.
17. After completion/handoff, stop only owned processes and release owned runtime resources. Preserve published commits, necessary evidence and any dirty/unpushed work. Remove disposable worktrees only after preservation and owner release; record why any retained checkout remains necessary.

## 7. Small, durable sprint and handoff notes

**Canonical location:** the task's GitHub issue comments. The issue body keeps the current packet/acceptance and a link to the latest sprint note; the PR holds the diff and final review evidence. `reports/` is ignored and machine-local, so a link to a local report alone is not a shared handoff. Promote a sanitized summary and necessary evidence to the task's approved receipt directory or an approved shared artifact location. Keep credentials, tokens, user projects and private absolute paths out of shared material.

Use one unique note identifier and one append-only comment per sprint. Correct errors with an explicit correction referencing the original note, rather than rewriting historical results. Longer final receipts use the existing [handoff template](../templates/HANDOFF_RECEIPT.md).

```text
NEMO-SPRINT <issue>-<lane>-<UTC-date-time>-<sequence>
Human / agent session / reviewer:
Sprint objective and result: <one sentence about behavior>
State: In progress | Review | Validate | Blocked
Base / candidate SHA / dirty state:
PR and changed repository-relative paths:
Acceptance: <passed criteria>/<total declared criteria>; remaining criteria named
Checks: command -> pass/fail/blocked/not-run -> evidence URL + limitation
Artifacts: source/build identity, fixture/seed, platform/backend, hashes
Blocker or decision: <what, responsible human, needed action; or none>
Next exact action: <file/test/command and expected result>
Ownership: retained | handoff requested to <name> | released by <human>
Resources: active reservations or confirmed cleanup
Board update: intended Status/Validation; both item readbacks or pending repair
Next checkpoint: <UTC time or agreed milestone>
```

For a replacement agent, the responsible human acknowledges the handoff. The successor verifies the preserved branch/head/dirty state, reads the last receipt and confirms scope before writing. Resume at the stated next action. Repeat an expensive test only when source/environment changed, the receipt cannot be trusted, or a remaining acceptance requirement calls for it. Do not let a timeout automatically transfer ownership.

## 8. Updating the board: mandatory part of the task

### Use the two existing Projects

| Purpose | Project | Status field |
|---|---|---|
| Remediation origin | [Nemo Foundation Remediation — ivg-design/8](https://github.com/users/ivg-design/projects/8) | **Status** |
| Repository-visible mirror | [Nemo Feature Roadmap — mysteropodes/2](https://github.com/users/mysteropodes/projects/2) | **Remediation status** |

Do not write the roadmap's ordinary `Status`, `Board Status` or `Category` when tracking remediation. Match the same issue URL on each board; item IDs and field/option IDs differ between boards. Keep the same semantic values for Priority, Area, Goal, Size, Phase, Program, Kind, Work package, Validation, Surface, References, dates and validation owner when those fields are part of the authorized update. The existing Program values are Core, Editor, Platform and Collaboration; use Assignees to distinguish the two human teams.

### State transitions and responsible updater

| Event | Status | Validation | Who records it |
|---|---|---|---|
| Planned, incomplete scope | Inbox | Planned | Human or assigned steward |
| Complete packet and available prerequisites | Ready | Planned | Assigning human/steward |
| Writer acknowledges and begins | In progress | Needs validation | Assigned agent |
| Reproducer is still required | In progress or Blocked, with reason | Needs reproduction | Assigned agent |
| Concrete candidate and appropriate checks ready | Review | Needs validation | Assigned agent |
| Review/integration complete but runtime acceptance remains | Validate | Needs validation | Reviewer/integration owner |
| Missing dependency/input prevents packet progress | Blocked | Preserve the truthful value | Assigned agent; name blocker owner |
| All declared acceptance passed on integrated bytes | Done | Accepted | Human acceptance owner or explicitly authorized steward |

If a merged change still needs installed-client or desktop validation, use Validate. Do not auto-close the issue with `Fixes`/`Closes` in a partial PR. Keep the parent open until every required child or approved exception is accepted. `Paused` and `handoff` are receipt dispositions, not new board status options. A dependency-ready paused task may return to Ready after its human releases ownership; it must not look claimable while the old writer retains it.

### Safe update sequence, UI or CLI

1. Confirm the packet authorizes issue/PR comments and Project updates. The recommended kickoff grant in section 10 supplies this authority when the human actually issues it. Do not ask again for each routine update within that grant.
2. Read the current issue body, latest comments, both items, field definitions and relevant values immediately before writing. Capture the issue's `updatedAt`, body digest and the old→new values in an ignored local update record.
3. Match by full canonical issue URL, requiring exactly one item on each board. Check pagination/completeness. A missing item is not permission to create a duplicate issue; add the existing issue only within the approved board scope.
4. Ensure no other updater owns this item. Post the uniquely identified sprint receipt once; inspect existing comments first if a previous post timed out. Re-read after the comment so its expected timestamp change is included.
5. Re-read fields immediately before each mutation. If a human or another updater changed the relevant values, reconcile rather than overwrite. Update only the intended fields, preserving unrelated references, dates, scope and acceptance boxes. Avoid full issue-body replacement; if necessary, compare a fresh body digest and update only the owned section.
6. Write the origin values, then the corresponding mirror values. Update `Validation` and references as appropriate; a Status change alone can conceal missing acceptance.
7. Re-read both items and the issue. Verify the intended values, issue identity and preserved fields. Record completion only after this readback.
8. If the second board fails, keep the successful first write. Record `BOARD-SYNC-PENDING` with issue URL, board, exact old/new values, receipt identifier and owner. Before retry, inspect actual effects; apply only still-missing changes. Do not revert newer human edits or retry the whole sequence blindly.

These are cooperative checks, not an atomic transaction or compare-and-swap lock. The one-updater rule is essential. If account/network access fails, save the exact proposed update and give it to the human; continue local work that does not depend on it. Never claim the board was updated when it was not.

### Verified CLI examples

Use the installed `gh` CLI and its local help. The read commands below are safe preflight examples; choose a limit above the current total and verify completeness. At this review the totals were 58 and 220; they will change.

```bash
gh api user --jq .login
gh issue view "$ISSUE" --repo mysteropodes/nemo \
  --json number,url,state,body,updatedAt,assignees,comments
gh project view 8 --owner ivg-design --format json
gh project field-list 8 --owner ivg-design --format json
gh project item-list 8 --owner ivg-design --limit 500 --format json
gh project view 2 --owner mysteropodes --format json
gh project field-list 2 --owner mysteropodes --format json
gh project item-list 2 --owner mysteropodes --limit 500 --format json
```

Set `ISSUE` to the actual assigned issue number. From fresh JSON, resolve the project ID, the item whose `content.url` equals its issue URL, the named field ID and the option ID with the intended value. Require exactly one match; never reuse IDs from another Project or an old receipt. Save a fresh snapshot immediately before applying.

The following are **mutation templates for a granted task**, not commands to run while merely reviewing this playbook. Each variable must be resolved and checked; `gh project item-edit` changes one field per invocation.

```bash
gh issue comment "$ISSUE" --repo mysteropodes/nemo --body-file "$RECEIPT_FILE"

gh project item-edit --project-id "$ORIGIN_PROJECT_ID" \
  --id "$ORIGIN_ITEM_ID" --field-id "$ORIGIN_STATUS_FIELD_ID" \
  --single-select-option-id "$ORIGIN_STATUS_OPTION_ID"

gh project item-edit --project-id "$MIRROR_PROJECT_ID" \
  --id "$MIRROR_ITEM_ID" --field-id "$MIRROR_REMEDIATION_FIELD_ID" \
  --single-select-option-id "$MIRROR_STATUS_OPTION_ID"
```

Repeat the checked one-field operation for `Validation` when required. For text such as References, use `--text` with the fresh preserved content plus the new unique link; for dates use `--date YYYY-MM-DD`. Write multiline comments to a file and use `--body-file`. Then rerun the reads and compare both items. A successful CLI exit alone is insufficient. See the [official item-edit reference](https://cli.github.com/manual/gh_project_item-edit) and [Projects API guide](https://docs.github.com/en/issues/planning-and-tracking-with-projects/automating-your-project/using-the-api-to-manage-projects).

## 9. Human oversight without constant agent interruption

Use the existing Projects, adding saved views only if the leads decide they help. Both humans can inspect both teams through the same columns: issue, Assignees, remediation status, Priority, Program, Work package, Validation, Validation owner, Target date, Updated and References.

- **Active work:** group by human Assignee, then inspect In progress / Review / Validate. Include all issue states while repairing the #910 inconsistency.
- **Review and integration:** Review / Validate, with candidate PR and latest receipt accessible.
- **Blockers:** Blocked, with the blocker owner and next action in the issue.
- **Next work:** Ready, sorted by priority and satisfied concrete dependencies.
- **Acceptance history:** Done / Accepted with integrated source/artifact references; keep closed history accessible.

At each joint checkpoint, Ilya and Mysteropodes inspect changes since the previous checkpoint, not entire conversation logs. Each human contributes a short issue-comment rollup on the relevant phase issue, linking child receipts:

```text
Team / checkpoint:
Accepted since last checkpoint: <issue, behavior, integrated SHA/artifact>
Review-ready: <issue, PR, reviewer, next review>
In progress: <issue, completed acceptance criteria, next criterion>
Blocked: <issue, dependency, responsible human, needed decision>
Shared changes next: <file/contract, current owner, proposed integration order>
Board drift or missing receipt: <exact item and repair owner>
Next granted packets: <IDs and writers, or no expansion until review clears>
```

Measure **accepted criteria and integrated behavior**, review wait, blocked age and missing acceptance. Do not invent percentage-complete figures or rank output by agent messages, token use, commits or lines changed. Forecast dates stay forecasts until a human commits to a revised date; never roll them forward automatically.

Integration is sequential even when implementation is parallel:

1. Reviewer verifies the candidate and evidence independently, recording its exact SHA.
2. Integration owner checks current main and conflicting changes, combines coherent packets and applies shared wiring once.
3. Run applicable checks on the combined candidate. Earlier green branches do not prove their composition. Rebuild/reinstall and rerun affected runtime evidence when relevant bytes change.
4. Publish/merge only under the packet's actual authority and normal PR protection. Hosted builds remain separately restricted.
5. Record integrated SHA, acceptance result and remaining limitations. Move to Validate or Done according to actual criteria; update both boards and parent progress.

## 10. Copyable instruction for either human's local agent

Fill the bracketed values before issuing this instruction. The publication line is an explicit suggested grant for a normal implementation packet; the human may narrow it. A reviewer or evidence-only packet should say so.

```text
You are working on Nemo foundation remediation under [Ilya / Mysteropodes].
Your task is [issue URL + packet ID]. Your human owner is [name]; your reviewer
is [name]. Read the assigned packet, latest sprint receipt, relevant PRs,
AGENTS.md, CONTRIBUTING.md and the applicable remediation/module documentation.

Outcome: [observable result]. Preserve: [invariants].
Writable paths: [exact repository-relative files/directories].
Read-only/shared boundaries: [paths and owners].
Base: [full SHA]; branch/worktree: [identity].
Dependencies already satisfied: [receipts]. Remaining gates: [criteria].
Resources: [isolated data/build/report roots and desktop/GPU reservations].
Checkpoint: [time or milestone].

Authority for this packet: implement and validate locally, create scoped
commits, push this task branch, open/update its PR, post its issue/PR sprint
receipts, and update its existing remediation items on both Projects using
the playbook. This does not authorize merge, release, deployment, issue closure,
new issues, unrelated messages, account changes or any hosted Actions run.
If a different action was already explicitly granted, preserve that authority.

Work continuously inside the scope. Do not dispatch instructions to the other
human's agents or take their files. Queue new requests without abandoning this
task. If a shared contract/path must change, submit the smallest proposal and
reproducer to the responsible human, then continue independent in-scope work.

At each sprint end, before a pause/context reset, and when review-ready:
preserve the branch/head and dirty state; post one NEMO-SPRINT receipt; update
origin Status and mirror Remediation status plus applicable Validation; read
both back; record next exact action, ownership and cleanup. Report failed board
synchronization honestly. Do not repeat work already evidenced on equivalent
bytes or call merged work accepted while required runtime checks remain.

Return: result, candidate/PR, focused checks and evidence, limitations,
remaining acceptance, exact next action and verified board state.
```

## 11. Reference points and maintenance

- [Current/target architecture](../01_CURRENT_AND_TARGET.md), [remediation sequence](../02_REMEDIATION_PLAN.md), [parallel-work contract](../07_GITHUB_PROJECT_AND_PARALLEL_WORK.md).
- [Project mapping and state semantics](../../project-management/README.md), [local CI execution policy](../../ci/README.md), [runtime isolation](../../runtime-isolation.md).
- [Opacity slice and authority](../../application/OPACITY_SLICE.md), [R05 adopted source profile](../../boundaries/profiles/app-surfaces.md), [source at the reviewed main](https://github.com/mysteropodes/nemo/tree/66ece0641708122eb8447e85ad8dd7e3402aaf6c).
- [Merged application/MCP integration #992](https://github.com/mysteropodes/nemo/pull/992), [packaging evidence #1001](https://github.com/mysteropodes/nemo/pull/1001), [payload correction #1002](https://github.com/mysteropodes/nemo/pull/1002).

Maintain both language editions together. Preserve issue numbers, paths, command syntax, schema names and exact GitHub field/option values in translation. Status belongs in live issues and Projects, not in repeated edits to this dated review. Revise the operating protocol only when the two leads change the working agreement.
