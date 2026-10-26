'use strict';
// CS8-B09 U37 (#2374): the twelve dormant Labs prototypes stay dormant, as in the baseline, and each carries an
// exact retained-source disposition with no desktop functionality acceptance (dispositions/U37.md).
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

const ROOT = path.join(__dirname, '..');
const DORMANT = ['auto-actions', 'clip-mask-bake', 'command-palette', 'layer-effects-bake', 'lipsync-assistant', 'oca-export',
  'rig-deform', 'screentone-bake', 'shader-effects-bake', 'storyboard-mode', 'timelapse', 'timeline-zoom'];
const read = (file) => fs.readFileSync(path.join(ROOT, file), 'utf8');
const manifest = JSON.parse(read('engineering/application/script-manifest.json'));
const loaded = new Set(manifest.entries.filter((e) => e.src).map((e) => e.src));

test('exactly the twelve dormant Labs files are outside the startup graph, now and in the baseline', () => {
  const labs = fs.readdirSync(path.join(ROOT, 'src/js/labs')).filter((f) => f.endsWith('.js')).map((f) => f.slice(0, -3));
  assert.deepEqual(labs.filter((n) => !loaded.has(`js/labs/${n}.js`)).sort(), [...DORMANT].sort());
  const baseline = execFileSync('git', ['show', '1065e9d8:src/index.html'], { cwd: ROOT, encoding: 'utf8' });
  for (const name of DORMANT) assert.equal(baseline.includes(`js/labs/${name}.js`), false, name + ' was dormant in the baseline');
  assert.ok(loaded.has('js/timeline-zoom.js'), 'the ordinary timeline zoom stays reachable');
  // #1033: the timelapse capability boundary is kept by its own tests, without a startup addition.
  assert.equal(loaded.has('js/application/timelapse-capability.js'), false);
  assert.ok(fs.existsSync(path.join(ROOT, 'tests/application-timelapse-capability.test.cjs')));
});

test('no source loads a dormant prototype by import, worker or script string', () => {
  const files = execFileSync('git', ['ls-files', '--', 'src/js', 'src/index.html'], { cwd: ROOT, encoding: 'utf8' }).split('\n').filter(Boolean);
  for (const name of DORMANT) {
    const literal = new RegExp(`['"\`][^'"\`\\n]*labs/${name}(?:\\.js)?['"\`]`);
    const loaders = files.filter((f) => f !== `src/js/labs/${name}.js` && literal.test(read(f)));
    assert.deepEqual(loaders, [], name);
  }
});

test('each dormant file is a retained source without a load site and its matrix row claims no acceptance', () => {
  const policy = JSON.parse(read('engineering/boundaries/profiles/app-js.coverage.json'));
  const retained = new Map(policy.retainedSources.map((r) => [r.path, r]));
  for (const name of DORMANT) {
    const entry = retained.get(`src/js/labs/${name}.js`);
    assert.ok(entry, name); assert.equal(entry.executionClass, 'classic-without-load-site', name); assert.deepEqual(entry.loadSites || [], [], name);
  }
  const matrix = JSON.parse(read('engineering/inventory/parity/matrix.json'));
  const rows = matrix.surfaces.filter((s) => (s.tasks || []).includes('U37'));
  // Ten labs:* rows plus the command palette's 25 menu entries and its own row, and labs:timeline-zoom (#2374 U31).
  assert.equal(rows.length, 37);
  assert.equal(rows.filter((r) => r.id.startsWith('menu:labs/command-palette.js:')).length, 25);
  for (const row of rows) {
    assert.equal(row.status, 'disposition', row.id);
    assert.match(row.disposition, /^dormant-in-baseline \(U37\)/, row.id);
  }
  assert.match(read('engineering/inventory/parity/dispositions/U37.md'), /The twelve dormant files/);
});
