# PR 949 browser acceptance and native handoff

Owner: Codeximator. Scope: the first Motion curve extraction only. This record
continues the existing candidate; it does not close R08 or its prerequisites.

## Task queue

1. Browser save/open/render/export and edit/undo/redo: passed on the source-identical `562a8b9` candidate.
2. Actual loader and unit discovery: passed; companion `2dd4054` has the same
   stable patch ID as `40ee5fd`. The alternate `b13c9d3` wrapper is absent.
3. Tauri application workflow: pending the native slot and verified R06 runtime
   isolation/identity mechanism. No native interaction occurred in this lane.
4. Final R03/R05/R06 integration and maintainer review: pending. Keep PR draft.

## Controlled fixture and oracle

[`curve-workflow.json`](../../tests/animation/fixtures/curve-workflow.json) is an
owned synthetic document: white 320 by 180 canvas, 24 fps, 21 frames, one red
20 by 20 rectangle at (20, 60). Its layer Position keys are (0, 0) at frame 0
and (128, 0) at frame 20. The outgoing curve uses endpoint manual tangents
(tx=1, ty=0), so its independent timing oracle is `s(t)=3t²−2t³`, `t=frame/20`.
It does not call the production evaluator to generate expectations.

| Zero-based frame | UI frame | Original X offset | After starting X is 64 | Original left | Edited left |
| --- | --- | --- | --- | --- | --- |
| 0 | 1 | 0 | 64 | 20 | 84 |
| 5 | 6 | 20 | 74 | 40 | 94 |
| 10 | 11 | 64 | 96 | 84 | 116 |
| 15 | 16 | 108 | 118 | 128 | 138 |
| 20 | 21 | 128 | 128 | 148 | 148 |

Every sampled native-size raster must contain exactly 400 opaque red pixels,
all remaining pixels opaque white, and bounds `[left,60,left+20,80)`.

## Observed browser result

Chrome 152.0.7977.76 passed all 25 numeric samples and 50 pixel comparisons
(25 Rust/vello readbacks and 25 Paper PNG previews), then downloaded and reopened
the exact saved JSON and produced five independently checked SVG downloads.
No page errors or extraction defects were observed. The controls used real input
and keyboard events. Runtime identity remained healthy with source/build matches;
served source, WASM and output hashes are retained in the acceptance receipt.
The fixture and driver were added during this continuation; the production source
bytes were unchanged from `562a8b9`.

## Reproduce browser acceptance

The opt-in [driver](../../tests/animation/browser-acceptance.cjs) uses externally
provided Playwright. It is not named `*.test.cjs` and adds no duplicate unit
suite, package dependency or lockfile. Run from the candidate root with
Playwright 1.61.0 available through the normal resolver or an absolute module
path supplied as `NEMO_PLAYWRIGHT_MODULE`:

```sh
node tests/animation/browser-acceptance.cjs reports/r08-browser-acceptance
```

The installed Chrome is launched headless in an isolated browser profile/context
on a port-0 loopback runtime. SwiftShader is explicitly selected; the driver
requires an observed software fallback adapter and an enabled Rust/vello engine.
First shader compilation can take over a minute. No native input or hardware GPU
slot is used. Paper installs globals that conflict with Playwright's in-page
poller, so readiness is polled from Node without changing application globals.

The driver opens the fixture through the real Open file chooser, selects Motion,
edits the Position X field through DOM input, uses the real keyboard undo/redo
and Save commands, inspects the downloaded JSON, and reopens it in a fresh
browser context with no shared autosave. It samples five frames before edit,
after edit, after undo, after redo and after reopen. Both production Rust readback
and Paper PNG preview are independently pixel-checked; stored layers must remain
unchanged by rendering. Actual SVG exports use the menu/modal/download path and
are decoded and pixel-checked. The work-area frame is selected through the state
API because this range has no numeric controls. The source and WASM responses
are hashed against the candidate's local bytes, with healthy runtime identity
required before and after. The output receipt records every artifact hash.

These are browser application results for a controlled fixture. PNG sequence
export is explicitly refused in browser mode; the browser's SVG fallback emits
one representative frame per export. Video, the R03 corpus and Tauri are separate
acceptance surfaces.

## Next runnable native acceptance

The candidate's [runtime isolation contract](../runtime-isolation.md) supplies
an isolated build launcher, but explicitly does not yet supply a verified Tauri
data/autosave override or native runtime identity handshake. Creating a
`tauri-data` directory alone does not redirect the app. Native history currently
resolves `__TAURI__.path.appDataDir()`. The desktop test job also has no wired
runner. Do not infer those gates from a browser receipt or a successful build.

After finalizing the candidate and fixture, the approved build procedure is:

```sh
git rev-parse HEAD
git status --short
shasum -a 256 tests/animation/fixtures/curve-workflow.json src/js/motion.js src/js/animation/curve.js
node scripts/nemo/build.cjs start --task pr949-curve-native-acceptance
```

Retain the returned task coordinates and owner token locally. Check the exact
operation with `node scripts/nemo/build.cjs status --task pr949-curve-native-acceptance
--owner <returned-token>` in a separate shell. Require successful completion,
matching source/build identities and an inspected post-build diff. Use the app
under the returned build root's `tauri-target/<host-triple>/release/bundle/macos/Nemo.app`;
hash its executable and preserve its logs before releasing task roots.

Once R06 supplies the verified native launch/identity mechanism and Claudirizer
releases the native slot:

1. Launch that exact built app through the verified mechanism. Record the app
   hash, process identity, WebView/autosave/history roots and renderer backend.
2. Click Open (`#start-open`), complete **Open Project**, and load the fixture.
   Select Motion and the layer name, then its disclosure arrow. Sample UI frames
   1, 6, 11, 16 and 21 against the table above.
3. At UI frame 1 edit Position X (first `.motion-val` in the layer's Position
   `.motion-prop-row`) to 64 and commit. Verify edited samples, blur, use Command-Z
   and Shift-Command-Z, and verify original/edited samples respectively.
4. Use Shift-Command-S and the native **Save Project As** dialog to write a
   task-owned JSON file. Inspect the two keys and endpoint tangents. Reopen that
   exact file with Command-O and repeat the five edited samples.
5. Export baseline and reopened edited states into separate empty task-owned
   folders. Menu (`#app-menu-btn`) → Export (`#ctx-export`); format PNG
   (`#exp-format=png`), range All (`#exp-range=all`), scale 1, alpha unchecked;
   run (`#exp-run`) and complete **Dossier de séquence PNG**.
6. Require 21 PNGs at 320 by 180. Check files `frame_0001.png`, `frame_0006.png`,
   `frame_0011.png`, `frame_0016.png`, `frame_0021.png` against the pixel oracle.
   Hash outputs and bind them to the candidate, fixture and built executable.

This plain-shape PNG sequence follows Paper's export path even in Tauri; native
live renderer evidence is therefore recorded separately. Copy wanted outputs and
logs before `build.cjs stop` with that task's returned owner token: stop releases
and removes the isolated task roots. None of this native procedure is claimed as
executed here.

## Remaining integration gates

Clauditron confirmed the synthetic fixture does not conflict with his R03 corpus
under `tests/fixtures` and `tests/bench`. PR #946's current benchmark loader
brace-extracts six curve functions from `motion.js`; extraction in #949 removes
those declarations. The second PR integrated must replace that curve extraction
with `require('../../src/js/animation/curve.js')`, expose `SMAnimationCurve` in its
VM and retain only the remaining Motion slices. R03 owns those files. Also
reconcile the additive `package.json`, jobs and scripts README changes with
#944/#946. Run the combined candidate's required checks after resolution; green
checks on the separate PRs do not establish combined acceptance.

Final R03 corpus/baselines, R05 boundary review, R06 runtime acceptance, native
workflow and maintainer review remain required. No merge, issue closure, release,
next extraction or `tweens.js` change is part of this handoff.
