'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {verifyEvidence,runEvidenceFile,plan}=require('./desktop/play-publication-oracle.cjs');
const {encodeRGBA,canvas,fillRect,crc32}=require('./fixtures/lib/png.cjs');
const crypto=require('node:crypto');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const RUN='da5f47f9-5681-4e71-a0b6-aa7578b6b5fa',CUT='3d755db1-cdf7-44f8-8706-cc812e1845ca';
// Independent literal MODEL evidence. Never an installed app/window or acceptance receipt.
function model(){
  const identity={instanceId:RUN,documentId:'native-document-1',contentRevision:0};
  const context=(call,frame,status)=>({...identity,documentSnapshotId:'native-opacity:native-document-1:0',contextId:'scene-root',
    frame,jsLifecycleGeneration:1,hostGeneration:7,sessionOrdinal:1,hostCallOrdinal:call,
    workId:'native-work-'+call,viewGeneration:call,status});
  const events=[{event:'ready'},{event:'play-start',...identity,runGeneration:1},{event:'play-state',playing:true,currentFrame:0}];
  const request=(intent,call,frame,status)=>{
    const c=context(call,frame,status),base={...c,intent};
    events.push({event:'navigate-request',...identity,intent,frame},{event:'host-request',...c},{event:'host-receipt',...c});
    if(status==='presented'){
      events.push({event:'presented-return',...c});
      for(const event of ['commit-enter','ui-publication','commit-return','paint-enter','paint-return','sync-enter','sync-return']){
        events.push({event,...base,...(event==='ui-publication'?{currentFrame:frame}:{}),...(event==='sync-return'?{currentFrame:frame,result:true}:{})});
      }
    }else{
      const restored=context(8,0,'presented'),retry={...restored,intent,priorFrame:0};
      events.push({event:'rollback',...identity,intent,frame:0,currentFrame:0},
        {event:'receipt-retry',...base,priorFrame:0,result:false},
        {event:'host-request',...restored},{event:'host-receipt',...restored},{event:'presented-return',...restored},
        {event:'receipt-retry',...retry,result:true},{event:'rollback',...retry,currentFrame:0,result:true},
        {event:'failure',...base,code:'unknown'});
    }
    events.push({event:'navigate-return',...base,result:status==='presented'});
  };
  events.push({event:'play-step',...identity,runGeneration:1,frame:10});request(1,1,10,'deferred-occluded');
  events.push({event:'play-step',...identity,runGeneration:1,frame:10,result:false},{event:'play-state',playing:false,currentFrame:0},
    {event:'play-stop',...identity,runGeneration:2});request(2,2,0,'presented');
  events.push({event:'play-start',...identity,runGeneration:3},{event:'play-state',playing:true,currentFrame:0},
    {event:'play-step',...identity,runGeneration:3,frame:20});request(3,3,20,'presented');
  events.push({event:'play-step',...identity,runGeneration:3,frame:20,result:true},{event:'play-state',playing:false,currentFrame:20},
    {event:'play-stop',...identity,runGeneration:4});request(4,4,20,'presented');
  request(5,5,0,'presented');request(6,6,10,'presented');request(7,7,20,'presented');
  const native=[],emit=(event,fields,preview=null,exposureEpoch=1)=>native.push({diagnostic:'native-occlusion-mapping/v1',sequence:native.length+1,event,fields,preview,exposureEpoch});
  const p=(call,frame)=>({instanceId:RUN,documentId:'native-document-1',revision:0,frame,workId:call,viewGeneration:call,jsLifecycleGeneration:'MISSING'});
  emit('exposure',{windowNumber:81,minimized:true,occlusionVisible:false});
  emit('preview-begin',null,p(1,10));emit('preview-end',{status:'deferred-occluded'},p(1,10));
  emit('exposure',{windowNumber:81,minimized:false,occlusionVisible:true},null,2);
  emit('preview-begin',null,p(8,0),2);emit('preview-end',{status:'presented'},p(8,0),2);
  emit('preview-begin',null,p(2,0),2);emit('preview-end',{status:'presented'},p(2,0),2);
  emit('preview-begin',null,p(3,20),2);emit('preview-end',{status:'presented'},p(3,20),2);
  for(const[call,frame]of[[4,20],[5,0],[6,10],[7,20]]){emit('preview-begin',null,p(call,frame),2);emit('preview-end',{status:'presented'},p(call,frame),2);}
  const e={schema:'nemo.n20r-evidence/1',sourceCommit:'a'.repeat(40),executableSha256:'b'.repeat(64),fixtureSha256:'c'.repeat(64),
    taskId:'one-run-model',runId:RUN,pid:123,windowId:81,events,native};
  e.package={sourceCommit:e.sourceCommit,executableSha256:e.executableSha256,observerFeature:true,unsigned:true};
  e.collection={source:e.sourceCommit,executableSha256:e.executableSha256,taskId:e.taskId,runId:RUN,pid:e.pid,status:'complete',phase:'after-owner-stop'};
  e.cleanup={taskId:e.taskId,pid:e.pid,appExited:true,launcherExited:true,processTreeStopped:true,slotsReleased:true,taskRemoved:true};
  e.visuals=[[0,0,20,204],[10,64,50,128],[20,128,80,51]].map(([frame,x,opacity,color])=>{
    const image=canvas(320,180,[255,255,255,255]);fillRect(image,20+x,60,20,20,[255,color,color,255]);
    const png=encodeRGBA(image.width,image.height,image.rgba);
    return{...context(frame===0?5:frame===10?6:7,frame,'presented'),frame,positionX:x,opacity,displayedFrame:frame+1,windowOnly:true,reviewed:true,reviewer:'independent-model-review',
      sourceCommit:e.sourceCommit,executableSha256:e.executableSha256,pid:e.pid,windowId:e.windowId,viewport:{x:0,y:0,width:320,height:180},png,sha256:hash(png)};
  });return refresh(e);
}
function refresh(e){
  const header={kind:'header',schema:'nemo.play-observer/1',runId:RUN,taskId:e.taskId,pid:e.pid,window:'main'};
  const counters={runId:RUN,accepted:e.events.length,generated:e.events.length,dropped:0,failed:0,rejected:0,cutId:CUT};
  const records=[header,...e.events.map((event,i)=>({kind:'event',runId:RUN,sequence:i+1,event})),
    {kind:'seal',status:'provisional',...counters},{kind:'footer',status:'complete',terminalFence:true,pending:0,...counters}];
  e.stderr=Buffer.from(e.native.map(r=>JSON.stringify(r)+'\n').join('')+records.map(r=>'NEMO_PLAY_OBSERVER '+JSON.stringify(r)+'\n').join(''));
  e.collection.stderrSha256=hash(e.stderr);return e;
}
test('literal complete model binds actual-input order, native recovery, visuals and terminal cleanup',()=>{
  const result=verifyEvidence(model());assert.equal(result.status,'consistent');assert.equal(result.publications,6);assert.equal(result.failures,1);
  assert.match(result.limitations.join(' '),/independent installed review/);
});
for(const event of ['host-receipt','presented-return','ui-publication','paint-return','sync-return','navigate-return'])test(`omitted ${event} cannot be accepted`,()=>{
  const e=model();e.events.splice(e.events.findIndex(x=>x.event===event&&x.hostCallOrdinal===2),1);
  assert.equal(verifyEvidence(refresh(e)).status,'partial');
});
for(const [kind,key,value]of [['host-receipt','hostGeneration',8],['commit-enter','sessionOrdinal',2],
  ['ui-publication','currentFrame',19],['sync-return','frame',19],['sync-return','result',false],
  ['failure','hostCallOrdinal',2],['failure','documentSnapshotId','wrong'],['paint-return','workId','native-work-99'],
  ['navigate-return','result',false]])test(`mismatched ${kind}/${key} refuses`,()=>{
  const e=model();const row=e.events.find(x=>x.event===kind&&(kind==='failure'||x.hostCallOrdinal===2));row[key]=value;
  assert.equal(verifyEvidence(refresh(e)).status,'partial');
});
test('reordered paint/input, superseded true result and unawaited Stop work refuse',()=>{
  for(const mode of ['order','newer','pending','rollback']){
    const e=model();const at=e.events.findIndex(x=>x.event==='navigate-return'&&x.intent===2);
    if(mode==='order'){const a=e.events.findIndex(x=>x.event==='paint-return'),b=e.events.findIndex(x=>x.event==='sync-return');[e.events[a],e.events[b]]=[e.events[b],e.events[a]];}
    if(mode==='newer')e.events.splice(at,0,{event:'navigate-request',instanceId:RUN,documentId:'native-document-1',contentRevision:0,intent:99,frame:0});
    if(mode==='pending')e.events.push({event:'play-step',instanceId:RUN,runGeneration:5,frame:0});
    if(mode==='rollback')e.events.splice(at,0,{event:'rollback',instanceId:RUN,documentId:'native-document-1',intent:2,frame:0,currentFrame:0});
    assert.equal(verifyEvidence(refresh(e)).status,'partial',mode);
  }
});
test('wrong native window/work/status, missing restore and unknown-generation substitutions refuse',()=>{
  for(const mode of ['window','work','status','restore','unsafe']){
    const e=model();if(mode==='window')e.native[0].fields.windowNumber=99;
    if(mode==='work')e.native[5].preview.workId=99;
    if(mode==='status')e.native[5].fields.status='stale-discarded';
    if(mode==='restore')e.native[3].fields.minimized=true;
    if(mode==='unsafe')e.native[5].preview.workId=Number.MAX_SAFE_INTEGER+1;
    assert.equal(verifyEvidence(refresh(e)).status,'partial',mode);
  }
});
test('owned visual source/window/input/pixel/hash and independent review are mandatory',()=>{
  for(const mode of ['window','source','input','pixels','hash','review']){
    const e=model(),v=e.visuals[1];if(mode==='window')v.windowId=99;if(mode==='source')v.sourceCommit='d'.repeat(40);
    if(mode==='input')v.displayedFrame=10;if(mode==='review')v.reviewed=false;if(mode==='hash')v.sha256='0'.repeat(64);
    if(mode==='pixels'){v.png=e.visuals[0].png;v.sha256=hash(v.png);}
    assert.equal(verifyEvidence(e).status,'partial',mode);
  }
});
test('terminal fence, zero-loss, copy hash and official cleanup cannot be replaced by a passing manifest',()=>{
  for(const mode of ['hash','cleanup','fence','loss']){
    const e=model();if(mode==='hash')e.collection.stderrSha256='0'.repeat(64);if(mode==='cleanup')e.cleanup.appExited=false;
    if(mode==='fence'||mode==='loss'){e.stderr=Buffer.from(e.stderr.toString().replace(mode==='fence'?'"terminalFence":true':'"dropped":0',mode==='fence'?'"terminalFence":false':'"dropped":1'));e.collection.stderrSha256=hash(e.stderr);}
    assert.equal(verifyEvidence(e).status,'partial',mode);
  }
});
test('offline file driver is read-only and rejects traversal/symlink/missing evidence',t=>{
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'nemo-oracle-'));t.after(()=>fs.rmSync(dir,{recursive:true,force:true}));
  const e=model();fs.writeFileSync(path.join(dir,'log.txt'),e.stderr);const manifest={...e,stderr:undefined,events:undefined,native:undefined,stderrFile:'log.txt',
    visuals:e.visuals.map((v,i)=>{fs.writeFileSync(path.join(dir,`frame${i}.png`),v.png);return {...v,png:undefined,file:`frame${i}.png`};})};
  const file=path.join(dir,'evidence.json');fs.writeFileSync(file,JSON.stringify(manifest));const original=fs.readFileSync(file);
  assert.equal(runEvidenceFile(file).status,'consistent');assert.deepEqual(fs.readFileSync(file),original);
  for(const relative of ['../outside','/absolute']){fs.writeFileSync(file,JSON.stringify({...manifest,stderrFile:relative}));assert.equal(runEvidenceFile(file).status,'partial');}
  fs.symlinkSync('log.txt',path.join(dir,'alias'));fs.writeFileSync(file,JSON.stringify({...manifest,stderrFile:'alias'}));assert.equal(runEvidenceFile(file).code,'evidence-symlink');
  assert.match(plan().join(' '),/no automatic second launch/);
});

test('direct exported O1/preview/navigator/scheduler producers use deterministic ports, never test-source eval',async t=>{
  const {createObserver}=require('./desktop/play-publication-observer-bootstrap.js');
  const Preview=require('../src/js/adapters/native-opacity-preview.js');
  const Motion=require('../src/js/adapters/native-opacity-motion-surface.js');
  const records=[],identity={instanceId:RUN,documentId:'native-document-1',contentRevision:0};let seal,frame=0,playing=false,runFrame=10,now=0,step;
  const api=createObserver({runId:RUN},{async listen(fn){seal=()=>fn({payload:{runId:RUN}});},async invoke(action,payload){
    if(action==='record'){records.push(payload.event);return{sequence:payload.sequence};}
    if(action==='seal')return{...payload,kind:'seal',status:'provisional',cutId:CUT};return{};}});await api.ready;
  const oldWindow=global.window;global.window={NemoTestPlayObserver:api};t.after(()=>{if(oldWindow===undefined)delete global.window;else global.window=oldWindow;});
  const raw=Preview.createNativeOpacityPreviewAdapter();api.bindPreview(raw,Object.freeze({}),()=>7);let call=0,hostGate=null;
  const controller={isActive:()=>true,identity:()=>identity,async presentPreview(value){
    const meta={documentSnapshotId:'native-opacity:native-document-1:0',documentId:identity.documentId,contentRevision:0,contextId:'scene-root',frame:value,
      quality:'final',outputSpec:{kind:'frame',format:'rgba8',width:320,height:180,colorInterpretation:'srgb',alphaMode:'straight'},
      geometryHandle:{resourceId:'geometry-1',resourceVersion:'v1'}};
    const receipt=await raw.present(RUN,1,meta,async()=>{const ordinal=++call;if(hostGate)await hostGate;
      return{workId:'native-work-'+ordinal,viewGeneration:ordinal,status:ordinal===1?'deferred-occluded':'presented'};},()=>{});
    if(receipt.status!=='presented'){const error=new Error('model deferred');api.associateFailure(receipt,error);throw error;}
    const final=Object.freeze({owner:'native',...receipt});api.presentedReturn(receipt,final);return final;
  }};
  const nav=Motion.createFrameNavigator({frame:()=>frame,total:()=>21,controller:()=>controller,commit(value){frame=value;api.uiPublication(frame,frame);},
    paint(){},syncInput(){api.inputPublication(frame+1,frame,frame);},fail(){}});
  let scheduler;const stop=()=>{if(!playing)return;playing=false;api.playState(false,frame);const state=scheduler.stop();if(state.restore)nav.navigate(controller,frame);};
  scheduler=Motion.createPlaybackScheduler({frame:()=>frame,fps:()=>10,playing:()=>playing,controller:()=>controller,advance:()=>runFrame,
    navigate:value=>nav.navigate(controller,value),stop,now:()=>now,request:fn=>{step=fn;return 1;},cancel(){}});
  const drain=async()=>{for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));};
  assert.equal(scheduler.start(controller),true);playing=true;api.playState(true,frame);now+=100;step(now);await drain();
  assert.equal(playing,false);assert.equal(api.inspect().productPendingNavigation,0);
  runFrame=20;assert.equal(scheduler.start(controller),true);playing=true;api.playState(true,frame);now+=100;step(now);await drain();stop();await drain();
  assert.deepEqual(await Promise.all([nav.navigate(controller,10),nav.navigate(controller,20)]),[false,true]);
  let release;hostGate=new Promise(resolve=>{release=resolve;});
  const held=nav.navigate(controller,10);await drain();const newer=nav.navigate(controller,20);
  hostGate=null;release();assert.deepEqual(await Promise.all([held,newer]),[false,true]);
  for(const value of[0,10,20])assert.equal(await nav.navigate(controller,value),true);await drain();await seal();
  const e=model();e.events=records;e.native=[];
  let restored=false;const emit=(event,fields,preview=null)=>e.native.push({diagnostic:'native-occlusion-mapping/v1',
    sequence:e.native.length+1,event,fields,preview,exposureEpoch:restored?2:1});
  emit('exposure',{windowNumber:81,minimized:true,occlusionVisible:false});
  for(const row of e.events.filter(e=>e.event==='host-receipt')){
    if(!restored&&row.status==='presented'){restored=true;emit('exposure',{windowNumber:81,minimized:false,occlusionVisible:true});}
    const preview={instanceId:row.instanceId,documentId:row.documentId,revision:row.contentRevision,frame:row.frame,
      workId:Number(row.workId.slice(12)),viewGeneration:row.viewGeneration,jsLifecycleGeneration:'MISSING'};
    emit('preview-begin',null,preview);emit('preview-end',{status:row.status},preview);
  }
  for(const v of e.visuals){const terminal=e.events.filter(e=>e.event==='navigate-return'&&e.result&&e.frame===v.frame).at(-1);
    for(const key of['instanceId','documentId','contentRevision','hostCallOrdinal','workId','viewGeneration'])v[key]=terminal[key];}
  const result=verifyEvidence(refresh(e));assert.equal(result.status,'consistent',JSON.stringify(result));
  const ack=records.find(row=>row.event==='rollback'&&row.result===true);assert.ok(ack);
  assert.equal(ack.intent,1);assert.equal(ack.frame,0);assert.equal(ack.hostCallOrdinal,2);
  for(const mode of['omit-request','omit-presented','omit-ack','omit-chain','foreign-intent','foreign-work','foreign-call',
    'foreign-revision','foreign-generation','foreign-session','foreign-prior','missing-work','missing-view','false-ack','early-ack','newer-intent',
    'terminal-revision','terminal-frame','terminal-call','terminal-work','terminal-view','terminal-all','relabel-superseded','relabel-clean',
    'superseded-terminal-frame','superseded-terminal-revision','superseded-terminal-work']){
    const altered={...e,events:structuredClone(records)},rows=altered.events;
    const request=rows.find(row=>row.event==='receipt-retry'&&row.result===false),presented=rows.find(row=>row.event==='receipt-retry'&&row.result===true);
    const ack=rows.find(row=>row.event==='rollback'&&row.result===true);
    const terminal=rows.find(row=>row.event==='navigate-return'&&row.result===false);
    if(mode==='omit-request')rows.splice(rows.indexOf(request),1);
    if(mode==='omit-presented')rows.splice(rows.indexOf(presented),1);
    if(mode==='omit-ack')rows.splice(rows.indexOf(ack),1);
    if(mode==='omit-chain')altered.events=rows.filter(row=>!['rollback','receipt-retry'].includes(row.event));
    if(mode==='relabel-superseded'||mode==='relabel-clean'){
      altered.events=rows.filter(row=>!['rollback','receipt-retry'].includes(row.event));
      const fake=altered.events.find(row=>row.event==='failure');fake.event='navigate-superseded';
      if(mode==='relabel-clean')for(const row of[fake,terminal])for(const key of Object.keys(row))
        if(!['event','instanceId','documentId','contentRevision','frame','intent','result'].includes(key))delete row[key];
    }
    if(mode.startsWith('superseded-terminal-')){
      const superseded=rows.find(row=>row.event==='navigate-superseded'),end=rows.find(row=>row.event==='navigate-return'&&row.intent===superseded.intent);
      if(mode==='superseded-terminal-frame')end.frame=19;
      if(mode==='superseded-terminal-revision')end.contentRevision=99;
      if(mode==='superseded-terminal-work')end.workId='native-work-99';
    }
    if(mode==='foreign-intent')ack.intent=99;
    if(mode==='foreign-work')ack.workId='native-work-99';
    if(mode==='foreign-call')ack.hostCallOrdinal=1;
    if(mode==='foreign-revision')ack.contentRevision=1;
    if(mode==='foreign-generation')ack.jsLifecycleGeneration=2;
    if(mode==='foreign-session')ack.sessionOrdinal=2;
    if(mode==='foreign-prior')ack.priorFrame=1;
    if(mode==='missing-work')delete presented.workId;
    if(mode==='missing-view')delete request.viewGeneration;
    if(mode==='false-ack')ack.result=false;
    if(mode==='early-ack'){rows.splice(rows.indexOf(ack),1);rows.splice(rows.indexOf(presented),0,ack);}
    if(mode==='newer-intent')rows.splice(rows.indexOf(ack),0,{event:'navigate-request',...identity,intent:99,frame:20});
    const mutations={'terminal-revision':{contentRevision:99},'terminal-frame':{frame:19},'terminal-call':{hostCallOrdinal:99},
      'terminal-work':{workId:'native-work-99'},'terminal-view':{viewGeneration:99},
      'terminal-all':{contentRevision:99,frame:19,hostCallOrdinal:99,workId:'native-work-99',viewGeneration:99}};
    if(mutations[mode])Object.assign(terminal,mutations[mode]);
    assert.equal(verifyEvidence(refresh(altered)).status,'partial',mode);
  }
});

for(const kind of['play-start','play-step','play-stop'])test(`removing actual scheduler ${kind} controls refuses`,()=>{
  const e=model();e.events=e.events.filter(x=>x.event!==kind);assert.equal(verifyEvidence(refresh(e)).status,'partial');
});
test('each Stop needs its successful same-frame restoration before restart/end',()=>{
  for(const intent of[2,4]){const e=model();e.events=e.events.filter(x=>x.intent!==intent&&x.hostCallOrdinal!==intent);
    assert.equal(verifyEvidence(refresh(e)).status,'partial');}
});
test('foreign revision rollback cannot share a pending intent; valid unlinked rollback remains partial',()=>{
  for(const foreign of[false,true]){
    const e=model(),index=e.events.findIndex(x=>x.event==='navigate-return'&&x.intent===3);
    e.events.splice(index,0,{event:'rollback',instanceId:RUN,documentId:'native-document-1',contentRevision:foreign?1:0,intent:3,frame:0,currentFrame:0});
    const result=verifyEvidence(refresh(e));assert.equal(result.status,'partial');
    assert.equal(result.code,foreign?'rollback-context':'false-publication');
  }
});

test('original Display P3 PNGs are measured in sRGB; wrong colors and metadata still fail',()=>{
  // Independent ColorSync reference encodings; no installed evidence or image
  // conversion is used to manufacture these synthetic full-oracle controls.
  for(const mode of ['valid','wrong-pixels','wrong-profile','out-of-gamut','bad-hash']){
    const e=model();
    for(const [i,color] of [[0,[247,206,205]],[1,[239,135,132]],[2,[235,72,63]]]){
      const v=e.visuals[i],im=canvas(320,180);
      const rgb=mode==='wrong-pixels' ? [120,120,120] : mode==='out-of-gamut' ? [255,0,0] : color;
      fillRect(im,20+v.positionX,60,20,20,[...rgb,255]);
      const raw=encodeRGBA(320,180,im.rgba),tag=Buffer.from([0,0,0,4,99,73,67,80,12,13,0,1,0,0,0,0]);
      if(mode==='wrong-profile')tag[8]=1;
      tag.writeUInt32BE(crc32(tag.subarray(4,12)),12);
      v.png=Buffer.concat([raw.subarray(0,33),tag,raw.subarray(33)]);
      if(mode!=='bad-hash')v.sha256=hash(v.png);
    }
    const originals=e.visuals.map(v=>Buffer.from(v.png)),result=verifyEvidence(e);
    assert.equal(result.status,mode==='valid'?'consistent':'partial',mode);
    if(['wrong-pixels','wrong-profile','out-of-gamut'].includes(mode))assert.equal(result.code,'visual-pixels');
    e.visuals.forEach((v,i)=>assert.deepEqual(v.png,originals[i]));
  }
});
