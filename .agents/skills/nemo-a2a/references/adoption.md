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
3. Run `check_package.py` and both local smoke terminals.
4. Only with explicit staging authorization, continue with `staging-smoke.md`.

Current Codex skill-discovery guidance:
<https://learn.chatgpt.com/docs/build-skills>. Current repository-agent instruction guidance:
<https://developers.openai.com/cookbook/examples/gpt-5/codex_prompting_guide>.
