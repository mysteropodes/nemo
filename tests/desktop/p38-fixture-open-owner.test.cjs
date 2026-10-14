'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const crypto = require('node:crypto');
const { EventEmitter } = require('node:events');
const { PHASES, PROBE_FAILURE_CODES, runObserver, fail, writeOutcome, verifyHelperExecutable,
  helperEnvironment, stopSpawnedHelper, defaultServices, safeCode } = require('./p38-fixture-open-owner.cjs');

const config = Object.freeze({ taskId:'p38-task-test', grantId:'p38-grant-test',
  helperPath:'/tmp/p38-window-helper', helperSha256:'e'.repeat(64), sourceHead:'b'.repeat(40), timeoutMs:10000 });
const baseManifest = Object.freeze({ schema:'nemo.p38-fixture-open/1',
  runId:'12345678-1234-1234-1234-123456789abc', taskKey:'a'.repeat(64),
  sourceSha:config.sourceHead, buildSha:config.sourceHead, pid:42, window:'main',
  origin:'tauri://localhost/index.html', fixtures:[{id:'A'},{id:'B'}], manifestSha256:'c'.repeat(64),
  waiting:true, failed:false, checkpoint:0 });
function probeManifest(root, overrides = {}) {
  const fixtures = [
    {id:'A',file:path.join(root,'A.json'),sha256:'e1bc63b0e284f521e170d8236296392bdcf19d4b1f13b4e04683727844fa71a0',device:1,inode:2},
    {id:'B',file:path.join(root,'B.json'),sha256:'a438bda3b5c25ec3e8797d1c95fb8ade1da7b0bda675c2279499c659ece46bf6',device:1,inode:3},
  ];
  const value={...baseManifest,fixtures,result:overrides.result ||
    (overrides.failed ? 'failed' : overrides.waiting === false ? 'prepared' : 'waiting'),...overrides};
  const pins={schema:value.schema,runId:value.runId,taskKey:value.taskKey,sourceSha:value.sourceSha,
    buildSha:value.buildSha,pid:value.pid,window:value.window,origin:value.origin,fixtures:value.fixtures};
  value.manifestSha256=crypto.createHash('sha256').update(JSON.stringify(pins)).digest('hex');
  return value;
}
async function probeFailure(manifest, useSymlink = false, receiptMismatch = false) {
  const base=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'p38-probe-diagnostic-')));
  const realTemp=path.join(base,'tmp-real'), temp=path.join(base,'tmp');
  fs.mkdirSync(realTemp);
  if(useSymlink)fs.symlinkSync(realTemp,temp); else fs.mkdirSync(temp);
  if(manifest){
    const root=path.join(temp,'p38-fixture-open-12345678-1234-1234-1234-123456789abc');
    fs.mkdirSync(root);
    const {fixtureMutation,...overrides}=manifest;
    const value=probeManifest(root,overrides);
    if(fixtureMutation){
      const fixture=value.fixtures[fixtureMutation.index];
      if(fixtureMutation.value===undefined)delete fixture[fixtureMutation.field];
      else fixture[fixtureMutation.field]=fixtureMutation.value;
      const pins={schema:value.schema,runId:value.runId,taskKey:value.taskKey,sourceSha:value.sourceSha,
        buildSha:value.buildSha,pid:value.pid,window:value.window,origin:value.origin,fixtures:value.fixtures};
      value.manifestSha256=crypto.createHash('sha256').update(JSON.stringify(pins)).digest('hex');
    }
    fs.writeFileSync(path.join(root,'manifest.json'),JSON.stringify(value));
  }
  let tick=0;
  const services=defaultServices(config,{roots:{temp},isolation:{taskRoots:()=>({temp})},owner:{receipt(){
    if(receiptMismatch)return {value:{manifestSha256:'f'.repeat(64)}};
    throw new Error('must not read checkpoint receipt');}},
    now:()=>tick,pause:async()=>{tick=10;}});
  try { await services.waitForProbe(config,{pid:42,taskKey:'a'.repeat(64),source:config.sourceHead},5); }
  catch(error){return {stage:error.observerStage,code:error.observerCode};}
  finally {fs.rmSync(base,{recursive:true,force:true});}
  return null;
}
async function checkpointFailure(manifestOverrides) {
  const base=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'p38-checkpoint-failure-')));
  const temp=path.join(base,'tmp'), reportDir=path.join(base,'reports');
  fs.mkdirSync(temp); fs.mkdirSync(reportDir);
  const root=path.join(temp,'p38-fixture-open-12345678-1234-1234-1234-123456789abc'); fs.mkdirSync(root);
  const manifest=probeManifest(root,{failed:true,waiting:false,checkpoint:1,failureCode:'a-fs-read-rejected',result:'failed',...manifestOverrides});
  fs.writeFileSync(path.join(root,'manifest.json'),JSON.stringify(manifest));
  fs.writeFileSync(path.join(reportDir,'receipt.json'),JSON.stringify({schema:'nemo.p38-operator/1',grant:config.grantId,
    task:config.taskId,source:config.sourceHead,appPid:42,windowId:6068}));
  const expected={taskId:config.taskId,taskKey:'a'.repeat(64),source:config.sourceHead,pid:42,windowId:6068};
  const services=defaultServices(config,{roots:{temp},reportDir,isolation:{taskRoots:()=>({temp})},owner:{}});
  try { await services.waitForCheckpoint(config,expected,{root,manifest},'a-opened',1,Date.now()+1000); }
  catch(error) { return {stage:error.observerStage,code:error.observerCode}; }
  finally { fs.rmSync(base,{recursive:true,force:true}); }
  return null;
}
async function checkpointObservation() {
  const base=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'p38-checkpoint-observation-')));
  const temp=path.join(base,'tmp'),reportDir=path.join(base,'reports');fs.mkdirSync(temp);fs.mkdirSync(reportDir);
  const root=path.join(temp,'p38-fixture-open-12345678-1234-1234-1234-123456789abc');fs.mkdirSync(root);
  const manifestPath=path.join(root,'manifest.json');fs.writeFileSync(manifestPath,JSON.stringify(probeManifest(root)));
  fs.writeFileSync(path.join(reportDir,'receipt.json'),JSON.stringify({schema:'nemo.p38-operator/1',grant:config.grantId,
    task:config.taskId,source:config.sourceHead,appPid:42,windowId:6068}));
  const owner={receipt:()=>({value:JSON.parse(fs.readFileSync(manifestPath,'utf8'))}),visibleEvidence(){},
    acknowledge(_root,expected){if(expected.phase==='ready')fs.writeFileSync(manifestPath,JSON.stringify(probeManifest(root,
      {failed:true,waiting:false,checkpoint:1,failureCode:'a-fs-read-rejected',result:'failed'})));}};
  const defaults=defaultServices(config,{roots:{temp},reportDir,isolation:{taskRoots:()=>({temp}),idKey:()=> 'a'.repeat(64)},owner});
  const services={...defaults,readOwner:async()=>({taskId:config.taskId,ownerToken:'d'.repeat(32)}),
    startHelper:async()=>({next:async()=>({status:'pinned',pid:42,windowId:6068,matchedAX:true}),
      inspect:async()=>({pid:42,windowId:6068,matchedAX:true,nodes:[]}),close:async()=>{}})};
  try {
    const result=await runObserver(config,services);
    const saved=JSON.parse(fs.readFileSync(path.join(reportDir,'owner-observation.json'),'utf8'));
    return {firstFailure:result.firstFailure,observations:result.observations,cleanup:result.cleanup,saved:saved.firstFailure};
  } finally {fs.rmSync(base,{recursive:true,force:true});}
}
test('checkpoint failure preserves only its phase-aligned fixed manifest cause',async()=>{
  assert.deepEqual(await checkpointFailure({}),
    {stage:'probe-manifest',code:'probe-manifest-failed-a-fs-read-rejected'});
  for(const overrides of [
    {checkpoint:0,failureCode:'post-wait-validation-refused'},
    {failureCode:'private /Users/example/A.json exception'},
    {taskKey:'f'.repeat(64)},
    {checkpoint:0,failureCode:'ready-open-path-unavailable'},
    {result:'refused'},
    {failed:false,result:'refused'},
    {failed:0,result:'waiting'},
  ]) assert.deepEqual(await checkpointFailure(overrides),
    {stage:'probe-manifest',code:'probe-manifest-failure-unknown'});
});
test('owner observation retains the fixed A-phase cause after ready acknowledgement',async()=>{
  const result=await checkpointObservation();
  const failure={stage:'probe-manifest',code:'probe-manifest-failed-a-fs-read-rejected'};
  assert.deepEqual(result.firstFailure&&{stage:result.firstFailure.stage,code:result.firstFailure.code},failure);
  assert.deepEqual(result.saved&&{stage:result.saved.stage,code:result.saved.code},failure);
  assert.deepEqual(result.observations.map(item=>[item.phase,item.decision]),[['ready','accepted']]);
  assert.equal(result.cleanup,'complete');
});
test('owner classifies an unsafe temporary-root identity separately',async()=>{
  assert.deepEqual(await probeFailure(null,true),{stage:'probe-manifest',code:'probe-temp-identity-refused'});
});
function rustAllowedFailures(phase) {
  const source=fs.readFileSync(path.join(__dirname,'../../src-tauri/src/task_runtime/p38_fixture_open_probe_control.rs'),'utf8');
  const match=source.match(new RegExp(`pub\\(super\\) fn allowed_${phase}_failure\\(code: &str\\) -> bool \\{([\\s\\S]*?)\\n\\}`));
  assert.ok(match,`Rust ${phase} failure allowlist exists`);
  return [...match[1].matchAll(/"([^"]+)"/g)].map(item=>item[1]);
}
test('owner allowlists and classifies every fixed Rust failure by matching phase cursor',async()=>{
  for(const phase of ['ready','a','b'])
    assert.deepEqual(PROBE_FAILURE_CODES[phase],rustAllowedFailures(phase),`${phase} owner/Rust failure allowlists match`);
  const cases=[['ready',0],['a',1],['b',2]].flatMap(([phase,checkpoint])=>
    PROBE_FAILURE_CODES[phase].map(failureCode=>({checkpoint,failureCode})));
  for(const {checkpoint,failureCode} of cases){
    const category=`probe-manifest-failed-${failureCode}`;
    assert.deepEqual(await probeFailure({failed:true,waiting:false,checkpoint,failureCode,result:'failed'}),
      {stage:'probe-manifest',code:category});
    assert.equal(safeCode('probe-manifest',category),category);
  }
});
test('owner keeps mismatched, malformed, and private manifest failures sanitized',async()=>{
  assert.deepEqual(await probeFailure({failed:true,waiting:false,checkpoint:0,failureCode:'ready-open-path-unavailable',result:'failed'}),
    {stage:'probe-manifest',code:'probe-manifest-failed-ready-open-path-unavailable'});
  for(const manifest of [
    {failed:true,waiting:false,checkpoint:1,failureCode:'ready-observer-install-failed',result:'failed'},
    {failed:true,waiting:false,checkpoint:1,failureCode:'b-open-threw',result:'failed'},
    {failed:true,waiting:true,checkpoint:1,failureCode:'a-open-threw',result:'failed'},
    {failed:true,waiting:false,checkpoint:1,failureCode:'private path or exception',result:'failed'},
  ]) assert.deepEqual(await probeFailure(manifest),
    {stage:'probe-manifest',code:'probe-manifest-failure-unknown'});
  assert.deepEqual(await probeFailure({failed:true,waiting:false,checkpoint:0,failureCode:'private path or exception',result:'failed'}),
    {stage:'probe-manifest',code:'probe-manifest-failure-unknown'});
});
test('owner rejects manifest identity and pin-shape mismatches with one sanitized category',async()=>{
  assert.deepEqual(await probeFailure({taskKey:'f'.repeat(64)}),
    {stage:'probe-manifest',code:'probe-manifest-validation-refused'});
  assert.deepEqual(await probeFailure({fixtures:[{id:'A'},{id:'B'}]}),
    {stage:'probe-manifest',code:'probe-manifest-validation-refused'});
  assert.deepEqual(await probeFailure({},false,true),
    {stage:'probe-manifest',code:'probe-manifest-validation-refused'});
});
test('owner refuses wrong A and B fixture IDs and hashes before classifying a fixed failure',async()=>{
  const mutations=[
    {label:'A ID',index:0,field:'id',value:'unexpected-a'},
    {label:'A digest',index:0,field:'sha256',value:'f'.repeat(64)},
    {label:'B ID',index:1,field:'id',value:'unexpected-b'},
    {label:'B digest',index:1,field:'sha256',value:'e'.repeat(64)},
  ];
  for(const mutation of mutations){
    const result=await probeFailure({failed:true,waiting:false,checkpoint:0,
      failureCode:'ready-open-path-unavailable',result:'failed',fixtureMutation:mutation});
    assert.deepEqual(result,{stage:'probe-manifest',code:'probe-manifest-validation-refused'},mutation.label);
  }
});
test('owner distinguishes a prepared-only ready timeout from missing manifest timeout',async()=>{
  assert.deepEqual(await probeFailure({waiting:false,failed:false,checkpoint:0}),
    {stage:'probe-manifest',code:'probe-ready-timeout'});
  assert.deepEqual(await probeFailure(null),{stage:'probe-manifest',code:'probe-manifest-timeout'});
});
function makeServices(overrides = {}) {
  const events = [], saved = [], acks = [];
  let now = 1000;
  const owner = {
    receipt(root, expected) {
      events.push('manifest:' + expected.phase);
      return { value:{ ...baseManifest, ...overrides.manifest, checkpoint:PHASES.indexOf(expected.phase) } };
    },
    visibleEvidence(snapshot, expected) {
      events.push('visible:' + expected.phase);
      if (snapshot.visible !== true) throw Object.assign(new Error('private native text'), { code:'probe-visible-state-refused' });
      return { nodes:[] };
    },
    acknowledge(root, expected, token, decision) {
      events.push('ack:' + expected.phase);
      acks.push({ phase:expected.phase, token, decision });
      if (overrides.ackFailure === expected.phase) fail('acknowledgment','owner-ack-refused');
    },
  };
  const helper = {
    async next() {
      events.push('helper-pin');
      if (overrides.helperPinFailure) return { status:'pinned', pid:42, windowId:99, matchedAX:true };
      return { status:'pinned', pid:42, windowId:6068, matchedAX:true };
    },
    async inspect(window) {
      const phase = PHASES.filter(item => item);
      const index = events.filter(event => event.startsWith('visible:')).length;
      const name = phase[index];
      events.push('inspect:' + name);
      if (overrides.inspectFailure === name) fail('inspection','inspection-refused');
      return { visible: overrides.visibleFailure !== name, pid:42, windowId:6068 };
    },
    async quit() { events.push('quit'); if (overrides.quitFailure) fail('inspection','inspection-refused'); },
    async close() {
      events.push('cleanup');
      if (overrides.cleanupFailure) fail('cleanup','helper-cleanup-incomplete');
    },
  };
  const services = {
    now:()=>now++,
    taskKey:()=>baseManifest.taskKey,
    owner,
    waitForDriver:async()=> {
      events.push('driver');
      if (overrides.driverFailure) return { schema:'nemo.p38-operator/1', grant:config.grantId,
        task:config.taskId, source:config.sourceHead, appPid:0, windowId:6068 };
      return { schema:'nemo.p38-operator/1', grant:config.grantId, task:config.taskId,
        source:config.sourceHead, appPid:42, windowId:6068 };
    },
    readOwner:async()=> {
      events.push('owner');
      if (overrides.ownerFailure) return null;
      return { taskId:config.taskId, ownerToken:'d'.repeat(32) };
    },
    waitForProbe:async()=> {
      events.push('probe');
      if (overrides.probeFailure) return { root:'/tmp/p38-fixture-open-test', manifest:{ ...baseManifest, taskKey:'f'.repeat(64) } };
      return { root:'/tmp/p38-fixture-open-test', manifest:{ ...baseManifest } };
    },
    startHelper:async()=> {
      events.push('helper-start');
      if (overrides.helperStartFailure) fail('helper-start','helper-start-failed');
      return helper;
    },
    waitForCheckpoint:async(_config,_expected,_probe,phase,index)=> {
      events.push('checkpoint:' + phase);
      if (overrides.checkpointFailure === phase) fail('checkpoint','checkpoint-refused');
      return { ...baseManifest, checkpoint:index };
    },
    save:value=>{ saved.push(JSON.parse(JSON.stringify(value))); events.push('save:' + (value.firstFailure?.stage||value.status)); },
  };
  return { services, events, saved, acks };
}

test('successful owner observation acknowledges ready, A, B once and in order', async()=>{
  const f=makeServices();
  const result=await runObserver(config,f.services);
  assert.equal(result.status,'observed');
  assert.equal(result.installedAcceptance,'not-established');
  assert.deepEqual(result.observations.map(item=>item.phase),PHASES);
  assert.deepEqual(f.acks.map(item=>item.phase),PHASES);
  assert.ok(f.acks.every(item=>item.token==='d'.repeat(32)&&item.decision==='accepted'));
  assert.ok(f.events.indexOf('save:observed') > f.events.indexOf('ack:b-opened'));
  assert.ok(f.events.indexOf('cleanup') < f.events.indexOf('save:observed'));
  assert.equal(result.firstFailure,null);
});

const failureCases=[
  ['startup/config',{invalidConfig:true},'config','observer-config-refused'],
  ['driver receipt',{driverFailure:true},'driver-receipt','driver-receipt-refused'],
  ['task owner',{ownerFailure:true},'task-owner','task-owner-unavailable'],
  ['P38 manifest',{probeFailure:true},'probe-manifest','probe-manifest-validation-refused'],
  ['helper startup',{helperStartFailure:true},'helper-start','helper-start-failed'],
  ['helper pin',{helperPinFailure:true},'helper-pin','helper-window-identity-refused'],
  ['checkpoint',{checkpointFailure:'ready'},'checkpoint','checkpoint-refused'],
  ['inspection',{inspectFailure:'ready'},'inspection','inspection-refused'],
  ['owner acknowledgment',{ackFailure:'ready'},'acknowledgment','owner-ack-refused'],
];
for(const [label,options,stage,code] of failureCases) test('records sanitized first failure at '+label+' before cleanup',async()=>{
  const f=makeServices(options), candidate=options.invalidConfig?{...config,sourceHead:'invalid'}:config;
  const result=await runObserver(candidate,f.services);
  assert.deepEqual(result.firstFailure&&{stage:result.firstFailure.stage,code:result.firstFailure.code},{stage,code});
  assert.equal(f.saved[0].firstFailure.stage,stage);
  assert.equal(f.saved[0].firstFailure.code,code);
  assert.doesNotMatch(JSON.stringify(f.saved),/private native text|helperPath|ownerToken|token/);
  const cleanAt=f.events.indexOf('cleanup');
  if(cleanAt>=0) assert.ok(f.events.findIndex(event=>event.startsWith('save:')) < cleanAt);
});

test('visible refusal is acknowledged once and stops before the next phase',async()=>{
  const f=makeServices({visibleFailure:'a-opened'});
  const result=await runObserver(config,f.services);
  assert.equal(result.firstFailure,null);
  assert.deepEqual(result.observations.map(item=>[item.phase,item.decision]),[
    ['ready','accepted'],['a-opened','refused'],
  ]);
  assert.deepEqual(f.acks.map(item=>[item.phase,item.decision]),[
    ['ready','accepted'],['a-opened','refused'],
  ]);
  assert.equal(f.events.some(event=>event==='checkpoint:b-opened'),false);
});

test('helper cleanup failure is preserved after a successful observation',async()=>{
  const f=makeServices({cleanupFailure:true});
  const result=await runObserver(config,f.services);
  assert.equal(result.cleanup,'incomplete');
  assert.deepEqual(result.firstFailure&&{stage:result.firstFailure.stage,code:result.firstFailure.code},
    {stage:'cleanup',code:'helper-cleanup-incomplete'});
});

test('final receipt failure changes an observed outcome to partial',async()=>{
  const f=makeServices();
  f.services.save=()=>{ throw new Error('private filesystem detail'); };
  const result=await runObserver(config,f.services);
  assert.equal(result.status,'partial');
  assert.deepEqual(result.firstFailure&&{stage:result.firstFailure.stage,code:result.firstFailure.code},
    {stage:'receipt',code:'receipt-write-failed'});
});

test('incomplete cleanup after helper start refusal is recorded without replacing the primary failure',async()=>{
  const f=makeServices();
  f.services.startHelper=async()=>{
    const error=new Error('private executable detail');
    error.observerStage='helper-start'; error.observerCode='helper-executable-refused'; error.helperCleanupIncomplete=true;
    throw error;
  };
  const result=await runObserver(config,f.services);
  assert.equal(result.cleanup,'incomplete');
  assert.deepEqual(result.firstFailure&&{stage:result.firstFailure.stage,code:result.firstFailure.code},
    {stage:'helper-start',code:'helper-executable-refused'});
  assert.equal(f.saved[0].cleanup,'incomplete');
});

test('spawned helper termination waits for close after SIGTERM',async()=>{
  const child=new EventEmitter();
  child.exitCode=null; child.signalCode=null;
  const signals=[];
  child.kill=signal=>{ signals.push(signal); child.signalCode=signal; setImmediate(()=>child.emit('close',null,signal)); return true; };
  await stopSpawnedHelper(child);
  assert.deepEqual(signals,['SIGTERM']);
});

test('spawned helper termination escalates and reports when close never arrives',async()=>{
  const child=new EventEmitter();
  child.exitCode=null; child.signalCode=null;
  const signals=[];
  child.kill=signal=>{ signals.push(signal); return true; };
  await assert.rejects(stopSpawnedHelper(child),error=>error.observerCode==='helper-cleanup-incomplete');
  assert.deepEqual(signals,['SIGTERM','SIGKILL']);
});

test('driver first failure is terminal and no helper is started',async()=>{
  const f=makeServices();
  f.services.waitForDriver=async()=>({firstFailure:{code:'private'},schema:'nemo.p38-operator/1'});
  const result=await runObserver(config,f.services);
  assert.deepEqual(result.firstFailure&&{stage:result.firstFailure.stage,code:result.firstFailure.code},
    {stage:'driver-receipt',code:'driver-first-failure'});
  assert.equal(f.events.includes('helper-start'),false);
});

test('outcome writer persists sanitized receipt with private permissions',()=>{
  const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'p38-owner-observer-test-'));
  const directory=fs.realpathSync(temporary);
  try {
    const outcome={schema:'nemo.p38-owner-observation/2',issue:1645,grant:config.grantId,
      task:config.taskId,source:config.sourceHead,status:'partial',observations:[],
      firstFailure:{stage:'inspection',code:'inspection-refused',elapsedMs:4},
      cleanup:'pending',installedAcceptance:'not-established'};
    writeOutcome(directory,outcome);
    const file=path.join(directory,'owner-observation.json');
    assert.deepEqual(JSON.parse(fs.readFileSync(file,'utf8')),outcome);
    assert.equal(fs.statSync(file).mode&0o777,0o600);
    outcome.cleanup='complete';
    writeOutcome(directory,outcome);
    assert.equal(JSON.parse(fs.readFileSync(file,'utf8')).cleanup,'complete');
    assert.equal(fs.readdirSync(directory).some(name=>name.startsWith('.owner-observation-')),false);
    assert.doesNotMatch(fs.readFileSync(file,'utf8'),/ownerToken|d{32}/);
  } finally { fs.rmSync(temporary,{recursive:true,force:true}); }
});

test('helper executable is accepted only at its pinned path and digest',()=>{
  const directory=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'p38-helper-pin-test-')));
  const executable=path.join(directory,'helper');
  try {
    fs.writeFileSync(executable,'#!/bin/sh\nexit 0\n',{mode:0o700});
    const expected=require('node:crypto').createHash('sha256').update(fs.readFileSync(executable)).digest('hex');
    assert.equal(verifyHelperExecutable(executable,expected).sha256,expected);
    assert.throws(()=>verifyHelperExecutable(executable,'a'.repeat(64)),error=>
      error.observerStage==='helper-start'&&error.observerCode==='helper-executable-refused');
    fs.symlinkSync(executable,path.join(directory,'helper-link'));
    assert.throws(()=>verifyHelperExecutable(path.join(directory,'helper-link'),expected),error=>
      error.observerStage==='helper-start'&&error.observerCode==='helper-executable-refused');
    fs.writeFileSync(executable,'#!/bin/sh\nexit 1\n',{mode:0o700});
    assert.throws(()=>verifyHelperExecutable(executable,expected),error=>
      error.observerStage==='helper-start'&&error.observerCode==='helper-executable-refused');
  } finally { fs.rmSync(directory,{recursive:true,force:true}); }
});

test('helper receives only the two P38-scoped environment values',()=>{
  assert.deepEqual(helperEnvironment(config.grantId,'/private/report-root'),{
    NEMO_P38_GRANT:config.grantId,NEMO_P38_REPORT_ROOT:'/private/report-root'});
});
