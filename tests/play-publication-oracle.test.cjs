'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {verifyEvidence,runEvidenceFile,plan}=require('./desktop/play-publication-oracle.cjs');
const {encodeRGBA,canvas,fillRect}=require('./fixtures/lib/png.cjs');
const crypto=require('node:crypto');
const {createRequire}=require('node:module');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const RUN='da5f47f9-5681-4e71-a0b6-aa7578b6b5fa',CUT='3d755db1-cdf7-44f8-8706-cc812e1845ca';
// Independent literal MODEL evidence. Never an installed app/window or acceptance receipt.
function model(){
  const identity={instanceId:RUN,documentId:'native-document-1',contentRevision:0};
  const context=(call,frame,status)=>({...identity,documentSnapshotId:'native-opacity:native-document-1:0',contextId:'scene-root',
    frame,jsLifecycleGeneration:1,hostGeneration:7,sessionOrdinal:1,hostCallOrdinal:call,
    workId:'native-work-'+call,viewGeneration:call,status});
  const events=[{event:'ready'},{event:'play-state',playing:true,currentFrame:0}];
  const request=(intent,call,frame,status)=>{
    const c=context(call,frame,status),base={...c,intent};
    events.push({event:'navigate-request',...identity,intent,frame},{event:'host-request',...c},{event:'host-receipt',...c});
    if(status==='presented'){
      events.push({event:'presented-return',...c});
      for(const event of ['commit-enter','ui-publication','commit-return','paint-enter','paint-return','sync-enter','sync-return']){
        events.push({event,...base,...(event==='ui-publication'?{currentFrame:frame}:{}),...(event==='sync-return'?{currentFrame:frame,result:true}:{})});
      }
    }else events.push({event:'failure',...base,code:'unknown'});
    events.push({event:'navigate-return',...base,result:status==='presented'});
  };
  request(1,1,10,'deferred-occluded');events.push({event:'play-state',playing:false,currentFrame:0});
  events.push({event:'play-state',playing:true,currentFrame:0});request(2,2,20,'presented');
  events.push({event:'play-state',playing:false,currentFrame:20});request(3,3,20,'presented');
  request(4,4,0,'presented');request(5,5,10,'presented');request(6,6,20,'presented');
  const native=[],emit=(event,fields,preview=null,exposureEpoch=1)=>native.push({diagnostic:'native-occlusion-mapping/v1',sequence:native.length+1,event,fields,preview,exposureEpoch});
  const p=(call,frame)=>({instanceId:RUN,documentId:'native-document-1',revision:0,frame,workId:call,viewGeneration:call,jsLifecycleGeneration:'MISSING'});
  emit('exposure',{windowNumber:81,minimized:true,occlusionVisible:false});
  emit('preview-begin',null,p(1,10));emit('preview-end',{status:'deferred-occluded'},p(1,10));
  emit('exposure',{windowNumber:81,minimized:false,occlusionVisible:true},null,2);
  emit('preview-begin',null,p(2,20),2);emit('preview-end',{status:'presented'},p(2,20),2);
  emit('preview-begin',null,p(3,20),2);emit('preview-end',{status:'presented'},p(3,20),2);
  for(const[call,frame]of[[4,0],[5,10],[6,20]]){emit('preview-begin',null,p(call,frame),2);emit('preview-end',{status:'presented'},p(call,frame),2);}
  const e={schema:'nemo.n20r-evidence/1',sourceCommit:'a'.repeat(40),executableSha256:'b'.repeat(64),fixtureSha256:'c'.repeat(64),
    taskId:'one-run-model',runId:RUN,pid:123,windowId:81,events,native};
  e.package={sourceCommit:e.sourceCommit,executableSha256:e.executableSha256,observerFeature:true,unsigned:true};
  e.collection={source:e.sourceCommit,executableSha256:e.executableSha256,taskId:e.taskId,runId:RUN,pid:e.pid,status:'complete',phase:'after-owner-stop'};
  e.cleanup={taskId:e.taskId,pid:e.pid,appExited:true,launcherExited:true,processTreeStopped:true,slotsReleased:true,taskRemoved:true};
  e.visuals=[[0,0,20,204],[10,64,50,128],[20,128,80,51]].map(([frame,x,opacity,color])=>{
    const image=canvas(320,180,[255,255,255,255]);fillRect(image,20+x,60,20,20,[255,color,color,255]);
    const png=encodeRGBA(image.width,image.height,image.rgba);
    return{...context(frame===0?4:frame===10?5:6,frame,'presented'),frame,positionX:x,opacity,displayedFrame:frame+1,windowOnly:true,reviewed:true,reviewer:'independent-model-review',
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
  const result=verifyEvidence(model());assert.equal(result.status,'consistent');assert.equal(result.publications,5);assert.equal(result.failures,1);
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

test('oracle consumes actual O1/O2C JS producer records without treating model native/visual evidence as installed',async()=>{
  const helperFile=path.join(__dirname,'desktop/play-observer-publication.test.cjs'),source=fs.readFileSync(helperFile,'utf8');
  const end=source.indexOf("test('real navigator");assert.ok(end>0);
  const {setup,drain}=new Function('require','__dirname',source.slice(0,end)+'\nreturn {setup,drain};')(createRequire(helperFile),path.dirname(helperFile));
  const s=await setup('on',{collector:true,receipt:r=>require('node:vm').runInContext(
    '(receipt => ({...receipt,workId:"native-work-"+receipt.viewGeneration,status:occluded?"deferred-occluded":"presented"}))',r.context)});
  s.r.context.occluded=true;s.start();assert.equal(await s.navigate(10),false);s.stop();await drain();await drain();
  s.r.context.occluded=false;s.start();assert.equal(await s.navigate(20),true);s.stop();await drain();await drain();
  for(const frame of[0,10,20])assert.equal(await s.navigate(frame),true);
  await drain();await s.seal();assert.equal(s.footer().status,'complete');
  const e=model();e.events=s.r.events();e.native=[];
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
  assert.match(result.limitations.join(' '),/independent installed review/);
});
