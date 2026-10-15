'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');
const {SCHEMA,STAGES,safeDiscoveryDiagnostics}=require('./p38-discovery-diagnostics.cjs');
const {runOperator,createWindowTransport,safeDirectory,atomicJSON,cleanupHelper}=require('./p38-installed-driver.cjs');
const {fixture}=require('./p38-operator-model.cjs');
const booleanFields={arguments:'valid',pid:'available','process-start':'available','cg-window-id':'available',
  'report-path':'valid','report-kind':'valid','cg-second-query':'available','cg-bounds':'valid','ax-trust':'trusted'};
function diagnostic(stage) {
  return {schema:SCHEMA,stage,...(booleanFields[stage]?{[booleanFields[stage]]:false}:
    stage==='cg-first'?{available:true,countBucket:0}:stage==='cg-second-count'?{countBucket:2}:
    stage==='ax-match'?{queryClass:'cannot-complete',valueClass:'missing',candidates:0,matches:0}:{})};
}
function refuse(facts) {return {status:'refused',code:'adapter-refused',...(facts===undefined?{}:{discoveryDiagnostics:facts})};}
function environment(t,frames,executable) {
  const root=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'p38-discovery-test-'))),helpers=new Set();
  t.after(async()=>{for(const child of [...helpers])await cleanupHelper(child);fs.rmSync(root,{recursive:true,force:true});});
  const guard=safeDirectory(root,['reports']);let helper=executable;
  if(!helper) {
    helper=path.join(root,'fixture-helper');
    fs.writeFileSync(helper,'#!/usr/bin/env node\nconst frames='+JSON.stringify(frames)+';\n'+
      'function emit(){const f=frames.shift();if(!f)return;require("node:fs").writeSync(1,JSON.stringify(f)+"\\n");if(f.status==="refused")process.exit(1);}\n'+
      'emit();require("node:readline").createInterface({input:process.stdin}).on("line",emit);\n',{mode:0o700});
  }
  const fixtures=['A','B'].map(label=>{
    const file=path.join(__dirname,'../fixtures/p38/native-project',label+'.json');
    return {label,path:file,sha256:crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex')};
  });
  const transport=createWindowTransport({workflow:'P38',windowHelper:helper,grant:{id:'source-model-only'},fixtures},
    guard.directory,()=>guard,helpers,3000);
  return {guard,transport,helpers};
}
async function failureBeforeCleanup(t,facts,{executable,invalidPid=false,cleanupFails=false}={}) {
  const e=environment(t,[refuse(facts)],executable),f=fixture();let durableBeforeClose=false;
  f.services.window=()=>e.transport('discover',invalidPid?-1:123);
  const save=f.services.save;
  f.services.save=value=>{save(value);atomicJSON(e.guard,'receipt.json',value);};
  f.services.close=async()=>{
    f.events.push('helpers-close');
    const saved=JSON.parse(fs.readFileSync(path.join(e.guard.directory,'receipt.json')));
    assert.equal(saved.status,'partial');assert.equal(saved.firstFailure.stage,'identity');
    durableBeforeClose=true;
    for(const child of [...e.helpers])await cleanupHelper(child);
    if(cleanupFails)throw Error('/private/cleanup-error');
  };
  const result=await runOperator(f.cfg,f.services);
  assert.equal(durableBeforeClose,true);assert.equal(f.attempts,1);assert.equal(f.cleaned,1);
  assert.equal(result.windowId,null);assert.deepEqual(result.actions,[]);
  assert.ok(f.events.indexOf('save-failure')<f.events.indexOf('helpers-close'));
  assert.ok(f.events.indexOf('helpers-close')<f.events.indexOf('cleanup'));
  return result;
}

test('actual fixture writes its complete refusal before exit under asynchronous pipe backpressure',t=>{
  const facts=diagnostic('pid'),e=environment(t,[refuse(facts)]),root=path.dirname(e.guard.directory);
  const preload=path.join(root,'pipe-backpressure.cjs');
  fs.writeFileSync(preload,'const write=process.stdout._write.bind(process.stdout);\n'+
    'process.stdout._write=(chunk,encoding,callback)=>setTimeout(()=>write(chunk,encoding,callback),25);\n');
  const result=spawnSync(process.execPath,['--require',preload,path.join(root,'fixture-helper')],
    {encoding:'utf8',timeout:2000});
  assert.equal(result.error,undefined);assert.equal(result.status,1);
  assert.deepEqual(JSON.parse(result.stdout),refuse(facts));assert.equal(result.stderr,'');
});

for(const stage of STAGES)test(`structured ${stage} refusal crosses real transport and is durable before cleanup`,async t=>{
  const facts=diagnostic(stage),r=await failureBeforeCleanup(t,facts);
  assert.equal(r.firstFailure.code,'adapter-refused');assert.deepEqual(r.firstFailure.discoveryDiagnostics,facts);
});

test('original discovery evidence survives subsequent helper/owner cleanup failure',async t=>{
  const facts=diagnostic('ax-trust'),r=await failureBeforeCleanup(t,facts,{cleanupFails:true});
  assert.deepEqual(r.firstFailure.discoveryDiagnostics,facts);assert.equal(r.helperCleanup,'incomplete');
  assert.equal(r.firstFailure.code,'adapter-refused');assert.doesNotMatch(JSON.stringify(r),/private/);
});

const bad=[null,[],{},'/private/value',
  {...diagnostic('scope'),stage:'private-stage'},
  {...diagnostic('scope'),schema:'other'},
  {...diagnostic('scope'),title:'private-title'},
  {...diagnostic('scope'),pid:123},
  {...diagnostic('scope'),path:'/private/file'},
  {...diagnostic('ax-trust'),trusted:true},
  {...diagnostic('pid'),available:'false'},
  {...diagnostic('cg-first'),countBucket:1},
  {...diagnostic('cg-first'),available:false,countBucket:2},
  {...diagnostic('cg-second-count'),countBucket:3},
  {...diagnostic('cg-second-count'),countBucket:0.5},
  {...diagnostic('ax-match'),queryClass:'/private/error'},
  {...diagnostic('ax-match'),valueClass:'array',candidates:2},
  {...diagnostic('ax-match'),queryClass:'success',valueClass:'array',candidates:1,matches:2},
  {...diagnostic('ax-match'),queryClass:'success',valueClass:'array',candidates:1,matches:1},
  {...diagnostic('ax-match'),valueClass:'wrong-type'},
];
for(const [index,value]of bad.entries())test(`malformed discovery ${index} refuses without retaining arbitrary fields`,async t=>{
  assert.throws(()=>safeDiscoveryDiagnostics(value),{code:'helper-malformed'});
  const r=await failureBeforeCleanup(t,value);
  assert.equal(r.firstFailure.code,'helper-malformed');assert.equal(r.firstFailure.discoveryDiagnostics,undefined);
  assert.doesNotMatch(JSON.stringify(r),/private|other|false"/);
});

test('valid AX/count witnesses are bounded buckets, including zero and multiple matches',()=>{
  for(const available of [false,true])assert.deepEqual(safeDiscoveryDiagnostics({schema:SCHEMA,stage:'cg-first',available,countBucket:0}),
    {schema:SCHEMA,stage:'cg-first',available,countBucket:0});
  for(const queryClass of ['success','unsupported','no-value','cannot-complete','other-error'])
    assert.ok(safeDiscoveryDiagnostics({...diagnostic('ax-match'),queryClass}));
  for(const candidates of [0,1,2])assert.ok(safeDiscoveryDiagnostics({schema:SCHEMA,stage:'ax-match',
    queryClass:'success',valueClass:'array',candidates,matches:0}));
  assert.ok(safeDiscoveryDiagnostics({schema:SCHEMA,stage:'ax-match',queryClass:'success',valueClass:'array',candidates:2,matches:2}));
});

test('diagnostic getters and symbol/private properties are refused without reading them',()=>{
  let reads=0;const value={schema:SCHEMA,get stage(){reads++;return 'scope';}};
  assert.throws(()=>safeDiscoveryDiagnostics(value),{code:'helper-malformed'});assert.equal(reads,0);
  assert.throws(()=>safeDiscoveryDiagnostics({...diagnostic('scope'),[Symbol('private')]:true}),{code:'helper-malformed'});
});

test('legacy refusal is unchanged and receives no fabricated discovery credit',async t=>{
  const r=await failureBeforeCleanup(t,undefined);
  assert.equal(r.firstFailure.code,'adapter-refused');assert.equal(r.firstFailure.discoveryDiagnostics,undefined);
});

test('successful pin/check wire shape remains unchanged; discovery data cannot appear after pin',async t=>{
  const pinned={status:'pinned',pid:123,windowId:45,matchedAX:true};
  const e=environment(t,[pinned,{status:'ok',type:'check',pid:123,windowId:45,matchedAX:true},refuse(diagnostic('scope'))]);
  assert.deepEqual(await e.transport('discover',123),pinned);
  assert.deepEqual(await e.transport('check',123,45),{status:'ok',type:'check',pid:123,windowId:45,matchedAX:true});
  await assert.rejects(e.transport('check',123,45),{code:'helper-malformed'});
});

test('a pin carrying refusal diagnostics is malformed rather than successful',async t=>{
  const e=environment(t,[{status:'pinned',pid:123,windowId:45,matchedAX:true,discoveryDiagnostics:diagnostic('scope')}]);
  await assert.rejects(e.transport('discover',123),{code:'helper-malformed'});
});

// Explicit compiled-producer control under a booked Swift slot. Invalid PID fails
// before process-start, CG or AX access; no application or window is queried.
if(process.env.NEMO_P38_DISCOVERY_HELPER)test('compiled Swift refusal traverses actual transport and persists before cleanup',async t=>{
  const r=await failureBeforeCleanup(t,undefined,{executable:process.env.NEMO_P38_DISCOVERY_HELPER,invalidPid:true});
  assert.equal(r.firstFailure.code,'adapter-refused');
  assert.deepEqual(r.firstFailure.discoveryDiagnostics,diagnostic('pid'));
});
