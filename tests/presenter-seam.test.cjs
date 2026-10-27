'use strict';
// CS5 (#2347): presenter seam contract, plus a replacement presenter mounted without editing a monolith.
const test=require('node:test'),assert=require('node:assert/strict');
const Seam=require('../src/js/application/presenter-seam.js');
const {CASES,render}=require('./native-layer-list-presenter.test.cjs');
const pin=(revision,doc='d')=>({instanceId:'i',documentId:doc,lifecycleGeneration:1,contentRevision:revision,documentSnapshotId:doc+'-'+revision});
const rootWith=nodes=>({document:{getElementById:id=>nodes[id]||null}});

test('publish notifies each subscriber once per revision identity and retains the latest read model',()=>{
  const root={},seen=[];const off=Seam.subscribe(root,(p,info)=>seen.push([p.contentRevision,info.tag]));
  assert.equal(Seam.publish(root,{...pin(1),tag:'a'}),true);assert.equal(Seam.publish(root,{...pin(1),tag:'b'}),false);
  assert.equal(Seam.current(root).info.tag,'b');assert.equal(Seam.publish(root,{...pin(2),tag:'c'}),true);
  assert.equal(Seam.publish(root,{...pin(2,'other'),tag:'d'}),true,'a replacement document is a new identity');
  assert.deepEqual(seen,[[1,'a'],[2,'c'],[2,'d']]);
  assert.equal(off(),true);assert.equal(off(),false);Seam.publish(root,pin(3));assert.equal(seen.length,3);
  assert.ok(Object.isFrozen(Seam.current(root).pin));
});

test('a failing subscriber does not starve the others and the failure is surfaced',()=>{
  const root={},seen=[];Seam.subscribe(root,()=>{throw Error('boom');});Seam.subscribe(root,p=>seen.push(p.contentRevision));
  assert.throws(()=>Seam.publish(root,pin(1)),/boom/);assert.deepEqual(seen,[1]);
  assert.throws(()=>Seam.subscribe(root,null),TypeError);
});

test('incomplete pins refuse and pinned queries read only the exact published revision',()=>{
  const root={};for(const bad of [null,{},{...pin(1),documentSnapshotId:undefined},{...pin(1),contentRevision:null}])assert.throws(()=>Seam.publish(root,bad),TypeError);
  assert.equal(Seam.current(root),null);assert.throws(()=>Seam.query(root,pin(1),()=>1),/stale/);
  Seam.publish(root,{...pin(4),value:7});
  assert.equal(Seam.query(root,pin(4),info=>info.value),7);
  assert.throws(()=>Seam.query(root,pin(3),()=>1),/stale/);assert.throws(()=>Seam.query(root,pin(4,'foreign'),()=>1),/stale/);
});

test('mount points are declared surfaces only, one presenter each, and present refuses without one',()=>{
  const list={id:'layer-list'},root=rootWith({'layer-list':list}),presenter={render:(node,ctx)=>({node,ctx})};
  assert.throws(()=>Seam.register(root,'toolbar',presenter),/Unknown presenter surface/);
  assert.throws(()=>Seam.register(root,'layer-list',{}),TypeError);
  assert.throws(()=>Seam.present(root,'layer-list',{}),/No presenter is mounted/);
  const off=Seam.register(root,'layer-list',presenter);
  assert.throws(()=>Seam.register(root,'layer-list',presenter),/already mounted/);
  assert.deepEqual(Seam.present(root,'layer-list',{n:1}),{node:list,ctx:{n:1}});
  assert.throws(()=>Seam.present(rootWith({}),'layer-list',{}),/No presenter is mounted/);
  const unmounted=rootWith({});Seam.register(unmounted,'frame-grid',presenter);
  assert.throws(()=>Seam.present(unmounted,'frame-grid',{}),/mount point is unavailable/);
  assert.equal(off(),true);assert.equal(off(),false);assert.equal(Seam.isMounted(root,'layer-list'),false);
  assert.deepEqual(Object.keys(Seam.SURFACES),['layer-list','frame-header','frame-grid','inspector','motion-properties']);
});

test('native rendering publishes its read model and a replacement layer-list presenter is used verbatim',()=>{
  const seen=[];let mounted=null;
  const replacement={render(list,ctx){mounted=list;return {rows:ctx.info.layers.map(layer=>{const row=ctx.document.createElement('div');
    row.textContent='custom '+layer.layerUid;row.replaceChildren(ctx.document.createElement('a'),ctx.document.createElement('a'),ctx.document.createElement('a'),ctx.document.createElement('a'));return row;})};}};
  const dom=render(CASES[0],root=>{Seam.register(root,'layer-list',replacement);Seam.subscribe(root,p=>seen.push(p.contentRevision));});
  assert.deepEqual(dom['layer-list'].children.map(r=>r.text),['custom opaque A:雪','custom B']);
  assert.equal(mounted.id,'layer-list');assert.deepEqual(seen,[3]);
});

test('presenter rows route pick, visibility and policy clicks with the row and the flipped flag',()=>{
  const LayerList=require('../src/js/adapters/native-layer-list-presenter.js'),calls=[];
  const doc={createElement:tag=>{const listeners={};return {tagName:tag,children:[],style:{},dataset:{},attributes:{},
    setAttribute(k,v){this.attributes[k]=v;},addEventListener(k,f){listeners[k]=f;},replaceChildren(...kids){this.children=kids;},
    click(event={}){return listeners.click(event);}};}};
  const layers=[{layerUid:'L1',name:'One',visible:true,locked:true},{layerUid:'L2',visible:false,shy:false}];
  const {rows}=LayerList.render({},{document:doc,info:{layers},selection:[1],shyMode:false,policyOwner:{},
    onPick:(i,layer,event,row)=>calls.push(['pick',i,layer.layerUid,event.shiftKey===true,row===rows[i]]),
    onVisibility:layer=>calls.push(['eye',layer.layerUid]),
    onPolicy:(layer,flag,value,row)=>calls.push(['policy',layer.layerUid,flag,value,row===rows.find(r=>r.dataset.layerUid===layer.layerUid)])});
  rows[0].children[0].click({shiftKey:true});rows[1].children[1].click();rows[0].children[2].click();rows[1].children[3].click();
  assert.deepEqual(calls,[['pick',0,'L1',true,true],['eye','L2'],['policy','L1','locked',false,true],['policy','L2','shy',true,true]]);
  assert.equal(rows[1].children[0].attributes['aria-pressed'],'true');assert.equal(rows[0].children[2].disabled,false);
  assert.throws(()=>LayerList.render({},{info:{layers}}),TypeError);
});

test('page mode refuses to paint, before touching the DOM, when the seam or presenter is missing',()=>{
  const fs=require('node:fs'),vm=require('node:vm'),source=fs.readFileSync(require.resolve('../src/js/adapters/native-object-ui.js'),'utf8');
  const context=vm.createContext({});vm.runInContext(source,context,{filename:'native-object-ui.js'});
  const UI=context.NemoNativeObjectUI;assert.equal(typeof UI.render,'function');
  const touched=[];const root={state:{currentFrame:0},document:{getElementById:id=>{touched.push(id);return null;}}};
  const ports={projectionReady:()=>true,currentSession:()=>null,selectLayer:()=>true,repaint:()=>true};
  const info={...pin(1),totalFrames:24,layers:[{layerUid:'L',visible:true}]};
  assert.throws(()=>UI.render(root,info,false,ports),/presenter seam/);assert.deepEqual(touched,[]);
});

test('a presenter whose rows do not match the read model is refused',()=>{
  assert.throws(()=>render(CASES[0],root=>Seam.register(root,'layer-list',{render:()=>({rows:[]})})),/do not match the read model/);
  assert.throws(()=>Seam.query({},{documentId:'x'},()=>1),/stale/);
});
