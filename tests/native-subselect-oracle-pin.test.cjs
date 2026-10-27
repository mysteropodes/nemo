'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const cp = require('node:child_process');
const crypto = require('node:crypto');
const root = path.resolve(__dirname, '..');
const baseline = '46442ea96c09e6bfaadf956eb8586e273a658085';
const fixture = JSON.parse(fs.readFileSync(path.join(root, 'native-engine/tests/fixtures/subselect-baseline/oracle.json')));
test('CS8 subselect oracle executes the pinned original tools, bridge and Paper', () => {
  assert.equal(fixture.baseline, baseline);
  assert.equal(fixture.cases.length, 220);
  for (const file of ['src/js/tools.js', 'src/js/subselect-bridge.js', 'src/js/timeline.js', 'src/js/app.js', 'src/paper-full.min.js']) {
    const bytes = cp.execFileSync('git', ['show', `${baseline}:${file}`], {cwd:root,maxBuffer:8e6});
    assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'), fixture.hashes[file]);
  }
  const generated = cp.execFileSync(process.execPath, ['native-engine/tests/fixtures/subselect-baseline/gen.cjs'], {cwd:root,maxBuffer:8e6});
  assert.deepEqual(JSON.parse(generated), fixture);
});
test('subselect parent-space oracle runs frozen Motion and point-type code',()=>{
  const projection=JSON.parse(fs.readFileSync(path.join(root,'native-engine/tests/fixtures/subselect-baseline/projection.json')));
  assert.equal(projection.baseline,baseline);
  const bytes=cp.execFileSync('git',['show',`${baseline}:src/js/motion.js`],{cwd:root,maxBuffer:8e6});
  assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'),projection.sourceHash);
  assert.deepEqual(JSON.parse(cp.execFileSync(process.execPath,['native-engine/tests/fixtures/subselect-baseline/gen-projection.cjs'],{cwd:root,maxBuffer:8e6})),projection);
});
test('subselect held-frame promotion oracle executes the original key and exposure functions',()=>{
  const held=JSON.parse(fs.readFileSync(path.join(root,'native-engine/tests/fixtures/subselect-baseline/held.json')));
  assert.equal(held.baseline,baseline);assert.equal(held.cases.length,3);
  for(const [file,hash] of Object.entries(held.hashes))assert.equal(crypto.createHash('sha256').update(cp.execFileSync('git',['show',`${baseline}:${file}`],{cwd:root,maxBuffer:8e6})).digest('hex'),hash);
  assert.deepEqual(JSON.parse(cp.execFileSync(process.execPath,['native-engine/tests/fixtures/subselect-baseline/gen-held.cjs'],{cwd:root,maxBuffer:8e6})),held);
});
