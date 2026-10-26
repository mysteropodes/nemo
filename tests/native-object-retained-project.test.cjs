'use strict';
// Actual Project/Files/controller/tab renderer; native I/O and DOM are modeled.
// Native history/cleanup acceptance belongs to the separate real Rust controls.
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const Project=require('../src/js/adapters/native-object-project.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const Tabs=require('../src/js/adapters/native-object-inactive-project-tabs.js');
const TabsUI=require('../src/js/adapters/native-object-project-tabs-ui.js');
const Workflow=require('../src/js/application/native-object-save-as-workflow.js');
const Ack=require('../src/js/application/native-object-saved-acknowledgement.js');
const turn=()=>new Promise(setImmediate);
const clone=value=>JSON.parse(JSON.stringify(value));
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};}
class Element {
  constructor(tag){this.tagName=tag;this.children=[];this.events={};this.attributes={};this.dataset={};this.className='';
    this.classList={add:name=>this.className+=' '+name,toggle:(name,on)=>{
      const classes=new Set(this.className.split(/\s+/).filter(Boolean));on?classes.add(name):classes.delete(name);this.className=[...classes].join(' ');
    }};}
  set innerHTML(value){assert.equal(value,'');this.children=[];}
  appendChild(child){this.children.push(child);child.parentNode=this;return child;}
  setAttribute(name,value){this.attributes[name]=value;}
  getAttribute(name){return this.attributes[name];}
  addEventListener(name,handler){this.events[name]=handler;}
  dispatch(name,event={}){return this.events[name]&&this.events[name](event);}
}
function host(){
  let active=null,inactive=null,generation=0,serial=0,mode=null,previewHold=null;
  const calls=[],copies=[];
  function document(letter,w=320,h=180,fps=24){return {letter,documentId:'native-'+letter,revision:0,w,h,fps};}
  function info(doc=active){
    if(!doc)return {apiVersion:2,instanceId:'instance',available:false};
    return {apiVersion:2,instanceId:'instance',documentId:doc.documentId,lifecycleGeneration:generation,
      contentRevision:doc.revision,documentSnapshotId:doc.letter+'-snapshot-'+doc.revision,
      documentDigest:(doc.revision?(doc.letter==='B'?'d':'c'):(doc.letter==='B'?'b':'a')).repeat(64),
      byteCount:100+doc.revision,canvasW:doc.w,canvasH:doc.h,fps:doc.fps,totalFrames:8,
      layers:[{layerUid:doc.letter+'-0',name:doc.letter+' first'},{layerUid:doc.letter+'-1',name:doc.letter+' second'}],
      reference:doc.revision?{id:'reference',name:'image.png',visible:true,opacity:1,offsetFrames:0}:null,
      undoDepth:doc.revision,redoDepth:0};
  }
  function project(doc,request,token){return {...info(doc),preparationToken:token,preparationRequestId:request,
    origin:{documentId:active.documentId,documentSnapshotId:info().documentSnapshotId,contentRevision:active.revision}};}
  function inactiveReceipt(r,state,p=null){return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,
    documentId:r.documentId,contentRevision:r.expectedRevision,documentSnapshotId:r.documentSnapshotId,
    lifecycleGeneration:r.lifecycleGeneration,state,preparationToken:p?p.preparationToken:null,project:p,retryExecution:false};}
  async function invoke(command,args){
    const r=args?JSON.parse(args.requestJson):null;calls.push({command,r});
    if(command==='nemo_native_object_client_status')return {...info(),available:!!active};
    if(command==='nemo_native_object_bootstrap'){
      active=document('A',r.newProject?r.newProject.canvasW:320,r.newProject?r.newProject.canvasH:180,r.newProject?r.newProject.fps:24);
      generation++;inactive=null;
      return {apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:active.documentId,
        contentRevision:0,resourceCount:0,viewportAvailable:false};
    }
    if(command==='nemo_native_object_reference'&&r.action.kind==='inspect'){
      assert.equal(r.documentId,active.documentId);return {...info(),requestId:r.requestId};
    }
    if(command==='nemo_native_object_reference'){
      assert.equal(r.documentId,active.documentId);assert.equal(r.expectedRevision,active.revision);active.revision++;
      return {apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:active.documentId,
        contentRevision:active.revision,ok:true,result:{applied:true,historyEntriesAdded:1}};
    }
    if(command==='nemo_native_object_preview'){
      if(previewHold){const hold=previewHold;previewHold=null;hold.entered.resolve();await hold.release.promise;}
      assert.equal(r.documentId,active.documentId);assert.equal(r.contentRevision,active.revision);
      return {status:mode==='preview-after-switch'&&generation>1?'failed':'presented',workId:'work-'+(++serial),viewGeneration:serial};
    }
    if(command==='nemo_native_object_save_as'){
      copies.push(r);return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
        atRevision:r.atRevision,documentSnapshotId:r.documentSnapshotId,byteCount:info().byteCount,published:true,cleanupPending:false};
    }
    if(command!=='nemo_native_object_inactive_project')throw Error('Unexpected native I/O '+command);
    assert.equal(r.documentId,active.documentId);assert.equal(r.expectedRevision,active.revision);
    assert.equal(r.documentSnapshotId,info().documentSnapshotId);assert.equal(r.lifecycleGeneration,generation);
    if(r.action.kind==='inspect'){
      if(!inactive)return inactiveReceipt(r,'inactive-absent');
      const p=project(inactive.doc,inactive.request,inactive.token);
      if(mode==='changed-target')p.documentSnapshotId='changed-target';
      if(generation>1){
        if(mode==='outgoing-token')p.preparationToken='foreign-token';
        if(mode==='outgoing-origin-snapshot')p.origin.documentSnapshotId='foreign-snapshot';
        if(mode==='outgoing-origin-revision')p.origin.contentRevision++;
        if(mode==='outgoing-selection')p.layers=p.layers.slice(0,1);
        if(mode==='outgoing-frame')p.totalFrames=1;
        if(mode==='outgoing-duplicate-uid')p.layers.push({...p.layers[0]});
        if(mode==='outgoing-missing-uid')p.layers.push({name:'unused malformed layer'});
      }
      const receipt=inactiveReceipt(r,'inactive-retained',p);
      if(generation>1&&mode==='outgoing-wrapper-token')receipt.preparationToken='foreign-token';
      return receipt;
    }
    if(r.action.kind==='prepare'){
      assert.equal(inactive,null);inactive={doc:document('B',640,360,30),token:'prepared-B',request:r.requestId};
      return inactiveReceipt(r,'inactive-prepared',project(inactive.doc,inactive.request,inactive.token));
    }
    assert.equal(r.action.kind,'activate');
    if(mode==='refuse-before')throw {code:'stale_revision',details:{disposition:'notDispatched'}};
    assert.equal(r.action.preparationToken,inactive.token);assert.equal(r.action.targetDocumentId,inactive.doc.documentId);
    assert.equal(r.action.targetRevision,inactive.doc.revision);assert.equal(r.action.targetSnapshotId,info(inactive.doc).documentSnapshotId);
    const previous=active;active=inactive.doc;generation++;
    inactive={doc:previous,token:'parked-'+previous.letter+'-'+generation,request:r.requestId};
    const p=project(previous,inactive.request,inactive.token),now=info();
    if(mode==='lose-after')throw Error('controlled lost activation response after modeled effect');
    const value={apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:now.documentId,
      contentRevision:now.contentRevision,documentSnapshotId:now.documentSnapshotId,lifecycleGeneration:generation,
      inactiveProject:{documentId:p.documentId,contentRevision:p.contentRevision,documentSnapshotId:p.documentSnapshotId,
        preparationToken:p.preparationToken,preparationRequestId:p.preparationRequestId,documentDigest:p.documentDigest},
      viewportAvailable:false,retryExecution:false};
    if(mode==='wrong-receipt')value.inactiveProject.documentId='foreign';
    if(mode==='outgoing-digest')value.inactiveProject.documentDigest='e'.repeat(64);
    return value;
  }
  return {invoke,info,calls,copies,setMode:value=>{mode=value;},holdPreview(){
    previewHold={entered:deferred(),release:deferred()};return previewHold;}};
}
function fixture(afterCapture){
  const h=host(),list=new Element('div'),add=new Element('button'),notices=[],renders=[];
  const document={getElementById:id=>id==='project-tabs-list'?list:id==='project-tab-add'?add:null,
    createElement:tag=>new Element(tag),querySelector:()=>null};
  const root={document,SM:{t:value=>value},state:{activeSymbolId:null,currentFrame:0,totalFrames:8,layers:[],activeLayerIdx:0},
    _layerSel:[],_layerSelAnchor:-1,selectedPaths:[],userLayers:[],crypto:{randomUUID:(()=>{let i=0;return ()=> 'tabs-'+(++i);})()},
    __TAURI__:{core:{invoke:h.invoke}},SMProjectDocument:{baseName:path=>path.split('/').at(-1).replace(/\.json$/i,'')},
    NemoNativeObjectInactiveProjectTabs:{create(root,ports){
      if(afterCapture){const capture=ports.capture;ports.capture=async meta=>{
        const result=await capture(meta);afterCapture({session,current,root,h});return result;};}
      return Tabs.create(root,ports);}}};
  vm.runInNewContext(fs.readFileSync(require.resolve('../src/js/project-entry.js'),'utf8'),{window:root});
  let current={activeTabId:'tab-A',path:null,name:'Untitled',tabs:[{id:'tab-A',name:'Untitled',path:null,json:null}]};
  let files,tabs,lastAction,nextPath='/saved.json',tabSerial=0;
  const session=Project.create({invoke:h.invoke,frame:()=>root.state.currentFrame,
    mapping:info=>({cssBounds:{x:0,y:0,width:320,height:180},physicalExtent:{width:320,height:180},compositionExtent:{width:info.canvasW,height:info.canvasH},reportedDpr:1}),
    changed:info=>{if(Files.hasProjection(root))Files.refreshLayerTable(root,info);}});
  root.NemoNativeObjectProject={desktop:()=>session};
  function render(){
    if(!files)return;
    TabsUI.render(root,{read:()=>current,native:()=>true,dirty:t=>t.id===current.activeTabId?files.isDirty():!!t.dirty,
      switchTab:id=>{const target=current.tabs.find(t=>t.id===id);lastAction=tabs.activateClick(target.nativeInactiveProject);},
      closeTab:()=>{},renameTab:()=>{}});
    renders.push(list.children.map(el=>({id:el.dataset.tab,dirty:el.className.includes('dirty'),active:el.attributes['aria-selected']})));
  }
  function commit(path,name){current.path=path;current.name=name;const t=current.tabs.find(t=>t.id===current.activeTabId);t.path=path;t.name=name;}
  files=Files.create(session,{readUI:()=>({handle:current.activeTabId,generation:1}),reveal:async(info,view)=>Files.projectView(root,info,view),
    opened:path=>{commit(path,root.SMProjectDocument.baseName(path));return true;},
    created:name=>{commit(null,name);return true;},
    commit:(path)=>{commit(path,root.SMProjectDocument.baseName(path));return true;},
    pick:()=>typeof nextPath==='function'?nextPath():Promise.resolve(nextPath),repaint:render,failed:error=>notices.push(error),
    refused:error=>notices.push(error),notice:()=>{},confirmReplace:async()=>true},Workflow,Ack);
  tabs=TabsUI.bind(root,{read:()=>current,file:()=>files,dirty:()=>files.isDirty(),makeId:()=> 'tab-'+(++tabSerial),
    notice:text=>notices.push(text),render,
    activateTab:(id,path,name)=>{current.activeTabId=id;commit(path,name);}});
  async function click(id){const element=list.children.find(el=>el.dataset.tab===id);assert.ok(element);element.dispatch('click');return lastAction;}
  return {h,session,files,tabs,root,current,list,notices,renders,click,pick:value=>{nextPath=value;},
    async open(){assert.equal(await files.openPath('/A.json'),true);render();},
    async prepare(){assert.equal((await tabs.openPath('/B.json')).status,'prepared');return current.tabs.find(t=>t.nativeInactiveProject);},
    async edit(){await session.reference({kind:'attach',id:'reference',sourcePath:'/image.png'});},
    async context(frame,selection,active,anchor){root.state.currentFrame=frame;root._curFrame=frame;root._layerSel=selection;root.state.activeLayerIdx=active;root._layerSelAnchor=anchor;await session.presentPreview(frame);}};
}

test('ordinary rendered native tab clicks preserve dirty baselines, paths, nonzero revisions, frame and native layer context',async()=>{
  const f=fixture();await f.open();assert.equal(f.files.isDirty(),false);const b=await f.prepare();
  assert.ok(f.notices.includes('Project prepared. Select its tab to switch.'));
  await f.edit();await f.context(5,[1,0],1,0);assert.equal(f.files.isDirty(),true);
  assert.equal((await f.click(b.id)).status,'activated');assert.equal(f.current.path,'/B.json');assert.equal(f.files.isDirty(),false);
  assert.equal(f.current.activeTabId,b.id);assert.equal(f.h.info().documentId,'native-B');
  await f.edit();await f.context(3,[1],1,1);f.pick('/B-saved.json');assert.equal((await f.files.saveAs()).acknowledged,true);
  assert.equal(f.files.isDirty(),false);assert.equal(f.current.name,'B-saved');
  assert.equal((await f.click('tab-A')).status,'activated');assert.equal(f.session.info().contentRevision,1);
  assert.equal(f.files.isDirty(),true);assert.equal(f.current.path,'/A.json');assert.equal(f.current.name,'A');
  assert.equal(f.root.state.currentFrame,5);assert.deepEqual(f.root._layerSel,[1,0]);assert.equal(f.root.state.activeLayerIdx,1);assert.equal(f.root._layerSelAnchor,0);
  assert.deepEqual(f.h.calls.filter(c=>c.command==='nemo_native_object_preview'&&c.r.documentId==='native-A').slice(-2).map(c=>c.r.frame),[5,5]);
  f.pick('/A-saved.json');assert.equal((await f.files.saveAs()).acknowledged,true);assert.equal(f.files.isDirty(),false);
  assert.equal((await f.click(b.id)).status,'activated');assert.equal(f.files.isDirty(),false);
  assert.equal(f.current.path,'/B-saved.json');assert.equal(f.root.state.currentFrame,3);assert.deepEqual(f.root._layerSel,[1]);
  assert.equal(f.root.state.activeLayerIdx,1);assert.equal(f.root._layerSelAnchor,1);assert.equal(f.session.info().contentRevision,1);
  f.pick('/B-final.json');assert.equal((await f.files.saveAs()).acknowledged,true);
  assert.deepEqual(f.h.copies.map(r=>[r.documentId,r.atRevision,r.destinationPath]),[
    ['native-B',1,'/B-saved.json'],['native-A',1,'/A-saved.json'],['native-B',1,'/B-final.json']]);
  assert.equal(f.current.tabs.length,2);assert.equal(f.current.tabs.filter(t=>t.nativeInactiveProject).length,1);
  assert.ok(f.current.tabs.every(t=>t.json===null));assert.equal(JSON.stringify(f.h.calls).includes('documentJson'),false);
  assert.equal((await f.click('tab-A')).status,'activated');assert.equal(f.files.isDirty(),false);assert.equal(f.current.path,'/A-saved.json');
});

test('an unsaved native New retains its null saved baseline when parked and restored',async()=>{
  const f=fixture();assert.equal(await f.files.newProject({w:320,h:180,fps:24,name:'Unsaved A'}),true);
  assert.equal(f.files.isDirty(),true);const b=await f.prepare();
  assert.equal((await f.click(b.id)).status,'activated');assert.equal(f.files.isDirty(),false);
  assert.equal((await f.click('tab-A')).status,'activated');assert.equal(f.files.isDirty(),true);
  assert.equal(f.current.path,null);assert.equal(f.current.name,'Unsaved A');
});

test('an in-flight Save As blocks tab activation before native dispatch and keeps the retained tab',async()=>{
  const f=fixture();await f.open();const b=await f.prepare(),picker=deferred();f.pick(()=>picker.promise);
  const save=f.files.saveAs();await turn();
  assert.equal((await f.click(b.id)).status,'refused');assert.equal(f.current.activeTabId,'tab-A');assert.ok(f.tabs.held());
  assert.equal(f.h.calls.filter(c=>c.command==='nemo_native_object_inactive_project'&&c.r.action.kind==='activate').length,0);
  picker.resolve('/A-current.json');assert.equal((await save).acknowledged,true);
  assert.equal((await f.click(b.id)).status,'activated');
});

test('ordinary tab click waits the current preview queue and activates once after its confirmed presentation',async()=>{
  const f=fixture();await f.open();const b=await f.prepare(),hold=f.h.holdPreview();
  const preview=f.session.presentPreview(0);await hold.entered.promise;
  assert.equal(f.session.readPresentedObjectSession(),null);
  let finished=false;const switching=f.click(b.id).then(value=>{finished=true;return value;});await turn();
  assert.equal(finished,false);assert.equal(f.current.activeTabId,'tab-A');
  assert.equal(f.h.calls.filter(c=>c.r&&c.r.action&&c.r.action.kind==='activate').length,0);
  hold.release.resolve();await preview;assert.equal((await switching).status,'activated');
  assert.equal(f.current.activeTabId,b.id);assert.equal(f.h.info().documentId,'native-B');
  assert.equal(f.h.calls.filter(c=>c.r&&c.r.action&&c.r.action.kind==='activate').length,1);
});

for(const [name,change] of Object.entries({
  path:f=>{f.current.path='/changed.json';},name:f=>{f.current.name='Changed';},
  frame:f=>{f.root.state.currentFrame=1;},selection:f=>{f.root._layerSel=[1];},
  activeLayer:f=>{f.root.state.activeLayerIdx=1;},anchor:f=>{f.root._layerSelAnchor=1;},
  activeTab:(f,b)=>{f.current.activeTabId=b.id;},target:(f,b)=>{b.nativeInactiveProject={...b.nativeInactiveProject};},
  targetRow:(f,b)=>{f.current.tabs=f.current.tabs.map(t=>t===b?{...t}:t);},
  activeRow:f=>{f.current.tabs=f.current.tabs.filter(t=>t.id!=='tab-A');},
  session:f=>{f.root.NemoNativeObjectProject.desktop=()=>null;},
  newQueue:f=>f.session.presentPreview(0),revision:f=>f.session.reference({kind:'attach',id:'new',sourcePath:'/new.png'})
})){
  test('waiting tab capture rejects changed '+name+' before activation',async()=>{
    const f=fixture();await f.open();const b=await f.prepare(),hold=f.h.holdPreview(),diagnostics=[];
    f.root.NemoNativeObjectProjectDesktop={openDiagnostic:(root,event,reason)=>diagnostics.push([event,reason])};
    const preview=f.session.presentPreview(0);await hold.entered.promise;
    const switching=f.click(b.id);await turn();const work=change(f,b);hold.release.resolve();
    await preview;const result=await switching;await work;
    assert.equal(result.status,'indeterminate');assert.equal(f.h.info().documentId,'native-A');
    assert.equal(f.h.calls.filter(c=>c.r&&c.r.action&&c.r.action.kind==='activate').length,0);
    assert.equal(diagnostics.length,1);assert.equal(diagnostics[0][0],'tab-capture-refused');
    assert.ok(['ui-changed','owner-mismatch','capture-superseded'].includes(diagnostics[0][1]));
  });
}

for(const changed of ['preview','revision','path','frame','target']){
  test('synchronous tab admission rejects '+changed+' arriving after async capture',async()=>{
    let work,hold;
    const f=fixture(({session,current,root,h})=>{
      if(changed==='path')current.path='/later.json';
      else if(changed==='frame')root.state.currentFrame=1;
      else if(changed==='target')current.tabs.find(t=>t.nativeInactiveProject).nativeInactiveProject={};
      else {hold=h.holdPreview();work=changed==='preview'?session.presentPreview(0):session.reference({kind:'attach',id:'new',sourcePath:'/new.png'});}
    });
    await f.open();const b=await f.prepare();assert.equal((await f.click(b.id)).status,'refused');
    assert.equal(f.h.calls.filter(c=>c.r&&c.r.action&&c.r.action.kind==='activate').length,0);
    assert.equal(f.current.activeTabId,'tab-A');assert.equal(f.h.info().documentId,'native-A');
    if(hold){hold.release.resolve();await work;}
    assert.equal(f.files.isDirty(),changed==='revision');assert.equal(f.h.copies.length,0);
  });
}

for(const mode of ['lose-after','wrong-receipt','preview-after-switch']){
  test('uncertain '+mode+' retains both UI rows and fences saving without retry or fake rollback',async()=>{
    const f=fixture();await f.open();const b=await f.prepare();f.h.setMode(mode);
    assert.equal((await f.click(b.id)).status,'indeterminate');assert.equal(f.current.tabs.length,2);
    assert.equal(f.current.activeTabId,'tab-A');assert.equal(f.session.isIndeterminate(),true);
    assert.equal(f.h.calls.filter(c=>c.command==='nemo_native_object_inactive_project'&&c.r.action.kind==='activate').length,1);
    assert.equal((await f.files.saveAs()).acknowledged,false);assert.equal(f.h.copies.length,0);
    assert.equal((await f.tabs.reconcile()).status,'indeterminate');assert.equal(f.current.tabs.length,2);
  });
}

test('known pre-dispatch refusal restores the current session and permits a later fresh tab request',async()=>{
  const f=fixture();await f.open();const b=await f.prepare();f.h.setMode('refuse-before');
  assert.equal((await f.click(b.id)).status,'refused');assert.equal(f.session.isActive(),true);assert.equal(f.files.isDirty(),false);
  assert.equal(f.current.activeTabId,'tab-A');assert.equal(f.current.tabs.length,2);assert.equal((await f.tabs.reconcile()).status,'idle');
  f.h.setMode(null);assert.equal((await f.click(b.id)).status,'activated');
});

test('changed retained metadata refuses before the native activate route or row publication',async()=>{
  const f=fixture();await f.open();const b=await f.prepare();f.h.setMode('changed-target');
  assert.equal((await f.click(b.id)).status,'indeterminate');assert.equal(f.current.activeTabId,'tab-A');
  assert.equal(f.session.isActive(),true);assert.equal(f.current.tabs.length,2);
  assert.equal(f.h.calls.filter(c=>c.command==='nemo_native_object_inactive_project'&&c.r.action.kind==='activate').length,0);
});

for(const mode of ['outgoing-digest','outgoing-token','outgoing-wrapper-token','outgoing-origin-snapshot',
  'outgoing-origin-revision','outgoing-selection','outgoing-frame','outgoing-duplicate-uid','outgoing-missing-uid']){
  test('uncorrelated '+mode+' refuses tab publication and fences the changed native owner',async()=>{
    const f=fixture();await f.open();const b=await f.prepare();await f.context(5,[1],1,0);
    const rows=f.current.tabs.map(t=>[t.id,t.name,t.path,t.nativeInactiveProject]);f.h.setMode(mode);
    assert.equal((await f.click(b.id)).status,'indeterminate');assert.equal(f.session.isIndeterminate(),true);
    assert.equal(f.current.activeTabId,'tab-A');assert.equal(f.current.path,'/A.json');
    assert.deepEqual(f.current.tabs.map(t=>[t.id,t.name,t.path,t.nativeInactiveProject]),rows);
    assert.equal(f.h.calls.filter(c=>c.command==='nemo_native_object_inactive_project'&&c.r.action.kind==='activate').length,1);
    assert.equal((await f.files.saveAs()).acknowledged,false);assert.equal(f.h.copies.length,0);
    assert.equal((await f.tabs.reconcile()).status,'indeterminate');
  });
}
