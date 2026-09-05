# Boundaries checker — R05 bounded enforcement

Implements the enforcement half of
[`engineering/remediation/04_MODULARITY_POLICY.md`](../remediation/04_MODULARITY_POLICY.md)
for a **bounded, explicitly declared set of modules** — a *profile* — not the whole
application. Code lives in [`scripts/nemo/lib/boundaries.cjs`](../../scripts/nemo/lib/boundaries.cjs)
(the checker, pure functions, no I/O beyond reading the files a profile names) and
[`scripts/nemo/boundaries.cjs`](../../scripts/nemo/boundaries.cjs) (a standalone CLI). Behavioral
tests are in [`scripts/nemo/boundaries.test.cjs`](../../scripts/nemo/boundaries.test.cjs)
and [`scripts/nemo/boundaries-ratchet.test.cjs`](../../scripts/nemo/boundaries-ratchet.test.cjs).
The library validates the profile before reading its sources; the CLI uses the same validation
and exits 2 on malformed policy or baseline input.

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
| `unsupported-import` / `unsupported-global` | A dynamic load or computed `window` member cannot be determined from literal tokens. |
| `expired-exception` | An exception's `expires` date is on/before the check's clock; it stops shielding its rule (which is then re-evaluated and may itself fail) and is reported by itself too. |

Intra-module imports (a file importing another file of the *same* module) are never flagged —
only cross-module edges are checked, matching the policy's "no private cross-module deep
imports, new cycles, ... or UI imports into domain."

## Usage

```bash
node scripts/nemo/boundaries.cjs <profile.json> [--root <dir>] [--json]
node scripts/nemo/boundaries.cjs <profile.json> --baseline <prior-profile.json> [--root <dir>] [--json]
```

Exit 0 = no violations, 1 = one or more, 2 = bad usage or a profile that could not run (e.g.
an unknown `sizeProfile` key, malformed exception, missing file, or unsupported lexical syntax).
Unknown flags, missing option values and extra positional arguments also exit 2.

The second form enables the size-baseline ratchet. `<prior-profile.json>` must be the explicit,
previously reviewed profile from the commit being compared; the checker does not guess a Git
revision or silently fall back when that file is missing. The original invocation remains
unchanged and runs the profile rules without a baseline comparison.

`--baseline` is a caller-supplied trust boundary. Future CI must materialize this file from the
protected base revision and pass that path to the checker; it must never accept a baseline from
candidate-controlled contents. This CLI validates and compares the supplied document, but it
cannot prove which Git revision supplied it.

## Size-baseline ratchet

[`scripts/nemo/lib/boundaries-ratchet.cjs`](../../scripts/nemo/lib/boundaries-ratchet.cjs)
compares the prior and candidate profiles by normalized root-relative file path. Module IDs and
`sizeProfile` names are metadata, not ratchet identity. For each exact path, the effective
ceiling is its policy value: the size exception's `ceiling` when one exists, otherwise the
assigned profile's `hardMax`.

Baseline mode fails when:

- a retained named size profile raises its ordinary `hardMax`, or a newly named profile exceeds
  the largest ordinary hard maximum in the prior adopted policy;
- a candidate effective ceiling exceeds the prior exact-path ceiling, including after a module
  reassignment or `sizeProfile` rename;
- a prior path still exists in the source tree but was deleted from the candidate profile;
- a prior size exception is deleted while an ordinary profile preserves the same enlarged
  allowance, which would discard its owner/issue/expiry accountability; or
- a size exception appears on a path absent from the prior baseline, including an exception
  moved or renamed onto a new source path.

A lower effective ceiling is accepted and recorded in `ratchet.reductions` with its current
nonblank line count. A prior profile entry whose source was actually removed is accepted and
recorded in `ratchet.removals`. The ordinary candidate check still reads every declared source
and rejects actual line count above the candidate ceiling; unchanged policy therefore cannot
hide source growth above its committed ceiling. JSON output adds a `ratchet` object only when
`--baseline` is supplied. Text output reports ratchet violations, reductions and removals.

The ratchet is intentionally policy-to-policy. Source renames that do not carry a size
exception are treated as retired and new paths, but the new path still cannot use an enlarged
ordinary policy: same-budget profile renames and new files at or below the prior adopted maximum
remain valid. Full source coverage and classification remain the R01/R03 inventory gate. Review
the reported removals rather than treating them as proof that code was deleted intentionally.
The comparator does not infer file types or detect content moves, and a new path may use any
still-adopted ordinary profile. Reviewing each path's profile assignment and making baseline
provenance immutable are responsibilities of the R01/R03 and CI adoption gate.

## Limitations (v1, deliberate scope cut)

- **Lexical JS scanning, not AST or binding analysis.** Literal `require` calls (including
  optional calls), static imports/re-exports and dynamic `import()` accept whitespace and
  comments between tokens. Quoted/no-substitution template targets and ordinary string
  escapes are decoded. Object method declarations named `require` with their body opener on
  the closing-parameter line are not loader calls; separating that brace remains outside this
  lexical subset because automatic semicolon insertion can make the same tokens a real call
  followed by a block. Comments, string text and ordinary regex literals are opaque; template
  substitutions are scanned. Nonliteral loads fail with `unsupported-import`.
- **Deliberately unsupported lexical ambiguity fails the run (exit 2).** A slash after `)`
  or `}` requires statement/expression context to distinguish regex from division; escaped
  identifiers and legacy numeric string escapes also require a fuller parser. This can
  reject otherwise valid JS. Use a parsed R03 inventory before broader adoption.
- **Global rule covers direct `window.SM*` access**, including whitespace, optional chaining
  and literal bracket properties. Nonliteral computed `window` access fails explicitly.
  Binding aliases, destructuring, `globalThis`/`self`, indirect loaders such as `module.require`
  or `eval`, and function-local shadowing are not resolved. Hand-review these in the bounded
  profile; this checker does not certify the absence of every implicit global or dependency.
- **No filesystem walking.** A module's `files` list is authoritative; a file that exists on
  disk but isn't listed is invisible to the checker. This is intentional for this increment —
  see the inventory contract below for where that changes.
- **Bare (non-relative) specifiers are external and unchecked** — no alias resolution,
  `node:`/npm packages are always ignored for cycle/private/layer purposes. Unresolved
  relative references (including URL query/hash suffixes) are also outside this graph;
  extension inference is limited to `.cjs`, `.js`, `.mjs` and their directory indexes.
  Review unresolved/indirect edges before using any profile as an adoption gate.
- **A layer with no `layerRules` entry is permissive** — declare every layer whose outbound
  imports you want enforced.
- `size` is measured **per file**, not summed per module, matching the policy's per-file line
  budgets.

## Profile and exception validation

The schema describes the input shape; the library additionally checks relationships that
JSON Schema cannot directly express here: unique module IDs and file ownership, normalized
relative paths, nonempty module/file sets, known size profiles, public API subsets and
`warn <= hardMax`. Unknown fields, invalid numbers and malformed policy never become a pass.

Every exception must name a declared exact file, supported rule, nonblank owner/issue/reason
and a real `YYYY-MM-DD` expiry. Duplicate file/rule exceptions are rejected. Size exceptions
require a finite integer ceiling at least as high as the base hard maximum; exceeding that
ceiling still fails. Other rules cannot carry a size ceiling. All expired entries fail even
when the associated source has no current violation; expired exceptions never suppress rules.

An active exception for private imports, layer edges or globals applies only to its exact
file/rule. A cycle exception removes only dependency contributions originating in its exact
file; another file contributing the same module edge remains in the prohibited graph. Applied
exceptions are recorded in the report. Unsupported-source diagnostics
cannot be waived by an exception. Supplying `--baseline` compares the candidate against an
explicitly selected older profile; the checker never infers which Git revision is authoritative.

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
- No canonical committed baseline yet. This increment supplies the comparison mechanism; R01/R03
  still supply reviewed real profiles and a later integration packet must materialize the
  protected-base copy in standard commands/CI.
- `layer-violation` was implemented alongside the five rules the R05 acceptance criteria name
  (cycle, private-import, global-state, size, expired-exception) because it falls out of the
  same module graph at near-zero extra cost, and the policy explicitly calls out forbidden
  layer edges; it is exercised by one additional test but isn't a required category.
