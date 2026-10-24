'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Desktop=require('../src/js/adapters/native-object-project-desktop.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const Project=require('../src/js/adapters/native-object-project.js');
const Workflow=require('../src/js/application/native-object-save-as-workflow.js');
const Ack=require('../src/js/application/native-object-saved-acknowledgement.js');
const turn=()=>new Promise(setImmediate);
function fixture(mode){
  let installed=false,previews=0,visible=true,pendingBytes=null;
  const writes=[],commands=[],effects=[],notices=[];
  const info=()=>({apiVersion:2,instanceId:'instance',documentId:'document',lifecycleGeneration:1,contentRevision:0,
    documentSnapshotId:'snapshot',canvasW:320,canvasH:180,fps:24,totalFrames:21,
    documentDigest:'a'.repeat(64),byteCount:100,reference:null,undoDepth:0,redoDepth:0,layers:[{layerUid:'drawing',visible:true}]});
  const root={state:{},document:{getElementById:()=>({classList:{contains:()=>!visible}})},
    requestAnimationFrame:fn=>{if(mode!=='layout')setImmediate(fn);},
    updateUI:()=>{effects.push('update');if(mode==='ui-refresh')throw Error('PRIVATE /Users/name/document.json');},
    NemoNativeObjectProjectFile:Files,NemoNativeObjectProjectDesktop:Desktop,
    NemoNativeObjectSaveAsWorkflow:Workflow,NemoNativeObjectSavedAcknowledgement:Ack,
    SMProjectDocument:{baseName:()=> 'PRIVATE DOCUMENT'},showToast:text=>notices.push(text),
    NemoNativeViewportUnderlay:{desktop:()=>({frame:()=>0,mapping:()=>({cssBounds:{x:0,y:0,width:320,height:180},
      physicalExtent:{width:320,height:180},compositionExtent:{width:320,height:180},reportedDpr:1})})}};
  root.__TAURI__={core:{invoke:async(command,args)=>{
    const request=args&&JSON.parse(args.requestJson);commands.push(command);
    if(command==='nemo_native_object_client_status')return {...info(),available:installed};
    if(command==='nemo_native_object_bootstrap'){installed=true;return {...info(),requestId:request.requestId,resourceCount:0,viewportAvailable:false};}
    if(command==='nemo_native_object_reference')return {...info(),requestId:request.requestId};
    if(command==='nemo_native_object_preview'){
      previews++;
      if(mode==='invoke')throw {code:'unavailable',message:'PRIVATE /secret/file'};
      return {status:mode==='second-preview'&&previews===2?'deferred-occluded':'presented',workId:'work-'+previews,viewGeneration:1};
    }
    throw Error('Unexpected native command '+command);
  }},path:{appDataDir:async()=>'/private/app-data/'},fs:{mkdir:async()=>{},writeTextFile:async(path,bytes)=>{
    if(mode==='sink')throw Error('PRIVATE STORAGE FAILURE');
    assert.equal(path,'/private/app-data/diagnostics/native-open.pending.json');pendingBytes=bytes;
  },rename:async(from,to)=>{
    assert.equal(from,'/private/app-data/diagnostics/native-open.pending.json');assert.equal(to,'/private/app-data/diagnostics/native-open.json');
    if(mode==='rename')throw Error('PRIVATE RENAME FAILURE');writes.push({path:to,bytes:pendingBytes});
  }}};
  if(mode==='projection')Object.defineProperty(root.state,'canvasW',{set(){throw Error('PRIVATE PROJECTION');}});
  const session=Project.create(Desktop.createPorts(root));
  root.NemoNativeObjectProject={desktop:()=>session};
  const ports={read:()=>({name:'PRIVATE',tabId:'tab'}),show:()=>{visible=true;effects.push('show');},
    hide:()=>{effects.push('hide');if(mode==='hide-start')throw Error('PRIVATE HIDE');visible=false;},
    touchRecent:()=>{effects.push('recent');if(mode==='commit')throw Error('PRIVATE RECENT');},commit:()=>effects.push('commit'),
    renderRecents:()=>{},opened:()=>effects.push('opened'),notice:text=>notices.push(text),repaint:()=>{}};
  return {root,session,ui:Files.bind(root,ports),writes,commands,effects,notices,
    async records(){await turn();await turn();return writes.length?JSON.parse(writes.at(-1).bytes).events:[];},visible:()=>visible};
}
test('ordinary File bind and Desktop ports persist production Open stages without fixture activation',async()=>{
  const f=fixture();assert.equal(f.root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__,undefined);
  assert.equal(await f.ui.openPath('/PRIVATE/project.json'),true);
  const events=await f.records();
  assert.deepEqual(events.map(e=>[e.event,e.code]),[
    ['file-open-started',null],['bootstrap-dispatched',null],['bootstrap-resolved',null],
    ['file-stage','projection'],['file-stage','ui-refresh'],['file-stage','hide-start'],
    ['file-stage','layout-wait'],['file-stage','layout-settled'],['file-completed',null]]);
  assert.equal(f.visible(),false);assert.equal(f.session.isActive(),true);
  assert.ok(f.effects.includes('recent'));assert.ok(f.effects.includes('opened'));
  assert.ok(f.writes.every(w=>w.path==='/private/app-data/diagnostics/native-open.json'));
  assert.doesNotMatch(JSON.stringify(events),/PRIVATE|project.json|documentId|snapshot|stack|message/);
});
for(const [mode,stage]of [['invoke',null],['projection','projection'],['ui-refresh','ui-refresh'],['hide-start','hide-start'],['layout','layout-wait'],['second-preview','layout-settled'],['commit','layout-settled']]){
  test('ordinary Open records '+mode+' failure without accepting it',async()=>{
    const f=fixture(mode);assert.equal(await f.ui.openPath('/PRIVATE/project.json'),false);
    const events=await f.records();
    assert.equal(events.at(-1).event,'file-failed');assert.equal(events.at(-1).code,'unconfirmed');
    assert.equal(events.filter(e=>e.event==='file-stage').at(-1)?.code,stage||undefined);
    const failure=events.find(e=>e.event==='post-bootstrap-failed');assert.ok(failure);
    if(mode==='invoke')assert.equal(failure.code,'post-bootstrap-first-preview:invoke-rejected:unavailable:other');
    if(mode==='second-preview')assert.equal(failure.code,'post-bootstrap-second-preview-status-deferred-occluded:local-refusal:unknown:other');
    if(['projection','ui-refresh','hide-start','layout'].includes(mode))assert.equal(failure.code,'post-bootstrap-reveal:local-refusal:unknown:other');
    assert.equal(f.session.isActive(),false);assert.equal(f.visible(),true);
    assert.equal(f.effects.includes('opened'),false);assert.doesNotMatch(JSON.stringify(events),/PRIVATE|secret|project.json|stack|message/);
  });
}
test('missing/rejected/hung storage and throwing optional observer do not block or change Open',async()=>{
  const control=fixture();assert.equal(await control.ui.openPath('/x'),true);
  for(const kind of ['sink','rename','missing','hung','observer']){
    const f=fixture(kind);
    if(kind==='missing')delete f.root.__TAURI__.path;
    if(kind==='hung')f.root.__TAURI__.fs.writeTextFile=()=>new Promise(()=>{});
    if(kind==='observer')f.root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__=()=>{throw Error('PRIVATE OBSERVER');};
    assert.equal(await f.ui.openPath('/x'),true,kind);
    assert.deepEqual(f.commands,control.commands,kind+': original native command order');
    assert.deepEqual(f.effects,control.effects,kind+': original UI effects');
  }
});
test('collector bounds records, sanitizes all code slots and coalesces while one write is pending',async()=>{
  const f=fixture();let release;let writes=0,active=0,maxActive=0,last;
  f.root.__TAURI__.fs.writeTextFile=async(path,bytes)=>{
    writes++;active++;maxActive=Math.max(active,maxActive);last=bytes;
    if(writes===1)await new Promise(resolve=>{release=resolve;});active--;
  };
  Desktop.openDiagnostic(f.root,'file-open-started');await turn();
  for(let i=0;i<100;i++)Desktop.openDiagnostic(f.root,'file-stage','PRIVATE /Users/secret');
  Desktop.openDiagnostic(f.root,'post-bootstrap-failed','post-bootstrap-PRIVATE:invoke-rejected:unavailable:other');
  Desktop.openDiagnostic(f.root,'PRIVATE EVENT','PRIVATE CODE');
  assert.equal(writes,1);release();await turn();await turn();
  const log=JSON.parse(last);assert.equal(log.events.length,32);assert.equal(writes,2);assert.equal(maxActive,1);
  assert.equal(log.events.at(-1).code,'unknown');assert.equal(log.events.at(-1).sequence,102);
  assert.doesNotMatch(last,/PRIVATE|Users|secret/);
});
test('safe fixture observer remains optional and receives the existing bootstrap vocabulary',async()=>{
  const f=fixture(),observed=[];f.root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__=(event,code)=>observed.push([event,code]);
  assert.equal(await f.ui.openPath('/x'),true);
  assert.deepEqual(observed.filter(e=>e[0].startsWith('bootstrap')).map(e=>e[0]),['bootstrap-dispatched','bootstrap-resolved']);
});
test('file terminal records distinguish cancellation and pre-dispatch refusal without error text',async()=>{
  for(const mode of ['cancelled','refused']){
    const f=fixture(),error={message:'PRIVATE REFUSAL',details:{disposition:'notDispatched'}};
    const session={readPresentedObjectSession:()=>null,info:()=>null,openPath:async()=>{if(mode==='refused')throw error;return null;}};
    const notices=[];
    const ui=Files.create(session,{readUI:()=>({handle:'tab',generation:1}),
      openDiagnostic:(event,code)=>Desktop.openDiagnostic(f.root,event,code),repaint:()=>{},
      refused:e=>notices.push(e),failed:()=>assert.fail('pre-dispatch refusal must retain its original handler')},Workflow,Ack);
    assert.equal(await ui.openPath('/PRIVATE'),mode==='cancelled'?null:false);
    const records=await f.records();assert.equal(records.at(-1).event,mode==='cancelled'?'file-cancelled':'file-failed');
    assert.equal(records.at(-1).code,mode==='cancelled'?null:'not-dispatched');
    if(mode==='refused')assert.equal(notices[0],error);
    assert.doesNotMatch(JSON.stringify(records),/PRIVATE/);
  }
});
test('collector publishes complete snapshots only after the pending write and rename finish',async()=>{
  const f=fixture(),calls=[];let release;
  f.root.__TAURI__.fs.writeTextFile=async(path,bytes)=>{calls.push(['write',path,JSON.parse(bytes)]);await new Promise(resolve=>{release=resolve;});};
  f.root.__TAURI__.fs.rename=async(from,to)=>calls.push(['rename',from,to]);
  Desktop.openDiagnostic(f.root,'file-open-started');await turn();
  assert.equal(calls.length,1);assert.equal(calls[0][0],'write');
  assert.equal(calls[0][2].events[0].event,'file-open-started');
  release();await turn();assert.equal(calls.length,2);assert.equal(calls[1][0],'rename');
  assert.equal(calls[1][2],'/private/app-data/diagnostics/native-open.json');
});


test('BPM entry refusal uses only fixed collector vocabulary and never records caller detail',async()=>{
  const f=fixture(),observed=[];f.root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__=(event,code)=>observed.push([event,code]);
  const codes=['controller-missing','cache-empty','retired','generation-stale','session-replaced','inactive',
    'five-pin-stale','subscription-stale','presentation-missing','presentation-mismatch','projection-unready','exception'];
  for(const code of codes)Desktop.openDiagnostic(f.root,'bpm-settings-refused',code);
  for(const code of ['PRIVATE /Users/secret document-id stack',null,{documentId:'PRIVATE'}])
    Desktop.openDiagnostic(f.root,'bpm-settings-refused',code);
  await turn();await turn();const records=await f.records();
  assert.deepEqual(records.map(record=>record.code),[...codes,'unknown','unknown','unknown']);
  assert.ok(records.every(record=>record.event==='bpm-settings-refused'));
  assert.deepEqual(observed.map(record=>record[1]),[...codes,'unknown','unknown','unknown']);
  assert.doesNotMatch(JSON.stringify(records),/PRIVATE|Users|secret|document-id|stack/);
  assert.deepEqual(f.commands,[]);assert.deepEqual(f.effects,[]);
});

test('tab capture refusal records a bounded fixed vocabulary without caller or project detail',async()=>{
  const f=fixture(),observed=[];
  f.root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__=(event,code)=>observed.push([event,code]);
  const codes=['inactive','synchronizing','subscription-inactive','presentation-missing','view-mismatch',
    'owner-mismatch','queue-pending','capture-superseded','capture-unavailable','tab-unavailable','ui-changed'];
  for(const code of codes)Desktop.openDiagnostic(f.root,'tab-capture-refused',code);
  for(const code of ['PRIVATE /Users/secret/name.json',null,{documentId:'PRIVATE'}])
    Desktop.openDiagnostic(f.root,'tab-capture-refused',code);
  const records=await f.records();
  assert.deepEqual(records.map(record=>record.code),[...codes,'unknown','unknown','unknown']);
  assert.ok(records.every(record=>record.event==='tab-capture-refused'));
  assert.deepEqual(observed.map(record=>record[1]),[...codes,'unknown','unknown','unknown']);
  assert.doesNotMatch(JSON.stringify(records),/PRIVATE|Users|secret|name.json|documentId/);
  for(let i=0;i<40;i++)Desktop.openDiagnostic(f.root,'tab-capture-refused','queue-pending');
  assert.equal((await f.records()).length,32);
  f.root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__=()=>{throw Error('PRIVATE observer failure');};
  assert.doesNotThrow(()=>Desktop.openDiagnostic(f.root,'tab-capture-refused','ui-changed'));
  assert.equal((await f.records()).at(-1).code,'ui-changed');
  assert.deepEqual(f.commands,[]);assert.deepEqual(f.effects,[]);
});
