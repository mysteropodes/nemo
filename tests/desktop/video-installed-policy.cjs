'use strict';
// Fixed T12 (#2232 acceptance point 8) installed video UI policy. It is test-only:
// never a product grant, never a document writer and never an installed verdict.
// The driver drives only ordinary UI; steps without a reachable ordinary UI are
// listed in BLOCKED with source evidence and are never simulated or relabeled.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),zlib=require('node:zlib');
const png=require('./png-ui-installed-policy.cjs');
const {crc32}=require('./reference-installed-policy.cjs');
const WORKFLOW='VIDEO_UI',ISSUE=2232;
// Protected codex/native-remediation head that contains the merged T12 PR #2307.
const SOURCE='2c65d228a272f4f3c7995b594d0d95b66802817e';
const W=32,H=16,TOTAL=24,FPS=24;
const SHA=/^[a-f0-9]{64}$/,HEAD=/^[a-f0-9]{40}$/,ID=/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
// Same lossless generators as src-tauri/src/native_media_host_tests.rs (PNG in MOV).
const SOURCES=Object.freeze({
  A:Object.freeze({file:'A.mov',frames:20,lavfi:"nullsrc=s=32x16:r=10:d=2,format=rgba,geq=r='10*N':g='8*X':b='16*Y':a='if(lt(X,16),255,8*X)'"}),
  B:Object.freeze({file:'B.mov',frames:12,lavfi:"nullsrc=s=32x16:r=12:d=1,format=rgba,geq=r=200:g='20*N':b='8*X':a='if(lt(Y,8),255,16*Y)'"}),
});
// The plan the installed UI can reach today: the opened project's source A, offset 0.
const PLAN=Object.freeze({source:'A',offsetFrames:0});
const STEPS=Object.freeze(['open-video-project','preview','replace-source','offset-or-time-remap','save-reopen','export-png-sequence','verify-pixels']);
const COVERED=Object.freeze(['open-video-project','save-reopen','export-png-sequence','verify-pixels']);
const BLOCKED=Object.freeze([
  // The ordinary UI route now exists (T12 point 8 JS slice); this driver's ordered witnesses do not drive it yet.
  Object.freeze({step:'import-video',finding:'installed-step-not-driven',
    evidence:['src/js/images.js:611-612 importVideo -> nativeMedia(null,"video") -> native-import-drop.js openMedia("video")',
      'src/js/adapters/native-import-desktop.js:233 providers:{image, video}; native-import-video.js openFile -> native-video-asset.js importPicked',
      'tests/desktop/video-installed-policy.cjs:189 witnessAction orders open/save/reopen/export only: no video-picker witness'],
    substitute:'Open Project… on a driver-authored native project that links staged source A (fixture open, not import)'}),
  Object.freeze({step:'preview',finding:'no-exact-installed-preview-readback',
    evidence:['tests/desktop/p38-owned-window.swift:282 helper actions; capture is a scaled window screenshot, not a pixel readback'],
    substitute:'captures are retained as evidence only; pixels are proven by export'}),
  Object.freeze({step:'replace-source',finding:'installed-step-not-driven',
    evidence:['src/js/adapters/native-time-remap-ui.js:285 the native Footage inspector enables Replace only for a declared native video route',
      'src/js/adapters/native-time-remap-ui.js:272 Replace -> NemoNativeImportController.video.replaceActive(placementId)',
      'tests/desktop/video-installed-policy.cjs:189 witnessAction has no replace witness; PLAN stays source A']}),
  Object.freeze({step:'offset',finding:'missing-required-local-consumer',
    evidence:['src/js/adapters/native-project-placement-contract.js:33-34 setOffset/shiftOffset have no UI caller']}),
  // The native Footage inspector now carries enable/key/value; the driver does not drive it yet.
  Object.freeze({step:'time-remap',finding:'installed-step-not-driven',
    evidence:['src/js/adapters/native-time-remap-ui.js:262 the native Footage inspector toggles Time Remap; its key/value controls call timeRemapKeyIntent (:168)',
      'tests/desktop/p38-owned-window.swift:282 has no inspector witness; tests/desktop/p38-owned-projection.swift:146-152 omits "Enable time remapping"']}),
]);
const actions=new Set(['inspect','press','set','select','raise','capture','check','pin-sheet','sheet-dismissed','go-to-folder','location-dismissed','stop','artifacts']);
const state=new WeakMap();
const {check,hash,readFile,identity,observeSheet}=png;
function sanitizeHelper(r,target){return png.sanitizeHelper(r,target);}
function texel(label,x,y,n){
  if(label==='A')return [10*n,8*x,16*y,x<16?255:8*x];
  if(label==='B')return [200,20*n,8*x,y<8?255:16*y];
  check(false,'video-oracle-refused');
}
// Timeline frame t draws source clamp(t - offset, 0, last) (native_media_host_tests.rs:428-440).
function sourceFrame(t,plan=PLAN){
  const count=SOURCES[plan.source]?.frames;check(Number.isInteger(t)&&t>=0&&Number.isInteger(count)&&Number.isInteger(plan.offsetFrames),'video-oracle-refused');
  return Math.max(0,Math.min(count-1,t-plan.offsetFrames));
}
// Exact alpha and opaque RGB; below 255 alpha the pinned compositor's one
// premultiply round trip allows RGB within 2 (native_media_host_tests.rs:62-87).
function matches(actual,want){
  const tolerance=want[3]===255?0:2;
  return actual[3]===want[3]&&[0,1,2].every(i=>Math.abs(actual[i]-want[i])<=tolerance);
}
function verifyFixtureFrames(label,raw){
  const spec=SOURCES[label];check(spec&&Buffer.isBuffer(raw)&&raw.length===spec.frames*W*H*4,'video-fixture-refused');
  for(let n=0;n<spec.frames;n++)for(let y=0;y<H;y++)for(let x=0;x<W;x++){
    const at=((n*H+y)*W+x)*4,want=texel(label,x,y,n);
    check(want.every((v,i)=>raw[at+i]===v),'video-fixture-refused');
  }
  return {label,frames:spec.frames,width:W,height:H};
}
function projectBytes(locatorA,locatorB){
  const timing={format:'nemo.native-layer-timing',formatVersion:1,family:'drawing',contextId:'scene-root',keys:[{frame:0}],samples:[]};
  const target={kind:'drawing',contextId:'scene-root',layerUid:'V'};
  const value={format:'nemo.native-project-document',formatVersion:2,
    project:{canvasW:W,canvasH:H,fps:FPS,canvasBg:'#00000000'},
    drawing:{format:'nemo.native-object-document',formatVersion:2,totalFrames:TOTAL,
      layers:[{layerUid:'V',visible:true,timing}],objects:[],
      compositionOrder:{format:'nemo.native-object-order-plan',formatVersion:1,documentFormatVersion:2,
        contexts:[{contextId:'scene-root',layers:[{layerUid:'V',naturalPosition:0,order:{}}],scopes:[]}],
        migration:'explicit-plan-required',referencePolicy:'unresolved'}},
    viewportReference:null,
    assets:[{id:'va',name:'take-a',kind:'video',source:{kind:'linked',locator:locatorA}},
      {id:'vb',name:'take-b',kind:'video',source:{kind:'linked',locator:locatorB}}],
    assetInventory:{assetIds:['va','vb'],placements:[{id:'p1',assetId:'va',target,offsetFrames:0}]}};
  return Buffer.from(JSON.stringify(value)+'\n');
}
function authorize(c){
  check(c&&c.workflow===WORKFLOW&&c.grant?.issue===ISSUE&&c.grant.confirmed===true&&typeof c.grant.id==='string'&&ID.test(c.grant.id),'new-video-grant-required');
  check(typeof c.taskId==='string'&&ID.test(c.taskId)&&c.sourceHead===SOURCE&&typeof c.driverHead==='string'&&HEAD.test(c.driverHead)&&
    typeof c.executableSha256==='string'&&SHA.test(c.executableSha256)&&typeof c.helperSha256==='string'&&SHA.test(c.helperSha256)&&
    Number.isInteger(c.timeoutMs)&&c.timeoutMs>=1000&&c.timeoutMs<=600000,'video-config-refused');
  for(const key of ['appRoot','app','buildProof','windowHelper','helperProof','reportRoot'])
    check(typeof c[key]==='string'&&path.isAbsolute(c[key])&&path.normalize(c[key])===c[key],'video-path-refused');
  check(c.fixtures&&typeof c.fixtures==='object'&&Object.keys(c.fixtures).sort().join()==='A,B','video-fixture-refused');
  for(const label of ['A','B']){const f=c.fixtures[label];
    check(f&&typeof f.path==='string'&&path.isAbsolute(f.path)&&path.normalize(f.path)===f.path&&typeof f.sha256==='string'&&SHA.test(f.sha256),'video-fixture-refused');}
  check(c.fixtures.A.sha256!==c.fixtures.B.sha256&&c.fixtures.A.path!==c.fixtures.B.path,'video-fixture-refused');
}
function movBytes(file,sha256){
  let bytes;try{bytes=readFile(file).bytes;}catch{check(false,'video-fixture-refused');}
  check(hash(bytes)===sha256&&bytes.length>=16&&bytes.toString('latin1',4,8)==='ftyp','video-fixture-refused');
  return bytes;
}
function verifyInputs(c){authorize(c);return {A:movBytes(c.fixtures.A.path,c.fixtures.A.sha256),B:movBytes(c.fixtures.B.path,c.fixtures.B.sha256)};}
function directory(file){const s=fs.lstatSync(file);check(s.isDirectory()&&!s.isSymbolicLink()&&fs.realpathSync(file)===file,'video-root-refused');return {file,dev:s.dev,ino:s.ino};}
function verifyDirectory(pin){const now=directory(pin.file);check(now.dev===pin.dev&&now.ino===pin.ino,'video-root-refused');}
// decode(file) -> straight RGBA rawvideo of every frame, from the frozen bundled sidecar.
function prepareWorkspace(c,decode){
  const inputs=verifyInputs(c),parent=directory(path.dirname(c.reportRoot));
  check(typeof decode==='function','video-fixture-refused');
  check(!fs.existsSync(c.reportRoot),'video-workspace-reused');fs.mkdirSync(c.reportRoot,{mode:0o700});
  const root=directory(c.reportRoot),file=name=>path.join(root.file,name);
  const videos={A:file(SOURCES.A.file),B:file(SOURCES.B.file)},project=file('A.json'),saved=file('Saved.json'),output=file('output');
  verifyDirectory(parent);
  for(const label of ['A','B'])fs.writeFileSync(videos[label],inputs[label],{mode:0o400,flag:'wx'});
  const fixtures={};
  for(const label of ['A','B']){fixtures[label]=verifyFixtureFrames(label,decode(videos[label]));fixtures[label].sha256=c.fixtures[label].sha256;}
  const projectData=projectBytes(videos.A,videos.B);fs.writeFileSync(project,projectData,{mode:0o400,flag:'wx'});fs.mkdirSync(output,{mode:0o700});
  const pins={A:readFile(videos.A),B:readFile(videos.B),project:readFile(project)};
  const s={parent,root,videos,project,projectSha256:hash(projectData),saved,output,outputPin:directory(output),pins,saved0:null,published:null,
    cursor:0,ui:{step:0,intent:null,value:null,confirmed:null,opened:false,saved:false,reopened:false,exported:false}};
  state.set(c,s);
  return {project,saved,output,projectSha256:s.projectSha256,fixtures};
}
function verifyWorkspace(c){
  verifyInputs(c);const s=state.get(c);check(s,'video-workspace-required');
  for(const p of [s.parent,s.root,s.outputPin,s.cache?.parent,s.cache?.pin].filter(Boolean))verifyDirectory(p);
  for(const [key,file,sha] of [['A',s.videos.A,c.fixtures.A.sha256],['B',s.videos.B,c.fixtures.B.sha256],['project',s.project,s.projectSha256]]){
    const now=readFile(file);check(now.dev===s.pins[key].dev&&now.ino===s.pins[key].ino&&hash(now.bytes)===sha,'video-fixture-refused');
  }
  return s;
}
function bindCache(c,manifest){
  const s=verifyWorkspace(c),file=manifest?.dirs?.appCache;
  check(typeof file==='string'&&path.isAbsolute(file)&&path.normalize(file)===file,'video-cache-refused');
  const parent=directory(path.dirname(file));
  if(!s.cache)s.cache={file,parent,pin:null};
  check(s.cache.file===file&&s.cache.parent.dev===parent.dev&&s.cache.parent.ino===parent.ino,'video-cache-changed');
  let present=false;try{fs.lstatSync(file);present=true;}catch(e){if(e.code!=='ENOENT')throw e;}
  if(present){const now=directory(file);if(s.cache.pin)check(now.dev===s.cache.pin.dev&&now.ino===s.cache.pin.ino,'video-cache-changed');else s.cache.pin=now;}
  else check(!s.cache.pin,'video-cache-changed');
  return file;
}
function exportOptions(last){
  const popups=(last?.nodes||[]).filter(n=>n.role==='AXPopUpButton'&&n.blocked===undefined&&n.enabled!==false);
  const one=(labels,allowed)=>{const hits=popups.filter(n=>labels.includes(n.control));return hits.length===1&&allowed.includes(hits[0].control);};
  return one(['SVG Sequence','PNG','PNG Sequence'],['PNG','PNG Sequence'])&&one(['Work area','Whole timeline'],['Whole timeline'])&&one(['1x'],['1x']);
}
function verifyAction(c,a,sheet,last){
  const s=verifyWorkspace(c);check(a&&actions.has(a.type),'video-action-refused');
  const target=a.target||'main';check(target==='main'||target==='sheet','video-action-refused');
  if(target==='sheet')check(sheet.token&&a.sheetToken===sheet.token,'video-sheet-refused');
  else if(sheet.token)check(['pin-sheet','sheet-dismissed','capture','stop'].includes(a.type),'video-sheet-refused');
  if(a.type==='sheet-dismissed')check(sheet.token&&a.sheetToken===sheet.token,'video-sheet-refused');
  if(sheet.locationToken)check(target==='sheet'&&a.locationToken===sheet.locationToken&&['inspect','press','set','location-dismissed'].includes(a.type),'video-location-refused');
  else check(a.locationToken===undefined&&a.type!=='location-dismissed','video-location-refused');
  if(a.type==='go-to-folder')check(target==='sheet'&&!sheet.locationAttempted&&Object.keys(a).every(k=>['type','target','sheetToken'].includes(k)),'video-location-refused');
  if(['press','set','select'].includes(a.type)){
    check(typeof a.element==='string'&&/^\d+(?:\.\d+)*$/.test(a.element)&&a.element.length<=128,'video-element-refused');
    const matches=last?.target===target?last.nodes?.filter(n=>n.path===a.element):[];
    const node=matches?.length===1?matches[0]:null;
    check(last?.complete===true&&['truncated','depthClipped','childrenUnreadable','foreignChildren'].every(k=>last[k]===false),'video-control-refused');
    check(node&&node.blocked===undefined&&node.enabled!==false,'video-control-refused');
    if(a.type==='press')check(png.controls.has(node.control)&&node.press===true&&['AXButton','AXMenuItem','AXPopUpButton'].includes(node.role),'video-control-refused');
    if(a.type==='press'&&['SVG Sequence','Work area'].includes(node.control))check(node.role==='AXPopUpButton','video-navigation-refused');
    if(a.type==='press'&&target==='main'&&['Export','Export…','Export PNG'].includes(node.control)&&
      last.nodes.some(n=>['SVG Sequence','PNG','PNG Sequence','Work area','Whole timeline','1x'].includes(n.control)))
      check(exportOptions(last),'video-export-options-refused');
    if(a.type==='select')check(target==='sheet'&&node.role==='AXRow'&&['A.json','Saved.json'].includes(node.control),'video-control-refused');
    if(a.type==='set')check(node.role==='AXTextField','video-control-refused');
  }
  if(a.type==='set')check(target==='sheet'&&typeof a.value==='string'&&[s.project,s.saved,s.output,s.root.file].includes(a.value),'video-path-refused');
  if(a.type==='inspect'){
    check(a.element===undefined||typeof a.element==='string'&&a.element.length<=128&&/^(?:\d+(?:\.\d+)*)?$/.test(a.element),'video-element-refused');
    if(a.childOffset!==undefined)check(Number.isInteger(a.childOffset)&&a.childOffset>=0&&a.childOffset<4096&&a.depth===1&&target==='main'&&
      (a.childOffset===0?a.pageToken===undefined:typeof a.pageToken==='string'&&/^[a-f0-9-]{36}$/.test(a.pageToken)),'video-page-refused');
    else check(a.pageToken===undefined,'video-page-refused');
    if(a.childOffset>0)check(last?.target==='main'&&last.pageToken===a.pageToken&&last.nextChildOffset===a.childOffset&&
      last.childrenComplete===false&&last.foreignChildren!==true&&last.nodes?.[0]?.path===(a.element||''),'video-page-refused');
    if(a.depth!==undefined)check(Number.isInteger(a.depth)&&a.depth>=1&&a.depth<=5,'video-inspect-budget');
    if(a.maxNodes!==undefined)check(Number.isInteger(a.maxNodes)&&a.maxNodes>=1&&a.maxNodes<=128,'video-inspect-budget');
  }
  if(a.type==='capture')check(['opened','saved','reopened','export-dialog','exported'].includes(a.name),'video-capture-refused');
  if(a.type==='stop')check(!sheet.token,'video-sheet-refused');
  if(a.type==='artifacts')check(!sheet.token&&['saved','exported'].includes(a.phase),'video-artifact-refused');
}
// Ordered witnesses: 0 open A.json, 1 Save As Saved.json, 2 reopen Saved.json, 3 export.
function witnessAction(c,a,last){
  const s=verifyWorkspace(c),ui=s.ui,target=a.target||'main',node=last?.nodes?.find(n=>n.path===a.element);
  if(a.type==='set')ui.value=a.value;
  if(a.type==='press'&&node){
    const label=node.control;
    if(target==='main'){
      if(['Open Project…','Open…','Open'].includes(label)){check([0,2].includes(ui.step),'video-ui-order');ui.intent=ui.step===0?'open':'reopen';}
      if(['Save As…','Save As','Save as…'].includes(label)){check(ui.step===1,'video-ui-order');ui.intent='save';}
      if(['Export','Export…','Export PNG'].includes(label)){check(ui.step===3,'video-ui-order');ui.intent=exportOptions(last)?'export':'export-options';}
    }else{
      if(label==='Open'&&ui.intent==='open'&&ui.value===s.project)ui.confirmed='opened';
      if(label==='Save'&&ui.intent==='save'&&ui.value===s.saved)ui.confirmed='saved';
      if(label==='Open'&&ui.intent==='reopen'&&ui.value===s.saved)ui.confirmed='reopened';
      if(['Choose','Select','Open'].includes(label)&&ui.intent==='export'&&ui.value===s.output)ui.confirmed='exported';
    }
  }
  if(a.type==='sheet-dismissed'&&ui.confirmed){const expected={opened:0,saved:1,reopened:2,exported:3};
    check(ui.step===expected[ui.confirmed],'video-ui-order');ui[ui.confirmed]=true;ui.step++;ui.confirmed=null;ui.intent=null;ui.value=null;}
}
function witnessSummary(c){const ui=verifyWorkspace(c).ui;return {step:ui.step,opened:ui.opened,saved:ui.saved,reopened:ui.reopened,exportPickerObserved:ui.exported};}
// Independent of the producer: the saved file must still carry the linked source,
// its one placement and no remap; unrelated serializer fields may vary.
function savedStructure(value,s){
  check(value?.format==='nemo.native-project-document'&&value.formatVersion===2&&value.project?.canvasW===W&&value.project.canvasH===H&&
    value.drawing?.totalFrames===TOTAL,'video-save-mismatch');
  const asset=(value.assets||[]).find(x=>x?.id==='va'),placements=value.assetInventory?.placements;
  check(asset&&asset.kind==='video'&&asset.source?.kind==='linked'&&asset.source.locator===s.videos.A,'video-save-mismatch');
  check(Array.isArray(placements)&&placements.length===1,'video-save-mismatch');
  const p=placements[0];
  check(p.assetId==='va'&&p.target?.kind==='drawing'&&p.target.contextId==='scene-root'&&p.target.layerUid==='V'&&
    (p.offsetFrames===undefined||p.offsetFrames===PLAN.offsetFrames)&&(p.timeRemap===undefined||p.timeRemap===null),'video-save-mismatch');
}
function decodePng(bytes){
  check(bytes.subarray(0,8).equals(Buffer.from('89504e470d0a1a0a','hex')),'video-png-refused');
  let offset=8,started=false,ended=false,header=false;const chunks=[];
  while(offset<bytes.length){
    check(offset+12<=bytes.length&&!ended,'video-png-refused');
    const size=bytes.readUInt32BE(offset),end=offset+12+size;check(end<=bytes.length,'video-png-refused');
    const type=bytes.toString('ascii',offset+4,offset+8),data=bytes.subarray(offset+8,end-4);
    check(crc32(bytes.subarray(offset+4,end-4))===bytes.readUInt32BE(end-4),'video-png-refused');
    if(type==='IHDR'){check(offset===8&&size===13&&data.readUInt32BE(0)===W&&data.readUInt32BE(4)===H&&
      data.subarray(8).equals(Buffer.from([8,6,0,0,0])),'video-png-refused');header=true;}
    else if(type==='IDAT'){check(header,'video-png-refused');started=true;chunks.push(data);}
    else if(type==='IEND'){check(size===0&&started&&end===bytes.length,'video-png-refused');ended=true;}
    else check(false,'video-png-refused');
    offset=end;
  }
  check(ended,'video-png-refused');
  const stride=W*4+1,raw=zlib.inflateSync(Buffer.concat(chunks),{maxOutputLength:stride*H});
  check(raw.length===stride*H,'video-png-refused');
  const out=Buffer.alloc(W*H*4);
  for(let y=0;y<H;y++){check(raw[y*stride]===0,'video-png-refused');raw.copy(out,y*W*4,y*stride+1,(y+1)*stride);}
  return out;
}
function verifyFrame(rgba,t,plan=PLAN){
  const n=sourceFrame(t,plan);let maxRgbDelta=0;
  for(let y=0;y<H;y++)for(let x=0;x<W;x++){
    const at=(y*W+x)*4,actual=[...rgba.subarray(at,at+4)],want=texel(plan.source,x,y,n);
    check(matches(actual,want),'video-pixel-mismatch');
    for(let i=0;i<3;i++)maxRgbDelta=Math.max(maxRgbDelta,Math.abs(actual[i]-want[i]));
  }
  return {timelineFrame:t,sourceFrame:n,pixels:W*H,maxRgbDelta};
}
function frameNames(){return Array.from({length:TOTAL},(_,i)=>'frame_'+String(i+1).padStart(4,'0')+'.png');}
function artifacts(c,phase,manifest){
  const s=verifyWorkspace(c),children=fs.readdirSync(s.output,{withFileTypes:true});
  const order=['saved','exported'];
  check(phase===order[s.cursor]&&s.ui.step===[2,4][s.cursor],'video-ui-unwitnessed');
  if(phase==='saved'){
    check(s.saved0===null&&children.length===0,'video-baseline-refused');
    const bytes=readFile(s.saved).bytes;let value;try{value=JSON.parse(bytes);}catch{check(false,'video-save-mismatch');}
    savedStructure(value,s);s.saved0=hash(bytes);s.cursor++;return {phase,savedSha256:s.saved0,projectSha256:s.projectSha256};
  }
  check(s.saved0&&children.length===1&&children[0].isDirectory()&&!children[0].isSymbolicLink()&&
    /^nemo-native-opacity-[0-9]{13}-[0-9]{1,6}$/.test(children[0].name),'video-output-refused');
  const folder=path.join(s.output,children[0].name);directory(folder);
  check(fs.readdirSync(folder).sort().join(',')===frameNames().join(','),'video-manifest-refused');
  const cache=bindCache(c,manifest);check(s.cache.pin,'video-cache-refused');
  const stage=path.join(cache,'native-export-staging');let entries=[];
  try{directory(stage);entries=fs.readdirSync(stage);}catch(e){if(e.code!=='ENOENT')throw e;}
  check(entries.length===0,'video-staging-retained');
  const files=frameNames().map((name,t)=>{const bytes=readFile(path.join(folder,name)).bytes;return {name,sha256:hash(bytes),...verifyFrame(decodePng(bytes),t)};});
  check(hash(readFile(s.saved).bytes)===s.saved0,'video-save-changed');
  s.published=files;s.cursor++;
  return {phase,plan:PLAN,frames:files.length,files,savedSha256:s.saved0,stagingEmpty:true};
}
module.exports={WORKFLOW,ISSUE,SOURCE,W,H,TOTAL,SOURCES,PLAN,STEPS,COVERED,BLOCKED,actions,check,hash,readFile,identity,observeSheet,
  sanitizeHelper,texel,sourceFrame,matches,verifyFixtureFrames,projectBytes,authorize,verifyInputs,prepareWorkspace,verifyWorkspace,
  bindCache,exportOptions,verifyAction,witnessAction,witnessSummary,savedStructure,decodePng,verifyFrame,frameNames,artifacts};
