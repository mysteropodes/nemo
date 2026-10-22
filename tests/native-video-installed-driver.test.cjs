'use strict';
// Offline controls for the manual T12 installed video operator (#2232 point 8).
// Nothing here launches an app, a helper or ffmpeg; it cannot stand in for the run.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),os=require('node:os'),zlib=require('node:zlib');
const P=require('./desktop/video-installed-policy.cjs'),D=require('./desktop/video-installed-driver.cjs');
const crc=require('./desktop/reference-installed-policy.cjs').crc32;
const complete={complete:true,truncated:false,depthClipped:false,childrenUnreadable:false,foreignChildren:false};
const ROOT=path.resolve(__dirname,'..');
function mov(tag){return Buffer.concat([Buffer.from('00000014667479707174202000000000','hex'),Buffer.from(tag)]);}
function rawSource(label,patch){
  const spec=P.SOURCES[label],raw=Buffer.alloc(spec.frames*P.W*P.H*4);
  for(let n=0;n<spec.frames;n++)for(let y=0;y<P.H;y++)for(let x=0;x<P.W;x++)Buffer.from(P.texel(label,x,y,n)).copy(raw,((n*P.H+y)*P.W+x)*4);
  if(patch)patch(raw);return raw;
}
function setup(t){
  const root=fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()),'video-ui-test-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const a=path.join(root,'A.mov'),b=path.join(root,'B.mov');fs.writeFileSync(a,mov('A'));fs.writeFileSync(b,mov('B'));fs.mkdirSync(path.join(root,'cache'));
  const c={workflow:'VIDEO_UI',grant:{issue:2232,confirmed:true,id:'video-test-'+path.basename(root)},taskId:'video-owned',sourceHead:P.SOURCE,driverHead:'a'.repeat(40),
    executableSha256:'b'.repeat(64),helperSha256:'c'.repeat(64),timeoutMs:10000,appRoot:root,app:path.join(root,'Nemo.app'),buildProof:path.join(root,'proof.json'),
    windowHelper:path.join(root,'helper'),helperProof:path.join(root,'helper-proof.json'),reportRoot:path.join(root,'workspace'),
    fixtures:{A:{path:a,sha256:P.hash(mov('A'))},B:{path:b,sha256:P.hash(mov('B'))}}};
  const decode=file=>rawSource(path.basename(file)==='A.mov'?'A':'B');
  return {root,c,decode,prepare:(d=decode)=>P.prepareWorkspace(c,d)};
}
// Straight RGBA PNG; `skew` perturbs RGB where alpha < 255, as the compositor may.
function png(rgba,width=P.W,height=P.H){
  function chunk(type,data){const name=Buffer.from(type),out=Buffer.alloc(data.length+12);out.writeUInt32BE(data.length);name.copy(out,4);data.copy(out,8);out.writeUInt32BE(crc(Buffer.concat([name,data])),out.length-4);return out;}
  const header=Buffer.alloc(13);header.writeUInt32BE(width);header.writeUInt32BE(height,4);header[8]=8;header[9]=6;
  const raw=Buffer.alloc(height*(width*4+1));
  for(let y=0;y<height;y++)rgba.copy(raw,y*(width*4+1)+1,y*width*4,(y+1)*width*4);
  return Buffer.concat([Buffer.from('89504e470d0a1a0a','hex'),chunk('IHDR',header),chunk('IDAT',zlib.deflateSync(raw)),chunk('IEND',Buffer.alloc(0))]);
}
function frame(t,{skew=0,plan=P.PLAN,source}={}){
  const out=Buffer.alloc(P.W*P.H*4),n=P.sourceFrame(t,plan);
  for(let y=0;y<P.H;y++)for(let x=0;x<P.W;x++){const v=P.texel(source||plan.source,x,y,n);if(v[3]<255)for(let i=0;i<3;i++)v[i]=Math.max(0,Math.min(255,v[i]+skew));Buffer.from(v).copy(out,(y*P.W+x)*4);}
  return out;
}
function outputs(w,options={}){
  const folder=path.join(w.output,'nemo-native-opacity-1791970000000-1');fs.mkdirSync(folder);
  const count=options.count||P.TOTAL;
  for(let t=0;t<count;t++)fs.writeFileSync(path.join(folder,'frame_'+String(t+1).padStart(4,'0')+'.png'),png(options.frame?options.frame(t):frame(t,{skew:t%2?2:-2})));
  return folder;
}
function savedFrom(w,mutate){const value=JSON.parse(fs.readFileSync(w.project));value.extraSerializerField={kept:true};if(mutate)mutate(value);fs.writeFileSync(w.saved,JSON.stringify(value));}
function witness(c,label,target='main',value,last){
  if(value!==undefined)P.witnessAction(c,{type:'set',target:'sheet',value},null);
  P.witnessAction(c,{type:'press',target,element:'0'},last||{nodes:[{path:'0',control:label}]});
  if(target==='sheet')P.witnessAction(c,{type:'sheet-dismissed'},null);
}
const exportLast={nodes:[{path:'0',control:'Export'},...['PNG Sequence','Whole timeline','1x'].map(control=>({role:'AXPopUpButton',control}))]};
function openSave(c,w){witness(c,'Open Project…');witness(c,'Open','sheet',w.project);witness(c,'Save As…');witness(c,'Save','sheet',w.saved);}
function reopenExport(c,w){witness(c,'Open Project…');witness(c,'Open','sheet',w.saved);witness(c,'Export','main',undefined,exportLast);witness(c,'Choose','sheet',w.output);}

test('fixture formulas match the T12 host test sources and refuse any changed texel or length',()=>{
  assert.deepEqual(P.texel('A',3,2,7),[70,24,32,255]);assert.deepEqual(P.texel('A',20,5,1),[10,160,80,160]);
  assert.deepEqual(P.texel('B',4,9,11),[200,220,32,144]);assert.deepEqual(P.texel('B',31,0,0),[200,0,248,255]);
  assert.match(P.SOURCES.A.lavfi,/^nullsrc=s=32x16:r=10:d=2,format=rgba,geq=/);assert.equal(P.SOURCES.A.frames,20);assert.equal(P.SOURCES.B.frames,12);
  const host=fs.readFileSync(path.join(ROOT,'src-tauri/src/native_media_host_tests.rs'),'utf8');
  for(const label of ['A','B'])assert.ok(host.includes('"'+P.SOURCES[label].lavfi+'"'),label+' generator is the host test generator');
  for(const label of ['A','B'])assert.deepEqual(P.verifyFixtureFrames(label,rawSource(label)),{label,frames:P.SOURCES[label].frames,width:32,height:16});
  assert.throws(()=>P.verifyFixtureFrames('A',rawSource('A',raw=>{raw[raw.length-1]^=1;})),/fixture-refused/);
  assert.throws(()=>P.verifyFixtureFrames('A',rawSource('B')),/fixture-refused/);
  assert.throws(()=>P.verifyFixtureFrames('C',Buffer.alloc(4)),/fixture-refused/);
});
test('offset mapping mirrors the host test clamps; the reachable plan is A at offset 0',()=>{
  assert.deepEqual([2,9,30].map(t=>P.sourceFrame(t,{source:'B',offsetFrames:5})),[0,4,11]);
  assert.deepEqual([0,7,19,20,23].map(t=>P.sourceFrame(t)),[0,7,19,19,19]);
  assert.deepEqual(P.PLAN,{source:'A',offsetFrames:0});
  assert.equal(P.matches([10,20,30,255],[10,20,30,255]),true);assert.equal(P.matches([11,20,30,255],[10,20,30,255]),false);
  assert.equal(P.matches([12,18,30,160],[10,20,30,160]),true);assert.equal(P.matches([13,20,30,160],[10,20,30,160]),false);
  assert.equal(P.matches([10,20,30,159],[10,20,30,160]),false);
});
test('blocked steps carry source evidence that still exists at the cited lines',()=>{
  assert.deepEqual(P.BLOCKED.map(x=>x.step),['import-video','preview','replace-source','offset','time-remap']);
  const cite=/^([\w./-]+):(\d+)(?:-(\d+))?/;
  const probes={'src/js/images.js:611':'async function importVideo','src/js/adapters/native-import-desktop.js:233':'providers: { image, video }','src/js/adapters/native-time-remap-ui.js:285':'replace.disabled=!routed',
    'src/js/adapters/native-time-remap-ui.js:272':'c.video.replaceActive(p.placementId)','tests/desktop/video-installed-policy.cjs:189':'function witnessAction',
    'src/js/layer-kind.js:110':'ld.nativeVideo','src/js/motion.js:2805':"n20AllowLegacyWrite('motion-toggle-animated')",
    'src/js/adapters/native-time-remap-ui.js:262':'own.toggleTimeRemap(active())','src/js/adapters/native-time-remap-ui.js:168':'function timeRemapKeyIntent','src/js/adapters/native-project-placement-contract.js:39':'setTimeRemapKey',
    'tests/desktop/p38-owned-window.swift:282':'let actions: Set<String>','src/js/native-video-bridge.js:1378':'async function replaceNativeVideoSource'};
  for(const [where,text] of Object.entries(probes)){const [file,line]=where.split(':');
    assert.ok(fs.readFileSync(path.join(ROOT,file),'utf8').split('\n')[Number(line)-1].includes(text),where);}
  for(const item of P.BLOCKED)for(const e of item.evidence){const m=cite.exec(e);assert.ok(m&&fs.existsSync(path.join(ROOT,m[1])),e);}
  const owned=fs.readFileSync(path.join(ROOT,'tests/desktop/p38-owned-projection.swift'),'utf8');assert.equal(owned.includes('Enable time remapping'),false);
  // The JS routes exist; the installed driver still does not drive them, so these steps stay blocked as not driven.
  assert.deepEqual(P.BLOCKED.filter(x=>x.finding==='installed-step-not-driven').map(x=>x.step),['import-video','replace-source','time-remap']);
  const callers=fs.readdirSync(path.join(ROOT,'src/js/adapters')).filter(n=>n.endsWith('.js')&&n!=='native-project-placement-contract.js')
    .filter(n=>/setOffset'|shiftOffset|setTimeRemapKey/.test(fs.readFileSync(path.join(ROOT,'src/js/adapters',n),'utf8').replace(/kind: 'setOffset', index/g,'')));
  assert.deepEqual(callers,['native-time-remap-ui.js'],'only the Time Remap key intent has a UI caller; offset has none');
  assert.equal(/setOffset'|shiftOffset/.test(fs.readFileSync(path.join(ROOT,'src/js/adapters/native-time-remap-ui.js'),'utf8')),false);
  assert.deepEqual([...P.COVERED,'preview','replace-source','offset-or-time-remap'].sort(),[...P.STEPS].sort());
});
test('authorization refuses other workflows, stale sources and malformed fixtures before effects',t=>{
  const {c}=setup(t);assert.doesNotThrow(()=>P.authorize(c));
  for(const patch of [{workflow:'PNG_UI'},{workflow:'P37_REFERENCE'},{grant:{issue:1959,confirmed:true,id:'png'}},{grant:{issue:2232,confirmed:false,id:'v'}},
    {grant:null},{sourceHead:'d'.repeat(40)},{timeoutMs:600001},{driverHead:'bad'},{reportRoot:'relative'},{fixtures:{A:c.fixtures.A}},
    {fixtures:{A:c.fixtures.A,B:c.fixtures.A}},{fixtures:{A:c.fixtures.A,B:{path:'rel',sha256:c.fixtures.B.sha256}}},{fixtures:{...c.fixtures,C:c.fixtures.A}}])
    assert.throws(()=>P.authorize({...c,...patch}));
});
test('workspace stages pinned fixtures, authors the linked project and refuses reuse, drift and symlinks',t=>{
  const f=setup(t),w=f.prepare();assert.throws(()=>f.prepare(),/workspace-reused/);
  const project=JSON.parse(fs.readFileSync(w.project));
  assert.equal(project.assets[0].source.locator,path.join(f.c.reportRoot,'A.mov'));assert.equal(project.assets[1].source.locator,path.join(f.c.reportRoot,'B.mov'));
  assert.deepEqual(project.assetInventory.placements,[{id:'p1',assetId:'va',target:{kind:'drawing',contextId:'scene-root',layerUid:'V'},offsetFrames:0}]);
  assert.equal(project.drawing.totalFrames,24);assert.equal(project.project.canvasBg,'#00000000');assert.equal(w.projectSha256,P.hash(fs.readFileSync(w.project)));
  assert.equal(fs.statSync(path.join(f.c.reportRoot,'A.mov')).mode&0o777,0o400);
  fs.chmodSync(path.join(f.c.reportRoot,'B.mov'),0o600);fs.writeFileSync(path.join(f.c.reportRoot,'B.mov'),'changed');assert.throws(()=>P.verifyWorkspace(f.c),/fixture-refused/);
  const g=setup(t);fs.writeFileSync(g.c.fixtures.A.path,'not a mov');assert.throws(()=>g.prepare(),/fixture-refused/);assert.equal(fs.existsSync(g.c.reportRoot),false);
  const h=setup(t);assert.throws(()=>h.prepare(file=>rawSource('B')),/fixture-refused/);
  const k=setup(t),link=path.join(k.root,'link.mov');fs.symlinkSync(k.c.fixtures.A.path,link);assert.throws(()=>P.verifyInputs({...k.c,fixtures:{...k.c.fixtures,A:{...k.c.fixtures.A,path:link}}}));
  const m=setup(t);m.prepare();fs.renameSync(m.c.reportRoot,m.c.reportRoot+'-old');fs.mkdirSync(m.c.reportRoot);assert.throws(()=>P.verifyWorkspace(m.c),/root-refused/);
});
test('only owned paths, fresh complete projections and allowed controls can act',t=>{
  const f=setup(t),w=f.prepare(),sheet={token:'sheet'},field={...complete,target:'sheet',nodes:[{path:'0',role:'AXTextField',enabled:true}]};
  for(const value of [w.project,w.saved,w.output])P.verifyAction(f.c,{type:'set',target:'sheet',sheetToken:'sheet',element:'0',value},sheet,field);
  for(const value of [f.c.fixtures.A.path,path.join(f.c.reportRoot,'A.mov'),'/tmp/foreign.json','../A.json'])
    assert.throws(()=>P.verifyAction(f.c,{type:'set',target:'sheet',sheetToken:'sheet',element:'0',value},sheet,field),/path-refused/);
  const good={...complete,target:'main',nodes:[{path:'0',control:'Open Project…',role:'AXButton',press:true,enabled:true}]};
  P.verifyAction(f.c,{type:'press',element:'0'},{},good);
  assert.throws(()=>P.verifyAction(f.c,{type:'press',element:'0'},{},{...good,truncated:true}),/control-refused/);
  assert.throws(()=>P.verifyAction(f.c,{type:'press',element:'0'},{},{...good,nodes:[{...good.nodes[0],control:'Enable time remapping'}]}),/control-refused/);
  assert.throws(()=>P.verifyAction(f.c,{type:'press',element:'0'},{token:'sheet'},good),/sheet-refused/);
  const svg={...complete,target:'main',nodes:[{path:'0',control:'Export',role:'AXButton',press:true,enabled:true},{path:'1',control:'SVG Sequence',role:'AXPopUpButton'},{path:'2',control:'Whole timeline',role:'AXPopUpButton'},{path:'3',control:'1x',role:'AXPopUpButton'}]};
  assert.throws(()=>P.verifyAction(f.c,{type:'press',element:'0'},{},svg),/export-options-refused/);
  assert.throws(()=>P.verifyAction(f.c,{type:'artifacts',phase:'after-save'},{},null),/artifact-refused/);
  assert.throws(()=>P.verifyAction(f.c,{type:'capture',name:'private'},{},null),/capture-refused/);
});
test('UI witnesses must run open, save, reopen, export in order with the owned values',t=>{
  const f=setup(t),w=f.prepare();
  assert.throws(()=>witness(f.c,'Save As…'),/ui-order/);
  witness(f.c,'Open Project…');witness(f.c,'Open','sheet',f.c.fixtures.A.path);assert.equal(P.witnessSummary(f.c).opened,false);
  const g=setup(t),v=g.prepare();openSave(g.c,v);assert.deepEqual(P.witnessSummary(g.c),{step:2,opened:true,saved:true,reopened:false,exportPickerObserved:false});
  assert.throws(()=>witness(g.c,'Export','main',undefined,exportLast),/ui-order/);
  reopenExport(g.c,v);assert.deepEqual(P.witnessSummary(g.c),{step:4,opened:true,saved:true,reopened:true,exportPickerObserved:true});
});
test('saved structure and every exported pixel are proven against the formula, independent of the producer',t=>{
  const f=setup(t),w=f.prepare();openSave(f.c,w);savedFrom(w);
  assert.throws(()=>P.artifacts(f.c,'exported',{dirs:{appCache:path.join(f.root,'cache')}}),/ui-unwitnessed/);
  const saved=P.artifacts(f.c,'saved',{});assert.equal(saved.savedSha256,P.hash(fs.readFileSync(w.saved)));
  reopenExport(f.c,w);outputs(w);
  const r=P.artifacts(f.c,'exported',{dirs:{appCache:path.join(f.root,'cache')}});
  assert.equal(r.frames,24);assert.deepEqual(r.files.map(x=>x.sourceFrame),[...Array(20).keys(),19,19,19,19]);
  assert.ok(r.files.every(x=>x.pixels===512&&x.maxRgbDelta<=2));assert.equal(r.stagingEmpty,true);
});
test('a wrong source, frame, alpha, size, count, staging leftover or save drift never passes',t=>{
  const cases={
    replacedSource:{frame:t=>frame(t,{source:'B',plan:{source:'B',offsetFrames:0}})},
    offByOne:{frame:t=>frame(t+1>23?23:t+1)},
    alpha:{frame:t=>{const b=frame(t);b[3*4+3]=254;return b;}},
    rgbOverTolerance:{frame:t=>frame(t,{skew:3})},
    count:{count:23},
  };
  for(const [name,options] of Object.entries(cases)){
    const f=setup(t),w=f.prepare();openSave(f.c,w);savedFrom(w);P.artifacts(f.c,'saved',{});reopenExport(f.c,w);outputs(w,options);
    assert.throws(()=>P.artifacts(f.c,'exported',{dirs:{appCache:path.join(f.root,'cache')}}),/pixel-mismatch|manifest-refused/,name);
  }
  const f=setup(t),w=f.prepare();openSave(f.c,w);savedFrom(w);P.artifacts(f.c,'saved',{});reopenExport(f.c,w);
  const folder=path.join(w.output,'nemo-native-opacity-1791970000000-1');fs.mkdirSync(folder);
  for(let i=1;i<=24;i++)fs.writeFileSync(path.join(folder,'frame_'+String(i).padStart(4,'0')+'.png'),png(Buffer.alloc(64*16*4),64,16));
  assert.throws(()=>P.artifacts(f.c,'exported',{dirs:{appCache:path.join(f.root,'cache')}}),/png-refused/);
  const g=setup(t),v=g.prepare();openSave(g.c,v);savedFrom(v);P.artifacts(g.c,'saved',{});reopenExport(g.c,v);outputs(v);
  fs.mkdirSync(path.join(g.root,'cache','native-export-staging'));fs.writeFileSync(path.join(g.root,'cache','native-export-staging','left'),'x');
  assert.throws(()=>P.artifacts(g.c,'exported',{dirs:{appCache:path.join(g.root,'cache')}}),/staging-retained/);
  const h=setup(t),u=h.prepare();openSave(h.c,u);savedFrom(u);P.artifacts(h.c,'saved',{});reopenExport(h.c,u);outputs(u);fs.appendFileSync(u.saved,' ');
  assert.throws(()=>P.artifacts(h.c,'exported',{dirs:{appCache:path.join(h.root,'cache')}}),/save-changed/);
});
test('a save that lost the link, the placement or gained a remap/offset is refused',t=>{
  for(const mutate of [v=>{v.assets[0].source.locator='/elsewhere/A.mov';},v=>{v.assetInventory.placements=[];},
    v=>{v.assetInventory.placements[0].offsetFrames=5;},v=>{v.assetInventory.placements[0].timeRemap={keys:[]};},v=>{v.assetInventory.placements[0].assetId='vb';},v=>{v.drawing.totalFrames=48;}]){
    const f=setup(t),w=f.prepare();openSave(f.c,w);savedFrom(w,mutate);assert.throws(()=>P.artifacts(f.c,'saved',{}),/save-mismatch/);
  }
});
function fakeServices(f,options={}){
  const events=[],saved=[],actions=options.actions||[{type:'inspect'},{type:'stop'}];let index=0,timerFn;
  const helper=options.helper||{async pin(){return {status:'pinned',pid:42,windowId:3,matchedAX:true};},
    async action(_w,a){events.push('helper:'+a.type);return {status:'ok',type:a.type,pid:42,windowId:3,matchedAX:true,nodes:[]};},async close(){events.push('helper-close');}};
  const controller={async start(){events.push('start');return {};},async status(){return {value:{app:{pid:42,dirs:{appCache:path.join(f.root,'cache')}},runtime:{childPid:42}}};},
    async cleanup(){events.push('app-cleanup');}};
  return {events,saved,services:{async transport(){if(options.transportFault)throw {code:'video-transport-refused'};},package(){return {bundle:path.join(f.root,'Nemo.app')};},
    journal(){events.push('journal');},workspace(){return f.prepare();},controller(){return controller;},validate(){},helper(){events.push('helper-start');return helper;},
    async next(){if(index>=actions.length)throw {code:'video-input-eof'};return actions[index++];},
    feedback(){},save(r){saved.push(JSON.parse(JSON.stringify(r)));},setTimer(fn){timerFn=fn;return 1;},clearTimer(){},async close(){events.push('input-close');}}};
}
test('transport refusal and wrong grants stop before journal or start; stop without artifacts stays partial',async t=>{
  const f=setup(t),s=fakeServices(f,{transportFault:true});const r=await D.runOperator(f.c,s.services);
  assert.equal(r.status,'partial');assert.equal(r.reason,'video-transport-refused');assert.deepEqual(s.events,['input-close']);
  await assert.rejects(D.runOperator({...f.c,workflow:'PNG_UI'},s.services));
  const g=setup(t),u=fakeServices(g),q=await D.runOperator(g.c,u.services);
  assert.equal(q.status,'partial');assert.equal(q.reason,'video-workflow-incomplete');assert.equal(q.installedAcceptance,'not-established');
  assert.ok(u.events.includes('app-cleanup')&&u.events.includes('helper-close'));
});
test('a complete witnessed protocol reports covered steps only and leaves installed acceptance to the owner',async t=>{
  const f=setup(t),root=f.c.reportRoot,w={project:path.join(root,'A.json'),saved:path.join(root,'Saved.json'),output:path.join(root,'output')};
  const token='11111111-1111-4111-8111-111111111111';let main='open',dialog=null,value=null,opened=0;const actions=[];
  const inspect=target=>actions.push({type:'inspect',...(target?{target,sheetToken:token}:{})});
  const press=(element,target)=>actions.push({type:'press',element:String(element),...(target?{target,sheetToken:token}:{})});
  function picker(p){actions.push({type:'pin-sheet'});inspect('sheet');actions.push({type:'set',target:'sheet',sheetToken:token,element:'0',value:p});inspect('sheet');press(1,'sheet');actions.push({type:'sheet-dismissed',sheetToken:token});}
  inspect();press(0);picker(w.project);actions.push({type:'capture',name:'opened'});
  inspect();press(0);picker(w.saved);actions.push({type:'artifacts',phase:'saved'});
  inspect();press(0);picker(w.saved);
  inspect();press(0);picker(w.output);actions.push({type:'artifacts',phase:'exported'},{type:'stop'});
  const s=fakeServices(f,{actions});
  s.services.helper=()=>({async pin(){return {status:'pinned',pid:42,windowId:3,matchedAX:true};},async close(){s.events.push('helper-close');},
    async action(_w,a){
      const r={status:'ok',type:a.type,pid:42,windowId:3,matchedAX:true},node=(p,control,role='AXButton')=>({path:p,control,role,press:true,enabled:true});
      if(a.type==='inspect'){Object.assign(r,complete);
        if(a.target==='sheet')r.nodes=[node('0',undefined,'AXTextField'),node('1',dialog==='save'?'Save':dialog==='export'?'Choose':'Open')];
        else r.nodes=main==='open'?[node('0','Open Project…')]:main==='save'?[node('0','Save As…')]:
          [node('0','Export'),node('1','PNG Sequence','AXPopUpButton'),node('2','Whole timeline','AXPopUpButton'),node('3','1x','AXPopUpButton')];}
      if(a.type==='capture'){r.capture=a.capture;fs.writeFileSync(path.join(root,a.capture),'window pixels');}
      if(a.type==='pin-sheet'||a.target==='sheet'){r.target='sheet';r.sheetToken=token;}
      if(a.type==='set')value=a.value;
      if(a.type==='press'&&a.target!=='sheet'){assert.equal(a.expectedRole,'AXButton');dialog=main;}
      if(a.type==='press'&&a.target==='sheet'){if(dialog==='save')savedFrom(w);if(dialog==='export')outputs(w);}
      if(a.type==='sheet-dismissed'){r.dismissed=true;if(dialog==='open')opened++;main=dialog==='open'?(opened===1?'save':'export'):dialog==='save'?'open':'done';dialog=null;}
      return r;
    }});
  const r=await D.runOperator(f.c,s.services);
  assert.equal(r.reason,undefined);assert.equal(r.status,'operator-complete');assert.equal(r.installedAcceptance,'not-established');
  assert.equal(r.ownerReview,'required');assert.equal(r.workflowCoverage,'partial');assert.deepEqual(r.blocked.map(x=>x.step),P.BLOCKED.map(x=>x.step));
  assert.deepEqual(r.artifacts.map(x=>x.phase),['saved','exported']);assert.equal(r.artifacts[1].frames,24);
  assert.equal(r.actions.find(x=>x.type==='capture').evidence.evidenceOnly,true);
  assert.equal(JSON.stringify(r).includes(f.root),false,'receipt carries no private paths');
  assert.equal(s.saved.at(-1).status,'operator-complete');assert.equal(s.events.indexOf('journal')<s.events.indexOf('start'),true);
});
test('driver module is manual-only and never named for automated desktop discovery',()=>{
  assert.equal(D.SIDECAR,'src-tauri/binaries/ffmpeg-aarch64-apple-darwin');
  assert.equal(/\.test\.cjs$/.test('video-installed-driver.cjs'),false);
  const src=fs.readFileSync(path.join(ROOT,'tests/desktop/video-installed-driver.cjs'),'utf8');
  assert.match(src,/process\.platform==='darwin'/);assert.equal(/npm (ci|install)|actions\/|workflow_dispatch/.test(src),false);
});
