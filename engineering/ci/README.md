<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Local validation and explicitly requested hosted runs

Implementation scope reviewed on **2026-09-07** at
`66ece0641708122eb8447e85ad8dd7e3402aaf6c`. The
[execution checklist](../remediation/EXECUTION_PLAN.en.md) governs current remediation
leaves and known-defect disposition. The local/hosted policy below remains in force;
updating this reference enables no workflow and claims no fresh runtime acceptance.

**Current policy (2026-09-06): all builds and validation run locally by default.**
Commits, pushes, PR updates, merges, and version tags must not trigger GitHub Actions
builds. A routine request to implement, test, open a PR, merge, deploy, or release does
not authorize a hosted build. Agents must not enable, dispatch, rerun, or introduce
automatic build workflows without an explicit human request for that specific hosted run.

All four workflows (`nemo-validation`, `deploy-web`, `deploy-feedback-worker`, and
`release`) expose only `workflow_dispatch`. Every job also requires the boolean input
`allow_hosted_build: true`, which defaults to false. This is an execution guard; the
human's request must already exist before an agent sets it. No scheduled, push, PR,
tag, or chained **build/test/deploy/release** workflow trigger is permitted. The sole
automatic exception is the metadata-only collaborator PR policy described below. Build and publish locally unless the
requested exception explicitly covers hosted execution and any deployment/release effect.

The workflows were disabled in repository settings as immediate containment. Keep them
disabled until the manual-only definitions are merged and a specific hosted run is
requested. Before enabling one, verify both the default branch and the selected ref
contain the manual-only definition; older branches/tags can retain automatic triggers.
Enable only the needed workflow for the authorized run, then disable it again afterward.
Never dispatch a hosted run simply to test these trigger changes.

Attach local command receipts and exact source/base SHAs to PRs. Preserve required PR
review; do not add an unattended hosted-build requirement to branch protection or bypass
existing protection to compensate for a disabled check. This changes execution policy,
not [R07](https://github.com/mysteropodes/nemo/issues/903)'s remaining runtime acceptance.
The optional workflow's aggregate retains the name **`Nemo / required`** for compatibility.

| Lane in an explicitly requested validation run | Local invocation | Success criterion |
|---|---|---|
| `quick` / Nemo / local quick | `node scripts/nemo/ci.cjs quick` | Existing `verify.cjs --jobs doctor,check,test:unit,test:rust --json`; every selected job passes |
| `boundaries` / Nemo / boundaries | `node scripts/nemo/ci.cjs boundaries` | Repository discovery, adopted profiles, coverage checks and protected-base ratchets all pass |
| `surfaces` / Nemo / affected surfaces | `node scripts/nemo/ci.cjs surfaces` | Explicit applicability decision, then every applicable runtime job passes |
| `aggregate` / Nemo / required | `node scripts/nemo/ci.cjs aggregate` | All three named workflow lanes return exactly `success` |

`boundaries` and `surfaces` require `NEMO_CI_BASE_SHA`, the full 40-character reviewed
protected-base commit SHA. Locally, fetch the protected branch, select its reviewed SHA
and set that variable; do not substitute an arbitrary contributor revision. For an
explicitly requested hosted run, provide the same SHA as the required `base_sha` input.
The workflow checks out the selected dispatch ref, with full history where base content
is needed; it does not synthesize a PR merge candidate. Use a reviewed integration ref
when merge-result validation is required. Concurrency is scoped to the selected ref.
Local uncommitted edits are included by quick/boundary source checks, but surface
selection compares **committed** base and HEAD, just as the workflow does.

`aggregate` consumes GitHub's `toJSON(needs)` through `NEMO_CI_NEEDS`. The expected lane
list is fixed in the runner, not inferred from the received results. Missing, cancelled,
failed, skipped, blocked, unavailable and unknown results all fail. The job uses
[`if: always()` with explicit dependencies](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idneeds)
plus the manual-event/approval guard, so a failed prerequisite does not silently skip
an authorized aggregation. Without approval, every job including the aggregate is skipped.
Cancelling the aggregate
itself cannot produce success. No workflow path filters or `continue-on-error` are used.

The existing local registry permits some optional jobs to return `blocked`/`not-run`
with an overall zero exit. CI independently requires exactly one `pass` receipt entry
per selected job, a passing summary, and a zero process exit. Missing or duplicate
entries, invalid JSON/schema, contradictory exits and unexpected jobs fail. An absent
runner or killed process cannot pass. This does not change local optional-job semantics.

## Applicability and remaining acceptance

Quick and the adopted boundary profile run whenever the validation lanes are explicitly
invoked, including for documentation changes; opening or updating a PR runs no build or validation workflow.
Only explicit Markdown documentation paths (including `scripts/nemo/README.md`), boundary
policy JSON, the isolated `engineering/boundaries/profiles/scripts-nemo.fixture/` subtree,
the CI workflow/runner, and boundary checker/tests are exempt from runtime jobs. See `applicability()` in
[ci.cjs](../../scripts/nemo/ci.cjs) for the exact allowlist. Runtime launcher changes,
package/lockfiles, application source, native/WASM source, application tests, and unknown
paths conservatively require **all** runtime jobs. Renames are considered as deletion
plus addition, so moving application code into a docs path cannot hide its former surface.

When applicable, the runner invokes the established verifier with `test:integration`,
`test:browser`, `test:rust-tauri`, `build:wasm`, `build:desktop`, and `test:desktop`.
Source already contains browser, integration and packaged-native harnesses. Each selected
job still needs its real environment and artifact, and a harness's existence does not
establish full feature or platform acceptance:

- `tests/integration/r06-browser-runtime.test.cjs` is the current integration-directory
  suite. It does not by itself cover every document, history or persistence contract.
  The opacity application tests have separate unit/browser coverage.
- `@playwright/test` is declared in `package.json`, and `tests/browser` contains opacity
  consumers, preview lifecycle and isolation specs. Dependency/browser installation and
  actual supported graphics behavior must still be verified on the selected environment.
- `tests/desktop/native-harness.cjs` and `packaged-native.test.cjs` exist. The desktop job
  requires a real package and rejects empty/skipped tests. Process/storage isolation is
  separate from installed UI, save/reload, render/export and MCP-client acceptance.
- Native builds require toolchain/dependencies and usable sidecar packaging. The existing
  isolated builder preserves an unsigned package and runs FFmpeg dylib finalization; its
  source is not a release receipt. This workflow does not supply signing,
  updater, notarization, deployment, or feedback credentials.
- WASM builds require `wasm-pack` and its target. The current local job builds geometry only;
  vectorize rebuild/parity and GPU acceptance remain outside that job's success claim.
- Repository discovery records all tracked and nonignored untracked Git candidates,
  independent of language or profile selection. Missing tracked files, unresolved index
  conflicts, unsupported entries and unreadable content fail the boundary lane. This is
  an inventory and integrity check; it does not classify every candidate into an
  architectural profile or take an atomic filesystem snapshot.
- The adopted tooling profile and its source coverage run alongside application size,
  exclusion provenance and protected-base ratchets. Application size checks count UTF-8
  text without applying a JavaScript lexer, including when called for Rust, WGSL, CSS,
  Python, shell or HTML source. Language-neutral counting does not add dependency-graph
  coverage for those languages. Full source classification and application architecture
  enforcement remain the named P10/P11 and related leaf outcomes in the current checklist,
  tracked under R05; whole-parent closure is not a global extraction prerequisite.

The selected CI jobs do not include the `nemo-mcp` crate's Cargo suite or the new coverage
and feature-registration gates. Run the crate's applicable tests explicitly and implement
the missing gate wiring through its named leaves; a green aggregate cannot stand in for
checks it never selected.

There are no placeholder green browser/native jobs and no label-based bypass. A green
tooling/docs PR reports runtime jobs as **not applicable**, not tested or accepted.
Application-affecting candidates need the selected job receipts and actual prerequisites;
do not assume they are all blocked because earlier documentation predates the harnesses.
Known baseline failures remain explicit comparison evidence, not concealed passes or an
instruction to fix unrelated features first. Runtime acceptance still needs actual browser
and packaged-native receipts on supported environments; CPU tests cannot supply them.

## Collaborator merge-policy automation

Ilya and Cyrill approved collaborator self-merge on **9 September 2026**. The sole automatic
Actions exception is [.github/workflows/collaborator-pr-policy.yml](../../.github/workflows/collaborator-pr-policy.yml).
It reads PR/review metadata and live author permissions, then acknowledges policy eligibility
with an explicitly automated approval at the current head. It never checks out or executes
PR code, builds, tests, publishes artifacts, merges PRs or dismisses a human review. The
script is loaded from the trusted base SHA; manual dispatch is restricted to `main`.
The pinned official action receives only contents-read and pull-requests-write permissions.

Keep the native one-review requirement, latest-push approval, stale-review dismissal and
conversation resolution. Only current human authors with write/maintain/admin permission
qualify. External/read/triage authors still need a collaborator's GitHub review. Each team
records its own technical review and local acceptance before merging; the bot's policy
acknowledgement cannot substitute for that evidence. Human change requests remain blocking.
GitHub's existing administrator emergency bypass is not the ordinary merge route.

An admin enables “Allow GitHub Actions to create and approve pull requests” while keeping
the default workflow token read-only. The four product workflows stay disabled. PR metadata
events reconcile eligible heads automatically. To recover a failed/missed event, run:

```sh
gh workflow run collaborator-pr-policy.yml --repo mysteropodes/nemo --ref main -f pull_request=1077
# Omit the PR input for a one-time reconciliation of every open main PR.
```

This policy-only dispatch is covered by the agreed workflow, not permission to run product
CI. Inspect failed/ambiguous writes before retrying; a matching bot approval at the same SHA
is reused. A newer push needs fresh review/validation and policy evaluation. Avoid running
an all-PR reconciliation concurrently with individual manual retries.

**Access changes:** after granting/removing collaborator write access, the repository admin
must immediately dispatch an all-PR reconciliation and verify its result. It removes only
this policy's marked bot approvals from ineligible PRs. GitHub does not notify this workflow
of membership changes; an earlier policy approval is not continuously re-evaluated. Do not
merge affected PRs during access reconciliation. Scheduled coordination reminders are
unrelated and remain paused outside active remediation sessions.

Run policy regressions locally with
`node --test .github/scripts/collaborator-pr-policy.test.cjs`. These check the authorization
boundary, changed heads, access loss, human review objections and ambiguous retries.

## Protected base and PR trust

The runner materializes
`engineering/boundaries/profiles/scripts-nemo.profile.json` directly with `git show`
from the reviewed base SHA into a unique temporary directory. It passes that absolute file
to the existing `boundaries.cjs --baseline` CLI against the candidate profile/root.
It records the base SHA, source path and content SHA-256. Missing commit/profile or
malformed baseline fails closed. Candidate `scripts-nemo.baseline.json` is never used
as the trusted prior policy. Temporary materialization is removed after the checker runs.

The optional workflow uses `workflow_dispatch`, a read-only contents token, nonpersistent checkout
credentials and disposable GitHub-hosted runners. No personal/self-hosted runner,
`pull_request_target`, settings API, privileged follow-up workflow, cache reuse, or
secret injection is involved. macOS arm64 matches the only committed FFmpeg sidecar;
Linux runs the pure boundary/aggregate checks. Release/deploy workflows follow the same
manual-only execution rule and require authority for their publishing effects.
Workflow/checker changes remain reviewable candidate code;
base provenance does not make that code immutable or replace required maintainer review.

## Evidence

Each completed lane writes `reports/ci/<lane>.json`; the established verifier also writes
its normal source/build/platform receipt and logs. GitHub uploads a separate artifact for
each lane even on failure, and missing artifacts fail their upload step. A setup error,
runner crash or cancellation may leave no receipt; the aggregate still rejects the lane.
No source-build identity is inferred from historical receipts.

Run focused regressions with `node --test tests/nemo-ci.test.cjs`; the wrapper includes
them in `npm test` and `npm run verify`. Tests exercise real CLI invocation, a zero-exit
blocked verifier, missing/duplicate receipts, all nonsuccess aggregate states, conservative
selection and a Git fixture where candidate policy differs from the protected base.

Historical fixture correction: [PR #945](https://github.com/mysteropodes/nemo/pull/945)
merged as `1d652dd5de550bf6f5863faf5ed89c2481a36412` on 2026-09-05 and is contained in the
reviewed main. It is no longer an outstanding prerequisite for Node executable paths
containing spaces. Preserve its regression coverage and diagnose any fresh failure on the
actual candidate; this source review is not a rerun of those fixtures.
