<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Local validation and explicitly requested hosted runs

**Current policy (2026-09-06): all builds and validation run locally by default.**
Commits, pushes, PR updates, merges, and version tags must not trigger GitHub Actions
builds. A routine request to implement, test, open a PR, merge, deploy, or release does
not authorize a hosted build. Agents must not enable, dispatch, rerun, or introduce
automatic build workflows without an explicit human request for that specific hosted run.

All four workflows (`nemo-validation`, `deploy-web`, `deploy-feedback-worker`, and
`release`) expose only `workflow_dispatch`. Every job also requires the boolean input
`allow_hosted_build: true`, which defaults to false. This is an execution guard; the
human's request must already exist before an agent sets it. No scheduled, push, PR,
tag, or chained workflow trigger is permitted. Build and publish locally unless the
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
invoked, including for documentation changes; opening or updating a PR runs no workflow.
Only explicit Markdown documentation paths (including `scripts/nemo/README.md`), boundary
policy JSON, the isolated `engineering/boundaries/profiles/scripts-nemo.fixture/` subtree,
the CI workflow/runner, and boundary checker/tests are exempt from runtime jobs. See `applicability()` in
[ci.cjs](../../scripts/nemo/ci.cjs) for the exact allowlist. Runtime launcher changes,
package/lockfiles, application source, native/WASM source, application tests, and unknown
paths conservatively require **all** runtime jobs. Renames are considered as deletion
plus addition, so moving application code into a docs path cannot hide its former surface.

When applicable, the runner invokes the established verifier with `test:integration`,
`test:browser`, `test:rust-tauri`, `build:wasm`, `build:desktop`, and `test:desktop`.
The current dependency/suite gaps deliberately prevent a green aggregate:

- Document-contract suites are not yet defined (R12/R13).
- Playwright and the browser acceptance suite are not yet installed/defined (R03/R07).
- Packaged desktop test harness and installed-artifact acceptance remain open (R06/R21).
- Native builds require toolchain/dependencies and usable sidecar packaging. R04's unsigned
  desktop build/sidecar correction is separate work. This workflow does not supply signing,
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
  enforcement remain the R05 adoption/integration gate.

There are no placeholder green browser/native jobs and no label-based bypass. A green
tooling/docs PR reports runtime jobs as **not applicable**, not tested or accepted.
Application-affecting PRs will remain blocked until the required harnesses and build
prerequisites land. Full R07 acceptance still needs actual browser and packaged-native
receipts on their supported environments; CPU tests cannot supply that evidence.

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

[PR #945](https://github.com/mysteropodes/nemo/pull/945) is a separate test-fixture
prerequisite for Node executable paths containing spaces. Until integrated, those existing
fixtures can fail on affected hosts; this CI runner does not mask or patch them.
