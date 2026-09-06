# R03 baseline acceptance

This candidate combines the retained R03 inventory, fixture corpus, initial CPU
workloads, and browser harnesses on main `b86c69dda2a061ef551fc522dd96327a5b9a4f66`.
The existing R08 extraction remains separate. Application JavaScript, Rust, WASM,
and packaged sidecar bytes are unchanged from that base.

This is evidence for [R03](https://github.com/mysteropodes/nemo/issues/899) and
[F0](https://github.com/mysteropodes/nemo/issues/888). It does not close the later
application extraction, shared-command, native-isolation, or installed MCP gates.

## Reproduction

Run the named commands from a clean checkout. Fixture generation requires one of
the compression runtimes documented in the [fixture guide](../../tests/fixtures/README.md).
The recorded regeneration used Node 22.17.1 with zlib
`1.3.0.1-motley-780819f` and reproduced all 41 files across 12 fixtures.

```sh
npm run fixtures -- --check
npm run inventory -- --check
npm run verify
npm run bench
node scripts/nemo/verify.cjs --jobs test:rust-tauri,test:integration,test:desktop
```

The opt-in [browser document harness](../../tests/fixtures/BROWSER_ACCEPTANCE.md)
and [render workload](../../tests/bench/BROWSER_RENDER.md) document their external
Playwright dependency and exact commands. Neither installs dependencies or changes
user projects. Missing platform capabilities must remain explicit in the receipt.

## Standard verification

Clean candidate `74a2b0059e74bb0321e388650aabc9172d2f9144` passed `npm run verify`:
all six static checks, the current 902-row inventory, 416 Node tests (zero failures,
one filesystem skip), and 15 geometry Rust tests. The actual boundary lane against
protected base `b86c69dda2a061ef551fc522dd96327a5b9a4f66` also passed, with all 41
handwritten tooling files declared and no exclusions.

The inventory now identifies the exported color-swatch wrapper's real click
registration while preserving the paired input's own input event. Its status totals
are 875 inventoried, 26 explicitly unmapped, and one unavailable with reason.
Sixteen permanent swatch/record-identity controls pass, including local receiver
shadowing, named and inline export replacements, member writes, and list reassignment.
No application source change or new policy exception is needed.

Quick receipt SHA-256:
`d487251f9815dbc554dafc5bae44629c088ce3634f81a1f9b37de2d0979f60cd`.

## CPU workload evidence

The retained CPU runner measured all 15 evaluation, copy, serialization, and memory
workloads on an Apple M3 Ultra (32 logical CPUs, arm64, Node 22.17.1). Representative
medians were 755.014 ns/property evaluation, 1,819.279 ns/ease-curve evaluation,
7.097 ms for the production undo clone of `bench-vectors-8x24`, and 1,972,656 bytes
for that workload's parsed document. The two export-fixture render/export entries
remain explicitly `not-run`; separate software rendering is reported below.

The CPU receipt identifies `ad20a824efacbd7d439d21645c00a798f4695faa` plus dirty digest
`fac4f525d33d8ebbf885dee8104a88714f7c89aaed4324fd773009a6d56711f5`; its only untracked
file was this baseline document. Benchmark and application inputs were unchanged.
Receipt SHA-256: `0e8c6fcb51d30e80541f439a4d590b07db4bbce72f0788d059f9b22a58b6605b`.

## Browser and native evidence

These runs identify clean candidate
`ad20a824efacbd7d439d21645c00a798f4695faa`. Later inventory-only corrections do not
change their application, fixture, or harness inputs; integration must verify those
input hashes before reusing the results.

| Surface | Result | Evidence and limit |
|---|---|---|
| Browser document | Pass | 16 independent expectations executed before and after reopen: 32 executions. Both curve-handle and translation corruption controls rejected. Eight contexts and owned runtimes cleaned up. |
| Browser render | Pass | 24 frames completed by production Rust/vello WASM WebGPU through SwiftShader; each sample awaited the actual render queues. Chrome 152.0.7977.76, Playwright 1.61.0, headless software adapter. |
| Native Rust | Fail | 36 tests passed; `video_decode::tests::indexed_random_seek_cost_is_flat_and_small` failed its existing timing assertion at p95 36.2 ms. This run does not establish native performance acceptance. |
| Application integration | Not run | No `tests/integration` suite exists in this candidate; R12/R13 own the shared-command lifecycle suite. |
| Packaged desktop | Blocked | No built app or `tests/desktop` harness exists in this candidate. Earlier R04 acceptance retains its own artifact identity. |

The render workload was `bench-vectors-8x24`, seed 1113248609, with a 1920 by 1080
document and a 1038 by 624 measured render target. For one 24-frame iteration after
one warmup frame, navigation through queue completion measured p50 42.5 ms, p95
46.8 ms, and p99 305.1 ms. These are shared-host software-renderer measurements,
with no invented budget, hardware-GPU claim, scanout measurement, or native export
acceptance.

Retained receipt SHA-256 values:

- Browser document: `3168c78afbdfa88a9c4ca979cc0395d8c15b657eb170bff38f586f00a9290a5f`.
- Browser render: `716d68af8d842b7e9170c0cba97f8485ca3db00453887f775e8c4ce906f73d29`.
- Fixture manifest: `eea9aa1cb9e39e1ed86b9d44169f415fdc36fef0e65c96640acbc8cd999ecbc6`.

## Remaining acceptance

Browser document round trips do not exercise browser Save/download UI. Software
rendering does not establish fixture pixel equality, native GPU behavior, or the
application's complete export pipeline. The CPU benchmark keeps unavailable
render/export workloads explicit; its separate software-render receipt does not
turn the native export entry into a pass. Full R03 closure requires reconciliation
of these outstanding fixture and measurement gates. F0 permits their explicit
fail/blocked/not-run disposition while retaining the identified reproducible baseline.
