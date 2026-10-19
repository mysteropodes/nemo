'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
// Private execution may bind the immutable existing View source explicitly. The
// candidate test naturally resolves the same sibling from its actual checkout.
const ROOT=process.env.NEMO_KEYS_SOURCE_ROOT||path.join(__dirname,'..');
const View=require(path.join(ROOT,'src/js/adapters/native-motion-property-view.js'));
const code=fs.readFileSync(path.join(__dirname,'../src/js/adapters/native-motion-property-keys.js'),'utf8');
const moduleValue={exports:{}},legacyEffects={reads:0};
const sandbox={module:moduleValue,TextEncoder,setTimeout,require(name){assert.equal(name,'./native-motion-property-view.js');return View;}};
for(const name of ['state','paper','SMProject','undoHistory','evaluateExpr','getVal','lerp'])Object.defineProperty(sandbox,name,{get(){legacyEffects.reads++;throw Error('Legacy authority accessed: '+name);}});
vm.compileFunction(code,['module','require','TextEncoder','setTimeout'],{contextExtensions:[sandbox]})(moduleValue,sandbox.require,TextEncoder,setTimeout);
const Keys=moduleValue.exports,copy=v=>JSON.parse(JSON.stringify(v));
class Node{
 constructor(doc,tag){this.ownerDocument=doc;this.tagName=tag.toUpperCase();this.children=[];this.parentNode=null;this.attrs={};this.dataset={};this.style={};this.listeners=new Map();this.value='';this.checked=false;this.disabled=false;this._text='';}
 set textContent(v){this.replaceChildren();this._text=String(v);}get textContent(){return this._text+this.children.map(n=>n.textContent).join('');}
 appendChild(n){if(n.parentNode)n.parentNode.removeChild(n);this.children.push(n);n.parentNode=this;return n;}
 removeChild(n){this.children.splice(this.children.indexOf(n),1);n.parentNode=null;return n;}
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
const target={contextId:'opaque scene:α',layerUid:'layer / value',strokeId:'stroke:β'};
function fixture(options={}){
 const doc=new Node(null,'#document');doc.createElement=t=>new Node(doc,t);const container=doc.createElement('div');doc.appendChild(container);
 let pin={instanceId:'instance-keys',documentId:'document-keys',lifecycleGeneration:1,contentRevision:7,documentSnapshotId:'native-object:document-keys:7'},frame=10,generation=2,ready=true;
 const reads=[],writes=[],pending=[],receipts=[],keys=options.keys||[{frame:-0.5,v:[25],curvePoints:null},{frame:4.25,v:[50],curvePoints:[],hold:false,hIn:[0,0]},{frame:10,v:[75],hold:true}];let refreshes=0;
 function context(){return {pin,frame,viewGeneration:generation,presented:{...pin,available:ready,presentation:{...pin,owner:'native',status:ready?'presented':'pending',frame}}};}
 function response(q){return {instanceId:q.instanceId,documentId:q.documentId,atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,kind:'authoredKeys',target:copy(q.view.target),property:q.view.property,dimension:options.dimension||1,trackPresent:options.trackPresent!==false,items:copy(keys.slice(q.view.offset,q.view.offset+8)),nextOffset:q.view.offset+8<keys.length?q.view.offset+8:null};}
 const ports={document:doc,readContext:context,readProperty(q){assert.deepEqual(Object.keys(q).sort(),['atRevision','documentId','documentSnapshotId','instanceId','view']);reads.push(copy(q));if(options.reader)return options.reader(q,response);return Promise.resolve(response(q));},
  async editPropertyTrack(payload,expected,isCurrent){writes.push({payload:copy(payload),expected:copy(expected),isCurrent});if(options.writer)return options.writer(payload,expected,isCurrent);assert.equal(isCurrent(),true);const result={applied:options.noop!==true,historyEntriesAdded:options.noop?0:1};receipts.push(result);return result;},
  async refresh(){refreshes++;if(options.refresh)await options.refresh();}};
 if(options.readonly)delete ports.editPropertyTrack;
 const editor=Keys.create(container,ports);
 return {doc,container,ports,editor,reads,writes,pending,receipts,response,context,get refreshes(){return refreshes;},
  checks:()=>tree(editor.element).filter(n=>n.type==='checkbox'),button:label=>tree(editor.element).find(n=>n.tagName==='BUTTON'&&n.textContent===label),percent:()=>tree(editor.element).find(n=>n.attrs['aria-label']==='Key influence percent'),status:()=>tree(editor.element).find(n=>n.attrs.role==='status').textContent,
  async select(index=0,on=true){const n=this.checks()[index];n.checked=on;await n.fire('change');},open:()=>editor.open(target,'opacity'),
  frame(v){frame=v;},generation(){generation++;},replace(){pin={...pin,lifecycleGeneration:pin.lifecycleGeneration+1};},revision(){pin={...pin,contentRevision:8,documentSnapshotId:'native-object:document-keys:8'};generation++;},ready(v){ready=v;},foreign(){pin={...pin,documentId:'another-document'};}};
}
test('authored keys preserve opaque identity, fractional frames and native DTO bytes without legacy work',async()=>{
 const h=fixture();assert.equal(await h.open(),true);assert.equal(h.checks().length,3);assert.equal(h.reads[0].view.limit,8);assert.equal(h.reads[0].view.offset,0);assert.deepEqual(h.reads[0].view.target,target);assert.match(h.editor.element.textContent,/-0.5/);assert.match(h.editor.element.textContent,/4.25/);assert.equal(h.writes.length,0);assert.deepEqual(legacyEffects,{reads:0});
 await h.select(1);await h.button('Ease').fire('click');assert.deepEqual(h.writes[0].payload,{command:'object.property.track.edit',selections:[{target,property:'opacity',frame:4.25}],action:{kind:'preset',preset:'ease'}});assert.equal(h.writes[0].expected.contentRevision,7);assert.equal(h.refreshes,1);assert.equal(h.checks().length,0);
});
test('all five presets and subdivision use existing tagged actions and acknowledged receipts',async()=>{
 for(const [label,action] of [['Linear',{kind:'preset',preset:'linear'}],['Ease',{kind:'preset',preset:'ease'}],['Ease In',{kind:'preset',preset:'easeIn'}],['Ease Out',{kind:'preset',preset:'easeOut'}],['Hold',{kind:'preset',preset:'hold'}],['Subdivide',{kind:'subdivide'}]]){const h=fixture();await h.open();await h.select();await h.button(label).fire('click');assert.deepEqual(h.writes[0].payload.action,action);assert.equal(h.refreshes,1);assert.match(h.status(),/accepted/);}
});
test('incoming/outgoing influence preserve finite out-of-range values; malformed input never dispatches',async()=>{
 for(const [label,side,value] of [['Incoming influence','in',-123.5],['Outgoing influence','out',250.25]]){const h=fixture();await h.open();await h.select();for(const invalid of ['',' ','Infinity','NaN','bad']){h.percent().value=invalid;await h.button(label).fire('click');assert.equal(h.writes.length,0);}h.percent().value=String(value);await h.button(label).fire('click');assert.deepEqual(h.writes[0].payload.action,{kind:'influence',side,percent:value});}
});
test('native edit port absence or revocation keeps read-only keys unavailable for mutation',async()=>{
 const h=fixture({readonly:true});assert.equal(await h.open(),true);assert.match(h.status(),/read-only/);assert.equal(h.checks()[0].disabled,true);assert.equal(h.button('Linear').disabled,true);await h.button('Linear').fire('click');assert.equal(h.writes.length,0);
 const k=fixture();await k.open();await k.select();delete k.ports.editPropertyTrack;await k.button('Linear').fire('click');assert.equal(k.writes.length,0);assert.equal(k.checks().length,0);
});
test('absent authored track differs from present empty without invented keys',async()=>{
 for(const absent of [true,false]){const h=fixture({keys:[],trackPresent:!absent});assert.equal(await h.open(),true);assert.equal(h.checks().length,0);assert.equal(h.status(),absent?'No authored track.':'Authored track is empty.');assert.equal(h.button('Linear').disabled,true);}
});
test('full pagination follows native cursor without a complete claim at the first page',async()=>{
 const wait=deferred(),keys=Array.from({length:18},(_,i)=>({frame:i-3.25,v:[i]}));const h=fixture({keys,reader(q,reply){return q.view.offset===8?wait.promise:Promise.resolve(reply(q));}});const loading=h.open();await tick();await new Promise(r=>setTimeout(r,5));assert.deepEqual(h.reads.map(q=>q.view.offset),[0,8]);assert.match(h.status(),/Reading/);assert.equal(h.checks().length,0);wait.resolve(h.response(h.reads[1]));assert.equal(await loading,true);assert.deepEqual(h.reads.map(q=>q.view.offset),[0,8,16]);assert.equal(h.checks().length,18);
});
test('at most32 stable keys may be selected across pages, without trimming the read',async()=>{
 const h=fixture({keys:Array.from({length:35},(_,i)=>({frame:i-4.5,v:[i]}))});await h.open();for(let i=0;i<33;i++)await h.select(i);assert.equal(h.checks().length,35);assert.equal(h.checks()[32].checked,false);await h.button('Hold').fire('click');assert.equal(h.writes[0].payload.selections.length,32);assert.equal(h.writes[0].payload.selections[31].frame,26.5);
});
test('foreign, malformed, duplicate and nonadvancing pages fail closed',async()=>{
 const keys=Array.from({length:9},(_,i)=>({frame:i,v:[i]}));
 for(const modify of [v=>v.documentId='foreign',v=>v.property='position',v=>v.target.layerUid='wrong',v=>v.extra=true,v=>v.dimension=0,v=>v.items[0].v=[Infinity],v=>v.items[0].curvePoints=[{x:1,y:0},{x:0,y:1}],v=>v.nextOffset=0,v=>v.nextOffset=9,v=>v.items.pop(),v=>v.trackPresent=false]){const h=fixture({keys,reader(q,reply){const v=reply(q);modify(v);return Promise.resolve(v);}});assert.equal(await h.open(),false);assert.equal(h.checks().length,0);assert.equal(h.writes.length,0);}
 const duplicate=fixture({keys,reader(q,reply){const v=reply(q);if(q.view.offset)v.items[0].frame=7;return Promise.resolve(v);}});assert.equal(await duplicate.open(),false);assert.match(duplicate.status(),/Duplicate/);
});
test('inconsistent page dimensions or track presence and oversized results are refused',async()=>{
 const keys=Array.from({length:9},(_,i)=>({frame:i,v:[i]}));
 for(const kind of ['dimension','track']){const h=fixture({keys,reader(q,reply){const v=reply(q);if(q.view.offset){if(kind==='dimension'){v.dimension=2;v.items[0].v=[1,2];}else{v.trackPresent=false;v.items=[];}}return Promise.resolve(v);}});assert.equal(await h.open(),false);assert.equal(h.checks().length,0);}
 const big=fixture({reader(q,reply){const v=reply(q);v.items[0].curvePoints=Array.from({length:180},(_,i)=>({x:i/179,y:1}));return Promise.resolve(v);}});assert.equal(await big.open(),false);assert.match(big.status(),/bound/);
});
test('each late reply is checked against frame, view, lifecycle, presentation and local lifetime',async()=>{
 for(const change of [h=>h.frame(11),h=>h.generation(),h=>h.replace(),h=>h.foreign(),h=>h.ready(false),h=>h.editor.cancel(),h=>h.editor.clear(),h=>h.editor.dispose(),h=>h.doc.removeChild(h.container)]){const wait=deferred(),h=fixture({reader:()=>wait.promise});const opening=h.open();const q=h.reads[0];change(h);wait.resolve(h.response(q));assert.equal(await opening,false);assert.equal(h.checks().length,0);assert.equal(h.writes.length,0);}
});
test('cancelled paging never accepts a later page and new reads supersede old reads',async()=>{
 const wait=deferred(),keys=Array.from({length:9},(_,i)=>({frame:i,v:[i]}));const h=fixture({keys,reader(q,reply){return q.view.offset?wait.promise:Promise.resolve(reply(q));}});const opening=h.open();await new Promise(r=>setTimeout(r,5));assert.equal(h.reads.length,2);h.editor.cancel();wait.resolve(h.response(h.reads[1]));assert.equal(await opening,false);assert.equal(h.reads.length,2);assert.equal(h.checks().length,0);
 const hold=deferred();let first=true;const k=fixture({reader(q,reply){if(first){first=false;return hold.promise;}return Promise.resolve(reply(q));}});const old=k.open();assert.equal(await k.editor.open(target,'scale'),true);hold.resolve(k.response(k.reads[0]));assert.equal(await old,false);assert.match(k.editor.element.textContent,/scale/);
});
test('queued dispatch receives a live local selection/lifetime predicate, never a captured true',async()=>{
 const wait=deferred(),h=fixture({writer:async(payload,pin,current)=>{await wait.promise;if(!current())throw Error('cancelled before dispatch');return {applied:true,historyEntriesAdded:1};}});await h.open();await h.select();const acting=h.button('Linear').fire('click');assert.equal(h.writes[0].isCurrent(),true);h.editor.cancel();assert.equal(h.writes[0].isCurrent(),false);wait.resolve();await acting;assert.equal(h.refreshes,0);assert.equal(h.checks().length,0);assert.match(h.status(),/cancelled/);
});
test('held commands publish no optimistic keys; accepted revision refreshes and noop stays a native noop',async()=>{
 const wait=deferred(),h=fixture({writer:()=>wait.promise});await h.open();await h.select();const before=h.editor.element.textContent,acting=h.button('Ease').fire('click');assert.equal(h.checks().length,3);assert.equal(h.button('Ease').disabled,true);assert.match(h.status(),/Waiting/);assert.ok(before.includes('4.25'));h.revision();wait.resolve({applied:true,historyEntriesAdded:1});await acting;assert.equal(h.refreshes,1);assert.equal(h.checks().length,0);
 const n=fixture({noop:true});await n.open();await n.select();await n.button('Hold').fire('click');assert.deepEqual(n.receipts,[{applied:false,historyEntriesAdded:0}]);assert.match(n.status(),/no change/);assert.equal(n.refreshes,1);
});
test('refusals, malformed receipts and replacement while awaiting never auto-retry or refresh foreign owner',async()=>{
 for(const result of [null,{applied:true,historyEntriesAdded:0},{applied:false,historyEntriesAdded:1},{applied:true,historyEntriesAdded:1,extra:true}]){const h=fixture({writer:async()=>result});await h.open();await h.select();await h.button('Linear').fire('click');assert.equal(h.writes.length,1);assert.equal(h.refreshes,0);assert.equal(h.checks().length,0);}
 const wait=deferred(),h=fixture({writer:()=>wait.promise});await h.open();await h.select();const acting=h.button('Linear').fire('click');h.replace();wait.resolve({applied:true,historyEntriesAdded:1});await acting;assert.equal(h.refreshes,0);assert.equal(h.checks().length,0);
 const failed=fixture({writer:async()=>{throw Error('native stale revision');}});await failed.open();await failed.select();await failed.button('Hold').fire('click');assert.equal(failed.writes.length,1);assert.match(failed.status(),/native stale revision/);
});
test('unready presentation and detached/disposed component refuse reads and detach their listeners',async()=>{
 const h=fixture();h.ready(false);assert.equal(await h.open(),false);assert.equal(h.reads.length,0);h.ready(true);await h.open();const nodes=tree(h.editor.element);h.editor.dispose();assert.equal(h.editor.element.parentNode,null);assert.ok(nodes.every(n=>[...n.listeners.values()].every(l=>!l.length)));assert.equal(await h.open(),false);assert.equal(h.reads.length,1);
});
test('native refresh failure does not misreport an accepted command as a native refusal',async()=>{
 const h=fixture({refresh:async()=>{throw Error('presentation unavailable');}});await h.open();await h.select();await h.button('Linear').fire('click');assert.match(h.status(),/receipt accepted; native refresh unavailable/);assert.equal(h.writes.length,1);assert.equal(h.checks().length,0);
});
function integratedModule(name,dependencies){
 const sourceRoot=process.env.NEMO_KEYS_INTEGRATION_ROOT||ROOT,filename=path.join(sourceRoot,'src/js/adapters/'+name+'.js'),m={exports:{}};
 vm.compileFunction(fs.readFileSync(filename,'utf8'),['module','require','TextEncoder','setTimeout'])(m,p=>{assert.ok(Object.prototype.hasOwnProperty.call(dependencies,p),p);return dependencies[p];},TextEncoder,setTimeout);return m.exports;
}
const Panel=integratedModule('native-motion-property-panel',{'./native-motion-property-view.js':View,'./native-motion-property-keys.js':Keys,'./native-motion-property-value.js':require(path.join(ROOT,'src/js/adapters/native-motion-property-value.js'))});
const Binding=integratedModule('native-motion-property-binding',{'./native-motion-property-view.js':View,'./native-motion-property-panel.js':Panel});
function ordinary(options={}){
 const h=fixture(options);h.editor.dispose();const queries=[],rawReads=[],layerTarget={contextId:target.contextId,layerUid:target.layerUid};
 const root={document:h.doc,state:{appMode:'motion',currentFrame:10,activeLayerIdx:0,activeSymbolId:null},_layerSel:[0]};Object.defineProperty(root.state,'layers',{get(){throw Error('Legacy layer owner accessed');}});
 const session={isActive:()=>true,info:()=>({...h.context().pin,totalFrames:21,layers:[{layerUid:target.layerUid,name:'Native layer'}]}),readPresentedObjectSession:()=>({...h.context().presented,kind:'native-object'})};
 const ui={currentSession:()=>session,projectionReady:()=>true,readProperty(q){if(q.view.kind!=='rawSample')return h.ports.readProperty(q);rawReads.push(copy(q));return Promise.resolve({instanceId:q.instanceId,documentId:q.documentId,atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,kind:'rawSample',target:copy(q.view.target),property:q.view.property,dimension:1,frame:q.view.frame,value:[75]});},
  async editPropertyTrack(payload,pin,current){return {ok:true,result:await h.ports.editPropertyTrack(payload,pin,current)};}};
 if(options.readonly)delete ui.editPropertyTrack;
 async function query(q){queries.push(copy(q));const v=q.view,items=v.kind==='targets'?[{kind:'context',contextId:target.contextId},{kind:'layer',target:layerTarget}]:[{property:'opacity',dimension:1,unit:'%',defaultValue:[100],animatable:true,controlType:null,controlName:null,authored:{availability:'available',keyCount:3,hasStaticOverride:false,expressionEnabled:false},availability:{metadata:'available',applicability:'available',propertyWrite:'unavailable',reason:'Generic property writes remain unavailable.'}}];
  return {instanceId:q.instanceId,documentId:q.documentId,atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,kind:v.kind,target:v.kind==='targets'?null:copy(v.target),items,nextOffset:null,unavailableCategories:[{category:'propertyWrites',state:'unavailable',reason:'Generic property writes remain unavailable.'}]};}
 const binding=Binding.create(root,ui,query,{container:h.container,readContextId:()=>target.contextId});
 return {h,root,ui,binding,queries,rawReads,tree:()=>tree(h.container),button:label=>tree(h.container).find(n=>n.tagName==='BUTTON'&&n.textContent===label),checks:()=>tree(h.container).filter(n=>n.type==='checkbox'),keySection:()=>tree(h.container).find(n=>n.className==='native-motion-property-keys'),layerTarget};
}
test('ordinary Binding and Panel row click reach exact D11 keys and D12 edit then refresh metadata',async()=>{
 const o=ordinary();assert.equal(await o.binding.update(true),true);const pick=o.button('Keys');assert.ok(pick);assert.equal(o.keySection().hidden,true);await pick.fire('click');assert.equal(o.h.reads.length,1);assert.equal(o.rawReads.length,1);assert.equal(o.rawReads[0].view.kind,'rawSample');assert.deepEqual(o.h.reads[0].view.target,o.layerTarget);assert.equal(o.h.reads[0].view.property,'opacity');assert.equal(o.checks().length,3,o.keySection().textContent);const check=o.checks()[1];check.checked=true;await check.fire('change');await o.button('Ease Out').fire('click');assert.equal(o.h.writes.length,1);assert.deepEqual(o.h.writes[0].payload.action,{kind:'preset',preset:'easeOut'});assert.equal(o.h.writes[0].payload.selections[0].frame,4.25);assert.equal(o.queries.length,4);assert.equal(o.checks().length,0);assert.equal(o.keySection().hidden,true);o.binding.dispose();assert.equal(o.h.container.children.length,0);
});
test('ordinary metadata write-unavailable facts grant no permission without the actual edit facet',async()=>{
 const o=ordinary({readonly:true});await o.binding.update(true);await o.button('Keys').fire('click');assert.equal(o.checks()[0].disabled,true);assert.equal(o.button('Linear').disabled,true);assert.equal(o.h.writes.length,0);assert.equal(o.h.reads.length,1);o.binding.dispose();
});
test('ordinary metadata refresh, Cancel and disposal invalidate pending key reads',async()=>{
 for(const operation of ['refresh','cancel','dispose']){const wait=deferred(),o=ordinary({reader:()=>wait.promise});await o.binding.update(true);const opening=o.button('Keys').fire('click');assert.equal(o.h.reads.length,1);if(operation==='refresh')await o.binding.update(true);else if(operation==='cancel')o.binding.cancel();else o.binding.dispose();wait.resolve(o.h.response(o.h.reads[0]));await opening;assert.equal(o.checks().length,0);assert.equal(o.h.writes.length,0);o.binding.dispose();}
});
test('ordinary Binding queues a live selection/frame guard and cannot dispatch a stale key action',async()=>{
 const wait=deferred(),o=ordinary({writer:async(p,pin,current)=>{await wait.promise;if(!current())throw Error('queued native command cancelled');return {applied:true,historyEntriesAdded:1};}});await o.binding.update(true);await o.button('Keys').fire('click');const check=o.checks()[0];check.checked=true;await check.fire('change');const acting=o.button('Subdivide').fire('click');assert.equal(o.h.writes[0].isCurrent(),true);o.root.state.currentFrame=11;o.h.frame(11);assert.equal(o.h.writes[0].isCurrent(),false);wait.resolve();await acting;assert.equal(o.h.writes.length,1);assert.equal(o.queries.length,4,'settled refusal refreshes native metadata');assert.equal(o.checks().length,0);o.binding.dispose();
});
function actualOwnerFixture(){
 const base=process.env.NEMO_KEYS_OWNER_ROOT||ROOT,testFile=path.join(base,'tests/native-property-track-owner.test.cjs'),ownerFile=path.join(base,'src/js/adapters/native-object-project.js');
 const source=fs.readFileSync(testFile,'utf8'),marker=source.indexOf('\nasync function spin(');assert.ok(marker>0,'literal existing actual-Project fixture marker');
 const ownRequire=name=>name.startsWith('../src/')?require(path.join(ROOT,name.slice(3))):name.startsWith('./helpers/')?require(path.join(ROOT,'tests',name.slice(2))):require(name);
 function load(file,resolve){const m={exports:{}};vm.compileFunction(fs.readFileSync(file,'utf8'),['module','require'])(m,resolve);return m.exports;}
 const properties=load(path.join(base,'src/js/adapters/native-object-project-properties.js'),ownRequire);
 const Project=load(ownerFile,name=>name==='./native-object-project-properties.js'?properties:require(path.join(ROOT,'src/js/adapters',name)));
 return vm.compileFunction(source.slice(0,marker)+'\nreturn fixture;', ['require','__dirname','setImmediate'])(name=>name==='../src/js/adapters/native-object-project.js'?Project:ownRequire(name),path.dirname(testFile),setImmediate)();
}
test('ordinary editor passes the real Project pending queue through actual SDK and subscribed desktop',async()=>{
 const f=actualOwnerFixture();await f.open();const doc=new Node(null,'#document');doc.createElement=t=>new Node(doc,t);doc.getElementById=()=>({getBoundingClientRect:()=>({left:0,top:0,width:320,height:180})});const container=doc.createElement('div');doc.appendChild(container);f.root.document=doc;Object.assign(f.root.state,{appMode:'motion',activeLayerIdx:0,activeSymbolId:null,currentFrame:0});f.root._layerSel=[0];let sawPending=false,metadataQueries=0;
 const ui={currentSession:()=>f.session,projectionReady:()=>true,readProperty:q=>f.session.readProperty(q),get editPropertyTrack(){const edit=f.session.editPropertyTrack;return typeof edit==='function'?function(payload,pin,current){return edit(payload,pin,function(){if(f.session.readPresentedObjectSession()===null)sawPending=true;return current();});}:undefined;}};
 async function metadata(q){metadataQueries++;const v=q.view,t={contextId:'scene-root',layerUid:'data:layer 雪'},items=v.kind==='targets'?[{kind:'context',contextId:'scene-root'},{kind:'layer',target:t}]:[{property:'opacity',dimension:1,unit:'%',defaultValue:[100],animatable:true,controlType:null,controlName:null,authored:{availability:'available',keyCount:2,hasStaticOverride:false,expressionEnabled:false},availability:{metadata:'available',applicability:'available',propertyWrite:'unavailable',reason:'Generic write unavailable.'}}];return {instanceId:q.instanceId,documentId:q.documentId,atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,kind:v.kind,target:v.kind==='targets'?null:v.target,items,nextOffset:null,unavailableCategories:[{category:'propertyWrites',state:'unavailable',reason:'Generic write unavailable.'}]};}
 const binding=Binding.create(f.root,ui,metadata,{container,readContextId:()=> 'scene-root'});assert.equal(await binding.update(true),true);const button=label=>tree(container).find(n=>n.tagName==='BUTTON'&&n.textContent===label);await button('Keys').fire('click');const check=tree(container).find(n=>n.type==='checkbox');assert.ok(check,'actual Project D11 read must populate keys');check.checked=true;await check.fire('change');await button('Ease').fire('click');const commands=f.requests.filter(q=>q.operation==='command.document.object.property.track.edit');assert.equal(commands.length,1,'real pending queue must reach dispatch');assert.equal(sawPending,true,'readPresentedObjectSession is suppressed while the callback executes');assert.equal(f.session.info().contentRevision,1);assert.equal(metadataQueries,4);assert.equal(tree(container).filter(n=>n.type==='checkbox').length,0);binding.dispose();
});

test('authored vector dimensions1 through4 stay literal and legacy global traps remain untouched',async()=>{for(const n of [1,2,3,4]){const h=fixture({dimension:n,keys:[{frame:-0.125,v:Array.from({length:n},(_,i)=>i+0.5),curvePoints:[],hOut:[0.25,0.75]}]});assert.equal(await h.open(),true);assert.match(h.editor.element.textContent,new RegExp('Frame -0.125'));assert.equal(h.checks().length,1);}assert.equal(legacyEffects.reads,0);});
async function repaintingOwner(options={}){
 const f=actualOwnerFixture();await f.open();const doc=new Node(null,'#document');doc.createElement=t=>new Node(doc,t);doc.getElementById=()=>({getBoundingClientRect:()=>({left:0,top:0,width:320,height:180})});const container=doc.createElement('div');doc.appendChild(container);f.root.document=doc;Object.assign(f.root.state,{appMode:'motion',activeLayerIdx:0,activeSymbolId:null,currentFrame:0});f.root._layerSel=[0];
 let activeSession=f.session,queries=0,busyRepaints=0,busyCleared=false,heldRefresh=false;const updates=[],errors=[],refreshEntered=deferred();
 const ui={currentSession:()=>activeSession,projectionReady:()=>true,readProperty:q=>activeSession.readProperty(q),get editPropertyTrack(){const edit=activeSession.editPropertyTrack;return typeof edit==='function'?async function(payload,pin,current){try{return await edit(payload,pin,current);}catch(e){errors.push(e);throw e;}}:undefined;}};
 async function metadata(q){queries++;if(options.refreshGate&&!heldRefresh&&queries===3){heldRefresh=true;refreshEntered.resolve();await options.refreshGate.promise;}const v=q.view,t={contextId:'scene-root',layerUid:'data:layer 雪'},items=v.kind==='targets'?[{kind:'context',contextId:'scene-root'},{kind:'layer',target:t}]:[{property:'opacity',dimension:1,unit:'%',defaultValue:[100],animatable:true,controlType:null,controlName:null,authored:{availability:'available',keyCount:2,hasStaticOverride:false,expressionEnabled:false},availability:{metadata:'available',applicability:'available',propertyWrite:'unavailable',reason:'Generic write unavailable.'}}];return {instanceId:q.instanceId,documentId:q.documentId,atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,kind:v.kind,target:v.kind==='targets'?null:v.target,items,nextOffset:null,unavailableCategories:[{category:'propertyWrites',state:'unavailable',reason:'Generic write unavailable.'}]};}
 const binding=Binding.create(f.root,ui,metadata,{container,readContextId:()=> 'scene-root'}),button=label=>tree(container).find(n=>n.tagName==='BUTTON'&&n.textContent===label),checks=()=>tree(container).filter(n=>n.type==='checkbox'),mainStatus=()=>{const p=tree(container).find(n=>n.className==='native-motion-property-panel');return p&&p.children.find(n=>n.attrs.role==='status')?.textContent;};
 assert.equal(await binding.update(true),true);const oldUpdate=f.root.updateUI;f.root.updateUI=function(){oldUpdate();const busy=f.session.readPresentedObjectSession()===null;if(busy)busyRepaints++;const update=binding.update(false);updates.push(update);if(busy&&checks().length===0&&mainStatus()==='Native properties are unavailable.')busyCleared=true;};
 async function select(){await button('Keys').fire('click');const n=checks()[0];assert.ok(n,'ordinary actual-Project key read');n.checked=true;await n.fire('change');}
 return {f,binding,container,button,checks,mainStatus,select,errors,updates,refreshEntered,get queries(){return queries;},get busyRepaints(){return busyRepaints;},get busyCleared(){return busyCleared;},replace(session){activeSession=session;}};
}
test('real publication repaint retires busy editor, then settled success restores ready controls and notice',async()=>{
 const o=await repaintingOwner();await o.select();await o.button('Ease').fire('click');await Promise.all(o.updates);assert.equal(o.f.session.info().contentRevision,1);assert.equal(o.f.requests.filter(q=>q.operation==='command.document.object.property.track.edit').length,1);assert.equal(o.busyRepaints,1);assert.equal(o.busyCleared,true,'publication synchronously clears editor while pending');assert.equal(o.queries,4);assert.equal(o.mainStatus(),'Keys updated.');assert.ok(o.button('Keys'));assert.equal(o.checks().length,0);await o.select();assert.equal(o.button('Hold').disabled,false,'fresh authored keys restore action availability');assert.equal(o.errors.length,0);o.binding.dispose();
});
test('typed native refusal recovers the busy inspector without retrying or reporting success',async()=>{
 const o=await repaintingOwner();await o.select();o.f.refuse('not_found');await o.button('Ease').fire('click');await Promise.all(o.updates);assert.equal(o.f.session.info().contentRevision,0);assert.equal(o.f.requests.filter(q=>q.operation==='command.document.object.property.track.edit').length,1);assert.equal(o.busyRepaints,1);assert.equal(o.busyCleared,true);assert.equal(o.queries,4);assert.equal(o.mainStatus(),'Key edit could not be confirmed.');assert.equal(o.errors.length,1);assert.match(o.errors[0].message,/Native command refused: not_found/);await o.select();assert.equal(o.button('Linear').disabled,false);o.binding.dispose();
});
test('replaced or disposed ordinary owner before native reply receives no old edit notice',async()=>{
 for(const dispose of [false,true]){const o=await repaintingOwner(),gate=deferred();await o.select();o.f.holdReply(gate);const acting=o.button('Ease').fire('click');await tick();assert.equal(o.f.requests.filter(q=>q.operation==='command.document.object.property.track.edit').length,1);if(dispose)o.binding.dispose();else{const other=actualOwnerFixture();await other.open();await other.open();o.replace(other.session);await o.binding.update(true);}gate.resolve();await acting;await Promise.all(o.updates);if(dispose)assert.equal(o.container.children.length,0);else{assert.ok(o.button('Keys'));assert.doesNotMatch(o.mainStatus(),/Keys updated|No key changes|could not be confirmed/);}o.binding.dispose();}
});
test('replacement or disposal while settled refresh is awaiting cannot receive the old command outcome',async()=>{
 for(const dispose of [false,true]){const gate=deferred(),o=await repaintingOwner({refreshGate:gate});await o.select();const acting=o.button('Ease').fire('click');await o.refreshEntered.promise;assert.equal(o.busyCleared,true);assert.equal(o.f.session.info().contentRevision,1);if(dispose)o.binding.dispose();else{const other=actualOwnerFixture();await other.open();await other.open();o.replace(other.session);await o.binding.update(true);}gate.resolve();await acting;await Promise.all(o.updates);if(dispose)assert.equal(o.container.children.length,0);else{assert.ok(o.button('Keys'));assert.doesNotMatch(o.mainStatus(),/Keys updated|No key changes|could not be confirmed/);}o.binding.dispose();}
});
test('settled ordinary native no-op restores its inspector and reports no key changes',async()=>{
 const o=await repaintingOwner();await o.select();await o.button('Linear').fire('click');await Promise.all(o.updates);assert.equal(o.f.session.info().contentRevision,0);assert.equal(o.f.requests.filter(q=>q.operation==='command.document.object.property.track.edit').length,1);assert.equal(o.busyCleared,true);assert.equal(o.queries,4);assert.equal(o.mainStatus(),'No key changes.');await o.select();assert.equal(o.button('Hold').disabled,false);o.binding.dispose();
});

test('selected-key projection exports actual qualified fractional identities and independent local queued currency',async()=>{
 const h=fixture({keys:[{frame:-2.5,v:[1],lockTo:'in'},{frame:4.25,v:[2],lockTo:null}]});assert.equal(await h.open(),true);
 const cleared=h.editor.captureSelectedKeys();assert.deepEqual(cleared.keys,[]);assert.equal(cleared.isCurrent(),true);
 await h.select(1);assert.equal(cleared.isCurrent(),false);const captured=h.editor.captureSelectedKeys();
 assert.deepEqual(captured.keys,[{target:{kind:'property',target,property:'opacity'},frame:4.25}]);assert.equal(Object.isFrozen(captured.keys[0].target),true);assert.equal(Object.keys(captured).includes('isCurrent'),false);
 h.ready(false);assert.equal(h.editor.captureSelectedKeys(),null);assert.equal(captured.isCurrent(),true,'local selection proof remains readable while the queue hides presentation');
 h.editor.clear();assert.equal(captured.isCurrent(),false);h.ready(true);assert.deepEqual(h.editor.captureSelectedKeys().keys,[]);h.editor.dispose();assert.equal(h.editor.captureSelectedKeys(),null);
 for(const lockTo of [false,0,{},'other']){const bad=fixture({keys:[{frame:0,v:[1],lockTo}]});assert.equal(await bad.open(),false);}
});

test('key timeline is a frozen single-property native read projection with nonserialized pure UI selectors',async()=>{
 const h=fixture();assert.equal(h.editor.readKeyTimeline(),null);await h.open();
 const projection=h.editor.readKeyTimeline(),literal=copy(projection);
 assert.deepEqual(Object.keys(literal).sort(),['frame','items','pin','property','target','viewGeneration']);
 assert.deepEqual(literal,{pin:h.context().pin,frame:10,viewGeneration:2,target,property:'opacity',items:[{frame:-0.5,selected:false},{frame:4.25,selected:false},{frame:10,selected:false}]});
 assert.equal(Object.isFrozen(projection),true);assert.equal(Object.isFrozen(projection.items),true);assert.equal(Object.isFrozen(projection.items[0]),true);
 assert.equal(projection.isCurrent(),true);assert.equal(Object.getOwnPropertyDescriptor(projection,'selectFrames').enumerable,false);
 const reads=h.reads.length;assert.equal(projection.selectFrames([4.25,-0.5]),true);assert.equal(projection.isCurrent(),false);
 assert.deepEqual(h.checks().map(n=>n.checked),[true,true,false]);assert.match(h.status(),/^2 native key/);
 assert.deepEqual(copy(h.editor.captureSelectedKeys()).keys.map(k=>k.frame),[-0.5,4.25]);
 const next=h.editor.readKeyTimeline();assert.equal(next.isCurrent(),true);assert.deepEqual(copy(next.items),[{frame:-0.5,selected:true},{frame:4.25,selected:true},{frame:10,selected:false}]);
 assert.equal(next.selectFrames([4.25,-0.5]),true);assert.equal(next.isCurrent(),true,'same selected set does not invalidate an unchanged projection');
 assert.equal(h.reads.length,reads);assert.equal(h.writes.length,0);
});

test('timeline selection validates all existing finite unique frames and the32 bound before changing the inspector Set',async()=>{
 const h=fixture({keys:Array.from({length:35},(_,i)=>({frame:i-4.5,v:[i]}))});await h.open();
 let view=h.editor.readKeyTimeline();assert.equal(view.selectFrames([-4.5]),true);view=h.editor.readKeyTimeline();
 const before=h.checks().map(n=>n.checked),message=h.status();
 for(const frames of [null,{},[-4.5,-4.5],[1000],[NaN],[Infinity],['-4.5'],new Array(1),Array.from({length:33},(_,i)=>i-4.5)]){
  assert.equal(view.selectFrames(frames),false);assert.equal(view.isCurrent(),true);assert.deepEqual(h.checks().map(n=>n.checked),before);assert.equal(h.status(),message);
 }
 assert.equal(view.selectFrames(Array.from({length:32},(_,i)=>i-4.5)),true);assert.equal(h.checks().filter(n=>n.checked).length,32);
 assert.equal(h.writes.length,0);assert.equal(h.editor.readKeyTimeline().selectFrames([]),true);assert.equal(h.checks().filter(n=>n.checked).length,0);
});

test('stale or pending timeline callbacks refuse without clearing accepted selected checkboxes',async()=>{
 for(const change of [h=>h.frame(11),h=>h.generation(),h=>h.replace(),h=>h.foreign(),h=>h.revision(),h=>h.ready(false)]){
  const h=fixture();await h.open();await h.select(1);const view=h.editor.readKeyTimeline(),before=h.checks().map(n=>n.checked);
  change(h);assert.equal(view.isCurrent(),false);assert.equal(h.editor.readKeyTimeline(),null);assert.equal(view.selectFrames([]),false);
  assert.deepEqual(h.checks().map(n=>n.checked),before);assert.equal(h.writes.length,0);
 }
 const h=fixture();await h.open();const view=h.editor.readKeyTimeline();h.editor.dispose();assert.equal(view.isCurrent(),false);assert.equal(view.selectFrames([]),false);assert.equal(h.editor.readKeyTimeline(),null);
});

test('read reset checkbox timeline selection busy and dispose publish safe synchronous key timeline notifications',async()=>{
 const wait=deferred(),h=fixture({writer:()=>wait.promise}),events=[];
 h.ports.keyTimelineChanged=reason=>{events.push({reason,available:h.editor.readKeyTimeline()!==null});if(reason==='selection')throw Error('observer must not own control flow');};
 await h.open();assert.ok(events.some(e=>e.reason==='read'&&e.available));await h.select(1);
 assert.equal(h.editor.readKeyTimeline().selectFrames([-0.5]),true);
 const pending=h.button('Hold').fire('click');assert.ok(events.some(e=>e.reason==='busy'&&!e.available));assert.equal(h.editor.readKeyTimeline(),null);
 wait.resolve({applied:true,historyEntriesAdded:1});await pending;assert.ok(events.some(e=>e.reason==='reset'&&!e.available));
 h.editor.dispose();assert.equal(events.at(-1).reason,'dispose');assert.equal(events.at(-1).available,false);
});

test('read-only native keys may project but timeline selection retains existing disabled-writer semantics',async()=>{
 const h=fixture({readonly:true});await h.open();const view=h.editor.readKeyTimeline(),before=h.status();
 assert.ok(view);assert.equal(view.isCurrent(),true);assert.equal(view.selectFrames([-0.5]),false);assert.equal(h.status(),before);
 assert.deepEqual(h.checks().map(n=>n.checked),[false,false,false]);assert.equal(h.writes.length,0);
});

test('selection preflight and checkboxes honor the aggregate budget without mutating on refusal',async()=>{
 const h=fixture();let elsewhere=31,preflights=0;h.ports.acceptKeySelection=frames=>{preflights++;return frames.length+elsewhere<=32;};await h.open();
 let view=h.editor.readKeyTimeline();assert.equal(view.canSelectFrames([4.25]),true);assert.equal(h.checks().filter(n=>n.checked).length,0);
 assert.equal(view.selectFrames([4.25]),true);await h.select(0);assert.deepEqual(h.checks().map(n=>n.checked),[false,true,false]);
 view=h.editor.readKeyTimeline();assert.equal(view.canSelectFrames([-0.5,4.25]),false);assert.equal(view.selectFrames([-0.5,4.25]),false);assert.equal(view.isCurrent(),true);
 elsewhere=30;assert.equal(view.selectFrames([-0.5,4.25]),true);assert.equal(h.checks().filter(n=>n.checked).length,2);assert.ok(preflights>=5);assert.equal(h.writes.length,0);
});

test('throwing or revoked aggregate selection admission never changes inspector selection',async()=>{
 const h=fixture();await h.open();assert.equal(h.editor.readKeyTimeline().selectFrames([4.25]),true);
 h.ports.acceptKeySelection=()=>{throw Error('Aggregate selection unavailable');};const view=h.editor.readKeyTimeline();
 assert.equal(view.canSelectFrames([]),false);assert.equal(view.selectFrames([]),false);await h.select(0);
 assert.deepEqual(h.checks().map(n=>n.checked),[false,true,false]);assert.equal(view.isCurrent(),true);assert.equal(h.writes.length,0);
});
