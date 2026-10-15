# R18.2 prep (parked, not for merge)

Local preparation by Cyrill's team while waiting for the next R18.2 leaf on #916.
Not claimed, not admitted, no PR. Kept on this branch only so it survives the cloud container.

- `native-engine/src/motion_parent_chain.rs` + test + oracle (508 rows) — parent-chain affine math from `src/js/motion.js` @ 660ac5ef.
- `native-engine/src/expression_controls.rs` + test + oracle (248 rows) — `layerControl` / expression-control reads.
- `oracle-gen/` — Node generators (`jsc-bundle.mjs` builds self-contained JavaScriptCore bundles).
- `registration.diff` / `REGISTRATION.md` — the shared-file registration lines, deliberately NOT applied (those files belong to Ilya's team).

JavaScriptCore check (macOS 27.0.1, 2026-10-15): controls oracle identical to V8; parent-chain oracle differs only in NaN sign and last-ulp trig rows, and the Rust test passes against both outputs.
