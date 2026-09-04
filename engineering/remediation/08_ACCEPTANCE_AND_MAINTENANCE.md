<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Acceptance, evidence and maintenance

## Evidence vocabulary

- **Observed:** directly reproduced on the named source/runtime with retained evidence.
- **Implemented:** source contains the behavior; end-to-end acceptance may still be missing.
- **Historical:** earlier documentation/receipt says it worked; revalidate before relying on it.
- **Proposed:** design or plan that has not been adopted or delivered.
- **Blocked:** required validation could not run for an exact reason.
- **Unknown/not-run:** not checked; no inference from another surface.
- **Accepted:** designated maintainer accepts the required behavior on identified bytes.

Every health claim identifies source/base/candidate SHA, environment/platform, command or
interaction, fixture/version/seed, result, date and limitations.

## Per-packet acceptance

Use [the handoff receipt](templates/HANDOFF_RECEIPT.md). Minimum gates:

1. Intended behavior and forbidden regression are explicit.
2. Changed paths stay inside the acknowledged grant.
3. Module/API/schema and state-authority changes are declared.
4. Fast static and behavior checks run on the candidate.
5. Persistent changes cover create/edit/key/undo/redo/save/reload and every relevant consumer.
6. UI/API/SDK/MCP parity is checked where the surface exists.
7. Browser and packaged desktop are treated as separate acceptance surfaces.
8. Native media/plugin/export claims use the actual packaged artifact.
9. Performance changes preserve output and compare controlled workloads.
10. Limitations, blocked/not-run scope, reviewer and rollback are visible.

## Foundation close gate

Remediation is complete only when:

- every actionable shipped surface is mapped to a capability and handler or has an explicit
  unavailable/owned exception;
- each migrated aggregate has one writable authority and no undocumented bypass;
- module dependencies, profiles and exception expiry are enforced;
- supported documents preserve IDs, fields, assets and unknown-data policy across migrations;
- the direct application API, UI, SDK and MCP agree for declared shared behavior;
- real browser, desktop, media/export and supported-platform evidence is current;
- representative performance, soak, cancellation, crash and recovery gates pass;
- a real OpenFX subset and color/precision boundaries are fixture-backed;
- clean-clone Codex and Claude sessions find the same adopted instructions and can complete
  one bounded task through NEMO-A2A-1;
- handbook, ADRs, support matrix and release path match the integrated source;
- the maintainer accepts the identified candidate/artifacts.

A line-count reduction, generated schema, passing unit suite, merged PR, relay acknowledgement
or agent completion message alone is not this gate.

## Required maintenance cadence

| Trigger | Update |
|---|---|
| task start | fetch/read current main, issues/PRs/claims and relevant policies |
| branch/worktree/base change | re-evaluate scope, contract versions and prior evidence |
| new feature/type/field/dependency | update architecture, capability inventory, fixtures and affected consumers |
| bug reproduction/test result | retain exact evidence and failure scope |
| accepted decision | add/update ADR and supersede old guidance explicitly |
| policy/protocol change | version all affected instructions, schemas and client adapters together |
| before handoff | receipt with state, results, uncertainty and exact next action |
| before integration | validate the combined candidate against current main |
| before release | validate built bytes, support matrix, native/media/plugin paths and recovery |
| periodic active review | triage stale claims/exceptions, dependencies, CI flakes, docs drift and benchmark baselines |

Do not schedule activity merely to generate reports. Review when the project is active and
when evidence can affect a decision.

## Document ownership and updates

- One writer owns a shared policy/manifest in each integration window.
- Other agents submit per-task receipts; the owner reconciles rather than overwriting.
- Normative rules are concise. Detailed logs/traces live in approved artifacts and are linked.
- Architecture or behavior changes update the relevant document in the same PR.
- Mark old facts superseded; do not delete the only failure evidence.
- Keep current source facts separate from proposals and future product scope.
- Revalidate public links and package-local links after moves/renames.
- Increment the package version for contract changes and record migration notes.
- Validate clean-clone Codex and Claude loading whenever instruction topology changes.

## Metrics for active review

Track measures that drive decisions:

- delivery: review wait, blocked age, integration conflicts, reopened regressions;
- validation: time to first useful failure, gate time, flake/blocked rate;
- architecture: cycles, forbidden edges, new globals, oversized debt and exception expiry;
- product performance: p95/p99 evaluation/presentation, dropped frames, GPU/decode/upload,
  memory plateau and export throughput;
- agent operations: duplicate investigations, abandoned claims, rework after handoff and
  cost per accepted outcome where available.

Do not rank people by lines of code, generated commits or agent completion messages.
