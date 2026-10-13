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
  const actions=[{type:'inspect'},{type:'press',element:'0.1'},{type:'inspect',element:'0.2'},
    {type:'capture'},{type:'stop'}];
  const controller={async start(){events.push('start');attempts++;if(change.start)return change.start(fire);return instance;},
    async status(){events.push('status');return {value:change.snapshot?change.snapshot(structuredClone(snapshot)):structuredClone(snapshot)};},
    async cleanup(){events.push('cleanup');cleaned++;if(change.cleanup)throw Error('/private/secret');}};
  const services={async transport(){events.push('transport');if(typeof change.transport==='function')return change.transport(fire);
    if(change.transport)throw Object.assign(Error('transport'),{code:'transport-handshake-refused'});},
    async package(){events.push('package');if(change.package)throw Error('/private/pkg');return {executableSha256:HASH,proofHash:HASH};},
    async journal(){events.push('journal');if(used)throw Object.assign(Error('duplicate'),{code:'grant-already-attempted'});used=true;},
    save(r){events.push('save-'+(r.appPid||'pre-pid'));receipts.push(structuredClone(r));},feedback(){events.push('ephemeral-feedback');},
    controller(){return controller;},validate(){events.push('validate');if(change.validate)throw Error('wrong owner');},
    async window(mode,pid,wid,action){events.push('window-'+mode);if(change.window)return change.window(mode,pid,wid,action,fire);
      return {pid,windowId:45,matchedAX:true,...mode==='capture'?{captureSha256:HASH}:{}};},
    async next(){return actions.shift();},setTimer(fn){events.push('watchdog');fire=fn;return 0;},clearTimer(){events.push('timer-clear');},
    async close(){events.push('helpers-close');if(change.close)throw Error('/private/helper-secret');}};
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
test('bounded operator think time is separate from action time and each action receipt is timed',async()=>{
  const f=fixture();let step=0;f.services.next=async()=>{
    await new Promise(resolve=>setTimeout(resolve,25));
    return step++===0?{type:'inspect'}:{type:'stop'};
  };
  const r=await runOperator(f.cfg,f.services);
  assert.equal(r.status,'operator-complete');assert.equal(r.actions.length,1);
  assert.ok(r.actions[0].operatorWaitMs>=15);assert.ok(r.actions[0].durationMs>=0);
  assert.ok(r.operatorWaitMs>=r.actions[0].operatorWaitMs);
  assert.equal(r.cleanup,'complete');assert.equal(f.attempts,1);
});
test('global watchdog timeout while waiting for the operator is truthfully staged and cleaned',async()=>{
  const f=fixture();f.services.next=async()=>{f.services._fire();return new Promise(()=>{});};
  const original=f.services.setTimer;f.services.setTimer=(fn)=>{f.services._fire=fn;return original(fn);};
  const r=await runOperator(f.cfg,f.services);
  assert.equal(r.status,'partial');assert.equal(r.reason,'watchdog-timeout');
  assert.equal(r.firstFailure.stage,'awaiting-operator-input');assert.equal(r.firstFailure.action,undefined);
  assert.ok(Number.isInteger(r.operatorWaitTimedOutMs));assert.equal(r.operatorWaitMs,r.operatorWaitTimedOutMs);
  assert.equal(r.cleanup,'complete');assert.equal(r.helperCleanup,'complete');assert.equal(f.attempts,1);
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
  assert.equal(r.firstFailure.code,r.reason);assert.equal(r.firstFailure.stage,'identity');
  assert.equal(f.events.some(e=>e.startsWith('window-')),false);
});
for(const mode of ['pid','id','ax'])test('synthetic discovery rejects '+mode+' mismatch and cleans owner',async()=>{
  const f=fixture({window(){return {pid:mode==='pid'?456:123,windowId:mode==='id'?0:45,matchedAX:mode!=='ax'};}});
  const r=await runOperator(f.cfg,f.services);assert.equal(r.status,'partial');assert.equal(r.windowId,null);assert.equal(f.cleaned,1);
  assert.equal(r.firstFailure.code,'owned-window-refused');assert.equal(r.firstFailure.stage,'identity');assert.ok(Number.isInteger(r.firstFailure.elapsedMs));
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
  assert.equal(r.reason,'cleanup-incomplete');assert.equal(r.firstFailure.code,'cleanup-incomplete');assert.equal(r.firstFailure.stage,'owner-cleanup');
  assert.equal(JSON.stringify(r).includes('/private/secret'),false);
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
  assert.ok(s.includes('initialRows.count == 1'));assert.ok(s.includes('AXUIElementGetPid'));assert.ok(s.includes('CFEqual(parent,main)'));
  assert.ok(s.includes('"-l"+String(number)'));assert.equal(/CGEvent|SystemWide|osascript/.test(s),false);
  assert.ok(s.includes('confirm-unsupported'));assert.ok(s.includes('No title/value/children reads from a foreign child'));
});
test('flat identity and window validators reject unsafe scalar coercions',()=>{
  const f=fixture();assert.throws(()=>verifyIdentity({app:{pid:'123'}},{task:f.cfg.taskId},f.cfg));
  assert.throws(()=>verifyWindow({pid:123,windowId:'45',matchedAX:true},123));
});
test('helper cleanup failure does not skip official native cleanup or report complete cleanup',async()=>{
  const f=fixture();f.services.close=async()=>{throw Error('helper group alive');};
  const r=await runOperator(f.cfg,f.services);assert.equal(f.cleaned,1);assert.equal(r.cleanup,'complete');
  assert.equal(r.helperCleanup,'incomplete');assert.equal(r.status,'partial');
  assert.equal(r.firstFailure.code,'helper-cleanup-incomplete');assert.equal(r.firstFailure.stage,'helper-cleanup');
});
test('first exact-sheet refusal survives both cleanup failures and is saved before cleanup',async()=>{
  const token='da5f47f9-5681-4e71-a0b6-aa7578b6b5fa',f=fixture({close:true,cleanup:true});let step=0;
  f.cfg.fixtures=[{path:'/private/fixture-A.json'}];
  f.services.next=async()=>step++===0?{type:'pin-sheet'}:{type:'set',target:'sheet',sheetToken:token,element:'0.7',value:f.cfg.fixtures[0].path};
  f.services.window=async(mode,pid)=>({pid,windowId:45,matchedAX:true,
    ...(mode==='pin-sheet'?{target:'sheet',sheetToken:token}:{}),
    ...(mode==='set'?{status:'refused',code:'confirm-unsupported',value:'/private/helper-secret'}:{})});
  const close=f.services.close;
  f.services.close=async()=>{assert.ok(f.receipts.some(x=>x.firstFailure?.code==='confirm-unsupported'));
    return close();};
  const r=await runOperator(f.cfg,f.services);
  assert.equal(r.status,'partial');assert.equal(r.reason,'confirm-unsupported');
  assert.equal(r.firstFailure.code,'confirm-unsupported');assert.equal(r.firstFailure.stage,'window-set-sheet');
  assert.deepEqual(r.firstFailure.action,{type:'set',ordinal:2,durationMs:r.firstFailure.action.durationMs});
  assert.equal(r.helperCleanup,'incomplete');assert.equal(r.cleanup,'incomplete');
  assert.equal(f.attempts,1);assert.equal(f.cleaned,1);
  const saved=f.receipts.find(x=>x.firstFailure?.code==='confirm-unsupported');
  assert.ok(saved);assert.equal(saved.stage,'window-set-sheet');
  for(const value of [r,...f.receipts]){
    const serialized=JSON.stringify(value);
    for(const secret of [token,'/private/fixture-A.json','/private/helper-secret'])assert.equal(serialized.includes(secret),false);
  }
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
  const s=fs.readFileSync('tests/desktop/p38-owned-window.swift','utf8');assert.ok(s.includes('try project(child,childPath,level+1)'));
  assert.equal(/facts\["(?:title|value)"\]/.test(s),false);assert.ok(s.includes('switch-unavailable'));
  assert.ok(s.includes('projectionLimit(depth,maxNodes)'));assert.ok(s.includes('rowSelectedAfter'));
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
  const identity={pid:123,windowId:45,matchedAX:true};
  for(const path of ['/private/path','0.'.repeat(100),'a'])assert.throws(()=>safeWindowEvidence({...identity,nodes:[{path}]}));
  assert.throws(()=>safeWindowEvidence({...identity,nodes:Array(4097).fill({path:''})}));
});
test('malformed helper identity scalars refuse at the JS evidence boundary',()=>{
  const {safeWindowEvidence}=require('./desktop/p38-installed-driver.cjs');
  const identity={pid:123,windowId:45,matchedAX:true,nodes:[]};
  for(const field of ['pid','windowId','matchedAX']) for(const value of ['/private/secret',{},[],null]) {
    assert.throws(()=>safeWindowEvidence({...identity,[field]:value}),error=>
      ['ax-evidence-refused','owned-window-refused'].includes(error.code) && !error.message.includes('private'));
  }
  for(const pid of [0,-1,1.5,2147483648])assert.throws(()=>safeWindowEvidence({...identity,pid}));
  for(const windowId of [0,-1,1.5,4294967296])assert.throws(()=>safeWindowEvidence({...identity,windowId}));
  assert.throws(()=>safeWindowEvidence({...identity,matchedAX:false}));
  assert.deepEqual(safeWindowEvidence(identity),identity);
});

test('transport refusal/EOF/watchdog occurs before package, journal or app start',async()=>{
  for(const transport of [true, fire=>{fire();return new Promise(()=>{});}]){
    const f=fixture({transport}),r=await runOperator(f.cfg,f.services);
    assert.equal(r.status,'partial');assert.equal(f.attempts,0);
    assert.equal(f.events.includes('package'),false);assert.equal(f.events.includes('journal'),false);
    assert.equal(f.events.includes('helpers-close'),true);
  }
});
test('nonce handshake admits delayed reply only on explicit PTY, with zero modeled effects',async()=>{
  const {transportReady}=require('./desktop/p38-installed-driver.cjs'),written=[],nonce='fixed-model-nonce';
  await transportReady(async()=>{await Promise.resolve();return{transport:'p38-ready',nonce};},r=>written.push(r),true,nonce);
  assert.deepEqual(written.map(r=>r.status),['waiting','ready']);assert.ok(written.every(r=>r.startAttempts===0&&r.journalWrites===0));
  let reads=0;await assert.rejects(transportReady(async()=>{reads++;},()=>{},false),e=>e.code==='transport-pty-required');assert.equal(reads,0);
  for(const reply of [{transport:'p38-ready',nonce:'foreign'},{transport:'N20R',nonce},null]){
    await assert.rejects(transportReady(async()=>reply,()=>{},true,nonce),e=>e.code==='transport-handshake-refused');
  }
  await assert.rejects(transportReady(async()=>{throw Object.assign(Error('EOF'),{code:'operator-input-ended'});},()=>{},true,nonce));
});
test('real no-app CLI refuses non-PTY transport probe before any nonce or journal',()=>{
  const {spawnSync}=require('node:child_process'),p=spawnSync(process.execPath,['tests/desktop/p38-installed-driver.cjs','--transport-probe'],{encoding:'utf8',timeout:5000});
  assert.equal(p.status,1);assert.equal(p.stdout,'');assert.match(p.stderr,/operator refused/);
});
test('positive main/direct-sheet pin/routes/dismissal preserve one main and one start',async()=>{
  const token='da5f47f9-5681-4e71-a0b6-aa7578b6b5fa',f=fixture();
  const actions=[{type:'pin-sheet'},{type:'inspect',target:'sheet',sheetToken:token,element:''},
    {type:'select',target:'sheet',sheetToken:token,element:'0.1'},{type:'press',target:'sheet',sheetToken:token,element:'0.2'},
    {type:'sheet-dismissed',sheetToken:token},{type:'inspect'},{type:'stop'}];
  f.services.next=async()=>actions.shift();f.services.window=async(mode,pid,wid,a={})=>({pid,windowId:45,matchedAX:true,
    ...(mode==='pin-sheet'?{target:'sheet',sheetToken:token}:{}),...(a.target==='sheet'?{target:'sheet',sheetToken:token}:{}),
    ...(mode==='sheet-dismissed'?{dismissed:true}:{})});
  const r=await runOperator(f.cfg,f.services);assert.equal(r.status,'operator-complete');assert.equal(r.appPid,123);assert.equal(r.windowId,45);
  assert.equal(f.attempts,1);assert.equal(f.cleaned,1);assert.equal(r.installedAcceptance,'not-established');
});
test('next action learns fresh token from ephemeral output, with no token/private receipt evidence',async()=>{
  const token=require('node:crypto').randomUUID(),f=fixture(),messages=[];let step=0,learned;
  f.services.feedback=async reply=>{messages.push(structuredClone(reply));learned=reply.token;};
  f.services.next=async()=>{
    if(step++===0)return{type:'pin-sheet'};
    assert.equal(learned,token);
    if(step===2)return{type:'inspect',target:'sheet',sheetToken:learned};
    if(step===3)return{type:'sheet-dismissed',sheetToken:learned};return{type:'stop'};
  };
  f.services.window=async(mode,pid,wid,a={})=>({pid,windowId:45,matchedAX:true,
    ...(mode==='pin-sheet'?{target:'sheet',sheetToken:token,ownerToken:'private-owner',value:'private-file-path'}:{}),
    ...(a.target==='sheet'?{target:'sheet',sheetToken:token}:{}),...(mode==='sheet-dismissed'?{dismissed:true}:{})});
  const receipt=await runOperator(f.cfg,f.services);assert.equal(receipt.status,'operator-complete');
  assert.deepEqual(messages,[{target:'sheet',ordinal:1,token}]);
  for(const value of [receipt,...f.receipts]){
    const serialized=JSON.stringify(value);assert.equal(serialized.includes(token),false);assert.equal(serialized.includes('private'),false);
  }
});
test('missing token feedback channel refuses instead of continuing with undisclosed routing state',async()=>{
  const f=fixture();delete f.services.feedback;f.services.next=async()=>({type:'pin-sheet'});
  f.services.window=async(mode,pid)=>({pid,windowId:45,matchedAX:true,...(mode==='pin-sheet'?{target:'sheet',sheetToken:'da5f47f9-5681-4e71-a0b6-aa7578b6b5fa'}:{})});
  const r=await runOperator(f.cfg,f.services);assert.equal(r.status,'partial');assert.equal(r.reason,'sheet-feedback-required');assert.equal(f.attempts,1);assert.equal(f.cleaned,1);
});
test('foreign/stale/absent sheet routes and undismissed main/stop refuse before interaction',()=>{
  const {actionTarget,sheetResult}=require('./desktop/p38-installed-driver.cjs'),token='da5f47f9-5681-4e71-a0b6-aa7578b6b5fa';
  const sheet={token};
  for(const a of [{type:'press',target:'sheet',sheetToken:'other'},{type:'set',target:'sheet'},
    {type:'press',target:'main'},{type:'select',target:'foreign'}, {type:'sheet-dismissed',sheetToken:'other'}])assert.throws(()=>actionTarget(a,sheet));
  for(const r of [{target:'sheet',sheetToken:'bad'},{target:'main',sheetToken:token},
    {target:'sheet',sheetToken:'3d755db1-cdf7-44f8-8706-cc812e1845ca'}])assert.throws(()=>sheetResult({type:'pin-sheet'},{...sheet},r));
  assert.throws(()=>sheetResult({type:'sheet-dismissed'},{...sheet},{dismissed:false}));
  sheetResult({type:'sheet-dismissed'},sheet,{dismissed:true});assert.equal(sheet.token,null);
  assert.throws(()=>actionTarget({type:'inspect',target:'sheet',sheetToken:token},sheet));
});
for(const reason of ['sheet-owner','sheet-parent','sheet-replaced','standalone-window','child-owner','confirm-unsupported','children-unavailable']){
  test('bounded '+reason+' helper refusal stays partial and always cleans without retry',async()=>{
    const f=fixture({window(mode,pid){return mode==='discover'?{pid,windowId:45,matchedAX:true}:{status:'refused',code:reason,pid,windowId:45,matchedAX:true};}});
    const r=await runOperator(f.cfg,f.services);assert.equal(r.status,'partial');assert.equal(r.reason,reason);assert.equal(f.attempts,1);assert.equal(f.cleaned,1);
    assert.equal(r.firstFailure.code,reason);assert.equal(r.firstFailure.stage,'window-inspect-main');
  });
}
test('unrecognized refusal and cleanup messages cannot enter the first-failure receipt',async()=>{
  const f=fixture({close:true,cleanup:true,window(mode,pid){return mode==='discover'?{pid,windowId:45,matchedAX:true}:
    {status:'refused',code:'/private/secret',pid,windowId:45,matchedAX:true};}});
  const r=await runOperator(f.cfg,f.services);
  assert.equal(r.firstFailure.code,'adapter-refused');assert.equal(r.firstFailure.stage,'window-inspect-main');
  assert.deepEqual(r.firstFailure.action,{type:'inspect',ordinal:1,durationMs:r.firstFailure.action.durationMs});
  assert.equal(r.reason,'adapter-refused');assert.equal(r.helperCleanup,'incomplete');assert.equal(r.cleanup,'incomplete');
  assert.equal(f.attempts,1);assert.equal(f.cleaned,1);
  for(const value of [r,...f.receipts])assert.equal(JSON.stringify(value).includes('/private/'),false);
});
test('blocked local child metadata persists without foreign values or arbitrary reasons',()=>{
  const {safeWindowEvidence}=require('./desktop/p38-installed-driver.cjs');
  const r=safeWindowEvidence({pid:123,windowId:45,matchedAX:true,nodes:[{path:'0',role:'other',blocked:'child-owner',value:'private',title:'secret'},
    {path:'1',role:'AXGroup',blocked:'children-unavailable',press:false,confirm:false},{path:'2',role:'other',blocked:'private'}]});
  assert.equal(r.nodes[0].blocked,'child-owner');assert.equal(r.nodes[1].blocked,'children-unavailable');assert.equal(r.nodes[2].blocked,undefined);
  assert.equal(JSON.stringify(r).includes('private'),false);assert.equal(JSON.stringify(r).includes('secret'),false);
});
test('bounded sheet projection facts retain selection/enabled state and reject unsafe budgets',async()=>{
  const {safeWindowEvidence}=require('./desktop/p38-installed-driver.cjs');
  const result=safeWindowEvidence({pid:123,windowId:45,matchedAX:true,truncated:true,nodes:[
    {path:'0',role:'AXRow',selected:true,enabled:true,value:'/private/secret'}]});
  assert.equal(result.truncated,true);assert.deepEqual(result.nodes[0],{path:'0',role:'AXRow',selected:true,enabled:true});
  const token='da5f47f9-5681-4e71-a0b6-aa7578b6b5fa',f=fixture();let step=0;
  f.services.next=async()=>step++===0?{type:'pin-sheet'}:{type:'inspect',target:'sheet',sheetToken:token,depth:6,maxNodes:129};
  f.services.window=async(mode,pid)=>({pid,windowId:45,matchedAX:true,...(mode==='pin-sheet'?{target:'sheet',sheetToken:token}:{})});
  const r=await runOperator(f.cfg,f.services);assert.equal(r.reason,'inspect-budget-refused');assert.equal(r.cleanup,'complete');
});
test('bounded main inspect accepts valid budgets while refusing malformed or unsafe budgets before per-action window access',async()=>{
  const valid=fixture();let step=0;
  valid.services.next=async()=>step++===0?{type:'inspect',target:'main',depth:3,maxNodes:32}:{type:'stop'};
  const calls=[];valid.services.window=async(mode,pid,wid,action)=>{
    calls.push({mode,action});return {pid,windowId:45,matchedAX:true,...mode==='capture'?{captureSha256:HASH}:{}};
  };
  const accepted=await runOperator(valid.cfg,valid.services);
  assert.equal(accepted.status,'operator-complete');assert.equal(accepted.actions.length,1);
  assert.deepEqual(calls.filter(call=>call.mode==='inspect').map(call=>call.action),[{type:'inspect',target:'main',depth:3,maxNodes:32}]);

  const defaultMain=fixture();let defaultStep=0;
  defaultMain.services.next=async()=>defaultStep++===0?{type:'inspect',depth:2,maxNodes:16}:{type:'stop'};
  const defaultCalls=[];defaultMain.services.window=async(mode,pid,wid,action)=>{
    if(mode==='inspect')defaultCalls.push(action);
    return {pid,windowId:45,matchedAX:true};
  };
  const defaultAccepted=await runOperator(defaultMain.cfg,defaultMain.services);
  assert.equal(defaultAccepted.status,'operator-complete');
  assert.deepEqual(defaultCalls,[{type:'inspect',depth:2,maxNodes:16}]);

  const token='da5f47f9-5681-4e71-a0b6-aa7578b6b5fa',sheet=fixture();let sheetStep=0;
  sheet.services.next=async()=>[
    {type:'pin-sheet'},
    {type:'inspect',target:'sheet',sheetToken:token,depth:2,maxNodes:24},
    {type:'sheet-dismissed',sheetToken:token},
    {type:'stop'},
  ][sheetStep++];
  const sheetInspects=[];sheet.services.window=async(mode,pid,wid,action)=>{
    if(mode==='inspect')sheetInspects.push(action);
    return {pid,windowId:45,matchedAX:true,
      ...(mode==='pin-sheet'||action?.target==='sheet'?{target:'sheet',sheetToken:token}:{}),
      ...(mode==='sheet-dismissed'?{dismissed:true}:{})};
  };
  const sheetResult=await runOperator(sheet.cfg,sheet.services);
  assert.equal(sheetResult.status,'operator-complete');
  assert.deepEqual(sheetInspects,[{type:'inspect',target:'sheet',sheetToken:token,depth:2,maxNodes:24}]);

  for(const action of [
    {type:'inspect',target:'main',depth:0,maxNodes:32},
    {type:'inspect',target:'main',depth:6,maxNodes:32},
    {type:'inspect',target:'main',depth:3,maxNodes:0},
    {type:'inspect',target:'main',depth:3,maxNodes:129},
    {type:'inspect',target:'main',depth:3},
  ]){
    const f=fixture();f.services.next=async()=>action;const perActionCalls=[];
    f.services.window=async(mode,pid,wid,a)=>{perActionCalls.push(mode);return {pid,windowId:45,matchedAX:true};};
    const refused=await runOperator(f.cfg,f.services);
    assert.equal(refused.reason,'inspect-budget-refused',JSON.stringify(action));
    assert.deepEqual(perActionCalls,['discover'],JSON.stringify(action));
    assert.equal(refused.cleanup,'complete');
  }
});
test('selection capability diagnostics are allowlisted, bounded, and survive cleanup failures',async()=>{
  const {safeSelectionDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  const diagnostics={stage:'row-capability',rowQueryError:0,rowSettableQueryError:-25205,rowSettable:false,
    outlineFound:true,outlineSelectedRowsQueryError:-25205,outlineSelectedRowsSettableQueryError:0,outlineSelectedRowsSettable:true,
    outlineSelectedRowsReadable:true,outlineSelectedRowMember:false,outlineSelectedRowsOwned:true,
    outlineRowsQueryError:0,outlineRowsReadable:true,outlineRowMember:true,outlineRowsOwned:true,privatePath:'/private/secret'};
  const clean=safeSelectionDiagnostics(diagnostics);
  assert.equal(clean.outlineSelectedRowsSettable,true);assert.equal(clean.rowSettable,false);
  assert.equal(clean.outlineSelectedRowMember,false);assert.equal(clean.outlineRowMember,true);
  const capabilityOnly=safeSelectionDiagnostics({stage:'row-capability',rowQueryError:0,rowSettableQueryError:-25205,rowSettable:false});
  assert.equal(capabilityOnly.rowSetError,undefined);assert.equal(capabilityOnly.rowReadbackError,undefined);
  assert.equal(JSON.stringify(clean).includes('private'),false);
  assert.throws(()=>safeSelectionDiagnostics({...diagnostics,stage:'/private'}));
  assert.throws(()=>safeSelectionDiagnostics({...diagnostics,rowQueryError:999999}));
  const f=fixture({close:true,cleanup:true});let step=0;
  f.services.next=async()=>step++===0?{type:'pin-sheet'}:{type:'select',target:'sheet',sheetToken:'da5f47f9-5681-4e71-a0b6-aa7578b6b5fa',element:'0.2'};
  f.services.window=async(mode,pid)=>({pid,windowId:45,matchedAX:true,
    ...(mode==='pin-sheet'?{target:'sheet',sheetToken:'da5f47f9-5681-4e71-a0b6-aa7578b6b5fa'}:{}),
    ...(mode==='select'?{status:'refused',code:'adapter-refused',selectionDiagnostics:diagnostics}:{})});
  const result=await runOperator(f.cfg,f.services);
  assert.equal(result.firstFailure.code,'adapter-refused');assert.deepEqual(result.firstFailure.selectionDiagnostics,clean);
  assert.equal(result.helperCleanup,'incomplete');assert.equal(result.cleanup,'incomplete');
});
test('main press diagnostics distinguish route, AXPress support and perform outcomes without leaking strings',async()=>{
  const {safeActionDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  const diagnostic={stage:'axpress-perform',elementPath:'2',role:'AXButton',control:'Open Project…',
    axPressQueryError:0,axPressAdvertised:true,performActionError:-25200,privatePath:'/private/secret'};
  const clean=safeActionDiagnostics(diagnostic);
  assert.deepEqual(clean,{stage:'axpress-perform',elementPath:'2',role:'AXButton',control:'Open Project…',
    axPressQueryError:0,axPressAdvertised:true,performActionError:-25200});
  assert.throws(()=>safeActionDiagnostics({...diagnostic,stage:'/private'}));
  assert.throws(()=>safeActionDiagnostics({...diagnostic,elementPath:'../secret'}));
  assert.throws(()=>safeActionDiagnostics({...diagnostic,performActionError:999999}));
  const f=fixture();f.services.next=async()=>({type:'press',element:'2'});
  f.services.window=async(mode,pid)=>mode==='discover'?{pid,windowId:45,matchedAX:true}:
    {status:'refused',code:'adapter-refused',pid,windowId:45,matchedAX:true,actionDiagnostics:diagnostic};
  f.services.close=async()=>{throw Object.assign(Error('/private/group'),{cleanupDiagnostics:[{stage:'group-remains',helperPid:321,capturePossible:true,
    groupAliveBefore:true,groupAliveAfterTerm:true,groupAliveAfterKill:true,signalAttempts:2}]});};
  const result=await runOperator(f.cfg,f.services);
  assert.equal(result.firstFailure.code,'adapter-refused');assert.equal(result.firstFailure.stage,'window-press-main');
  assert.deepEqual(result.firstFailure.actionDiagnostics,clean);
  assert.deepEqual(result.helperCleanupDiagnostics,[{stage:'group-remains',helperPid:321,capturePossible:true,
    groupAliveBefore:true,groupAliveAfterTerm:true,groupAliveAfterKill:true,signalAttempts:2}]);
  assert.equal(result.helperCleanup,'incomplete');assert.equal(result.cleanup,'complete');
  for(const receipt of f.receipts)assert.equal(JSON.stringify(receipt).includes('/private/'),false);
});
test('post-press identity receipts retain bounded per-predicate facts and reject arbitrary evidence',()=>{
  const {safeActionDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  const facts={stage:'post-action-application-window-membership',elementPath:'2',axPidQueryError:0,axPidMatches:true,
    applicationWindowsQueryError:-25205,applicationWindowsReadable:false,pinnedAXWindowMember:false,
    minimizedQueryError:0,minimizedReadable:false,minimized:false,pinnedCGWindowCount:1,privatePath:'/private/identity'};
  const clean=safeActionDiagnostics(facts);
  assert.deepEqual(clean,{stage:facts.stage,elementPath:'2',axPidQueryError:0,axPidMatches:true,
    applicationWindowsQueryError:-25205,applicationWindowsReadable:false,pinnedAXWindowMember:false,
    minimizedQueryError:0,minimizedReadable:false,minimized:false,pinnedCGWindowCount:1});
  assert.throws(()=>safeActionDiagnostics({...facts,stage:'post-action-identity/secret'}));
  assert.throws(()=>safeActionDiagnostics({...facts,pinnedCGWindowCount:999}));
  assert.throws(()=>safeActionDiagnostics({...facts,applicationWindowsReadable:'/private'}));
  const unavailable=safeActionDiagnostics({stage:'post-action-pinned-cg-window-list',elementPath:'2',pinnedCGWindowListReadable:false});
  assert.deepEqual(unavailable,{stage:'post-action-pinned-cg-window-list',elementPath:'2',pinnedCGWindowListReadable:false});
  assert.equal(unavailable.pinnedCGWindowCount,undefined);
});
test('retained-main sheet probe is bounded, identity-gated, sanitized and never resumes a refused press',async()=>{
  const {safeActionDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  const ownedSheet={index:0,candidatePidQueryError:0,candidatePid:123,candidatePidMatches:true,candidateRoleQueryError:0,
    candidateRoleReadable:true,candidateRole:'AXSheet',candidateParentQueryError:0,candidateParentReadable:true,
    candidateParentPidQueryError:0,candidateParentPidMatches:true,candidateParentIsRetainedMain:true,privatePath:'/private/secret'};
  const scenarios=[
    {name:'query error',queryError:-25205,readable:false,count:0,facts:[],probe:'children-query-error'},
    {name:'foreign child',queryError:0,readable:true,count:1,facts:[{index:0,candidatePidQueryError:0,candidatePid:124,candidatePidMatches:false}],probe:'no-attached-sheet'},
    {name:'multiple sheets',queryError:0,readable:true,count:2,facts:[ownedSheet,{...ownedSheet,index:1}],probe:'multiple-attached-sheets'},
    {name:'parent mismatch',queryError:0,readable:true,count:1,facts:[{...ownedSheet,candidateParentIsRetainedMain:false}],probe:'no-attached-sheet'},
    {name:'exact attached sheet',queryError:0,readable:true,count:1,facts:[ownedSheet],probe:'one-attached-sheet'},
  ];
  for(const scenario of scenarios){
    const diagnostic={stage:'post-action-application-window-membership',elementPath:'2',
      axPidQueryError:0,axPidMatches:true,applicationWindowsQueryError:0,applicationWindowsReadable:true,pinnedAXWindowMember:false,
      minimizedQueryError:0,minimizedReadable:true,minimized:false,pinnedCGWindowListReadable:true,pinnedCGWindowCount:1,
      retainedMainChildrenQueryError:scenario.queryError,retainedMainChildrenReadable:scenario.readable,
      ...(scenario.readable?{retainedMainChildrenCount:scenario.count,retainedMainChildrenCountCapped:false,retainedMainChildrenTruncated:false}:{}),
      retainedMainChildFacts:scenario.facts,retainedMainSheetProbe:scenario.probe,privatePath:'/private/identity'};
    const clean=safeActionDiagnostics(diagnostic);
    assert.equal(clean.retainedMainSheetProbe,scenario.probe,scenario.name);
    if(scenario.probe==='one-attached-sheet')assert.equal(clean.retainedMainChildFacts[0].candidateParentIsRetainedMain,true);
    assert.equal(JSON.stringify(clean).includes('/private/'),false,scenario.name);
    const f=fixture();let requests=0;f.services.next=async()=>{requests++;return{type:'press',element:'2'};};
    f.services.window=async(mode,pid)=>mode==='discover'?{pid,windowId:45,matchedAX:true}:
      {status:'refused',code:'adapter-refused',pid,windowId:45,matchedAX:true,actionDiagnostics:diagnostic};
    const result=await runOperator(f.cfg,f.services);
    assert.equal(result.firstFailure.code,'adapter-refused',scenario.name);
    assert.equal(result.firstFailure.stage,'window-press-main',scenario.name);
    assert.equal(result.firstFailure.actionDiagnostics.retainedMainSheetProbe,scenario.probe,scenario.name);
    assert.equal(result.installedAcceptance,'not-established',scenario.name);
    assert.equal(requests,1,scenario.name);
  }
  assert.throws(()=>safeActionDiagnostics({stage:'post-action-application-window-membership',retainedMainSheetProbe:'AXSheet'}));
  assert.throws(()=>safeActionDiagnostics({stage:'post-action-application-window-membership',retainedMainChildFacts:Array(65).fill({index:0})}));
  assert.throws(()=>safeActionDiagnostics({stage:'post-action-application-window-membership',retainedMainChildFacts:[{index:0,candidateRole:'/private'}]}));
  assert.throws(()=>safeActionDiagnostics({stage:'post-action-application-window-membership',retainedMainChildFacts:[{index:0,candidatePid:999999999999}]}));
});
test('bounded post-press transition samples are sanitized and never resume a refused press',async()=>{
  const {safeActionDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  const helperSource=fs.readFileSync('tests/desktop/p38-owned-window.swift','utf8');
  const samplerStart=helperSource.indexOf('  func transitionObservationDiagnostic()');
  const samplerEnd=helperSource.indexOf('  func verify()',samplerStart);
  const sampler=helperSource.slice(samplerStart,samplerEnd);
  assert.ok(samplerStart>=0&&samplerEnd>samplerStart);
  assert.match(sampler,/let deadline = started \+ 750_000_000/);
  assert.match(sampler,/for index in 0\.\.\<2/);
  assert.match(sampler,/AXUIElementSetMessagingTimeout/);
  assert.match(sampler,/kAXChildrenAttribute/);
  assert.match(sampler,/budgetKind":"best-effort/);
  assert.match(sampler,/budgetScope":"transition-samples-only/);
  assert.match(sampler,/facts\["sampleDeadlineReached"\] = true/);
  assert.match(sampler,/transitionObservationStatus\(samples,overrun\)/);
  assert.match(sampler,/let completed = DispatchTime\.now\(\)\.uptimeNanoseconds/);
  assert.match(sampler,/let overrun = completed >= deadline/);
  assert.doesNotMatch(sampler,/AXUIElementPerformAction|AXUIElementPostKeyboardEvent|CGEvent/);
  const refusalPath=helperSource.slice(helperSource.indexOf('if shouldProbeRetainedMainChildren(error.stage,error.facts)'),
    helperSource.indexOf('throw pressFailure("post-action-"',helperSource.indexOf('if shouldProbeRetainedMainChildren(error.stage,error.facts)')));
  assert.ok(refusalPath.indexOf('transitionObservationDiagnostic()')<refusalPath.indexOf('retainedMainChildrenDiagnostic()'));
  assert.match(refusalPath,/transitionObservationDiagnostic\(\)/);
  assert.match(refusalPath,/retainedMainChildrenDiagnostic\(\)/);
  assert.match(refusalPath,/configured && status != "deadline" && status != "timeout-config-refused"/);
  assert.match(sampler,/AXUIElementSetMessagingTimeout\(application,timeout\)/);
  assert.match(sampler,/AXUIElementSetMessagingTimeout\(main,timeout\)/);
  const stable=(index,extra={})=>({sampleIndex:index,elapsedMs:index?330:0,axPidQueryError:0,axPidMatches:true,
    applicationWindowsQueryError:0,applicationWindowsReadable:true,pinnedAXWindowMember:false,
    minimizedQueryError:0,minimizedReadable:true,minimized:false,pinnedCGWindowListReadable:true,pinnedCGWindowCount:1,
    retainedMainChildrenQueryError:0,retainedMainChildrenReadable:true,retainedMainChildrenCount:1,
    retainedMainChildrenCountCapped:false,...extra});
  const scenarios=[
    {name:'membership returns',status:'membership-returned',samples:[stable(0),stable(1,{pinnedAXWindowMember:true})]},
    {name:'direct child count changes',status:'persistent-omission',samples:[stable(0),stable(1,{retainedMainChildrenCount:2})]},
    {name:'persistent omission',status:'persistent-omission',samples:[stable(0),stable(1)]},
    {name:'identity drift',status:'inconclusive',samples:[stable(0),stable(1,{axPidMatches:false})]},
    {name:'AX query error',status:'inconclusive',samples:[stable(0),stable(1,{applicationWindowsQueryError:-25205,applicationWindowsReadable:false})]},
    {name:'direct child query error',status:'inconclusive',samples:[stable(0),stable(1,{retainedMainChildrenQueryError:-25205,retainedMainChildrenReadable:false})]},
  ];
  for(const scenario of scenarios){
    const diagnostic={stage:'post-action-application-window-membership',elementPath:'2',transitionObservation:{
      status:scenario.status,budgetMs:750,budgetKind:'best-effort',budgetScope:'transition-samples-only',
      axTimeoutsConfigured:true,elapsedMs:480,samples:scenario.samples},privatePath:'/private/identity'};
    const clean=safeActionDiagnostics(diagnostic);
    assert.equal(clean.transitionObservation.status,scenario.status,scenario.name);
    assert.equal(clean.transitionObservation.samples.length,2,scenario.name);
    assert.equal(JSON.stringify(clean).includes('/private/'),false,scenario.name);
    const f=fixture();let requests=0,presses=0;f.services.next=async()=>{requests++;return{type:'press',element:'2'};};
    f.services.window=async(mode,pid,wid,action)=>{
      f.events.push('window-'+mode);
      if(mode==='discover')return{pid,windowId:45,matchedAX:true};
      if(mode==='press'){presses++;return{status:'refused',code:'adapter-refused',pid,windowId:45,matchedAX:true,actionDiagnostics:diagnostic};}
      return{status:'ok',pid,windowId:45,matchedAX:true};
    };
    const result=await runOperator(f.cfg,f.services);
    assert.equal(result.status,'partial',scenario.name);assert.equal(result.reason,'adapter-refused',scenario.name);
    assert.equal(result.firstFailure.actionDiagnostics.transitionObservation.status,scenario.status,scenario.name);
    assert.equal(result.installedAcceptance,'not-established',scenario.name);
    assert.equal(requests,1,scenario.name);assert.equal(presses,1,scenario.name);
    assert.equal(f.events.filter(event=>event==='window-press').length,1,scenario.name);
  }
  const timeout=safeActionDiagnostics({stage:'post-action-application-window-membership',transitionObservation:{
    status:'timeout-config-refused',budgetMs:750,budgetKind:'best-effort',budgetScope:'transition-samples-only',
    axTimeoutsConfigured:false,elapsedMs:8,samples:[]}});
  assert.deepEqual(timeout.transitionObservation,{status:'timeout-config-refused',budgetMs:750,budgetKind:'best-effort',
    budgetScope:'transition-samples-only',axTimeoutsConfigured:false,elapsedMs:8,samples:[]});
  const deadline=safeActionDiagnostics({stage:'post-action-application-window-membership',transitionObservation:{
    status:'deadline',budgetMs:750,budgetKind:'best-effort',budgetScope:'transition-samples-only',
    axTimeoutsConfigured:true,elapsedMs:740,samples:[stable(0)]}});
  assert.equal(deadline.transitionObservation.status,'deadline');
  const overrunDeadline=safeActionDiagnostics({stage:'post-action-application-window-membership',transitionObservation:{
    status:'deadline',budgetMs:750,budgetKind:'best-effort',budgetScope:'transition-samples-only',
    axTimeoutsConfigured:true,elapsedMs:810,
    samples:[stable(0),stable(1,{pinnedAXWindowMember:true,sampleDeadlineReached:true})]}});
  assert.equal(overrunDeadline.transitionObservation.elapsedMs,810);
  assert.equal(overrunDeadline.transitionObservation.status,'deadline');
  for(const observation of [
    {status:'persistent-omission',budgetMs:750,budgetKind:'best-effort',budgetScope:'transition-samples-only',axTimeoutsConfigured:true,elapsedMs:751,samples:[stable(0),stable(1)]},
    {status:'persistent-omission',budgetMs:750,budgetKind:'best-effort',budgetScope:'transition-samples-only',axTimeoutsConfigured:true,elapsedMs:500,samples:[stable(0),stable(1),stable(2)]},
    {status:'attached-sheet-observed',budgetMs:750,budgetKind:'best-effort',budgetScope:'transition-samples-only',axTimeoutsConfigured:true,elapsedMs:500,samples:[stable(0),stable(1)]},
    {status:'timeout-config-refused',budgetMs:750,budgetKind:'best-effort',budgetScope:'transition-samples-only',axTimeoutsConfigured:false,elapsedMs:0,samples:[stable(0)]},
    {status:'deadline',budgetMs:750,budgetKind:'best-effort',budgetScope:'transition-samples-only',axTimeoutsConfigured:true,elapsedMs:10001,samples:[stable(0),stable(1)]},
    {status:'persistent-omission',budgetMs:750,budgetKind:'best-effort',budgetScope:'transition-samples-only',axTimeoutsConfigured:true,elapsedMs:500,samples:[stable(0),stable(1,{pinnedCGWindowCount:0})]},
    {status:'membership-returned',budgetMs:750,budgetKind:'best-effort',budgetScope:'transition-samples-only',axTimeoutsConfigured:true,elapsedMs:500,
      samples:[stable(0),stable(1,{pinnedAXWindowMember:true,sampleDeadlineReached:true})]},
    {status:'persistent-omission',budgetMs:750,budgetKind:'best-effort',budgetScope:'transition-samples-only',axTimeoutsConfigured:false,
      elapsedMs:500,samples:[stable(0),stable(1)]},
  ])assert.throws(()=>safeActionDiagnostics({stage:'post-action-application-window-membership',transitionObservation:observation}));
  assert.throws(()=>safeActionDiagnostics({stage:'post-action-application-window-membership',transitionObservation:{
    status:'persistent-omission',budgetMs:750,budgetKind:'best-effort',budgetScope:'transition-samples-only',
    axTimeoutsConfigured:true,elapsedMs:750,
    samples:[stable(0),stable(1,{pinnedAXWindowMember:true})]}}));
  assert.throws(()=>safeActionDiagnostics({stage:'axpress-perform',transitionObservation:{
    status:'deadline',budgetMs:750,budgetKind:'best-effort',budgetScope:'transition-samples-only',
    axTimeoutsConfigured:false,elapsedMs:0,samples:[]}}));
});
test('helper liveness, signal and wait exceptions produce sanitized incomplete cleanup facts',async()=>{
  const {cleanupHelper}=require('./desktop/p38-installed-driver.cjs');
  const cases=[
    {stage:'group-probe-error',services:{alive(){throw Error('/private/probe');}}},
    {stage:'group-signal-error',services:{alive:()=>true,signal(){throw Error('/private/signal');}}},
    {stage:'group-wait-error',services:{alive:()=>true,signal:()=>{},delay(){throw Error('/private/delay');}}},
  ];
  for(const item of cases){
    await assert.rejects(cleanupHelper({pid:321},item.services),error=>{
      assert.equal(error.cleanupDiagnostics[0].stage,item.stage);
      const {safeCleanupDiagnostics}=require('./desktop/p38-installed-driver.cjs');
      const sanitized=safeCleanupDiagnostics(error.cleanupDiagnostics);
      assert.equal(JSON.stringify(sanitized).includes('/private/'),false);
      assert.notEqual(sanitized.length,0);
      return true;
    });
  }
});
test('non-capture helper cleanup records exact-handle signal and wait exceptions',async()=>{
  const {cleanupHelper,safeCleanupDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  const child={pid:321,p38CapturePossible:false,exitCode:null,signalCode:null,kill(){throw Error('/private/exact-signal');}};
  await assert.rejects(cleanupHelper(child),error=>{
    assert.deepEqual(safeCleanupDiagnostics(error.cleanupDiagnostics),[{stage:'exact-helper-signal-error',helperPid:321,capturePossible:false,
      signalKind:'SIGTERM',signalAttempts:1,signalError:true}]);return true;
  });
  const waiting={pid:321,p38CapturePossible:false,exitCode:null,signalCode:null,kill:()=>{}};
  await assert.rejects(cleanupHelper(waiting,{delay(){throw Error('/private/exact-wait');}}),error=>{
    assert.deepEqual(safeCleanupDiagnostics(error.cleanupDiagnostics),[{stage:'exact-helper-wait-error',helperPid:321,capturePossible:false,
      signalKind:'SIGTERM',signalAttempts:1,probePoint:'wait-term'}]);return true;
  });
});
test('cleanup loop preserves detailed exception facts or supplies a bounded fallback stage',()=>{
  const {cleanupFailureDiagnostics,safeCleanupDiagnostics,closeOwnedHelpers}=require('./desktop/p38-installed-driver.cjs');
  const prior=[{stage:'group-absent',helperPid:123,capturePossible:true,groupAliveBefore:false,signalAttempts:0}];
  const fallback=cleanupFailureDiagnostics(Error('/private/close'),prior,99);
  assert.deepEqual(safeCleanupDiagnostics(fallback),[...prior,{stage:'cleanup-loop-error',helpersRemaining:8}]);
  const reported=[{stage:'group-probe-error',helperPid:123,capturePossible:true,probePoint:'initial',signalAttempts:0,probeError:true}];
  assert.deepEqual(cleanupFailureDiagnostics(Error('ignored'),prior,1),[...prior,{stage:'cleanup-loop-error',helpersRemaining:1}]);
  assert.deepEqual(cleanupFailureDiagnostics(Object.assign(Error('private'),{cleanupDiagnostics:reported}),prior,1),[...prior,...reported]);
});
test('input-close failure still attempts helper cleanup and retains both observations',async()=>{
  const {closeOwnedHelpers,safeCleanupDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  let cleaned=0;
  await assert.rejects(closeOwnedHelpers(()=>{throw Error('/private/stdin');},[{pid:12},{pid:13}],async child=>{
    cleaned++; if(child.pid===12)throw Object.assign(Error('/private/helper'),{cleanupDiagnostics:[{stage:'group-signal-error',helperPid:12,capturePossible:true,
      groupAliveBefore:true,signalAttempts:1,signalKind:'SIGTERM',signalError:true}]});
    return [{stage:'group-absent',helperPid:13,capturePossible:true,groupAliveBefore:false,signalAttempts:0}];
  }),error=>{
    assert.equal(cleaned,2);
    assert.deepEqual(safeCleanupDiagnostics(error.cleanupDiagnostics),[
      {stage:'group-absent',helperPid:13,capturePossible:true,groupAliveBefore:false,signalAttempts:0},
      {stage:'input-close-error',helpersRemaining:2},
      {stage:'group-signal-error',helperPid:12,capturePossible:true,groupAliveBefore:true,signalAttempts:1,signalKind:'SIGTERM',signalError:true},
    ]);
    assert.equal(JSON.stringify(error.cleanupDiagnostics).includes('/private/'),false);return true;
  });
});
test('cleanup exception facts do not replace the saved first refusal',async()=>{
  const {cleanupHelper,safeActionDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  const diagnostic={stage:'post-action-pinned-cg-window',elementPath:'2',axPidQueryError:0,axPidMatches:true,
    applicationWindowsQueryError:0,applicationWindowsReadable:true,pinnedAXWindowMember:true,
    minimizedQueryError:0,minimizedReadable:true,minimized:false,pinnedCGWindowCount:0};
  const f=fixture();f.services.next=async()=>({type:'press',element:'2'});
  f.services.window=async(mode,pid)=>mode==='discover'?{pid,windowId:45,matchedAX:true}:
    {status:'refused',code:'adapter-refused',pid,windowId:45,matchedAX:true,actionDiagnostics:diagnostic};
  f.services.close=async()=>cleanupHelper({pid:321},{alive(){throw Error('/private/probe');}});
  const result=await runOperator(f.cfg,f.services);
  assert.equal(result.firstFailure.code,'adapter-refused');
  assert.deepEqual(result.firstFailure.actionDiagnostics,safeActionDiagnostics(diagnostic));
  assert.equal(result.helperCleanup,'incomplete');
  assert.deepEqual(result.helperCleanupDiagnostics,[{stage:'group-probe-error',helperPid:321,capturePossible:true,
    probePoint:'initial',signalAttempts:0,probeError:true}]);
  assert.equal(result.cleanup,'complete');assert.equal(JSON.stringify(result).includes('/private/'),false);
});
test('modeled helper transport preserves sanitized press diagnostics in the first-failure receipt',async()=>{
  const crypto=require('node:crypto'),os=require('node:os'),path=require('node:path');
  const {createWindowTransport,cleanupHelper,safeDirectory}=require('./desktop/p38-installed-driver.cjs');
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'p38-v2-press-diagnostics-')),helpers=new Set();
  try{
    const reports=safeDirectory(dir,['reports']),fixtureRoot=path.join(dir,'fixtures');fs.mkdirSync(fixtureRoot);
    const fixtures=['A','B'].map(label=>{const file=path.join(fixtureRoot,label+'.json');fs.writeFileSync(file,JSON.stringify({label}));
      return{label,path:file,sha256:crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex')};});
    const helper=path.join(dir,'modeled-helper'),diagnostics={stage:'axpress-perform',elementPath:'2',role:'AXButton',
      axPressQueryError:0,axPressAdvertised:true,performActionError:-25200,privatePath:'/private/secret'};
    fs.writeFileSync(helper,'#!/usr/bin/env node\nconsole.log(JSON.stringify({status:"pinned",pid:123,windowId:45,matchedAX:true}));\n'+
      'require("node:readline").createInterface({input:process.stdin}).on("line",line=>{const r=JSON.parse(line);const out=r.type==="check"?{status:"ok",type:r.type,pid:r.pid,windowId:r.windowId,matchedAX:true}:'+
      '{status:"refused",code:"adapter-refused",pid:r.pid,windowId:r.windowId,matchedAX:true,actionDiagnostics:'+JSON.stringify(diagnostics)+'};console.log(JSON.stringify(out));});\n',{mode:0o700});
    const cfg={windowHelper:helper,grant:{id:'modeled-diagnostics'},fixtures},window=createWindowTransport(cfg,reports.directory,()=>reports,helpers);
    const f=fixture();f.cfg.fixtures=fixtures;f.services.window=window;f.services.next=async()=>({type:'press',element:'2'});
    f.services.close=async()=>{for(const child of [...helpers])await cleanupHelper(child);};
    const receipt=await runOperator(f.cfg,f.services);
    assert.equal(receipt.firstFailure.code,'adapter-refused');
    assert.deepEqual(receipt.firstFailure.actionDiagnostics,{stage:'axpress-perform',elementPath:'2',role:'AXButton',
      axPressQueryError:0,axPressAdvertised:true,performActionError:-25200});
    assert.equal(receipt.startAttempts,1);assert.equal(receipt.cleanup,'complete');assert.equal(receipt.helperCleanup,'complete');
    assert.equal(JSON.stringify(receipt).includes('/private/'),false);
  }finally{for(const child of [...helpers])await cleanupHelper(child);fs.rmSync(dir,{recursive:true,force:true});}
});
test('main inspection refuses an unpinned attached sheet BEFORE any route/attribute traversal',()=>{
  const source=fs.readFileSync('tests/desktop/p38-owned-window.swift','utf8');
  const start=source.indexOf('    case "inspect":'),end=source.indexOf('    case "capture":',start),body=source.slice(start,end);
  const guard=body.indexOf('if target == "main" { try noSheet() }'),traversal=body.indexOf('try node(base,selectedRoot)');
  assert.ok(start>=0&&end>start&&guard>=0&&traversal>guard);
  const noSheet=source.slice(source.indexOf('  func noSheet()'),source.indexOf('  func verify()',source.indexOf('  func noSheet()')));
  assert.match(noSheet,/sheet == nil/);assert.match(noSheet,/members\(\)\)\.isEmpty/);
});
test('A/B fixture proof uses exact same-descriptor bytes and rejects swap/hash/name/symlink',()=>{
  const {verifyFixtures}=require('./desktop/p38-installed-driver.cjs'),crypto=require('node:crypto'),dir=fs.mkdtempSync(path.join(os.tmpdir(),'p38-v2-fixtures-'));
  try{
    const fixtures=['A','B'].map(label=>{const file=path.join(dir,label+'.json');fs.writeFileSync(file,JSON.stringify({label}));return{label,path:file,sha256:crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex')};});
    verifyFixtures({fixtures});assert.throws(()=>verifyFixtures({fixtures:[fixtures[0]]}));
    assert.throws(()=>verifyFixtures({fixtures:[{...fixtures[0],sha256:'0'.repeat(64)},fixtures[1]]}),e=>e.code==='fixture-refused');
    fs.writeFileSync(fixtures[1].path,'changed');assert.throws(()=>verifyFixtures({fixtures}),e=>e.code==='fixture-refused');
    fs.unlinkSync(fixtures[1].path);fs.symlinkSync(fixtures[0].path,fixtures[1].path);assert.throws(()=>verifyFixtures({fixtures}),e=>e.code==='fixture-refused');
  }finally{fs.rmSync(dir,{recursive:true,force:true});}
});
test('exact spawned-helper group cleanup escalates only when needed and refuses unreaped group',async()=>{
  const {cleanupHelper}=require('./desktop/p38-installed-driver.cjs');
  for(const endOn of ['already','SIGTERM','SIGKILL','never']){
    let live=endOn!=='already';const signals=[];
    const work=cleanupHelper({pid:123},{alive:()=>live,signal:kind=>{signals.push(kind);if(kind===endOn)live=false;},delay:async()=>{}});
    if(endOn==='never')await assert.rejects(work,e=>e.code==='helper-group-remains'&&e.cleanupDiagnostics[0].groupAliveAfterKill===true);
    else {const observations=await work;assert.equal(observations[0].helperPid,123);
      if(endOn==='already')assert.equal(observations[0].groupAliveBefore,false);
      else if(endOn==='SIGTERM')assert.equal(observations[0].groupAliveAfterTerm,false);
      else assert.equal(observations[0].groupAliveAfterKill,false);}
    assert.deepEqual(signals,endOn==='already'?[]:endOn==='SIGTERM'?['SIGTERM']:['SIGTERM','SIGKILL']);
  }
  await assert.rejects(cleanupHelper({pid:-1}),e=>e.code==='helper-cleanup-invalid');
});

for(const mode of ['positive','EOF','malformed','foreign','refused','oversized','timeout'])test('real Node-only modeled helper pipe '+mode+' is framed/tracked/cleaned',async()=>{
  // Disposable Node stub only. Never Nemo, macOS AX, a native window or installed evidence.
  const {createWindowTransport,cleanupHelper,safeDirectory}=require('./desktop/p38-installed-driver.cjs'),crypto=require('node:crypto');
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'p38-v2-node-')),helpers=new Set();
  try{
    const fixtures=['A','B'].map(label=>{const file=path.join(dir,label+'.json');fs.writeFileSync(file,'{}');return{label,path:file,sha256:crypto.createHash('sha256').update('{}').digest('hex')};});
    const script=path.join(dir,'modeled-helper');
    const before=mode==='EOF'?'process.exit(0);':mode==='timeout'?'':mode==='malformed'?"console.log('not-json');":
      mode==='oversized'?"console.log('x'.repeat(1048577));":mode==='refused'?"console.log(JSON.stringify({status:'refused',code:'confirm-unsupported'}));":
      "console.log(JSON.stringify({status:'pinned',pid:"+(mode==='foreign'?124:123)+",windowId:45,matchedAX:true}));";
    fs.writeFileSync(script,'#!/usr/bin/env node\n'+before+"\nrequire('node:readline').createInterface({input:process.stdin}).on('line',line=>{const r=JSON.parse(line);console.log(JSON.stringify({...r,status:'ok',matchedAX:true}));});\n",{mode:0o700});
    const guard=safeDirectory(dir,['reports']),window=createWindowTransport({windowHelper:script,grant:{id:'model-only'},fixtures},guard.directory,()=>guard,helpers,mode==='timeout'?20:10000);
    if(mode==='positive'){
      assert.equal((await window('discover',123)).windowId,45);
      assert.equal((await window('check',123,45)).type,'check');assert.equal(helpers.size,1);
      await assert.rejects(window('check',124,45),e=>e.code==='owned-window-refused');
    }else await assert.rejects(window('discover',123),e=>['helper-input-ended','helper-malformed','owned-window-refused','confirm-unsupported'].includes(e.code));
  }finally{
    for(const child of [...helpers])await cleanupHelper(child);
    fs.rmSync(dir,{recursive:true,force:true});
  }
});
