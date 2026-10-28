'use strict';
// CS4b (#2364): one declaration drives the legacy-writer wraps and the desktop-admitted kinds.
const test=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const Writers=require('../scripts/nemo/legacy-writers.cjs');
const ROOT=path.resolve(__dirname,'..'),DOC=require('../engineering/application/legacy-writer-families.json');
const clone=()=>JSON.parse(JSON.stringify(DOC));

test('generated wraps and desktop kinds are current, and every guard kind is declared exactly once',()=>{
  assert.match(Writers.run(['--check'],ROOT),/legacy writers current/);
  const {names,kinds}=Writers.validate(DOC,ROOT);
  assert.equal(names.size,133);assert.equal(kinds.size,50);
  for(const kind of ['history-panel','motion-preset'])assert.equal(DOC.families.find(f=>f.kinds.includes(kind)).authority,'unavailable');
  assert.deepEqual(Writers.plan(DOC).admitted,[],'no family is legacy-owned on desktop today');
});

test('flipping one family to legacy-owned admits exactly its kinds and prefixed entry points',()=>{
  const doc=clone(),frames=doc.families.find(f=>f.family==='frames');frames.authority='legacy-owned';
  const admitted=Writers.plan(doc).admitted;
  const expected=[...frames.kinds,...Object.entries(frames.entries).flatMap(([t,list])=>list.map(n=>doc.prefixes[t]+n))].sort();
  assert.deepEqual(admitted,expected);assert.ok(admitted.includes('timeline-insertFrame')&&admitted.includes('app-insertFrame'));
  const surface=Writers.build(ROOT,doc).files['src/js/adapters/native-opacity-legacy-surface.js'];
  assert.match(surface,/'timeline-remove-frame'/);assert.doesNotMatch(surface,/'motion-set-value'/);
});

test('invalid declarations refuse',()=>{
  const cases=[
    d=>{d.schema='x';},d=>{d.families[0].authority='maybe';},d=>{d.families[0].capability='native-missing';},
    d=>{delete d.families[0].capability;},d=>{d.families[1].entries.global=['insertFrame'];},
    d=>{d.families[1].kinds.push(d.families[0].kinds[0]);},d=>{d.families[0].entries.desktop=['x'];},
    d=>{d.families[0].kinds.push('boot-scaffold');},d=>{delete d.prefixes.motion;},
    d=>{const o=d.families.find(f=>f.family==='opacity');o.authority='legacy-owned';delete o.capability;},
    d=>{d.families[0].kindPrefixes=['opacity-'];},d=>{d.families[0].kindPrefixes=['bad'];},
  ];
  for(const mutate of cases){const d=clone();mutate(d);assert.throws(()=>Writers.validate(d,ROOT),/legacy-writers:/);}
  const stale=clone();stale.families[0].kinds.push('never-guarded');
  assert.throws(()=>Writers.build(ROOT,stale),/has no guard call site/);
  const missing=clone();missing.families.find(f=>f.family==='motion').kinds.pop();
  assert.throws(()=>Writers.build(ROOT,missing),/is not declared/);
  const prefix=clone();delete prefix.families.find(f=>f.family==='opacity').kindPrefixes;
  assert.throws(()=>Writers.build(ROOT,prefix),/kind prefix opacity- is not declared/);
  const extra=clone();extra.families.find(f=>f.family==='opacity').kindPrefixes.push('never-');
  assert.throws(()=>Writers.build(ROOT,extra),/prefix never- has no guard call site/);
});
