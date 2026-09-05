#!/usr/bin/env node
'use strict';
// Deterministic fixture corpus for Nemo (work package R03).
//
// Every fixture is BUILT by code from a seed, never edited by hand, so the
// corpus can be regenerated and `--check` proves the committed files are
// exactly what this generator writes. Each manifest entry records generation
// (script, version, seed), the SHA-256 and size of the file, the areas of the
// surface inventory it covers, the capabilities/backend it needs to be
// exercised, tolerances, and INDEPENDENT expectations: values derived by hand
// from the fixture's own definition (geometry, the documented placement rule,
// the ease curve's waypoints), not produced by running Nemo. Tests then hold
// the shipped code to those expectations where it can run under Node
// (tests/nemo-fixtures.test.cjs); the browser/desktop harnesses (R12/R13/R21)
// take the rest.
//
//   node tests/fixtures/generate.cjs            # regenerate projects/, interaction/, manifest.json
//   node tests/fixtures/generate.cjs --check    # exit 1 when a committed file differs
//   node tests/fixtures/generate.cjs --json     # manifest to stdout
const fs = require('node:fs');
const path = require('node:path');
const L = require('./lib.cjs');

const ROOT = path.resolve(__dirname, '..', '..');
const DIR = __dirname;
const GENERATOR_VERSION = 1;
const SCRIPT = 'tests/fixtures/generate.cjs';

const A = { draw: 'drawing-selection', tl: 'timeline-layers-frames', anim: 'animation-rigs-expressions', proj: 'project-lifecycle-integrations', media: 'media-import-export', fx: 'effects-masks' };
const RED = '#e63946', BLUE = '#1d3557', TEAL = '#2a9d8f', STEEL = '#457b9d';
const PNG_RED = L.tinyPng(2, 2, [230, 57, 70, 255]);
const PNG_TEAL = L.tinyPng(2, 2, [42, 157, 143, 255]);

// ---- independent derivations used by expectations -------------------------
const ease = (t) => { if (!(t in L.DEFAULT_CURVE_WAYPOINTS)) throw new Error('waypoint only'); return L.DEFAULT_CURVE_WAYPOINTS[t]; };
const lerp = (a, b, y) => a.map((x, i) => L.r3(x + (b[i] - x) * y));
// Component instance frame for main frame f: the placement/loop rule as
// documented in README.md (placedAt offset, speed, loop / once / pingpong /
// single). No timing keys, no time remap in these fixtures.
function instanceFrame(total, ld, f) {
  const elapsed = Math.max(0, f - (ld.symPlacedAt || 0)) * (ld.symSpeed || 1);
  if (ld.symPlayMode === 'single') return Math.min(total - 1, Math.max(0, Math.floor(ld.symSingleFrame || 0)));
  if (ld.symPlayMode === 'once') return Math.min(total - 1, Math.floor(elapsed));
  if (ld.symPlayMode === 'pingpong') { if (total < 2) return 0; const cycle = (total - 1) * 2; const pos = Math.floor(elapsed) % cycle; return pos < total ? pos : cycle - pos; }
  return Math.floor(elapsed) % total;
}
// Keyframe inheritance for a plain 2D layer: a non-key, non-interpolated
// frame shows the nearest earlier keyframe (getEffectiveStrokes, app.js).
function effectiveKeyFor(frames, i) { if (frames[i].isKeyframe || frames[i].isInterpolated) return i; for (let k = i - 1; k >= 0; k--) if (frames[k].isKeyframe) return k; return null; }

const NODE = ['node'], APP = ['app-runtime'], ENGINE = ['engine-webgpu'], DESKTOP = ['desktop'];

const FIXTURES = [
  {
    id: 'static-shapes', seed: 101, areas: [A.draw, A.proj], features: ['path', 'fill', 'stroke', 'bezier-handles', 'single-keyframe'], capabilities: NODE, backend: 'none', tolerance: { geometry: 0.001 },
    build() {
      const shapes = [L.rect(40, 40, 80, 60, { fill: BLUE }), L.circle(200, 120, 40, { fill: RED, stroke: '#000000', width: 2 }), L.polygon(120, 180, 30, 3, { stroke: STEEL, width: 4 })];
      return L.doc({ totalFrames: 12, layers: [L.layer('Shapes', L.frames(12, { 0: shapes }))] });
    },
    expectations(d) {
      const s = d.layers[0].frames[0].strokes;
      return { canvas: [320, 240], fps: 12, totalFrames: 12, keyframes: [0], strokeCount: 3, shapes: [
        { index: 0, kind: 'rect', bounds: L.bounds(s[0]), fill: BLUE, stroked: false },
        { index: 1, kind: 'circle', center: [200, 120], radius: 40, bounds: [160, 80, 80, 80], fill: RED, stroked: true, strokeWidth: 2 },
        { index: 2, kind: 'triangle', bounds: L.bounds(s[2]), filled: false, stroked: true, strokeWidth: 4 }] };
    },
    consumers: ['tests/nemo-fixtures.test.cjs', 'R12 render characterization'],
  },
  {
    id: 'keyed-position-default-ease', seed: 102, areas: [A.anim], features: ['motion.position', 'keyframes', 'default-ease', 'clamp-outside-keys'], capabilities: NODE, backend: 'none', tolerance: { value: 1e-6 },
    build() {
      const ld = L.layer('Mover', L.frames(25, { 0: [L.rect(0, 0, 40, 40, { fill: RED })] }), { motion: { position: L.track([{ frame: 0, v: [0, 0] }, { frame: 24, v: [240, 0] }]) } });
      return L.doc({ totalFrames: 25, layers: [ld] });
    },
    expectations() {
      const a = [0, 0], b = [240, 0];
      return { prop: 'position', derivation: 'DEFAULT_CURVE waypoints (0.25→0.156, 0.5→0.5, 0.75→0.844) on the on-curve-waypoint model; frames outside the key range clamp to the nearest key',
        at: { '-5': a, 0: a, 6: lerp(a, b, ease(0.25)), 12: lerp(a, b, ease(0.5)), 18: lerp(a, b, ease(0.75)), 24: b, 30: b },
        defaults: { rotation: [0], scale: [100, 100], opacity: [100], anchor: [0, 0] } };
    },
    consumers: ['tests/nemo-fixtures.test.cjs', 'tests/bench/run.cjs evaluation.rawValueAtFrame'],
  },
  {
    id: 'keyed-position-linear', seed: 103, areas: [A.anim], features: ['motion.position', 'keyframes', 'linear-curve'], capabilities: NODE, backend: 'none', tolerance: { value: 1e-6 },
    build() {
      const ld = L.layer('Slider', L.frames(25, { 0: [L.rect(0, 0, 40, 40, { fill: TEAL })] }), { motion: { position: L.track([{ frame: 0, v: [0, 0], curve: L.LINEAR_CURVE }, { frame: 24, v: [240, 0] }]) } });
      return L.doc({ totalFrames: 25, layers: [ld] });
    },
    expectations() {
      const a = [0, 0], b = [240, 0];
      return { prop: 'position', derivation: 'five collinear curve waypoints define the identity ease, so the value is linear in frame',
        at: { 0: a, 6: lerp(a, b, 0.25), 9: lerp(a, b, 0.375), 12: lerp(a, b, 0.5), 18: lerp(a, b, 0.75), 24: b } };
    },
    consumers: ['tests/nemo-fixtures.test.cjs'],
  },
  {
    id: 'keyed-hold-and-static', seed: 104, areas: [A.anim], features: ['motion.position', 'hold-key', 'motionStatic.opacity', 'property-defaults'], capabilities: NODE, backend: 'none', tolerance: { value: 1e-6 },
    build() {
      const ld = L.layer('Stepper', L.frames(25, { 0: [L.rect(0, 0, 40, 40, { fill: STEEL })] }), {
        motion: { position: L.track([{ frame: 0, v: [0, 0], hold: true }, { frame: 12, v: [100, 0] }, { frame: 24, v: [200, 0] }]) },
        motionStatic: { opacity: [50] },
      });
      return L.doc({ totalFrames: 25, layers: [ld] });
    },
    expectations() {
      return { position: { derivation: 'hold flag on the LEFT key pins the whole segment; the second segment uses the default ease', at: { 0: [0, 0], 5: [0, 0], 11: [0, 0], 12: [100, 0], 18: lerp([100, 0], [200, 0], ease(0.5)), 24: [200, 0] } },
        opacity: { derivation: 'static (unkeyed) value applies at every frame', at: { 0: [50], 12: [50], 24: [50] } },
        defaults: { rotation: [0], scale: [100, 100] } };
    },
    consumers: ['tests/nemo-fixtures.test.cjs'],
  },
  {
    id: 'expression-rotation', seed: 105, areas: [A.anim], features: ['expressions', 'frame-native-vocabulary', 'rotation'], capabilities: APP, backend: 'expression-sandbox', tolerance: { value: 1e-6 },
    build() {
      const ld = L.layer('Spinner', L.frames(25, { 0: [L.rect(140, 100, 40, 40, { fill: RED })] }), { expressions: { rotation: { code: 'frame * 15', enabled: true, lastError: null } } });
      return L.doc({ totalFrames: 25, layers: [ld] });
    },
    expectations() {
      const at = {}; for (let f = 0; f <= 24; f += 6) at[f] = [f * 15];
      return { prop: 'rotation', derivation: 'the expression reads the FRAME-native `frame` binding; 15 degrees per frame', at, note: 'evaluated only by the app runtime (expression sandbox); not runnable under Node' };
    },
    consumers: ['R12/R13 characterization (test:browser)'],
  },
  {
    id: 'held-frames-on-sixes', seed: 106, areas: [A.tl], features: ['2d-animation', 'keyframes-on-sixes', 'held-frames', 'keyframe-inheritance'], capabilities: NODE, backend: 'none', tolerance: {},
    build() {
      const sq = (x) => [L.rect(x, 100, 40, 40, { fill: BLUE })];
      return L.doc({ totalFrames: 24, layers: [L.layer('Walk', L.frames(24, { 0: sq(20), 6: sq(100), 12: sq(180) }))] });
    },
    expectations(d) {
      const fr = d.layers[0].frames; const table = {}; for (let i = 0; i < fr.length; i++) table[i] = effectiveKeyFor(fr, i);
      return { keyframes: [0, 6, 12], derivation: 'a non-key frame shows the nearest earlier keyframe', effectiveKeyframeByFrame: table, rectXByKeyframe: { 0: 20, 6: 100, 12: 180 }, strokesPerEffectiveFrame: 1 };
    },
    consumers: ['tests/nemo-fixtures.test.cjs', 'R12 frame-load characterization'],
  },
  {
    id: 'component-instances', seed: 107, areas: [A.tl, A.anim], features: ['symbols', 'component-instance', 'symPlacedAt', 'loop', 'pingpong', 'once', 'single'], capabilities: NODE, backend: 'none', tolerance: {},
    build() {
      const inner = L.layer('Blink content', L.frames(6, { 0: [L.circle(160, 120, 30, { fill: RED })], 3: [L.circle(160, 120, 50, { fill: TEAL })] }));
      const symbols = { sym_blink: { name: 'Blink', totalFrames: 6, fps: 12, layers: [inner] } };
      const inst = (name, extra) => L.layer(name, L.frames(24), Object.assign({ symbolId: 'sym_blink', symSpeed: 1 }, extra));
      return L.doc({ totalFrames: 24, symbols, layers: [inst('Loop from 3', { symPlacedAt: 3, symPlayMode: 'loop' }), inst('PingPong', { symPlacedAt: 0, symPlayMode: 'pingpong' }), inst('Once from 2', { symPlacedAt: 2, symPlayMode: 'once' }), inst('Single 4', { symPlacedAt: 0, symPlayMode: 'single', symSingleFrame: 4 })] });
    },
    expectations(d) {
      const total = d.symbols.sym_blink.totalFrames; const out = {};
      for (const ld of d.layers) { const t = []; for (let f = 0; f < d.totalFrames; f++) t.push(instanceFrame(total, ld, f)); out[ld.layerUid] = { playMode: ld.symPlayMode, placedAt: ld.symPlacedAt, componentFrameByMainFrame: t }; }
      return { symbolTotalFrames: total, derivation: 'placement rule: elapsed = max(0, f - placedAt) * speed; loop = elapsed mod total; once = min(total-1, elapsed); pingpong over a (total-1)*2 cycle; single = symSingleFrame', instances: out };
    },
    consumers: ['tests/nemo-fixtures.test.cjs', 'StoryBoard/Animation 2D characterization'],
  },
  {
    id: 'matte-alpha', seed: 108, areas: [A.fx], features: ['track-matte', 'matteMode.alpha', 'matteSourceLayerUid', 'alpha-export'], capabilities: ENGINE, backend: 'engine', tolerance: { pixel: 2 / 255 },
    build() {
      const source = L.layer('Matte shape', L.frames(12, { 0: [L.circle(160, 120, 60, { fill: '#000000' })] }));
      const content = L.layer('Content', L.frames(12, { 0: [L.rect(0, 0, 320, 240, { fill: TEAL })] }), { matteMode: 'alpha', matteSourceLayerUid: source.layerUid });
      return L.doc({ totalFrames: 12, layers: [source, content] });
    },
    expectations(d) {
      return { matteMode: 'alpha', sourceLayerUid: d.layers[0].layerUid, structural: { sourceResolves: true },
        pixels: { derivation: 'content is visible only where the matte shape has alpha', inside: { at: [160, 120], color: TEAL }, outside: { at: [20, 20], color: '#ffffff' } }, note: 'pixel expectations are for the engine render / PNG export (R12/R21)' };
    },
    consumers: ['R12 render characterization', 'R21 packaged export'],
  },
  {
    id: 'text-root', seed: 109, areas: [A.draw], features: ['vector-text', 'isTextRoot', 'glyph-paths', 'isTextLayer'], capabilities: NODE, backend: 'none', tolerance: {},
    build() {
      const text = 'Nemo';
      const root = L.shape([[40, 140]], { closed: false, extra: { isText: true, isTextRoot: true, text, vectorFont: 'Roboto-Regular', textSize: 48, textColor: '#000000', textAlign: 'left' } });
      const glyphs = text.split('').map((ch, i) => L.rect(40 + i * 30, 100, 24, 40, { fill: '#000000', extra: { isVectorText: true, vectorChar: ch, charIndex: i, wordIndex: 0, lineIndex: 0 } }));
      return L.doc({ totalFrames: 12, layers: [L.layer('Text', L.frames(12, { 0: [root].concat(glyphs) }), { isTextLayer: true })] });
    },
    expectations() { return { text: 'Nemo', font: 'Roboto-Regular', size: 48, glyphCount: 4, chars: ['N', 'e', 'm', 'o'], rootIsFirstStroke: true }; },
    consumers: ['tests/nemo-fixtures.test.cjs', 'R12 text characterization'],
  },
  {
    id: 'image-mesh', seed: 110, areas: [A.media, A.anim], features: ['raster', 'image-mesh', 'meshId-on-all-frames', 'normalized-mesh-space', 'vertex-offset'], capabilities: NODE, backend: 'none', tolerance: { geometry: 1e-9 },
    build() {
      const mesh = L.gridMesh(4, 4);
      const moved = 16 + 1 * 3 + 1; // interior grid point (gx=2, gy=2): the outline's 16 points come first
      mesh.offsets[moved] = [0.2, 0];
      const keyed = {}; for (let f = 0; f < 24; f++) keyed[f] = [L.raster(PNG_RED, 160, 120, 200, 100, { meshId: 'im_fixture' })];
      return L.doc({ totalFrames: 24, layers: [L.layer('Photo', L.frames(24, keyed))], extra: { imageMeshes: { im_fixture: mesh } } });
    },
    expectations(d) {
      const m = d.imageMeshes.im_fixture;
      return { meshId: 'im_fixture', framesTagged: 24, outlineCount: 16, vertexCount: 25, triangleCount: m.tris.length / 3, allVerticesNormalized: true, movedVertex: { index: 20, offset: [0.2, 0], worldDisplacementPx: [0.2 * 200, 0], derivation: 'offset is a fraction of the raster display size (200×100)' } };
    },
    consumers: ['tests/nemo-fixtures.test.cjs', 'R12 mesh render characterization'],
  },
  {
    id: 'media-raster', seed: 111, areas: [A.media], features: ['raster', 'embedded-png', 'same-dict-every-frame'], capabilities: NODE, backend: 'none', tolerance: {},
    build() {
      const keyed = {}; for (let f = 0; f < 12; f++) keyed[f] = [L.raster(PNG_TEAL, 160, 120, 160, 160)];
      return L.doc({ totalFrames: 12, layers: [L.layer('Image', L.frames(12, keyed))] });
    },
    expectations() { return { frames: 12, sameSourceEveryFrame: true, sourceSha256: L.sha256(PNG_TEAL), bounds: [80, 40, 160, 160], sourceKind: 'data:image/png' }; },
    consumers: ['tests/nemo-fixtures.test.cjs', 'R12/R21 media characterization'],
  },
  {
    id: 'migration-legacy-frames-only', seed: 112, areas: [A.proj], features: ['legacy-schema', 'frames-without-layers', 'importJSON-defaults'], capabilities: APP, backend: 'none', tolerance: {},
    build() { return { frames: L.frames(6, { 0: [L.rect(40, 40, 80, 60, { fill: RED })] }) }; },
    expectations() { return { legacyShape: 'no version, no layers, frames at top level', afterImport: { layers: 1, layerName: 'Layer 1', totalFrames: 6, fps: 12, canvasW: 1920, canvasH: 1080, canvasBg: '#ffffff', waIn: 0, waOut: 5 }, derivation: 'the defaults importJSON fills when a field is absent' }; },
    consumers: ['R12 load characterization (test:browser)'],
  },
  {
    id: 'export-12f-320x240', seed: 113, areas: [A.media, A.tl], features: ['export', 'png-sequence', 'mp4', 'per-frame-content', 'known-fixture'], capabilities: DESKTOP.concat(ENGINE), backend: 'engine+ffmpeg-sidecar', tolerance: { pixel: 8 / 255, note: 'yuv420p chroma subsampling; probe pixels sit 20px inside edges' },
    build() {
      const keyed = {}; for (let i = 0; i < 12; i++) keyed[i] = [L.rect(20 * i, 100, 40, 40, { fill: RED })];
      return L.doc({ totalFrames: 12, fps: 12, layers: [L.layer('Square', L.frames(12, keyed))] });
    },
    expectations(d) {
      const perFrame = d.layers[0].frames.map((f, i) => ({ frame: i, squareBounds: L.bounds(f.strokes[0]), probeInside: { at: [20 * i + 20, 120], color: RED }, probeOutside: { at: [300, 20], color: '#ffffff' } }));
      return { frames: 12, width: 320, height: 240, fps: 12, perFrame, mp4: { codec: 'h264', pixFmt: 'yuv420p', frames: 12, durationSeconds: 1 }, derivation: 'the square advances 20px per frame; probes avoid edges by 20px' };
    },
    consumers: ['R04/R21 packaged desktop export (the known fixture)', 'tests/bench/run.cjs render/export workloads (declared)'],
  },
];

const INTERACTIONS = [{
  id: 'pen-stroke', file: 'interaction/pen-stroke.json', areas: [A.draw], features: ['pen', 'pointer-script', 'undo'], capabilities: ['browser-harness'], backend: 'none', tolerance: { geometry: 2 },
  build() {
    const pointer = [{ t: 0, type: 'down', x: 40, y: 120, pressure: 0.5 }];
    for (let i = 1; i <= 8; i++) pointer.push({ t: i * 16, type: 'move', x: 40 + i * 25, y: 120 - Math.sin(i / 8 * Math.PI) * 40, pressure: 0.6 });
    pointer.push({ t: 160, type: 'up', x: 240, y: 120, pressure: 0 });
    return { schema: 'nemo.interaction/1', id: 'pen-stroke', tool: 'pen', canvas: { w: 320, h: 240 }, layer: 0, frame: 0, pointer, expect: { strokesAdded: 1, minSegments: 3, boundsWithin: [30, 70, 220, 60], undoRestoresStrokeCount: 0 } };
  },
  expectations(s) { return s.expect; },
  consumers: ['R12/R13 browser harness (test:browser)'],
}];

const GENERATED_SCALE = [{ id: 'scale-500', strokes: 500, seed: 9001 }, { id: 'scale-2000', strokes: 2000, seed: 9002 }, { id: 'scale-4000', strokes: 4000, seed: 9003 }];

function buildFixture(id) { const f = FIXTURES.find((x) => x.id === id); if (!f) throw new Error('unknown fixture ' + id); return f.build(); }

function render() {
  const files = {};
  const entries = [];
  const one = (f, kind, rel, data) => {
    const text = JSON.stringify(data, null, 1) + '\n';
    files[rel] = text;
    entries.push({ id: f.id, kind, file: rel, schemaVersion: kind === 'project' ? (data.version || null) : (data.schema || null), areas: f.areas, features: f.features,
      generation: { script: SCRIPT, generatorVersion: GENERATOR_VERSION, seed: f.seed === undefined ? null : f.seed }, sha256: L.sha256(text), bytes: Buffer.byteLength(text),
      capabilities: f.capabilities, backend: f.backend, tolerance: f.tolerance, expectations: f.expectations(data), consumers: f.consumers });
  };
  for (const f of FIXTURES) one(f, 'project', 'projects/' + f.id + '.json', f.build());
  for (const f of INTERACTIONS) one(f, 'interaction', f.file, f.build());
  const generated = GENERATED_SCALE.map((g) => { const text = JSON.stringify(L.scaleDocument(g.strokes, g.seed)); return { id: g.id, kind: 'generated', committed: false, strokes: g.strokes, canvas: [1920, 1080], generation: { script: 'tests/fixtures/lib.cjs scaleDocument', generatorVersion: GENERATOR_VERSION, seed: g.seed }, sha256: L.sha256(text), bytes: text.length, consumers: ['tests/bench/run.cjs copy.* and memory.* workloads'] }; });
  const manifest = { schema: 'nemo.fixtures/1', generator: SCRIPT, generatorVersion: GENERATOR_VERSION, note: 'Generated; do not edit. `node tests/fixtures/generate.cjs --check` fails when a committed file differs from what the generator writes.', fixtures: entries, generated };
  files['manifest.json'] = JSON.stringify(manifest, null, 2) + '\n';
  return { files, manifest };
}

function staleFiles(files) {
  const stale = [];
  for (const [rel, text] of Object.entries(files)) {
    const p = path.join(DIR, rel);
    if (!fs.existsSync(p)) { stale.push(rel + ' (missing)'); continue; }
    if (fs.readFileSync(p, 'utf8') !== text) stale.push(rel);
  }
  return stale;
}

function main() {
  const args = process.argv.slice(2);
  const { files, manifest } = render();
  if (args.includes('--json')) { process.stdout.write(files['manifest.json']); return; }
  if (args.includes('--check')) {
    const stale = staleFiles(files);
    if (stale.length) { console.error('fixtures stale: ' + stale.join(', ') + ' — run `node tests/fixtures/generate.cjs`'); process.exit(1); }
    console.log('fixtures up to date (' + manifest.fixtures.length + ' committed, ' + manifest.generated.length + ' generated on demand)');
    return;
  }
  for (const [rel, text] of Object.entries(files)) { const p = path.join(DIR, rel); fs.mkdirSync(path.dirname(p), { recursive: true }); fs.writeFileSync(p, text); }
  console.log('fixtures: ' + manifest.fixtures.length + ' files -> ' + path.relative(ROOT, DIR) + '/ (' + manifest.generated.length + ' scale documents generated on demand)');
}

if (require.main === module) main();
module.exports = { FIXTURES, INTERACTIONS, GENERATED_SCALE, GENERATOR_VERSION, buildFixture, render, staleFiles, instanceFrame, effectiveKeyFor };
