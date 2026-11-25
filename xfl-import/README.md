# nemo-xfl-import

Standalone reader for Adobe Animate / Flash `.fla` files in the XFL format
(Animate CS5 and later), tracked in #2020. **Post-remediation prototype:
nothing in the app, the native engine or the build depends on this crate.**

`read_fla(bytes)` (zipped `.fla`) or `read_xfl_dir(path)` (uncompressed XFL
folder) return an `XflDocument`: stage size, frame rate and background;
timelines with layers (normal, folder, guide, mask, parent index), keyframe
spans (index + duration), labels and tween settings; elements (shapes,
groups, symbol and bitmap instances, text); the library (symbols with their
own timelines, bitmap items); and an `ImportReport` counting what was read
and listing every feature that is not converted, with its first location.

Shapes are decoded from the XFL edge language into closed fill loops (holes
wound opposite to their outer boundary) and stroke paths. Quadratic curves
stay quadratic in the model; `Segment::to_cubic` gives the exact cubic.
Points are matched on exact fixed-point values (1/256 twip), so edges listed
in any order and in either number form (decimal or `#hex.frac`) close.
A 6-digit hex integer with its high bit set is read as a 24-bit negative;
this is inferred, to be confirmed on real files. Animate's rectangle and
oval primitives become a rectangle or 8-arc ellipse (corner radii, arcs and
inner radii are reported).

Binary `.fla` files from Flash CS4 and earlier are refused with
`XflError::LegacyBinaryFla`. Archives are read under hard limits
(entries, bytes per entry, total bytes, XML nesting depth).

```
cargo test
cargo run --example fla_dump -- path/to/file.fla
```

## Slices (#2020)

1. **This crate:** XFL reading into a neutral model, with the import report. Done here.
2. **Bitmap `.dat` decoding:** done. JPEG/PNG/GIF entries are passed through
   encoded; Animate's lossless ARGB entries (`03 05`) are decoded to straight
   RGBA. The 8-bit palette form (`03 03`) is reported, not decoded. The
   lossless layout follows public XFL references and must be confirmed on
   real files.
3. Mapping onto a Nemo document through the native import path (shape
   islands, colours as hex8, layers and folders, held keyframes, labels as
   markers, masks). Needs a decision with the native-engine owners on the
   entry point, like the SVG/PSD/Figma import plans.
4. Symbols as Components (one instance per layer for now) and classic tweens
   as Motion keys with ease. Model side started: `classic_tweens(timeline, layer_index)`
   finds each tween and `ClassicTween::sample(frame)` gives Flash's
   decomposed transform (pivot position, scales, skews) with rotation options,
   a per-property ease (simple, or the custom ease Bézier solved for time) and
   travel along a motion guide stroke by arc length (`ease.rs`, `guide.rs`).
   Not confirmed against Animate: the orient-to-path rule, the 5 px distance
   under which a keyframe counts as attached to the guide, and named ease
   presets (`<Ease method>`), which fall back to the simple ease.
   Mapping side (`instance.rs`): a scene layer that only places one symbol
   becomes an instance layer of a component (`symbols`, its own layers
   flattened, nested symbols expanded); fixed frames are `componentFrame`
   picks, a playing graphic one `symPlayMode`/`symPlacedAt`, empty spans
   `blankOverride`. The transform is layer Motion with the pivot held on the
   transformation point; simple eases and custom eases with symmetric handles
   are exact `curvePoints`, other custom eases and motion guides are baked one
   key per frame. Skewed instances keep a fixed `symMatrix` or are flattened.
   `ConvertOptions { components: false }` flattens every layer.
5. Validation on a real Animate corpus (PNG export from Animate against
   Nemo's render), then the `.swf` fallback via Ruffle's `swf` crate.

Shape tweens, text, sound, CS4+ motion tweens, filters, tints, blend modes
and variable-width strokes are v2 and only reported today.

Onionskyn (MIT) and xfl2svg (MIT) were read as references for the XFL format;
no code is copied from them.
