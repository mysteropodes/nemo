'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const { EventEmitter } = require('node:events');
const { PHASES, runObserver, fail, writeOutcome, verifyHelperExecutable, helperEnvironment, stopSpawnedHelper } = require('./p38-fixture-open-owner.cjs');

const config = Object.freeze({ taskId:'p38-task-test', grantId:'p38-grant-test',
  helperPath:'/tmp/p38-window-helper', helperSha256:'e'.repeat(64), sourceHead:'b'.repeat(40), timeoutMs:10000 });
const baseManifest = Object.freeze({ schema:'nemo.p38-fixture-open/1',
  runId:'12345678-1234-1234-1234-123456789abc', taskKey:'a'.repeat(64),
  sourceSha:config.sourceHead, buildSha:config.sourceHead, pid:42, window:'main',
  origin:'tauri://localhost/index.html', fixtures:[{id:'A'},{id:'B'}], manifestSha256:'c'.repeat(64),
  waiting:true, failed:false, checkpoint:0 });
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
  ['P38 manifest',{probeFailure:true},'probe-manifest','probe-manifest-refused'],
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
