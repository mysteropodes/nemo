'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ROOT=process.env.NEMO_VALUE_SOURCE_ROOT||path.join(__dirname,'..');
const View=require(path.join(ROOT,'src/js/adapters/native-motion-property-view.js'));
const code=fs.readFileSync(path.join(__dirname,'../src/js/adapters/native-motion-property-value.js'),'utf8');
const moduleValue={exports:{}},legacyEffects={reads:0};
const sandbox={module:moduleValue,TextEncoder,require(name){assert.equal(name,'./native-motion-property-view.js');return View;}};
for(const name of ['state','paper','SMProject','undoHistory','evaluateExpr','getVal','lerp'])Object.defineProperty(sandbox,name,{get(){legacyEffects.reads++;throw Error('Legacy authority accessed: '+name);}});
vm.compileFunction(code,['module','require','TextEncoder'],{contextExtensions:[sandbox]})(moduleValue,sandbox.require,TextEncoder);
const Value=moduleValue.exports,copy=v=>JSON.parse(JSON.stringify(v));
class Node{
 constructor(doc,tag){this.ownerDocument=doc;this.tagName=tag.toUpperCase();this.children=[];this.parentNode=null;this.attrs={};this.dataset={};this.style={};this.listeners=new Map();this.value='';this.checked=false;this.disabled=false;this._text='';}
 set textContent(v){this.replaceChildren();this._text=String(v);}get textContent(){return this._text+this.children.map(n=>n.textContent).join('');}
 appendChild(n){if(n.parentNode)n.parentNode.removeChild(n);this.children.push(n);n.parentNode=this;return n;}
 removeChild(n){this.children.splice(this.children.indexOf(n),1);n.parentNode=null;return n;}
 replaceChildren(...nodes){this.children.forEach(n=>n.parentNode=null);this.children=[];this._text='';nodes.forEach(n=>this.appendChild(n));}
 setAttribute(k,v){this.attrs[k]=String(v);}
 addEventListener(type,fn){this.listeners.set(type,[...this.listeners.get(type)||[],fn]);}
 removeEventListener(type,fn){this.listeners.set(type,(this.listeners.get(type)||[]).filter(x=>x!==fn));}
 async fire(type,extra){for(const fn of [...this.listeners.get(type)||[]])await fn({target:this,type,...extra});}
 get isConnected(){return this.tagName==='#DOCUMENT'||!!this.parentNode&&this.parentNode.isConnected;}
}
function tree(n){return [n,...n.children.flatMap(tree)];}
function deferred(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};}
const tick=()=>new Promise(r=>setImmediate(r));
const target={contextId:'opaque scene:α',layerUid:'layer / value',strokeId:'stroke:β'};
function fixture(options={}){
 const doc=new Node(null,'#document');doc.defaultView={_scrubLiveActive:false};doc.createElement=t=>new Node(doc,t);const container=doc.createElement('div');doc.appendChild(container);
 let pin={instanceId:'instance-value',documentId:'document-value',lifecycleGeneration:1,contentRevision:7,documentSnapshotId:'native-object:document-value:7'},frame=10,generation=2,ready=true;
 const reads=[],writes=[],nativeValues=options.values||[0,64],descriptor={property:'position',dimension:nativeValues.length,animatable:options.animatable!==false,authored:{expressionEnabled:!!options.expression,keyCount:options.keyCount||0,hasStaticOverride:false}},keyFrames=options.keyFrames||[];
 function context(){return {pin,frame,viewGeneration:generation,presented:{...pin,available:ready,presentation:{...pin,owner:'native',status:ready?'presented':'pending',frame}}};}
 function response(q){if(q.view.kind==='authoredKeys')return {instanceId:q.instanceId,documentId:q.documentId,atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,kind:'authoredKeys',target:copy(q.view.target),property:q.view.property,dimension:nativeValues.length,trackPresent:true,items:keyFrames.slice(q.view.offset,q.view.offset+8).map(f=>({frame:f,v:nativeValues.slice()})),nextOffset:q.view.offset+8<keyFrames.length?q.view.offset+8:null};return {instanceId:q.instanceId,documentId:q.documentId,atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,kind:'rawSample',target:copy(q.view.target),property:q.view.property,dimension:nativeValues.length,frame:q.view.frame,value:nativeValues.slice()};}
 const ports={document:doc,readContext:context,readProperty(q){reads.push(copy(q));return options.reader?options.reader(q,response):Promise.resolve(response(q));},
  async editProperty(payload,expected,current){writes.push({payload:copy(payload),expected:copy(expected),current});assert.equal(current(),true);return options.writer?options.writer(payload,expected,current):{applied:true,historyEntriesAdded:1};}};
 if(options.readonly)delete ports.editProperty;
 if(options.openKeys)ports.openKeys=options.openKeys;
 const editor=Value.create(container,ports,target,descriptor);
 return {doc,win:doc.defaultView,container,ports,editor,reads,writes,context,response,nativeValues,
  inputs:()=>tree(editor.element).filter(n=>n.type==='number'),sw:()=>tree(editor.element).find(n=>/\bmotion-stopwatch\b/.test(n.className||'')),keyReads:()=>reads.filter(r=>r.view.kind==='authoredKeys'),status:()=>tree(editor.element).find(n=>n.attrs.role==='status').textContent,
  frame(v){frame=v;},generation(){generation++;},replace(){pin={...pin,lifecycleGeneration:pin.lifecycleGeneration+1};},revision(){pin={...pin,contentRevision:8,documentSnapshotId:'native-object:document-value:8'};generation++;},ready(v){ready=v;},foreign(){pin={...pin,documentId:'another-document'};}};
}
test('ordinary numeric fields read exact native values and preserve opaque targets and finite numbers',async()=>{
 const h=fixture();assert.equal(await h.editor.refresh(),true);assert.deepEqual(h.inputs().map(n=>n.value),['0','64']);assert.deepEqual(h.reads[0].view,{kind:'rawSample',target,property:'position',frame:10});
 h.inputs()[0].value='-3.25';await h.inputs()[0].fire('change');assert.deepEqual(h.writes[0].payload,{command:'object.property.edit',target,property:'position',action:{kind:'setValue',frame:10,values:[-3.25,64]}});assert.equal(h.writes[0].expected.contentRevision,7);assert.deepEqual(h.nativeValues,[0,64]);assert.equal(legacyEffects.reads,0);
});
test('stopwatch diamond maps four states onto the existing native intents without local sampling',async()=>{
 const cases=[
  ['not animated enables animation',{},{kind:'setAnimated',frame:10,animated:true},false,false],
  ['animated, key here, several keys removes the key',{keyCount:3,keyFrames:[2,10,20]},{kind:'removeKey',frame:10},true,true],
  ['animated, last key here freezes via setAnimated false',{keyCount:1,keyFrames:[10]},{kind:'setAnimated',frame:10,animated:false},true,true],
  ['animated, no key here adds a key with the raw sample',{keyCount:2,keyFrames:[2,20]},{kind:'setKey',frame:10,values:[0,64]},true,false]];
 for(const [name,options,action,on,filled] of cases){
  const h=fixture(options);await h.editor.refresh();const sw=h.sw();assert.ok(sw,name);
  assert.equal(/\bon\b/.test(sw.className),on,name);assert.equal(/fill="currentColor"/.test(sw.innerHTML),filled,name);assert.doesNotMatch(sw.className,/disabled/);
  assert.equal(h.keyReads().length,on?1:0,name);
  await sw.fire('click');assert.deepEqual(h.writes.map(w=>w.payload.action),[action],name);assert.match(h.status(),/updated/);assert.deepEqual(h.nativeValues,[0,64]);
 }
});
test('no text action buttons remain in the property value editor',async()=>{
 const h=fixture({keyCount:2,keyFrames:[10,20]});await h.editor.refresh();
 assert.equal(tree(h.editor.element).filter(n=>n.tagName==='BUTTON').length,0);assert.doesNotMatch(h.editor.element.textContent,/Add key|Remove key|Enable animation|Disable animation/);
 assert.equal(tree(h.editor.element).filter(n=>/native-motion-property-actions/.test(n.className||'')).length,0);
});
test('stopwatch pages through authored keys and finds a key beyond the first page',async()=>{
 const frames=Array.from({length:20},(_,i)=>i*2),h=fixture({keyCount:20,keyFrames:frames});await h.editor.refresh();
 assert.equal(h.keyReads().length,3);assert.deepEqual(h.keyReads().map(r=>r.view.offset),[0,8,16]);assert.match(h.sw().innerHTML,/fill="currentColor"/);
 const g=fixture({keyCount:20,keyFrames:frames.map(f=>f+1)});await g.editor.refresh();assert.match(g.sw().innerHTML,/fill="none"/);
});
test('stopwatch titles come from translated stopwatch keys with English fallbacks',async()=>{
 const h=fixture({keyCount:2,keyFrames:[10,20]});h.ports.translate=k=>({motionKeyRemove:'RM',motionKeyAdd:'ADD',motionAnimateProp:'ANIM'})[k];
 const e=Value.create(h.container,h.ports,target,{property:'position',dimension:2,animatable:true,authored:{keyCount:2,expressionEnabled:false}});await e.refresh();
 const sw=tree(e.element).find(n=>/motion-stopwatch/.test(n.className||''));assert.equal(sw.title,'RM');assert.equal(sw.attrs['aria-label'],'RM');
 const none=fixture();await none.editor.refresh();assert.match(none.sw().title,/Enable animation/);
});
test('stopwatch is disabled for expression rows, nonanimatable rows, unreadable keys and read-only ports',async()=>{
 const x=fixture({expression:true,keyCount:2,keyFrames:[10,20]});await x.editor.refresh();assert.match(x.sw().className,/disabled/);assert.equal(x.sw().attrs['aria-disabled'],'true');await x.sw().fire('click');assert.equal(x.writes.length,0);
 const n=fixture({animatable:false});await n.editor.refresh();assert.match(n.sw().className,/disabled/);await n.sw().fire('click');assert.equal(n.writes.length,0);assert.equal(n.inputs()[0].disabled,false);
 const u=fixture({keyCount:2,keyFrames:[10,20],reader:(q,r)=>q.view.kind==='authoredKeys'?Promise.reject(Error('nope')):Promise.resolve(r(q))});await u.editor.refresh();assert.match(u.sw().className,/disabled/);await u.sw().fire('click');assert.equal(u.writes.length,0);
 const r=fixture({readonly:true});await r.editor.refresh();assert.match(r.sw().className,/disabled/);await r.sw().fire('click');assert.equal(r.writes.length,0);
});
test('stopwatch also activates from the keyboard',async()=>{
 const h=fixture();await h.editor.refresh();await h.sw().fire('keydown',{key:'Enter',preventDefault(){}});assert.deepEqual(h.writes.map(w=>w.payload.action),[{kind:'setAnimated',frame:10,animated:true}]);
});
test('one and four dimensional zero and out-of-range numbers remain native values',async()=>{
 for(const numbers of [[0],[0,-100.5,999,0.25]]){const h=fixture({values:numbers});await h.editor.refresh();await h.inputs()[0].fire('change');assert.deepEqual(h.writes[0].payload.action.values,numbers);}
});
test('enabled expressions are explicitly presented as base values and refuse animation toggling',async()=>{
 const h=fixture({expression:true});await h.editor.refresh();assert.match(h.editor.element.textContent,/Base value before expression/);await h.sw().fire('click');assert.equal(h.writes.length,0);assert.deepEqual(h.nativeValues,[0,64]);
});
test('absent or revoked write capability and nonanimatable metadata never manufacture a writer',async()=>{
 const h=fixture({readonly:true});assert.equal(await h.editor.refresh(),true);assert.ok(h.inputs().every(n=>n.disabled&&n.readOnly));await h.inputs()[0].fire('change');assert.equal(h.writes.length,0);
 const r=fixture();await r.editor.refresh();delete r.ports.editProperty;await r.sw().fire('click');assert.equal(r.writes.length,0);
 const n=fixture({animatable:false});await n.editor.refresh();assert.match(n.sw().className,/disabled/);await n.sw().fire('click');assert.equal(n.writes.length,0);assert.equal(n.inputs()[0].disabled,false);
});
test('blank and nonfinite drafts never dispatch or replace the last accepted native value',async()=>{
 for(const text of ['',' ','Infinity','-Infinity','NaN','abc']){const h=fixture();await h.editor.refresh();h.inputs()[1].value=text;await h.inputs()[1].fire('change');assert.equal(h.writes.length,0);assert.match(h.status(),/finite numeric/);assert.deepEqual(h.nativeValues,[0,64]);}
});
test('foreign malformed sparse or oversized reads fail closed without defaults',async()=>{
 for(const change of [v=>v.instanceId='foreign',v=>v.documentId='foreign',v=>v.atRevision++,v=>v.documentSnapshotId='bad',v=>v.frame++,v=>v.target.strokeId='foreign',v=>delete v.target.strokeId,v=>v.property='opacity',v=>v.extra=true,v=>v.value=[0,Infinity],v=>delete v.value[1],v=>v.dimension=1,v=>v.value=[1],v=>v.target.contextId='x'.repeat(4000)]){
  const h=fixture({reader(q,response){const value=response(q);change(value);return Promise.resolve(value);}});assert.equal(await h.editor.refresh(),false);assert.ok(h.inputs().every(n=>n.disabled&&n.value===''));assert.equal(h.writes.length,0);
 }
});
test('each delayed read is fenced by frame, generation, owner, presentation and local lifetime',async()=>{
 for(const change of [h=>h.frame(11),h=>h.generation(),h=>h.replace(),h=>h.foreign(),h=>h.ready(false),h=>h.editor.cancel(),h=>h.editor.dispose(),h=>h.doc.removeChild(h.container)]){
  const wait=deferred(),h=fixture({reader:()=>wait.promise});const read=h.editor.refresh();change(h);wait.resolve(h.response(h.reads[0]));assert.equal(await read,false);assert.ok(h.inputs().every(n=>n.disabled));assert.equal(h.writes.length,0);
 }
});
test('queued local intent expires on cancellation or detachment before submission',async()=>{
 for(const change of [h=>h.editor.cancel(),h=>h.doc.removeChild(h.container)]){let applied=0;const wait=deferred(),h=fixture({writer:async(payload,pin,current)=>{await wait.promise;if(!current())throw Error('stale');applied++;return {applied:true,historyEntriesAdded:1};}});await h.editor.refresh();const acting=h.sw().fire('click');change(h);assert.equal(h.writes[0].current(),false);wait.resolve();await acting;assert.equal(applied,0);}
});
test('busy fields cannot submit duplicate commands and a native no-op remains a no-op',async()=>{
 const wait=deferred(),h=fixture({writer:()=>wait.promise});await h.editor.refresh();const acting=h.sw().fire('click');assert.ok(h.inputs().every(n=>n.disabled));await h.sw().fire('click');assert.equal(h.writes.length,1);assert.deepEqual(h.nativeValues,[0,64]);wait.resolve({applied:false,historyEntriesAdded:0});await acting;assert.match(h.status(),/No property change/);
});
test('malformed receipt and owner replacement cannot claim a current accepted edit',async()=>{
 for(const result of [null,{applied:true,historyEntriesAdded:0},{applied:false,historyEntriesAdded:1},{applied:true,historyEntriesAdded:1,extra:true}]){const h=fixture({writer:async()=>result});await h.editor.refresh();await h.sw().fire('click');assert.match(h.status(),/refused/);assert.equal(h.writes.length,1);}
 const wait=deferred(),h=fixture({writer:()=>wait.promise});await h.editor.refresh();const acting=h.sw().fire('click');h.replace();wait.resolve({applied:true,historyEntriesAdded:1});await acting;assert.match(h.status(),/view changed/);assert.doesNotMatch(h.status(),/updated/);
});
test('fields carry baseline classes, X/Y labels, unit span and scrub step; port dimLabels overrides the default',async()=>{
 const h=fixture();await h.editor.refresh();const all=tree(h.editor.element);
 assert.equal(h.editor.element.className,'native-motion-property-value motion-fields');
 assert.ok(h.inputs().every(n=>n.className==='pi scrub motion-val'&&n.step==='1'&&n.dataset.step==='1'&&n.dataset.nativeOpacityScrub==='static'));
 assert.deepEqual(all.filter(n=>n.className==='motion-dim-label').map(n=>n.textContent),['X','Y']);
 assert.equal(all.filter(n=>n.className==='motion-unit').length,0,'no unit span when the descriptor has no unit');
 const one=fixture({values:[1]});await one.editor.refresh();assert.equal(tree(one.editor.element).filter(n=>n.className==='motion-dim-label').length,0);
 const doc=h.doc,box=doc.createElement('div');doc.appendChild(box);
 const e=Value.create(box,{...h.ports,dimLabels:()=>['W','H']},target,{property:'position',dimension:2,unit:'deg',animatable:true,authored:{}});
 assert.deepEqual(tree(e.element).filter(n=>n.className==='motion-dim-label').map(n=>n.textContent),['W','H']);
 assert.deepEqual(tree(e.element).filter(n=>n.className==='motion-unit').map(n=>n.textContent),['°']);
});
test('read-only rows grey their fields with a tooltip and never print a sentence',async()=>{
 const h=fixture({readonly:true});await h.editor.refresh();
 assert.ok(h.inputs().every(n=>n.disabled&&n.readOnly&&n.title==='Read-only'));assert.equal(h.status(),'');
 assert.doesNotMatch(h.editor.element.textContent,/read-only/i);
 const w=fixture({writer:async()=>{throw Error('no');}});await w.editor.refresh();w.inputs()[0].value='1';await w.inputs()[0].fire('change');
 assert.match(w.status(),/^Property edit refused/);
 const s=tree(w.editor.element).find(n=>n.attrs.role==='status');assert.equal(s.className,'native-motion-property-status');
});

const listenerCount=h=>(h.doc.listeners.get('pointerup')||[]).length+(h.doc.listeners.get('pointercancel')||[]).length;
test('a scrub drag commits ONE native edit with the last value on release, not one per change',async()=>{
 for(const end of ['pointerup','pointercancel']){
  const h=fixture();await h.editor.refresh();const input=h.inputs()[0];h.win._scrubLiveActive=true;
  for(let i=1;i<=30;i++){input.value=String(i);await input.fire('change');}
  assert.equal(h.writes.length,0,'no edit while the drag is live');assert.equal(input.value,'30');assert.equal(listenerCount(h),2);
  h.win._scrubLiveActive=false;await h.doc.fire(end);await tick();
  assert.equal(h.writes.length,1);assert.deepEqual(h.writes[0].payload.action,{kind:'setValue',frame:10,values:[30,64]});assert.match(h.status(),/updated/);
  await h.doc.fire(end);assert.equal(h.writes.length,1,'a second release is inert');assert.equal(listenerCount(h),0);
 }
});
test('the closing change dispatched inside the pointerup window commits once after the flag drops',async()=>{
 const h=fixture();await h.editor.refresh();const input=h.inputs()[1];h.win._scrubLiveActive=true;input.value='-7';
 const closing=input.fire('change');h.win._scrubLiveActive=false;await closing;await tick();
 assert.equal(h.writes.length,1);assert.deepEqual(h.writes[0].payload.action.values,[0,-7]);
});
test('typed values and Enter commit immediately and once; Escape reverts without an edit',async()=>{
 const h=fixture();await h.editor.refresh();const input=h.inputs()[0];
 input.value='12';await input.fire('keydown',{key:'Enter'});await input.fire('change');
 assert.equal(h.writes.length,1);assert.deepEqual(h.writes[0].payload.action.values,[12,64]);assert.equal(listenerCount(h),0);
 const e=fixture();await e.editor.refresh();const field=e.inputs()[0];field.value='99';let blurred=0;field.blur=()=>blurred++;
 await field.fire('keydown',{key:'Escape'});assert.equal(field.value,'0');assert.equal(blurred,1);assert.equal(e.writes.length,0);
 field.value='5';await field.fire('keydown',{key:'a'});assert.equal(field.value,'5');
});
test('cancel and dispose drop a pending scrub commit and remove the document listeners',async()=>{
 for(const stop of [h=>h.editor.cancel(),h=>h.editor.dispose()]){
  const h=fixture();await h.editor.refresh();h.win._scrubLiveActive=true;h.inputs()[0].value='8';await h.inputs()[0].fire('change');assert.equal(listenerCount(h),2);
  stop(h);assert.equal(listenerCount(h),0);h.win._scrubLiveActive=false;await h.doc.fire('pointerup');await tick();assert.equal(h.writes.length,0);
 }
});
test('a nonfinite value mid-drag never arms a commit',async()=>{
 const h=fixture();await h.editor.refresh();h.win._scrubLiveActive=true;h.inputs()[0].value='';await h.inputs()[0].fire('change');
 h.win._scrubLiveActive=false;await h.doc.fire('pointerup');assert.equal(h.writes.length,0);assert.equal(listenerCount(h),0);
});

// #2377 MS-5: the keys drawer opens on demand from the stopwatch instead of a per-row Keys button.
test('Alt-click and double-click on the stopwatch open the keys drawer without toggling the key',async()=>{
 let opened=0;const h=fixture({keyCount:2,keyFrames:[2,20],openKeys(){opened++;return true;}});await h.editor.refresh();const sw=h.sw();
 await sw.fire('click',{altKey:true});assert.equal(opened,1);assert.equal(h.writes.length,0,'Alt-click never edits');
 await sw.fire('dblclick');assert.equal(opened,2);assert.equal(h.writes.length,0);
 await sw.fire('click',{detail:2});assert.equal(h.writes.length,0,'the second click of a double-click is not a second toggle');
 await sw.fire('click');assert.equal(h.writes.length,1,'a plain click still drives the key toggle');assert.equal(opened,2);
});
test('Alt-click opens the drawer even when the stopwatch is disabled, and is inert without a drawer port',async()=>{
 let opened=0;const h=fixture({animatable:false,openKeys(){opened++;return true;}});await h.editor.refresh();
 await h.sw().fire('click',{altKey:true});assert.equal(opened,1);assert.equal(h.writes.length,0);
 const bare=fixture({});await bare.editor.refresh();await bare.sw().fire('click',{altKey:true});await bare.sw().fire('dblclick');assert.equal(bare.writes.length,0);
});
