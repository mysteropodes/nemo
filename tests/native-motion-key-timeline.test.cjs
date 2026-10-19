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
 assert.equal(ordinary.paint(ui,container,header),true);
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
