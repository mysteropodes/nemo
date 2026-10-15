// Build self-contained JSC bundles of the two oracle generators.
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
const G = '/tmp/claude-0/r18prep-wt/oracle-gen/';
const common = readFileSync(G + 'common.mjs', 'utf8');
const PIN = '660ac5ef79f9a1da54cfabbe5279aeb76a941f73';
const src = execFileSync('git', ['-C', '/home/claude/nemo', 'show', `${PIN}:src/js/motion.js`], { encoding: 'utf8', maxBuffer: 64 << 20 });
// reuse the real extract() on the real source
const commonBody = common.replace(/^import .*$/m, '').replace(/^export /gm, '');
const extract = new Function('src', 'name', commonBody.match(/function extract[\s\S]*?\n}\n/)[0] + 'return extract(src, name);');
for (const gen of ['parent_chain', 'controls']) {
  let g = readFileSync(G + gen + '.mjs', 'utf8');
  const names = new Set();
  for (const m of g.matchAll(/'([A-Za-z_$][\w$]*)'/g)) { try { extract(src, m[1]); names.add(m[1]); } catch {} }
  for (const m of g.matchAll(/extract\(src,\s*'([\w$]+)'\)/g)) names.add(m[1]);
  const fn = {}; for (const n of names) fn[n] = extract(src, n);
  const shim = [
    "'use strict';",
    `// R18.2 prep oracle (${gen}) for JavaScriptCore: functions extracted verbatim from src/js/motion.js @ ${PIN}.`,
    'var process = { argv: [], env: {}, stdout: { write: function (s) { print(s.replace(/\\n$/, "")); } } };',
    'var __FN = ' + JSON.stringify(fn) + ';',
    commonBody.replace(/export const PIN[^\n]*/, `const PIN = '${PIN}';`)
      .replace(/function motionSource[\s\S]*?\n}\n/, 'function motionSource() { return null; }\n')
      .replace(/function extract[\s\S]*?\n}\n/, 'function extract(_s, name) { if (!(name in __FN)) throw new Error("missing " + name); return __FN[name]; }\n'),
  ].join('\n');
  g = g.replace(/^import .*$/gm, '');
  writeFileSync(`jsc-${gen}.js`, shim + '\n' + g);
  console.error(gen, Object.keys(fn).join(','));
}
