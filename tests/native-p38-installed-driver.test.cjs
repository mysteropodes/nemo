'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const { runOperator, verifyIdentity, verifyWindow, captureStartupDiagnostics, classifyStartupStderr, classifyManifestOutcome } = require('./desktop/p38-installed-driver.cjs');
const {HEAD,HASH,config,fixture}=require('./desktop/p38-operator-model.cjs');
test('P38 safe projection retains typed paging and semantic provenance while refusing malformed fields',()=>{
  const {safeWindowEvidence}=require('./desktop/p38-installed-driver.cjs');
  const raw={pid:42,windowId:9,matchedAX:true,truncated:true,depthClipped:true,childrenUnreadable:false,
    foreignChildren:false,complete:false,childrenComplete:false,childOffset:0,nextChildOffset:1,childCount:2,
    pageToken:'12345678-1234-1234-1234-123456789abc',projectTabs:{path:'1',token:'87654321-1234-1234-1234-123456789abc',unique:true,private:'secret'},
    nodes:[{path:'',role:'AXWindow',childCount:2},{path:'0',role:'other',control:'Project tabs',childCount:0,title:'secret'}]};
  const safe=safeWindowEvidence(raw);
  assert.equal(safe.depthClipped,true);assert.equal(safe.childrenComplete,false);assert.equal(safe.childCount,2);
  assert.equal(safe.nodes[1].control,'Project tabs');assert.equal(safe.nodes[1].childCount,0);
  assert.deepEqual(safe.projectTabs,{path:'1',token:raw.projectTabs.token,unique:true});
  assert.doesNotMatch(JSON.stringify(safe),/private|secret|title/);
  for(const change of [s=>s.pageToken='private',s=>s.nextChildOffset=0,s=>s.childOffset=true,
    s=>s.childrenComplete='true',s=>s.depthClipped=1,s=>s.nodes[1].childCount=-1,
    s=>s.projectTabs.token='private',s=>s.projectTabs.unique=1,s=>delete s.pageToken]){
    const bad=structuredClone(raw);change(bad);assert.throws(()=>safeWindowEvidence(bad),/ax-evidence-refused/);
  }
});
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
test('partial start failure retains only allowlisted startup diagnostics before owner cleanup',async()=>{
  const f=fixture({start(){throw Object.assign(Error('/private/partial-start'),{startupDiagnostics:{classification:'rust-panic',runtimeState:'failed',manifestOutcome:'manifest-not-written',signal:'SIGABRT',stderrBytes:18,stderrSha256:HASH,truncated:false,raw:'/private/secret private-owner-token'}});}});
  const r=await runOperator(f.cfg,f.services);
  assert.equal(f.attempts,1);assert.equal(f.cleaned,1);assert.equal(r.cleanup,'complete');assert.equal(r.status,'partial');assert.equal(r.reason,'operator-failed');
  assert.equal(r.firstFailure.stage,'launch');
  assert.deepEqual(r.firstFailure.startupDiagnostics,{classification:'rust-panic',runtimeState:'failed',manifestOutcome:'manifest-not-written',signal:'SIGABRT',stderrBytes:18,stderrSha256:HASH,truncated:false});
  const saved=f.receipts.find(receipt=>receipt.firstFailure);
  assert.ok(f.events.indexOf('save-failure') < f.events.indexOf('cleanup'));
  assert.equal(JSON.stringify(saved).includes('private-owner-token'),false);assert.equal(JSON.stringify(saved).includes('/private/'),false);
});
test('startup stderr capture is bounded, hashed, classified and redacts arbitrary text',async t=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'p38-startup-diagnostic-')),reports=path.join(root,'reports');
  fs.mkdirSync(reports);t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const raw=Buffer.concat([Buffer.alloc(20*1024,0x78),Buffer.from("thread 'main' panicked at /private/path secret-token")]);
  fs.writeFileSync(path.join(reports,'native-app.stderr.log'),raw,{mode:0o600});
  const taskRoots={root,reports};
  const result=await captureStartupDiagnostics('task',{taskRoots:()=>taskRoots},{readNativeStatus:()=>({state:'failed',signal:'SIGABRT',appRuntimeError:'app did not write its runtime manifest'})});
  assert.equal(result.classification,'rust-panic');assert.equal(result.runtimeState,'failed');assert.equal(result.signal,'SIGABRT');
  assert.equal(result.manifestOutcome,'manifest-not-written');
  assert.equal(result.stderrBytes,16*1024);assert.equal(result.truncated,true);
  assert.equal(result.stderrSha256,crypto.createHash('sha256').update(raw.subarray(raw.length-16*1024)).digest('hex'));
  assert.equal(JSON.stringify(result).includes('/private/path'),false);assert.equal(JSON.stringify(result).includes('secret-token'),false);
  assert.equal(classifyStartupStderr(Buffer.from('P38 fixture refused /private/secret')),'p38-startup-refusal');
});
test('manifest startup outcomes are classified without retaining runtime error text',async t=>{
  const cases=[
    [null,'status-unavailable'],
    [{state:'active'},'manifest-pending'],
    [{state:'active',appRuntimeError:'app did not write its runtime manifest'},'manifest-not-written'],
    [{state:'active',appRuntimeError:'app manifest is not a nemo.native-runtime/1 document'},'manifest-invalid-schema'],
    [{state:'active',appRuntimeError:'app manifest belongs to task /private/path-token'},'manifest-task-mismatch'],
    [{state:'active',appRuntimeError:'/private/unknown-token'},'manifest-error-other'],
    [{state:'active',appRuntime:{schema:'nemo.native-runtime/1'}},'manifest-present'],
  ];
  for(const [status,expected] of cases){
    const outcome=classifyManifestOutcome(status);assert.equal(outcome,expected);
    assert.equal(JSON.stringify({manifestOutcome:outcome}).includes('private'),false);
  }
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'p38-manifest-status-')),reports=path.join(root,'reports');
  fs.mkdirSync(reports);t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const started=Date.now();
  const result=await captureStartupDiagnostics('task',{taskRoots:()=>({root,reports})},
    {readNativeStatus:()=>({state:'active',appRuntimeError:'app manifest belongs to task /private/path-token'})});
  assert.ok(Date.now()-started<500,'known manifest result should be captured without waiting for launcher exit');
  assert.equal(result.runtimeState,'active');assert.equal(result.manifestOutcome,'manifest-task-mismatch');
  assert.equal(JSON.stringify(result).includes('/private/path-token'),false);
});
test('startup stderr capture refuses a FIFO without blocking',async t=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'p38-startup-fifo-')),reports=path.join(root,'reports');
  fs.mkdirSync(reports);t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  execFileSync('mkfifo',[path.join(reports,'native-app.stderr.log')]);
  const started=Date.now();
  const result=await captureStartupDiagnostics('task',{taskRoots:()=>({root,reports})},{readNativeStatus:()=>({state:'failed',signal:'SIGABRT'})});
  assert.ok(Date.now()-started<500,'FIFO refusal must not stall startup cleanup');
  assert.equal(result.classification,'stderr-unavailable');assert.equal(result.stderrBytes,0);
});
test('startup stderr capture refuses a symlinked report root',async t=>{
  if(process.platform==='win32')return t.skip('directory symlinks are not portable on Windows');
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'p38-startup-symlink-'));
  const taskRoot=path.join(root,'task'),target=path.join(root,'outside-reports'),reports=path.join(taskRoot,'reports');
  fs.mkdirSync(taskRoot);fs.mkdirSync(target);fs.symlinkSync(target,reports,'dir');
  fs.writeFileSync(path.join(target,'native-app.stderr.log'),"thread 'main' panicked at /private/secret-token");
  t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const result=await captureStartupDiagnostics('task',{taskRoots:()=>({root:taskRoot,reports})},{readNativeStatus:()=>({state:'active'})});
  assert.equal(result.classification,'stderr-unavailable');assert.equal(result.stderrBytes,0);
  assert.equal(result.stderrSha256,crypto.createHash('sha256').update(Buffer.alloc(0)).digest('hex'));
  assert.equal(JSON.stringify(result).includes('secret-token'),false);
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
  const s=fs.readFileSync('tests/desktop/p38-owned-window.swift','utf8')+fs.readFileSync('tests/desktop/p38-owned-projection.swift','utf8');
  assert.ok(s.includes('initialRows.count == 1'));assert.ok(s.includes('AXUIElementGetPid'));assert.ok(s.includes('CFEqual(parent,main)'));
  assert.ok(s.includes('"-l"+String(number)'));assert.equal(/\.post\s*\(|CGEventPost\s*\(|SystemWide|osascript/.test(s),false);
  assert.equal(/\w+\.postToPid\([^)]*\)|CGEvent|kAXFocusedUIElementAttribute/.test(s),false);
  assert.ok(s.includes('func attachedForeignRoot<'));assert.ok(s.includes('func boundedOwnedTree<'));
  assert.ok(s.includes('SecCodeCopyGuestWithAttributes'));assert.ok(s.includes('SecCodeCheckValidity'));
  assert.ok(s.includes('com.apple.appkit.xpc.openAndSavePanelService'));
  assert.ok(s.includes('AXUIElementPerformAction(opener,kAXPressAction as CFString)'));
  assert.ok(s.includes('AXUIElementSetAttributeValue(selected,kAXValueAttribute as CFString,text as CFString)'));
  assert.ok(s.includes('try requireResolvedSheetMutation(target,action,serviceRoot != nil,locationToken != nil)'));
  assert.ok(s.includes('func requireResolvedSheetMutation('));
  assert.ok(s.includes('"unresolvedTransitionCases":3'));
  assert.ok(s.includes('"serviceIdentityCases":5'));assert.ok(s.includes('"serviceAttachmentCases":5'));
  assert.ok(s.includes('confirm-unsupported'));assert.ok(s.includes('No title/value/children reads from a foreign child'));
});
for(const type of ['go-to-folder','location-dismissed']) {
  test('P38 refuses Reference-only location command before helper: '+type,async()=>{
    const f=fixture();f.services.next=async()=>({type,target:'sheet',sheetToken:'other'});
    const result=await runOperator(f.cfg,f.services);
    assert.equal(result.reason,'action-refused');assert.equal(f.events.includes('window-'+type),false);
    assert.equal(result.cleanup,'complete');assert.equal(result.helperCleanup,'complete');
  });
}
test('P38 refuses a nested location token even on its existing inspect verb',()=>{
  const {actionTarget}=require('./desktop/p38-installed-driver.cjs');
  assert.throws(()=>actionTarget({type:'inspect',target:'sheet',sheetToken:'outer',locationToken:'inner'},
    {token:'outer',locationToken:'inner'}),/action-refused/);
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
  const s=fs.readFileSync('tests/desktop/p38-owned-window.swift','utf8')+fs.readFileSync('tests/desktop/p38-owned-projection.swift','utf8');assert.ok(s.includes('try project(child,childPath,level+1)'));
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
test('main-inspect refusal preserves only finite diagnostic enums and never retries',async()=>{
  const {safeInspectionDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  const diagnostics=[
    {stage:'identity-precondition',queryClass:'identity-check-failed'},
    {stage:'request-precondition',queryClass:'request-refused'},
    {stage:'main-children-query',queryClass:'attribute-query-failed'},
    {stage:'main-children-value',queryClass:'attribute-value-unreadable'},
    {stage:'application-windows-query',queryClass:'attribute-query-failed'},
    {stage:'application-windows-value',queryClass:'attribute-value-unreadable'},
    {stage:'main-sheet-state',queryClass:'attached-sheet-present'},
    {stage:'root-resolution',queryClass:'route-refused'},
    {stage:'projection-budget',queryClass:'budget-refused'},
    {stage:'ax-owner-query',queryClass:'ownership-refused'},
    {stage:'ax-attribute-query',queryClass:'attribute-query-failed'},
    {stage:'ax-children-query',queryClass:'attribute-value-unreadable'},
    {stage:'ax-projection',queryClass:'topology-refused'},
  ];
  for(const diagnostic of diagnostics)assert.deepEqual(safeInspectionDiagnostics(diagnostic),diagnostic);
  for(const diagnostic of [null,[],{stage:'main-children-query',queryClass:'attribute-query-failed',title:'private'},
    {stage:'/private/path',queryClass:'secret'}, {stage:'main-children-query',queryClass:'attached-sheet-present'},
    {stage:'main-sheet-state',queryClass:'attribute-query-failed'}])assert.equal(safeInspectionDiagnostics(diagnostic),undefined);
  for(const diagnostic of [...diagnostics,{stage:'/private/path',queryClass:'secret',title:'private',value:'/private/file'}]){
    const f=fixture({window(mode,pid,wid,action){return mode==='discover'?{pid,windowId:45,matchedAX:true}:
      mode==='check'?{pid,windowId:45,matchedAX:true}:
      {status:'refused',code:'adapter-refused',pid,windowId:45,matchedAX:true,inspectionDiagnostics:diagnostic};}});
    const result=await runOperator(f.cfg,f.services);
    assert.equal(result.status,'partial');assert.equal(result.reason,'adapter-refused');
    assert.equal(f.attempts,1);assert.equal(f.cleaned,1);
    assert.equal(JSON.stringify(result).includes('/private/'),false);
    if(diagnostics.includes(diagnostic))assert.deepEqual(result.firstFailure.inspectionDiagnostics,diagnostic);
    else assert.equal(result.firstFailure.inspectionDiagnostics,undefined);
  }
  const sheet=fixture();let step=0;
  sheet.services.next=async()=>step++===0?{type:'pin-sheet'}:{type:'inspect',target:'sheet',sheetToken:'da5f47f9-5681-4e71-a0b6-aa7578b6b5fa'};
  sheet.services.window=async(mode,pid)=>mode==='discover'?{pid,windowId:45,matchedAX:true}:
    mode==='pin-sheet'?{pid,windowId:45,matchedAX:true,target:'sheet',sheetToken:'da5f47f9-5681-4e71-a0b6-aa7578b6b5fa'}:
      {status:'refused',code:'adapter-refused',pid,windowId:45,matchedAX:true,inspectionDiagnostics:diagnostics[2]};
  const sheetResult=await runOperator(sheet.cfg,sheet.services);
  assert.equal(sheetResult.reason,'adapter-refused');assert.equal(sheetResult.firstFailure.inspectionDiagnostics,undefined);
});
test('helper transport preserves bounded main-inspect diagnostic through its safe pipe boundary',async()=>{
  const {createWindowTransport,cleanupHelper,safeDirectory}=require('./desktop/p38-installed-driver.cjs');
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'p38-inspect-diagnostic-')),helpers=new Set();
  try{
    const fixtures=['A','B'].map(label=>{const file=path.join(dir,label+'.json');fs.writeFileSync(file,JSON.stringify({label}));
      return{label,path:file,sha256:crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex')};});
    const diagnostic={stage:'application-windows-query',queryClass:'attribute-query-failed'};
    const helper=path.join(dir,'owned-window-helper');
    const refused=JSON.stringify({status:'refused',code:'adapter-refused',pid:'__PID__',windowId:45,
      matchedAX:true,inspectionDiagnostics:diagnostic}).replace('"__PID__"','r.pid');
    const helperSource=['#!/usr/bin/env node',
      'console.log(JSON.stringify({status:"pinned",pid:Number(process.argv[3]),windowId:45,matchedAX:true}));',
      'require("node:readline").createInterface({input:process.stdin}).on("line",line=>{const r=JSON.parse(line);',
      'const result=r.type==="check"?{status:"ok",type:r.type,pid:r.pid,windowId:45,matchedAX:true}: '+refused+';',
      'console.log(JSON.stringify(result));});',''].join('\n');
    fs.writeFileSync(helper,helperSource,{mode:0o700});
    const guard=safeDirectory(dir,['reports']),window=createWindowTransport({workflow:'P38',windowHelper:helper,
      grant:{id:'modeled-inspect'},fixtures},guard.directory,()=>guard,helpers);
    const pinned=await window('discover',123);assert.equal(pinned.windowId,45);
    await window('check',123,45);
    await assert.rejects(window('inspect',123,45,{type:'inspect',target:'main'}),error=>{
      assert.deepEqual(error.inspectionDiagnostics,diagnostic);assert.equal(error.code,'adapter-refused');return true;
    });
  }finally{
    for(const child of [...helpers])await cleanupHelper(child);
    fs.rmSync(dir,{recursive:true,force:true});
  }
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
  f.services.close=async()=>{throw Object.assign(Error('/private/group'),{cleanupDiagnostics:[{stage:'group-remains',helperPid:321,descendantPossible:true,
    groupAliveBefore:true,groupAliveAfterTerm:true,groupAliveAfterKill:true,signalAttempts:2}]});};
  const result=await runOperator(f.cfg,f.services);
  assert.equal(result.firstFailure.code,'adapter-refused');assert.equal(result.firstFailure.stage,'window-press-main');
  assert.deepEqual(result.firstFailure.actionDiagnostics,clean);
  assert.deepEqual(result.helperCleanupDiagnostics,[{stage:'group-remains',helperPid:321,descendantPossible:true,
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
