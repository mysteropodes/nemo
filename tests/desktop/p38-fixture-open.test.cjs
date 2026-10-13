'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const test = require('node:test');
const crypto = require('node:crypto');
const script = fs.readFileSync('tests/desktop/p38-fixture-open-bootstrap.js', 'utf8');
const fixtures = Object.fromEntries(['A', 'B'].map(id => [id,
  `/task/tmp/p38-fixture-open-12345678-1234-1234-1234-123456789abc/${id}.json`]));
const hashes = Object.fromEntries(['A', 'B'].map(id => [id,
  crypto.createHash('sha256').update(fs.readFileSync(`tests/fixtures/p38/open/${id}.json`)).digest('hex')]));
function execute(openResults, configPatch = {}) {
  const calls = [], opened = [];
  const config = { schema:'nemo.p38-fixture-open-bootstrap/1', runId:'12345678-1234-1234-1234-123456789abc',
    taskKey:'a'.repeat(64), sourceSha:'b'.repeat(40), buildSha:'b'.repeat(40), manifestSha256:'c'.repeat(64),
    pid:42, fixtures:{...fixtures}, ...configPatch };
  const window = { [ '__NEMO_P38_FIXTURE_OPEN__' ]:config,
    __TAURI__:{core:{invoke:async (_name,args)=>{calls.push(JSON.parse(args.payloadJson));}}},
    SMProject:{openPath:async path=>{opened.push(path);return openResults.shift();}} };
  vm.runInNewContext(script, {window, Date, setTimeout, Promise, Object, Number, Error});
  return Promise.resolve(window.__NEMO_P38_FIXTURE_OPEN_DONE__).then(result=>({result,calls,opened}));
}
test('fixed A/B sequence accepts legacy undefined and adapter prepared results', async()=>{
  for (const values of [[undefined,undefined],[{status:'prepared'},{status:'prepared'}]]) {
    const r=await execute([...values]); assert.equal(r.result,'complete');
    assert.deepEqual(r.opened, [fixtures.A,fixtures.B]); assert.deepEqual(r.calls.map(x=>x.phase), ['ready','a-opened','b-opened']);
    assert.deepEqual(r.calls.slice(1).map(x=>x.fixtureSha256), [hashes.A,hashes.B]);
  }
});
for (const result of [false, {status:'refused'}, {status:'unavailable'}, {status:'indeterminate'},
  {status:'cancelled'}, {status:'prepared',extra:true}, null, true]) test(`open refusal does not advance to B: ${JSON.stringify(result)}`, async()=>{
  const r=await execute([result]); assert.equal(r.result,'refused'); assert.deepEqual(r.opened,[fixtures.A]);
  assert.deepEqual(r.calls.map(x=>x.phase), ['ready','a-opened']); assert.equal(r.calls[1].status,'failed');
});
test('tampered static identity sends no checkpoint and opens no fixture', async()=>{
  const r=await execute([undefined], {buildSha:'d'.repeat(40)});
  assert.equal(r.result,undefined); assert.deepEqual(r.calls,[]); assert.deepEqual(r.opened,[]);
});

test('owner runner requires observed original-window fixture and alert before ack', async()=>{
  const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
  const {acknowledge,visibleEvidence} = require('./p38-fixture-open-installed.cjs');
  const base=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'p38-installed-'))), tmp=path.join(base,'tmp');
  fs.mkdirSync(tmp); const root=path.join(tmp,'p38-fixture-open-test'); fs.mkdirSync(root);
  try {
    for (const id of ['A','B']) fs.copyFileSync(`tests/fixtures/p38/open/${id}.json`,path.join(root,`${id}.json`));
    const fixturePins=['A','B'].map(id=>{const file=path.join(root,`${id}.json`),stat=fs.statSync(file);
      return {id,file,sha256:hashes[id],device:stat.dev,inode:stat.ino};});
    const pins={schema:'nemo.p38-fixture-open/1',runId:'12345678-1234-1234-1234-123456789abc',
      taskKey:'a'.repeat(64),sourceSha:'b'.repeat(40),buildSha:'b'.repeat(40),pid:process.pid,window:'main',
      fixtures:fixturePins,origin:'tauri://localhost/index.html',checkpoint:1,waiting:true,failed:false};
    const pinBytes=JSON.stringify({schema:pins.schema,runId:pins.runId,taskKey:pins.taskKey,sourceSha:pins.sourceSha,
      buildSha:pins.buildSha,pid:pins.pid,window:pins.window,origin:pins.origin,fixtures:pins.fixtures});
    pins.manifestSha256=crypto.createHash('sha256').update(pinBytes).digest('hex');
    fs.writeFileSync(path.join(root,'manifest.json'),JSON.stringify(pins));
    const expected={...pins,phase:'a-opened',windowId:9};
    const snapshot={pid:process.pid,windowId:9,matchedAX:true,nodes:[
      {path:'1',role:'AXStaticText',control:'A.json'}, {path:'2',role:'AXStaticText',alert:'switch-unavailable'}]};
    assert.throws(()=>acknowledge(root,expected,'a'.repeat(32),'accepted',
      {...snapshot,nodes:[{path:'1',role:'AXStaticText',control:'B.json'}]}),/probe-visible-state-refused/);
    for(const windowId of [undefined,0,-1,4294967296])
      assert.throws(()=>visibleEvidence(snapshot,{...expected,windowId}),/probe-window-pin-refused/);
    assert.throws(()=>acknowledge(root,{...expected,windowId:undefined},'a'.repeat(32),'refused'),/probe-window-pin-refused/);
    assert.throws(()=>visibleEvidence(snapshot,{...expected,phase:'unknown'}),/probe-phase-refused/);
    const alias=path.join(base,'same-A.json'), original=path.join(base,'original-A.json');
    fs.copyFileSync(path.join(root,'A.json'),alias); fs.renameSync(path.join(root,'A.json'),original);
    fs.symlinkSync(alias,path.join(root,'A.json'));
    assert.throws(()=>acknowledge(root,expected,'a'.repeat(32),'accepted',snapshot),/probe-file-refused/);
    fs.unlinkSync(path.join(root,'A.json')); fs.renameSync(original,path.join(root,'A.json'));
    const pinned=path.join(base,'pinned-A.json'); fs.renameSync(path.join(root,'A.json'),pinned);
    fs.copyFileSync(alias,path.join(root,'A.json'));
    assert.throws(()=>acknowledge(root,expected,'a'.repeat(32),'accepted',snapshot),/probe-fixture-identity-refused/);
    fs.unlinkSync(path.join(root,'A.json')); fs.renameSync(pinned,path.join(root,'A.json'));
    const result=acknowledge(root,expected,'a'.repeat(32),'accepted',snapshot);
    assert.equal(result.phase,'a-opened');
    const ack=JSON.parse(fs.readFileSync(path.join(root,'ack.json'),'utf8'));
    assert.equal(ack.decision,'accepted'); assert.equal(ack.ownerToken,'a'.repeat(32));
  } finally { fs.rmSync(base,{recursive:true,force:true}); }
});
