'use strict';
// Production Project, SDK, Binding and Panel. Only the native transport/DOM are simulated.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ROOT=process.env.NEMO_VALUE_SOURCE_ROOT||path.join(__dirname,'..');
const Binding=require(path.join(ROOT,'src/js/adapters/native-motion-property-binding.js'));
class Node{
 constructor(doc,tag){this.ownerDocument=doc;this.tagName=tag.toUpperCase();this.children=[];this.parentNode=null;this.attrs={};this.dataset={};this.style={};this.listeners=new Map();this.value='';this.checked=false;this.disabled=false;this._text='';}
 set textContent(v){this.replaceChildren();this._text=String(v);}get textContent(){return this._text+this.children.map(n=>n.textContent).join('');}
 appendChild(n){if(n.parentNode)n.parentNode.removeChild(n);this.children.push(n);n.parentNode=this;return n;}
 removeChild(n){this.children.splice(this.children.indexOf(n),1);n.parentNode=null;return n;}
 contains(n){for(;n;n=n.parentNode)if(n===this)return true;return false;}
 replaceChildren(...nodes){this.children.forEach(n=>n.parentNode=null);this.children=[];this._text='';nodes.forEach(n=>this.appendChild(n));}
 setAttribute(k,v){this.attrs[k]=String(v);}
 addEventListener(type,fn){this.listeners.set(type,[...this.listeners.get(type)||[],fn]);}
 removeEventListener(type,fn){this.listeners.set(type,(this.listeners.get(type)||[]).filter(x=>x!==fn));}
 async fire(type){for(const fn of [...this.listeners.get(type)||[]])await fn({target:this});}
 get isConnected(){return this.tagName==='#DOCUMENT'||!!this.parentNode&&this.parentNode.isConnected;}
}
function tree(n){return [n,...n.children.flatMap(tree)];}
function deferred(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};}
const tick=()=>new Promise(r=>setImmediate(r));
function makeOwner(root){
  const file=path.join(root,'tests/native-property-track-owner.test.cjs');
  let source=fs.readFileSync(file,'utf8');
  const end=source.indexOf('\nasync function spin(');if(end<0)throw Error('Missing existing protocol fixture');source=source.slice(0,end);
  function replace(before,after){if(!source.includes(before))throw Error('Protocol fixture changed: '+before);source=source.replace(before,after);}
  replace("assert.equal(q.operation, 'command.document.object.property.track.edit')","assert.equal(q.operation, 'command.document.object.property.edit')");
  replace("shape = 'linear'","shape = 25");replace("shape = 'linear'","shape = 25");
  replace('value:[25]','value:[shape]');
  replace('const applied = q.payload.action.preset !== shape; shape = q.payload.action.preset;',
    "const action=q.payload.action; const applied=action.kind!=='setValue'||action.values[0]!==shape; if(action.kind==='setValue')shape=action.values[0];");
  const requireAt=require('node:module').createRequire(file);
  return vm.compileFunction(source+'\nreturn fixture();',['require','__dirname','setImmediate'])(requireAt,path.dirname(file),setImmediate);
}
async function ordinary(options={}){
 const f=makeOwner(ROOT);await f.open();const doc=new Node(null,'#document');doc.createElement=t=>new Node(doc,t);doc.getElementById=()=>({getBoundingClientRect:()=>({left:0,top:0,width:320,height:180})});const container=doc.createElement('div');doc.appendChild(container);f.root.document=doc;Object.assign(f.root.state,{appMode:'motion',activeLayerIdx:0,activeSymbolId:null,currentFrame:0});f.root._layerSel=[0];
 let active=f.session,sawPending=false,queries=0;const updates=[];
 const ui={currentSession:()=>active,projectionReady:()=>true,readProperty:q=>active.readProperty(q),get editProperty(){const edit=active.editProperty;return typeof edit==='function'?(payload,pin,current)=>edit(payload,pin,()=>{if(active.readPresentedObjectSession()===null)sawPending=true;return current();}):undefined;}};
 async function metadata(q){queries++;const v=q.view,t={contextId:'scene-root',layerUid:'data:layer 雪'},items=v.kind==='targets'?[{kind:'context',contextId:'scene-root'},{kind:'layer',target:t}]:[{property:'opacity',dimension:1,unit:'%',defaultValue:[100],animatable:true,controlType:null,controlName:null,authored:{availability:'available',keyCount:0,hasStaticOverride:true,expressionEnabled:false},availability:{metadata:'available',applicability:'available',propertyWrite:'unavailable',reason:'Description does not grant a writer.'}}];return {instanceId:q.instanceId,documentId:q.documentId,atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,kind:v.kind,target:v.kind==='targets'?null:v.target,items,nextOffset:null,unavailableCategories:[{category:'propertyWrites',state:'unavailable',reason:'Description does not grant a writer.'}]};}
 const binding=Binding.create(f.root,ui,metadata,{container,readContextId:()=> 'scene-root'});
 const input=()=>tree(container).find(n=>n.attrs['aria-label']==='opacity component 1');
 const status=()=>{const panel=tree(container).find(n=>n.className==='native-motion-property-panel');return panel&&panel.children.find(n=>n.attrs.role==='status')?.textContent;};
 await binding.update(true);await tick();assert.ok(input()&&!input().disabled,'ordinary raw value is presented and editable');
 const oldUpdate=f.root.updateUI;f.root.updateUI=function(){oldUpdate();updates.push(binding.update(false));};
 return {f,ui,binding,container,input,status,updates,get sawPending(){return sawPending;},get queries(){return queries;},async change(value){const n=input();n.value=String(value);await n.fire('change');await Promise.all(updates);await tick();},async replace(){const other=makeOwner(ROOT);await other.open();await other.open();active=other.session;await binding.update(true);await tick();return other;}};
}
test('ordinary numeric edit crosses actual Project queue, SDK and desktop port then resamples native result',async()=>{
 const h=await ordinary();assert.equal(h.input().value,'25');const old=h.input();await h.change(42);
 const commands=h.f.requests.filter(q=>q.operation.startsWith('command.'));assert.equal(commands.length,1);assert.deepEqual(commands[0].payload,{command:'object.property.edit',target:{contextId:'scene-root',layerUid:'data:layer 雪'},property:'opacity',action:{kind:'setValue',frame:0,values:[42]}});
 assert.equal(h.sawPending,true);assert.equal(h.f.session.info().contentRevision,1);assert.equal(h.input().value,'42');assert.equal(h.input().disabled,false);assert.equal(h.status(),'Property updated.');assert.equal(old.isConnected,false);assert.equal(h.f.log.filter(x=>x==='acknowledge').length,0);assert.equal(h.queries,4);h.binding.dispose();
});
test('ordinary numeric native no-op and refusal retain native value and recover the inspector',async()=>{
 const n=await ordinary();await n.change(25);assert.equal(n.f.session.info().contentRevision,0);assert.equal(n.input().value,'25');assert.equal(n.status(),'No property change.');n.binding.dispose();
 const h=await ordinary();h.f.refuse('not_found');await h.change(91);assert.equal(h.f.session.info().contentRevision,0);assert.equal(h.input().value,'25');assert.equal(h.status(),'Property edit could not be confirmed.');assert.equal(h.f.requests.filter(q=>q.operation.startsWith('command.')).length,1);assert.equal(h.input().disabled,false);h.binding.dispose();
});
test('replaced ordinary owner receives neither stale sampled value nor old successful edit notice',async()=>{
 const h=await ordinary(),wait=deferred();h.f.holdReply(wait);const acting=h.change(71);await tick();assert.equal(h.f.requests.filter(q=>q.operation.startsWith('command.')).length,1);const other=await h.replace();wait.resolve();await acting;assert.equal(other.session.info().contentRevision,0);assert.equal(h.input().value,'25');assert.doesNotMatch(h.status(),/Property updated|No property change|could not be confirmed/);h.binding.dispose();
});
test('captured numeric writer cannot bypass an external revision acknowledgement fence',async()=>{
 const h=await ordinary(),edit=h.f.session.editProperty,pin=h.f.session.info(),wait=deferred();h.f.holdAck(wait);const syncing=h.f.external();await tick();assert.equal(h.f.session.editProperty,undefined);await assert.rejects(edit({command:'object.property.edit',target:{contextId:'scene-root',layerUid:'data:layer 雪'},property:'opacity',action:{kind:'setValue',frame:0,values:[70]}},pin,()=>true));assert.equal(h.f.requests.filter(q=>q.operation.startsWith('command.')).length,0);wait.resolve();await syncing;h.binding.dispose();
});
