'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
const Contract=require('../src/js/adapters/native-composition-contract.js');
const clone=x=>JSON.parse(JSON.stringify(x));
function node(tag='div') {
  const listeners=new Map(),attributes=new Map();let text='';
  const el={tagName:tag.toUpperCase(),children:[],className:'',value:'',style:{setProperty(k,v){this[k]=v;}},
    classList:{toggle(){}},addEventListener(k,f,options){if(!listeners.has(k))listeners.set(k,[]);listeners.get(k).push({f,once:!!(options&&options.once)});},
    removeEventListener(k,f){listeners.set(k,(listeners.get(k)||[]).filter(item=>item.f!==f));},
    fire(k,e={}){let stopped=false;const event={button:0,clientX:0,clientY:0,target:el,defaultPrevented:false,
      preventDefault(){this.defaultPrevented=true;},stopPropagation(){stopped=true;},...e};
      for(let at=el;at;at=k==='blur'?null:at.parentNode){event.currentTarget=at;
        for(const item of [...(at._listeners.get(k)||[])]){item.f.call(at,event);if(item.once)at.removeEventListener(k,item.f);}
        if(stopped)break;}return event;},
    appendChild(child){child.parent=child.parentNode=this;this.children.push(child);return child;},
    remove(){if(this.parentNode)this.parentNode.children=this.parentNode.children.filter(x=>x!==this);this.parent=this.parentNode=null;},
    setAttribute(k,v){attributes.set(k,String(v));},getAttribute:k=>attributes.get(k)??null,
    focus(){if(this.ownerDocument)this.ownerDocument.activeElement=this;},
    select(){this.selectionStart=0;this.selectionEnd=String(this.value).length;this.selectCalls=(this.selectCalls||0)+1;},
    querySelector(selector){return this.querySelectorAll(selector)[0]||null;},
    querySelectorAll(selector){const found=[];function visit(parent){for(const child of parent.children){
      if(selector[0]==='#'?child.id===selector.slice(1):selector[0]==='.'?child.className.split(/\s+/).includes(selector.slice(1)):child.tagName.toLowerCase()===selector)found.push(child);
      visit(child);}}visit(this);return found;},
    closest(){return null;},getBoundingClientRect(){return {left:0};},_listeners:listeners};
  Object.defineProperty(el,'textContent',{get(){return text+this.children.map(child=>child.textContent).join('');},
    set(value){text=String(value);for(const child of this.children)child.parent=child.parentNode=null;this.children=[];}});
  return el;
}
function fixture() {
  const elements=Object.fromEntries(['bars-row','frame-hdr','fg-wrap','frame-grid','btn-bpm'].map(id=>[id,node()]));
  const created=[],windowEvents=node('window'),body=node('body');
  const document=Object.assign(node('document'),{readyState:'loading',body,
    getElementById:id=>elements[id]||body.querySelector('#'+id),
    createElement:tag=>{const el=node(tag);el.ownerDocument=document;created.push(el);return el;},
    createDocumentFragment:()=>node('fragment'),querySelector:()=>null,
    querySelectorAll:selector=>selector==='.tl-marker'?elements['bars-row'].children.filter(x=>x.className.startsWith('tl-marker')):[]});
  body.parentNode=document;
  const persisted=Object.freeze([{frame:39,name:'legacy'}]),layerMarkers=Object.freeze([{frame:20}]);
  const state={markers:persisted,layers:[{markers:layerMarkers}],totalFrames:40,currentFrame:3,fps:24};
  let info={instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'s0',
    canvasW:320,canvasH:180,fps:24,totalFrames:40,layers:[{layerUid:'drawing',visible:true}]},
    revision=0,ticket=1,pending=0,serial=0,active=true,tail=Promise.resolve(),block=null;
  let metadata={markers:[{frame:2.5,name:'First',color:''},{frame:8,name:'Second'}]};
  const calls=[],notices=[],prompts=[],menus=[],jumps=[];
  const shown=pin=>({...pin,presentation:{...pin,owner:'native',status:'presented'}});
  let controller,browserRoot;
  async function dispatch(q) {
    calls.push(clone(q));root.NemoNativeCompositionContract.payload(q.operation,q.payload);
    if(block){const held=block;block=null;held.entered();await held.wait;}
    const response={apiVersion:2,requestId:q.requestId,instanceId:info.instanceId,documentId:info.documentId,contentRevision:revision,ok:true};
    if(q.operation===Contract.EDIT) {
      assert.equal(q.expectedRevision,revision);const a=q.payload.action,before=JSON.stringify(metadata);
      if(a.kind==='setBpm')for(const k of ['bpm','bpmOffset','bpmShow'])if(Object.hasOwn(a,k))metadata[k]=a[k];
      if(a.kind==='addMarker'){metadata.markers.push({frame:a.frame,name:a.name,color:a.color});metadata.markers.sort((x,y)=>x.frame-y.frame);}
      if(a.kind==='updateMarker'){Object.assign(metadata.markers[a.ordinal-1],Object.fromEntries(Object.entries(a).filter(([k])=>!['kind','ordinal'].includes(k))));metadata.markers.sort((x,y)=>x.frame-y.frame);}
      if(a.kind==='removeMarker')metadata.markers.splice(a.ordinal-1,1);
      if(a.kind==='clearMarkers')metadata.markers=[];
      const applied=before!==JSON.stringify(metadata);if(applied)revision++;
      response.contentRevision=revision;response.result={applied,historyEntriesAdded:Number(applied)};
    } else {
      assert.equal(q.payload.atRevision,revision);assert.equal(q.payload.documentSnapshotId,'s'+revision);
      const s=q.payload.selector;
      const result=s.kind==='settings'?{markersPresent:true,markerCount:metadata.markers.length,
        effectiveBpm:metadata.bpm??120,effectiveBpmOffset:metadata.bpmOffset??0,effectiveBpmShow:metadata.bpmShow??false,
        ...Object.fromEntries(['bpm','bpmOffset','bpmShow'].filter(k=>Object.hasOwn(metadata,k)).map(k=>[k,metadata[k]]))}:
        {kind:'markerPage',markersPresent:true,markerCount:metadata.markers.length,cursor:s.cursor,
          items:metadata.markers.slice(s.cursor).map((m,i)=>({ordinal:s.cursor+i+1,...m})),nextCursor:null};
      response.result={instanceId:info.instanceId,documentId:info.documentId,atRevision:revision,
        documentSnapshotId:'s'+revision,contextId:'scene-root',result};
    }
    // Tauri JSON is deserialized into the consumer browser realm.
    return vm.runInContext('JSON.parse('+JSON.stringify(JSON.stringify(response))+')',context);
  }
  const session={info:()=>info,isActive:()=>active,requestId:()=> 'q'+(++serial),readSubscriptionGeneration:()=>ticket,
    readPresentedObjectSession:()=>pending===0&&active?shown(info):null,
    runResourceCommand(action){pending++;const work=tail.then(async()=>{const origin=info;
      return action({dispatch,readPresentedObjectSession:()=>active&&pending===1&&info===origin?shown(origin):null,
        async reconcile(r){assert.equal(r,revision);info={...info,contentRevision:revision,documentSnapshotId:'s'+revision};
          controller.willProject(info);assert.equal(root.NemoNativeObjectProjectFile.refreshLayerTable(browserRoot,info),true);
          controller.presented(info);return info;}});
    }).finally(()=>{pending--;controller.presented(info);});tail=work.catch(()=>{});return work;}};
  const promptCalls=[],diagnostics=[];
  const root={document,state,FC:10,__TAURI__:{core:{}},SM:{t:x=>x},
    NemoNativeObjectProject:{desktop:()=>session},
    NemoNativeObjectProjectDesktop:{openDiagnostic(value,event,code){assert.strictEqual(value,vm.runInContext('window',context));diagnostics.push([event,code]);}},
    showToast:x=>notices.push(x),showContextMenu:(x,y,items)=>menus.push(items),prompt:(...args)=>{promptCalls.push(args);return prompts.shift();},
    pushUndo(){throw new Error('legacy JS history must not run on native desktop');},renderTimeline(){},
    goToFrame:f=>jumps.push(f),addEventListener:windowEvents.addEventListener,removeEventListener:windowEvents.removeEventListener};
  root.window=root;root.globalThis=root;
  const context=vm.createContext(root);
  // These actual production scripts use their original HTML registration order.
  const wanted=['adapters/native-object-layer-splice.js','adapters/native-object-project-file.js','markers.js','bpm-grid.js',
    'adapters/native-composition-contract.js','adapters/native-composition-desktop.js','adapters/native-composition-ui.js'];
  const html=fs.readFileSync(require.resolve('../src/index.html'),'utf8');
  const scripts=Array.from(html.matchAll(/<script src="js\/([^"]+)"/g),match=>match[1]).filter(file=>wanted.includes(file));
  assert.deepEqual(scripts,wanted);
  for(const file of scripts)vm.runInContext(fs.readFileSync(require.resolve('../src/js/'+file),'utf8'),context,{filename:file});
  browserRoot=vm.runInContext('globalThis',context);
  function project(){root.NemoNativeObjectProjectFile.projectView(browserRoot,info);
    state.layers[0].markers=layerMarkers;state.currentFrame=3;}
  project();controller=root.NemoNativeCompositionController;document.fire('DOMContentLoaded');
  const edits=()=>calls.filter(q=>q.operation===Contract.EDIT);
  async function settled(){for(let i=0;i<20;i++){await tail;await new Promise(resolve=>setImmediate(resolve));if(!controller.isPending())return;}throw Error('controller did not settle');}
  return {root,state,elements,document,controller,calls,notices,prompts,promptCalls,menus,jumps,edits,settled,diagnostics,
    dialogsCreated:()=>created.filter(el=>el.id==='bpm-settings-dialog').length,pagehide:()=>windowEvents.fire('pagehide'),
    projectionReady:value=>root.NemoNativeObjectProjectFile.projectionReadyForUI(browserRoot,value),
    fullInfo:()=>info,drift(){state.canvasW++;},repairProjection:project,
    setController(value){browserRoot.NemoNativeCompositionController=value;assert.strictEqual(browserRoot.NemoNativeCompositionController,value);},
    sameInstall:()=>vm.runInContext('NemoNativeCompositionUI.install(globalThis) === NemoNativeCompositionController',context),
    async ready(){await controller.presented();await settled();},
    marker:()=>elements['bars-row'].children.find(x=>x.className==='tl-marker tl-marker-comp'),
    unchanged(){assert.strictEqual(state.markers,persisted);assert.strictEqual(state.layers[0].markers,layerMarkers);
      for(const k of ['bpm','bpmOffset','bpmShow'])assert.equal(Object.hasOwn(state,k),false);},
    replace(){info={...info,documentId:'replacement',lifecycleGeneration:2};controller.willProject(info);project();},
    resubscribe(){ticket++;},
    hold(){let entered,release;const started=new Promise(r=>entered=r),wait=new Promise(r=>release=r);block={entered,wait};return {started,release};},
    retire(){active=false;controller.dispose();}};
}

function bpmDialog(h){
  const overlay=h.document.getElementById('bpm-settings-dialog');assert.ok(overlay,'native BPM form must be attached');
  const form=overlay.querySelector('form'),input=h.document.getElementById('bpm-settings-value');
  assert.ok(form);assert.ok(input);return {overlay,form,input,label:h.document.getElementById('bpm-settings-label')};
}
function submitBpm(h,value){const view=bpmDialog(h);view.input.value=value;assert.equal(view.form.fire('submit').defaultPrevented,true);return view;}
function enterBpm(h,tempo,offset){
  const labels=[bpmDialog(h).label.textContent];submitBpm(h,tempo);labels.push(bpmDialog(h).label.textContent);submitBpm(h,offset);return labels;
}

test('ordinary native consumers publish complete pinned readback without authoring legacy defaults',async()=>{
  const h=fixture();h.unchanged();assert.equal(h.root.SMMarkers.compMarkers().length,0);
  await h.ready();assert.equal(h.root.SMMarkers.compMarkers().length,2);assert.equal(h.root.SMBpm.framesPerBeat(),12);
  const count=h.calls.length;await h.controller.presented();await h.settled();assert.equal(h.calls.length,count,'presentation does not recursively query');
  assert.equal(h.sameInstall(),true);h.unchanged();
});

test('ordinary add key and BPM buttons submit native intents and display confirmed readback',async()=>{
  const h=fixture();await h.ready();
  h.document.fire('keydown',{key:'*',target:{tagName:'DIV'}});await h.settled();
  assert.equal(h.edits().length,1);assert.equal(h.edits()[0].payload.action.kind,'addMarker');
  assert.equal(h.edits()[0].payload.action.frame,3);assert.equal(h.root.SMMarkers.compMarkers().length,3);
  h.elements['btn-bpm'].fire('click');await h.settled();assert.equal(h.edits().length,2);
  assert.deepEqual(h.edits()[1].payload.action,{kind:'setBpm',bpmShow:true});
  h.elements['btn-bpm'].fire('contextmenu');enterBpm(h,'137.5','4');await h.settled();
  assert.deepEqual(h.edits()[2].payload.action,{kind:'setBpm',bpm:137.5,bpmOffset:4,bpmShow:true});
  assert.equal(h.root.SMBpm.framesPerBeat(),60/137.5*24);h.unchanged();
});

test('native drag changes only temporary DOM and commits exactly one captured ordinal edit',async()=>{
  const h=fixture();await h.ready();const marker=h.marker(),before=h.root.SMMarkers.compMarkers();
  marker.fire('mousedown',{clientX:10});h.document.fire('mousemove',{clientX:40});
  assert.equal(h.edits().length,0);assert.equal(before[0].frame,2.5);assert.equal(marker.style.left,'55px');h.unchanged();
  h.document.fire('mouseup');await h.settled();assert.equal(h.edits().length,1);
  assert.deepEqual(h.edits()[0].payload.action,{kind:'updateMarker',ordinal:1,frame:5.5});
  assert.equal(h.root.SMMarkers.compMarkers()[0].frame,5.5);h.unchanged();
});

test('retired marker menu and drag cannot write the same ordinal in a replacement document',async()=>{
  for(const gesture of ['menu','drag']) {
    const h=fixture();await h.ready();const marker=h.marker();
    if(gesture==='menu')marker.fire('contextmenu');else {marker.fire('mousedown');h.document.fire('mousemove',{clientX:30});}
    h.replace();await h.ready();
    if(gesture==='menu')h.menus[0].find(x=>x.label==='Supprimer le repère').action();else h.document.fire('mouseup');
    await h.settled();assert.equal(h.edits().length,0);assert(h.notices.length>0);h.unchanged();
  }
});

test('ordinary rename/remove and ruler clear retain native history ownership',async()=>{
  const h=fixture();await h.ready();h.prompts.push('Renamed');h.marker().fire('dblclick');await h.settled();
  assert.deepEqual(h.edits()[0].payload.action,{kind:'updateMarker',ordinal:1,name:'Renamed'});
  h.marker().fire('contextmenu');h.menus[0].find(x=>x.label==='Supprimer le repère').action();await h.settled();
  assert.deepEqual(h.edits()[1].payload.action,{kind:'removeMarker',ordinal:1});
  h.elements['bars-row'].fire('contextmenu');h.menus[1].find(x=>x.label==='Supprimer tous les repères').action();await h.settled();
  assert.deepEqual(h.edits()[2].payload.action,{kind:'clearMarkers'});assert.equal(h.root.SMMarkers.compMarkers().length,0);h.unchanged();
});

test('unavailable, replaced prompt, and changed subscription never use a legacy writer',async()=>{
  const h=fixture();h.root.SMMarkers.addCompMarker(2,'missing');h.root.SMBpm.toggle();assert.equal(h.edits().length,0);
  await h.ready();h.root.prompt=()=>{h.replace();return 'Changed';};h.marker().fire('dblclick');await h.settled();
  assert.equal(h.edits().length,0);await h.ready();h.resubscribe();h.root.SMBpm.toggle();await h.settled();
  assert.equal(h.edits().length,0);assert.equal(h.root.SMMarkers.addLayerMarker(0,1,''),null);h.unchanged();
});

test('pending native edit rejects overlapping ordinary intents and publishes only after reconciliation',async()=>{
  const h=fixture();await h.ready();const hold=h.hold();h.root.SMBpm.toggle();await hold.started;
  assert.equal(h.edits().length,1);assert.equal(h.controller.read(),null);h.root.SMMarkers.addCompMarker(7,'while pending');
  assert.equal(h.edits().length,1);h.unchanged();hold.release();await h.settled();
  assert.equal(h.controller.read().composition.effectiveBpmShow,true);assert.equal(h.edits().length,1);h.unchanged();
});

test('retirement during a pinned read prevents publication and ordinary controls remain unavailable',async()=>{
  const h=fixture(),hold=h.hold();const refresh=h.controller.presented();await hold.started;h.retire();hold.release();
  assert.equal(await refresh,null);await h.settled();assert.equal(h.controller.read(),null);
  h.root.SMMarkers.addCompMarker(4,'late');h.root.SMBpm.toggle();assert.equal(h.edits().length,0);h.unchanged();
});

// Exercise the retained scripting entry itself, without constructing a parallel API.
test('legacy scripting reads native projection and refuses synchronous metadata writes',async()=>{
  const h=fixture();await h.ready();
  h.root.NemoNativeLayerPlayheadCutUI={scripting:()=>()=>{}};
  vm.runInContext(fs.readFileSync(require.resolve('../src/js/nemo-script.js'),'utf8'),vm.createContext(h.root));
  const api=h.root.SMScript.api();
  assert.equal(api.comp.bpm,0);assert.equal(api.comp.bpmOffset,0);assert.equal(api.markers.list().length,2);
  assert.throws(()=>{api.comp.bpm=140;},/composition application API/);
  assert.throws(()=>{api.comp.bpmOffset=12;},/composition application API/);
  assert.throws(()=>api.markers.add(3,'script'),/composition application API/);
  assert.throws(()=>api.markers.removeAt(2.5),/composition application API/);
  h.unchanged();assert.equal(h.edits().length,0);h.retire();
  assert.throws(()=>api.markers.list(),/composition is unavailable/);
  assert.throws(()=>api.comp.bpm,/composition is unavailable/);
});

test('a ruler menu opened before native readiness never acquires a later document',async()=>{
  const h=fixture();h.elements['bars-row'].fire('contextmenu');
  const action=h.menus[0].find(x=>x.label.startsWith('Ajouter un repère ici'));
  await h.ready();action.action();await h.settled();assert.equal(h.edits().length,0);h.unchanged();
});

test('fresh readback after same-pin subscription replacement cannot revive old menu or drag',async()=>{
  for(const gesture of ['menu','drag','bpm-menu']) {
    const h=fixture();await h.ready();const old=h.controller.read(),marker=h.marker();
    if(gesture==='menu')marker.fire('contextmenu');
    else if(gesture==='drag'){marker.fire('mousedown');h.document.fire('mousemove',{clientX:30});}
    else h.elements['bars-row'].fire('contextmenu');
    h.resubscribe();await h.ready();const fresh=h.controller.read();
    assert.deepEqual(fresh.info,old.info);assert.notStrictEqual(fresh,old);
    if(gesture==='menu')h.menus[0].find(x=>x.label==='Supprimer le repère').action();
    else if(gesture==='drag')h.document.fire('mouseup');
    else {h.menus[0].find(x=>x.label==='Grille BPM…').action();enterBpm(h,'141','2');}
    await h.settled();assert.equal(h.edits().length,0);assert(h.notices.length>0);h.unchanged();
  }
});


test('a retained browser scripting facade switches to native projection and refuses legacy writers at admission',async()=>{
  const h=fixture();await h.ready();
  const native=h.root.__TAURI__;delete h.root.__TAURI__;
  h.root.NemoNativeLayerPlayheadCutUI={scripting:()=>()=>{}};
  h.root.SMBpm.refresh=()=>{};
  vm.runInContext(fs.readFileSync(require.resolve('../src/js/nemo-script.js'),'utf8'),vm.createContext(h.root));
  const api=h.root.SMScript.api();
  api.comp.bpm=93;api.comp.bpmOffset=7;
  assert.equal(api.comp.bpm,93);assert.equal(api.comp.bpmOffset,7);
  assert.equal(api.markers.list()[0].name,'legacy');
  h.root.__TAURI__=native;
  assert.equal(api.comp.bpm,0);assert.equal(api.comp.bpmOffset,0);
  assert.equal(api.markers.list()[0].name,'First');
  assert.throws(()=>{api.comp.bpm=140;},/composition application API/);
  assert.throws(()=>{api.comp.bpmOffset=12;},/composition application API/);
  assert.throws(()=>api.markers.add(3,'retained'),/composition application API/);
  assert.throws(()=>api.markers.removeAt(39),/composition application API/);
  assert.equal(h.state.bpm,93);assert.equal(h.state.bpmOffset,7);
  assert.equal(h.state.markers[0].name,'legacy');assert.equal(h.edits().length,0);
});


test('actual HTML-bound BPM uses full project readiness while keeping five-field authority pins',async()=>{
  const h=fixture();assert.equal(h.projectionReady(h.fullInfo()),true);await h.ready();
  const origin=h.controller.read();assert.ok(origin);
  assert.deepEqual(Object.keys(origin.info).sort(),['contentRevision','documentId','documentSnapshotId','instanceId','lifecycleGeneration']);
  assert.equal(h.projectionReady(origin.info),false,'a narrow authority pin is not full UI metadata');
  h.elements['btn-bpm'].fire('contextmenu');const stages=enterBpm(h,'137.5','4');await h.settled();
  assert.equal(stages.length,2);assert.equal(h.promptCalls.length,0);assert.equal(h.edits().length,1);
  assert.deepEqual(h.edits()[0].payload.action,{kind:'setBpm',bpm:137.5,bpmOffset:4,bpmShow:true});
  assert.equal(h.controller.read().composition.effectiveBpm,137.5);h.unchanged();
  h.drift();assert.equal(h.controller.read(),null);
  h.elements['btn-bpm'].fire('contextmenu');await h.settled();
  assert.equal(stages.length,2);assert.equal(h.dialogsCreated(),1,'unavailable full projection cannot start another form');
  assert.equal(h.promptCalls.length,0,'native full projection never uses window.prompt');
  assert.equal(h.edits().length,1);assert.equal(h.controller.read(),null);
  h.repairProjection();await h.ready();assert.ok(h.controller.read());
  assert.equal(h.edits().length,1,'restoring presentation never replays the old intent');h.unchanged();
});

test('full UI drift during a held native read refuses the complete projection',async()=>{
  const h=fixture(),hold=h.hold(),reading=h.controller.presented();await hold.started;
  h.drift();hold.release();assert.equal(await reading,null);await h.settled();
  assert.equal(h.controller.read(),null);assert.equal(h.edits().length,0);h.unchanged();
});


test('only the ordinary BPM settings origin read emits bounded refusal detail',async()=>{
  const h=fixture();h.root.SMBpm.refresh();h.root.SMBpm.framesPerBeat();assert.deepEqual(h.diagnostics,[]);
  h.elements['btn-bpm'].fire('contextmenu');await h.settled();
  assert.deepEqual(h.diagnostics,[['bpm-settings-refused','cache-empty']]);assert.equal(h.promptCalls.length,0);
  await h.ready();h.elements['btn-bpm'].fire('contextmenu');h.document.getElementById('bpm-settings-cancel').fire('click');await h.settled();
  assert.equal(h.dialogsCreated(),1);assert.equal(h.promptCalls.length,0);assert.equal(h.diagnostics.length,1);assert.equal(h.edits().length,0);
  h.drift();h.elements['btn-bpm'].fire('contextmenu');await h.settled();
  assert.deepEqual(h.diagnostics.at(-1),['bpm-settings-refused','projection-unready']);assert.equal(h.dialogsCreated(),1);assert.equal(h.promptCalls.length,0);
  const controller=h.root.NemoNativeCompositionController;h.setController(null);
  h.elements['btn-bpm'].fire('contextmenu');assert.deepEqual(h.diagnostics.at(-1),['bpm-settings-refused','controller-missing']);
  h.setController(controller);
  h.root.NemoNativeObjectProjectDesktop.openDiagnostic=()=>{throw Error('PRIVATE diagnostic sink');};
  assert.doesNotThrow(()=>h.elements['btn-bpm'].fire('contextmenu'));await h.settled();
  assert.equal(h.dialogsCreated(),1);assert.equal(h.promptCalls.length,0);assert.equal(h.edits().length,0);h.unchanged();
});


test('native BPM form exposes two focused text stages without window prompt or blur commit',async()=>{
  const h=fixture();await h.ready();h.elements['btn-bpm'].fire('contextmenu');
  const first=bpmDialog(h);
  assert.equal(first.form.getAttribute('role'),'dialog');assert.equal(first.form.getAttribute('aria-modal'),'true');
  assert.equal(first.form.getAttribute('aria-labelledby'),first.label.id);assert.equal(first.label.htmlFor,first.input.id);
  assert.equal(h.document.getElementById('bpm-settings-confirm').type,'submit');
  assert.equal(h.document.getElementById('bpm-settings-cancel').type,'button');
  assert.equal(h.document.getElementById('bpm-settings-close').type,'button');
  assert.equal(first.label.textContent,'Tempo en BPM (0 pour masquer la grille)');assert.equal(first.input.value,'120');
  assert.equal(first.input.type,'text');assert.equal(first.input.inputMode,'decimal');
  assert.strictEqual(h.document.activeElement,first.input);assert.equal(first.input.selectCalls,1);
  first.input.value='137.5';first.input.fire('blur');assert.equal(h.edits().length,0);
  assert.strictEqual(bpmDialog(h).overlay,first.overlay);assert.equal(first.input.value,'137.5');
  submitBpm(h,'137.5');const second=bpmDialog(h);
  assert.strictEqual(second.overlay,first.overlay);assert.strictEqual(second.input,first.input);
  assert.equal(second.label.textContent,'Décalage du premier temps, en frames');assert.equal(second.input.value,'0');
  assert.strictEqual(h.document.activeElement,second.input);assert.equal(second.input.selectCalls,2);assert.equal(h.edits().length,0);
  submitBpm(h,'4');first.form.fire('submit');await h.settled();
  assert.equal(h.document.getElementById('bpm-settings-dialog'),null);assert.equal(h.promptCalls.length,0);
  assert.equal(h.edits().length,1);assert.deepEqual(h.edits()[0].payload.action,{kind:'setBpm',bpm:137.5,bpmOffset:4,bpmShow:true});h.unchanged();
});

test('both native BPM stages cancel through Cancel, close, Escape or overlay with no command',async()=>{
  for(const stage of ['tempo','offset'])for(const route of ['cancel','close','escape','overlay']){
    const h=fixture();await h.ready();h.elements['btn-bpm'].fire('contextmenu');
    if(stage==='offset')submitBpm(h,'137.5');const view=bpmDialog(h);
    if(route==='cancel'||route==='close')h.document.getElementById('bpm-settings-'+route).fire('click');
    else if(route==='escape')assert.equal(view.input.fire('keydown',{key:'Escape'}).defaultPrevented,true);
    else view.overlay.fire('click');
    assert.equal(h.document.getElementById('bpm-settings-dialog'),null,stage+'/'+route);
    view.form.fire('submit');await h.settled();assert.equal(h.edits().length,0,stage+'/'+route);
    assert.equal(h.promptCalls.length,0);assert.equal(h.fullInfo().contentRevision,0);h.unchanged();
  }
});

test('native BPM retains invalid tempo hide and original fractional offset parsing',async()=>{
  for(const tempo of ['0','-2','not a number','']){
    const h=fixture();await h.ready();h.elements['btn-bpm'].fire('contextmenu');const view=submitBpm(h,tempo);
    assert.equal(view.label.textContent,'Tempo en BPM (0 pour masquer la grille)');
    assert.equal(h.document.getElementById('bpm-settings-dialog'),null);await h.settled();
    assert.equal(h.edits().length,1);assert.deepEqual(h.edits()[0].payload.action,{kind:'setBpm',bpmShow:false});
    assert.equal(h.promptCalls.length,0);h.unchanged();
  }
  for(const [offset,expected] of [['4.9frames',4],['-2.9',0],['',0]]){
    const h=fixture();await h.ready();h.elements['btn-bpm'].fire('contextmenu');enterBpm(h,'145.25bpm',offset);await h.settled();
    assert.equal(h.edits().length,1);assert.deepEqual(h.edits()[0].payload.action,{kind:'setBpm',bpm:145.25,bpmOffset:expected,bpmShow:true});h.unchanged();
  }
});

test('native BPM frozen origin refuses stale revision, replacement, resubscription, disposal and controller replacement',async()=>{
  for(const mode of ['revision','replacement','subscription','disposed','controller']){
    const h=fixture();await h.ready();const origin=h.controller.read();h.elements['btn-bpm'].fire('contextmenu');submitBpm(h,'137.5');
    let unexpected=0;
    if(mode==='revision'){h.elements['btn-bpm'].fire('click');await h.settled();}
    else if(mode==='replacement'){h.replace();await h.ready();}
    else if(mode==='subscription'){h.resubscribe();await h.ready();assert.deepEqual(h.controller.read().info,origin.info);}
    else if(mode==='disposed')h.retire();
    else h.setController({read:()=>origin,edit(){unexpected++;}});
    const before=h.edits().length;submitBpm(h,'4');await h.settled();
    assert.equal(h.edits().length,before,mode);assert.equal(unexpected,0);assert.ok(h.notices.length>0,mode);
    assert.equal(h.document.getElementById('bpm-settings-dialog'),null);h.unchanged();
  }
});

test('native BPM refuses pending confirmation and duplicate submit without replay',async()=>{
  const h=fixture();await h.ready();h.elements['btn-bpm'].fire('contextmenu');submitBpm(h,'137.5');
  const held=h.hold();h.elements['btn-bpm'].fire('click');await held.started;
  try{
    const view=submitBpm(h,'4');view.form.fire('submit');assert.equal(h.edits().length,1);
    assert.equal(h.document.getElementById('bpm-settings-dialog'),null);assert.ok(h.notices.length>0);h.unchanged();
  }finally{held.release();await h.settled();}
  assert.equal(h.edits().length,1);assert.equal(h.controller.read().composition.effectiveBpm,120);
});

test('duplicate BPM open preserves draft and captured origin; pagehide disposes its form',async()=>{
  const h=fixture();await h.ready();h.elements['btn-bpm'].fire('contextmenu');const view=bpmDialog(h);view.input.value='137.5';
  h.root.SMBpm.openSettings();assert.strictEqual(bpmDialog(h).overlay,view.overlay);assert.equal(view.input.value,'137.5');assert.equal(h.dialogsCreated(),1);
  h.elements['btn-bpm'].fire('click');await h.settled();h.root.SMBpm.openSettings();
  assert.strictEqual(bpmDialog(h).overlay,view.overlay);enterBpm(h,'137.5','4');await h.settled();
  assert.equal(h.edits().length,1,'new readback must not rebase the open form');
  h.elements['btn-bpm'].fire('contextmenu');const late=bpmDialog(h);h.pagehide();
  assert.equal(h.document.getElementById('bpm-settings-dialog'),null);late.form.fire('submit');await h.settled();
  assert.equal(h.edits().length,1);h.unchanged();
});

test('browser BPM retains synchronous prompt values and both cancellation points',async()=>{
  for(const values of [[null],['137.5',null],['137.5','4']]){
    const h=fixture();await h.ready();delete h.root.__TAURI__;h.prompts.push(...values);
    h.elements['btn-bpm'].fire('contextmenu');await h.settled();
    assert.equal(h.promptCalls.length,values.length);assert.equal(h.dialogsCreated(),0);assert.equal(h.edits().length,0);
    assert.equal(h.promptCalls[0][0],'Tempo en BPM (0 pour masquer la grille)');assert.equal(h.promptCalls[0][1],'120');
    if(values.length===2){assert.equal(h.promptCalls[1][0],'Décalage du premier temps, en frames');assert.equal(h.promptCalls[1][1],'0');}
    const accepted=values[1]==='4';assert.equal(h.state.bpm,accepted?137.5:120);assert.equal(h.state.bpmOffset,accepted?4:0);
    assert.equal(h.state.bpmShow,accepted?true:undefined);assert.equal(h.state.markers[0].name,'legacy');
  }
});
