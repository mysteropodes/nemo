'use strict';
// Real layer-inout handlers + Range/Desktop/Intent/Project/SDK. Only native I/O and DOM are modeled.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const Selection=require('../src/js/layer-inout-selection.js'),Range=require('../src/js/adapters/native-layer-range-desktop.js'),Desktop=require('../src/js/adapters/native-frame-edit-desktop.js');
const Intent=require('../src/js/adapters/native-frame-edit-intent.js'),Project=require('../src/js/adapters/native-object-project.js'),SDK=require('../src/js/adapters/native-application.js');
const turn=()=>new Promise(setImmediate),OP='command.document.object.layer.range.drag';
class Node {
 constructor(tag='div'){this.tagName=tag.toUpperCase();this.children=[];this.parentNode=null;this.style={};this.dataset={};this.attrs={};this.events=new Map();this.classes=new Set();this.className='';this.classList={toggle:(k,on)=>on?this.classes.add(k):this.classes.delete(k),add:k=>this.classes.add(k),remove:k=>this.classes.delete(k)};}
 appendChild(n){if(n.parentNode)n.parentNode.removeChild(n);this.children.push(n);n.parentNode=this;return n;}
 removeChild(n){this.children.splice(this.children.indexOf(n),1);n.parentNode=null;return n;}
 replaceChildren(...nodes){this.children.forEach(n=>n.parentNode=null);this.children=[];nodes.forEach(n=>this.appendChild(n));}
 setAttribute(k,v){this.attrs[k]=String(v);}
 addEventListener(k,fn){this.events.set(k,[...(this.events.get(k)||[]),fn]);}
 get parentElement(){return this.parentNode;}
 querySelectorAll(selector){return this.children.flatMap(n=>(selector[0]==='.'&&selector.slice(1).split('.').every(k=>n.className.split(' ').includes(k))?[n]:[]).concat(n.querySelectorAll(selector)));}
 querySelector(selector){for(const n of this.children){if(selector[0]==='.'&&selector.slice(1).split('.').every(k=>n.className.split(' ').includes(k)))return n;const found=n.querySelector(selector);if(found)return found;}return null;}
 fire(type,extra={}){const event={type,target:this,clientX:100,preventDefault(){},stopPropagation(){},...extra};for(const f of this.events.get(type)||[])f(event);}
}
function deferred(){let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};}
const source=fs.readFileSync(path.join(__dirname,'../src/js/layer-inout.js'),'utf8');
function actualHandlers(root,legacy){
 const start=source.indexOf('  function onDown('),end=source.indexOf('\n  function isTimeLinkDescendant',start);
 const a=source.indexOf('  function buildBar('),b=source.indexOf('\n  // Alt-reveal',a);assert.ok(start>0&&end>start&&a>0&&b>a);
 const context=vm.createContext({window:root,state:root.state,document:root.document,nativeRangeMode:()=>true,nativeRangeUnavailable:legacy,pushUndo:legacy,saveAllLayerFrames:legacy,_rowLayerIdx:new WeakMap(),_liToRow:{},_barSel:[],HANDLE_TOLERANCE_PX:6});
 const sel=source.indexOf('  function applySelClasses('),selEnd=source.indexOf('  var _marquee',sel);
 const part=source.indexOf('  function partForBoxHit('),partEnd=source.indexOf('  function updateMarquee(',part);
 vm.runInContext(source.slice(sel,selEnd)+'\n'+source.slice(part,partEnd)+'\n'+source.slice(start,end)+'\n'+source.slice(a,b),context);
 return {onDown:context.onDown,buildBar:context.buildBar,marqueeSelect:context.applyMarqueeSelection,readBars:()=>context._barSel,writeBars:v=>{context._barSel=v;}};
}
async function harness(options={}){
 let active=false,revision=0,gate=null,legacyKeys=[],localKeys=[],keyEpoch=0,barSel=[],legacy=0;
 const doc=new Node('document');doc.createElement=tag=>new Node(tag);doc.body=doc.createElement('body');doc.appendChild(doc.body);const grid=doc.createElement('div');doc.body.appendChild(grid);grid.appendChild(doc.createElement('div'));
 const root={document:doc,NemoLayerInOutSelection:Selection,FC:10,state:{currentFrame:2,activeLayerIdx:0,totalFrames:12,appMode:'motion',activeSymbolId:null,activeCompositionId:null,layers:[{layerUid:'alpha'},{layerUid:'beta'}]},_layerSel:[],_sel:{frames:[]},events:new Map(),notices:[]};
 root.addEventListener=(k,f)=>root.events.set(k,f);root.showToast=m=>root.notices.push(m);
 for(const layer of root.state.layers)for(const field of ['inPoint','outPoint','frames','timeLink'])Object.defineProperty(layer,field,{get(){legacy++;throw Error('LEGACY '+field);}});
 const calls=[],info=()=>({apiVersion:2,instanceId:'range-instance',documentId:'range-document',lifecycleGeneration:1,contentRevision:revision,documentSnapshotId:'range-snapshot-'+revision,
  documentDigest:String(revision).repeat(64),byteCount:1200,canvasW:320,canvasH:180,fps:24,totalFrames:12,layers:[{layerUid:'alpha'},{layerUid:'beta'}],reference:null,undoDepth:revision,redoDepth:0});
 const resolved={rawInPoint:-2.5,rawOutPoint:14,inPoint:0,outPoint:11,hasCustomRange:true,hasTimeLink:false};
 async function invoke(command,args){const q=args&&(args.request||JSON.parse(args.requestJson));calls.push({command,q});
  if(command==='nemo_native_object_client_status')return {...info(),available:active};
  if(command==='nemo_native_object_bootstrap'){active=true;return {...info(),requestId:q.requestId,resourceCount:0,viewportAvailable:false};}
  if(command==='nemo_native_object_reference')return {...info(),requestId:q.requestId};
  if(command==='nemo_native_object_preview')return {status:'presented',workId:'work-'+revision,viewGeneration:1};
  assert.equal(command,'nemo_native_dispatch');
  if(q.operation==='query.document.object.layer.timing'){
   assert.equal(q.payload.view,'range');const {view,...selector}=q.payload;
   return {apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:revision,ok:true,
    result:{kind:'layerRange',instanceId:q.instanceId,documentId:q.documentId,...selector,range:{...resolved}}};
  }
  assert.equal(q.operation,OP);if(gate)await gate.promise;revision++;
  return {apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:revision,ok:true,result:{applied:true,historyEntriesAdded:1}};
 }
 const s=Project.create({invoke,frame:()=>root.state.currentFrame,projectionReady:()=>true,changed:()=>{},publishRevision:()=>true,
  mapping:i=>({cssBounds:{x:0,y:0,width:320,height:180},physicalExtent:{width:320,height:180},compositionExtent:{width:i.canvasW,height:i.canvasH},reportedDpr:1}),
  subscribeRevisions:async()=>({instanceId:'range-instance',documentId:'range-document',lifecycleGeneration:1,contentRevision:revision,subscriptionId:'ranges'}),
  dispatchApplication:q=>SDK.createNativeApplicationAdapter('range-test',{dispatch:value=>invoke('nemo_native_dispatch',{request:value})}).dispatch(q)});
 Object.assign(root,{NemoNativeObjectProject:{desktop:()=>s},NemoNativeObjectProjectFile:{projectionReadyForUI:()=>true},NemoNativeLayerRangeDesktop:Range,NemoNativeFrameEditDesktop:Desktop,NemoNativeFrameEditIntent:Intent,NemoNativeApplicationAdapter:SDK,
  NemoNativeObjectUI:{readSelectedMotionKeys(r,pin){if(options.staleKeys)return null;const epoch=keyEpoch,v={pin,frame:root.state.currentFrame,viewGeneration:epoch,keys:structuredClone(localKeys)};Object.defineProperty(v,'isCurrent',{value:()=>epoch===keyEpoch});return Object.freeze(v);}},
  SMMotion:{getKeySelection:()=>legacyKeys}});
 const legacyFn=()=>{legacy++;throw Error('LEGACY WRITER');},actual=actualHandlers(root,legacyFn);
 root.SMLayerInOut={...actual,getBarSelection:()=>Array.from(actual.readBars(),v=>({...v})),setBarSelection(v){barSel=v.slice();actual.writeBars(v.slice());}};
 const ports={barUi:root.SMLayerInOut,readLegacyMotionSelection:()=>root.SMMotion.getKeySelection(),readSelectedMotionKeys:pin=>root.NemoNativeObjectUI.readSelectedMotionKeys(root,pin),selectLayer(i,choice){root.state.activeLayerIdx=i;root._layerSel=choice&&choice.mode==='toggle'?(root._layerSel.includes(i)?root._layerSel.filter(v=>v!==i):root._layerSel.concat(i)):[i];return true;}};
 const controller=Desktop.install(root);await s.openPath('/range.json');await s.presentPreview(2);Range.mount(root,grid,s.info(),ports);await turn();await turn();
 return {root,doc,grid,s,controller,calls,info,ports,commands:()=>calls.filter(c=>c.q?.operation===OP),legacy:()=>legacy,
  rows:()=>grid.children[1].children,bar:i=>grid.children[1].children[i].querySelector('.layer-inout-bar'),
  down(i=0,part='both',event={}){actual.onDown(i,this.rows()[i],part,{clientX:100,preventDefault(){},stopPropagation(){},...event});},
  move:x=>doc.fire('mousemove',{clientX:x}),up:x=>doc.fire('mouseup',{clientX:x}),
  setLegacyKeys(v){legacyKeys=v;},setNativeKeys(v){localKeys=v;keyEpoch++;},setBars(v){root.SMLayerInOut.setBarSelection(v);},hold(){gate=deferred();return ()=>{gate.resolve();gate=null;};},
  async settle(){for(let i=0;i<30&&controller.isPending();i++)await turn();assert.equal(controller.isPending(),false);await turn();await turn();}};
}

test('real layer-inout native mount and handles render only qualified raw D27 points, never legacy ranges',async()=>{
 const h=await harness();assert.equal(h.rows().length,2);assert.equal(h.bar(0).style.left,'-25px');assert.equal(h.bar(0).style.width,'175px');assert.equal(h.bar(0).attrs['aria-disabled'],'false');
 assert.equal(h.legacy(),0);assert.equal(h.commands().length,0);assert.equal(h.calls.filter(c=>c.q?.payload?.view==='range').length,2);
 const keys=Object.keys(h.root.state.layers[0]);assert.ok(!keys.includes('range'));assert.ok(!h.bar(0).querySelector('.layer-inout-timelink-anchor'));
});

test('actual bar body reaches existing queue once with every ordered quantized sample and one true1 checkpoint',async()=>{
 const h=await harness();h.bar(0).fire('mousedown');h.move(100);h.move(130);h.move(110);h.move(110);h.up(110);await h.settle();
 assert.equal(h.commands().length,1);assert.deepEqual(h.commands()[0].q.payload,{command:'object.layer.range.drag',contextId:'scene-root',layerUid:'alpha',activeLayerUid:'alpha',part:'both',barSelection:[],frame:2,selectedKeys:[],samples:[0,3,1,1],alt:false,plainClick:false});
 assert.equal(h.s.info().contentRevision,1);assert.equal(h.s.readPresentedObjectSession().contentRevision,1);assert.equal(h.root.state.currentFrame,2);assert.equal(h.legacy(),0);
 const click=await harness();click.down(1,'in');click.up(100);await click.settle();assert.equal(click.commands().length,1);assert.equal(click.commands()[0].q.payload.plainClick,true);assert.deepEqual(click.commands()[0].q.payload.samples,[]);
 assert.equal(click.s.info().contentRevision,1);assert.equal(click.root.state.activeLayerIdx,1);assert.equal(click.legacy(),0);
});

test('Ctrl/Cmd precedence and Shift bar selection never checkpoint or invoke legacy selection writers',async()=>{
 const h=await harness();h.down(0,'in',{ctrlKey:true,shiftKey:true});h.down(1,'out',{shiftKey:true});
 assert.deepEqual(h.root.SMLayerInOut.getBarSelection(),[{li:0,part:'both'},{li:1,part:'both'}]);assert.equal(h.commands().length,0);assert.equal(h.legacy(),0);
 h.down(1,'both');h.move(120);h.up(120);await h.settle();assert.deepEqual(h.commands()[0].q.payload.barSelection,[{layerUid:'alpha',part:'both'},{layerUid:'beta',part:'both'}]);
});

test('actual qualified selected property keys are sent, while legacy refs or stale native selection fail closed',async()=>{
 const h=await harness();const selected={target:{kind:'property',target:{contextId:'scene-root',layerUid:'alpha',strokeId:'stroke-a'},property:'opacity'},frame:2.25};
 h.setNativeKeys([selected]);h.down();h.move(120);h.up(120);await h.settle();assert.deepEqual(h.commands()[0].q.payload.selectedKeys,[selected]);
 const old=await harness();old.setLegacyKeys([{holder:{},prop:'opacity',key:{frame:0}}]);old.down();old.up(120);await old.settle();assert.equal(old.commands().length,0);assert.ok(old.root.notices.length);assert.equal(old.legacy(),0);
 const stale=await harness({staleKeys:true});stale.down();stale.up(120);await stale.settle();assert.equal(stale.commands().length,0);
});

test('prequeue selection drift or cancelled capture refuses; delayed receipt does not overwrite newer UI selection',async()=>{
 const stale=await harness();stale.down();stale.move(120);stale.up(120);stale.root._layerSel=[1];await stale.settle();assert.equal(stale.commands().length,0);
 const cancelled=await harness();cancelled.down();cancelled.move(120);cancelled.root.events.get('blur')();cancelled.up(120);await cancelled.settle();assert.equal(cancelled.commands().length,0);
 const held=await harness();const release=held.hold();held.down();held.move(120);held.up(120);await turn();assert.equal(held.commands().length,1);assert.equal(held.s.readPresentedObjectSession(),null);
 held.root._layerSel=[1];held.root._sel.frames=[{layer:1,frame:5}];release();await held.settle();assert.deepEqual(held.root._layerSel,[1]);assert.deepEqual(held.root._sel.frames,[{layer:1,frame:5}]);assert.equal(held.s.info().contentRevision,1);assert.equal(held.legacy(),0);
});

test('long gestures refuse without dropping samples and pointer FC/context drift never falls back',async()=>{
 for(const count of [4096,4097]){const h=await harness();h.down();for(let i=0;i<count;i++)h.move(100);h.up(120);await h.settle();assert.equal(h.commands().length,0);assert.ok(h.root.notices.length);assert.equal(h.legacy(),0);}
 for(const change of [h=>h.root.FC=20,h=>h.root.state.activeSymbolId='symbol',h=>h.root.state.currentFrame=3,h=>Range.retire(h.root)]){
  const h=await harness();h.down();change(h);h.move(120);h.up(120);await h.settle();assert.equal(h.commands().length,0);assert.equal(h.legacy(),0);
 }
});


test('ordinary Drawing bar uses known-cleared native selection without Motion click semantics',async()=>{
 const h=await harness();h.root.state.appMode='drawing';h.down(0,'out');h.move(120);h.up(120);await h.settle();
 assert.equal(h.commands().length,1);assert.deepEqual(h.commands()[0].q.payload.selectedKeys,[]);
 assert.equal(h.commands()[0].q.payload.plainClick,false);assert.equal(h.s.info().contentRevision,1);assert.equal(h.legacy(),0);
});


test('actual native row marquee authors mixed in/out handle selection and group drag without legacy key writers',async()=>{
 const h=await harness();
 function geometry(i,left,right){const bar=h.bar(i),top=i*22;bar.getBoundingClientRect=()=>({left,right,top,bottom:top+20});
  bar.querySelector('.layer-inout-handle.left').getBoundingClientRect=()=>({left,right:left+4,top,bottom:top+20});
  bar.querySelector('.layer-inout-handle.right').getBoundingClientRect=()=>({left:right-4,right,top,bottom:top+20});}
 geometry(0,100,300);geometry(1,-100,110);
 h.rows()[0].fire('mousedown',{clientX:95,clientY:0});h.doc.fire('mousemove',{clientX:115,clientY:43});h.doc.fire('mouseup',{clientX:115,clientY:43});
 assert.deepEqual(h.root.SMLayerInOut.getBarSelection(),[{li:0,part:'in'},{li:1,part:'out'}]);assert.equal(h.commands().length,0);
 h.down(0,'in');h.move(120);h.up(120);await h.settle();assert.equal(h.commands().length,1);
 assert.deepEqual(h.commands()[0].q.payload.barSelection,[{layerUid:'alpha',part:'in'},{layerUid:'beta',part:'out'}]);assert.equal(h.legacy(),0);
 const drift=await harness();drift.rows()[0].fire('mousedown',{clientY:0});drift.root.FC=11;drift.doc.fire('mousemove',{clientX:200,clientY:30});
 assert.deepEqual(drift.root.SMLayerInOut.getBarSelection(),[]);assert.equal(drift.commands().length,0);assert.ok(drift.root.notices.length);
});
