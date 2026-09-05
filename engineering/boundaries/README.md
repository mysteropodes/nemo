# Boundaries checker — R05 first increment

Implements the enforcement half of
[`engineering/remediation/04_MODULARITY_POLICY.md`](../remediation/04_MODULARITY_POLICY.md)
for a **bounded, explicitly declared set of modules** — a *profile* — not the whole
application. Code lives in [`scripts/nemo/lib/boundaries.cjs`](../../scripts/nemo/lib/boundaries.cjs)
(the checker, pure functions, no I/O beyond reading the files a profile names) and
[`scripts/nemo/boundaries.cjs`](../../scripts/nemo/boundaries.cjs) (a standalone CLI). Behavioral
tests are in [`scripts/nemo/boundaries.test.cjs`](../../scripts/nemo/boundaries.test.cjs)
(`node --test scripts/nemo/boundaries.test.cjs`).

This is **not wired into `npm run check`/`verify`, `scripts/nemo/lib/jobs.cjs` or
`package.json`** in this packet — see "What's pending" below.

## What it checks

Given a profile (JSON, shape in [`profile.schema.json`](./profile.schema.json)), for every
file every declared module lists:

| Rule | Fires when |
|---|---|
| `cycle` | Two or more declared modules import each other, directly or transitively. |
| `private-import` | An import resolves into another module's file that is not in that module's `publicApi`. |
| `layer-violation` | An import crosses into a layer the importer's declared `layerRules` does not allow. |
| `global-state` | `window.SM*` is accessed from a layer other than `adapters`/`bootstrap`. |
| `size` | A file's nonblank physical line count exceeds its `sizeProfile`'s `hardMax`, and no non-expired exception raises the ceiling far enough. |
| `expired-exception` | An exception's `expires` date is on/before the check's clock; it stops shielding its rule (which is then re-evaluated and may itself fail) and is reported by itself too. |

Intra-module imports (a file importing another file of the *same* module) are never flagged —
only cross-module edges are checked, matching the policy's "no private cross-module deep
imports, new cycles, ... or UI imports into domain."

## Usage

```bash
node scripts/nemo/boundaries.cjs <profile.json> [--root <dir>] [--json]
```

Exit 0 = no violations, 1 = one or more, 2 = bad usage or a profile that could not run (e.g.
an unknown `sizeProfile` key, or a listed file missing on disk).

## Limitations (v1, deliberate scope cut)

- **Regex-based import extraction**, not a real parser. A specifier that only *looks* like
  `require('x')` inside a comment or string would be misread. Acceptable for a small,
  hand-reviewed profile; not for scanning arbitrary source at scale.
- **No filesystem walking.** A module's `files` list is authoritative; a file that exists on
  disk but isn't listed is invisible to the checker. This is intentional for this increment —
  see the inventory contract below for where that changes.
- **Bare (non-relative) specifiers are external and unchecked** — no alias resolution,
  `node:`/npm packages are always ignored for cycle/private/layer purposes.
- **A layer with no `layerRules` entry is permissive** — declare every layer whose outbound
  imports you want enforced.
- `size` is measured **per file**, not summed per module, matching the policy's per-file line
  budgets.

## Integration contract for R01/R03 (pending)

R05's acceptance is explicit that full adoption is **blocked on R01** (current/target
inventory) and **R03** (`scripts/nemo/inventory.cjs`, owned by a separate work package —
`engineering/inventory/**`, `tests/fixtures/**`, `tests/bench/**` are out of scope for this
packet). This checker is written so that once either lands, its output can be mapped to
[`profile.schema.json`](./profile.schema.json) without changing `checkProfile`'s public
contract:

- **R03 inventory → `modules[]`.** Whatever the inventory scan groups a directory/file set
  into as a "module" should be emitted with the same four required fields this schema already
  needs: `id`, `layer`, `dir`, `files`. If the inventory already knows a module's declared
  public surface (e.g. an `index.cjs`/barrel file, or JSDoc-tagged exports), that becomes
  `publicApi`; if it doesn't yet, the safe default is `publicApi: []` (everything private,
  every cross-module reach flagged) rather than guessing.
- **R01 current/target layers → `layerRules`.** The "Logical layers" table and dependency
  rules in `04_MODULARITY_POLICY.md` are the source of truth; R01's classification of
  existing/legacy files into a target layer should produce exactly the `layer` value each
  module needs and the `layerRules.<layer>.allowedLayers` list.
- **`04_MODULARITY_POLICY.md`'s "Initial size profiles" table → `sizeProfiles`.** Copy the
  Warn/Hard maximum columns verbatim per row name (e.g. `"Domain/application JS or TS"`) once
  a profile is meant to represent real, not synthetic, ceilings.
- **`04_MODULARITY_POLICY.md`'s "Exceptions" section → `exceptions[]`.** Every field the
  policy already requires per exception (exact path, rule/profile, ceiling, owner, tracking
  issue, review/expiry date) maps 1:1 onto this schema's `exception` definition — nothing new
  to invent there.
- Once real `modules[]`/`layerRules`/`sizeProfiles` exist for a meaningful slice of the app,
  wiring `scripts/nemo/boundaries.cjs` into `scripts/nemo/lib/jobs.cjs` as a named job (and
  from there into `npm run check`/`verify`) is a small, mechanical follow-up — not part of
  this packet, which is scoped to proving the checker itself works.

## What's pending after this increment

- No `package.json` script, no `scripts/nemo/lib/jobs.cjs` job registration (explicitly out of
  scope for this packet).
- No real, reviewed profile for any part of the actual `src/js` tree — only the illustrative
  fixtures inside `boundaries.test.cjs`. Producing one is R01/R03's job, not this checker's;
  fabricating a "reviewed" profile here would misrepresent unreviewed code as audited.
- `layer-violation` was implemented alongside the five rules the R05 acceptance criteria name
  (cycle, private-import, global-state, size, expired-exception) because it falls out of the
  same module graph at near-zero extra cost, and the policy explicitly calls out forbidden
  layer edges; it is exercised by one additional test but isn't a required category.
