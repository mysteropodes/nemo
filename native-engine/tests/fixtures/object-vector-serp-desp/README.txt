object-vector-serp-desp -- characterization oracle for the baseline stroke normalization desP(d) -> serP(path)
(G14/G22/G13, lane 01, leaf L01-C of #2107, package #2106)

What it is
  oracle.txt holds 65 rows (5 `meta`, 60 `case`) of text, one row per line, `sd1 <meta|case> <id> <value>`.
  A case row is {"op","in","paper","out","out2"}:
    in     the literal stroke dictionary handed to desP (undefined-valued members are kept as "k":!)
    op     the optional third desP argument (opacity override), ! when not given
    paper  the state of the Paper item desP rebuilt (colors as colorHex8 text, stroke style, closed, data keys,
           and whether the pressure/paramShape objects were stored by reference)
    out    serP(path) -- the normalised dictionary (undefined-valued members omitted)
    out2   serP(desP(out)) -- the same pair applied again (round-trip stability)
  Value grammar: ! undefined, ^ null, T/F, n<16 hex> IEEE 754 bits of a number, JSON strings, [..], {"k":v,..}.

Baseline and provenance
  Baseline commit   fbe4b8a33e6d5a229f014154df2ba21988a9fabc
  Executed verbatim in a Node vm (only a canvas/document stub and the `userLayers`/`state` globals are added):
    src/js/app.js          colorHex8 (:551), applyBrushKeyline (:591), _r3 (:608), serP (:609), desP (:846)
    src/paper-full.min.js  the shipped Paper.js build -- Path, Segment, Point, Layer, Color are the real ones
  Recorded as meta rows and pinned by the Rust test: the anchor lines above, the SHA-256 of each executed slice,
  and the 81 member names serP can return (in order).
  app.js blob 816979f85fd8bfe583c52e889bd1f0733cb5db2a, paper-full.min.js blob 11a760c45796d402a191f1a9cf8c650130699fcb,
  path-fx.js blob bcc09a28387279b5d54a81813f4dffb465d94160 (pathFx is a LAYER field: path-fx.js:185 activeStack reads
  ld.pathFx; app.js:3400/3651/3909 copy it with the other layer fields; serP has no pathFx member).

Regenerate (reads baseline text through `git show`, writes only the directory named; no network, no repo test file)
  node tools/gen.cjs [outDir]
  generator sha256   3caaf392074a6b0cb18dfab0483f966c42c27840ff8d2209574d47270321b4c1
  oracle.txt sha256  f0803f02ed889d38523bc3447c1a767360411d38ae5c073903f5b5f3ffbadfdb  (158557 bytes)
  The generator lives in the leaf's tools/ directory and is not part of the repository tree.

What the cases cover
  open/closed geometry (closure independent of the segments, empty and single-anchor paths, relative handles,
  3-decimal rounding with -0, non-finite coordinates); stroke (hasRealStroke vs the white fallback, legacy
  records, alpha colors, width 0/absent -> 3, cap/join defaults, miter limit, dash, paintOrder, opacity with and
  without the `op` argument, fill null/absent/empty/short/alpha); pressure ribbons (isVectorBrush with
  centerSegments/widthProfile/strokeProfile/profileBase carried by reference, isFillShape, non-ribbon dictionaries
  dropping those members, bitmapPressureProfile as the only pressure-named member and a lane-7 field);
  paramShape and boxAngle by reference; a pathFx member on a stroke dictionary being dropped; strokeId/groupId.

Not characterized (stub): gen.cjs runs serP/desP with userLayers=[] and state={layers:[]}, so layer-channel (fill/shadow)
and brushTexturePreset/isBrushTextureCopy anchor branches are not covered.

Not in this oracle (consumers of later leaves assert them): native record admission, codec errors, transactions,
history/replay, scene/export rendering, the JS-side import mapper.
