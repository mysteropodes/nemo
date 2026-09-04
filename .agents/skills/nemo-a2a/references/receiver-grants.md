> Dedicated Nemo workspace: skill 1.3.0 is the current operating contract.
> Manual pins, Project assignment, and explicit-grant steps below describe legacy/general
> mode and infrastructure tests, not normal Nemo onboarding or task prerequisites.

# Receiver-local checkout grants

Each managed agent receives an operator-controlled grant document through the configured
Buzz grant source. The default file is `.buzz/agent-job-grants.json`; keep it local and
untracked because `checkout_root` is machine-specific. Its normative shape is
[agent-job-grants.schema.json](agent-job-grants.schema.json).

Every grant binds one exact Project/repository/checkout coordinate:

```json
{
  "version": 1,
  "grants": [
    {
      "project_address": "30621:<project-owner-pubkey>:nemo",
      "home_channel": "<canonical-channel-uuid>",
      "repository": "https://github.com/mysteropodes/nemo",
      "requester_pubkeys": ["<authorized-requester-pubkey>"],
      "capabilities": ["nemo.a2a.implementation"],
      "path_prefixes": ["src", "src-tauri"],
      "base_sha": "<lowercase-commit-sha>",
      "branch": "codex/example-task",
      "worktree_id": "example-task",
      "checkout_root": "/absolute/path/to/receiver/checkout"
    }
  ]
}
```

`path_prefixes` is required and nonempty; an empty list never means the whole repository.
`base_sha`, `branch`, and `worktree_id` are scalar exact matches. Broad arrays of accepted
branches, SHAs, or worktree IDs are invalid. `checkout_root` is an absolute existing path
that remains receiver-local and never enters a signed event or model prompt.

Loading a grant canonicalizes its checkout path. Every admission then runs fail-closed Git
checks: `rev-parse --show-toplevel` must equal that canonical root, `origin` must normalize
to the exact GitHub repository, the symbolic branch must equal `branch`, and `HEAD` must
equal `base_sha`. A detached head, changed remote, moved branch, new commit, unavailable
checkout, Git warning/error, or multiple matching grants denies admission.

Both grant prefixes and request paths must be normalized repository-relative paths. Reject
absolute paths, backslashes, empty, `.` or `..` segments, symlink escape, and every
case-variant of a `.git` segment such as `.GiT/config`. Update a grant deliberately when a
checkout advances; do not weaken it to make a stale request pass.
