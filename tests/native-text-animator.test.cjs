'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Model=require('../src/js/adapters/native-text-animator.js');
const Desktop=require('../src/js/adapters/native-text-animator-desktop.js');
const Item=require('../src/js/adapters/native-object-item-contract.js');
const turn=()=>new Promise(setImmediate),copy=v=>JSON.parse(JSON.stringify(v));
const selector={selectorType:'range',units:'percent',basedOn:'chars',start:0,end:100,offset:0,shape:1,amount:100,easeHigh:0,easeLow:0,smooth:100,wiggleMin:-50,wiggleMax:50,wigglesPerSec:2,correlation:50,seed:1};
class Element{
 constructor(tag){this.tagName=tag;this.children=[];this.parentNode=null;this.listeners={};this.style={};this.attributes={};this.value='';this.textContent='';}
 appendChild(n){n.parentNode=this;this.children.push(n);return n;}prepend(n){n.parentNode=this;this.children.unshift(n);}
 replaceChildren(...ns){this.children.forEach(n=>n.parentNode=null);this.children=[];ns.forEach(n=>this.appendChild(n));}
 setAttribute(k,v){this.attributes[k]=String(v);}addEventListener(k,f){(this.listeners[k]||=[]).push(f);}fire(k){for(const fn of this.listeners[k]||[])fn({target:this});}
 contains(n){return this===n||this.children.some(c=>c.contains(n));}
}
function fixture(){
 let revision=0,selected='a',serial=0,records=[{id:'ta-1',selector:{},props:{position:[20,0]}}];const commands=[];
 function pin(){return {instanceId:'instance',documentId:'document',lifecycleGeneration:1,contentRevision:revision,documentSnapshotId:'snapshot-'+revision,totalFrames:8,layers:[{layerUid:'L0',locked:false}]};}
 const session={info:pin,isActive:()=>true,blocksLegacy:()=>true,projectionReady:true,requestId:p=>p+'-'+(++serial),runResourceCommand:fn=>fn({reconcile:async()=>{},dispatch:async q=>{
  let result,contentRevision=revision;
  if(q.operation==='query.document.object.text'){
   const text=JSON.stringify({animators:records,selectors:records.map(()=>selector),hasText:true}),v=q.payload.view,part=Buffer.from(text).subarray(v.offset,v.offset+v.limit).toString();
   const end=v.offset+Buffer.byteLength(part);result={instanceId:'instance',documentId:'document',atRevision:revision,documentSnapshotId:'snapshot-'+revision,kind:'animators',layerUid:v.layerUid,frame:v.frame,textBytes:Buffer.byteLength(text),offset:v.offset,text:part,nextOffset:end===Buffer.byteLength(text)?null:end};
  }else{Item.payload(q.operation,q.payload);commands.push(copy(q.payload));revision++;contentRevision=revision;result={applied:true,historyEntriesAdded:1,operation:q.payload.command.slice(7),strokeId:q.payload.animator?q.payload.animator.id:q.payload.animatorId};}
  return {apiVersion:2,requestId:q.requestId,instanceId:'instance',documentId:'document',contentRevision,ok:true,result};
 }})};
 const context=()=>({pin:pin(),layerUid:'L0',frame:0,selectionKey:selected});
 const doc={body:new Element('body'),createElement:t=>new Element(t),getElementById:id=>find(doc.body,n=>n.id===id)};
 ['p-textanim-sec','p-textanim-add-row','p-textanim-empty','p-textanim-list','p-textanim-add'].forEach(id=>{const n=doc.createElement('div');n.id=id;doc.body.appendChild(n);});
 const root={document:doc,state:{appMode:'motion',currentFrame:0,activeLayerIdx:0,playing:false},NemoNativeObjectProject:{desktop:()=>session},NemoNativeTextAnimator:Model};
 return {session,context,commands,root,doc,advance(){revision++;},select(){selected='b';},setRecords(v){records=v;},input(label){return find(doc.body,n=>n.attributes['aria-label']===label);}};
}
function find(n,p){if(p(n))return n;for(const c of n.children){const x=find(c,p);if(x)return x;}return null;}
test('native animator caller sends one detached sealed intent and rejects stale origins',async()=>{
 const f=fixture(),m=Model.create(f.session,f.context),loaded=await m.load();loaded.animators[0].props.position[0]=30;
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
 const f=fixture();Desktop.render(f.root);await turn();const old=f.input('position X');assert.ok(old);f.advance();Desktop.render(f.root);await turn();const fresh=f.input('position X');assert.notEqual(old,fresh);
 old.value='99';old.fire('change');await turn();assert.equal(f.commands.length,0,'retired control must not publish');
 fresh.value='25';fresh.fire('change');await turn();assert.equal(f.commands.length,1,'current mounted control still works');assert.equal(f.commands[0].animator.props.position[0],25);
});
