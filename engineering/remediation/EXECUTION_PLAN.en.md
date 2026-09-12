# Nemo — remediation execution checklist

Approved strategy: **7 September 2026**. Humans: **Ilya** (`ivg-design`) and **Cyrill** (`mysteropodes`). [French copy](EXECUTION_PLAN.fr.md).

This is the single operating plan for the current remediation. It supersedes the execution order, forecasts, global blocking gates, remote-agent assumptions and reporting requirements in the older R00–R22 plan and local-agent playbooks. Existing architecture and source documentation remain references; this document governs scope and workflow where they conflict. GitHub issues hold live claims and handoffs; the checklist defines outcomes. The [shared hourly progress log #1062](https://github.com/mysteropodes/nemo/issues/1062) is the one central report destination; do not create competing sprint ledgers or report PRs.

## 1. Outcome, starting point and limits

- [ ] Finish with every agreed handwritten responsibility behind a clear public module boundary, one state owner, appropriate regression tests, enforced size/dependency rules, and a feature capability contract connected to the bundled Rust MCP.
- [ ] Preserve exact observed behavior, including documented existing defects. A baseline identifies source SHA, fixtures/hashes, environment and pass/fail/blocked/not-run observations. It does **not** require repairing all features.
- [ ] Repair a defect within an extraction only if that extraction introduced it, or if it prevents the selected module from being extracted or meaningfully validated. Record unrelated defects as product debt with a bounded reproducer; keep working on remediation.
- [ ] Preserve project compatibility, identity, history and applicable save/load, selection, animation, render/export and native consumers. Known defects need exact signatures; a generic known-failure waiver cannot hide a new regression.
- [ ] Keep new OpenFX effects, a full OCIO/EXR/OTIO implementation, new Buzz transport infrastructure, broad performance improvements and product feature additions outside this program. Define their ports, availability and data contracts where needed; do not implement new product breadth to close remediation. The separately authorized Buzz workspace-instruction editor is a current coordination improvement, not a Nemo structural completion gate.
- [ ] Treat generated, vendor and data catalogs by exact provenance/integrity policy. Handwritten control logic cannot escape modularity checks by being labeled a catalog. Line counts support coherent APIs; arbitrary file splitting is not acceptance.

The source audit used `66ece0641708122eb8447e85ad8dd7e3402aaf6c`, not the older local main checkout. Current `main` at `54e15b6503810607911ae43f7b19ea508790cff0` includes the adopted P02 comparison manifest; completed C01–C08 census work; P04/D02 contracts; P05 deterministic feature registration; completed enforcement/validation leaves; and the completed P20, P22, A01 and P28 extractions. The app-JS profile now has 151 modules, of which 140 remain `app-legacy`. P06 [#1112](https://github.com/mysteropodes/nemo/pull/1112) and P07 [#1113](https://github.com/mysteropodes/nemo/pull/1113) are open candidates, not current-main behavior: both have owner changes requested and remain unmerged until their takeover corrections are accepted. These are source and canonical-record observations, not newly executed test results.

The starting issues below are concrete entry tasks, not a claim that their count exhausts all monoliths. C01–C08 map the fixed source set; P03 admits the remaining one-responsibility leaves. This mapping happens first, and every uncovered responsibility must receive a leaf before a completion forecast. No later family may hide behind a multi-day “migrate everything” issue.

## 2. Two human teams, three slots each

| Slot | Ilya | Cyrill |
|---|---|---|
| **O — orchestrator** | `gpt-6-astra`, **high**. Own shared contracts, source/coverage policy, integration order, combined validation and the canonical board. | `opus`, **high**. Review at milestones; own team claims, technical validation, PR merges, board closure, existing Fizz/Honey handoffs, Claude/native acceptance and quota decisions. |
| **D1 — delegate** | `gpt-5.6-sol`, **medium**. Feature extraction, application services and bounded persistence work. Escalate to high for authority/history ambiguity. | `sonnet`, **medium**. Focused tests, pure helpers, native parsers/adapters and feature registration. |
| **D2 — delegate** | `gpt-5.6-terra`, **medium**. Boundaries, Rust/render adapters and independent implementation. Use high for concurrency/resource ownership. | `sonnet`, **medium**. Coverage/reporting, isolated browser tests, preferences/Labs and diagnostics UI. |

These are recommended session settings, not quota guarantees. Check the actual model and effort at first claim. If a listed model is unavailable, choose an available equivalent and record the substitution; do not silently run a less capable model on an unresolved authority problem. `gpt-5.6-luna` / medium may replace an Ilya delegate for mechanical inventory/link checks; it is not a fourth slot.

Cyrill starts with **one** Sonnet delegate and activates the second only when it has independent work and sufficient allowance. Opus receives a compact issue packet and milestone receipt, not continuous polling or repeated full-repository reads. Avoid automatic maximum effort and extended-context variants. If quotas run low, push the checkpoint and release or retain the claim explicitly; Ilya can take a leaf only after that handoff. Human issue assignees remain the owners even when agents change.

Launch examples from already allocated checkouts; these commands create sessions, not branches:

```sh
codex --model gpt-6-astra -c 'model_reasoning_effort="high"'
codex --model gpt-5.6-sol -c 'model_reasoning_effort="medium"'
codex --model gpt-5.6-terra -c 'model_reasoning_effort="medium"'
claude --model opus --effort high
claude --model sonnet --effort medium
```

Use the app's model/effort controls for app sessions. Claude aliases resolve according to account/provider; record the actual session version. The recommendation follows the available Codex model metadata and current official [Astra](https://developers.openai.com/api/docs/models/gpt-6-astra), [Sol](https://developers.openai.com/api/docs/models/gpt-5.6-sol), [Terra](https://developers.openai.com/api/docs/models/gpt-5.6-terra), [Claude model](https://code.claude.com/docs/en/model-config) and [Claude usage](https://code.claude.com/docs/en/costs) guidance. Effort levels across providers are not equivalent measurements.

### Portable skills: use these workflows directly

No personal skill installation is required. The local `nemo-a2a` skill is not tracked on the audited main, so Cyrill must not depend on it. Optional agent skill wrappers may point to this document; they must not copy and diverge from it. Use local agents for implementation. Buzz supplies the shared conversation, manually authored scheduled prompts and links to the central progress log; it does not imply permission to dispatch remote workers or duplicate local claims.

| Skill named on an issue | Step-by-step method | Checked-in references |
|---|---|---|
| `orchestration` | Verify owner/dependencies → allocate one writer → review evidence → integrate → read back board → release resources. | `AGENTS.md`, `CONTRIBUTING.md`, this plan. |
| `inventory` | Enumerate tracked inputs → identify responsibility/state/consumers → partition exact paths → assign one primary owner → admit small leaves. | Existing inventory/profile files and relevant source. |
| `architecture` | Inspect existing authority → write two concrete contract examples → identify compatibility/failure cases → approve narrow ports. | `CLAUDE.md`, current/target architecture, application source. |
| `extraction` | Characterize → define public API → move one responsibility → route real callers → remove old writer → verify consumers. | Relevant `CLAUDE.md` section, local module/node docs, modularity policy. |
| `capabilities` | Declare typed feature schema/handler → validate registration → expose via shared API/MCP → test real discovery/dispatch and availability. | Application opacity slice, MCP source and stdio tests. |
| `validation` | Use independent expected results → run required surface → report coverage/trace → exercise a negative control → classify limitations. | Testing guide, fixture/desktop READMEs, `scripts/nemo/README.md`. |
| `diagnostics` | Correlate existing commands → bound capture → inspect/export via shared service → replay synthetic fixture in isolation. | Existing opacity trace/replay tests and the diagnostics tasks below. |

Before giving instructions about a node's settings, read that node's local documentation and implementation. Read only the relevant contracts/source for the current leaf; do not repeat a full audit at every restart.

## 3. Task size, claims and daily execution

**One executable issue = one observable outcome = one writer = one coherent PR.** Aim for 30–90 minutes of implementation plus at most 30 minutes of review. This is a scope limit, not a promise that machine builds finish on a timer. If a leaf clearly needs more, split it before starting. A long validation run can continue against a frozen candidate while a different non-overlapping Ready leaf proceeds.

Use native sub-issues under R03/R05/etc to count progress. Those old issues are tracking parents, not worker assignments or global prerequisites. Only named leaf dependencies block work. Unstarted future work is Inbox; Blocked means an actual impediment, not ordinary queue order. Do not mark Done merely because a timebox ended.

### Orchestrator startup and check-ins

- [ ] Read this plan, the team's filtered board view and the latest handoff of each active leaf. Fetch main, inspect PRs/worktrees and preserve active ownership. Silence never releases a claim.
- [ ] Reserve at most two writable task worktrees per machine, plus the primary checkout. Read-only review, board updates and notes create no worktree. The orchestrator serializes integration in a clean, available task checkout; do not add a fourth checkout just for integration.
- [ ] Allocate only Ready leaves whose predecessors are integrated/accepted and whose **whole files** do not overlap. A function-level split inside `motion.js`, `timeline.js`, `tweens.js`, `tools.js` or `engine-bridge.js` is not permission for concurrent writers.
- [ ] Keep shared `package.json`, lockfile, bootstrap/index, profiles, generated manifest and `scripts/nemo/ci.cjs` wiring with Ilya's orchestrator. Delegates supply focused changes in their task branch; the orchestrator integrates these shared hunks serially.
- [ ] At delegate start, blocker, review-ready and sprint end, inspect the changed issue. During active work, an hourly reminder triggers one central report from each orchestrator. Read the latest delta and board state, not the whole history; do not interrupt a delegate's tool call merely to obtain a report. A paused team posts one pause/handoff notice and pauses its timer.
- [ ] Review a blocker within the current check-in when available: resolve the boundary question, supply a dependency, or split a bounded diagnostic leaf. Do not let a delegate spend a second sprint fixing unrelated product defects.
- [ ] Before each integration, verify the exact candidate SHA, scoped diff, acceptance checks and relevant runtime evidence. Run the combined candidate checks after actual integration changes; prior isolated green branches do not prove the combined result.
- [ ] At each hourly checkpoint and at the end of the working session, reconcile accepted/active/blocked leaves and parent sub-issue progress. Add the structured report to the shared progress log and link it from the designated Buzz conversation. Keep detailed evidence in the owning issue/PR. No check-in PR or duplicate report folder.

### Delegate start → work → handoff

- [ ] Read the leaf and its last receipt. Confirm human assignee, model/effort, actual predecessor SHAs and one current writer. Inspect relevant source and existing tests before creating anything.
- [ ] Record base SHA, task branch, proposed paths, expected observable result and named focused validation commands in the issue. New paths in this checklist are proposed targets, not claims that files already exist.
- [ ] Reuse the current branch for that outcome across sprints. Create a branch/worktree only when the task needs a writer slot; use `codex/<task-id>-<topic>` for Codex or the team's existing prefix. Reuse an owned PR instead of creating a report or session PR.
- [ ] Establish the minimal characterization fixture before editing. Keep independent expectations and exact existing failures. Do not regenerate a golden from the new implementation to make a failure disappear.
- [ ] Extract one responsibility behind a public API. Keep one authority for persistent state. Route real consumers and retire the superseded writer/facade within the same outcome. A helper extraction links to its feature's capability; internal helpers do not each require an MCP tool.
- [ ] Run the leaf's tests and applicable coverage, size/dependency and schema/registration gates. Expand testing only for a concrete remaining risk. Distinguish browser, native, packaged installation and actual-client results.
- [ ] Commit a coherent reviewable increment with a task ID and concrete outcome. Commit before a handoff or interruption if the tracked work is meaningful; a WIP checkpoint is allowed but must name failing/unrun checks and must not be presented as merge-ready.
- [ ] Push the **same branch** at review-ready and before ending a sprint or transferring ownership with committed work. Routine local edits do not need repeated pushes. Keep logs/build products out of source; attach sanitized reports or reproducible artifact references to the existing issue/PR.
- [ ] Update the issue and board at start, a material blocker, review-ready and sprint end. Never silently drop ownership, reassign another lane, merge your own unreviewed work or close the integration gate.
- [ ] If interrupted, write the compact handoff below. The successor reads it, checks actual source/remote state and resumes from the next action; it does not rerun the original assignment from scratch.
- [ ] Never write "resuming"/"continuing" without checking actual state (worktree `git status`/`reflog`, `gh pr list`) and citing it in the same message; an unverified intention is not a status report.
- [ ] "Done" or "in progress" requires a pushed commit or an existing PR to cite; without that proof, the real state is "not started," even if announced the day before.
- [ ] Before resuming a task left by another writer: flag it in the shared log/conversation first, to avoid a duplicate restart if the other acts in the meantime.
- [ ] Split then delegate (A2A or a second writer) any blocked or clearly oversized cross-cutting leaf instead of waiting silently or leaving it idle.

Handoff template, used as one issue comment or the issue's current handoff section:

```text
Task / human / lane / actual model-effort:
Base → candidate SHA / branch / PR:
Outcome reached; acceptance checks passed / remaining:
Exact commands + result; coverage/report/trace links:
Known baseline failure / new regression / unavailable environment:
Next concrete action; blocking issue if any:
Whole-file/runtime claim retained or released; pending unpushed work:
```

Do not create a commit merely to update this template. A note without new code belongs in the issue. Publish scheduled coordination reports only to the designated shared log/Buzz conversation; direct messages and new remote-agent work require their own authorization.

## 4. Tests, coverage and built-in debugging

Keep **Node `node:test` + `node:assert`**, **Playwright**, and **Cargo tests**. Add **c8** for JavaScript coverage and **cargo-llvm-cov** for Rust coverage. Nemo already selected Node in [the recorded runner comparison](../animation/ADR-001-curve-runner.md); do not repeat the trial or migrate the suite to Vitest during this remediation. Revisit a runner only when a real TS/ESM/component requirement justifies it.

| Layer | Tool and purpose | Required report / acceptance |
|---|---|---|
| Pure JS and application contracts | `node:test`, importing production modules; narrow VM bootstrap only for legacy script consumers. | Machine-readable test receipt; independent identity, history, codec, clock and failure invariants. |
| Migrated JS coverage | c8 with explicit source inclusion and `--all`. | HTML for humans, LCOV for tooling, JSON for agents; provisional per-module minimum 90% lines / 80% branches for migrated domain/application code. Unimported files remain in the denominator. |
| Real browser workflows | Playwright against the shipped application, real controls and relevant save/render/export consumers. | HTML/JUnit report, failure screenshots and `retain-on-failure` traces; retries must not hide the first failure. |
| Rust units/contracts | Cargo tests; existing MCP stdio tests. Generated/property cases only where they exercise meaningful invariants. | Test receipt; cargo-llvm-cov HTML/LCOV with a measured initial baseline and no-regression ratchet. Adopt per-crate coverage incrementally. |
| Architecture | Existing checker extended for actual imports, cycles, globals, Rust modules, budgets and capability/schema completeness. | Normal local validation must reject deliberate forbidden edges, oversize files, absent registration and stale schemas. |
| Product/native acceptance | Existing desktop harness and actual installed Codex/Claude sessions. | Exact app/MCP bytes, client versions, observed operation results and platform limitations; browser or compile success cannot substitute. |

Coverage measures executed code, not correctness. For history, persistence and request validation, the listed failure scenarios are mandatory even above the percentage threshold. Legacy code receives an observed baseline rather than an immediate global target. Every exception names its path, owner, uncovered scenario, alternative evidence and removal condition. Keep JavaScript and Rust denominators distinct in reports; do not present a misleading combined percentage.

T01–T04 add the report producers. The normal validation receipt should link test reports, per-module coverage, boundary results and the baseline comparison at the same candidate SHA. P32 verifies this from a clean clone. Do not introduce a hosted dashboard/service as a prerequisite. Reports stay local or in sanitized review artifacts under the current local-CI policy.

Current entry commands include `npm run doctor`, `npm run check`, `npm test`, `npm run test:browser`, `npm run test:desktop` and `npm run verify`; read `scripts/nemo/README.md` and the selected job's help before choosing flags. `doctor` can exit zero while reporting blocked capabilities, and `verify` defaults to a quick profile: inspect structured job results. MCP tests require their crate's actual Cargo command, not an assumption that a root script covers every crate. Coverage and diagnostics commands proposed in T01–T08 are not installed merely because this plan is published.

The debugging layer extends the existing opacity trace/replay seed. T05 extracts bounded structured diagnostics; T06 correlates Rust spans; T07 adds one synthetic reproduction bundle; T08 adds a thin panel and MCP inspection. Include build/source, instance, document, revision, command/request and optional job IDs, structured errors, selected state and sampled timing. Keep detailed tracing opt-in, bounded and detached from writable state. Native `tracing` output must never contaminate MCP stdout.

A reproduction bundle carries a versioned synthetic fixture, hashes, clock/seed, command sequence and relevant versions. Replay into an **isolated document**; never silently replay writes into the user's active project. Exclude secrets, private paths and full user assets by default. No arbitrary eval/shell tool is part of the runtime debugger. Playwright Trace Viewer supplies browser evidence; the application diagnostics service supplies product state and command evidence.

Primary tool references: [Node test runner](https://nodejs.org/api/test.html), [c8](https://github.com/bcoe/c8), [Playwright Trace Viewer](https://playwright.dev/docs/trace-viewer), [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov), [Rust tracing](https://docs.rs/tracing/latest/tracing/). Pin compatible versions when the corresponding leaf is implemented; do not upgrade the whole toolchain speculatively.

## 5. Capability and growth contract

- [ ] UI, SDK/scripts, tests and MCP call the same versioned application services. Rust MCP is an adapter; it does not become a second writable document or a repository of feature logic.
- [ ] Each feature owns a machine-readable declaration: stable ID/version, input/output schema, units/bounds, effects, availability, public handler, examples and fixture links. Commands specify revision/retry/undo; jobs specify progress/cancel/terminal states; resources specify identity/lifetime/disposal.
- [ ] Build/launch discovers trusted declarations deterministically, validates them and generates the catalog/schema/docs. Adding a feature does not require hand editing a central MCP operation switch. Missing/duplicate/stale declarations fail the normal gate.
- [ ] Discover compact summaries first; load detailed contracts on demand. Large media/geometry travels by handles/artifact references. Agents can inspect targeted frames/state; do not expose thousands of widget-level tools or serialize entire buffers into prompts.
- [ ] Enforce mutation/access policy in application handlers. Tool descriptions are explanatory, not enforcement. Use explicit instances/documents/revisions, reject stale writes and reconcile interrupted requests before retrying.
- [ ] Keep protocol and feature-schema versions separate, with explicit compatibility and deprecation. Add replaceable adapters for future protocols/backends; no wholesale Rust rewrite, universal plugin framework or distributed scheduler is required now.
- [ ] A broken existing feature remains connected to its real implementation and advertises its precise availability/failure. A descriptor alone does not establish working behavior. Internal kernels associate with a useful feature capability instead of becoming meaningless public tools.

## 6. Board rules and human oversight

The authoritative board is **Cyrill's [Nemo Feature Roadmap — Remediation execution](https://github.com/users/mysteropodes/projects/2/views/1)**. Use [Ilya's assigned view](https://github.com/users/mysteropodes/projects/2/views/7), [Cyrill's assigned view](https://github.com/users/mysteropodes/projects/2/views/8) and [parent progress](https://github.com/users/mysteropodes/projects/2/views/9) on that same project. [Ilya's former board](https://github.com/users/ivg-design/projects/8) is a legacy reference snapshot; it was created when access to Cyrill's board was unavailable. Agents do not maintain a second board or require parity with that snapshot.

Every leaf has exactly one GitHub assignee: Ilya=`ivg-design`, Cyrill=`mysteropodes`. `Work package` displays task ID, human/lane and skill. Native Parent issue / Sub-issues progress show rollups; native blocked-by links identify actual predecessors. Labels separate `remediation:leaf`, `remediation:parent` and `remediation:deferred`; `stream:O/D1/D2` identifies the lane. Person views filter by assignee. The execution board includes Done leaves so completed work remains visible.

| Transition | Who writes it | Required evidence |
|---|---|---|
| Inbox → Ready | Human's orchestrator | Exact bounded scope, predecessor disposition, independent acceptance, owner and available file/runtime slot. |
| Ready → In progress | Claimed delegate or orchestrator | Actual branch/base/model and focused commands recorded; no competing writer. |
| In progress → Blocked | Current worker | Named missing decision/environment/dependency, last candidate and smallest next action. |
| In progress → Review | Current worker | Pushed candidate, coherent diff, checks/report links and complete handoff. |
| Review → Validate | Orchestrator | Code review passed; specifically named integrated/runtime evidence still outstanding. |
| Review/Validate → Done | Orchestrator | Exact accepted integrated candidate, all applicable checks, known-defect disposition, closed issue and released/preserved branch/runtime ownership. |

- [ ] Immediately before a write, fetch the issue body/current claim and its live Project #2 item ID/fields. Preserve changes another owner made; update only the intended fields.
- [ ] Update **Project #2 `Remediation status`** and `Validation`. For these new remediation leaves, also map roadmap `Status` / `Board Status`: Inbox→Planned later; Ready→To do; In progress/Review/Validate→In progress; Blocked→Needs work; Done→Already there. Leave unrelated roadmap items and the legacy Project #8 snapshot alone.
- [ ] Keep every applicable classification filled: Priority, Area, Goal, Size, Phase, Program, Kind, Work package, Planning window, Risk, Validation, Surface, Estimate days, Validation owner and References; set Triage date and Schedule basis. One estimate day means eight active work hours; 0.125–0.25 days is a bounded one-to-two-hour packet, not elapsed delivery time. Unclaimed dependency-ordered work is Forecast / Unscheduled. Set Start date on actual claim and Target date only when the human commits a schedule; Exception expiry applies only to a real exception. Blank non-applicable dates must not become fictional commitments.
- [ ] Read the primary item and issue back. If a field update or issue closure only partly succeeds, record the partial state and reconcile it before the next transition. Do not assume that a successful API call updated every intended field.
- [ ] Set `Validation owner` to the assigned human’s own orchestrator: `Ilya/O (ivg-design)` or `Cyrill/O (mysteropodes)`. The owning orchestrator reviews, validates, merges and closes its team’s leaves and updates parent/dependency progress; Ilya coordinates shared integration order. A cross-team validator is an explicit task-specific agreement, not a default. Preserve historical acceptance ownership on completed tasks.
- [ ] Delegates update only their own leaf. Orchestrators handle their team’s integration, reassignment, parent summaries, dependency release and final closure. A human ownership transfer changes issue assignee, validation owner, plan allocation and current handoff together.
- [ ] Keep live progress in issues. Checklist ticks/allocations can be reconciled in the next ordinary plan/code change; do not create a PR per status update. The issue's current state takes precedence over a static checkbox snapshot.

Agents may use authenticated GitHub tools or `gh`; inspect current fields rather than pasting stale item IDs. Discovery commands:

```sh
gh issue view <issue-number> --repo mysteropodes/nemo --json body,assignees,state,url
gh project field-list 2 --owner mysteropodes --limit 100 --format json
gh project item-list 2 --owner mysteropodes --limit 500 --format json
```

Use the returned project/item/field/option IDs with `gh project item-edit --id <item-id> --project-id <project-id> --field-id <field-id> --single-select-option-id <option-id>`, then repeat the read. These placeholders must be resolved live. Use `--body-file` for multiline issue/PR text; never interpolate untrusted text into shell commands.

## 7. Integration and branch cleanup

**Merge policy agreed by Ilya and Cyrill on 9 September 2026:** each collaborator team owns its technical review, local validation and ordinary PR merges. No routine approval from the other human team is required. This replaces the earlier cross-account approval instruction.

- [ ] Keep one task branch/PR across sprints. An orchestrator may integrate several reviewed leaves sequentially, but each leaf keeps its own acceptance and ownership. Do not bundle unrelated changes just to reduce PR count.
- [ ] Publish the candidate in the existing task PR. The team's orchestrator reviews the exact SHA and scoped diff, checks the leaf's local validation/runtime receipts, and records the reviewer, candidate, commands/results, known baseline failures and acceptance decision in that PR. Agents sharing the author's GitHub account can perform this technical review; do not attempt GitHub self-approval or invent another review identity.
- [ ] For a PR authored by a **current repository collaborator with write, maintain or admin access**, the owning team may merge after that review and validation. The metadata-only `Collaborator PR policy` workflow supplies GitHub's required approval as an explicitly labeled policy acknowledgement at the current SHA. This acknowledgement is not a code review, test pass or instruction to merge; the team's evidence must be complete first. Read/triage access, board membership and Buzz enrollment do not qualify.
- [ ] For an **external-author PR**, obtain an approving GitHub review from a repository collaborator after reviewing and locally validating the current candidate. The policy workflow does not approve these PRs, even if a collaborator pushes commits to them or presses Merge. Keep the one-review and latest-push requirements, stale-review dismissal and conversation resolution enabled.
- [ ] Resolve outstanding change requests and conversations. Coordinate with the other team for overlapping ownership, shared contracts, conflicts or an actual product decision; these are specific coordination needs, not a routine merge gate. Named predecessor acceptance still applies.
- [ ] Immediately before merging, re-read the remote head and review state; if the SHA changed, repeat affected review/validation. Use the normal GitHub merge route, then record the integrated SHA, perform applicable integration checks and update the owning leaf. An open PR or policy acknowledgement alone is not Done.
- [ ] If the policy workflow failed or an existing PR lacks its acknowledgement, inspect the run and retry only the metadata policy from trusted main: `gh workflow run collaborator-pr-policy.yml --repo mysteropodes/nemo --ref main -f pull_request=<number>`. Do not request approval from the other team solely to work around this policy. Resolve an access/API failure with a repository admin; never manufacture a technical approval.
- [ ] No direct main push or routine protection bypass. Builds, tests, releases and deployments remain local unless a human explicitly requests the specific hosted run. The agreed metadata-only policy workflow is the sole automatic Actions exception; it executes no PR code and does not merge anything. Keep the four product workflows disabled. Timed coordination reminders remain paused outside active work.
- [ ] After merge/acceptance, verify remote containment, clean tracked/untracked state and absence of owned processes before deleting the task branch/worktree. Do not remove another lane's checkout. An incomplete useful branch stays owned or is archived with a tested restoration path.
- [ ] Preserve the protected remote `archive` branch. Archived bundles have original-name→SHA manifests, checksums and empty-repository restore verification. Never delete a branch solely because of age or absence of a PR. Open PR heads/bases, releases, worktrees and unresolved ownership remain protected from cleanup.
- [ ] New branches should not accumulate after closure. The orchestrator performs cleanup as part of the handoff/Done workflow; the human should not inherit a worktree-cleaning chore after every session.

## 8. Ordered execution checklist

Dependencies determine readiness, not the numeric order of IDs. Start independent Ready tasks only within the three-slot/two-writer limits. A suggested first allocation is Ilya O=P04/P02 and integration, D1=C02, D2=C03; Cyrill O=F01 and existing lane reconciliation, D1=F02 if that owner is active (otherwise P15), D2=T01 only when quota and the shared package-file slot permit. P14 reserves a separate native runtime slot; it must not interrupt a parity session.

After C01–C08, P03 must create the remaining small extraction leaves and attach them to the same family parents. Reuse the leaf protocol below: exact symbols/owner/dependencies, three observable checks, known-defect exclusions, same assignee/lane labels and native parent/blocking links. Extend this checklist in the same ordinary change that adopts the census. The fixed source-set digest and zero-unmapped checks prevent silently declaring remediation complete after only these starting tasks.

<!-- generated-task-index -->

The initial board contains **59 leaves: 33 owned by Ilya and 26 by Cyrill**. These counts describe the starting queue, not a fixed total for all future extractions.

### A — establish the facts and retain current work

| Ilya — owned steps | Cyrill — owned steps |
|---|---|
| [P01 / #1003](https://github.com/mysteropodes/nemo/issues/1003) · **O** · Install the approved execution checklist, owned leaf issues and board views | [C04 / #1039](https://github.com/mysteropodes/nemo/issues/1039) · **D1** · Map editor tools and presentation into bounded extraction packets |
| [P02 / #1004](https://github.com/mysteropodes/nemo/issues/1004) · **O** · Adopt one current-state comparison manifest | [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040) · **D1** · Map media, import, export and native services into bounded extraction packets |
| [C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036) · **O** · Map document and history into bounded extraction packets | [C06 / #1041](https://github.com/mysteropodes/nemo/issues/1041) · **D2** · Map preferences, labs and bootstrap into bounded extraction packets |
| [C02 / #1037](https://github.com/mysteropodes/nemo/issues/1037) · **D1** · Map animation and time into bounded extraction packets | [C08 / #1043](https://github.com/mysteropodes/nemo/issues/1043) · **D2** · Map completeness and remaining tracked roots into bounded extraction packets |
| [C03 / #1038](https://github.com/mysteropodes/nemo/issues/1038) · **D2** · Map rendering and resource ownership into bounded extraction packets | [F01 / #1047](https://github.com/mysteropodes/nemo/issues/1047) · **O** · Review the existing Honey installed-package evidence |
| [C07 / #1042](https://github.com/mysteropodes/nemo/issues/1042) · **O** · Map application, mcp and diagnostics into bounded extraction packets | [F02 / #1048](https://github.com/mysteropodes/nemo/issues/1048) · **D1** · Finish the existing Fizz payload-schema acceptance |
| [P03 / #1005](https://github.com/mysteropodes/nemo/issues/1005) · **O** · Validate the combined source census and admit the complete extraction queue | [P14 / #1016](https://github.com/mysteropodes/nemo/issues/1016) · **D2** · Complete two-instance native isolation acceptance |
| — | [P15 / #1017](https://github.com/mysteropodes/nemo/issues/1017) · **D1** · Add one real opacity slider gesture regression |

- [x] **[P01 / #1003](https://github.com/mysteropodes/nemo/issues/1003) — Install the approved execution checklist, owned leaf issues and board views**

  Owner **Ilya/O** · skill `orchestration` · `gpt-6-astra` / **high**. Predecessors: none; claim the file/runtime slot.

  Scope: `engineering/remediation/EXECUTION_PLAN.en.md`; `engineering/remediation/EXECUTION_PLAN.fr.md`; `AGENTS.md`; `handbook entry links`; `GitHub remediation issues, primary Project 2 views and legacy Project 8 cutover`.

  1. English/French checklist contains matching IDs, human assignees, models, lane skills, scope and completion tests.
  2. Every executable issue has one human assignee, a lane, checked dependencies and a parent; live board views separate leaves from parents.
  3. Existing active PR ownership is preserved; stale R03/R05/R06/R14 summaries are reconciled; published plan and board are read back.

  Limit: No issue is marked Done because a PR merged. No roadmap Status overwrite before a board decision.

- [x] **[P02 / #1004](https://github.com/mysteropodes/nemo/issues/1004) — Adopt one current-state comparison manifest**

  Owner **Ilya/O** · skill `orchestration` · `gpt-6-astra` / **high**. Predecessors: none; claim the file/runtime slot.

  Scope: `engineering/inventory/BASELINE.md`; `existing ignored receipt output`.

  1. Identify selected source, dirty state, fixture hashes and applicable environment. Reuse equivalent prior receipts rather than re-running all suites.
  2. Known fail / blocked / not-run entries stay explicit with exact case and evidence; comparison distinguishes new regression, unchanged known failure and environment mismatch.
  3. Publish a machine-readable comparison manifest with immutable fixture/source references; a missing or mismatched reference is reported, not silently reused.

  Limit: No feature-repair prerequisite or silently changed golden. Historical F0 remains valid at its own revision.

- [x] **[C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036) — Map document and history into bounded extraction packets**

  Owner **Ilya/O** · skill `inventory` · `gpt-6-astra` / **high**. Predecessors: none; claim the file/runtime slot.

  Scope: `src/js/project.js`; `src/js/project-document.js`; `src/js/project-entry.js`; `src/js/idb-store.js`; `src/js/asset-tree.js`; `src/js/history-panel.js`; `document/history responsibilities in timeline.js and tweens.js`; `engineering/inventory/remediation-scope.json (partition submitted to orchestrator)`.

  1. Read-only map for Document and history: name actual responsibilities, current writers, public entry points and applicable consumers at the chosen source SHA.
  2. Partition into one-responsibility packets: exact symbols/paths, independent fixture oracle, expected module boundary and one whole-file writer. No packet requires days of implementation.
  3. Return JSON partition with linked proposed IDs, known-defect exclusions and all remaining responsibilities accounted for; orchestrator serializes the shared index and creates the leaves. Shared files have one primary file owner despite multiple responsibility families.

  Limit: No source migration in this census task. For C08, verify set difference mechanically and map only uncovered paths; do not repeat C01-C07 research.

- [x] **[C02 / #1037](https://github.com/mysteropodes/nemo/issues/1037) — Map animation and time into bounded extraction packets**

  Owner **Ilya/D1** · skill `inventory` · `gpt-5.6-sol` / **medium**. Predecessors: none; claim the file/runtime slot.

  Scope: `src/js/motion.js`; `src/js/tweens.js`; `src/js/timeline.js`; `src/js/animation/**`; `src/js/domain/animation/**`; `src/js/expr-*.js`; `src/js/layer-inout.js`; `src/js/camera.js`; `src/js/text-animator*.js`; `src/js/markers.js`; `src/js/bpm-grid.js`; `engineering/inventory/remediation-scope.json (partition submitted to orchestrator)`.

  1. Read-only map for Animation and time: name actual responsibilities, current writers, public entry points and applicable consumers at the chosen source SHA.
  2. Partition into one-responsibility packets: exact symbols/paths, independent fixture oracle, expected module boundary and one whole-file writer. No packet requires days of implementation.
  3. Return JSON partition with linked proposed IDs, known-defect exclusions and all remaining responsibilities accounted for; orchestrator serializes the shared index and creates the leaves. Shared files have one primary file owner despite multiple responsibility families.

  Limit: No source migration in this census task. For C08, verify set difference mechanically and map only uncovered paths; do not repeat C01-C07 research.

- [x] **[C03 / #1038](https://github.com/mysteropodes/nemo/issues/1038) — Map rendering and resource ownership into bounded extraction packets**

  Owner **Ilya/D2** · skill `inventory` · `gpt-5.6-terra` / **medium**. Predecessors: none; claim the file/runtime slot.

  Scope: `src/js/engine-bridge.js`; `src/js/render-manager.js`; `src/js/playback-cache.js`; `src/js/color-manager.js`; `src/js/path-fx.js`; `src/js/custom-effects.js`; `src/js/shader-effects-library.js`; `geometry-wasm/src/**`; `engineering/inventory/remediation-scope.json (partition submitted to orchestrator)`.

  1. Read-only map for Rendering and resource ownership: name actual responsibilities, current writers, public entry points and applicable consumers at the chosen source SHA.
  2. Partition into one-responsibility packets: exact symbols/paths, independent fixture oracle, expected module boundary and one whole-file writer. No packet requires days of implementation.
  3. Return JSON partition with linked proposed IDs, known-defect exclusions and all remaining responsibilities accounted for; orchestrator serializes the shared index and creates the leaves. Shared files have one primary file owner despite multiple responsibility families.

  Limit: No source migration in this census task. For C08, verify set difference mechanically and map only uncovered paths; do not repeat C01-C07 research.

- [x] **[C04 / #1039](https://github.com/mysteropodes/nemo/issues/1039) — Map editor tools and presentation into bounded extraction packets**

  Owner **Cyrill/D1** · skill `inventory` · `sonnet` / **medium**. Predecessors: none; claim the file/runtime slot.

  Scope: `src/js/tools.js`; `src/js/select-bridge.js`; `src/js/*-bridge.js (editor bindings only)`; `src/js/ui.js`; `src/js/*-panel.js (editor panels)`; `drawing, text, rig, mesh and brush modules from app-js.profile.json`; `engineering/inventory/remediation-scope.json (partition submitted to orchestrator)`.

  1. Read-only map for Editor tools and presentation: name actual responsibilities, current writers, public entry points and applicable consumers at the chosen source SHA.
  2. Partition into one-responsibility packets: exact symbols/paths, independent fixture oracle, expected module boundary and one whole-file writer. No packet requires days of implementation.
  3. Return JSON partition with linked proposed IDs, known-defect exclusions and all remaining responsibilities accounted for; orchestrator serializes the shared index and creates the leaves. Shared files have one primary file owner despite multiple responsibility families.

  Limit: No source migration in this census task. For C08, verify set difference mechanically and map only uncovered paths; do not repeat C01-C07 research.

- [x] **[C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040) — Map media, import, export and native services into bounded extraction packets**

  Owner **Cyrill/D1** · skill `inventory` · `sonnet` / **medium**. Predecessors: none; claim the file/runtime slot.

  Scope: `src/js/export.js`; `src/js/*import*.js`; `src/js/*export*.js`; `src/js/native-video-bridge.js`; `src/js/linked-media.js`; `src/js/media-library.js`; `src-tauri/src/** (excluding application_mcp.rs)`; `engineering/inventory/remediation-scope.json (partition submitted to orchestrator)`.

  1. Read-only map for Media, import, export and native services: name actual responsibilities, current writers, public entry points and applicable consumers at the chosen source SHA.
  2. Partition into one-responsibility packets: exact symbols/paths, independent fixture oracle, expected module boundary and one whole-file writer. No packet requires days of implementation.
  3. Return JSON partition with linked proposed IDs, known-defect exclusions and all remaining responsibilities accounted for; orchestrator serializes the shared index and creates the leaves. Shared files have one primary file owner despite multiple responsibility families.

  Limit: No source migration in this census task. For C08, verify set difference mechanically and map only uncovered paths; do not repeat C01-C07 research.

- [x] **[C06 / #1041](https://github.com/mysteropodes/nemo/issues/1041) — Map preferences, labs and bootstrap into bounded extraction packets**

  Owner **Cyrill/D2** · skill `inventory` · `sonnet` / **medium**. Predecessors: none; claim the file/runtime slot.

  Scope: `src/js/labs/**`; `src/js/app.js`; `src/js/i18n.js`; `src/index.html`; `src/css/**`; `preferences/shortcut/loading responsibilities in timeline.js`; `engineering/inventory/remediation-scope.json (partition submitted to orchestrator)`.

  1. Read-only map for Preferences, Labs and bootstrap: name actual responsibilities, current writers, public entry points and applicable consumers at the chosen source SHA.
  2. Partition into one-responsibility packets: exact symbols/paths, independent fixture oracle, expected module boundary and one whole-file writer. No packet requires days of implementation.
  3. Return JSON partition with linked proposed IDs, known-defect exclusions and all remaining responsibilities accounted for; orchestrator serializes the shared index and creates the leaves. Shared files have one primary file owner despite multiple responsibility families.

  Limit: No source migration in this census task. For C08, verify set difference mechanically and map only uncovered paths; do not repeat C01-C07 research.

- [x] **[C07 / #1042](https://github.com/mysteropodes/nemo/issues/1042) — Map application, mcp and diagnostics into bounded extraction packets**

  Owner **Ilya/O** · skill `inventory` · `gpt-6-astra` / **high**. Predecessors: none; claim the file/runtime slot.

  Scope: `src/js/application/**`; `src/js/adapters/application-mcp.js`; `src/js/bootstrap/**`; `nemo-mcp/**`; `src-tauri/src/application_mcp.rs`; `engineering/inventory/remediation-scope.json (partition submitted to orchestrator)`.

  1. Read-only map for Application, MCP and diagnostics: name actual responsibilities, current writers, public entry points and applicable consumers at the chosen source SHA.
  2. Partition into one-responsibility packets: exact symbols/paths, independent fixture oracle, expected module boundary and one whole-file writer. No packet requires days of implementation.
  3. Return JSON partition with linked proposed IDs, known-defect exclusions and all remaining responsibilities accounted for; orchestrator serializes the shared index and creates the leaves. Shared files have one primary file owner despite multiple responsibility families.

  Limit: No source migration in this census task. For C08, verify set difference mechanically and map only uncovered paths; do not repeat C01-C07 research.

- [x] **[C08 / #1043](https://github.com/mysteropodes/nemo/issues/1043) — Map completeness and remaining tracked roots into bounded extraction packets**

  Owner **Cyrill/D2** · skill `inventory` · `sonnet` / **medium**. Predecessors: [C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036), [C02 / #1037](https://github.com/mysteropodes/nemo/issues/1037), [C03 / #1038](https://github.com/mysteropodes/nemo/issues/1038), [C04 / #1039](https://github.com/mysteropodes/nemo/issues/1039), [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040), [C06 / #1041](https://github.com/mysteropodes/nemo/issues/1041), [C07 / #1042](https://github.com/mysteropodes/nemo/issues/1042).

  Scope: `all git-tracked paths not allocated by C01-C07`; `scripts/**`; `tests/**`; `remaining runtime/service/worker roots`; `vendor/generated/data provenance`; `engineering/inventory/remediation-scope.json (partition submitted to orchestrator)`.

  1. Read-only map for Completeness and remaining tracked roots: name actual responsibilities, current writers, public entry points and applicable consumers at the chosen source SHA.
  2. Partition into one-responsibility packets: exact symbols/paths, independent fixture oracle, expected module boundary and one whole-file writer. No packet requires days of implementation.
  3. Return JSON partition with linked proposed IDs, known-defect exclusions and all remaining responsibilities accounted for; orchestrator serializes the shared index and creates the leaves. Shared files have one primary file owner despite multiple responsibility families.

  Limit: No source migration in this census task. For C08, verify set difference mechanically and map only uncovered paths; do not repeat C01-C07 research.

- [ ] **[P03 / #1005](https://github.com/mysteropodes/nemo/issues/1005) — Validate the combined source census and admit the complete extraction queue**

  Owner **Ilya/O** · skill `orchestration` · `gpt-6-astra` / **high**. Predecessors: [P02 / #1004](https://github.com/mysteropodes/nemo/issues/1004), [C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036), [C02 / #1037](https://github.com/mysteropodes/nemo/issues/1037), [C03 / #1038](https://github.com/mysteropodes/nemo/issues/1038), [C04 / #1039](https://github.com/mysteropodes/nemo/issues/1039), [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040), [C06 / #1041](https://github.com/mysteropodes/nemo/issues/1041), [C07 / #1042](https://github.com/mysteropodes/nemo/issues/1042), [C08 / #1043](https://github.com/mysteropodes/nemo/issues/1043).

  Scope: `engineering/inventory/remediation-scope.json (new census index)`; `GitHub leaf issues and fixed source/consumer coverage`.

  1. Merge the eight census partitions at one source SHA; every tracked handwritten runtime/tooling root is classified exactly once, with explicit vendor/generated/data exclusions.
  2. Every remaining responsibility is covered by a linked executable leaf with whole-file writer, public API, consumer matrix and <=90-minute implementation scope; split larger units before Ready.
  3. Freeze counts and source-set digest; added product scope requires explicit human decision. Any unmatched source/surface fails the completeness check.

  Limit: Do not confuse 902 inventory rows with 902 separate capabilities, or classify every large data table as a code monolith.

- [x] **[F01 / #1047](https://github.com/mysteropodes/nemo/issues/1047) — Review the existing Honey installed-package evidence**

  Owner **Cyrill/O** · skill `validation` · `opus` / **high**. Predecessors: none; claim the file/runtime slot.

  Scope: `PR #1001 engineering/remediation/receipts/R14-packaging/**`.

  1. Read the latest PR #1001 head and corrected summary; verify artifact hashes and installed attachment evidence.
  2. Identify accepted architecture evidence and remaining product/platform limitations without calling FFmpeg/export fixed.
  3. Record a bounded review disposition on the existing lane and preserve Honey ownership until its handoff is explicit.

  Limit: Preserve known defects; no unrelated repair or feature addition.

- [x] **[F02 / #1048](https://github.com/mysteropodes/nemo/issues/1048) — Finish the existing Fizz payload-schema acceptance**

  Owner **Cyrill/D1** · skill `capabilities` · `sonnet` / **medium**. Predecessors: none; claim the file/runtime slot.

  Scope: `PR #1002 nemo-mcp/src/contract.rs`; `PR #1002 nemo-mcp/src/server.rs`; `PR #1002 nemo-mcp/tests/stdio_contract.rs`; `PR #1002 engineering/application/transport-v1.schema.json`.

  1. Continue the existing Fizz lane; do not assign a duplicate writer or replace its branch.
  2. Advertised object payload and instance identity spelling pass actual-client and malformed-input checks on the exact candidate.
  3. Existing PR includes focused validation and an explicit ready-for-review handoff; orchestrator integrates through normal protection.

  Limit: Preserve known defects; no unrelated repair or feature addition.

- [x] **[P14 / #1016](https://github.com/mysteropodes/nemo/issues/1016) — Complete two-instance native isolation acceptance**

  Owner **Cyrill/D2** · skill `validation` · `sonnet` / **medium**. Predecessors: none; claim the file/runtime slot.

  Scope: `tests/desktop/* R06 acceptance additions`; `scripts/nemo/native.cjs and runtime docs read-only`.

  1. Two identified instances have different app/browser/temp/cache/build/report roots.
  2. A deliberate cross-owner stop is rejected and reserved desktop/GPU slot behavior is observed.
  3. Record actual candidate bytes and process cleanup; unavailable platform is bounded to its claim.

  Limit: No unrelated FFmpeg, export, signing or platform-support repair; harness defect gets its own narrow packet.

- [x] **[P15 / #1017](https://github.com/mysteropodes/nemo/issues/1017) — Add one real opacity slider gesture regression**

  Owner **Cyrill/D1** · skill `validation` · `sonnet` / **medium**. Predecessors: none; claim the file/runtime slot.

  Scope: `new tests/browser/opacity-ui-consumers.spec.cjs`; `existing tests/browser/opacity-consumers.spec.cjs read-only`; `existing opacity fixtures`.

  1. Drive the actual opacity control on a fixed fixture; check independently specified final value.
  2. The gesture creates one undo entry; undo restores the recorded initial value and redo restores the edited value.
  3. Existing direct-command/save/reopen/export tests remain intact. Record any pre-existing UI failure precisely, without fixing it in this test leaf.

  Limit: Do not repeat existing save/reopen/SVG assertions under a second harness, or silently fix product behavior in test-only scope.

### B — contracts, registry and enforceable reports

| Ilya — owned steps | Cyrill — owned steps |
|---|---|
| [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006) · **O** · Approve capability descriptor v1 with two concrete examples | [P09 / #1011](https://github.com/mysteropodes/nemo/issues/1011) · **D2** · Wire capability/schema drift checks into normal local validation |
| [D01 / #1044](https://github.com/mysteropodes/nemo/issues/1044) · **O** · Specify document identity, time and coordinate compatibility | [P11 / #1013](https://github.com/mysteropodes/nemo/issues/1013) · **D1** · Cover Rust source classification and size ratchet |
| [D02 / #1045](https://github.com/mysteropodes/nemo/issues/1045) · **O** · Specify transaction, job and resource lifecycle contracts | [P13 / #1015](https://github.com/mysteropodes/nemo/issues/1015) · **D2** · Classify UI bootstrap/style/data separately from executable modules |
| [P05 / #1007](https://github.com/mysteropodes/nemo/issues/1007) · **D1** · Create deterministic feature registration and descriptor validation | [T01 / #1050](https://github.com/mysteropodes/nemo/issues/1050) · **D2** · Report and enforce migrated JavaScript coverage with c8 |
| [P06 / #1008](https://github.com/mysteropodes/nemo/issues/1008) · **D1** · Move opacity metadata and routing into its feature module | [T03 / #1052](https://github.com/mysteropodes/nemo/issues/1052) · **D2** · Retain useful Playwright failure traces and reports |
| [P07 / #1009](https://github.com/mysteropodes/nemo/issues/1009) · **D2** · Make Rust MCP discovery/dispatch consume feature contracts | [T04 / #1053](https://github.com/mysteropodes/nemo/issues/1053) · **D1** · Generate Rust coverage for the existing MCP crate |
| [P10 / #1012](https://github.com/mysteropodes/nemo/issues/1012) · **D2** · Enforce application dependency edges through the normal gate | [P16 / #1018](https://github.com/mysteropodes/nemo/issues/1018) · **O** · Accept the installed Claude opacity workflow |
| [P12 / #1014](https://github.com/mysteropodes/nemo/issues/1014) · **D2** · Enforce the geometry engine public-module dependency boundary | — |
| [B01 / #1046](https://github.com/mysteropodes/nemo/issues/1046) · **D2** · Enforce the native application/MCP public-module boundary | — |
| [T02 / #1051](https://github.com/mysteropodes/nemo/issues/1051) · **D2** · Distinguish exact baseline failures from new regressions | — |
| [F03 / #1049](https://github.com/mysteropodes/nemo/issues/1049) · **O** · Accept the installed Codex opacity workflow | — |

- [x] **[P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006) — Approve capability descriptor v1 with two concrete examples**

  Owner **Ilya/O** · skill `capabilities` · `gpt-6-astra` / **high**. Predecessors: none; claim the file/runtime slot.

  Scope: `engineering/application/capability-v1.schema.json (new)`; `one opacity descriptor fixture`; `one existing export-job descriptor fixture`.

  1. Schema names stable ID/version, typed input/output, units, effects, availability, handler key, fixture and examples.
  2. Opacity and export-job examples validate; malformed/duplicate IDs and unsupported version have specified failure cases.
  3. Registration is deterministic from trusted feature declarations; UI/API/MCP share handlers. Do not edit active Fizz files.

  Limit: No general plugin marketplace, arbitrary executable discovery, new DSL or TypeScript/framework rewrite. Do not overlap Fizz contract files while active.

- [ ] **[D01 / #1044](https://github.com/mysteropodes/nemo/issues/1044) — Specify document identity, time and coordinate compatibility**

  Owner **Ilya/O** · skill `architecture` · `gpt-6-astra` / **high**. Predecessors: [P02 / #1004](https://github.com/mysteropodes/nemo/issues/1004).

  Scope: `engineering/application/value-contract-v1.schema.json (new)`; `existing document/animation fixtures (read-only)`.

  1. Define stable IDs, document format version, time/frame and coordinate units using existing values; name the current state owner.
  2. An old project fixture roundtrips without changed serialized values; future migrations and unknown fields have an explicit compatibility rule.
  3. Document precision/color/alpha metadata at adapter boundaries; no storage-format conversion or new color pipeline.

  Limit: Preserve known defects; no unrelated repair or feature addition.

- [x] **[D02 / #1045](https://github.com/mysteropodes/nemo/issues/1045) — Specify transaction, job and resource lifecycle contracts**

  Owner **Ilya/O** · skill `architecture` · `gpt-6-astra` / **high**. Predecessors: [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Scope: `engineering/application/operation-contract-v1.schema.json (new)`; `existing opacity request/history code (read-only)`.

  1. Define request identity, expected revision, retry result, begin/update/commit/cancel and terminal job states.
  2. Define resource handle owner/disposal and stale-document behavior with explicit examples; one mutation authority remains.
  3. Public schema/version compatibility and server-side access checks are explicit; no generic service container or writable Rust mirror.

  Limit: Preserve known defects; no unrelated repair or feature addition.

- [x] **[P05 / #1007](https://github.com/mysteropodes/nemo/issues/1007) — Create deterministic feature registration and descriptor validation**

  Owner **Ilya/D1** · skill `capabilities` · `gpt-5.6-sol` / **medium**. Predecessors: [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006), [D02 / #1045](https://github.com/mysteropodes/nemo/issues/1045).

  Scope: `new src/js/application/capability-registry.js`; `new feature descriptor schema and registry tests`.

  1. Duplicate IDs, unsupported schema versions and missing handler/schema fail explicitly.
  2. Registry produces deterministic discovery from module registrations; handlers remain in feature owners.
  3. Opaque raw image/geometry data is represented by handles, not command JSON.

  Limit: Do not repair unrelated pre-existing feature defects. Preserve exact baseline behavior and record limitations.

- [ ] **[P06 / #1008](https://github.com/mysteropodes/nemo/issues/1008) — Move opacity metadata and routing into its feature module**

  Owner **Ilya/D1** · skill `capabilities` · `gpt-5.6-sol` / **medium**. Predecessors: [P05 / #1007](https://github.com/mysteropodes/nemo/issues/1007).

  Scope: `src/js/application/opacity-application.js`; `src/js/domain/animation/opacity.js`; `src/js/bootstrap/opacity-application.js`; `new opacity feature registration`; `tests/application-opacity*.cjs`.

  1. Current opacity operations, identity/revision/retry/history semantics pass existing independent controls through registry.
  2. The application public entry reaches the registered handler; no second opacity writer remains.
  3. Current non-opacity behavior and known defects remain unchanged.

  Limit: Do not repair unrelated pre-existing feature defects. Preserve exact baseline behavior and record limitations.

- [ ] **[P07 / #1009](https://github.com/mysteropodes/nemo/issues/1009) — Make Rust MCP discovery/dispatch consume feature contracts**

  Owner **Ilya/D2** · skill `capabilities` · `gpt-5.6-terra` / **medium**. Predecessors: [P05 / #1007](https://github.com/mysteropodes/nemo/issues/1007), [F02 / #1048](https://github.com/mysteropodes/nemo/issues/1048).

  Scope: `nemo-mcp/src/server.rs`; `nemo-mcp/src/contract.rs`; `nemo-mcp/src/schema.rs`; `src/js/adapters/application-mcp.js`; `MCP protocol tests`.

  1. MCP advertises the exact registered input/output contract and routes to shared application handlers.
  2. One generic transport change suffices: adding a subsequent feature requires no manual MCP operation switch entry.
  3. Malformed payload, unsupported capability and unavailable platform return typed actionable errors.

  Limit: Wait for Fizz #1002 publication/disposition; do not duplicate its payload fix. Preserve pinned supported transport until an explicit upgrade packet.

- [ ] **[P09 / #1011](https://github.com/mysteropodes/nemo/issues/1011) — Wire capability/schema drift checks into normal local validation**

  Owner **Cyrill/D2** · skill `validation` · `sonnet` / **medium**. Predecessors: [P05 / #1007](https://github.com/mysteropodes/nemo/issues/1007), [P07 / #1009](https://github.com/mysteropodes/nemo/issues/1009).

  Scope: `scripts/nemo/* capability check files (new)`; `tests/* capability checks (new)`; `scripts/nemo/ci.cjs shared wiring by Ilya`.

  1. Missing declared surface, stale generated schema, duplicate capability and missing example fixture each fail the named local gate.
  2. Negative controls exercise the shipped checker, not an unused test-only validator.
  3. The local receipt identifies the generated catalog and source digest; an omitted checker job cannot produce aggregate acceptance.

  Limit: No hosted Actions runs or new always-running CI automation.

- [ ] **[P10 / #1012](https://github.com/mysteropodes/nemo/issues/1012) — Enforce application dependency edges through the normal gate**

  Owner **Ilya/D2** · skill `validation` · `gpt-5.6-terra` / **medium**. Predecessors: [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Scope: `scripts/nemo/ci.cjs`; `scripts/nemo/lib/boundaries*.cjs`; `engineering/boundaries/profiles/app-js.profile.json`; `boundary negative-control tests`.

  1. The adopted migrated slice fails forbidden edge, private import, implicit global and cycle controls through the same local boundary command.
  2. Unmigrated classic-script relationships are honestly classified; they do not inherit fictional graph accuracy.
  3. Report analyzed versus legacy-unresolved paths separately, and retain no-growth checks on unresolved code without claiming full dependency coverage.

  Limit: Do not wait for full R03 or pretend text sizing proves dependency direction.

- [x] **[P11 / #1013](https://github.com/mysteropodes/nemo/issues/1013) — Cover Rust source classification and size ratchet**

  Owner **Cyrill/D1** · skill `validation` · `sonnet` / **medium**. Predecessors: [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040), [C07 / #1042](https://github.com/mysteropodes/nemo/issues/1042).

  Scope: `engineering/boundaries/profiles/* rust profiles`; `scripts/nemo/lib/boundaries-size.cjs`; `new Rust coverage controls`.

  1. All tracked Rust source roots discovered independently, including native, geometry and nemo-mcp; each path adopted or explicitly excluded.
  2. Oversized in-scope .rs file, undeclared file and raised legacy ceiling fail normal local validation.
  3. Emit the discovered Rust file count and classified/excluded path list; verify the test fixture does not alter retained legacy ceilings or generated/vendor exclusions.

  Limit: This packet proves discovery/size; it does not claim Cargo dependency/module graph enforcement.

- [ ] **[P12 / #1014](https://github.com/mysteropodes/nemo/issues/1014) — Enforce the geometry engine public-module dependency boundary**

  Owner **Ilya/D2** · skill `validation` · `gpt-5.6-terra` / **medium**. Predecessors: [P11 / #1013](https://github.com/mysteropodes/nemo/issues/1013), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Scope: `geometry-wasm/Cargo.toml`; `geometry-wasm/src/engine.rs`; `Rust dependency policy/checker tests`.

  1. Name permitted geometry engine module edges and the exported port.
  2. One forbidden production import fails the normal local gate using Rust-aware analysis; allowed edge passes.
  3. Declared Cargo feature configurations are exercised or explicitly blocked; no JS lexer is used for Rust.

  Limit: Do not feed Rust to a JavaScript lexer; unsupported graph cases are declared, not silently accepted.

- [ ] **[B01 / #1046](https://github.com/mysteropodes/nemo/issues/1046) — Enforce the native application/MCP public-module boundary**

  Owner **Ilya/D2** · skill `validation` · `gpt-5.6-terra` / **medium**. Predecessors: [P11 / #1013](https://github.com/mysteropodes/nemo/issues/1013), [D02 / #1045](https://github.com/mysteropodes/nemo/issues/1045).

  Scope: `src-tauri/src/application_mcp.rs`; `src-tauri/Cargo.toml`; `native Rust boundary policy/checker tests`.

  1. Declare the native application/MCP port and permitted dependencies.
  2. A private cross-boundary import fails the shipped local gate; approved port access passes.
  3. Unanalyzed Rust modules remain explicit census entries instead of receiving a false graph pass.

  Limit: Preserve known defects; no unrelated repair or feature addition.

- [x] **[P13 / #1015](https://github.com/mysteropodes/nemo/issues/1015) — Classify UI bootstrap/style/data separately from executable modules**

  Owner **Cyrill/D2** · skill `validation` · `sonnet` / **medium**. Predecessors: [C06 / #1041](https://github.com/mysteropodes/nemo/issues/1041), [C08 / #1043](https://github.com/mysteropodes/nemo/issues/1043).

  Scope: `src/index.html`; `src/css/style.css`; `src/js/i18n.js`; `src/js/shader-effects-library.js`; `associated profile policy`.

  1. Separate generated/data catalogs from handwritten control logic; target limits apply appropriately.
  2. A new unclassified bootstrap or executable block fails; existing catalog contents remain byte-equivalent where data-only.
  3. Record exact profile assignments and exclusion provenance; the normal checker rejects an executable file mislabeled as data by the adopted rule.

  Limit: No bulk formatting, translation rewrite or arbitrary splitting of tables purely to lower line counts. Exact source edits wait for their whole-file owner slot.

- [x] **[T01 / #1050](https://github.com/mysteropodes/nemo/issues/1050) — Report and enforce migrated JavaScript coverage with c8**

  Owner **Cyrill/D2** · skill `validation` · `sonnet` / **medium**. Predecessors: none; claim the file/runtime slot.

  Scope: `package.json`; `package-lock.json`; `new c8 configuration`; `curve/opacity coverage job tests`.

  1. Retain node:test; c8 --all reports every explicitly migrated module, including one deliberately unimported negative-control module.
  2. Generate HTML, LCOV and JSON reports; 90% lines and 80% branches per migrated domain/application module are enforced, with exact reviewed exceptions.
  3. An intentionally below-threshold run fails the receipt; existing tests remain discoverable and legacy modules are not falsely required to reach the new threshold.

  Limit: Preserve known defects; no unrelated repair or feature addition.

- [x] **[T02 / #1051](https://github.com/mysteropodes/nemo/issues/1051) — Distinguish exact baseline failures from new regressions**

  Owner **Ilya/D2** · skill `validation` · `gpt-5.6-terra` / **medium**. Predecessors: [P02 / #1004](https://github.com/mysteropodes/nemo/issues/1004).

  Scope: `scripts/nemo/lib/ baseline comparator (new)`; `fixture/baseline manifest`; `comparison tests`.

  1. Compare baseline/candidate with source, fixture and environment identity; classify unchanged known signature separately.
  2. A changed signature, newly failing case or missing required case fails; unavailable required runtime remains blocked.
  3. Keep raw test failures visible and produce a separate regression-comparison result; no blanket allow-failure or rewritten goldens.

  Limit: Preserve known defects; no unrelated repair or feature addition.

- [x] **[T03 / #1052](https://github.com/mysteropodes/nemo/issues/1052) — Retain useful Playwright failure traces and reports**

  Owner **Cyrill/D2** · skill `validation` · `sonnet` / **medium**. Predecessors: none; claim the file/runtime slot.

  Scope: `browser Playwright configuration`; `scripts/nemo browser job report adapter`; `browser harness negative control`.

  1. An intentional UI assertion failure produces an inspectable trace, HTML/JUnit report and failing receipt.
  2. Use retain-on-failure without automatic retry masking; passing runs follow the bounded retention policy.
  3. Report source/browser/runtime identity and reuse existing isolation; no real user document contents in test artifacts.

  Limit: Preserve known defects; no unrelated repair or feature addition.

- [x] **[T04 / #1053](https://github.com/mysteropodes/nemo/issues/1053) — Generate Rust coverage for the existing MCP crate**

  Owner **Cyrill/D1** · skill `validation` · `sonnet` / **medium**. Predecessors: [C07 / #1042](https://github.com/mysteropodes/nemo/issues/1042).

  Scope: `nemo-mcp/Cargo.toml (read-only unless necessary)`; `local cargo-llvm-cov job wrapper`; `Rust coverage receipt tests`.

  1. Run the existing MCP Cargo suites under a pinned compatible cargo-llvm-cov tool; produce HTML/LCOV summaries for production source.
  2. Record uncovered modules and observed initial percentages; introduce a reviewed no-regression ratchet rather than inventing a passing global target.
  3. A missing coverage tool/capability is blocked and a failed test fails. Native/geometry coverage adoption is a separate census leaf.

  Limit: Preserve known defects; no unrelated repair or feature addition.

- [ ] **[P16 / #1018](https://github.com/mysteropodes/nemo/issues/1018) — Accept the installed Claude opacity workflow**

  Owner **Cyrill/O** · skill `orchestration` · `opus` / **high**. Predecessors: [F01 / #1047](https://github.com/mysteropodes/nemo/issues/1047), [F02 / #1048](https://github.com/mysteropodes/nemo/issues/1048).

  Scope: `existing fizz/r14-parity harness and PR1001/1002 evidence`.

  1. Use the retained parity harness and identified installed app/MCP bytes through Claude with its advertised schema.
  2. Discover, inspect, edit, undo/redo and save/reopen the fixed opacity fixture; distinguish each result and any baseline defect.
  3. Record runtime/client versions, candidate hashes, trace and remaining platform limits; do not duplicate Fizz/Honey implementation.

  Limit: Retain Fizz/Honey current ownership; this is their existing work, not a new duplicate assignment. Existing FFmpeg failure need not block unrelated MCP/schema architecture acceptance; unsupported export claim stays open.

- [ ] **[F03 / #1049](https://github.com/mysteropodes/nemo/issues/1049) — Accept the installed Codex opacity workflow**

  Owner **Ilya/O** · skill `validation` · `gpt-6-astra` / **high**. Predecessors: [F01 / #1047](https://github.com/mysteropodes/nemo/issues/1047), [F02 / #1048](https://github.com/mysteropodes/nemo/issues/1048).

  Scope: `existing fizz/r14-parity tests/mcp-parity/** (reuse)`; `installed macOS app/MCP receipt`.

  1. Use the published parity harness against identified installed bytes through Codex using advertised schema.
  2. Discover, inspect, edit, undo/redo and save/reopen the fixed fixture; include reconnect and stale-write outcomes.
  3. Record candidate/client identity and exact accepted versus blocked cases; no hand-written payload oracle substitutes for client usability.

  Limit: Preserve known defects; no unrelated repair or feature addition.

### C — contrasting export job and shared diagnostics

| Ilya — owned steps | Cyrill — owned steps |
|---|---|
| [P17 / #1019](https://github.com/mysteropodes/nemo/issues/1019) · **D1** · Extract the existing single-frame SVG export adapter | [P19 / #1021](https://github.com/mysteropodes/nemo/issues/1021) · **D1** · Bind existing export UI and MCP to the same job |
| [H01 / #1058](https://github.com/mysteropodes/nemo/issues/1058) · **D1** · Map immutable SVG sequence inputs before job extraction | [P08 / #1010](https://github.com/mysteropodes/nemo/issues/1010) · **D1** · Verify export-feature auto-registration from a fresh agent session |
| [P18 / #1020](https://github.com/mysteropodes/nemo/issues/1020) · **D1** · Add bounded job lifecycle to that exporter | [T07 / #1056](https://github.com/mysteropodes/nemo/issues/1056) · **D1** · Roundtrip one isolated synthetic reproduction bundle |
| [T05 / #1054](https://github.com/mysteropodes/nemo/issues/1054) · **D1** · Extract bounded application diagnostics from opacity | [T08 / #1057](https://github.com/mysteropodes/nemo/issues/1057) · **D2** · Expose the shared diagnostics inspector in UI and MCP |
| [T06 / #1055](https://github.com/mysteropodes/nemo/issues/1055) · **D2** · Correlate one Rust MCP request with application diagnostics | — |

- [ ] **[P17 / #1019](https://github.com/mysteropodes/nemo/issues/1019) — Extract the existing single-frame SVG export adapter**

  Owner **Ilya/D1** · skill `extraction` · `gpt-5.6-sol` / **medium**. Predecessors: [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006), [P06 / #1008](https://github.com/mysteropodes/nemo/issues/1008), [D01 / #1044](https://github.com/mysteropodes/nemo/issues/1044), [D02 / #1045](https://github.com/mysteropodes/nemo/issues/1045).

  Scope: `src/js/export.js: exportFrameSVGString`; `new src/js/adapters/export-svg-frame.js`.

  1. Production `exportFrameSVGString` delegates to the adapter; XML preamble, width/height/viewBox and visibility toggling remain equivalent for the existing fixture.
  2. Existing opacity frame 0/10/20 SVG expectations and independent stored-document-before/after checks pass through the moved code.
  3. New module has explicit frame-builder/dimensions/Paper rectangle ports and applicable size/dependency enforcement; no second rendering implementation.

  Limit: Scope: `src/js/export.js:352`, `exportFrameSVGString(frameIdx)`, with existing `exportBuildFrame` supplied as a port; new `src/js/adapters/export-svg-frame.js`. Do not extract/rewrite the 320-line frame builder in this packet. Existing known oracle: `tests/browser/opacity-consumers.spec.cjs` calls this actual SVG path and checks keyed output pixels; this evidence is already present, not rerun here. No FFmpeg dependency. Suggested Ilya D1, after feature/export contract.

- [ ] **[H01 / #1058](https://github.com/mysteropodes/nemo/issues/1058) — Map immutable SVG sequence inputs before job extraction**

  Owner **Ilya/D1** · skill `architecture` · `gpt-5.6-sol` / **high**. Predecessors: [P17 / #1019](https://github.com/mysteropodes/nemo/issues/1019), [D02 / #1045](https://github.com/mysteropodes/nemo/issues/1045).

  Scope: `src/js/export.js: exportSVGSequenceToDir / exportFrameRange / exportBuildFrame`; `src/js/render-manager.js callers (read-only)`.

  1. Enumerate every source-state read needed by the chosen 3-frame static/keyed SVG fixture, including scene/component context, camera and frame dimensions; identify which are captured versus live.
  2. Specify a snapshot or protected evaluation-session port that prevents mixed document revisions between awaits; explicitly reject merely adding a `revision` label while reading mutable globals. Specify start/status/cancel/artifact states and cleanup of partial files.
  3. Name exact source owners and test cases for an immutable job and existing UI/MCP binding. Until this is accepted, P18/P19 stay Inbox; current code rereads global state per frame and has no cancellation contract.

  Limit: Bounded mapping only. Produce the exact contract and implementation packet in the issue; do not repair known context/cancel behavior or start a broad refactor.

- [ ] **[P18 / #1020](https://github.com/mysteropodes/nemo/issues/1020) — Add bounded job lifecycle to that exporter**

  Owner **Ilya/D1** · skill `extraction` · `gpt-5.6-sol` / **medium**. Predecessors: [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040), [P17 / #1019](https://github.com/mysteropodes/nemo/issues/1019), [H01 / #1058](https://github.com/mysteropodes/nemo/issues/1058).

  Scope: `new export application job module`; `src/js/export.js`; `job lifecycle tests`.

  1. start/status/cancel has stable job/request identity, monotonic terminal lifecycle and bounded progress.
  2. Retry does not create duplicate committed artifacts; late results cannot attach to a replaced document; cancellation never labels partial output complete.
  3. The existing exporter caller uses this job boundary; a three-frame fixture verifies terminal artifact identity and no mixed-revision reads through the H01-approved port.

  Limit: No general distributed scheduler, new worker fleet or durability guarantees beyond the explicitly chosen job contract.

- [ ] **[P19 / #1021](https://github.com/mysteropodes/nemo/issues/1021) — Bind existing export UI and MCP to the same job**

  Owner **Cyrill/D1** · skill `capabilities` · `sonnet` / **medium**. Predecessors: [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040), [P18 / #1020](https://github.com/mysteropodes/nemo/issues/1020), [P07 / #1009](https://github.com/mysteropodes/nemo/issues/1009).

  Scope: `export feature registration`; `dedicated browser/MCP export-job tests`; `UI bridge scoped by P03`.

  1. One real UI interaction and one actual MCP client reach the same job and artifact contract.
  2. Progress/cancel and terminal artifact identity match direct API, with declared platform availability.
  3. Disable the selected backend in a fixture: UI and MCP both report the same typed availability reason and no job or partial artifact is falsely marked successful.

  Limit: No pixel buffers through MCP JSON. Existing broken backends stay unavailable with reason.

- [ ] **[P08 / #1010](https://github.com/mysteropodes/nemo/issues/1010) — Verify export-feature auto-registration from a fresh agent session**

  Owner **Cyrill/D1** · skill `capabilities` · `sonnet` / **medium**. Predecessors: [P19 / #1021](https://github.com/mysteropodes/nemo/issues/1021).

  Scope: `export feature module and descriptor from P19`; `dedicated feature-registration acceptance tests`.

  1. A fresh agent loads the export feature declaration through normal generation/bootstrap without editing MCP source or a central operation switch.
  2. Direct API and MCP expose the same schema/handler and documented unavailable cases.
  3. Removing the declaration fails the standard completeness gate; restoring it passes. This accepts the second shape without creating a toy product feature.

  Limit: Do not invent a new product feature just to prove registration. Use only the export declaration and exact paths accepted in P19; verify that predecessor before starting.

- [ ] **[T05 / #1054](https://github.com/mysteropodes/nemo/issues/1054) — Extract bounded application diagnostics from opacity**

  Owner **Ilya/D1** · skill `diagnostics` · `gpt-5.6-sol` / **high**. Predecessors: [P06 / #1008](https://github.com/mysteropodes/nemo/issues/1008).

  Scope: `src/js/application/opacity-application.js trace/replay section`; `new application diagnostics port/service`; `tests/application-opacity-replay.test.cjs`.

  1. Reuse existing trace/replay behavior through a versioned service with build, instance, document, revision and request IDs.
  2. Bounded detached records cannot mutate the application; existing retention, retry and stale-document tests pass.
  3. Detailed recording is opt-in and bounded; no second state writer, arbitrary eval or shell capability.

  Limit: Preserve known defects; no unrelated repair or feature addition.

- [ ] **[T06 / #1055](https://github.com/mysteropodes/nemo/issues/1055) — Correlate one Rust MCP request with application diagnostics**

  Owner **Ilya/D2** · skill `diagnostics` · `gpt-5.6-terra` / **high**. Predecessors: [P07 / #1009](https://github.com/mysteropodes/nemo/issues/1009), [T05 / #1054](https://github.com/mysteropodes/nemo/issues/1054).

  Scope: `nemo-mcp/src transport instrumentation`; `nemo-mcp/Cargo.toml`; `MCP stdio tests`.

  1. Add direct Rust tracing dependency only where used; the same request ID links start, result and structured failure.
  2. Cancellation/termination closes the span; stderr/log sink is bounded and no diagnostics contaminate protocol stdout.
  3. A fixture sentinel for secrets/private paths is excluded by default; instrumentation does not change command authority.

  Limit: Preserve known defects; no unrelated repair or feature addition.

- [ ] **[T07 / #1056](https://github.com/mysteropodes/nemo/issues/1056) — Roundtrip one isolated synthetic reproduction bundle**

  Owner **Cyrill/D1** · skill `diagnostics` · `sonnet` / **medium**. Predecessors: [T05 / #1054](https://github.com/mysteropodes/nemo/issues/1054).

  Scope: `diagnostics reproduction bundle codec`; `synthetic opacity fixture`; `replay integration tests`.

  1. Export versioned fixture hash, command sequence, clock/seed and relevant versions into a bounded bundle.
  2. Import and replay into an isolated document produce the independently specified state/history digest.
  3. Malformed/version-incompatible bundles are rejected; active user document and private-path sentinel remain untouched.

  Limit: Preserve known defects; no unrelated repair or feature addition.

- [ ] **[T08 / #1057](https://github.com/mysteropodes/nemo/issues/1057) — Expose the shared diagnostics inspector in UI and MCP**

  Owner **Cyrill/D2** · skill `diagnostics` · `sonnet` / **medium**. Predecessors: [T05 / #1054](https://github.com/mysteropodes/nemo/issues/1054), [T07 / #1056](https://github.com/mysteropodes/nemo/issues/1056), [P07 / #1009](https://github.com/mysteropodes/nemo/issues/1009).

  Scope: `new Diagnostics panel binding`; `diagnostics feature descriptor`; `UI/MCP diagnostics tests`.

  1. A thin panel lists recent correlated operations, selected state and report links through the diagnostics API.
  2. MCP inspection/export uses the same handlers and schemas; on-demand detail avoids dumping the full document into context.
  3. Real panel interaction and MCP query observe the same synthetic trace; inspection is read-only and buffer limits hold.

  Limit: Preserve known defects; no unrelated repair or feature addition.

### D — first bounded family extractions, then the complete census queue

| Ilya — owned steps | Cyrill — owned steps |
|---|---|
| [P20 / #1022](https://github.com/mysteropodes/nemo/issues/1022) · **D1** · Extract folder metadata serialization | [P22 / #1024](https://github.com/mysteropodes/nemo/issues/1024) · **D1** · Extract expression clock conversion helpers |
| [H02 / #1059](https://github.com/mysteropodes/nemo/issues/1059) · **D1** · Map frame-only history capture and restore ownership | [A01 / #1061](https://github.com/mysteropodes/nemo/issues/1061) · **D1** · Extract seeded expression random draws |
| [P21 / #1023](https://github.com/mysteropodes/nemo/issues/1023) · **D1** · Extract the characterized frame-only history entry | [P28 / #1030](https://github.com/mysteropodes/nemo/issues/1030) · **D1** · Extract the FFmpeg probe text parser |
| [P23 / #1025](https://github.com/mysteropodes/nemo/issues/1025) · **D2** · Extract the pure Hungarian assignment solver | [P29 / #1031](https://github.com/mysteropodes/nemo/issues/1031) · **D1** · Extract the native project write adapter |
| [P24 / #1026](https://github.com/mysteropodes/nemo/issues/1026) · **D1** · Extract the playback frame-transition kernel | [P30 / #1032](https://github.com/mysteropodes/nemo/issues/1032) · **D2** · Extract shortcut binding data and persistence |
| [H03 / #1060](https://github.com/mysteropodes/nemo/issues/1060) · **D2** · Map one rotate-selection gesture and cancellation boundary | [P31 / #1033](https://github.com/mysteropodes/nemo/issues/1033) · **D2** · Register the existing Labs timelapse lifecycle |
| [P25 / #1027](https://github.com/mysteropodes/nemo/issues/1027) · **D2** · Extract the characterized rotate-selection gesture | — |
| [P26 / #1028](https://github.com/mysteropodes/nemo/issues/1028) · **D2** · Extract image LRU bookkeeping and eviction policy | — |
| [P27 / #1029](https://github.com/mysteropodes/nemo/issues/1029) · **D2** · Extract the existing Rust brightness/contrast pass | — |

- [x] **[P20 / #1022](https://github.com/mysteropodes/nemo/issues/1022) — Extract folder metadata serialization**

  Owner **Ilya/D1** · skill `extraction` · `gpt-5.6-sol` / **medium**. Predecessors: [C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Scope: `src/js/timeline.js: SM.exportJSON / SM.importJSON folder fields`; `src/js/tweens.js: folder snapshot/restore`; `new src/js/domain/document/folder-codec.js`.

  1. Independent fixture with two folders, names/collapsed flags and memberships round-trips through actual export/import with exact IDs and metadata.
  2. Missing legacy folder fields preserve current defaults; unknown nested metadata follows an explicitly recorded preservation policy, without redefining project format.
  3. Existing folder rename/grouping undo and save/reopen route through the same codec copy semantics; no folder/link-group feature changes and no new persistent writer.

  Limit: Scope: `SM.exportJSON` and `SM.importJSON` in `src/js/timeline.js` for only `layerFolders` and per-layer `folderId` (`2117`, `2170`, `2458`, `2562`); folder metadata snapshot/restore in `tweens.js`; new `src/js/domain/document/folder-codec.js`. Suggested Ilya D1. Reserve both monoliths; no concurrent P21/P23/P24/P30 edits. The existing `project-document.js` parser is already extracted; do not recreate it.

- [ ] **[H02 / #1059](https://github.com/mysteropodes/nemo/issues/1059) — Map frame-only history capture and restore ownership**

  Owner **Ilya/D1** · skill `architecture` · `gpt-5.6-sol` / **high**. Predecessors: [C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036).

  Scope: `src/js/tweens.js: _cloneStrokesForUndo / pushUndoActiveFrame / undo / redo (read-only)`.

  1. Trace Fill callers and prove the touched frame/layer set; identify all cache/UI/selection hooks and the entry schema `{frame,layers:[{strokes,isKeyframe,isInterpolated}]}`.
  2. Characterize capture→undo→redo on two layers at one frame and document current different-frame/context behavior. Full-layer history has context guards; the frame-only branch does not share those guards. Record any existing failure without silently fixing it.
  3. Specify exact capture/apply ports and cloned-heavy-field policy, plus an independent test for a stringify exception restoring live heavy fields. Exclude full-layer snapshot redesign.

  Limit: Bounded mapping only. Produce the exact contract and implementation packet in the issue; do not repair known context/cancel behavior or start a broad refactor.

- [ ] **[P21 / #1023](https://github.com/mysteropodes/nemo/issues/1023) — Extract the characterized frame-only history entry**

  Owner **Ilya/D1** · skill `extraction` · `gpt-5.6-sol` / **medium**. Predecessors: [C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036), [P20 / #1022](https://github.com/mysteropodes/nemo/issues/1022), [P06 / #1008](https://github.com/mysteropodes/nemo/issues/1008), [H02 / #1059](https://github.com/mysteropodes/nemo/issues/1059).

  Scope: `src/js/tweens.js undo section`; `new application/history module`; `history contract tests`.

  1. One identified history entry kind has explicit apply/revert lifecycle and independent multi-document/context tests.
  2. UI/API/MCP share undo ownership and exactly one entry per declared transaction.
  3. Remove the superseded frame-entry capture/apply writer; preserve H02-characterized limitations explicitly and keep full-layer history outside this leaf.

  Limit: Do not replace the entire undo system in one task. Extract only the frame-only entry characterized in H02; P03 tracks the remaining history responsibilities separately.

- [x] **[P22 / #1024](https://github.com/mysteropodes/nemo/issues/1024) — Extract expression clock conversion helpers**

  Owner **Cyrill/D1** · skill `extraction` · `sonnet` / **medium**. Predecessors: [C02 / #1037](https://github.com/mysteropodes/nemo/issues/1037), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Scope: `src/js/motion.js: exprStepTime / exprToFrames / exprToSeconds`; `new src/js/domain/animation/expression-time.js`.

  1. Fixed examples cover frame/second conversion, omitted arguments, zero fps and invalid/non-positive step sizes with current behavior.
  2. `stepTime` changes subsequent context-dependent evaluation in the same expression; bare `time`/`frame` argument values retain their documented unsnapped behavior.
  3. Real expression evaluation calls the extracted helpers; no `_ectx`/window/state access inside the domain module and limits pass.

  Limit: Scope: `motion.js:1712–1732`: `exprStepTime`, `exprToFrames`, `exprToSeconds`; new `src/js/domain/animation/expression-time.js`, existing expression evaluator as caller. Explicit context/fps/numeric-coercion inputs. Suggested Cyrill D1; exclusive motion.js slot.

- [x] **[A01 / #1061](https://github.com/mysteropodes/nemo/issues/1061) — Extract seeded expression random draws**

  Owner **Cyrill/D1** · skill `extraction` · `sonnet` / **medium**. Predecessors: [P22 / #1024](https://github.com/mysteropodes/nemo/issues/1024).

  Scope: `src/js/motion.js: exprSeed / _rand01 / _gauss01 / _randomWith / exprRandom*`; `new domain animation random module`.

  1. Independent fixed seed/counter/frame vectors preserve varying versus fixed draws and repeatability; different property seeds remain distinct.
  2. Scalar and array min/max, Gaussian pair consumption and counter increments match captured baseline; no random algorithm improvement.
  3. Existing expression runtime calls new module; deterministic tests require no DOM/global state and size/dependency gates pass.

  Limit: Exclude noise/wiggle and algorithm improvements. Whole motion.js slot must be released by P22 first.

- [ ] **[P23 / #1025](https://github.com/mysteropodes/nemo/issues/1025) — Extract the pure Hungarian assignment solver**

  Owner **Ilya/D2** · skill `extraction` · `gpt-5.6-terra` / **medium**. Predecessors: [C02 / #1037](https://github.com/mysteropodes/nemo/issues/1037), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Scope: `src/js/tweens.js: hungarian(cost)`; `new src/js/domain/tween/assignment.js`.

  1. Empty, 1×1 and independently brute-force-checked small square matrices return minimum-cost assignments.
  2. Fixed tie matrices preserve current tie order; input matrices are unchanged.
  3. Production matching invokes the module and fixed tween correspondence fixtures match baseline; no new matching heuristic or Rust parity claim.

  Limit: Scope: `tweens.js:415–445`, `hungarian(cost)`, called from existing `autoMatchJS` and relational matching. New `src/js/domain/tween/assignment.js`. Suggested Cyrill D1 or Ilya D2; exclusive tweens.js slot. The entire `autoMatchJS` matcher is not a small pure leaf: feature extraction uses Paper and refinement dependencies. Do not name this packet “complete tween matcher extraction.”

- [ ] **[P24 / #1026](https://github.com/mysteropodes/nemo/issues/1026) — Extract the playback frame-transition kernel**

  Owner **Ilya/D1** · skill `extraction` · `gpt-5.6-sol` / **medium**. Predecessors: [C02 / #1037](https://github.com/mysteropodes/nemo/issues/1037), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Scope: `src/js/timeline.js: advancePlayFrame`; `new domain playback-step kernel and existing wrapper`.

  1. Independent cases cover forward edge, non-loop stop, normal wrap, ping-pong reversal and single-frame work area.
  2. Wrapper preserves direction mutation and exactly the same audio-loop calls; real `startPlay` still calls the extracted kernel.
  3. Fixed-clock browser playback reaches expected frames; no change to frame dropping, auto-bake or fps storage, and domain limits/global prohibition pass.

  Limit: Scope: `timeline.js:29`, `advancePlayFrame(cur)`; new domain playback step function returns next frame/direction/loop-event, wrapper applies state and calls `SMAudio.onLoop`. `startPlay` rAF accumulator/auto-bake remains outside this leaf. Suggested Ilya D1.

- [ ] **[H03 / #1060](https://github.com/mysteropodes/nemo/issues/1060) — Map one rotate-selection gesture and cancellation boundary**

  Owner **Ilya/D2** · skill `architecture` · `gpt-5.6-terra` / **high**. Predecessors: [C04 / #1039](https://github.com/mysteropodes/nemo/issues/1039).

  Scope: `src/js/tools.js: rotate onMouseDown / onMouseDrag / onMouseUp / rotateCenterSegments (read-only)`.

  1. Map stable selected objects and every mutated ordinary/vector-brush/gradient companion field, including initial pivot and coordinate space.
  2. Capture begin→multiple drags→mouse-up→undo on one ordinary and one vector-brush fixture; characterize Escape/cancel and document replacement as current behavior or known failure.
  3. Define begin/update/commit/cancel ports with one history owner and exact new module/caller paths. Current code pushes history on mouse-down and mutates live Paper during drag; a generic wrapper alone cannot prove cancellation.

  Limit: Bounded mapping only. Produce the exact contract and implementation packet in the issue; do not repair known context/cancel behavior or start a broad refactor.

- [ ] **[P25 / #1027](https://github.com/mysteropodes/nemo/issues/1027) — Extract the characterized rotate-selection gesture**

  Owner **Ilya/D2** · skill `extraction` · `gpt-5.6-terra` / **medium**. Predecessors: [C04 / #1039](https://github.com/mysteropodes/nemo/issues/1039), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006), [P06 / #1008](https://github.com/mysteropodes/nemo/issues/1008), [H03 / #1060](https://github.com/mysteropodes/nemo/issues/1060).

  Scope: `src/js/tools.js transform section`; `src/js/select-bridge.js affected binding`; `new selection gesture application module`.

  1. begin/update/commit/cancel operates on stable selected IDs, coordinate convention and initial snapshot.
  2. Commit has one undo entry. Supported cancellation restores initial state; otherwise preserve the H03-characterized failure and expose its precise limitation. Tests distinguish existing cancellation/context defects from extraction regressions.
  3. Route only the H03-mapped rotate gesture through the new lifecycle and remove its duplicate writer; scale/node-edit rotation remain unchanged.

  Limit: Do not repair unrelated selection tools; do not assign tools.js to another writer concurrently.

- [ ] **[P26 / #1028](https://github.com/mysteropodes/nemo/issues/1028) — Extract image LRU bookkeeping and eviction policy**

  Owner **Ilya/D2** · skill `extraction` · `gpt-5.6-terra` / **medium**. Predecessors: [C03 / #1038](https://github.com/mysteropodes/nemo/issues/1038), [P04 / #1006](https://github.com/mysteropodes/nemo/issues/1006).

  Scope: `src/js/engine-bridge.js: _noteImageRegistered / _touchImage / _imgTotalBytes / enforceImageBudget`; `new application render image-budget module`.

  1. Independent sequences prove current-build images are protected, least-recently-used inactive images retire first, and total bytes/counters match dimensions.
  2. Failed `engine.retire_images` leaves bookkeeping and `registeredImageIds` untouched; success clears both so next use reuploads.
  3. Actual scene build uses begin/use/end lifecycle and existing budget/stats API; no new default budget or CPU/GPU copy optimization, fixed image rendering survives eviction/reupload.

  Limit: Scope: `engine-bridge.js:627–677`: `_imgBytes`, `_imgLastUsed`, `_imgUsedThisBuild`, tick/budget/eviction state and `_noteImageRegistered`, `_touchImage`, `_imgTotalBytes`, `enforceImageBudget`; lifecycle calls at scene begin/end (~807/~2653), public stats/budget at4583–4598. New application/render image-budget module; GPU upload adapters and retained path store unchanged. Suggested Ilya D2.

- [ ] **[P27 / #1029](https://github.com/mysteropodes/nemo/issues/1029) — Extract the existing Rust brightness/contrast pass**

  Owner **Ilya/D2** · skill `extraction` · `gpt-5.6-terra` / **medium**. Predecessors: [C03 / #1038](https://github.com/mysteropodes/nemo/issues/1038), [P12 / #1014](https://github.com/mysteropodes/nemo/issues/1014).

  Scope: `geometry-wasm/src/engine.rs: create_color_adjust_pipeline / color_adjust_pass`; `new geometry-wasm/src/engine/color_adjust.rs`.

  1. Production compositor creates and invokes the extracted pass, with explicit device/queue/layout/source/target/uniform inputs and unchanged shader bytes.
  2. Fixed brightness/contrast and neutral-parameter render fixtures match before/after within the existing tolerance, including alpha behavior.
  3. Rust build/module visibility and size/dependency controls pass; no color-space/precision migration or other effect pass in this leaf.

  Limit: Scope: `geometry-wasm/src/engine.rs:1665–1802`, `create_color_adjust_pipeline` and `color_adjust_pass`; new `geometry-wasm/src/engine/color_adjust.rs`; existing `color_adjust.wgsl` unchanged. Creation/use sites in `create_engine`/`run_one_effect` are wiring only. Suggested Ilya D2 after Rust module rule agreed.

- [x] **[P28 / #1030](https://github.com/mysteropodes/nemo/issues/1030) — Extract the FFmpeg probe text parser**

  Owner **Cyrill/D1** · skill `extraction` · `sonnet` / **medium**. Predecessors: [C05 / #1040](https://github.com/mysteropodes/nemo/issues/1040), [P11 / #1013](https://github.com/mysteropodes/nemo/issues/1013).

  Scope: `src-tauri/src/video_decode.rs: parse_probe / parse_hms`; `new src-tauri/src/media_probe.rs`.

  1. Existing captured H264/HEVC/VP9/ProRes/odd-dimension/25fps text yields identical width,height,fps,duration,codec values.
  2. Audio-only and malformed Duration/video/dimensions/fps cases preserve existing error strings and no subprocess runs.
  3. `open_session_core` invokes new parser; focused Cargo and source boundary tests pass. No FFmpeg install, codec or seek timing repair.

  Limit: Scope: `video_decode.rs:710–761`, `parse_probe` and `parse_hms`; existing seven `parse_probe_*` tests around1459–1512; new `src-tauri/src/media_probe.rs`, `open_session_core` remains caller. Suggested Cyrill D1.

- [ ] **[P29 / #1031](https://github.com/mysteropodes/nemo/issues/1031) — Extract the native project write adapter**

  Owner **Cyrill/D1** · skill `extraction` · `sonnet` / **medium**. Predecessors: [C01 / #1036](https://github.com/mysteropodes/nemo/issues/1036), [P20 / #1022](https://github.com/mysteropodes/nemo/issues/1022).

  Scope: `src/js/project.js: writeProjectTo filesystem block`; `new src/js/adapters/project-native-save.js`.

  1. Success writes `<path>.saving`, renames it and returns only after completion; caller marks saved afterward with unchanged JSON bytes.
  2. Rename/write failure follows **current** remove-temp/direct-write fallback order and propagates final failure. This fallback is a pre-existing crash-safety limitation; extracting it does not certify atomic save.
  3. Browser save path remains unchanged; native UI save and save-as invoke adapter, tests use injected fs failures and normal module gate passes. No open/read/cloud-sync extraction.

  Limit: Scope: filesystem block in `project.js:120–139` `writeProjectTo(path)`; new `src/js/adapters/project-native-save.js` receives `path`, already serialized JSON and fs port. Existing project state/recents/dirty/autosave updates remain in caller. Suggested Cyrill D1; cannot overlap P20 project-related wiring.

- [ ] **[P30 / #1032](https://github.com/mysteropodes/nemo/issues/1032) — Extract shortcut binding data and persistence**

  Owner **Cyrill/D2** · skill `extraction` · `sonnet` / **medium**. Predecessors: [C06 / #1041](https://github.com/mysteropodes/nemo/issues/1041), [P05 / #1007](https://github.com/mysteropodes/nemo/issues/1007).

  Scope: `src/js/timeline.js: shortcut tables / overrides / setShortcutKey`; `new preferences shortcut registry`.

  1. Defaults, stored overrides and reset survive a fresh preference context; action-to-key output matches fixed baseline table.
  2. Conflict/reserved keys and malformed stored JSON preserve current behavior; storage failure does not silently change documented result semantics.
  3. Actual keyboard dispatch and modal rebind use registry, with a registered preference query/command on same state; no remapping all keyboard event logic.

  Limit: Scope: `timeline.js:7457–7587`: `TOOL_SHORTCUTS`, `COMMAND_SHORTCUTS`, `READONLY_SHORTCUTS`, `shortcutOverrides`, `shortcutDefFor`, `shortcutKeyFor`, `shortcutClashFor`, `setShortcutKey`; new preferences shortcut registry with localStorage and badge-refresh ports. Leave `runCommandShortcut`, `runToolShortcut` and modal DOM rendering as callers. Suggested Cyrill D2; exclusive timeline.js slot.

- [ ] **[P31 / #1033](https://github.com/mysteropodes/nemo/issues/1033) — Register the existing Labs timelapse lifecycle**

  Owner **Cyrill/D2** · skill `capabilities` · `sonnet` / **medium**. Predecessors: [C06 / #1041](https://github.com/mysteropodes/nemo/issues/1041), [P05 / #1007](https://github.com/mysteropodes/nemo/issues/1007).

  Scope: `src/js/labs/timelapse.js: timelapseStart / timelapseStop`; `new timelapse feature descriptor`.

  1. Descriptor names start/stop/query availability, fps input and result shape, flag `nemo-labs-timelapse`, required canvas/MediaRecorder support; absent/unloaded/off states have explicit reasons.
  2. When existing module is loaded/enabled in isolated fixture, registered handlers call the same start/stop lifecycle; repeated start/stop and no-recorder cases match current behavior.
  3. Disabling ends owned timer/recording according to current observed contract; absent MediaRecorder remains unavailable. Any recording/cleanup defect is recorded rather than fixed, and no product feature is newly enabled.

  Limit: Scope: `src/js/labs/timelapse.js`, `SMLabs.timelapseStart`, `SMLabs.timelapseStop`, `SMLabs.register('timelapse',...)`; new feature descriptor/contract tests. Do not add it to startup scripts merely to claim coverage. Suggested Cyrill D2 after registry contract.

### E — integration and closure

| Ilya — owned steps | Cyrill — owned steps |
|---|---|
| [P33 / #1035](https://github.com/mysteropodes/nemo/issues/1035) · **O** · Accept the completed fixed census against integrated evidence | [P32 / #1034](https://github.com/mysteropodes/nemo/issues/1034) · **O** · Verify one clean-clone validation and agent-onboarding run |

- [ ] **[P32 / #1034](https://github.com/mysteropodes/nemo/issues/1034) — Verify one clean-clone validation and agent-onboarding run**

  Owner **Cyrill/O** · skill `validation` · `opus` / **high**. Predecessors: [P09 / #1011](https://github.com/mysteropodes/nemo/issues/1011), [P10 / #1012](https://github.com/mysteropodes/nemo/issues/1012), [P11 / #1013](https://github.com/mysteropodes/nemo/issues/1013), [P16 / #1018](https://github.com/mysteropodes/nemo/issues/1018), [F03 / #1049](https://github.com/mysteropodes/nemo/issues/1049), [T01 / #1050](https://github.com/mysteropodes/nemo/issues/1050), [T03 / #1052](https://github.com/mysteropodes/nemo/issues/1052), [T04 / #1053](https://github.com/mysteropodes/nemo/issues/1053).

  Scope: `existing entry docs/commands`; `dedicated clean-clone acceptance receipts`.

  1. Fresh Codex/Claude sessions find the single execution document and module contract, run named local validation and identify intended installed Nemo.
  2. Required failed/missing jobs cannot become aggregate pass; all platform limits and known baseline failures stay visible.
  3. A new agent locates one Ready assigned leaf and the precise report artifacts without an untracked personal skill or a second handoff document.

  Limit: No hosted Actions enablement; source and installed acceptance remain separate claims.

- [ ] **[P33 / #1035](https://github.com/mysteropodes/nemo/issues/1035) — Accept the completed fixed census against integrated evidence**

  Owner **Ilya/O** · skill `orchestration` · `gpt-6-astra` / **high**. Predecessors: [P03 / #1005](https://github.com/mysteropodes/nemo/issues/1005), [P08 / #1010](https://github.com/mysteropodes/nemo/issues/1010), [P32 / #1034](https://github.com/mysteropodes/nemo/issues/1034).

  Scope: `fixed remediation census and all linked completed leaf receipts (read-only)`; `final source and capability completeness checks`.

  1. Every census responsibility has a merged implementation or a reviewed non-code disposition; all dynamically created extraction leaves are complete.
  2. No obsolete writer/facade or expired legacy code exception remains; facade removal is part of its extraction leaf, never hidden work inside this audit.
  3. Normal coverage, boundaries, missing-registration and schema gates pass their negative controls; known product defects remain separate and human-accepted.

  Limit: This is a final acceptance task, not a substitute for unfinished migrations. Finishing P04-P32 alone does not establish all-monolith completion.

## 9. Final acceptance — the finish line

- [ ] P03's frozen source/consumer census is complete, including subsequently admitted small leaves. No handwritten monolith remains hidden under a legacy exception; legitimate generated/vendor/data files have explicit dispositions.
- [ ] Each feature has a coherent public API, one state authority, applicable lifecycle/resource contracts and a capability registration. Existing UI/API/MCP consumers use the same implementation; obsolete writers and bypasses are removed.
- [ ] Relevant unit, regression, browser and native checks protect migrated behavior. Coverage and failure reports are inspectable at the final source SHA. Known defects are explicit product debt, not repair prerequisites or concealed passing tests.
- [ ] Normal local validation enforces adopted boundaries, size profiles, schema freshness and registration completeness. Each checker has a meaningful failing negative control.
- [ ] A fresh agent can add a feature declaration using the documented convention and exercise it through the bundled Rust MCP without editing a central dispatcher. Both real clients have identified installed acceptance evidence for the supported macOS slice.
- [ ] Debugging provides bounded correlated inspection and a reproducible isolated fixture path. Protocol stdout, document ownership and user data remain intact.
- [ ] Ilya and Cyrill accept the structural result and its explicit product/platform limitations. Close the remaining tracking parents, reconcile the primary board and final central report, and release completed branch/worktree/runtime ownership.

No assertion above requires all pre-existing product bugs to be fixed. No open extraction, unowned state writer or missing architecture evidence may be renamed product debt just to declare remediation finished.

## 10. Buzz, hourly reports and the two startup packets

### One visible reporting place

Use the [shared orchestrator progress log, issue #1062](https://github.com/mysteropodes/nemo/issues/1062), accessible through [Hourly progress reports on the primary board](https://github.com/users/mysteropodes/projects/2/views/10). Both humans and future contributors read the same chronological log. It is a coordination issue, not an executable leaf, and does not inflate task-completion counts. Leaf issues/PRs retain detailed evidence; the central report links them. No report folder, status branch or check-in PR is needed.

- [ ] At session start, each orchestrator posts human/team, active leaf IDs, model/effort, allocated writer/runtime slots and next checkpoint. Read the other team's latest report before allocating overlapping work.
- [ ] While active, publish one report per team each hour. Also report a material blocker, ownership handoff, review-ready result or session end without waiting for the timer. Use existing delegate receipts; do not stop busy workers to fill a report.
- [ ] Include an explicit `active`, `paused` or `finished` session state. A timer does not infer permission to resume an inactive session. Retained claims remain owned while paused; silence does not release them.
- [ ] Before posting, read recent comments and reconcile the marker for this team/hour. Edit or link an already delivered report rather than duplicating it. After an uncertain write, inspect the actual issue before retrying.
- [ ] Update changed leaf fields on Project #2 only from fresh evidence and read them back. Then post the report's link in the designated Nemo Buzz conversation. If Buzz delivery is unavailable, keep the GitHub report and record that limitation. Do not invent another destination or send direct messages.

Copy this short report format; omit unchanged detail and use links:

```text
<!-- nemo-hourly:team=<Ilya-or-Cyrill>;hour=<YYYY-MM-DDTHH> -->
UTC time / team / session active-paused-finished:
O, D1, D2: actual model-effort and active leaf IDs:
Integrated source SHA; candidate SHAs/PRs separately:
Accepted since last report: outcome, merged SHA, issue/PR and evidence:
Progress awaiting acceptance: outcome, issue/PR and remaining check:
Evidence: checks/coverage/trace links; failed, blocked or not-run limits:
Current blockers and owner; next concrete action per occupied lane:
Whole-file/runtime claims retained or released; pending push/review:
Scoped census: accepted / active / review / blocked counts and changes:
Board changes read back; quota/capacity if actually known:
Next checkpoint; Buzz report-link delivery:
```

An hourly report records progress; it does not establish acceptance. Never label a feature Done because its delegate returned, tests passed in isolation, or its PR was merely opened.

### Workspace instructions, team overlays and reminders

Keep three layers with explicit purposes:

| Layer | What belongs there | Who maintains it |
|---|---|---|
| Shared Nemo workspace instructions | Baseline definition, architecture invariants, primary board/log, ownership, testing and Done rules. | Existing signed Project owner; all enrolled collaborators receive the same shared value. |
| Team Instructions | Human name, language, permitted model/effort settings, quota policy, local delegate limits and review responsibilities. | Each human for their own team. A team overlay does not weaken shared project rules. |
| Task and scheduled prompt | Current leaf/claim, immediate next action, report cadence/destination and pause/resume intent. | The orchestrator under the human's current instruction. |

The original `0.5.23-nemo.13` Settings viewer is read-only: `NemoWorkspaceSettingsCard.tsx` renders the policy and `project_preload.rs` embeds `docs/NEMO_WORKSPACE_INSTRUCTIONS.md` at build time. Changing that built-in text requires rebuilding the app/ACP; changing a manually authored reminder does not. Existing **Edit team → Team Instructions → Save changes** persists a team overlay. In that build, already running processes need a safe restart to receive changed team instructions; saving alone is not proof of uptake.

The separately authorized Buzz `0.5.23-nemo.14` update adds a persistent workspace instruction editor using the existing signed Project settings, with built-in fallback/reset and visible saved revision. Only the Project's signing owner edits the shared value; other members use their own Team Instructions for differences. Do not relabel a per-device value as shared or grant broader access to make the editor work. Open **Settings → Agents → Nemo workspace → Review workspace instructions**. The owner edits **Shared workspace instructions**, then uses **Save shared instructions**, **Reset to built-in** or **Reload saved value**. A conflicting save keeps the draft; copy any unsaved work, reload the current saved value, reconcile the change and retry. Shared changes take effect between agent turns on this build; a saved revision is not proof that an idle/offline agent has received it. **Team Instructions still require the affected local agent to restart at a safe checkpoint.** Release, installer and runtime-uptake evidence must be recorded before claiming this update installed on either Mac. Broader Buzz transport changes remain outside Nemo remediation.

Use this shared program supplement, either in the new workspace editor or as an immediate manually authored task instruction while installing the update:

```text
Nemo remediation uses engineering/remediation/EXECUTION_PLAN.en.md and its French copy as one operating plan. Baseline means the exact observed state, including known defects. Extract coherent modules, preserve state/data/history consumers, add regression protection and size/dependency/schema gates, and register feature-owned capabilities through the shared application API and bundled Rust MCP. Do not expand the task into unrelated product repairs.

Primary board: https://github.com/users/mysteropodes/projects/2 . Project 8 is a legacy snapshot. Live claims and evidence belong to each leaf issue/PR. Both orchestrators post hourly while active, and at blockers/handoffs/session end, in https://github.com/mysteropodes/nemo/issues/1062 ; link the report from the designated Nemo Buzz conversation. One human owner and one writer per leaf. Use at most one orchestrator plus two local delegates per human, preserve existing claims and quotas, and reuse one branch/PR across sprints. No report branches or PRs.

Read before writing and read back board/report changes. An inactive session stays inactive. Timers do not dispatch workers, interrupt busy agents, cancel/reassign claims, repair product code or certify Done. Each collaborator team reviews, locally validates, merges and closes its own work under section 7; external-author PRs require collaborator approval. A policy-bot acknowledgement is not technical acceptance. Use the owning team’s orchestrator as Validation owner; coordinate actual shared decisions without a routine cross-team approval gate. If explicit remote A2A is requested, preserve separate stored/processed/accepted/progress/terminal receipts and reconcile uncertain effects before retrying. Tool availability is not new authority.
```

### Per-human setup and activation checks

Prepare triggers **paused**. Activate them only when actual execution work starts, and pause them whenever the team stops. Planning alone does not activate coordination reminders. Use **one hourly trigger per team**, targeting its orchestrator. Do not install both a Codex heartbeat and a Buzz hourly workflow for the same team. The chosen trigger is recorded in #1062 so another orchestrator cannot accidentally duplicate it.

| Ilya | Cyrill |
|---|---|
| Use the English checklist. O=`gpt-6-astra` high; D1=`gpt-5.6-sol` medium; D2=`gpt-5.6-terra` medium. | Use the French checklist. O=`opus` high at milestones; start with one `sonnet` medium delegate and enable the second only with independent work and sufficient quota. |
| The Codex heartbeat **Nemo — Ilya hourly coordination** (`nemo-ilya-hourly-coordination`) is prepared and **PAUSED**. At actual execution kickoff, target the real orchestrator task and activate it; keep it paused during planning and inactivity. | Prepare the manually authored hourly reminder **paused** on your own account, targeting your actual orchestrator; activate it only at execution kickoff. This packet does not claim a timer has been created or delivered on your Mac. |
| Record the heartbeat destination and its first real report receipt in #1062. If moving to a Buzz workflow, pause this heartbeat first. | Buzz exposes a workflow **Schedule → Every hour** option. Configure its message destination as the designated Nemo conversation and explicitly address your actual orchestrator. Verify one real delivery before calling the schedule operational. |
| Pause the trigger at session end; post retained/released claims. Resume only with a new session-start notice. | Pause the trigger at session end or a quota handoff. A missing receipt is a setup problem to report, not permission to launch a replacement agent. |

- [ ] Install only the identified Mac release from Ilya's shared download link, verify its version/hash against the release receipt, and keep a recoverable prior app. The link must identify the actual tested installer; no placeholder is a download.
- [ ] Open workspace settings and verify shared source/revision. If not the signed Project owner, retain the shared policy and edit only your own team overlay.
- [ ] In your team instructions, set your human name, report language, model/effort defaults and quota policy from the table. Save the team overlay, preserve the current task checkpoint and restart the affected local agent when safe. Verify the new overlay and shared revision in its next startup acknowledgement. Shared workspace edits alone use the next-turn refresh; they do not require a busy task to restart.
- [ ] In one compact startup acknowledgement, the orchestrator names its human, primary board, log, current leaves, actual models and effective instruction revision. This confirms uptake rather than merely storage.
- [ ] Save the reminder **paused** with the prompt below, your own human name and the actual destination. Confirm no duplicate timer already exists; record the trigger/destination in the shared log.
- [ ] At actual execution kickoff, activate the trigger; observe one actual scheduled delivery and report link. Record send/receive time and resulting report URL. Scheduler configuration, relay storage and actual orchestrator execution are separate observations.
- [ ] At session end post a handoff and pause the trigger. At the next session, read both teams' latest reports, reconcile claims and then resume it.

Copy-ready hourly reminder; replace `<human>` with Ilya or Cyrill. The French copy supplies the French wording for Cyrill:

```text
Coordinate the active Nemo remediation session for <human>. Read only the latest handoff and recent reports in https://github.com/mysteropodes/nemo/issues/1062 and this human's active leaves on primary https://github.com/users/mysteropodes/projects/2 . Project 8 is a legacy snapshot. If the human has no active session, or it is explicitly paused/finished, stay quiet and do not start work.

While active, publish one concise hourly report in issue 1062: UTC time and session state; human/lane and actual model-effort if known; active leaf IDs; verified candidate SHAs/PRs; outcomes since last report; test/coverage evidence links and limitations; blockers; next concrete actions; claims retained/released; quota/capacity if known. Read recent comments first and use the nemo-hourly marker for this human and UTC hour to avoid duplicates. Reconcile an uncertain posting result before retrying. Use available delegate receipts; do not interrupt busy agents.

Update only this human's current task fields on Project 2 when supported by fresh evidence, and read them back. Link the report once in the designated Nemo Buzz conversation; if delivery is unavailable, record the limitation without inventing a destination or DMing anyone. Preserve the exact-state baseline and current ownership. Do not dispatch workers, cancel/reassign claims, change product code, merge, rebuild, publish releases or create report branches. A report is not completion. Notify the human of the requested hourly report while active and of material blockers. Stop routine reporting after a session-end notice until an explicit new session starts.
```

### Buzz installer for Ilya and Cyrill

Download **Buzz `0.5.23-nemo.14` for Apple Silicon Macs** from the [shared v.14 folder](https://drive.google.com/drive/folders/15g9EHoc5FlSSZPH7ek2x8dgBLfzeETOm). The folder uses **Anyone with the link → Viewer**. It contains `Buzz_0.5.23-nemo.14_aarch64.dmg`, `SHA256.txt` and `RELEASE-NOTES.txt`.

The installer is **191361056 bytes**, built from source `6dc631213f41010a3aa5ae8b92463f748f96d076`. Its SHA-256 is `3751a65fe876b3af7674a25f9150d0162e7e6c08984a9ec404113686a0eda9a2`. Run `shasum -a 256 Buzz_0.5.23-nemo.14_aarch64.dmg` in the download folder and compare the full result before opening it.

- [ ] Finish or checkpoint active agent work, preserve a recoverable previous app, install this version and verify the app version. Restart old agent processes at that safe checkpoint so they use the updated harness; installing the app alone does not update a process that is already running.
- [ ] Follow the workspace/team setup above. Confirm **Source**, **Saved revision** and **Running sessions**, then obtain each orchestrator's acknowledgement of its effective instruction source/revision. Future shared-policy edits on the updated harness apply between turns without another build; changed Team Instructions still need the affected agent process restarted.
- [ ] Record installation and actual uptake for each human in #1062. Keep reminders paused until actual Nemo execution starts.

The app and DMG passed Apple notarization, stapling and Gatekeeper checks. The mounted installer contains the expected version and matching executable. Focused UI, native editor, instruction, provider/session and core tests passed; owner/member screens were visually checked using fixtures. The desktop and ACP were rebuilt, with unchanged signed sidecars and the Codex bundle reused from `.13`. These are build and test results: installation on either user's Mac, live two-account policy propagation and scheduled reminder delivery remain explicit setup checks above. No live shared instructions were changed and no timers were activated during delivery.
