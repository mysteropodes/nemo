# N04 native Tauri viewport proof

An isolated macOS Tauri 2 feasibility app. It creates a pass-through AppKit
`NSView` above its WebView and presents a native Vello fixture through a wgpu
Metal surface. The HTML layer transmits only bounds, revision/generation and
pointer coordinates. It never receives a full viewport image.

Run locally from this directory:

```sh
cargo test
cargo run
cargo build --release
```

After each native presentation, `evidence/desktop-proof.json` records the
observed backend, physical size, presentation/blit counters, zero CPU-readback
and JavaScript-pixel counters, stale-token rejection, reconfigure count, pointer
mapping result, and the AppKit assigned-frame alignment error in device pixels. It
is generated runtime evidence, not a fixture. `evidence/RECEIPT.md` records the
local desktop command outcome and its explicit limits.

Final local desktop receipt: a real CUA click at window-relative `[160,230]`
inside the mint Vello rectangle produced a hit at revision 8/generation 4. The
input→diagnostic-overlay and CSS→AppKit assigned-frame measurements are distinct
and both 0.0 device pixels. The controlled CSS host resize changed the native
surface from 1396x1313 to 1396x1179 at backing scale/DPR 2. The terminal receipt
records 120/120 additional native presents (124 total) with p50/p95 present
intervals of 16.659459000000002/16.873625 ms, one stale-token discard, and one
forced reconfigure-path check. It then disposes exactly once and rejects a
subsequent presentation.

`gen/schemas/**` and `icons/n04.png` are reproducible local Tauri build outputs:
the former is generated from this crate's config and the latter by `build.rs` to
satisfy Tauri's compile-time icon requirement. Neither is an intended candidate
artifact; both are ignored. No production Nemo path is read or modified.

The proof is intentionally not a Nemo project renderer, document owner,
exporter, browser fallback, packaging result or cross-platform claim.
