# Inventory binding correction evidence

Base: PR #944, `dd39e5215f000031c5f75821913cce2e6f971db9`.
Candidate branch: `codex/r03-binding-repair-1a65df38`.

The generator now checks lexical regions for returned lookups, tracks anonymous
function/arrow parameters and block declarations, rejects reassigned ID parameters,
and resolves helpers at their call sites. ID-helper and returning-helper caches use
function identity so a sibling's same-name definition cannot supply a binding.
A helper without a proved binding is treated as transparent only when its own
return statements prove an unchanged element lookup.

## Runtime comparisons

The focused tests execute synthetic source in `node:vm` with independently
instrumented elements. They first assert the actual registrations, then compare
the generated rows. Missing registrations are allowed only for explicitly named
unsupported targets; extra registrations fail.

| Original counterexample | Actual registrations | Corrected inventory | Explanation |
|---|---|---|---|
| Quoted `return` text names `a`; real return names `b` | `b: click` | `b: click`; `a` unmapped | Literal text cannot establish a return. |
| Helper replaces argument `a` with ID `b` | `b: click` | Both unmapped; `a` has a helper reference and reason | No constant propagation of a replacement ID. The original argument cannot inherit its registration. |
| Anonymous callback parameter shadows outer element `a` | `b: click` | Both unmapped; both lookups remain references | Arrays of element expressions are not supported NodeList or literal-ID-table flows. Callback shadowing cannot bind `a`. |
| Inner block's `button` is `b`; outer `button` is `a` | `b: input`, `a: click` | Exact match | Block declarations own their assignments and uses; a separate `var` control confirms function ownership. |
| Sibling functions define different `wire` helpers | `b: click` | Exact match; `a` reference only | Each call resolves its own visible helper. |

The anonymous-shadow case also runs with block and concise arrow callbacks.
Additional comparisons cover ID-taking and fixed-return same-name helpers, a
quoted helper lookup, and a reassigned ID-returning helper. This is a bounded
scanner correction, not a complete JavaScript parser or constant evaluator.
Consumer reachability remains the existing heuristic.

## Validation

- `node --test tests/nemo-inventory.test.cjs`: 31 passed, including all 24
  existing tests and seven new test groups containing 12 runtime comparisons.
- Those seven new groups all fail against the unmodified pinned generator,
  demonstrating that they detect the reviewed defects.
- `node scripts/nemo/inventory.cjs` regenerated the artifacts; the focused
  suite's `--check` freshness gate passed. `git diff --check` passed.

## Shipped-row delta

All 902 rows retain their existing statuses, events, handlers and consumers:
874 inventoried, 27 unmapped and one unavailable with a reason. Four rows gain
one lookup-only reference each:

| Row | Added reference in `src/js/timeline.js` |
|---|---|
| `dom:#lip-start` | line 11352, `initLipSync()` |
| `dom:#lip-end` | line 11353, `initLipSync()` |
| `dom:#lip-sens` | line 11354, `initLipSync()` |
| `dom:#lip-hold` | line 11355, `initLipSync()` |

These calls use the local `num(id)` reader, which returns a numeric value. They
are references, not additional event registrations. The CSV is byte-identical;
JSON gains those references and the generator/source stamp, while Markdown
changes only the stamp. The source-input digest remains
`cf5faf8460255998487b05fa35401a5c3a3258603f4f1bb368043cef202b8e7a`.

## Integration

Apply the single scoped repair commit after the pinned #944 head (or its
descendant containing that head). The branch does not modify #944's author ref,
the #946 fixture/benchmark lane, packages, jobs, native code or parser profiles.
If source inputs change during integration, regenerate the derived artifacts
and rerun the focused suite. Integration and remediation acceptance remain with
the lead; this packet makes no browser, desktop or benchmark acceptance claim.
