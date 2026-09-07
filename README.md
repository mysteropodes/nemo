# Nemo branch archive

This branch stores reversible Git bundles and a name-to-commit audit. It is intentionally
independent of `main` and contains no application checkout or GitHub Actions workflows.
Do not merge it into `main`. Its active GitHub rules block force pushes and deletion.
Add later snapshots through ordinary fast-forward commits; never overwrite an earlier bundle.

## Retirement completed: 2026-09-07

The [retirement receipt](retirements/2026-09-07.json) records the verified deletion of
**168 remote and 259 local branch refs**, after fresh ownership and exact-SHA checks.
GitHub now has **60 branch heads including archive**; the original local repository has
**120 retained branches**. All 23 open PR heads/bases, existing worktrees, release refs
and unresolved work were preserved. The bundle below remains unchanged and restores every
retired tip exactly. Snapshot counts describe the earlier preparation census.

## Snapshot: 2026-09-07

The [audit](snapshots/2026-09-07/audit.json) classifies all **227 remote heads and 379 local
branches** observed during this audit. The [tab-separated index](snapshots/2026-09-07/branches.tsv)
is convenient for searching. **168 remote and 259 local branch refs** are retirement
candidates, preserving **190 distinct tips** in one self-contained 67,634,181-byte bundle.
There are **59 retained remote and 120 retained local refs** in this snapshot. The archive
branch itself was created afterward. A branch may appear in both scopes with different SHAs.

A candidate is backed by ancestry in main, an integrated merged PR, or an identical complete
Git tree at an integrated commit. Distinct original commit histories are still preserved.
Dates alone were not used to decide retirement. Main, release/protected refs, registered
worktrees, open PR heads/bases and unresolved work are retained. **This snapshot records
preparation: it does not itself assert that any candidate branch has been deleted.**

The audited main is `66ece0641708122eb8447e85ad8dd7e3402aaf6c`. Fresh ownership, PR, worktree,
protection and exact-SHA checks are required immediately before deleting any candidate.
If a ref changed, acquired an owner, or became an open PR dependency, retain it and audit again.

## Verify everything

Use an independent checkout of this branch:

```bash
git clone --branch archive --single-branch https://github.com/mysteropodes/nemo.git nemo-archive
cd nemo-archive
python3 verify.py
```

The verifier checks every artifact checksum, verifies the bundle in an empty repository,
fetches every archived ref, compares all 427 original tip SHAs, and runs full Git integrity
validation. It uses no existing Nemo objects or object alternates. No external Git history
is required. The machine-readable [manifest](snapshots/2026-09-07/manifest.json) contains the
bundle digest, size, scope and preservation limits.

## Restore one branch

Look up the exact `scope`, `name`, `sha` and `archive_ref` in the index first. The following
example restores one archived remote branch under a new local recovery name:

```bash
git init recovered-nemo
cd recovered-nemo
git fetch ../snapshots/2026-09-07/retired-branches.bundle \
  refs/archive/remote/codex/weekend-final-handoff-20260907:refs/heads/recovered-weekend-handoff
git switch recovered-weekend-handoff
git rev-parse HEAD
```

Run this example from `nemo-archive` so the relative bundle path resolves. Compare the printed
commit with the index before using it. Replace the example archive ref with the selected
entry; `local` entries preserve the original local tip, and `remote` entries preserve the
original GitHub tip. To restore an original name on GitHub, first ensure that name is still
absent and authorized, then push the verified recovered commit to that exact branch name
without force. Never overwrite a branch another agent has recreated.

## Preservation limits

Bundles preserve committed Git objects and named tips, including histories replaced by
rebases or squash merges. They do not preserve uncommitted or ignored files, application
state, reflog-only objects, or external Git LFS payloads. Existing worktrees, tags and earlier
non-Git cleanup backups were left intact. Private machine paths and raw authenticated API
responses are deliberately excluded from this archive's index and documentation.

The archive is a recovery mechanism, not a task queue. Active task progress belongs on the
project board and its linked issues/PRs. Future snapshots should contain newly retired refs
only; avoid repeatedly embedding the same entire repository history in separate per-branch
bundles. Keep an independent backup because this branch remains in the same GitHub repository.
