'use strict';
// Real Ordinary->Binding->Panel->Keys, with native read/session I/O and DOM modeled.
// This is a prospective UI projection test, never native/installed acceptance.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {createRequire}=require('node:module');
const Binding=require('../src/js/adapters/native-motion-property-binding.js');
const oracle=require('../engineering/application/examples/native-property-metadata-v1/cases.json');
const fixturePath=path.join(__dirname,'native-motion-property-panel.test.cjs'),fixtureSource=fs.readFileSync(fixturePath,'utf8');
const firstTest=fixtureSource.indexOf('\ntest(');assert.ok(firstTest>0);
const {document,tree}=new Function('require','__dirname',fixtureSource.slice(0,firstTest)+'\nreturn {document,tree};')(createRequire(fixturePath),__dirname);
const tick=()=>new Promise(setImmediate),copy=v=>JSON.parse(JSON.stringify(v));
async function fixture(options={}){
 const doc=document();doc.querySelector=()=>null;
 if(!('isConnected'in doc))Object.defineProperty(doc.constructor.prototype,'isConnected',{get(){return this.tagName==='#DOCUMENT'||!!this.parentNode&&this.parentNode.isConnected;}});
 const container=doc.createElement('div'),header=doc.createElement('div');doc.body.appendChild(header);doc.body.appendChild(container);
 let info={instanceId:'timeline-instance',documentId:'timeline-document',lifecycleGeneration:1,contentRevision:7,documentSnapshotId:'timeline-snapshot',totalFrames:96,layers:[{layerUid:'layer:opaque',name:'Blue',visible:true}]},ready=true;
 const root={document:doc,state:{appMode:'motion',activeLayerIdx:0,currentFrame:10,activeSymbolId:null},_layerSel:[0]},calls=[],writes=[],events=[];
 const target={contextId:'scene-root',layerUid:'layer:opaque'};
 const keys=options.keys||[{frame:-0.5,v:[1]},{frame:4.25,v:[2]},{frame:12,v:[3]}];
 const sample=oracle.cases.find(c=>c.documentVariant==='authored-facts'&&c.expectedResult.items.some(x=>x.property==='opacity'));
 assert.ok(sample);const property=copy(sample.expectedResult.items.find(x=>x.property==='opacity'));
 const categories=copy(oracle.cases.find(c=>c.expectedResult).expectedResult.unavailableCategories);
 const session={isActive:()=>true,blocksLegacy:()=>true,info:()=>info,readPresentedObjectSession:()=>ready?{...info,kind:'native-object',available:true,presentation:{...info,owner:'native',status:'presented',frame:root.state.currentFrame}}:null};
 const ui={currentSession:()=>session,projectionReady:value=>ready&&value===info,
  async readPropertyMetadata(q){calls.push(copy(q));const kind=q.view.kind;
   return {instanceId:q.instanceId,documentId:q.documentId,atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,kind,target:kind==='targets'?null:copy(q.view.target),items:kind==='targets'?[{kind:'context',contextId:target.contextId},{kind:'layer',target:copy(target)}]:[copy(property)],nextOffset:null,unavailableCategories:categories};},
  async readProperty(q){calls.push(copy(q));if(q.view.kind!=='authoredKeys')throw Error('Only authored-key read is modeled');
   return {instanceId:q.instanceId,documentId:q.documentId,atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,kind:'authoredKeys',target:copy(q.view.target),property:q.view.property,dimension:1,trackPresent:true,items:copy(keys.slice(q.view.offset,q.view.offset+8)),nextOffset:q.view.offset+8<keys.length?q.view.offset+8:null};},
  async editPropertyTrack(payload){writes.push(copy(payload));throw Error('Timeline selection must not dispatch property edits');}};
 if(options.readonly)delete ui.editPropertyTrack;
 const ordinary=Binding.createOrdinary(root,{readContextId:()=>target.contextId,acceptsTarget:()=>true,keyTimelineChanged:reason=>events.push(reason)});
 let latest=null,notifications=0;ordinary.setKeyTimelineListener(()=>{notifications++;latest=ordinary.readKeyTimeline();});
 assert.ok(ordinary.render(container,{ports:ui,header,isCurrent:()=>true}));
 async function settle(){for(let i=0;i<8;i++)await tick();}
 await settle();const keyButton=tree(container).find(n=>n.tagName==='BUTTON'&&n.textContent==='Keys');assert.ok(keyButton);
 return {root,doc,container,ordinary,events,calls,writes,session,target,settle,get latest(){return latest;},get notifications(){return notifications;},
  async open(){await keyButton.fire('click');await settle();},checks:()=>tree(container).filter(n=>n.type==='checkbox'),
  ready(v){ready=v;},replace(){info={...info,lifecycleGeneration:2};},retire(){ordinary.dispose();}};
}

test('actual ordinary property timeline publishes read and shared inspector selections without writer or legacy authority',async()=>{
 const h=await fixture();assert.equal(h.ordinary.readKeyTimeline(),null);await h.open();
 const projection=h.ordinary.readKeyTimeline();assert.ok(projection);assert.strictEqual(h.latest.property,'opacity');assert.deepEqual(copy(projection.target),h.target);
 assert.deepEqual(copy(projection.items),[{frame:-0.5,selected:false},{frame:4.25,selected:false},{frame:12,selected:false}]);
 const before=h.notifications;assert.equal(projection.selectFrames([4.25]),true);assert.ok(h.notifications>before);assert.equal(projection.isCurrent(),false);
 assert.deepEqual(h.checks().map(n=>n.checked),[false,true,false]);assert.deepEqual(copy(h.ordinary.captureSelectedKeys()).keys,[{target:{kind:'property',target:h.target,property:'opacity'},frame:4.25}]);
 const current=h.ordinary.readKeyTimeline();h.checks()[0].checked=true;await h.checks()[0].fire('change');
 assert.equal(current.isCurrent(),false);assert.deepEqual(copy(h.latest.items),[{frame:-0.5,selected:true},{frame:4.25,selected:true},{frame:12,selected:false}]);
 assert.equal(h.writes.length,0);h.retire();assert.equal(h.ordinary.readKeyTimeline(),null);assert.ok(h.events.includes('dispose'));
});

test('ordinary projection fences pending frame lifecycle and detached inspector changes without clearing stale selection',async()=>{
 for(const change of [h=>h.ready(false),h=>h.root.state.currentFrame++,h=>h.replace(),h=>h.root._layerSel=[],h=>h.container.replaceChildren()]){
  const h=await fixture();await h.open();let view=h.ordinary.readKeyTimeline();assert.equal(view.selectFrames([4.25]),true);view=h.ordinary.readKeyTimeline();
  const checks=h.checks();change(h);assert.equal(view.isCurrent(),false);assert.equal(h.ordinary.readKeyTimeline(),null);assert.equal(view.selectFrames([]),false);
  assert.deepEqual(checks.map(n=>n.checked),[false,true,false]);assert.equal(h.writes.length,0);h.retire();
 }
});

test('real ordinary full read retains35 keys while a33-key rectangle selection refuses atomically',async()=>{
 const h=await fixture({keys:Array.from({length:35},(_,i)=>({frame:i+0.25,v:[i]}))});await h.open();const view=h.ordinary.readKeyTimeline();
 assert.equal(view.items.length,35);assert.equal(view.selectFrames([0.25]),true);const current=h.ordinary.readKeyTimeline();
 assert.equal(current.selectFrames(Array.from({length:33},(_,i)=>i+0.25)),false);
 assert.equal(current.isCurrent(),true);assert.equal(h.checks().filter(n=>n.checked).length,1);assert.equal(h.writes.length,0);h.retire();
 const readonly=await fixture({readonly:true});await readonly.open();const shown=readonly.ordinary.readKeyTimeline();assert.ok(shown);assert.equal(shown.selectFrames([4.25]),false);assert.equal(readonly.checks().filter(n=>n.checked).length,0);readonly.retire();
});

// Native metadata and authored-key I/O are modeled; the Ordinary/Binding/Panel/
// inspector paths and checkbox/selection callbacks are the actual modules.
async function multiFixture(options={}){
 const doc=document();doc.querySelector=()=>null;
 if(!('isConnected'in doc))Object.defineProperty(doc.constructor.prototype,'isConnected',{get(){return this.tagName==='#DOCUMENT'||!!this.parentNode&&this.parentNode.isConnected;}});
 const container=doc.createElement('div'),header=doc.createElement('div');doc.body.appendChild(header);doc.body.appendChild(container);
 const layer={contextId:'scene-root',layerUid:'layer:opaque'},element={...layer,strokeId:'element:opaque'};
 const targets=[layer,element],properties=['opacity','position'];
 let info={instanceId:'multi-instance',documentId:'multi-document',lifecycleGeneration:1,contentRevision:7,documentSnapshotId:'multi-snapshot',totalFrames:96,layers:[{layerUid:layer.layerUid,name:'Blue',visible:true}]},ready=true,pending=false,held=null;
 const root={document:doc,state:{appMode:'motion',activeLayerIdx:0,currentFrame:10,activeSymbolId:null},_layerSel:[0]},writes=[],events=[],reads=[];
 const records=new Map(targets.flatMap((target,i)=>properties.map(property=>{
  const n=options.count||3;return [JSON.stringify([target,property]),Array.from({length:n},(_,j)=>({frame:j+0.25,v:property==='position'?[j+i,j-i]:[j+i]}))];})));
 const categories=copy(oracle.cases.find(c=>c.expectedResult).expectedResult.unavailableCategories);
 const authored=oracle.cases.filter(c=>c.documentVariant==='authored-facts').flatMap(c=>c.expectedResult.items);
 const rows=properties.map(property=>copy(authored.find(row=>row.property===property)));assert.ok(rows.every(Boolean));
 const session={isActive:()=>true,blocksLegacy:()=>true,info:()=>info,readPresentedObjectSession:()=>ready&&!pending?{...info,kind:'native-object',available:true,presentation:{...info,owner:'native',status:'presented',frame:root.state.currentFrame}}:null};
 const ui={currentSession:()=>session,projectionReady:value=>ready&&value===info,
  async readPropertyMetadata(q){return {instanceId:q.instanceId,documentId:q.documentId,atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,kind:q.view.kind,target:q.view.kind==='targets'?null:copy(q.view.target),items:q.view.kind==='targets'?[{kind:'context',contextId:layer.contextId},{kind:'layer',target:copy(layer)},{kind:'element',target:copy(element)}]:copy(rows),nextOffset:null,unavailableCategories:copy(categories)};},
  async readProperty(q){if(q.view.kind!=='authoredKeys')throw Error('Only authored-key I/O modeled');reads.push(copy(q));
   const items=records.get(JSON.stringify([q.view.target,q.view.property]));assert.ok(items);
   if(held&&held.property===q.view.property){const wait=held;await wait.promise;}
   return {instanceId:q.instanceId,documentId:q.documentId,atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,kind:'authoredKeys',target:copy(q.view.target),property:q.view.property,dimension:q.view.property==='position'?2:1,trackPresent:true,items:copy(items.slice(q.view.offset,q.view.offset+8)),nextOffset:q.view.offset+8<items.length?q.view.offset+8:null};},
  async editPropertyTrack(payload){writes.push(copy(payload));throw Error('Selection must not write native state');}};
 if(options.readonly)delete ui.editPropertyTrack;
 const ordinary=Binding.createOrdinary(root,{readContextId:()=>layer.contextId,readTargetSelection:()=>({primary:layer,selected:targets}),acceptsTarget:()=>true,keyTimelineChanged:reason=>events.push(reason)});
 let notices=0;ordinary.setKeyTimelineListener(()=>{notices++;ordinary.readKeyTimelines();});assert.ok(ordinary.render(container,{ports:ui,header,isCurrent:()=>true}));
 async function settle(){for(let i=0;i<8;i++)await tick();}await settle();
 function row(target,property){return tree(container).find(n=>n.className==='lrow motion-prop-row'&&n.dataset.property===property&&n.dataset.targetKey===JSON.stringify([target.contextId,target.layerUid,target.strokeId===undefined?null:target.strokeId]));}
 return {doc,root,container,ordinary,layer,element,targets,writes,events,reads,settle,get notices(){return notices;},
  open(target=layer,property='opacity'){const line=row(target,property);assert.ok(line);return tree(line).find(n=>n.tagName==='BUTTON'&&n.textContent==='Keys').fire('click');},
  inspectors:()=>tree(container).filter(n=>n.className==='native-motion-property-keys'&&!n.hidden),
  checks(index){return tree(this.inspectors()[index]).filter(n=>n.type==='checkbox');},
  hold(property){let resolve;const promise=new Promise(r=>resolve=r);held={property,promise,resolve};return ()=>{const wait=held;held=null;wait.resolve();};},
  ownAction(){const origin=session.readPresentedObjectSession();assert.ok(origin);pending=true;let running=true;
   return {lease:Object.freeze({readPresentedObjectSession:()=>running&&pending&&ready?origin:null}),retire(){running=false;pending=false;}};},
  publicPresentation:()=>session.readPresentedObjectSession(),
  ready(value){ready=value;},replace(){info={...info,lifecycleGeneration:info.lifecycleGeneration+1};},
  refresh(){ordinary.retire();assert.ok(ordinary.render(container,{ports:ui,header,isCurrent:()=>true}));return settle();},retire(){ordinary.dispose();}};
}
function address(target,property,frame){return {target:{kind:'property',target,property},frame};}
function selected(view){return view.tracks.flatMap(track=>track.items.filter(item=>item.selected).map(item=>address(copy(track.target),track.property,item.frame)));}

test('ordinary plural property timelines retain distinct inspectors and aggregate exact typed Shift addresses',async()=>{
 const h=await multiFixture();assert.equal(h.ordinary.readKeyTimelines(),null);await h.open();const first=h.inspectors()[0];await h.open(h.layer,'position');await h.open(h.element,'opacity');
 assert.strictEqual(h.inspectors()[0],first);assert.equal(h.inspectors().length,3);assert.equal(h.ordinary.readKeyTimeline().property,'opacity');assert.deepEqual(copy(h.ordinary.readKeyTimeline().target),h.element);
 const view=h.ordinary.readKeyTimelines();assert.ok(Object.isFrozen(view));assert.deepEqual(Object.keys(view),['pin','frame','viewGeneration','tracks']);assert.ok(Object.isFrozen(view.tracks));assert.ok(view.tracks.every(Object.isFrozen));
 const keys=[address(h.layer,'opacity',0.25),address(h.layer,'position',1.25),address(h.element,'opacity',2.25)];
 assert.equal(view.selectKeys(keys),true);assert.equal(view.isCurrent(),false);assert.deepEqual(copy(h.ordinary.captureSelectedKeys()).keys,keys);
 assert.deepEqual(copy(selected(h.ordinary.readKeyTimelines())),keys);assert.equal(h.writes.length,0);h.retire();
});

test('aggregate32 preflight redistributes selection atomically and checkbox33 refuses without truncation',async()=>{
 const h=await multiFixture({count:35});await h.open();await h.open(h.layer,'position');
 const a=Array.from({length:32},(_,i)=>address(h.layer,'opacity',i+0.25)),b=Array.from({length:32},(_,i)=>address(h.layer,'position',i+0.25));
 let view=h.ordinary.readKeyTimelines();assert.equal(view.selectKeys(a),true);view=h.ordinary.readKeyTimelines();
 const before=h.notices;assert.equal(view.selectKeys(b),true);assert.equal(h.notices,before+1);assert.deepEqual(copy(h.ordinary.captureSelectedKeys()).keys,b);
 assert.equal(h.checks(0).filter(n=>n.checked).length,0);assert.equal(h.checks(1).filter(n=>n.checked).length,32);
 view=h.ordinary.readKeyTimelines();assert.equal(view.selectKeys([address(h.layer,'opacity',0.25),...b]),false);assert.equal(view.isCurrent(),true);
 const check=h.checks(0)[0];check.checked=true;await check.fire('change');assert.equal(check.checked,false);assert.deepEqual(copy(h.ordinary.captureSelectedKeys()).keys,b);
 assert.equal(view.selectKeys([address(h.layer,'opacity',0.25),...b.slice(1)]),true);assert.equal(h.ordinary.captureSelectedKeys().keys.length,32);assert.equal(h.writes.length,0);h.retire();
});

test('malformed foreign duplicate and missing property addresses refuse whole aggregate selection',async()=>{
 const h=await multiFixture();await h.open();await h.open(h.layer,'position');const a=address(h.layer,'opacity',0.25);
 assert.equal(h.ordinary.readKeyTimelines().selectKeys([a]),true);
 const invalid=[[a,a],[{...a,extra:true}],[{target:{...a.target,extra:true},frame:0.25}],[address({...h.layer,extra:true},'opacity',0.25)],[address(h.layer,'scale',0.25)],[address(h.element,'opacity',0.25)],[address(h.layer,'position',99)],[address(h.layer,'position',Infinity)]];
 for(const keys of invalid){const view=h.ordinary.readKeyTimelines(),before=copy(selected(view));assert.equal(view.selectKeys(keys),false);assert.equal(view.isCurrent(),true);assert.deepEqual(copy(selected(h.ordinary.readKeyTimelines())),before);}
 assert.equal(h.writes.length,0);h.retire();
});

test('one pending track fences all property selection without erasing previously accepted checkboxes',async()=>{
 const h=await multiFixture();await h.open();assert.equal(h.ordinary.readKeyTimelines().selectKeys([address(h.layer,'opacity',0.25)]),true);
 const previous=h.ordinary.readKeyTimelines(),release=h.hold('position'),opening=h.open(h.layer,'position');await tick();
 assert.equal(previous.isCurrent(),false);assert.equal(h.ordinary.readKeyTimelines(),null);assert.equal(h.ordinary.captureSelectedKeys(),null);assert.equal(h.ordinary.readKeyTimeline(),null);
 const check=h.checks(0)[1];check.checked=true;await check.fire('change');assert.deepEqual(h.checks(0).map(n=>n.checked),[true,false,false]);
 release();await opening;assert.ok(h.ordinary.readKeyTimelines());assert.deepEqual(copy(h.ordinary.captureSelectedKeys()).keys,[address(h.layer,'opacity',0.25)]);h.retire();
});

test('plural capture fences checkbox frame owner and disposal while preserving per-track selection ownership',async()=>{
 for(const drift of [h=>h.ready(false),h=>h.root.state.currentFrame++,h=>h.replace()]){
  const h=await multiFixture();await h.open();await h.open(h.layer,'position');assert.equal(h.ordinary.readKeyTimelines().selectKeys([address(h.layer,'opacity',0.25)]),true);
  const capture=h.ordinary.captureSelectedKeys(),view=h.ordinary.readKeyTimelines();drift(h);
  assert.equal(capture.isCurrent(),false);assert.equal(view.isCurrent(),false);assert.equal(view.selectKeys([]),false);assert.equal(h.ordinary.readKeyTimelines(),null);assert.equal(h.checks(0)[0].checked,true);h.retire();
 }
 const h=await multiFixture();await h.open();await h.open(h.layer,'position');const capture=h.ordinary.captureSelectedKeys(),view=h.ordinary.readKeyTimelines();
 h.checks(1)[0].checked=true;await h.checks(1)[0].fire('change');assert.equal(capture.isCurrent(),false);assert.equal(view.isCurrent(),false);
 await h.refresh();assert.equal(h.ordinary.readKeyTimelines(),null);assert.equal(h.inspectors().length,0);await h.open();assert.equal(h.inspectors().length,1);assert.deepEqual(copy(h.ordinary.captureSelectedKeys()).keys,[]);
 const latest=h.ordinary.readKeyTimelines();h.retire();assert.equal(latest.isCurrent(),false);assert.equal(latest.selectKeys([]),false);assert.equal(h.ordinary.readKeyTimelines(),null);assert.equal(h.writes.length,0);
});

test('plural read-only projections never authorize aggregate selection and listener failure cannot create a writer',async()=>{
 const h=await multiFixture({readonly:true});await h.open();await h.open(h.layer,'position');const view=h.ordinary.readKeyTimelines();assert.ok(view);
 assert.equal(view.selectKeys([address(h.layer,'opacity',0.25)]),false);assert.equal(view.selectKeys([]),false);assert.deepEqual(copy(h.ordinary.captureSelectedKeys()).keys,[]);
 assert.equal(h.ordinary.setKeyTimelineListener(()=>{throw Error('UI observer failed');}),true);assert.equal(h.writes.length,0);h.retire();
});

// BEGIN REUSABLE REAL PROPERTY PANEL FIXTURE
// Extract this block without registering this file's tests. Native metadata and
// authored-key I/O are modeled; every inspector and aggregate is the real Panel.
function createPropertyPanel(options){
 const Panel=require('../src/js/adapters/native-motion-property-panel.js');
 const metadata=require('../engineering/application/examples/native-property-metadata-v1/cases.json');
 const clone=value=>JSON.parse(JSON.stringify(value));
 const tracks=options.tracks,targets=[];
 for(const track of tracks)if(!targets.some(target=>JSON.stringify(target)===JSON.stringify(track.target)))targets.push(track.target);
 const contextId=targets[0].contextId;
 const examples=metadata.cases.filter(c=>c.documentVariant==='authored-facts').flatMap(c=>c.expectedResult.items);
 const categories=clone(metadata.cases.find(c=>c.expectedResult).expectedResult.unavailableCategories);
 const panel=Panel.create(options.container,{document:options.document,readContext:options.readContext,
  readSelection:()=>targets[0],readSelections:()=>targets,readContextId:()=>contextId,keyTimelineChanged:options.keyTimelineChanged,
  async query(q){
   const candidates=q.view.kind==='targets'?[{kind:'context',contextId},...targets.map(target=>({kind:target.strokeId===undefined?'layer':'element',target:clone(target)}))]:tracks.filter(track=>JSON.stringify(track.target)===JSON.stringify(q.view.target)).map(track=>clone(examples.find(row=>row.property===track.property)));
   const items=candidates.slice(q.view.offset,q.view.offset+8);
   return {instanceId:q.instanceId,documentId:q.documentId,atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,kind:q.view.kind,target:q.view.kind==='targets'?null:clone(q.view.target),items,nextOffset:q.view.offset+8<candidates.length?q.view.offset+8:null,unavailableCategories:clone(categories)};
  },
  async readProperty(q){
   if(q.view.kind!=='authoredKeys')throw Error('Value reads not modeled by property-selection fixture');
   const track=tracks.find(track=>JSON.stringify(track.target)===JSON.stringify(q.view.target)&&track.property===q.view.property);
   if(!track)throw Error('Unmodeled authored track');
   return {instanceId:q.instanceId,documentId:q.documentId,atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,kind:'authoredKeys',target:clone(q.view.target),property:q.view.property,dimension:track.dimension,trackPresent:true,items:clone(track.items.slice(q.view.offset,q.view.offset+8)),nextOffset:q.view.offset+8<track.items.length?q.view.offset+8:null};
  },editPropertyTrack(){throw Error('Property-selection fixture must not dispatch edits');}});
 function descendants(node){return [node,...Array.from(node.children).flatMap(descendants)];}
 return {ready:panel.refresh(true),
  openProperty(target,property){
   const id=JSON.stringify([target.contextId,target.layerUid,target.strokeId===undefined?null:target.strokeId]);
   const row=descendants(options.container).find(node=>node.className==='lrow motion-prop-row'&&node.dataset.targetKey===id&&node.dataset.property===property);
   if(!row)throw Error('Real property row not present');
   const button=descendants(row).find(node=>node.tagName==='BUTTON'&&node.textContent==='Keys');
   if(typeof button.fire==='function')return button.fire('click');
   button.dispatchEvent(new options.document.defaultView.MouseEvent('click',{bubbles:true}));
  },readKeyTimeline:panel.readKeyTimeline,readKeyTimelines:panel.readKeyTimelines,captureSelectedKeys:panel.captureSelectedKeys,dispose:panel.dispose};
}
// END REUSABLE REAL PROPERTY PANEL FIXTURE

test('reusable real Panel fixture projects two properties for a shared native timeline consumer',async()=>{
 const doc=document(),container=doc.createElement('div');doc.body.appendChild(container);
 const pin={instanceId:'fixture-instance',documentId:'fixture-document',lifecycleGeneration:1,contentRevision:7,documentSnapshotId:'fixture-snapshot'},target={contextId:'scene-root',layerUid:'fixture-layer'};
 const readContext=()=>({pin,frame:5,viewGeneration:1,presented:{...pin,available:true,presentation:{...pin,owner:'native',status:'presented',frame:5}}});
 const h=createPropertyPanel({document:doc,container,readContext,tracks:[{target,property:'opacity',dimension:1,items:[{frame:2.5,v:[75]}]},{target,property:'position',dimension:2,items:[{frame:4.5,v:[3,7]}]}]});
 assert.equal(await h.ready,true);await h.openProperty(target,'opacity');await h.openProperty(target,'position');const view=h.readKeyTimelines();
 assert.deepEqual(view.tracks.map(track=>track.property),['opacity','position']);assert.equal(view.selectKeys([address(target,'opacity',2.5),address(target,'position',4.5)]),true);
 assert.deepEqual(copy(h.captureSelectedKeys()).keys,[address(target,'opacity',2.5),address(target,'position',4.5)]);h.dispose();assert.equal(view.isCurrent(),false);
});

test('real Binding propagates an own-action lease to plural selection while the public presentation stays fenced',async()=>{
 const h=await multiFixture();await h.open();await h.open(h.layer,'position');
 assert.equal(h.ordinary.readKeyTimelines().selectKeys([address(h.layer,'opacity',0.25)]),true);
 const capture=h.ordinary.captureSelectedKeys();assert.equal(capture.isCurrent(),true);assert.equal(capture.isSelectionCurrent(),true);
 const action=h.ownAction();assert.equal(h.publicPresentation(),null);
 assert.equal(capture.isSelectionCurrent(action.lease),true,'real Binding/Panel/Keys use only the supplied action-local presentation');
 assert.equal(capture.isCurrent(),false,'ordinary public context remains fenced');
 action.retire();assert.equal(capture.isSelectionCurrent(action.lease),false,'a retired lease cannot restore captured readiness');h.retire();
});

test('leased plural selection refuses real readiness loss, changed frame/pin/selection and disposal',async()=>{
 for(const drift of [h=>h.ready(false),h=>h.root.state.currentFrame++,h=>h.replace(),async h=>{h.checks(1)[0].checked=true;await h.checks(1)[0].fire('change');},h=>h.retire()]){
   const h=await multiFixture();await h.open();await h.open(h.layer,'position');const capture=h.ordinary.captureSelectedKeys();
   const action=h.ownAction();assert.equal(capture.isSelectionCurrent(action.lease),true);
   await drift(h);assert.equal(capture.isSelectionCurrent(action.lease),false);action.retire();h.retire();
 }
});
