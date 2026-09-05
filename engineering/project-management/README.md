<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Nemo foundation remediation project

The [Nemo Foundation Remediation project](https://github.com/users/ivg-design/projects/8) tracks the architectural remediation program and the evidence-triaged issue backlog before implementation kickoff. GitHub issues, pull requests and their receipts remain canonical.

The board is owned by `ivg-design`, as approved after GitHub rejected creation under `mysteropodes`. Existing repository administrators `mysteropodes` and `pencilpark` have Project Admin access; repository contributor `byyourself22` has write access. GitHub prohibits linking a user project to a repository owned by a different user through the repository Projects tab. The board instead contains the canonical Nemo issues, uses `mysteropodes/nemo` as its default repository for new issues, and links directly to the repository; those issues link back to the project. The [existing feature roadmap](https://github.com/users/mysteropodes/projects/2) remains separate.

## Mirror on the repository-listed project

Since September 5, 2026 the same program is also carried by the repository-listed [Nemo Feature Roadmap](https://github.com/users/mysteropodes/projects/2) (owner `mysteropodes`), so it is reachable from the repository's Projects tab. Project 8 remains the origin board; the mirror was written from its field values and verified value-for-value.

| Origin field (project 8) | Mirror field (project 2) | Note |
|---|---|---|
| Status | **Remediation status** | Same seven options. The roadmap keeps its own `Status`, `Board Status` and `Category` for draft roadmap items. |
| Priority, Area, Goal, Size, Phase, Program, Kind, Planning window, Risk, Validation, Surface, Schedule basis | Same name | Identical options. |
| Work package, References, Validation owner | Same name | Text. |
| Start date, Target date, Triage date, Exception expiry | Same name | Date. |
| Estimate days | Same name | Number. |

All 58 open canonical issues are items on the mirror: the 43 remediation items carry forecast Start date and Target date, the 15 historical issues remain unscheduled. Saved views cannot be created through the API; a project admin adds a delivery board grouped by Remediation status and a roadmap on Start date / Target date in the UI. The mirror's readme records the same mapping.

## Contents

- [Project schema](project-schema.json): 33 total fields, including native fields, and ten saved active views.
- [Remediation task manifest](remediation-tasks.json): 43 issues covering F0–F6, R00–R22, six R18 migration families and BZ0–BZ6, with dependencies and acceptance.
- [Issue triage ledger](issue-triage.csv): all 262 pre-existing issues, their dispositions, rationale and public evidence.
- [Timeline](TIMELINE.md) and [schedule data](remediation-schedule.json): dated forecasts for all 43 remediation items.
- [Plan review](PLAN_REVIEW.md): sequencing decisions and kickoff prerequisites.
- [Publication receipt](PUBLICATION.md): live issue links and verified coverage.

## Using the board

The delivery board groups by Status and sorts by Priority. The Ready view sorts by Priority. The roadmap uses Start date and Target date. All ten saved views include `is:open`, so closed work leaves the active views while its GitHub history remains available.

Status means:

- **Inbox:** triaged or planned, awaiting a named owner and a complete execution scope.
- **Ready:** human owner, bounded scope, dependencies and observable acceptance are confirmed.
- **In progress:** a writer has acknowledged and owns the scope.
- **Review:** a concrete diff and its evidence exist.
- **Validate:** a specific runtime, platform, reproduction or acceptance check remains.
- **Blocked:** a dependency or missing input is named with an owner and next action.
- **Done:** acceptance is recorded against the integrated candidate/artifact.

Assignees represent accountable humans. Program identifies a proposed lead lane, not a staffing assignment. Validation owner names the acceptance reviewer when assigned. Detailed branch, worktree, agent/session, fixtures, scoped paths and receipts belong in the issue/PR body. Parent issues and blocked-by relationships carry hierarchy and dependencies; labels carry triage, type, area and priority across repository views.

The [Task timeline](https://github.com/users/ivg-design/projects/8/views/10) and Phase roadmap use forecast Start date and Target date values for all 43 remediation items. The user-selected start is September 4, 2026; the hour-level planning reference is 20:00 EDT. The first foundation increment targets September 5, working completion September 14, and the acceptance buffer ends September 18. Eight milestone dates reflect the same forecast. Schedule basis distinguishes Forecast, Committed and Actual. Original Planning window values preserve the proposal; the reviewed calendar schedule takes precedence. The 15 existing backlog issues remain unscheduled. Numerical effort estimates and assignments await capacity decisions. Triage date records the current audit; Created, Updated and Closed are native GitHub dates. Large tasks must be split before concurrent implementation. Approved exceptions require a named owner, reason and expiry.

## Triage standard

A `resolved` label alone is insufficient. This pass reviewed maintainer comments and the linked landed changes against current main `2e37f07d4565a72daad1212bf3e9cecd14641077`. It does not claim fresh runtime tests for historical fixes. Explicit incomplete implementation or missing acceptance stays open. Explicit maintainer decisions to close delivered scope and defer optional residual work remain closed unless later evidence contradicts that disposition.

The audit supports closing 145 tickets as completed and one non-actionable report as not planned, retaining 15 open tickets (eight implementation/scope items and seven validation items), and preserving the 101 previously closed tickets. The original French titles and discussions are preserved. Remaining open items receive current triage, priority, area and type labels; stale `pending`/`resolved` labels are reconciled with their actual disposition.

## Source and platform boundaries

The reviewed source is [proposal PR #887](https://github.com/mysteropodes/nemo/pull/887), commit `2ec97e2440708fae783babc4d9c9e777b520fb04`. Its published [project contract](https://github.com/mysteropodes/nemo/blob/2ec97e2440708fae783babc4d9c9e777b520fb04/engineering/remediation/07_GITHUB_PROJECT_AND_PARALLEL_WORK.md) is the basis for the board, extended with the metadata requested for this program.

Browser and packaged desktop acceptance are separate. Persistent document changes must cover save/load, undo/redo, selection, animation, rendering, export and applicable native bridges. The Buzz release and final live collaboration acceptance remain with their owning session; creating these packets does not restart or duplicate that implementation work.

GitHub's [Projects guide](https://docs.github.com/en/issues/planning-and-tracking-with-projects/learning-about-projects/quickstart-for-projects) and [Projects API reference](https://docs.github.com/en/graphql/reference/projects) describe the native fields and saved view mechanisms used here.
