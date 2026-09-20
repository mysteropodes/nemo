# N04 local desktop receipt

Base observed before the isolated addition: `c489e5fbb1822f6a816db3a4d8e901d67382b7a2`.

Final commands run from this directory on macOS:

```sh
cargo fmt && cargo test --locked
# passed: 3/3 (stale ordering; backing-scale sizing; overlay mapping)
cargo build --release --locked
# passed
cargo run
# actual Tauri/AppKit/Metal desktop surface launch
```

The corrected final live run used a real CUA click at window-relative `[160,230]`
inside the mint Vello fixture. `terminal-proof.json` records pre- and post-shutdown
evidence at revision 8/generation 4: initial physical surface 1396x1313, actual
CSS-resize surface 1396x1179, and backing scale/reported DPR 2. The compact pointer
input selected the target (`hitTargetSelected:true`, center delta
57.35189924516631 device pixels). Its diagnostic Vello circle applies the same
fixture transform as the scene; its device→fixture→device center error is 0.0
device pixels (`overlayAlignmentDevicePx`). The separately measured AppKit assigned
child-frame error is also 0.0 (`surfaceFrameAlignmentDevicePx`).

The same receipt records 124 native presents and 124 GPU offscreen-to-surface
blits (the initial four plus a bounded 120/120 present burst), p50/p95 presentation
intervals 16.659459000000002/16.873625 ms, zero CPU readbacks, zero JavaScript pixel bytes,
one stale-token discard, one CSS resize reconfigure, and one forced surface-loss
reconfigure-path check. It then drops the surface before removing the NSView,
records exactly one disposal, and rejects a subsequent configure/present request;
the post-shutdown count remains 124.

This is fixture-only feasibility evidence. It is not a packaged bundle, a real
device-loss event, browser fallback, cross-platform implementation, or Nemo
document-render acceptance. `gen/schemas/**` and `icons/n04.png` are reproducible
ignored local build outputs, not candidate artifacts.
