---
name: nemo-a2a
description: Use the repository's canonical Nemo A2A collaboration protocol and shared project workflow from Claude sessions.
---

<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Nemo A2A for Claude

Read and follow `../../../.agents/skills/nemo-a2a/SKILL.md`. That tracked file and its
references are the only canonical copy. Its protocol version is `NEMO-A2A-1` and its skill
version is `1.3.0`; stop if either value disagrees with the canonical package.
