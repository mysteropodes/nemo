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

test('motion.js routes the native Time Remap row to the key intent before any legacy writer',()=>{
  const src=require('node:fs').readFileSync(require('node:path').join(__dirname,'../src/js/motion.js'),'utf8');
  assert.match(src,/holder\.timeRemap \|\| nativeRemapFacts\(holder\)/);
  const intents=src.match(/if \(remapFacts\) \{ window\.SM\.timeRemapKeyIntent\(state\.layers\.indexOf\(holder\), (null|nv)\); return; \}\n\s*pushUndo\(\);/g)||[];
  assert.equal(intents.length,2,'stopwatch and value field both return before pushUndo');
});

test('the footage panel reads the native video placement and never reaches the legacy relink writer',async()=>{
  const f=fixture(video());f.api.bindTimeRemapEntries();
  assert.equal(f.root.SM.nativeVideoPlacement(1),null);await settle();
  assert.equal(f.root.SM.nativeVideoPlacement(1).placementId,'p1');assert.equal(f.root.SM.nativeVideoPlacement(0),null);
  const src=require('node:fs').readFileSync(require('node:path').join(__dirname,'../src/js/timeline.js'),'utf8');
  const panel=src.slice(src.indexOf('function updateFootagePanel(){'),src.indexOf('function updateCompInstancePanel(){'));
  assert.match(panel,/SMLayerKind\.of\(nativePlacement\?\{nativeVideo:true\}:ld\)/);
  assert.match(panel,/!!\(ld\.nativeVideo\|\|nativePlacement\)&&window\.SM\.isTimeRemapped/);
  assert.match(panel,/if\(nativePlacement\)\{repBtn\.disabled=true;/,'replace reports unavailable for a native placement');
});

// Production motion.js panel and grid over the real binding; DOM nodes are modelled.
function motionHarness(placement){
  const {loadMotion,defaultState}=require('./fixtures/lib/sandbox.cjs');
  const nodes=[];
  function element(tag){
    const node={tag,children:[],dataset:{},style:{setProperty(){},removeProperty(){}},attributes:{},listeners:{},className:'',insertBefore(c){return this.appendChild(c);},
      classList:{add(c){node.className=(node.className+' '+c).trim();},remove(c){node.className=String(node.className).split(/\s+/).filter(x=>x!==c).join(' ');},
        toggle(){},contains(c){return String(node.className).split(/\s+/).includes(c);}},
      addEventListener(type,listener){(this.listeners[type]||=[]).push(listener);},
      appendChild(child){child.parentElement=this;this.children.push(child);return child;},
      setAttribute(name,value){this.attributes[name]=value;},removeAttribute(name){delete this.attributes[name];},
      closest(){return null;},querySelector(){return null;},querySelectorAll(){return [];},
      getBoundingClientRect(){return {height:24,width:24,left:0,right:24,top:0,bottom:24};}};
    nodes.push(node);return node;
  }
  const state={...defaultState(),appMode:'motion'};
  const motion=loadMotion(state,{beforeMotion(sb){
    sb.document.createElement=element;sb.document.createElementNS=(_ns,tag)=>element(tag);
    sb.document.readyState='complete';sb._layerIndexByUid=()=>-1;sb.FC=20;
    for(const k of ['ICO_3D','ICO_DUP','ICO_EYE','ICO_EYE_CLOSED','ICO_LOCK','ICO_UNLOCK'])sb[k]='';
    for(const k of ['getEffectiveStrokes'])sb[k]=()=>[];
    sb.pushUndo=()=>{throw new Error('a native Time Remap row must not push JS undo');};sb.NemoNativeTimeRemapUI=Remap;
  }});
  const f=fixture(placement,motion.sandbox);f.api.bindTimeRemapEntries();
  const sb=motion.sandbox;sb.state.appMode='motion';sb.state.activeLayerIdx=1;sb._layerSel=[1];sb._motionExpandedLayer=1;
  const panel=element('panel'),grid=element('grid'),wrap=element('wrap');
  Object.defineProperty(panel,'innerHTML',{set(){this.children.length=0;}});
  sb.document.getElementById=id=>id==='motion-props-body'?panel:id==='fg-wrap'?wrap:null;
  const all=node=>[node].concat(node.children.flatMap(all));
  function render(){
    panel.children.length=0;grid.children.length=0;
    motion.SMMotion.renderLayerListMotion(panel);motion.SMMotion.renderTimelineMotion(grid);
    const rows=all(panel).filter(n=>n._smProp&&String(n.className).includes('motion-prop-row'));
    const tracks=all(grid).filter(n=>String(n.className).includes('motion-track-row'));
    const remapRow=rows.find(n=>n._smProp==='timeRemap');
    return {rows,tracks,remapRow,field:remapRow&&all(remapRow).find(n=>n.tag==='input'),
      diamonds:all(grid).filter(n=>n.className==='native-remap-diamond')};
  }
  return {f,sb,motion,render};
}

test('the production panel and grid keep one aligned Time Remap row across a frame change, with the read value and diamonds',async()=>{
  const h=motionHarness(video({timeRemap:remap(KEYS,10,19.6)}));
  h.h=h.render();await settle();
  const a=h.render();
  assert.ok(a.remapRow,'the Time Remap row is listed for the remapped native placement');
  assert.equal(a.rows.length,a.tracks.length,'panel and grid rows stay identical');
  assert.equal(String(a.field.value),'19.6');assert.equal(a.field.dataset.nativeScrub,'commit');
  assert.equal(a.diamonds.length,2);
  h.f.frame(24);h.f.setFacts(video({timeRemap:remap(KEYS,24,47)}));
  const pending=h.render();
  assert.ok(pending.remapRow,'the row stays while the frame is read');
  assert.equal(pending.rows.length,pending.tracks.length);assert.equal(pending.rows.length,a.rows.length);
  assert.equal(String(pending.field.value),'',"an unread value is shown as unknown");
  assert.equal(pending.field.classList.contains('scrub'),false,'an unread value cannot be dragged');
  await settle();
  const b=h.render();
  assert.equal(b.rows.length,a.rows.length);assert.equal(b.tracks.length,a.tracks.length);
  assert.equal(String(b.field.value),'47');assert.equal(b.diamonds.length,2);
  assert.equal(h.f.calls.length,0,'rendering dispatches nothing');
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
  const h=motionHarness(video({timeRemap:remap(KEYS,10,19.6)}));
  h.render();await settle();
  const {field}=h.render(),fire=scrubHarness();
  field.closest=sel=>sel==='input.scrub'&&field.classList.contains('scrub')?field:null;
  field.setPointerCapture=()=>{};field.focus=()=>{};field.select=()=>{};field.min='';field.max='';
  field.dispatchEvent=event=>{for(const l of field.listeners[event.type]||[])l(event);};
  fire('pointerdown',field,100);
  for(const x of [120,140,160,180])fire('pointermove',field,x);
  assert.equal(h.f.calls.length,0,'no edit while dragging');
  fire('pointerup',field,180);
  await settle();
  assert.equal(h.f.calls.length,1,'one release, one native edit');
  assert.deepEqual(h.f.calls[0].payload,{kind:'setTimeRemapKey',placementId:'p1',frame:10,value:40},'20 whole steps from the read 19.6');
  assert.equal(String(field.value),'19.6','the tentative field rolls back until the native projection publishes');
  assert.equal(h.f.effects.includes('legacy'),false);
});

test('the native Time Remap and replace notices are localized in all four locales',()=>{
  const src=require('node:fs').readFileSync(require('node:path').join(__dirname,'../src/js/i18n.js'),'utf8');
  for(const key of ['toastNativeTimeRemapNotApplied','toastNativeTimeRemapValueUnavailable','titleNativeVideoReplaceUnavailable'])
    assert.equal(src.split(key+':').length-1,4,key);
  const ui=require('node:fs').readFileSync(require('node:path').join(__dirname,'../src/js/adapters/native-time-remap-ui.js'),'utf8');
  assert.equal(/showToast\('/.test(ui),false,'no unlocalized notice text');
});
