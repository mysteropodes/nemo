'use strict';
// Real Time Remap UI child, placement contract and project projection; the native
// owner, its queue and the desktop placement read are modelled. These controls
// establish no native kernel, decoder count, installed UI or pixel parity.
const test=require('node:test'),assert=require('node:assert/strict');
const Remap=require('../src/js/adapters/native-time-remap-ui.js');
const ProjectFile=require('../src/js/adapters/native-object-project-file.js');
const OP='command.document.object.project.placement.edit';
async function settle(){for(let i=0;i<20;i++)await Promise.resolve();}
function fixture(placement,host){
  let info={instanceId:'instance',documentId:'document',lifecycleGeneration:3,contentRevision:7,documentSnapshotId:'snapshot-7',
    canvasW:320,canvasH:180,fps:24,totalFrames:48,layers:[{layerUid:'a',name:'Ink'},{layerUid:'v',name:'Clip'}]};
  const effects=[],calls=[],reads=[];
  const legacy={toggleTimeRemap(){effects.push('legacy');return true;},isTimeRemapped(){effects.push('legacy');return true;},
    footageInterpMode(){effects.push('legacy');return 'blend';},setFootageInterpMode(){effects.push('legacy');return true;},t:key=>key};
  const root=host||{state:{}};
  Object.assign(root,{SM:Object.assign({},legacy),showToast(message){effects.push(['toast',message]);},
    updateUI(frameOnly){effects.push(frameOnly===true?'updateUI:frame':'updateUI');}});
  ProjectFile.projectView(root,info);root.state.appMode='motion';root.state.activeSymbolId=null;root.state.currentFrame=10;root._curFrame=10;
  let facts=placement,active=true,blocks=true,serial=0;
  const control={receipt:null,range:{inPoint:4,outPoint:30},failDispatch:null};
  function presented(){return {...info,available:true,kind:'native-object'};}
  const session={isActive:()=>active,blocksLegacy:()=>blocks,info:()=>info,readPresentedObjectSession:presented,
    requestId:label=>label+'-'+(++serial),
    async readLayerPlacement(uid,frame,pin){reads.push(['placement',uid,frame,pin.documentSnapshotId]);
      return {kind:'layerPlacement',instanceId:pin.instanceId,documentId:pin.documentId,atRevision:pin.contentRevision,
        documentSnapshotId:pin.documentSnapshotId,contextId:'scene-root',layerUid:uid,frame,placement:uid==='v'?facts:null};},
    async readLayerRange(uid,frame,pin){reads.push(['range',uid,frame]);
      return {kind:'layerRange',instanceId:pin.instanceId,documentId:pin.documentId,atRevision:pin.contentRevision,
        documentSnapshotId:pin.documentSnapshotId,contextId:'scene-root',layerUid:uid,frame,
        range:{rawInPoint:control.range.inPoint,rawOutPoint:control.range.outPoint,...control.range,hasCustomRange:true,hasTimeLink:false}};},
    async runResourceCommand(action){
      effects.push('queue');
      return action({readPresentedObjectSession:presented,
        async dispatch(request){calls.push(structuredClone(request));if(control.failDispatch)throw control.failDispatch;
          return control.receipt?control.receipt(request):{apiVersion:2,requestId:request.requestId,instanceId:info.instanceId,
            documentId:info.documentId,contentRevision:request.expectedRevision+1,ok:true,result:{applied:true,historyEntriesAdded:1}};},
        async reconcile(revision){effects.push(['reconcile',revision]);
          if(revision!==undefined){info={...info,contentRevision:revision,documentSnapshotId:'snapshot-'+revision};
            assert.equal(ProjectFile.refreshLayerTable(root,info),true);}return info;}});
    }};
  const ports={currentSession:()=>session,projectionReady:value=>ProjectFile.projectionReadyForUI(root,value)};
  const api=Remap.createPorts(root,ports);
  return {root,api,legacy,control,effects,calls,reads,setFacts:v=>{facts=v;},setBlocks:v=>{blocks=v;},setActive:v=>{active=v;},
    info:()=>info,frame:f=>{root.state.currentFrame=f;root._curFrame=f;}};
}
const video=extra=>({placementId:'p1',assetId:'v1',assetKind:'video',offsetFrames:0,timeRemap:null,interpMode:null,
  frameBlend:null,sourceFrameCount:48,...extra});

test('enable dispatches the bounded intent with the native range and the bound source frame count',async()=>{
  const f=fixture(video());assert.ok(f.api.bindTimeRemapEntries());
  assert.equal(await f.root.SM.toggleTimeRemap(1),true);
  assert.equal(f.calls.length,1);
  assert.equal(f.calls[0].operation,OP);assert.equal(f.calls[0].expectedRevision,7);
  assert.deepEqual(f.calls[0].payload,{kind:'enableTimeRemap',placementId:'p1',inFrame:4,outFrame:30,lastSourceFrame:47});
  assert.deepEqual(f.effects.filter(e=>Array.isArray(e)),[['reconcile',8],['toast','toastTimeRemapEnabledFromSuffix47']]);
  assert.equal(f.effects.includes('legacy'),false);
});

test('disable and the footage mode edit the existing placement; an unchanged mode dispatches nothing',async()=>{
  const f=fixture(video({timeRemap:{keyCount:2,keyAtFrame:false},interpMode:'blend',frameBlend:true}));f.api.bindTimeRemapEntries();
  assert.equal(await f.root.SM.toggleTimeRemap(1),true);
  assert.deepEqual(f.calls[0].payload,{kind:'disableTimeRemap',placementId:'p1'});
  assert.equal(await f.root.SM.setFootageInterpMode(1,'blend'),true);
  assert.equal(f.calls.length,1,'the stored mode is not re-sent');
  assert.equal(await f.root.SM.setFootageInterpMode(1,'motion'),true);
  assert.deepEqual(f.calls[1].payload,{kind:'setInterpMode',placementId:'p1',interpMode:'motion'});
  assert.equal(f.effects.includes('legacy'),false);
});

test('an unknown source frame count keeps Enable unavailable and a layer without video only explains',async()=>{
  for(const count of [null,0]){
    const f=fixture(video({sourceFrameCount:count}));f.api.bindTimeRemapEntries();
    assert.equal(await f.root.SM.toggleTimeRemap(1),false);
    assert.equal(f.calls.length,0);assert.equal(f.reads.some(r=>r[0]==='range'),false);
  }
  const f=fixture(video());f.api.bindTimeRemapEntries();
  assert.equal(await f.root.SM.toggleTimeRemap(0),false);
  assert.equal(f.calls.length,0);
  assert.deepEqual(f.effects.filter(e=>Array.isArray(e)),[['toast','toastTimeRemapAppliesToComponents']]);
});

test('synchronous answers come from one read of the presented snapshot and redraw once when they differ',async()=>{
  const f=fixture(video({timeRemap:{keyCount:2,keyAtFrame:true},interpMode:'motion'}));f.api.bindTimeRemapEntries();
  assert.equal(f.root.SM.isTimeRemapped(1),false,'the first answer is the default while the read is pending');
  await settle();
  assert.equal(f.root.SM.isTimeRemapped(1),true);assert.equal(f.root.SM.footageInterpMode(1),'motion');
  assert.equal(f.reads.filter(r=>r[0]==='placement').length,1);
  assert.equal(f.effects.filter(e=>e==='updateUI').length,1);
  assert.equal(f.root.SM.isTimeRemapped(0),false);await settle();
  assert.equal(f.effects.filter(e=>e==='updateUI').length,1,'a layer without video keeps the default without a redraw');
});

test('a refused, uncorrelated or no-op edit reports false and an uncertain receipt reconciles without replay',async()=>{
  const noop=fixture(video());noop.api.bindTimeRemapEntries();
  noop.control.receipt=r=>({apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'document',contentRevision:7,ok:true,result:{applied:false,historyEntriesAdded:0}});
  assert.equal(await noop.root.SM.toggleTimeRemap(1),false);
  assert.equal(noop.effects.some(e=>Array.isArray(e)&&e[0]==='reconcile'),false);
  assert.deepEqual(noop.effects.filter(e=>Array.isArray(e)),[['toast','toastNativeTimeRemapNotApplied']],'a refusal is surfaced');
  const lost=fixture(video());lost.api.bindTimeRemapEntries();lost.control.failDispatch=new Error('transport lost');
  assert.equal(await lost.root.SM.toggleTimeRemap(1),false);
  assert.equal(lost.calls.length,1);
  assert.deepEqual(lost.effects.filter(e=>Array.isArray(e)),[['reconcile',undefined],['toast','toastNativeTimeRemapNotApplied']]);
  const foreign=fixture(video());foreign.api.bindTimeRemapEntries();
  foreign.control.receipt=r=>({apiVersion:2,requestId:r.requestId,instanceId:'other',documentId:'document',contentRevision:8,ok:true,result:{applied:true,historyEntriesAdded:1}});
  assert.equal(await foreign.root.SM.toggleTimeRemap(1),false);
});

test('a second action while one is pending is consumed, and the entries stay native while the owner blocks legacy',async()=>{
  const f=fixture(video());f.api.bindTimeRemapEntries();
  const first=f.root.SM.toggleTimeRemap(1);
  assert.equal(await f.root.SM.toggleTimeRemap(1),false);
  assert.equal(await first,true);assert.equal(f.calls.length,1);
  f.api.disposeTimeRemapEntries();
  assert.notStrictEqual(f.root.SM.toggleTimeRemap,f.legacy.toggleTimeRemap,'an active owner keeps the refusing entries');
  assert.equal(await f.root.SM.toggleTimeRemap(1),false,'a retired binding never dispatches');
  f.setBlocks(false);f.setActive(false);f.api.disposeTimeRemapEntries();
  assert.strictEqual(f.root.SM.toggleTimeRemap,f.legacy.toggleTimeRemap);
  assert.strictEqual(f.root.SM.setFootageInterpMode,f.legacy.setFootageInterpMode);
});

const remap=(keys,frame,value)=>({keyCount:keys.length,keyAtFrame:keys.some(k=>k.frame===frame),value,keys});
const KEYS=[{frame:0,value:0},{frame:24,value:47}];
test('the Time Remap row value and stopwatch set or remove the key at the playhead through the placement',async()=>{
  const f=fixture(video({timeRemap:remap(KEYS,10,19.6)}));f.api.bindTimeRemapEntries();
  assert.equal(f.root.SM.timeRemapKeyFacts(1),null,'the first answer is the default while the read is pending');
  await settle();
  assert.deepEqual(f.root.SM.timeRemapKeyFacts(1),remap(KEYS,10,19.6),'keys and the eased source frame are read back');
  assert.equal(f.root.SM.timeRemapKeyFacts(0),null);
  assert.equal(await f.root.SM.timeRemapKeyIntent(1,12.5),true);
  assert.deepEqual(f.calls[0].payload,{kind:'setTimeRemapKey',placementId:'p1',frame:10,value:12.5});
  assert.equal(await f.root.SM.timeRemapKeyIntent(1,null),true,'add a key here keys the read value');
  assert.deepEqual(f.calls[1].payload,{kind:'setTimeRemapKey',placementId:'p1',frame:10,value:19.6});
  f.setFacts(video({timeRemap:{keyCount:0,keyAtFrame:false,value:null,keys:[]}}));
  assert.equal(await f.root.SM.timeRemapKeyIntent(1,null),false,'no value read: nothing is sent');
  assert.equal(f.calls.length,2);
  assert.ok(f.effects.some(e=>Array.isArray(e)&&e[0]==='toast'&&e[1]==='toastNativeTimeRemapValueUnavailable'));
  f.setFacts(video({timeRemap:remap([{frame:0,value:0},{frame:10,value:5},{frame:24,value:47}],10,5)}));
  assert.equal(await f.root.SM.timeRemapKeyIntent(1,null),true);
  assert.deepEqual(f.calls[2].payload,{kind:'removeTimeRemapKey',placementId:'p1',frame:10});
  f.setFacts(video({timeRemap:remap([{frame:10,value:5}],10,5)}));
  assert.equal(await f.root.SM.timeRemapKeyIntent(1,null),true);
  assert.deepEqual(f.calls[3].payload,{kind:'disableTimeRemap',placementId:'p1'});
  for(const bad of [NaN,Infinity,'3'])assert.equal(await f.root.SM.timeRemapKeyIntent(1,bad),false);
  f.setFacts(video());assert.equal(await f.root.SM.timeRemapKeyIntent(1,4),false,'an unremapped placement takes no key');
  assert.equal(f.calls.length,4);assert.equal(f.effects.includes('legacy'),false);
});

test('the row stays across a frame change and an edit; only the remapped layer re-reads, redrawing frame-only',async()=>{
  let facts=video({timeRemap:remap(KEYS,10,19.6)});
  const f=fixture(facts);f.api.bindTimeRemapEntries();
  f.root.SM.timeRemapKeyFacts(1);f.root.SM.isTimeRemapped(0);await settle();
  assert.deepEqual(f.effects.filter(e=>/^updateUI/.test(e)),['updateUI'],'the first answer redraws in full once');
  f.effects.length=0;const before=f.reads.length;
  f.frame(24);f.setFacts(video({timeRemap:remap(KEYS,24,47)}));
  assert.deepEqual(f.root.SM.timeRemapKeyFacts(1),{keyCount:2,keyAtFrame:true,value:null,keys:KEYS},'the row stays; the value is pending');
  assert.equal(f.root.SM.isTimeRemapped(0),false);assert.equal(f.root.SM.isTimeRemapped(1),true);
  await settle();
  assert.deepEqual(f.reads.slice(before),[['placement','v',24,'snapshot-7']],'the layer without video is not re-read per frame');
  assert.deepEqual(f.root.SM.timeRemapKeyFacts(1),remap(KEYS,24,47));
  assert.deepEqual(f.effects.filter(e=>/^updateUI/.test(e)),['updateUI:frame'],'a new value only is a frame-only redraw');
  f.effects.length=0;
  const moved=[{frame:0,value:0},{frame:24,value:30}];
  assert.equal(await f.root.SM.timeRemapKeyIntent(1,30),true);
  assert.equal(f.info().documentSnapshotId,'snapshot-8');
  f.setFacts(video({timeRemap:remap(moved,24,30)}));
  assert.deepEqual(f.root.SM.timeRemapKeyFacts(1),{keyCount:2,keyAtFrame:true,value:null,keys:KEYS},'the row stays while the new snapshot is read');
  await settle();
  assert.deepEqual(f.root.SM.timeRemapKeyFacts(1),remap(moved,24,30));
  assert.ok(f.effects.includes('updateUI'),'changed keys redraw the grid');
});

test('the key entries are optional on the legacy owner and are removed again on release',async()=>{
  const f=fixture(video());assert.equal('timeRemapKeyFacts' in f.legacy,false);f.api.bindTimeRemapEntries();
  assert.equal(typeof f.root.SM.timeRemapKeyIntent,'function');
  f.setBlocks(false);f.setActive(false);f.api.disposeTimeRemapEntries();
  assert.equal('timeRemapKeyFacts' in f.root.SM,false);assert.equal('timeRemapKeyIntent' in f.root.SM,false);
});

// A modelled DOM for the native inspector mount; the inspector code itself is production.
function dom(){
  const doc={activeElement:null,createElement(tag){
    const n={tag,children:[],parentNode:null,dataset:{},style:{},attributes:{},listeners:{},hidden:false,disabled:false,title:'',value:'',_cls:new Set(),textContent:'',
      get className(){return [...this._cls].join(' ');},set className(v){this._cls=new Set(String(v).split(/\s+/).filter(Boolean));},
      appendChild(c){c.parentNode=this;this.children.push(c);return c;},removeChild(c){this.children.splice(this.children.indexOf(c),1);c.parentNode=null;return c;},
      setAttribute(k,v){this.attributes[k]=String(v);},removeAttribute(k){delete this.attributes[k];if(k==='title')this.title='';},
      addEventListener(t,l){(this.listeners[t]||=[]).push(l);},dispatchEvent(e){return (this.listeners[e.type]||[]).map(l=>l(e))[0];},
      closest(sel){return sel==='input.scrub'&&this._cls.has('scrub')?this:null;},setPointerCapture(){},focus(){},select(){},min:'',max:''};
    n.classList={add:c=>n._cls.add(c),remove:c=>n._cls.delete(c),contains:c=>n._cls.has(c),toggle:(c,on)=>{if(on===undefined?!n._cls.has(c):on)n._cls.add(c);else n._cls.delete(c);}};
    return n;}};
  return doc;
}
function inspector(placement,opts={}){
  const doc=dom(),f=fixture(placement,{state:{},document:doc});f.api.bindTimeRemapEntries();f.root.state.activeLayerIdx=opts.active===undefined?1:opts.active;
  const replaced=[],surfaced=[];
  if(opts.controller!==false)f.root.NemoNativeImportController={video:{declared:()=>opts.declared!==false,replaceActive:id=>{replaced.push(id);return Promise.resolve({status:'replaced'});}},
    surface:r=>{surfaced.push(r);return r;}};
  const container=doc.createElement('div'),ports={currentSession:()=>null};
  const all=n=>[n].concat(n.children.flatMap(all));
  const mount=()=>Remap.mountInspector(f.root,container,ports);
  const q=()=>{const section=container.children.find(n=>n.id==='native-footage-inspector'),nodes=section?all(section):[],buttons=nodes.filter(n=>n.tag==='button');
    return {section,source:nodes.filter(n=>n._cls.has('psel'))[0],count:nodes.filter(n=>n._cls.has('psel'))[1],mode:nodes.find(n=>n.tag==='select'),
      toggle:buttons[0],key:buttons[1],replace:buttons[2],value:nodes.find(n=>n.tag==='input')};};
  return {f,doc,container,mount,q,replaced,surfaced};
}

test('the native Footage inspector appears only for the active native video layer, from the read',async()=>{
  const ink=inspector(video(),{active:0});ink.mount();await settle();ink.mount();
  assert.equal(ink.q().section.hidden,true,'a layer without a video placement shows no Footage section');
  const h=inspector(video());const b=h.mount();
  assert.equal(h.q().section.hidden,true,'hidden while the placement is read');
  await settle();assert.strictEqual(h.mount(),b,'a repaint refreshes the mounted section');
  const v=h.q();assert.equal(v.section.hidden,false);assert.equal(v.source.textContent,'v1');assert.equal(v.count.textContent,'48');
  assert.equal(v.toggle.attributes['aria-pressed'],'false');assert.equal(v.key.hidden,true);assert.equal(v.value.hidden,true);
  assert.equal(v.replace.disabled,false);assert.equal(h.container.children.length,1);
  const off=inspector(video(),{declared:false});off.mount();await settle();off.mount();
  assert.equal(off.q().replace.disabled,true);assert.equal(off.q().replace.title,'titleNativeVideoReplaceUnavailable','localized unavailable reason');
  const none=inspector(video(),{controller:false});none.mount();await settle();none.mount();assert.equal(none.q().replace.disabled,true);
  assert.equal(h.f.calls.length,0,'rendering dispatches nothing');
});

test('Replace calls replaceActive with the read placement id once and surfaces the outcome',async()=>{
  const h=inspector(video());h.mount();await settle();h.mount();
  await h.q().replace.dispatchEvent({type:'click'});
  assert.deepEqual(h.replaced,['p1']);assert.deepEqual(h.surfaced,[{status:'replaced'}]);
  const off=inspector(video(),{declared:false});off.mount();await settle();off.mount();
  await off.q().replace.dispatchEvent({type:'click'});assert.deepEqual(off.replaced,[]);
});

test('the Time Remap block reads value and keys natively; key toggle and value commit call the intents once',async()=>{
  const h=inspector(video({timeRemap:remap(KEYS,10,19.6)}));h.mount();await settle();h.mount();
  let v=h.q();
  assert.equal(v.toggle.attributes['aria-pressed'],'true');assert.equal(v.key.hidden,false);
  assert.equal(String(v.value.value),'19.6');assert.equal(v.value.dataset.nativeScrub,'commit');assert.ok(v.value.classList.contains('scrub'));
  assert.equal(v.key.attributes['aria-pressed'],'false','no key at the playhead');
  await v.key.dispatchEvent({type:'click'});
  assert.equal(h.f.calls.length,1);assert.deepEqual(h.f.calls[0].payload,{kind:'setTimeRemapKey',placementId:'p1',frame:10,value:19.6});
  h.f.setFacts(video({timeRemap:remap(KEYS,10,19.6)}));
  v.value.value='12.5';await v.value.dispatchEvent({type:'change'});
  assert.equal(h.f.calls.length,2);assert.deepEqual(h.f.calls[1].payload,{kind:'setTimeRemapKey',placementId:'p1',frame:10,value:12.5});
  v.value.value='';await v.value.dispatchEvent({type:'change'});assert.equal(h.f.calls.length,2,'an empty field sends nothing');
  await v.mode.dispatchEvent({type:'change'});assert.equal(h.f.calls.length,2,'the stored mode is not re-sent');
  v.mode.value='blend';await v.mode.dispatchEvent({type:'change'});
  assert.deepEqual(h.f.calls[2].payload,{kind:'setInterpMode',placementId:'p1',interpMode:'blend'});
  assert.equal(h.f.effects.includes('legacy'),false);
});

test('a frame change updates the mounted inspector in place; an unread value is blank and cannot be dragged',async()=>{
  const h=inspector(video({timeRemap:remap(KEYS,10,19.6)}));h.mount();await settle();h.mount();
  const before=h.q();h.f.effects.length=0;
  h.f.frame(24);h.f.setFacts(video({timeRemap:remap(KEYS,24,47)}));h.mount();
  let v=h.q();assert.strictEqual(v.section,before.section);assert.strictEqual(v.value,before.value);
  assert.equal(String(v.value.value),'');assert.equal(v.value.classList.contains('scrub'),false);
  assert.equal(v.key.attributes['aria-pressed'],'true','the read keys name a key here');
  await settle();assert.deepEqual(h.f.effects.filter(e=>/^updateUI/.test(e)),['updateUI:frame'],'a new value is a frame-only paint');
  h.mount();v=h.q();assert.equal(String(v.value.value),'47');assert.ok(v.value.classList.contains('scrub'));
});

test('a refused edit toasts and the field shows the read value again; retire removes the section',async()=>{
  const h=inspector(video({timeRemap:remap(KEYS,10,19.6)}));h.mount();await settle();h.mount();
  h.f.control.receipt=r=>({apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'document',contentRevision:7,ok:true,result:{applied:false,historyEntriesAdded:0}});
  const v=h.q();v.value.value='30';await v.value.dispatchEvent({type:'change'});await settle();
  assert.ok(h.f.effects.some(e=>Array.isArray(e)&&e[1]==='toastNativeTimeRemapNotApplied'));
  assert.equal(String(v.value.value),'19.6','readback, never the typed shadow');
  Remap.retire(h.f.root);assert.equal(h.container.children.length,0);
  await v.key.dispatchEvent({type:'click'});assert.equal(h.f.calls.length,1,'a retired inspector dispatches nothing');
  const again=h.mount();assert.equal(h.container.children.length,1);again.retire();assert.equal(h.container.children.length,0);
});
function scrubHarness(){
  const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
  const source=fs.readFileSync(path.join(__dirname,'../src/js/ui.js'),'utf8');
  const start=source.indexOf('var scrubState=null;'),end=source.indexOf('})();',start);
  assert.ok(start>=0&&end>start,'production numeric scrub is available');
  const listeners=new Map(),win={_scrubLiveActive:false,pushUndo(){throw new Error('legacy checkpoint reached');},addEventListener(){}};
  vm.runInNewContext(source.slice(start,end),{document:{activeElement:null,addEventListener(type,l){listeners.set(type,l);}},window:win,Math,
    Event:class{constructor(type){this.type=type;}},requestAnimationFrame(){throw new Error('a native scrub must not dispatch an interim change');},cancelAnimationFrame(){}});
  return (type,input,clientX)=>listeners.get(type)({type,target:input,pointerId:3,clientX,preventDefault(){}});
}


test('dragging the native Time Remap value commits once on release from the read value',async()=>{
  const h=inspector(video({timeRemap:remap(KEYS,10,19.6)}));h.mount();await settle();h.mount();
  const field=h.q().value,fire=scrubHarness();
  fire('pointerdown',field,100);
  for(const x of [120,140,160,180])fire('pointermove',field,x);
  assert.equal(h.f.calls.length,0,'no edit while dragging');
  fire('pointerup',field,180);
  await settle();
  assert.equal(h.f.calls.length,1,'one release, one native edit');
  assert.deepEqual(h.f.calls[0].payload,{kind:'setTimeRemapKey',placementId:'p1',frame:10,value:40},'20 whole steps from the read 19.6');
  h.mount();assert.equal(String(field.value),'19.6','the field shows the native read, never the dragged shadow');
  assert.equal(h.f.effects.includes('legacy'),false);
});

test('the native object inspector mounts the Footage section and retires it with the inspector',()=>{
  const src=require('node:fs').readFileSync(require('node:path').join(__dirname,'../src/js/adapters/native-object-ui.js'),'utf8');
  assert.match(src,/prior\.footage=root\.NemoNativeTimeRemapUI\.mountInspector\(root,prior\.section,/);
  assert.match(src,/if\(prior\.footage\)prior\.footage\.retire\(\);/);
});

test('the native Time Remap and replace notices are localized in all four locales',()=>{
  const src=require('node:fs').readFileSync(require('node:path').join(__dirname,'../src/js/i18n.js'),'utf8');
  for(const key of ['toastNativeTimeRemapNotApplied','toastNativeTimeRemapValueUnavailable','titleNativeVideoReplaceUnavailable'])
    assert.equal(src.split(key+':').length-1,4,key);
  const ui=require('node:fs').readFileSync(require('node:path').join(__dirname,'../src/js/adapters/native-time-remap-ui.js'),'utf8');
  assert.equal(/showToast\('/.test(ui),false,'no unlocalized notice text');
});
