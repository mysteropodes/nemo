'use strict';
// L04-K oracle generator for the INSTANCE side of exposed component properties (#2243).
// Usage: node gen.cjs > oracle.txt
// The baseline is executed, not re-derived: `applyOverrides`/`expose` come from the shipped module
// src/js/domain/component/exposed-properties.js; the value read is the shipped motion.js
// `rawValueAtFrame`/`staticValue`/`trackFor`/`segmentIndexAtFrame`/`registerExposedPropMeta` cut verbatim
// and run in a Node vm with the shipped animation/curve.js and domain/animation/opacity.js.
// Row: `scenario <TAB> frame <TAB> result`, result = the stroke's opacity, or HIDDEN when dropped.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ROOT = path.join(__dirname, '..', '..', '..', '..');
const read = (rel) => fs.readFileSync(path.join(ROOT, rel), 'utf8');
function cut(text, name) {
  const start = text.indexOf(`function ${name}(`);
  if (start < 0) throw new Error(`function ${name} not found`);
  let depth = 0;
  for (let i = text.indexOf('{', start); i < text.length; i++) {
    if (text[i] === '{') depth++;
    else if (text[i] === '}' && --depth === 0) return text.slice(start, i + 1);
  }
  throw new Error(`function ${name} is not closed`);
}
const motion = read('src/js/motion.js');
const sandbox = { console };
vm.createContext(sandbox);
vm.runInContext(read('src/js/domain/animation/opacity.js') + '\n;this.NemoOpacityDomain = NemoOpacityDomain;', sandbox);
vm.runInContext(read('src/js/animation/curve.js') + '\n;this.SMAnimationCurve = SMAnimationCurve;', sandbox);
vm.runInContext([
  'var PROP_LABEL = {}, PROP_DIM = {}, PROP_UNIT = {}, PROP_DEFAULT = {};',
  'var DEFAULT_CURVE = NemoOpacityDomain.defaultCurve();',
  'var evalCurvePoints = SMAnimationCurve.evalCurvePoints;',
  ...['trackFor', 'staticValue', 'segmentIndexAtFrame', 'rawValueAtFrame', 'registerExposedPropMeta'].map((n) => cut(motion, n)),
  'this.valueAtFrame = rawValueAtFrame; this.register = registerExposedPropMeta;',
].join('\n'), sandbox);
const props = require(path.join(ROOT, 'src/js/domain/component/exposed-properties.js'));

const keyed = (keys) => ({ motion: { ep_x: { keys: keys.map(([frame, v]) => ({ frame, v: [v] })) } } });
const cases = [
  ['default-only', 'opacity', 100, {}, [0, 3]],
  ['default-only-50', 'opacity', 50, {}, [0]],
  ['static-37.5', 'opacity', 100, { motionStatic: { ep_x: [37.5] } }, [0, 7]],
  ['static-over-100', 'opacity', 100, { motionStatic: { ep_x: [250] } }, [0]],
  ['static-negative', 'opacity', 100, { motionStatic: { ep_x: [-20] } }, [0]],
  ['keyed-0-100', 'opacity', 100, keyed([[0, 0], [8, 100]]), [0, 2, 4, 6, 8, 12]],
  ['keyed-three', 'opacity', 100, keyed([[0, 100], [4, 20], [8, 80]]), [0, 2, 4, 6, 8]],
  ['visible-49', '__visible', 100, { motionStatic: { ep_x: [49] } }, [0]],
  ['visible-50', '__visible', 100, { motionStatic: { ep_x: [50] } }, [0]],
  ['visible-keyed', '__visible', 100, keyed([[0, 100], [8, 0]]), [0, 4, 5, 8]],
];
for (const [name, field, def, ld, frames] of cases) {
  for (const frame of frames) {
    const sym = { exposedProps: [{ key: 'ep_x', label: 'X', targetStrokeId: 'ink', targetField: field, default: def }] };
    const out = props.applyOverrides(sym, [{ strokeId: 'ink', opacity: 1 }], JSON.parse(JSON.stringify(ld)), frame,
      () => true, (ep) => sandbox.register(ep.key, ep.label, ep.default), (l, key, f) => sandbox.valueAtFrame(l, key, f));
    console.log([name, frame, out.length ? out[0].opacity : 'HIDDEN'].join('\t'));
  }
}
