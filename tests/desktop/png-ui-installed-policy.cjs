'use strict';
// Fixed #1959 UI acceptance policy; never a product grant or document writer.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const decode=require('./reference-installed-policy.cjs').pngPixels;
const SOURCE='6d6390cdd4fe692eff80ebec2a7ecfce82a7eb8c';
const FIXTURE='cfad6f7e5cb1709dcc0ecee84bc29e9703e8ee19442ba10323a2139f0edd6a66';
const SHA=/^[a-f0-9]{64}$/,HEAD=/^[a-f0-9]{40}$/,ID=/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const controls=new Set(['Open','Open Project…','Open…','Cancel','File','Edit','Save','Save As…','Save As','Save as…',
  'Undo','Redo','Export','Export…','Export PNG','PNG','PNG Sequence','SVG Sequence','Work area','Whole timeline','1x','Choose','Select',
  'Go','Go to the folder:','Go to folder:','Go to Folder','Go to the folder']);
const actions=new Set(['inspect','press','set','select','raise','capture','check','pin-sheet','sheet-dismissed','go-to-folder','location-dismissed','stop','artifacts']);
const state=new WeakMap();
function check(ok,code){if(!ok)throw Object.assign(Error(code),{code});}
function hash(bytes){return crypto.createHash('sha256').update(bytes).digest('hex');}
function authorize(c){
  check(c&&c.workflow==='PNG_UI'&&c.grant?.issue===1959&&c.grant.confirmed===true&&typeof c.grant.id==='string'&&ID.test(c.grant.id),'new-png-grant-required');
  check(typeof c.taskId==='string'&&c.taskId.length<=120&&ID.test(c.taskId)&&c.sourceHead===SOURCE&&HEAD.test(c.driverHead)&&SHA.test(c.executableSha256)&&
    SHA.test(c.helperSha256)&&Number.isInteger(c.timeoutMs)&&c.timeoutMs>=1000&&c.timeoutMs<=600000,'png-config-refused');
  for(const key of ['appRoot','app','buildProof','windowHelper','helperProof','fixture','reportRoot'])
    check(typeof c[key]==='string'&&path.isAbsolute(c[key])&&path.normalize(c[key])===c[key],'png-path-refused');
}
function readFile(file,maximum=4*1024*1024){
  check(path.isAbsolute(file)&&fs.realpathSync(file)===file,'png-file-refused');
  const fd=fs.openSync(file,fs.constants.O_RDONLY|fs.constants.O_NOFOLLOW|fs.constants.O_NONBLOCK);
  try{const s=fs.fstatSync(fd);check(s.isFile()&&s.size>0&&s.size<=maximum,'png-file-refused');
    const bytes=fs.readFileSync(fd),after=fs.lstatSync(file);
    check(after.isFile()&&!after.isSymbolicLink()&&after.dev===s.dev&&after.ino===s.ino&&bytes.length===s.size,'png-file-refused');
    return {bytes,dev:s.dev,ino:s.ino};
  }finally{fs.closeSync(fd);}
}
function verifyInputs(c){authorize(c);const input=readFile(c.fixture,1048576);
  check(input.bytes.length===2103&&hash(input.bytes)===FIXTURE,'png-fixture-refused');return input;}
function directory(file){const s=fs.lstatSync(file);check(s.isDirectory()&&!s.isSymbolicLink()&&fs.realpathSync(file)===file,'png-root-refused');
  return {file,dev:s.dev,ino:s.ino};}
function verifyDirectory(pin){const now=directory(pin.file);check(now.dev===pin.dev&&now.ino===pin.ino,'png-root-refused');}
function prepareWorkspace(c){
  const input=verifyInputs(c),parent=directory(path.dirname(c.reportRoot));
  check(!fs.existsSync(c.reportRoot),'png-workspace-reused');fs.mkdirSync(c.reportRoot,{mode:0o700});
  const root=directory(c.reportRoot),fixture=path.join(root.file,'A.json'),saved=path.join(root.file,'Saved.json'),after=path.join(root.file,'SavedAfter.json'),output=path.join(root.file,'output');
  verifyDirectory(parent);fs.writeFileSync(fixture,input.bytes,{mode:0o600,flag:'wx'});fs.mkdirSync(output,{mode:0o700});
  const s={parent,root,fixture,saved,after,output,outputPin:directory(output),fixturePin:readFile(fixture),before:null,published:null,
    artifactCursor:0,ui:{step:0,intent:null,value:null,confirmed:null,opened:false,before:false,after:false,cancelled:false,exported:false}};state.set(c,s);
  return {fixture,saved,after,output};
}
function verifyWorkspace(c){verifyInputs(c);const s=state.get(c);check(s,'png-workspace-required');
  for(const p of [s.parent,s.root,s.outputPin,s.cache?.parent,s.cache?.pin].filter(Boolean))verifyDirectory(p);
  const file=readFile(s.fixture);check(file.dev===s.fixturePin.dev&&file.ino===s.fixturePin.ino&&hash(file.bytes)===FIXTURE,'png-fixture-refused');return s;}
function bindCache(c,manifest){
  const s=verifyWorkspace(c),file=manifest?.dirs?.appCache;
  check(typeof file==='string'&&path.isAbsolute(file)&&path.normalize(file)===file,'png-cache-refused');
  const parent=directory(path.dirname(file));
  if(!s.cache)s.cache={file,parent,pin:null};
  check(s.cache.file===file&&s.cache.parent.dev===parent.dev&&s.cache.parent.ino===parent.ino,'png-cache-changed');
  let present=false;try{fs.lstatSync(file);present=true;}catch(e){if(e.code!=='ENOENT')throw e;}
  if(present){const now=directory(file);if(s.cache.pin)check(now.dev===s.cache.pin.dev&&now.ino===s.cache.pin.ino,'png-cache-changed');else s.cache.pin=now;}
  else check(!s.cache.pin,'png-cache-changed');
  return file;
}
function identity(reply,pid,windowId){check(reply&&reply.pid===pid&&reply.windowId===windowId&&reply.matchedAX===true,'png-window-refused');}
function exportOptions(last){
  const popups=(last?.nodes||[]).filter(n=>n.role==='AXPopUpButton'&&n.blocked===undefined&&n.enabled!==false);
  const one=(labels,allowed)=>{const hits=popups.filter(n=>labels.includes(n.control));return hits.length===1&&allowed.includes(hits[0].control);};
  return one(['SVG Sequence','PNG','PNG Sequence'],['PNG','PNG Sequence'])&&one(['Work area','Whole timeline'],['Whole timeline'])&&one(['1x'],['1x']);
}
function verifyAction(c,a,sheet,last){
  const s=verifyWorkspace(c);check(a&&actions.has(a.type),'png-action-refused');
  const target=a.target||'main';check(target==='main'||target==='sheet','png-action-refused');
  if(target==='sheet')check(sheet.token&&a.sheetToken===sheet.token,'png-sheet-refused');
  else if(sheet.token)check(['pin-sheet','sheet-dismissed','capture','stop'].includes(a.type),'png-sheet-refused');
  if(a.type==='sheet-dismissed')check(sheet.token&&a.sheetToken===sheet.token,'png-sheet-refused');
  if(sheet.locationToken)check(target==='sheet'&&a.locationToken===sheet.locationToken&&['inspect','press','set','location-dismissed'].includes(a.type),'png-location-refused');
  else check(a.locationToken===undefined&&a.type!=='location-dismissed','png-location-refused');
  if(a.type==='go-to-folder')check(target==='sheet'&&!sheet.locationAttempted&&Object.keys(a).every(k=>['type','target','sheetToken'].includes(k)),'png-location-refused');
  if(['press','set','select'].includes(a.type)){
    check(typeof a.element==='string'&&/^\d+(?:\.\d+)*$/.test(a.element)&&a.element.length<=128,'png-element-refused');
    const matches=last?.target===target?last.nodes?.filter(n=>n.path===a.element):[];
    const node=matches?.length===1?matches[0]:null;
    // Pages/clipped trees discover candidates only. Reinspect the selected root
    // completely before input; the helper then resolves ownership and label anew.
    check(last?.complete===true&&['truncated','depthClipped','childrenUnreadable','foreignChildren'].every(k=>last[k]===false),
      'png-control-refused');
    check(node&&node.blocked===undefined&&node.enabled!==false,'png-control-refused');
    if(a.type==='press')check(controls.has(node.control)&&node.press===true&&['AXButton','AXMenuItem','AXPopUpButton'].includes(node.role),'png-control-refused');
    if(a.type==='press'&&['SVG Sequence','Work area'].includes(node.control))check(node.role==='AXPopUpButton','png-navigation-refused');
    if(a.type==='press'&&target==='main'&&['Export','Export…','Export PNG'].includes(node.control)&&
      last.nodes.some(n=>['SVG Sequence','PNG','PNG Sequence','Work area','Whole timeline','1x'].includes(n.control)))
      check(exportOptions(last),'png-export-options-refused');
    if(a.type==='select')check(target==='sheet'&&node.role==='AXRow'&&['A.json','Saved.json'].includes(node.control),'png-control-refused');
    if(a.type==='set')check(node.role==='AXTextField','png-control-refused');
  }
  if(a.type==='set')check(target==='sheet'&&typeof a.value==='string'&&[s.fixture,s.saved,s.after,s.output,s.root.file].includes(a.value),'png-path-refused');
  if(a.type==='inspect'){check(a.element===undefined||typeof a.element==='string'&&a.element.length<=128&&/^(?:\d+(?:\.\d+)*)?$/.test(a.element),'png-element-refused');
    if(a.childOffset!==undefined)check(Number.isInteger(a.childOffset)&&a.childOffset>=0&&a.childOffset<4096&&a.depth===1&&target==='main'&&
      (a.childOffset===0?a.pageToken===undefined:typeof a.pageToken==='string'&&/^[a-f0-9-]{36}$/.test(a.pageToken)),'png-page-refused');
    else check(a.pageToken===undefined,'png-page-refused');
    if(a.childOffset===0&&a.element)check(last?.target==='main'&&last.nodes?.some(n=>n.path===a.element&&n.blocked===undefined),'png-page-refused');
    if(a.childOffset>0)check(last?.target==='main'&&last.pageToken===a.pageToken&&last.nextChildOffset===a.childOffset&&
      last.childrenComplete===false&&last.foreignChildren!==true&&last.nodes?.[0]?.path===(a.element||''),'png-page-refused');
    if(a.depth!==undefined)check(Number.isInteger(a.depth)&&a.depth>=1&&a.depth<=5,'png-inspect-budget');
    if(a.maxNodes!==undefined)check(Number.isInteger(a.maxNodes)&&a.maxNodes>=1&&a.maxNodes<=128,'png-inspect-budget');}
  if(a.type==='capture')check(['before-open','opened','export-dialog','exported','saved'].includes(a.name),'png-capture-refused');
  if(a.type==='stop')check(!sheet.token,'png-sheet-refused');
  if(a.type==='artifacts')check(!sheet.token&&['baseline','exported','cancelled','after-save'].includes(a.phase),'png-artifact-refused');
}
function sanitizeHelper(r,target='main'){
  const result={status:['ok','pinned','refused'].includes(r.status)?r.status:'refused',pid:r.pid,windowId:r.windowId,matchedAX:r.matchedAX,type:actions.has(r.type)?r.type:undefined};
  for(const key of ['sheetToken','locationToken'])if(typeof r[key]==='string'&&/^[a-f0-9-]{36}$/.test(r[key]))result[key]=r[key];
  if(r.pageToken!==undefined){check(typeof r.pageToken==='string'&&/^[a-f0-9-]{36}$/.test(r.pageToken),'png-page-refused');result.pageToken=r.pageToken;}
  for(const key of ['childOffset','nextChildOffset','childCount'])if(r[key]!==undefined){check(Number.isInteger(r[key])&&r[key]>=0&&r[key]<4096,'png-page-refused');result[key]=r[key];}
  for(const key of ['truncated','depthClipped','childrenUnreadable','foreignChildren','complete','childrenComplete'])
    if(r[key]!==undefined){check(typeof r[key]==='boolean','png-page-refused');result[key]=r[key];}
  if(result.pageToken!==undefined)check(Number.isInteger(result.childOffset)&&Number.isInteger(result.nextChildOffset)&&Number.isInteger(result.childCount)&&
    result.childOffset<=result.nextChildOffset&&result.nextChildOffset<=result.childCount&&
    (result.nextChildOffset>result.childOffset||result.childOffset===result.childCount),'png-page-refused');
  else check(['childOffset','nextChildOffset','childCount','childrenComplete'].every(k=>r[k]===undefined),'png-page-refused');
  for(const key of ['dismissed','locationDismissed','commandPosted','locationReady'])if(typeof r[key]==='boolean')result[key]=r[key];
  if(r.target==='sheet')result.target='sheet';
  const roles=new Set(['AXWindow','AXSheet','AXButton','AXTextField','AXStaticText','AXGroup','AXWebArea','AXMenu','AXMenuItem','AXRow','AXPopUpButton','AXCheckBox']);
  if(r.type==='inspect'||r.nodes!==undefined){
    check(Array.isArray(r.nodes)&&r.nodes.length<=128&&r.nodes.every(n=>n&&typeof n==='object'&&!Array.isArray(n)&&
      typeof n.path==='string'&&n.path.length<=128&&/^(?:\d+(?:\.\d+)*)?$/.test(n.path)),'png-projection-refused');
    result.nodes=r.nodes.map(n=>{
    const p={path:typeof n.path==='string'&&/^(?:\d+(?:\.\d+)*)?$/.test(n.path)?n.path:'',role:roles.has(n.role)?n.role:'other'};
    if(controls.has(n.control)||target==='sheet'&&n.role==='AXRow'&&['A.json','Saved.json'].includes(n.control))p.control=n.control;
    if(['child-owner','children-unavailable'].includes(n.blocked))p.blocked=n.blocked;
    for(const key of ['enabled','press','selected'])if(typeof n[key]==='boolean')p[key]=n[key];return p;
    });
  }
  if(typeof r.capture==='string'&&/^capture-[1-9][0-9]{0,2}\.png$/.test(r.capture))result.capture=r.capture;
  return result;
}
function witnessSummary(c){const ui=verifyWorkspace(c).ui;return {step:ui.step,opened:ui.opened,beforeSaved:ui.before,
  cancelObserved:ui.cancelled,exportPickerObserved:ui.exported,afterSaved:ui.after};}
function witnessAction(c,a,last){
  const s=verifyWorkspace(c),ui=s.ui,target=a.target||'main',node=last?.nodes?.find(n=>n.path===a.element);
  if(a.type==='set')ui.value=a.value;
  if(a.type==='press'&&node){
    const label=node.control;
    if(target==='main'){
      if(['Open Project…','Open…','Open'].includes(label)){check(ui.step===0,'png-ui-order');ui.intent='open';}
      if(['Save As…','Save As','Save as…'].includes(label)){check([1,4].includes(ui.step),'png-ui-order');ui.intent='save';}
      if(['Export','Export…','Export PNG'].includes(label)){
        check([2,3].includes(ui.step),'png-ui-order');
        ui.intent=exportOptions(last)?'export':'export-options';
      }
      if(label==='Cancel'&&ui.intent==='export-options'){check(ui.step===2,'png-ui-order');ui.cancelled=true;ui.step=3;ui.intent=null;}
    }else{
      if(label==='Cancel'&&ui.intent==='export'){ui.confirmed='cancelled';}
      if(label==='Open'&&ui.intent==='open'&&ui.value===s.fixture)ui.confirmed='opened';
      if(label==='Save'&&ui.intent==='save')ui.confirmed=ui.value===s.saved?'before':ui.value===s.after?'after':null;
      if(['Choose','Select','Open'].includes(label)&&ui.intent==='export'&&ui.value===s.output)ui.confirmed='exported';
    }
  }
  if(a.type==='sheet-dismissed'&&ui.confirmed){const expected={opened:0,before:1,cancelled:2,exported:3,after:4};
    check(ui.step===expected[ui.confirmed],'png-ui-order');ui[ui.confirmed]=true;ui.step++;ui.confirmed=null;ui.intent=null;ui.value=null;}
}
function observeSheet(a,sheet,r){
  if(a.type==='pin-sheet'){check(r.target==='sheet'&&/^[a-f0-9-]{36}$/.test(r.sheetToken)&&(!sheet.token||sheet.token===r.sheetToken),'png-sheet-refused');
    if(!sheet.token)sheet.locationAttempted=false;sheet.token=r.sheetToken;}
  else if(a.type==='sheet-dismissed'){check(r.dismissed===true,'png-sheet-refused');sheet.token=null;sheet.locationToken=null;}
  else if((a.target||'main')==='sheet')check(r.target==='sheet'&&r.sheetToken===sheet.token,'png-sheet-refused');
  if(a.type==='go-to-folder'){check(r.commandPosted===true&&r.locationReady===true&&/^[a-f0-9-]{36}$/.test(r.locationToken),'png-location-refused');sheet.locationToken=r.locationToken;}
  if(a.type==='location-dismissed'){check(r.locationDismissed===true,'png-location-refused');sheet.locationToken=null;}
}
function artifacts(c,phase,manifest){
  const s=verifyWorkspace(c),children=fs.readdirSync(s.output,{withFileTypes:true});
  const order=['baseline','cancelled','exported','after-save'];
  check(phase===order[s.artifactCursor]&&s.ui.step===[2,3,4,5][s.artifactCursor],'png-ui-unwitnessed');
  if(phase==='baseline'){check(s.before===null&&children.length===0,'png-baseline-refused');s.before=hash(readFile(s.saved).bytes);s.artifactCursor++;return {phase,savedSha256:s.before};}
  if(phase==='cancelled'){check(children.length===0,'png-cancelled-output');s.artifactCursor++;return {phase,empty:true};}
  check(s.before&&children.length===1&&children[0].isDirectory()&&!children[0].isSymbolicLink()&&/^nemo-native-opacity-[0-9]{13}-[0-9]{1,6}$/.test(children[0].name),'png-output-refused');
  const folder=path.join(s.output,children[0].name);directory(folder);
  check(fs.readdirSync(folder).sort().join(',')==='frame_0001.png,frame_0002.png','png-manifest-refused');
  const cache=bindCache(c,manifest);check(s.cache.pin,'png-cache-refused');
  const stage=path.join(cache,'native-export-staging');let entries=[];
  try{directory(stage);entries=fs.readdirSync(stage);}catch(e){if(e.code!=='ENOENT')throw e;}
  check(entries.length===0,'png-staging-retained');
  const files=['frame_0001.png','frame_0002.png'].map((name,i)=>{const bytes=readFile(path.join(folder,name)).bytes;
    return {name,sha256:hash(bytes),pixels:decode(bytes,{width:320,height:180,pixels:[{x:40,y:40,rgba:i?[0,255,0,255]:[0,0,255,255]},{x:8,y:8,rgba:[255,255,255,255]}]})};});
  const current=hash(readFile(phase==='after-save'?s.after:s.saved).bytes);check(current===s.before,'png-save-changed');
  if(s.published)check(JSON.stringify(files)===JSON.stringify(s.published),'png-output-changed');else s.published=files;
  s.artifactCursor++;return {phase,files,savedSha256:current,stagingEmpty:true};
}
module.exports={SOURCE,FIXTURE,controls,actions,check,hash,authorize,readFile,verifyInputs,prepareWorkspace,verifyWorkspace,bindCache,identity,verifyAction,witnessAction,witnessSummary,sanitizeHelper,observeSheet,artifacts};
