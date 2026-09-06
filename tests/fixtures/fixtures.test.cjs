'use strict';
// R03 fixture corpus verification. Runs every node-verifiable check of
// tests/fixtures/<id>/expected.json against PRODUCTION code loaded through
// tests/fixtures/lib/sandbox.cjs, validates the manifest (hashes, coverage,
// structure) and, when a runnable ffmpeg sidecar is present, the export
// checks. Gated checks (pixel, interaction, importJSON contract) are only
// validated for shape here; the manifest names the gate that will run them.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');

const DIR = __dirname;
const ROOT = path.resolve(DIR, '..', '..');
const sandbox = require('./lib/sandbox.cjs');
const ref = require('./lib/reference.cjs');
const png = require('./lib/png.cjs');
const corpus = require('./lib/corpus.cjs');

const manifest = JSON.parse(fs.readFileSync(path.join(DIR, 'manifest.json'), 'utf8'));
const readJson = (rel) => JSON.parse(fs.readFileSync(path.join(DIR, rel), 'utf8'));
const sha256 = (buf) => crypto.createHash('sha256').update(buf).digest('hex');
// Values produced inside the vm sandbox belong to another realm (their Array/Object
// prototypes differ), which deepStrictEqual treats as a mismatch: clone() before comparing.
const clone = (x) => JSON.parse(JSON.stringify(x));

function getPath(obj, p) {
  const parts = p.replace(/\[(\d+)\]/g, '.$1').split('.').filter(Boolean);
  let cur = obj;
  for (const k of parts) { if (cur == null) return undefined; cur = cur[k]; }
  return cur;
}
function near(actual, expect, tol, msg) {
  assert.equal(actual.length, expect.length, msg + ': dimension');
  for (let i = 0; i < expect.length; i++) assert.ok(Math.abs(actual[i] - expect[i]) <= tol, `${msg}: [${i}] got ${actual[i]}, expected ${expect[i]} (±${tol})`);
}
function layerIndex(project, uid) { const i = project.layers.findIndex((l) => l.layerUid === uid); assert.notEqual(i, -1, 'layer ' + uid); return i; }
function strokeIds(strokes) { return Array.from(strokes, (s) => s.strokeId || (s.isRaster ? 'raster' : '?')); }

// Exercise the public CLI in a fresh process: an incompatible compression build
// must not rewrite assets/manifest or misdiagnose runtime drift as stale fixtures.
test('fixtures generator: incompatible zlib fails before writing or checking assets', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-fixture-runtime-'));
  try {
    const preload = path.join(tmp, 'runtime.cjs');
    fs.writeFileSync(preload, "Object.defineProperty(process.versions, 'zlib', { value: '1.2.12' });\n");
    const out = path.join(tmp, 'output');
    for (const args of [['--out', out], ['--check']]) {
      const r = spawnSync(process.execPath, ['--require', preload, path.join(DIR, 'generate.cjs'), ...args], { encoding: 'utf8' });
      assert.equal(r.status, 2, r.stdout + r.stderr);
      assert.match(r.stderr, /incompatible fixture generator runtime/);
      assert.match(r.stderr, /Node .*zlib 1\.2\.12/);
      assert.match(r.stderr, /20\.19\.4/);
      assert.doesNotMatch(r.stderr, /fixtures stale|run `npm run fixtures`/);
      assert.equal(fs.existsSync(out), false, 'runtime rejection wrote fixture files');
    }
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

// ---- manifest ----------------------------------------------------------------

test('fixtures manifest: schema, unique ids, required coverage', () => {
  assert.equal(manifest.schema, 'nemo.fixtures/1');
  assert.equal(manifest.formatVersion, corpus.FORMAT_VERSION);
  const ids = manifest.fixtures.map((f) => f.id);
  assert.equal(new Set(ids).size, ids.length, 'duplicate fixture ids');
  for (const tag of manifest.requiredCoverage) assert.ok(manifest.fixtures.some((f) => f.coverage.includes(tag)), `no fixture covers "${tag}"`);
  for (const f of manifest.fixtures) {
    for (const k of ['title', 'coverage', 'areas', 'backend', 'requiredCapabilities', 'tolerance', 'seed', 'generation', 'invariants', 'verification', 'files', 'sha256', 'checks']) assert.ok(k in f, `${f.id}: manifest field ${k}`);
    assert.ok(['document', 'engine', 'sidecar', 'browser'].includes(f.backend), `${f.id}: backend ${f.backend}`);
    assert.ok(f.invariants.length >= 1, `${f.id}: invariants`);
  }
  for (const w of manifest.workloads) for (const k of ['generator', 'params', 'seed', 'sha256', 'bytes']) assert.ok(k in w, `${w.id}: workload field ${k}`);
});

test('fixtures manifest: every committed file matches its recorded hash', () => {
  for (const f of manifest.fixtures) {
    for (const [rel, hash] of Object.entries(f.sha256)) {
      const p = path.join(DIR, rel);
      assert.ok(fs.existsSync(p), `${rel} missing`);
      assert.equal(sha256(fs.readFileSync(p)), hash, `${rel}: hash differs from manifest — regenerate with npm run fixtures or revert the hand edit`);
    }
    assert.ok(f.sha256[f.files.project] && f.sha256[f.files.expected], `${f.id}: project/expected hashed`);
    for (const a of f.files.assets) assert.ok(f.sha256[a], `${f.id}: asset ${a} hashed`);
  }
});

test('fixtures manifest: areas exist in the surface inventory', () => {
  const inv = path.join(ROOT, 'engineering', 'inventory', 'surfaces.json');
  if (!fs.existsSync(inv)) return;
  const areas = Object.keys(JSON.parse(fs.readFileSync(inv, 'utf8')).counts.byArea);
  for (const f of manifest.fixtures) for (const a of f.areas) assert.ok(areas.includes(a), `${f.id}: unknown inventory area "${a}"`);
});

test('fixtures: every project satisfies the importJSON structural preconditions', () => {
  for (const f of manifest.fixtures) {
    const p = readJson(f.files.project);
    assert.ok(Array.isArray(p.layers) && p.layers.length, `${f.id}: layers`);
    assert.ok(p.version <= corpus.FORMAT_VERSION, `${f.id}: version`);
    p.layers.forEach((ld, li) => {
      assert.ok(Array.isArray(ld.frames), `${f.id}: layer ${li} frames`);
      ld.frames.forEach((fr, fi) => assert.ok(fr && Array.isArray(fr.strokes), `${f.id}: layer ${li} frame ${fi} strokes`));
      assert.ok(ld.frames[0].isKeyframe, `${f.id}: layer ${li} frame 0 is a keyframe`);
    });
    const uids = p.layers.map((l) => l.layerUid).filter(Boolean);
    assert.equal(new Set(uids).size, uids.length, `${f.id}: duplicate layerUid`);
    const sids = [];
    for (const ld of p.layers) for (const fr of ld.frames) for (const s of fr.strokes) if (s.strokeId) sids.push(s.strokeId);
    assert.equal(new Set(sids).size, sids.length, `${f.id}: duplicate strokeId`);
  }
});

// ---- per-fixture checks against production code --------------------------------

function loadFixture(f) {
  const project = readJson(f.files.project);
  const expected = readJson(f.files.expected);
  const state = sandbox.defaultState();
  state.layers = clone(project.layers); state.symbols = clone(project.symbols || {});
  state.fps = project.fps; state.totalFrames = project.totalFrames; state.canvasW = project.canvasW; state.canvasH = project.canvasH;
  state.imageMeshes = clone(project.imageMeshes || {}); state.exprGlobals = project.exprGlobals || '';
  const motion = sandbox.loadMotion(state);
  const app = sandbox.loadAppHelpers(state, motion);
  return { project, expected, state, motion, app };
}

const EVALUATORS = {
  'motion-value'(c, fx) {
    const ld = fx.state.layers[layerIndex(fx.project, c.layerUid)];
    const v = fx.motion.SMMotion.valueAtFrame(ld, c.prop, c.frame);
    near(v, c.expect, c.tolerance, `${c.layerUid}.${c.prop}@${c.frame}${c.note ? ' (' + c.note + ')' : ''}`);
  },
  'expression-error'(c, fx) {
    const ld = fx.state.layers[layerIndex(fx.project, c.layerUid)];
    fx.motion.SMMotion.valueAtFrame(ld, c.prop, 3);
    const err = ld.expressions[c.prop].lastError;
    if (c.expectError) assert.ok(err, `${c.layerUid}.${c.prop}: expected lastError`); else assert.ok(!err, `${c.layerUid}.${c.prop}: unexpected lastError ${err}`);
  },
  'effective-keyframe'(c, fx) {
    const li = layerIndex(fx.project, c.layerUid);
    const got = strokeIds(fx.app.getEffectiveStrokes(li, c.frame));
    const want = c.expectKeyframe === null ? [] : strokeIds(fx.project.layers[li].frames[c.expectKeyframe].strokes);
    assert.deepEqual(got, want, `${c.layerUid}@${c.frame}: ${c.note || ''}`);
  },
  'symbol-frame'(c, fx) {
    const li = layerIndex(fx.project, c.layerUid);
    const ld = fx.state.layers[li];
    if (c.expectInternalFrame !== null) assert.equal(fx.app.resolveSymbolFrameIdx(fx.state.symbols[ld.symbolId], ld, c.frame), c.expectInternalFrame, `${c.layerUid}@${c.frame} internal frame (${c.note})`);
    assert.deepEqual(strokeIds(fx.app.getEffectiveStrokes(li, c.frame)), c.expectStrokeIds, `${c.layerUid}@${c.frame} strokes (${c.note})`);
  },
  migration(c, fx) {
    if (c.verify !== 'node') return;
    if (c.op === 'legacyCurve') {
      const n = fx.motion.SMMotion.migrateLegacyCurves();
      const ks = fx.state.layers[layerIndex(fx.project, c.layerUid)].motion.position.keys;
      assert.equal(n, c.expect.migratedCount, 'migrated key count');
      assert.deepEqual(clone(ks[0].curvePoints), c.expect.key0, 'legacy default replaced by DEFAULT_CURVE');
      assert.deepEqual(clone(ks[1].curvePoints), c.expect.key1, 'hand-edited curve untouched');
    } else if (c.op === 'timeLinkOffsets') {
      const ld = clone(fx.project.layers[layerIndex(fx.project, c.layerUid)]);
      fx.app.migrateTimeLinkOffsets(ld);
      assert.deepEqual(clone(ld.motionStatic), c.expect);
    } else if (c.op === 'effectorChannels') {
      const eff = clone(fx.project.layers[layerIndex(fx.project, c.layerUid)].duplicator.effectors[0]);
      assert.deepEqual(clone(fx.app.effectorChannels(eff)), c.expect);
      assert.deepEqual(clone(eff.channels), c.expect, 'migration is written back in place');
    } else assert.fail('unknown node migration op ' + c.op);
  },
  bounds(c, fx) {
    const li = layerIndex(fx.project, c.layerUid);
    let sd = null;
    for (const fr of fx.project.layers[li].frames) for (const s of fr.strokes) if (s.strokeId === c.strokeId) sd = s;
    assert.ok(sd, 'stroke ' + c.strokeId);
    assert.deepEqual(ref.pointsBounds(sd.segments.map((s) => s.point)), c.expect);
  },
  document(c, fx) { assert.deepEqual(getPath(fx.project, c.path), c.expect, `${c.path}${c.note ? ' (' + c.note + ')' : ''}`); },
  alpha(c) { assert.deepEqual(ref.hexToRgba(c.hex), c.expect); },
  mesh(c, fx) {
    const m = fx.project.imageMeshes[c.meshId];
    assert.ok(m, 'mesh ' + c.meshId);
    assert.equal(m.outline.length, c.outlineCount); assert.equal(m.verts.length, c.vertexCount); assert.equal(m.tris.length, c.triangleCount * 3);
    assert.equal(m.offsets.length, m.verts.length, 'offsets aligned with verts');
    for (let i = 0; i < m.outline.length; i++) assert.deepEqual(m.verts[i], m.outline[i], `verts[${i}] is outline[${i}]`);
    for (let t = 0; t < m.tris.length; t += 3) {
      const tri = [m.tris[t], m.tris[t + 1], m.tris[t + 2]];
      for (const i of tri) assert.ok(Number.isInteger(i) && i >= 0 && i < m.verts.length, `triangle index ${i}`);
      const cx = (m.verts[tri[0]][0] + m.verts[tri[1]][0] + m.verts[tri[2]][0]) / 3, cy = (m.verts[tri[0]][1] + m.verts[tri[1]][1] + m.verts[tri[2]][1]) / 3;
      assert.ok(ref.pointInPolygon(m.outline, cx, cy), `triangle ${t / 3} centroid inside the outline`);
    }
    // The production module accepts the entry as-is.
    const im = sandbox.loadImageMesh(fx.state);
    im.SMImageMesh.load({ [c.meshId]: clone(m) });
    const loaded = im.SMImageMesh.get(c.meshId);
    assert.ok(loaded && loaded.verts.length === m.verts.length, 'SMImageMesh.load keeps the topology');
  },
  'mesh-vertex-world'(c, fx) {
    const m = fx.project.imageMeshes[c.meshId];
    const [u, v] = m.verts[c.index];
    near(ref.meshVertexWorld(c.rect, u, v), c.expectRest, 1e-9, 'rest');
    near(ref.meshVertexWorld(c.rect, u + m.offsets[c.index][0], v + m.offsets[c.index][1]), c.expectDeformed, 1e-9, 'deformed');
    // Production mapping (scene payload rounds to 2 dp; its rect is top-left based).
    const im = sandbox.loadImageMesh(fx.state);
    im.SMImageMesh.load({ [c.meshId]: clone(m) });
    const payload = im.SMImageMesh.scenePayloadFor(c.meshId, { x: c.rect.x - c.rect.width / 2, y: c.rect.y - c.rect.height / 2, width: c.rect.width, height: c.rect.height, rotation: 0 }, null);
    assert.ok(payload && payload.verts, 'scenePayloadFor');
    near(payload.verts[c.index], c.expectDeformed, 0.005, 'SMImageMesh.scenePayloadFor world vertex');
  },
  'mesh-propagated'(c, fx) {
    const ld = fx.project.layers[layerIndex(fx.project, c.layerUid)];
    ld.frames.forEach((fr, fi) => { const r = fr.strokes.find((s) => s.isRaster); assert.ok(r && r.meshId === c.meshId, `frame ${fi} raster carries meshId`); });
  },
  'text-group'(c, fx) {
    // desP restores these fields from the dict onto path.data (src/js/app.js desP:
    // `if(d.groupId)p.data.groupId=d.groupId`, `if(d.isVectorText)p.data.isVectorText=true`,
    // `if(d.isTextRoot)…`); the lifted vectorTextGroupMembers reads data.groupId
    // and the parent's children, which is all a stroke list has to provide.
    const ld = fx.project.layers[layerIndex(fx.project, c.layerUid)];
    const parent = { children: [] };
    for (const d of ld.frames[0].strokes) {
      const data = {};
      if (d.groupId) data.groupId = d.groupId;
      if (d.isVectorText) data.isVectorText = true;
      if (d.isTextRoot) data.isTextRoot = true;
      if (d.vectorChar) data.vectorChar = d.vectorChar;
      parent.children.push({ strokeId: d.strokeId, data, parent });
    }
    const root = parent.children.find((p) => p.strokeId === c.rootStrokeId);
    assert.ok(root && root.data.isTextRoot, 'root ' + c.rootStrokeId + ' carries isTextRoot');
    assert.equal(parent.children.indexOf(root), 0, 'the root is the first glyph of the block');
    const members = Array.from(sandbox.loadVectorTextGroup().vectorTextGroupMembers(root), (p) => p.strokeId);
    assert.deepEqual(members, c.expectMembers, 'vectorTextGroupMembers over the restored block');
  },
  'text-units'(c, fx) {
    const sel = sandbox.loadTextSelector();
    const ld = fx.project.layers[layerIndex(fx.project, c.layerUid)];
    const got = ld.frames[0].strokes.filter((s) => s.isVectorText).map((s) => sel.unitIndexOf(s, 'chars'));
    assert.deepEqual(got, c.expectCharIndices);
  },
  'data-url-asset'(c, fx, f) {
    const ld = fx.project.layers[layerIndex(fx.project, c.layerUid)];
    const r = ld.frames[0].strokes.find((s) => s.isRaster);
    const m = /^data:image\/png;base64,(.+)$/.exec(r.src);
    assert.ok(m, 'png data URL');
    assert.ok(Buffer.from(m[1], 'base64').equals(fs.readFileSync(path.join(DIR, f.id, c.asset))), 'data URL bytes equal the committed asset');
  },
  'image-size'(c, fx, f) {
    const img = png.decode(fs.readFileSync(path.join(DIR, f.id, c.asset)));
    assert.deepEqual({ width: img.width, height: img.height }, c.expect);
  },
  'frame-pixels'(c, fx, f) {
    const img = png.decode(fs.readFileSync(path.join(DIR, f.id, c.asset)));
    for (const s of c.samples) assert.deepEqual(png.pixelAt(img, s.x, s.y).slice(0, 3), s.rgb, `${c.asset} (${s.x},${s.y})`);
  },
  'interaction-schema'(c, fx, f) {
    const g = readJson(`${f.id}/${c.file}`);
    assert.equal(g.schema, 'nemo.interaction/1');
    const draw = g.steps.find((s) => s.step === 'draw');
    assert.equal(draw.events.length, c.expectEvents);
    assert.equal(draw.events[0].type, 'pointerdown'); assert.equal(draw.events[draw.events.length - 1].type, 'pointerup');
    let t = -1;
    for (const e of draw.events) {
      assert.ok(e.t >= t, 'timestamps monotonic'); t = e.t;
      assert.ok(e.x >= 0 && e.x <= g.canvas.width && e.y >= 0 && e.y <= g.canvas.height, 'event inside the canvas');
      assert.ok(e.pressure >= 0 && e.pressure <= 1, 'pressure in range');
    }
    // The gated expectation must be consistent with the recording itself.
    const after = fx.expected.checks.find((k) => k.kind === 'interaction' && k.after === 'draw');
    const b = ref.pointsBounds(draw.events.map((e) => [e.x, e.y])), w = after.expect.boundsWithin;
    assert.ok(w.x <= b.x && w.y <= b.y && w.x + w.width >= b.x + b.width && w.y + w.height >= b.y + b.height, 'expected bounds contain the recorded gesture');
  },
  interaction() {},
  pixel(c, fx) {
    assert.ok(Number.isInteger(c.x) && Number.isInteger(c.y) && c.x >= 0 && c.x < fx.project.canvasW && c.y >= 0 && c.y < fx.project.canvasH, `pixel (${c.x},${c.y}) inside the canvas`);
    assert.ok(c.rgb.length === 3 && c.rgb.every((v) => Number.isInteger(v) && v >= 0 && v <= 255), 'rgb bytes');
    assert.ok(c.frame >= 0 && c.frame < fx.project.totalFrames, 'frame in range');
    assert.ok(c.gate, 'gate named');
  },
  encode() {}, 'decode-sample'() {},
};

for (const f of manifest.fixtures) {
  test(`fixture ${f.id}: ${f.checks.node} node-verifiable checks against production code`, () => {
    const fx = loadFixture(f);
    assert.equal(fx.expected.fixture, f.id);
    let ran = 0;
    for (const c of fx.expected.checks) {
      const ev = EVALUATORS[c.kind];
      assert.ok(ev, `${f.id}: no evaluator for check kind "${c.kind}"`);
      if (c.verify === 'gate') { assert.ok(c.gate, `${f.id}: gated check without a gate name`); if (c.kind === 'pixel') ev(c, fx, f); continue; }
      if (c.verify === 'node-when-available') continue;
      ev(c, fx, f); ran++;
    }
    assert.equal(ran, f.checks.node, 'every node check ran');
  });
}

// ---- export: encode with a runnable sidecar when one is present -----------------

function findFfmpeg() {
  if (process.env.NEMO_FFMPEG) return { bin: process.env.NEMO_FFMPEG, origin: 'NEMO_FFMPEG override' };
  const { hostTriple } = require(path.join(ROOT, 'scripts', 'nemo', 'lib', 'identity.cjs'));
  const triple = hostTriple();
  if (!triple) return { bin: null, reason: 'rustc missing: host triple unknown, committed sidecar cannot be named' };
  const bin = path.join(ROOT, 'src-tauri', 'binaries', 'ffmpeg-' + triple);
  if (!fs.existsSync(bin)) return { bin: null, reason: `committed sidecar ${path.relative(ROOT, bin)} absent` };
  const r = spawnSync(bin, ['-version'], { encoding: 'utf8', timeout: 20000 });
  if (r.status !== 0) return { bin: null, reason: `committed sidecar does not run (${(r.stderr || '').split('\n')[0] || 'exit ' + r.status}); set NEMO_FFMPEG to an explicit binary to run the export checks` };
  return { bin, origin: 'committed sidecar' };
}

test('fixture export: encode with the exact export.js argument lists and probe the outputs', (t) => {
  const f = manifest.fixtures.find((x) => x.id === 'export');
  const found = findFfmpeg();
  if (!found.bin) { t.skip(found.reason); return; }
  const ffprobe = spawnSync('ffprobe', ['-version'], { encoding: 'utf8' }).status === 0 ? 'ffprobe' : null;
  const encoders = spawnSync(found.bin, ['-hide_banner', '-encoders'], { encoding: 'utf8' }).stdout || '';
  const identity = { binary: found.bin, origin: found.origin, sha256: sha256(fs.readFileSync(found.bin)), version: (spawnSync(found.bin, ['-version'], { encoding: 'utf8' }).stdout || '').split('\n')[0] };
  t.diagnostic(`ffmpeg: ${identity.origin} ${identity.sha256.slice(0, 12)} ${identity.version}${ffprobe ? '' : ' (no ffprobe on PATH: stream probes skipped)'}`);
  const expected = readJson(f.files.expected);
  const framesDir = path.join(DIR, 'export', 'assets', 'frames');
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-fixture-export-'));
  try {
    const outputs = {};
    for (const c of expected.checks.filter((k) => k.kind === 'encode')) {
      const missing = c.requires.filter((r) => r === 'videotoolbox' ? !/h264_videotoolbox/.test(encoders) : false);
      if (missing.length) { t.diagnostic(`${c.encoder}: skipped, encoder requires ${missing.join(', ')}`); continue; }
      const outBase = path.join(tmp, c.encoder);
      if (c.files) fs.mkdirSync(outBase, { recursive: true });
      const args = c.args.map((a) => a.replace('{fps}', String(c.fps)).replace('{frames}', framesDir).replace('{out}', outBase));
      const r = spawnSync(found.bin, args, { encoding: 'utf8', timeout: 120000 });
      assert.equal(r.status, 0, `${c.encoder}: ffmpeg exit ${r.status}\n${(r.stderr || '').split('\n').slice(-6).join('\n')}`);
      if (c.files) {
        const n = fs.readdirSync(outBase).filter((x) => x.endsWith('.tif')).length;
        assert.equal(n, c.files, `${c.encoder}: file count`);
        continue;
      }
      const outFile = args[args.length - 1];
      assert.ok(fs.statSync(outFile).size > 1000, `${c.encoder}: output written`);
      outputs[c.encoder] = outFile;
      if (ffprobe && c.probe) {
        const p = spawnSync(ffprobe, ['-v', 'error', '-select_streams', 'v:0', '-show_entries', 'stream=codec_name,profile,pix_fmt,width,height,nb_frames', '-of', 'json', outFile], { encoding: 'utf8' });
        assert.equal(p.status, 0, `${c.encoder}: ffprobe`);
        const s = JSON.parse(p.stdout).streams[0];
        for (const [k, v] of Object.entries(c.probe)) {
          const got = String(s[k]);
          if (v && typeof v === 'object' && Array.isArray(v.oneOf)) assert.ok(v.oneOf.map(String).includes(got), `${c.encoder}: ${k} ${got} not in ${v.oneOf.join('/')}`);
          else assert.equal(got, String(v), `${c.encoder}: ${k}`);
        }
      }
    }
    for (const c of expected.checks.filter((k) => k.kind === 'decode-sample')) {
      const src = outputs[c.encoder];
      if (!src) { t.diagnostic(`${c.encoder}: decode sample skipped (no output)`); continue; }
      const frame = path.join(tmp, c.encoder + '-frame.png');
      const r = spawnSync(found.bin, ['-y', '-i', src, '-frames:v', '1', '-pix_fmt', 'rgba', frame], { encoding: 'utf8', timeout: 60000 });
      assert.equal(r.status, 0, `${c.encoder}: decode`);
      const img = png.decode(fs.readFileSync(frame));
      for (const s of c.samples) {
        const got = png.pixelAt(img, s.x, s.y);
        for (let i = 0; i < 3; i++) assert.ok(Math.abs(got[i] - s.rgb[i]) <= s.tolerance, `${c.encoder} (${s.x},${s.y}): got ${got.slice(0, 3)}, expected ${s.rgb} ±${s.tolerance}`);
      }
    }
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});
