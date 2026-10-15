// Oracle for native-engine/src/expression_controls.rs: one JSON object per
// line; numbers travel as IEEE 754 hex strings so NaN/-0 survive.
// usage: node oracle-gen/controls.mjs <repo> > native-engine/tests/fixtures/expression-controls/oracle.jsonl
import { motionSource, extract, hx, rng } from './common.mjs';

const src = motionSource(process.argv[2] || '.');
const names = ['normalizeControlName', 'findControl', 'controlsOf', '_controlValue', 'exprSelfControl',
  'exprLayerControl', 'findLayerIndexByUid', 'findLayerIndexByRef', '_num'];
const env = { ctx: null, self: null, sampled: null };
const state = { layers: [], currentFrame: 0 };
const api = new Function('state', 'env', `
  var SM = { t: function (k) { return '<' + k + '>'; } };
  function _exprTick() {}
  var _ectx = null;
  function _selfLd() { return env.self; }
  // Stub track: component i of a control is base[i] + frame * (i + 1).
  function valueAtFrame(ld, key, frame) {
    env.sampled = frame;
    return ld._base[key].map(function (b, i) { return b + frame * (i + 1); });
  }
  ${names.map((n) => extract(src, n)).join('\n')}
  return {
    layer: function (ref, name, f) { _ectx = env.ctx; return exprLayerControl(ref, name, f); },
    self: function (name, f) { _ectx = env.ctx; return exprSelfControl(name, f); },
    norm: normalizeControlName,
  };`)(state, env);

const r = rng(0xc0de);
const pick = (a) => a[Math.floor(r() * a.length)];
const num = (x) => (x === undefined ? null : hx(x));
const NAMES = ['Turn', 'turn', '  TURN\t', ' Turn﻿', '\u0085Turn', 'Turn ', 'Lift', 'ΟΔΟΣ', 'οδος',
  'İ', 'i̇', 'ẞ', 'ß', 'Ünïcode', '', '   ', null, undefined, 'Slider 1', 'slider 1', 'Ǆ', 'ǆ', '　Mix　'];
const FRAMES = [undefined, undefined, 0, -0, 3, 2.5, -7, NaN, Infinity, -Infinity, 1e308];

const rows = [];
function layersFor() {
  const n = 1 + Math.floor(r() * 4);
  const ls = [];
  for (let i = 0; i < n; i++) {
    const ld = { _base: {} };
    if (r() < 0.9) ld.layerUid = pick(['u1', 'u2', 'Ctrl', 'u3', '']);
    if (r() < 0.9) ld.name = pick(['Ctrl', 'u1', 'Rig', 'Layer 1', '']);
    if (r() < 0.85) {
      const k = Math.floor(r() * 4);
      ld.exprControls = [];
      for (let j = 0; j < k; j++) {
        const key = `xc_${i}_${j}`;
        const c = { key };
        const nm = pick(NAMES);
        if (nm !== undefined) c.name = nm;
        ld.exprControls.push(c);
        ld._base[key] = Array.from({ length: pick([0, 1, 1, 2, 4]) }, () => pick([0, -0, 1.5, -2, NaN, 100]));
      }
    } else if (r() < 0.5) ld.exprControls = { not: 'an array' };
    ls.push(ld);
  }
  return ls;
}
const jsLayer = (ld) => ({
  uid: ld.layerUid ?? null,
  name: ld.name ?? null,
  controls: Array.isArray(ld.exprControls)
    ? ld.exprControls.map((c) => ({ key: c.key, name: c.name ?? null, base: ld._base[c.key].map(hx) }))
    : null,
});
function run(fn) {
  env.sampled = null;
  try {
    const v = fn();
    return { ok: typeof v === 'number' ? { s: hx(v) } : { v: v.map(hx) }, sampled: num(env.sampled ?? undefined) };
  } catch (e) {
    return { err: e.message };
  }
}
for (let i = 0; i < 150; i++) {
  state.layers = layersFor();
  state.currentFrame = pick([0, 7, -0, 12.5]);
  env.ctx = r() < 0.7 ? { frame: pick([12, -0, 0.5, 30]) } : null;
  const known = state.layers.flatMap((l) => [l.layerUid, l.name]).filter((x) => x !== undefined);
  const ref = r() < 0.1 ? pick(['', null, undefined, 'missing']) : r() < 0.75 && known.length ? pick(known) : pick(['u1', 'Ctrl', 'Rig']);
  const ctrlNames = state.layers.flatMap((l) => (Array.isArray(l.exprControls) ? l.exprControls.map((c) => c.name) : []));
  let name = r() < 0.6 && ctrlNames.length ? pick(ctrlNames) : pick(NAMES);
  if (typeof name === 'string' && r() < 0.3) name = ` ${name.toUpperCase()}\t`;
  const f = pick(FRAMES);
  const row = {
    op: 'layer', layers: state.layers.map(jsLayer), current: hx(state.currentFrame),
    ctx: env.ctx ? hx(env.ctx.frame) : null, ref: ref ?? null, refUndefined: ref === undefined,
    name: name ?? null, nameUndefined: name === undefined, frame: num(f), ...run(() => api.layer(ref, name, f)),
  };
  rows.push(row);
  if (i % 2 === 0) {
    env.self = r() < 0.85 ? state.layers[Math.floor(r() * state.layers.length)] : null;
    const selfIdx = env.self ? state.layers.indexOf(env.self) : null;
    rows.push({
      op: 'self', layers: state.layers.map(jsLayer), current: hx(state.currentFrame),
      ctx: env.ctx ? hx(env.ctx.frame) : null, self: selfIdx, name: name ?? null, nameUndefined: name === undefined, frame: num(f),
      ...run(() => api.self(name, f)),
    });
  }
}
for (const n of NAMES) rows.push({ op: 'norm', name: n ?? null, out: api.norm(n) });
process.stdout.write(rows.map((x) => JSON.stringify(x)).join('\n') + '\n');
