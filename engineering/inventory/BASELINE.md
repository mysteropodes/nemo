# Nemo baseline

Two things live here. [The adopted current-state baseline](#adopted-current-state-baseline)
is what a later run is compared against today. [The R03 baseline acceptance](#historical-r03-baseline-acceptance)
below it is retained historical evidence at its own revision, which P02 does not
restate or supersede.

Baseline means the exact observed state, including its known defects. Nothing on
this page is a target, a budget, or a promise that a check will pass.

## Adopted current-state baseline

The machine-readable manifest is [`baseline-manifest.json`](baseline-manifest.json)
(schema `nemo.baseline-manifest/1`). It, not this page, is the comparison
authority: prose cannot be diffed, and a result quoted without the bytes it was
measured on is not evidence.

```sh
node scripts/nemo/baseline.cjs --check          # compare the newest receipt
node scripts/nemo/baseline.cjs --check --from <runId> --json
node scripts/nemo/baseline.cjs --adopt --from <runId>[,<runId>]   # re-adopt
```

`--check` classifies every job as **new regression**, **unchanged known
failure** or **environment mismatch**, and verifies the immutable references
first. Exit `0` reproduces the baseline, `1` is a blocking verdict (a new,
changed or newly observed failure), `2` is inconclusive (a stale reference, an
environment mismatch, or a job this run did not cover). A job the run did not
cover is reported as `missing-entry` — never as a pass.

Severity depends on the verdict **and on whether the job is required** (T02):

| situation | required job | optional job |
|---|---|---|
| baseline entry the run was expected to cover is absent | exit `1` — the run did not answer | exit `2` — uncomparable |
| still `blocked` / still `not-run`, unchanged | exit `2` — no evidence, listed under `summary.noEvidence` | exit `0` — nothing to answer for |

An unchanged `blocked` used to exit `0` on both, so a required runtime that had
produced no evidence at all read as "nothing to answer for". It is now
inconclusive until a run produces a result: missing native/desktop evidence is
never a pass. `test:desktop` is the live case — replaying this baseline exits
`2` and names it, and will keep doing so until a packaged app exists to test.

A run only answers for what it claimed: `compare(..., { expect })` limits the
required-missing rule to the jobs the run was asked to execute, so
`node scripts/nemo/job.cjs test:rust` is not judged for `test:desktop`. Entries
outside that scope are still listed, marked `severity: "ok"`.

Every run through `verify.cjs`/`job.cjs` also writes
`reports/<runId>/comparison.json` with this classification. It is a **separate**
result: job statuses and the run's own exit code are untouched by it, so a known
failure still fails the run and is only labelled as known. `--no-baseline` skips
it.

### Selected source and environment

| | |
|---|---|
| Source | `ded641bd763379f36d629bf1686fcce8a6137a66` (`origin/main`), clean |
| Version | package / `tauri.conf.json` / `index.html` fallback all `0.7.0-alpha.4` |
| Fixtures | `tests/fixtures/manifest.json` `eea9aa1cb9e3`, format version 13 |
| `geometry_wasm_bg.wasm` | `c35ad2045f18`, 1 644 666 bytes |
| FFmpeg sidecar | `ffmpeg-aarch64-apple-darwin` `8fbf0afc8a0c`, 22 127 000 bytes |
| Host | darwin 25.6.0 arm64, Apple M4 Max ×14, node v24.4.1, host triple `aarch64-apple-darwin` |

Evidence is two receipts on that commit: `20260909T154312Z-ded641b`
(sha256 `d1aae6928fb9`, quick profile, clean tree) and
`20260909T154544Z-ded641b-dirty` (sha256 `1d5c7768ccf9`, the remaining
full-profile jobs). The second ran with this task's own tooling files present
but uncommitted; they add no application source and ship in the same change.
The manifest records that as `references.source.worktreeIgnored`, rather than
hiding it.

This host is not the one that produced the R03 evidence below (Apple M3 Ultra
×32, node 22.17.1). Timing-sensitive results are not transferable between them,
which is why the environment is part of the manifest rather than a footnote.

### Current state — 12 jobs: 9 pass, 1 fail, 2 blocked

| Job | Result | Exact case |
|---|---|---|
| `doctor` | Pass | 16 tools probed, 1 absent (`wasm-bindgen`) |
| `check` | Pass | 6 static checks |
| `inventory` | Pass | up to date, 902 rows, digest `c4c968bc2d41` |
| `test:unit` | Pass | 634 tests, 632 pass, 0 fail |
| `test:rust` | Pass | `geometry-wasm`, 4 binaries, 15 passed |
| `test:integration` | Pass | `tests/integration` exit 0 |
| `test:browser` | Pass | `tests/browser` Playwright exit 0 |
| `build:wasm` | Pass | `wasm-pack` build ok; output differs from the committed `src/wasm` bundle — wasm-pack is not byte-reproducible across toolchains, recorded as information |
| `bench` | Pass | 15 workloads measured, **2 not-run** (`render.engine.export`, `export.mp4.export` — no render backend outside a browser or the packaged app); no budgets |
| `build:desktop` | **Fail** | `bundle-ffmpeg-dylibs.py failed (1)` |
| `test:rust-tauri` | **Blocked** | native fixture sidecar unavailable: `SIGABRT`, no FFmpeg version banner, `dyld: Library not loaded: /opt/homebrew/opt/libvpx/lib/libvpx.12.dylib` |
| `test:desktop` | **Blocked** | no packaged app (`src-tauri/target/*/release/bundle/macos/Nemo.app` or `NEMO_DESKTOP_APP`) |

`blocked` is a missing capability, never a skip and never a pass. `not-run` is
an intentional non-attempt. Neither is evidence that the underlying behavior
works.

### The one native blocker, and what it actually is

`build:desktop`, `test:rust-tauri` and `test:desktop` are **one** root cause,
not three. The committed sidecar is dynamically linked (the LGPL rebuild of
2026-08-18, `CLAUDE.md` §7) and references versioned Homebrew library names. Of
its 26 external dylibs, exactly two are absent on this host:

- `/opt/homebrew/opt/libvpx/lib/libvpx.12.dylib` — installed libvpx is 1.15.2
- `/opt/homebrew/opt/svt-av1/lib/libSvtAv1Enc.4.dylib`

So `ffmpeg -version` aborts under dyld, `test:rust-tauri` is blocked on its
native fixture, `bundle-ffmpeg-dylibs.py` cannot resolve the dependency and
fails the desktop build, and `test:desktop` is then blocked for want of a
packaged app. This is precisely the drift `CLAUDE.md` §7 documents: the build
host must carry the Homebrew **major** versions the committed sidecar was
linked against.

Two facts worth keeping separate: `doctor` reports **pass** while the sidecar
cannot execute, because it records sidecar health as a capability rather than a
job result; and the sidecar file itself is present and hash-stable. The failure
is the host's library set, not a missing or corrupted artifact.

Remedy is re-running `scripts/rebuild-ffmpeg-lgpl.sh` and committing the
relinked sidecar, or installing the matching majors. Doing either is **not**
part of P02 — this task records the baseline, and a repair would change the
thing being recorded.

### Known limits of this baseline

- No packaged-desktop evidence at this commit. `build:desktop` would in any case
  produce an unsigned local verification package, which is not release
  acceptance.
- `render.engine.export` and `export.mp4.export` remain unmeasured; they need
  the WebGPU engine in a browser or the packaged app.
- WebGPU availability is not observable from node; `test:browser` passing shows
  the browser harness, not the packaged app's GPU path.
- `test:coverage` is not part of the `full` profile and has no entry here.
- The manifest records this commit. Once it is committed, `--check` reports
  `source.head` as moved and exits `2`. That is the intended behavior — re-adopt
  deliberately rather than let a stale baseline answer for a tree it never saw.

## Historical R03 baseline acceptance

Retained unchanged as evidence for [R03](https://github.com/mysteropodes/nemo/issues/899)
and [F0](https://github.com/mysteropodes/nemo/issues/888) at its own revision. It
does not close the later application extraction, shared-command, native-isolation
or installed MCP gates, and it is not the current comparison baseline.

This candidate combines the retained R03 inventory, fixture corpus, initial CPU
workloads, and browser harnesses on main `b86c69dda2a061ef551fc522dd96327a5b9a4f66`.
The existing R08 extraction remains separate. Application JavaScript, Rust, WASM,
and packaged sidecar bytes are unchanged from that base.

### Reproduction

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

### Standard verification

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

### CPU workload evidence

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

### Browser and native evidence

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

### Remaining acceptance

Browser document round trips do not exercise browser Save/download UI. Software
rendering does not establish fixture pixel equality, native GPU behavior, or the
application's complete export pipeline. The CPU benchmark keeps unavailable
render/export workloads explicit; its separate software-render receipt does not
turn the native export entry into a pass. Full R03 closure requires reconciliation
of these outstanding fixture and measurement gates. F0 permits their explicit
fail/blocked/not-run disposition while retaining the identified reproducible baseline.
