'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Model=require('../src/js/adapters/native-text-animator.js');
const Read=require('../src/js/adapters/native-text-read.js'),Command=require('../src/js/adapters/native-object-item-command.js');
const make=f=>Model.create(f.session,f.context,{reader:Read.create(f.session),command:Command.create(f.session,'item'),samePin:Read.samePin});
const Desktop=require('../src/js/adapters/native-text-animator-desktop.js');
const Item=require('../src/js/adapters/native-object-item-contract.js');
const turn=()=>new Promise(setImmediate),copy=v=>JSON.parse(JSON.stringify(v));
const selector={selectorType:'range',units:'percent',basedOn:'chars',start:0,end:100,offset:0,shape:1,amount:100,easeHigh:0,easeLow:0,smooth:100,wiggleMin:-50,wiggleMax:50,wigglesPerSec:2,correlation:50,seed:1};
class Element{
 constructor(tag){this.tagName=tag;this.children=[];this.parentNode=null;this.listeners={};this.style={};this.attributes={};this.value='';this.textContent='';}
 removeChild(n){this.children.splice(this.children.indexOf(n),1);n.parentNode=null;return n;}
 appendChild(n){n.parentNode=this;this.children.push(n);return n;}prepend(n){n.parentNode=this;this.children.unshift(n);}
 replaceChildren(...ns){this.children.forEach(n=>n.parentNode=null);this.children=[];ns.forEach(n=>this.appendChild(n));}
 setAttribute(k,v){this.attributes[k]=String(v);}addEventListener(k,f){(this.listeners[k]||=[]).push(f);}fire(k){for(const fn of this.listeners[k]||[])fn({target:this});}
 contains(n){return this===n||this.children.some(c=>c.contains(n));}
}
function fixture(){
 let evaluatedAvailable=true,revision=0,selected='a',serial=0,records=[{id:'ta-1',selector:{},props:{position:[20,0]}}];const commands=[];
 function pin(){return {instanceId:'instance',documentId:'document',lifecycleGeneration:1,contentRevision:revision,documentSnapshotId:'snapshot-'+revision,totalFrames:8,layers:[{layerUid:'L0',locked:false}]};}
 const session={info:pin,isActive:()=>true,blocksLegacy:()=>true,projectionReady:true,requestId:p=>p+'-'+(++serial),runResourceCommand:fn=>fn({reconcile:async()=>{},dispatch:async q=>{
  let result,contentRevision=revision;
  if(q.operation==='query.document.object.text'){
   const text=JSON.stringify({animators:records,selectors:records.map(()=>evaluatedAvailable?{available:true,value:selector}:{available:false,reason:"native selector expression unavailable"}),hasText:true}),v=q.payload.view,part=Buffer.from(text).subarray(v.offset,v.offset+v.limit).toString();
   const end=v.offset+Buffer.byteLength(part);result={instanceId:'instance',documentId:'document',atRevision:revision,documentSnapshotId:'snapshot-'+revision,kind:'animators',layerUid:v.layerUid,frame:v.frame,textBytes:Buffer.byteLength(text),offset:v.offset,text:part,nextOffset:end===Buffer.byteLength(text)?null:end};
  }else{Item.payload(q.operation,q.payload);commands.push(copy(q.payload));revision++;contentRevision=revision;result={applied:true,historyEntriesAdded:1,operation:q.payload.command.slice(7),strokeId:q.payload.animator?q.payload.animator.id:q.payload.animatorId};}
  return {apiVersion:2,requestId:q.requestId,instanceId:'instance',documentId:'document',contentRevision,ok:true,result};
 }})};
 const context=()=>({pin:pin(),layerUid:'L0',frame:0,selectionKey:selected});
 const doc={body:new Element('body'),createElement:t=>new Element(t),getElementById:id=>find(doc.body,n=>n.id===id)};
 ['p-textanim-sec','p-textanim-add-row','p-textanim-empty','p-textanim-list','p-textanim-add'].forEach(id=>{const n=doc.createElement('div');n.id=id;doc.body.appendChild(n);});
 const root={document:doc,state:{appMode:'motion',currentFrame:0,activeLayerIdx:0,playing:false},NemoNativeObjectProject:{desktop:()=>session},NemoNativeTextAnimator:Model,NemoNativeTextRead:Read,NemoNativeObjectItemCommand:Command};
 return {session,context,commands,root,doc,advance(){revision++;},select(){selected='b';},setRecords(v){records=v;},unavailable(){evaluatedAvailable=false;},input(label){return find(doc.body,n=>n.attributes['aria-label']===label);}};
}
function find(n,p){if(p(n))return n;for(const c of n.children){const x=find(c,p);if(x)return x;}return null;}
test('native animator caller sends one detached sealed intent and rejects stale origins',async()=>{
 const f=fixture(),m=make(f),loaded=await m.load();loaded.animators[0].props.position[0]=30;
 await m.upsert(loaded.animators[0],loaded.context);assert.equal(f.commands.length,1);assert.equal(f.commands[0].animator.props.position[0],30);
 await assert.rejects(()=>m.upsert(loaded.animators[0],loaded.context),e=>e.code==='stale_revision');assert.equal(f.commands.length,1);
});
test('native animator contract refuses nonfinite null-key duplicate tracks and unknown members',()=>{
 for(const change of [{properties:[{property:'amount',static:[null]}]},{props:{position:[1]}},{selector:{unknown:1}},{properties:[{property:'amount',static:[1]},{property:'amount',static:[2]}]}]){
  assert.throws(()=>Item.animator({id:'ta',selector:{},...change}));
 }
 assert.doesNotThrow(()=>Item.animator({id:'ta',selector:{amount:null},props:{scale:[0,0]}}));
});
test('retired ordinary animator controls cannot write their old record against a refreshed context',async()=>{
 const f=fixture();Desktop.render(f.root);await turn();const old=f.input('position X'),oldKey=f.input('Animate amount'),oldRemove=find(f.doc.body,n=>n.tagName==='button'&&n.textContent==='Remove');assert.ok(old);assert.ok(oldKey);assert.ok(oldRemove);f.advance();Desktop.render(f.root);await turn();const fresh=f.input('position X');assert.notEqual(old,fresh);
 old.value='99';old.fire('change');oldKey.fire('click');oldRemove.fire('click');await turn();assert.equal(f.commands.length,0,'retired control must not publish');
 fresh.value='25';fresh.fire('change');await turn();assert.equal(f.commands.length,1,'current mounted control still works');assert.deepEqual(f.commands[0].edit,{kind:'setProps',field:'position',value:[25,0]});
});
test('unavailable evaluated selectors keep raw animator disable and remove intents usable',async()=>{
 const f=fixture();f.unavailable();Desktop.render(f.root);await turn();assert.equal(f.input('amount'),null);
 const enabled=find(f.doc.body,n=>n.tagName==='input'&&n.type==='checkbox');assert.ok(enabled);enabled.checked=false;enabled.fire('change');await turn();
 assert.deepEqual(f.commands[0].edit,{kind:'setEnabled',enabled:false});
});
test('ordinary selector key edits stay bounded when the persisted animator has a long track',async()=>{
 const f=fixture(),record={id:'long',selector:{},props:{position:[20,0]},properties:[{property:'amount',track:{keys:Array.from({length:512},(_,i)=>({frame:i,v:[i%100]}))}}]};
 assert.ok(Buffer.byteLength(JSON.stringify(record))>4096);f.setRecords([record]);Desktop.render(f.root);await turn();
 const input=f.input('amount');assert.ok(input);input.value='75';input.fire('change');await turn();
 assert.deepEqual(f.commands[0].edit,{kind:'setValue',property:'amount',frame:0,values:[75]});assert.ok(Buffer.byteLength(JSON.stringify(f.commands[0]))<256);
});
test('native inspector owns a fresh visible animator mount and revokes retired mounted controls',async()=>{
 const f=fixture(),legacy=f.doc.getElementById('p-textanim-sec');legacy.hidden=true;legacy.style.display='none';
 const container=f.doc.createElement('section');container.id='native-object-inspector';f.doc.body.appendChild(container);
 const mounted=Desktop.mount(f.root,container);await turn();assert.equal(legacy.hidden,true);assert.equal(legacy.style.display,'none');assert.equal(container.children.filter(n=>n.id==='native-text-animators').length,1);
 const old=f.input('position X');assert.ok(old);assert.ok(container.contains(old));Desktop.mount(f.root,container);await turn();assert.equal(container.children.filter(n=>n.id==='native-text-animators').length,1);
 mounted.retire();old.value='999';old.fire('change');await turn();assert.equal(f.commands.length,0);assert.equal(container.children.length,0);
});

test("animator application refuses absent I/O ports without deriving adapter fallbacks",()=>{const f=fixture();assert.throws(()=>Model.create(f.session,f.context),e=>e.code==='unavailable');assert.equal(f.commands.length,0);});
