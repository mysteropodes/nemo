> Dedicated Nemo workspace: skill 1.3.0 is the current operating contract.
> Manual pins, Project assignment, and explicit-grant steps below describe legacy/general
> mode and infrastructure tests, not normal Nemo onboarding or task prerequisites.

# Repository adoption

The tracked package is canonical at `.agents/skills/nemo-a2a/`. Codex discovers repository
skills from `.agents/skills`; the sanitized tracked root `AGENTS.md` points agents to that
canonical package without copying private reference material. Claude loads the small
`.claude/skills/nemo-a2a/SKILL.md` shim, which also points to the same package, and tracked
`CLAUDE.md` imports `@AGENTS.md` before its existing engineering guidance.

`check_package.py` parses those imports and links as a clean clone would. It also requires the
golden-rule block to be byte-identical across the root entry point, canonical skill, and
Claude shim. Never replace the sanitized root file with a machine-local reference copy.

No global skill copy, symlink outside the repository, plugin installation, identity
enrollment, or hosted configuration is required. A clone receives the same protocol and
tests from Git.

Buzz managed agents also load this package automatically for the Nemo Project. The tracked
`.agents/buzz-preload.json` manifest opts the repository in once for every Codex and Claude
agent. Before creating a Project session, Buzz matches the Project's authoritative repository
announcement to the checkout's real Git `origin`, starts the session in that verified checkout,
and injects the complete declared skill into its standing instructions. A checkout with a
different origin is ignored, and a present malformed or cross-repository manifest fails closed.
The operator only needs to make the Nemo checkout available in Buzz's project repository root;
there is no per-agent skill setting. Restart an existing managed agent after adding or updating
the manifest so its next Project session receives the current skill.

Each receiver still needs its own untracked `.buzz/agent-job-grants.json`, because the
absolute checkout root is local to that machine. Build it from
[receiver-grants.md](receiver-grants.md) and validate the exact shape against
[agent-job-grants.schema.json](agent-job-grants.schema.json). Update the scalar base SHA,
branch, and worktree ID when the checkout deliberately advances; never commit this file.

After integration, verify adoption in fresh Codex and Claude sessions from the repository
root:

1. Ask which skill governs Nemo agent delegation and require both sessions to name
   `.agents/skills/nemo-a2a/SKILL.md`, `NEMO-A2A-1`, the relay-ack boundary, and the
   typed-MCP requirement.
2. Ask each session to prepare, but not publish, a dispatch. Require it to select the typed
   `buzz_a2a_dispatch` MCP tool, use `buzz_chat_send` only for normal channel replies, and
   name `buzz_a2a_inbox`, `buzz_a2a_status`, `buzz_a2a_cancel`, and `buzz_a2a_handoff` as the
   remaining coordination tools. Confirm no raw signing, authentication, or job-control
   credential and no absolute path enters the model/child environment.
3. Run `check_package.py` and both local smoke terminals. Confirm a case-variant `.GiT`
   request path and live checkout root/origin/branch/HEAD drift each fail closed.
4. Only with explicit staging authorization, continue with `staging-smoke.md`.

Current Codex skill-discovery guidance:
<https://learn.chatgpt.com/docs/build-skills>. Current repository-agent instruction guidance:
<https://developers.openai.com/cookbook/examples/gpt-5/codex_prompting_guide>.
