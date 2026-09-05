# ADR 001: Motion curve extraction and test runner

Status: **review candidate**, 2026-09-05. Related: [R08](https://github.com/mysteropodes/nemo/issues/904).
Implementation owner: Codeximator. Integration/behavior reviewer: Codexitron and the maintainer.
R03/R05/R06 final acceptance and the full application workflow remain pending.

## Decision and production contract

Keep Node's test runner for this seam. Extract only the curve evaluator to
[`src/js/animation/curve.js`](../../src/js/animation/curve.js), using JSDoc/checkJs
and the same classic-script/CommonJS source. The bounded `SMAnimationCurve`
namespace publishes only `evalCurvePoints(points, x)`. `index.html` loads it before
`motion.js`; the existing `SMMotion.evalCurvePoints` facade references that function.
Existing track, expression-bake, effect and graph callers retain their public APIs.

The kernel owns no mutable state and has no imports, DOM, Paper, renderer, native
bridge or timer access. Callers own their point arrays. Evaluation neither mutates
nor caches them, so in-place edits are visible on the next sample. Key lookup,
defaults, legacy migration, hold flags, vector interpolation, spatial handles and
all document/UI ownership stay in `motion.js`. `ui.js` retains its own editor
evaluator; `tweens.js` and the open tween product PR are untouched.

The algorithm is preserved, including eight Newton iterations and the derivative
early exit. The old comment promising bisection fallback was inaccurate. Points
are on-curve waypoints, not CSS handles. Normal editor input has finite coordinates,
ordered x and endpoints 0/1. Missing/short curves return x before clamping; valid
curves clamp input x but allow output overshoot. Coincident-x legacy behavior is
characterized, not normalized. No persistent fields or item types are added.

## Characterization and consumer evidence

Sixteen cases passed on the original `35f0f5f` Motion facade before extraction and
on the candidate afterward. The retained Node suite directly imports the kernel
and loads the complete Motion script for facade tests; it does not extract function
text. Startup DOM registration is stubbed in the VM facade tests.

Independent expectations include the smoothstep polynomial, an analytic slope-limit
case, flat/turning waypoints, manual overshoot, unbounded y, clamping, absent/short
curves, degeneracies, frozen input and mutable curves. Stateful cases cover real
key insertion/replacement, outgoing holds at exact boundaries, fresh output arrays,
default-curve cloning, holder JSON restoration and spatial Position handles.
Holder JSON restoration alone does not establish application save/load behavior.
The existing independent CSS-bezier reference now imports the new module.

On code HEAD `2dd4054352bdb12e5323a7ae8d716e17b372a4b4`, `npm test` and named
`test:unit` both passed 201 tests. Doctor/check/quick verify passed, including 15
Rust tests. The real Chrome application loaded `index.html` without page errors
and exposed the identical evaluator through `SMMotion`. Native clicks created a
project and selected Motion; production key/history API calls produced samples
15.625 → edit 49.375 → undo 15.625 → redo 49.375. Those history checks used CDP API
calls, not the keyframe UI. Render pixels, save/reopen, export and Tauri are still
required workflow gates; browser startup does not substitute for them.

## Disposable runner trial

[Measured results and source hashes](runner-trial-20260905.json) identify the clean
code HEAD above. Environment: macOS 26.6 arm64, Node 25.9.0, npm 11.12.1; exact
trial dependencies: Vitest 4.1.0, Playwright 1.61.0, TypeScript 5.9.3 and
`@types/node` 25.3.0. Nothing was added to the repository's dependencies/lockfile.

The Node copies were byte-identical to the 16 retained tests. Vitest copies changed
only the `node:test` import line, retaining its line count; `--globals` supplied
`test`. Assertions, production source and stateful cases were identical.

| Fresh-process wall time | Node | Vitest |
| --- | ---: | ---: |
| First invocation | 0.17 s | 0.41 s |
| Subsequent invocations | 0.16 / 0.16 / 0.17 s | 0.41 / 0.41 / 0.42 s |
| Subsequent median | 0.16 s | 0.41 s |

Both passed 16/16. “Cold” means first invocation after setup; “warm” means later
fresh processes. No OS cache flush or watch-mode rerun was measured. This small
sample favors Node for startup cost, not a claim about general runner performance.

Commands in the disposable mirror were `node --test tests/animation/*.node.test.cjs`
and `vitest run --globals tests/animation/*.vitest.test.cjs`. An intentional
fallback assertion mismatch (`17 !== 18`) made both exit 1 at the original line 11.
Node provided the assertion and exact stack location; Vitest also gave a clearer
inline code frame/diff. This verifies untransformed CommonJS locations, not a
TypeScript/bundled source-map pipeline.

A fresh private-cache install took 5.51 s and installed 49 packages: 78,276 KiB of
dependencies plus 167,568 KiB cache. The initial default-cache attempt failed with
EACCES after 3.44 s; switching to the disposable cache resolved it. These sizes
include Playwright/type tooling and are not Vitest-only overhead. Node requires
no extra runner installation. Temporary adapters and failing probes are not retained
as a second test suite.

## Browser and isolation comparison

Both runners used the same Playwright 1.61.0 driver and Chrome 152.0.7977.76. A
synthetic native range control loaded both shipped scripts and drove `SMMotion`:
frame 25 = 15.625; keyboard ArrowRight to 26 = 16.7648; Hold at 26 = 0; endpoint
100 = 100. Node passed 1/1 in 1.30 s; Vitest passed 1/1 in 1.42 s. Concurrent runs
also passed using separate temporary roots, port-0 servers and browser contexts,
all closed afterward. This proves isolation for those disposable processes, not
arbitrary concurrent editor worktrees or native app data isolation.

This compares the runners controlling the same browser API, not
[Vitest Browser Mode](https://vitest.dev/config/browser/playwright). Component
transforms, browser-provider integration and watch ergonomics were not evaluated.

## Independent type boundary and rejected alternatives

The production kernel passed this separate command with the pinned trial compiler:

```sh
tsc --noEmit --allowJs --checkJs --strict --types node --target es2022 --module commonjs src/js/animation/curve.js
```

A separate call passing a string as x failed with TS2345 at the expected line.
Runtime tests are not type checking. The boundary covers the new kernel, not the
legacy Motion/global application. The trial does not install a repository-wide
type gate; shared CI/toolchain adoption remains with the lead.

Vitest's better failure presentation does not justify a dependency/transform layer
for this untransformed kernel. Revisit it when TS/ESM components or measured watch
requirements warrant a new comparison. Web Test Runner and Jest were not trialed;
neither supplies a demonstrated benefit for this bounded extraction. A custom test
runner is unnecessary. Keep the single Node suite in `tests/animation`; both
`npm test` and the named unit job discover it.

## Remaining acceptance

The coordinator confirmed no recorded competing owner of this seam. The original
information-only R08 request still has only its request and cancellation control;
there is no recipient/terminal acknowledgement. Its named worktree and branch refs
are absent. That is no observed effect at those coordinates, not proof of no effects
elsewhere. This new candidate does not replay or close that operation.

Review the bounded facade/global boundary with R05, complete applicable R03/R06
evidence, and exercise create/edit/key/undo/redo/save/reopen/render/export in the
real browser and Tauri before production merge or full R08 acceptance. No issue
closure, release or merge is part of this candidate.
