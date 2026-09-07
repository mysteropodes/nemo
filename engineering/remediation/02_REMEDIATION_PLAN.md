<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Dependency-ordered remediation plan

Status: **proposed execution plan**. R00-R22 are work-package identifiers, not existing issue
numbers. Resolve current `main`, open work and owner assignments at kickoff.

## Operating model

Use three lead programs when capacity is available:

| Lead | Program | Typical independent lanes |
|---|---|---|
| A: core/integration | sole ownership issuer, shared contracts, state-authority transitions, integration candidate | document/history, commands/evaluation, consumer audit |
| B: editor/capabilities | UI/tool/timeline extraction and capability adoption | feature modules, UI/API parity, interaction regression |
| C: platform/quality | test/diagnostic infrastructure, Buzz, native media/plugins, builds | fixtures/CI, OpenFX, security/runtime/benchmark review |

Each lead may use bounded subagents, but every actual writer receives its own non-overlapping
scope and worktree. One coordinator sequences shared contracts and integration. Agent count
does not grant repository, deployment or release authority.

Planning target: a first integrated foundation increment within 24 elapsed hours and the
existing-scope remediation within 7-14 calendar days, with days 11-14 reserved for defects and
acceptance reruns. These are planning targets. Gates are never waived to meet them.

## Immediate kickoff

Begin with **R00/BZ0**: collect every active developer's task, branch, head, changed/planned
paths, contracts, agent sessions, unpushed work and next checkpoint. Name the human integration
owner and backup. Reconcile overlaps, select the fresh base and issue the first three bounded
packets for baseline, surface inventory and portable workflow. Do not begin broad monolith
extraction before this shared inventory.

Normalize the existing Buzz pilot through these collaboration packets:

| Packet | Outcome |
|---|---|
| BZ0 | adopted authority, identity, scope and task-continuity contract |
| BZ1 | authenticated desktop onboarding and self-service owner-backed agents |
| BZ2 | one Nemo Project bound to the canonical GitHub repository and local checkout |
| BZ3 | typed A2A grants, lifecycle, reconnect, handoff and revocation acceptance |
| BZ4 | idempotent GitHub issue/PR/run links with GitHub canonical |
| BZ5 | CI and Nemo product-MCP diagnostic receipts as their capabilities land |
| BZ6 | backup/restore, offboarding, upgrades, monitoring and support ownership |

Retain accepted pilot behavior, but revalidate it on the final distributed builds. Do not
describe a source-complete packet as deployed until its runtime gate passes.

## Phase sequence

### Current execution order — September 6, 2026

The accepted F0 comparison is PR #983, merge `cf22365f909069e232a6b6bf992a272e4bd96ff3`.
Preserve its known native timing failure and unrun/blocked platform cases. Reuse
unchanged evidence; repeat checks only for changed inputs, concrete defects or a
required acceptance gate. Unrelated feature development is paused.

Refactoring monolithic files, modular application services, permanent unit/regression
tests and the bundled plug-and-play Rust MCP are one production task. First integrate
the retained R08 extraction and compatibility work (PR #986, selected Node runner),
then deliver layer Motion opacity through R09/R11/R12/R13/R14 together: one command
and history authority, UI/timeline, persistence, render/export, minimal replay and the
compiled Rust adapter. Codexitron owns integration and the MCP implementation;
Codeximator owns the bounded application-handler extraction. Develop adapter and
packaging alongside handlers once their shared contract is agreed.

Each extraction requires its concrete R03 characterization, applicable R05 size and
dependency enforcement, and R06 resource isolation. Whole-project R03/R05/R06 closure
is not a prerequisite to starting it. New/migrated modules obey real limits; existing
legacy ceilings remain explicit. All remaining obligations stay open.

Add R15's gesture/job shape, then migrate R18's subsystem families in dependency order.
Complete R03 and R05 coverage alongside each family. R10 starts at the native host/image
contract; R16 consumes that proof and the shared property/job services; R17 starts at
the image/color/time contracts. Accumulate R19/R20/R21 evidence during migration and
finish R22 acceptance. The phase gates below remain final acceptance obligations.
Installed bundled executable and actual Codex/Claude discovery, editing, history,
persistence, render/export, reconnect, cancellation and stale-write behavior are
required for the property/MCP slice. Workflows remain manual-only.

### F0 — current reproducible baseline

**R00** Resolve current main, active PRs, dirty work, owners and supported platforms.<br>
**R02** Add source/build identity, `doctor`, named commands and structured receipts.<br>
**R03** Inventory actionable UI/API surfaces and their save/undo/render/export consumers;
build deterministic fixtures and initial performance workloads.<br>
**R04** Repair native/sidecar packaging blockers if they reproduce on the selected base.

Gate: a fresh checkout runs CPU checks, reports unavailable native capabilities as blocked,
and produces a versioned baseline without changing user projects.

### F1 — portable rules and enforceable boundaries

**R01** Adopt the shared handbook, short Codex/Claude entry points, task/receipt templates,
Buzz coordination contract and GitHub Project setup.<br>
**R05** Profile every source file; install dependency, cycle, global-state, size and exception
checks.<br>
**R06** Add task-isolated launcher/runtime roots, ports, browser contexts, build outputs and
report paths.<br>
**R07** Add real PR gates and then configure required checks after their names and behavior
have passed on a candidate.

Gate: deliberate size/import/schema violations fail; two writers cannot overwrite each
other's source, state or artifacts; a clean clone finds the same team instructions.

### F2 — contracts, first extraction and diagnostics

**R08** Extract one pure animation/easing seam through a compatibility facade. Compare Node
and Vitest using the same unit, stateful behavior and browser control; record the runner ADR.<br>
**R09** Decide writable state ownership, stable IDs, timebase, persistence, image/color,
platform capability and dependency direction. Generate boundary types/schemas.<br>
**R10** Prove a disposable native OpenFX load/describe/CPU-float-render path.<br>
**R11** Add commands, queries, transactions, gesture lifecycle, history, revision checks,
idempotency, cancellation and per-aggregate authority transitions.<br>
**R12** Add structured diagnostics plus deterministic fixture record/replay.

Gate: one migrated aggregate has one writer, one gesture has one undo action, stale writes fail,
replay matches an independent expectation, and a real application caller uses the extracted code.

### F3 — complete slice, Rust MCP and real native effect

**R13** Migrate one animated property across inspector, timeline, SDK, save/reopen, undo and
render/export.<br>
**R14** Add the thin Rust MCP adapter over the same application services; prove Codex and
Claude discovery, direct-API parity, reconnect, cancellation and stale-write behavior.<br>
**R15** Prove a geometric gesture or asynchronous export job with progress and cancellation.<br>
**R16** Integrate one redistributable OpenFX effect through the registry, UI and MCP.<br>
**R17** Prove color transforms, half/float image handling, EXR fixtures and an explicit OTIO
subset/loss report.

Gate: UI, SDK, direct tests and both MCP clients reach the same accepted behavior. A real
plugin survives keying, undo, save/reload, absence, cancellation and host fault. Browser/native
capability differences are explicit.

### F4 — all shipped subsystem migrations

**R18** Split into bounded issues and migrate by dependency:

1. document IDs, codecs, recovery, components and history;
2. time, tracks, easing, expressions, parenting and timeline model;
3. render graph, masks/effects, color resources, cache and scheduler;
4. drawing, selection, gestures, inspector, graph/timeline, text, rig and mesh;
5. media, import/export, OpenFX/extensions and external-service adapters;
6. preferences, shortcuts, contextual/dynamic controls, diagnostics and Labs disposition.

Each child follows: claim → characterize → extract behind facade → route every consumer →
prove UI/API/MCP/persistence/render behavior → integrate → remove the old writer.

Gate per family: every shipped surface is mapped and migrated or has an owned, dated,
maintainer-approved exception. Generated coverage cannot substitute for behavior evidence.

### F5 — performance, resilience and distribution

**R19** Set workload budgets from measured product fixtures; run latency, throughput, memory
plateau, cache, export and soak regressions.<br>
**R20** Exercise interrupted save, old/damaged files, missing assets/plugins, worker/host crash,
device loss, stale sessions, disconnects, disk failure and recovery.<br>
**R21** Validate generated WASM, sidecars, plugin load, media export/reimport, installed desktop
artifacts and each supported OS.

Gate: agreed budgets and recovery cases pass on identified bytes. Missing platform evidence
blocks only the corresponding support claim, and remains visible.

### F6 — close foundation remediation

**R22** Reconcile the surface inventory, retire obsolete globals/adapters, validate clean-clone
setup, rehearse a task with fresh Codex and Claude sessions, and obtain maintainer acceptance.

Gate: all shipped actionable surfaces are classified; persistent data survives every required
consumer; no unowned bypass remains; browser and packaged desktop evidence is current.

## Parallel work during the program

Unrelated feature work is paused during this production sequence. Reserve the exact shared scaffold, contract or whole
legacy file under active extraction. Typical reservation windows are 1-4 hours for shared
configuration and 2-6 hours for a legacy extraction, with review at checkpoints. A timebox is
not automatic reassignment.

Resume unrelated work only after the production owner releases that pause, under four rules:

- untouched legacy area: scoped feature/fix plus regression fixture, with no added global debt;
- reserved area: coordinate with its owner or work on independent fixtures/leaves;
- migrated area: use its public API and module/capability contract;
- cross-cutting schema/build change: integration owner versions and sequences it.

Pause bulk formatting, mass renames and competing framework/core rewrites during remediation.
Integrate coherent packets every 2-4 hours when practical. Clear review/fix backlog before
expanding a team's work in progress.
