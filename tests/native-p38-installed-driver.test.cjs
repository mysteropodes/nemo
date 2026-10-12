'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { runOperator, verifyIdentity, verifyWindow } = require('./desktop/p38-installed-driver.cjs');
const HEAD = 'a'.repeat(40), HASH = 'b'.repeat(64);
function config() { return { workflow:'P38', grant:{issue:1645,confirmed:true,id:'new-p38-explicit-grant'},
  taskId:'p38-synthetic',sourceHead:HEAD,timeoutMs:1000 }; }
function fixture(change = {}) {
  const cfg=config(), events=[], receipts=[]; let fire, attempts=0, cleaned=0, used=false;
  const instance={task:cfg.taskId};
  const snapshot={ok:true,taskId:cfg.taskId,source:{matches:true,startup:{head:HEAD}},build:{matches:true},
    runtime:{childPid:123},app:{pid:123,taskId:cfg.taskId,isolated:true,state:'active'}};
  const actions=[{type:'inspect'},{type:'press',element:'0.1'},{type:'set',element:'0.2',value:'private-path'},
    {type:'capture'},{type:'stop'}];
  const controller={async start(){events.push('start');attempts++;if(change.start)return change.start(fire);return instance;},
    async status(){events.push('status');return {value:change.snapshot?change.snapshot(structuredClone(snapshot)):structuredClone(snapshot)};},
    async cleanup(){events.push('cleanup');cleaned++;if(change.cleanup)throw Error('/private/secret');}};
  const services={async package(){events.push('package');if(change.package)throw Error('/private/pkg');return {executableSha256:HASH,proofHash:HASH};},
    async journal(){events.push('journal');if(used)throw Object.assign(Error('duplicate'),{code:'grant-already-attempted'});used=true;},
    save(r){events.push('save-'+(r.appPid||'pre-pid'));receipts.push(structuredClone(r));},
    controller(){return controller;},validate(){events.push('validate');if(change.validate)throw Error('wrong owner');},
    async window(mode,pid,wid,action){events.push('window-'+mode);if(change.window)return change.window(mode,pid,wid,action,fire);
      return {pid,windowId:45,matchedAX:true,...mode==='capture'?{captureSha256:HASH}:{}};},
    async next(){return actions.shift();},setTimer(fn){events.push('watchdog');fire=fn;return 0;},clearTimer(){events.push('timer-clear');},
    async close(){events.push('helpers-close');}};
  return {cfg,services,events,receipts,get attempts(){return attempts;},get cleaned(){return cleaned;}};
}
test('synthetic one-run persists flat PID before discovery/UI and always uses exact window',async()=>{
  const f=fixture(),result=await runOperator(f.cfg,f.services);
  assert.equal(result.status,'operator-complete');assert.equal(result.installedAcceptance,'not-established');
  assert.equal(result.appPid,123);assert.equal(result.windowId,45);assert.equal(result.cleanup,'complete');
  assert.equal(f.attempts,1);assert.equal(f.cleaned,1);
  assert.ok(f.events.indexOf('watchdog')<f.events.indexOf('start'));
  assert.ok(f.events.indexOf('save-123')<f.events.indexOf('window-discover'));
  assert.equal(f.events.filter(e=>e==='window-check').length,4);assert.equal(result.actions.length,4);
  assert.equal(JSON.stringify(result).includes('private-path'),false);
});
for(const grant of [null,{issue:1573,confirmed:true,id:'N20R'}, {issue:1645,confirmed:false,id:'pending'},
  {issue:1645,confirmed:true,id:'6121922271'}])test('pending/foreign/consumed grant cannot access package or launch: '+JSON.stringify(grant),async()=>{
  const f=fixture();f.cfg.grant=grant;await assert.rejects(runOperator(f.cfg,f.services));assert.deepEqual(f.events,[]);
});
for(const mode of ['nested','missing','negative','wrong-task','wrong-child','wrong-source','source-mismatch'])test('synthetic manifest '+mode+' refuses before any window operation',async()=>{
  const f=fixture({snapshot(s){
    if(mode==='nested'){s.app={manifest:s.app};}if(mode==='missing')delete s.app.pid;
    if(mode==='negative')s.app.pid=-1;if(mode==='wrong-task')s.app.taskId='foreign';if(mode==='wrong-child')s.runtime.childPid=456;
    if(mode==='wrong-source')s.source.startup.head='c'.repeat(40);if(mode==='source-mismatch')s.source.matches=false;return s;}});
  const r=await runOperator(f.cfg,f.services);assert.equal(r.status,'partial');assert.equal(r.appPid,null);assert.equal(f.cleaned,1);
  assert.equal(f.events.some(e=>e.startsWith('window-')),false);
});
for(const mode of ['pid','id','ax'])test('synthetic discovery rejects '+mode+' mismatch and cleans owner',async()=>{
  const f=fixture({window(){return {pid:mode==='pid'?456:123,windowId:mode==='id'?0:45,matchedAX:mode!=='ax'};}});
  const r=await runOperator(f.cfg,f.services);assert.equal(r.status,'partial');assert.equal(r.windowId,null);assert.equal(f.cleaned,1);
});
test('a changed window between actions refuses instead of selecting another window',async()=>{
  const f=fixture({window(mode){return {pid:123,windowId:mode==='discover'?45:46,matchedAX:true};}});
  const r=await runOperator(f.cfg,f.services);assert.equal(r.status,'partial');assert.equal(r.actions.length,0);assert.equal(f.cleaned,1);
});
test('partial start rejection still invokes official controller cleanup',async()=>{
  const f=fixture({start(){throw Error('/private/partial-start');}});const r=await runOperator(f.cfg,f.services);
  assert.equal(f.attempts,1);assert.equal(f.cleaned,1);assert.equal(r.cleanup,'complete');assert.equal(r.status,'partial');assert.equal(r.reason,'operator-failed');
});
for(const phase of ['start','window'])test('prelaunch watchdog bounds held '+phase+' and cleans exactly once',async()=>{
  const hold=fire=>{fire();return new Promise(()=>{});};const f=fixture(phase==='start'?{start:hold}:{window(mode,pid,wid,action,fire){return hold(fire);}});
  const r=await runOperator(f.cfg,f.services);assert.equal(r.reason,'watchdog-timeout');assert.equal(f.cleaned,1);assert.equal(f.attempts,1);
  assert.ok(f.events.includes('timer-clear'));
});
test('cleanup failure is explicit partial, never a complete installed result',async()=>{
  const f=fixture({cleanup:true});const r=await runOperator(f.cfg,f.services);assert.equal(r.cleanup,'incomplete');assert.equal(r.status,'partial');
  assert.equal(r.reason,'cleanup-incomplete');assert.equal(JSON.stringify(r).includes('/private/secret'),false);
});
test('a reused grant journal never launches a second process',async()=>{
  const f=fixture();await runOperator(f.cfg,f.services);const r=await runOperator(f.cfg,f.services);
  assert.equal(r.reason,'grant-already-attempted');assert.equal(f.attempts,1);
});
test('package proof refusal does not acquire native/window authority',async()=>{
  const f=fixture({package:true});const r=await runOperator(f.cfg,f.services);assert.equal(f.attempts,0);assert.equal(f.cleaned,0);
  assert.equal(r.status,'partial');assert.equal(f.events.some(e=>e.startsWith('window-')),false);
});
test('static owned-window adapter excludes global input and requires exact PID/window/AX actions',()=>{
  const s=fs.readFileSync('tests/desktop/p38-owned-window.swift','utf8');
  assert.ok(s.includes('owned.count == 1'));assert.ok(s.includes('AXUIElementGetPid'));assert.ok(s.includes('UInt32(args[2]) == number'));
  assert.ok(s.includes('"-l" + String(number)'));assert.equal(/CGEvent|SystemWide|osascript/.test(s),false);
  assert.ok(s.includes('AXPress unsupported; no fallback'));
});
test('flat identity and window validators reject unsafe scalar coercions',()=>{
  const f=fixture();assert.throws(()=>verifyIdentity({app:{pid:'123'}},{task:f.cfg.taskId},f.cfg));
  assert.throws(()=>verifyWindow({pid:123,windowId:'45',matchedAX:true},123));
});
test('helper cleanup failure does not skip official native cleanup or report complete cleanup',async()=>{
  const f=fixture();f.services.close=async()=>{throw Error('helper group alive');};
  const r=await runOperator(f.cfg,f.services);assert.equal(f.cleaned,1);assert.equal(r.cleanup,'incomplete');
  assert.equal(r.helperCleanup,'incomplete');assert.equal(r.status,'partial');
});
test('changed runtime PID after discovery cannot dispatch any interaction',async()=>{
  let reads=0;const f=fixture({snapshot(s){if(++reads>1){s.app.pid=456;s.runtime.childPid=456;}return s;}});
  const r=await runOperator(f.cfg,f.services);assert.equal(r.reason,'native-pid-changed');assert.equal(r.actions.length,0);
  assert.equal(f.events.some(x=>x==='window-inspect'),false);assert.equal(f.cleaned,1);
});
test('operator cannot request a global-input or another-window action',async()=>{
  const f=fixture();f.services.next=async()=>({type:'click-global',windowId:99});
  const r=await runOperator(f.cfg,f.services);assert.equal(r.reason,'action-refused');assert.equal(r.actions.length,0);
});
test('unknown helper capture proof cannot leak arbitrary scalar text into receipt',async()=>{
  const f=fixture({window(mode,pid){return {pid,windowId:45,matchedAX:true,...mode==='capture'?{captureSha256:'/private/path'}:{}};}});
  const r=await runOperator(f.cfg,f.services);assert.equal(r.reason,'capture-proof-refused');assert.equal(JSON.stringify(r).includes('/private/path'),false);
});
const os=require('node:os'),path=require('node:path');
const {safeDirectory,atomicJSON}=require('./desktop/p38-installed-driver.cjs');
test('preexisting symlink report leaf refuses without writing foreign evidence',()=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'p38-reports-')),foreign=fs.mkdtempSync(path.join(os.tmpdir(),'p38-foreign-'));
  try{fs.mkdirSync(path.join(root,'reports'));fs.symlinkSync(foreign,path.join(root,'reports','grant'));
    assert.throws(()=>safeDirectory(root,['reports','grant']));assert.deepEqual(fs.readdirSync(foreign),[]);
  }finally{fs.rmSync(root,{recursive:true,force:true});fs.rmSync(foreign,{recursive:true,force:true});}
});
test('replacement of pinned report ancestor is refused before evidence output',()=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'p38-identity-')),foreign=fs.mkdtempSync(path.join(os.tmpdir(),'p38-outside-'));
  try{const guard=safeDirectory(root,['reports','grant']);fs.renameSync(guard.directory,guard.directory+'-old');fs.symlinkSync(foreign,guard.directory);
    assert.throws(()=>atomicJSON(guard,'receipt.json',{secret:'must-not-write'}));assert.deepEqual(fs.readdirSync(foreign),[]);
  }finally{fs.rmSync(root,{recursive:true,force:true});fs.rmSync(foreign,{recursive:true,force:true});}
});
test('atomic receipt updates do not follow a malicious receipt symlink',()=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'p38-receipt-'));try{
    const guard=safeDirectory(root,['reports']);atomicJSON(guard,'receipt.json',{phase:1});atomicJSON(guard,'receipt.json',{phase:2});
    assert.deepEqual(JSON.parse(fs.readFileSync(path.join(guard.directory,'receipt.json'))),{phase:2});
    const foreign=path.join(root,'foreign');fs.writeFileSync(foreign,'unchanged');fs.unlinkSync(path.join(guard.directory,'receipt.json'));
    fs.symlinkSync(foreign,path.join(guard.directory,'receipt.json'));assert.throws(()=>atomicJSON(guard,'receipt.json',{phase:3}));
    assert.equal(fs.readFileSync(foreign,'utf8'),'unchanged');
  }finally{fs.rmSync(root,{recursive:true,force:true});}
});
test('static persisted AX evidence is restricted to known control/alert facts',()=>{
  const s=fs.readFileSync('tests/desktop/p38-owned-window.swift','utf8');assert.ok(s.includes('nodes.append(facts)'));
  assert.equal(/facts\["(?:title|value)"\]/.test(s),false);assert.ok(s.includes('switch-unavailable'));
  const d=fs.readFileSync('tests/desktop/p38-installed-driver.cjs','utf8');assert.ok(d.includes('OPERATOR ASSERTION'));
});
test('JS persistence boundary strips arbitrary helper fields and AX values',()=>{
  const {safeWindowEvidence}=require('./desktop/p38-installed-driver.cjs');
  const input={pid:123,windowId:45,matchedAX:true,ownerToken:'private-token',nodes:[{path:'0.1',role:'AXStaticText',
    control:'Open',alert:'switch-unavailable',value:'/private/document',title:'secret',document:{layers:[]}},
    {path:'0.2',role:'/private/role',control:'private text',alert:'other'}]};
  const clean=safeWindowEvidence(input);assert.equal(JSON.stringify(clean).includes('private'),false);
  assert.deepEqual(clean.nodes[0],{path:'0.1',role:'AXStaticText',control:'Open',alert:'switch-unavailable'});
  assert.deepEqual(clean.nodes[1],{path:'0.2',role:'other'});
});
test('invalid or oversized AX paths cannot enter persisted evidence',()=>{
  const {safeWindowEvidence}=require('./desktop/p38-installed-driver.cjs');
  for(const path of ['/private/path','0.'.repeat(100),'a'])assert.throws(()=>safeWindowEvidence({nodes:[{path}]}));
  assert.throws(()=>safeWindowEvidence({nodes:Array(4097).fill({path:''})}));
});
