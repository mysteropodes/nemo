# Contributing to Nemo

Nemo is GPL-3.0-or-later (see [LICENSE](LICENSE)) — the same license family as
Blender, GIMP, Inkscape, and Krita. That choice is deliberate: forks and
commercial redistribution are allowed, but any distributed modification stays
open. There's no CLA — by opening a PR, you agree your contribution is
licensed under the project's license (GPL-3.0-or-later), same as the rest of
the codebase. If that's ever a problem for something you want to contribute,
say so in the PR before it's merged, not after.

## Before you start

- Read [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) if you're touching
  anything that bundles or calls out to third-party code (ffmpeg, vendored
  JS libs, Rust crates) — it documents why things are built the way they are
  license-wise, and what NOT to reintroduce (GPL/patent-encumbered codecs in
  particular).
- Nemo is a hybrid Paper.js (document model) + Rust/wasm (`geometry-wasm/`,
  stateless WebGPU renderer via vello) app running in Tauri. The root
  [CLAUDE.md](CLAUDE.md) is the real engineering guide — it documents
  invariants and past-bug postmortems that aren't obvious from reading the
  code once (e.g. the "new item type/tag handled in one consumer but not
  the others" bug family, which has bitten this codebase repeatedly). It
  was written for an AI assistant but every rule in it applies just as much
  to a human contributor — read it before touching the scene/save pipeline,
  the render engine, or Motion.

## Building locally

Builds and validation run locally by default. Commits, pushes, pull requests, merges,
and version tags must not start GitHub Actions builds. Hosted validation, web/Worker
deployment, and release builds are manual exceptions requiring an explicit human request
for that specific run; do not automatically dispatch or rerun them to satisfy a PR check.
Keep local receipts with the PR. See [the CI policy](engineering/ci/README.md).

```bash
npm install
npm run dev      # tauri dev — desktop build with hot reload
npm run serve    # browser-only preview, no Tauri (python3 -m http.server on src/)
```

Rust/wasm changes under `geometry-wasm/` need `wasm-pack build --target web`
(see that directory) before they show up in either mode.

Before opening a PR, run the named checks and keep the receipt:

```bash
npm run doctor   # read-only: toolchain, capabilities, source/build identity
npm run check    # version sync, JSON, JS syntax, script refs, guards
npm run verify   # doctor + check + npm test + cargo test, one receipt in reports/
```

Each job reports `pass`, `fail`, `blocked` or `not-run` with a reason; a missing tool is
`blocked`, never a silent skip. See [scripts/nemo/README.md](scripts/nemo/README.md).

## Workflow

- Branch per change (`your-topic`), PR against `main`. No direct pushes to
  `main`.
- Keep PRs scoped — one feature/fix per PR is much easier to review than a
  pile of unrelated changes.
- If you're fixing a bug, a short repro (or a failing test if the area has
  test coverage — `run_tests` for Luau-style suites, `cargo test` for Rust)
  makes review much faster than a description alone.

## Reporting bugs / suggesting features

Open a GitHub issue. Screenshots/recordings help a lot for anything visual
(rendering, brush behavior, timeline/UI).
