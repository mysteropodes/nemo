'use strict';
// Actual capture handlers, baseline geometry/style helpers, Project/Desktop/SDK/transport.
// Only DOM, native host and presentation completion are simulated. No native/installed claim.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ROOT=process.env.NEMO_SHAPE_SOURCE_ROOT||path.resolve(__dirname,'..');
const Project=require('../src/js/adapters/native-object-project.js'),Files=require('../src/js/adapters/native-object-project-file.js');
const SDK=require('../src/js/adapters/native-application.js'),Transport=require('../src/js/adapters/application-mcp.js'),Vector=require('../src/js/adapters/native-object-vector.js');
const turn=()=>new Promise(setImmediate);function gate(){let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};}
class Element{
 constructor(){this.listeners=new Map();this.children=[];this.style={};this.attributes={};this.captured=new Set();}
 addEventListener(k,fn,options){if(!this.listeners.has(k))this.listeners.set(k,[]);this.listeners.get(k).push({fn,capture:!!(options&&options.capture)});}
 removeEventListener(k,fn){this.listeners.set(k,(this.listeners.get(k)||[]).filter(f=>f.fn!==fn));}
 appendChild(x){this.children.push(x);x.parent=this;return x;}get firstChild(){return this.children[0];}
 remove(){if(this.parent)this.parent.children.splice(this.parent.children.indexOf(this),1);}
 setAttribute(k,v){this.attributes[k]=String(v);}setPointerCapture(id){this.captured.add(id);}releasePointerCapture(id){this.captured.delete(id);}
 fire(kind,data={}){const target=data.target||this,e={pointerId:1,isPrimary:true,button:0,clientX:80,clientY:90,shiftKey:false,...data,target,stopped:false,prevented:false,
  stopImmediatePropagation(){this.stopped=true;},preventDefault(){this.prevented=true;}};
  const parents=[];for(let p=target.parent;p;p=p.parent)parents.push(p);
  const run=(el,capture)=>{e.currentTarget=el;for(const item of el.listeners.get(kind)||[]){if(item.capture===capture)item.fn(e);if(e.stopped)return;}};
  for(const el of parents.slice().reverse()){run(el,true);if(e.stopped)return e;}
  run(target,true);if(!e.stopped)run(target,false);if(!e.stopped)for(const el of parents){run(el,false);if(e.stopped)break;}return e;}

}
function baselineFunction(text,name){const start=text.indexOf('function '+name+'(');assert.ok(start>=0,name);let at=text.indexOf('{',start),depth=1,end=at+1;while(depth){if(text[end]==='{')depth++;if(text[end]==='}')depth--;end++;}return text.slice(start,end);}
async function fixture(tool='rect',native=true){
 let installed=false,generation=0,revision=0,hold=null,refusal=null,replyChange=null,wrongOwner=false,ack=null,binding=null;
 let rect={left:40,top:50,width:640,height:360};const records=new Map(),receipts=new Map(),calls=[],events=new Map(),log=[];
 const canvas=new Element(),area=new Element(),body=new Element(),doc=new Element();canvas.getBoundingClientRect=()=>rect;
 const pills=new Element(),pill=new Element(),zoom=new Element(),fit=new Element(),ruler=new Element();
 zoom.id='zoom-scrub';zoom.tagName='INPUT';zoom.value='100';fit.id='canvas-fit-btn';ruler.id='ruler-h';
 // The actual index333 structure: input inside pill inside zoom-pills inside canvas-area.
 area.appendChild(canvas);area.appendChild(pills);pills.appendChild(fit);pills.appendChild(pill);pill.appendChild(zoom);area.appendChild(ruler);
 doc.readyState='complete';doc.body=body;doc.getElementById=id=>({'drawing-canvas':canvas,'canvas-area':area,'zoom-scrub':zoom,'canvas-fit-btn':fit,'ruler-h':ruler}[id]||null);
 doc.createElementNS=()=>new Element();doc.documentElement={clientWidth:1200,clientHeight:900};
 const info=()=>({apiVersion:2,instanceId:'instance-A',documentId:'document-'+generation,lifecycleGeneration:generation,contentRevision:revision,
  documentSnapshotId:'native-object:document-'+generation+':'+revision,documentDigest:String(revision%10).repeat(64),byteCount:1000,
  canvasW:320,canvasH:180,totalFrames:16,fps:24,undoDepth:revision,redoDepth:0,reference:null,
  layers:[{layerUid:'L1',name:'Drawing',visible:true,locked:false,shy:false,color:'#5FA875',parentLayerUid:null}]});
 const root={document:doc,state:{tool,appMode:'anim2d',currentFrame:0,strokeEnabled:true,fillEnabled:true,strokeColor:'#112233',fillColor:'#44556680',
  brushSize:4,opacity:50,strokeCap:'round',strokeStyle:'solid',miterLimit:10,dashOffset:0,paintOrder:'fillFirst'},userLayers:[],selectedPaths:[],devicePixelRatio:2,
  NemoNativeObjectProject:Project,NemoNativeObjectProjectFile:Files,NemoApplicationMcpTransport:Transport,NemoNativeApplicationAdapter:SDK,NemoNativeObjectVector:Vector,
  NemoNativeObjectUI:{refreshTiming(){},refreshPolicy(){}},updateUI:()=>log.push('publish-'+revision),showToast:s=>log.push(s)};
 require('./helpers/native-viewport-underlay.cjs').install(root);
 async function invoke(command,args){const q=args&&(args.request||JSON.parse(args.requestJson));
  if(command==='nemo_native_status'||command==='nemo_native_object_client_status')return {...info(),available:installed};
  if(command==='nemo_native_object_bootstrap'){installed=true;generation++;revision=0;records.clear();return {...info(),requestId:q.requestId,resourceCount:0,viewportAvailable:false};}
  if(command==='nemo_native_object_reference')return {...info(),requestId:q.requestId};
  if(command==='nemo_native_object_preview'){log.push('present-'+q.contentRevision);return {status:'presented',workId:'work-'+generation+'-'+revision,viewGeneration:1};}
  if(command==='nemo_native_revision_sync'){
   if(q.action==='binding')return {...info(),subscriptionId:'subscription-'+generation};
   if(q.action==='subscribe'){binding=q.binding;return q.binding;}
   if(q.action==='acknowledge'){if(ack)await ack.promise;return null;}if(q.action==='disconnect')return null;
  }
  assert.equal(command,'nemo_native_dispatch');calls.push(structuredClone(q));assert.equal(q.operation,'command.document.object.vector.edit');
  const prior=receipts.get(q.requestId);if(prior){assert.deepEqual(prior.request,q);const r=structuredClone(prior.response);if(replyChange)replyChange(r);return r;}
  assert.equal(q.expectedRevision,revision);assert.equal(q.payload.command,'object.vector.create');assert.equal(q.payload.record.target.strokeId,'');
  if(refusal)return {apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:revision,ok:false,error:{code:refusal,message:'Native refusal'}};
  revision++;const strokeId='native-stroke:'+revision+':0',record=structuredClone(q.payload.record);record.target.strokeId=strokeId;records.set(strokeId,record);
  let r={apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:revision,ok:true,result:{applied:true,historyEntriesAdded:1,operation:'create',strokeId}};
  receipts.set(q.requestId,{request:structuredClone(q),response:structuredClone(r)});if(replyChange)replyChange(r);if(hold)await hold.promise;if(wrongOwner)r.documentId='replacement';return r;
 }
 root.__TAURI__={core:{invoke},event:{listen:async(name,fn)=>{events.set(name,fn);return ()=>events.delete(name);}}};
 const session=Project.desktop(root);if(native)await session.openPath('/simulated.json',{commit:meta=>Files.projectView(root,meta)});
 // VM global proxies have a different WeakMap identity; resolve the actual cached
 // desktop session explicitly, without substituting any Project/queue method.
 root.NemoNativeObjectProject={...Project,desktop:()=>session};
 root.NemoNativeObjectProjectFile={...Files,projectionReadyForUI:(_proxy,pin)=>Files.projectionReadyForUI(root,pin)};
 // Baseline Paper defaults are verified separately; Point is geometry-only here.
 root.Point=function(x,y){this.x=x;this.y=y;};
 for(const name of ['Path','Segment','pushUndo','ensureKeyframe','saveActiveLayerFrame','tagOwner','renderOS','applyShadowBrushTag'])root[name]=()=>assert.fail('legacy writer reached: '+name);
 root.SMEngineBridge={isEnabled:()=>true,nativeEditGuard:{allow:()=>!native},screenToWorld:()=>assert.fail('stale legacy viewport used'),suspend:()=>assert.fail('legacy engine suspended')};
 root.window=root;root.console=console;root.TextEncoder=TextEncoder;const context=vm.createContext(root),tools=fs.readFileSync(path.join(ROOT,'src/js/tools.js'),'utf8');
 for(const name of ['constrainSquare','constrainAngle45','applyStrokeStyle'])vm.runInContext(baselineFunction(tools,name),context,{filename:'tools.js:'+name});
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/adapters/native-object-shape-tool.js'),'utf8'),context,{filename:'native-object-shape-tool.js'});
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/shape-bridge.js'),'utf8'),context,{filename:'shape-bridge.js'});
 return {root,context,session,area,canvas,zoom,fit,ruler,doc,body,calls,records,log,events,info,setRect:r=>rect=r,refuse:code=>refusal=code,hold:g=>hold=g,mutate:fn=>replyChange=fn,
  async external(g){ack=g;const from=revision;revision++;return events.get('nemo-native-revision')({payload:{instanceId:'instance-A',documentId:info().documentId,lifecycleGeneration:generation,requestId:'external-'+revision,fromRevision:from,toRevision:revision}});},
  async gesture(end={clientX:240,clientY:210}){area.fire('pointerdown');area.fire('pointermove',end);area.fire('pointerup',end);return root.NemoNativeShapeToolController.whenSettled();}};
}
test('ordinary capture events reach real Project/SDK/native create once; drag preview never writes a document',async()=>{
 const f=await fixture();const down=f.area.fire('pointerdown');assert.equal(down.stopped,true);assert.equal(f.calls.length,0);assert.equal(f.body.children.length,1);
 f.area.fire('pointermove',{clientX:240,clientY:210});assert.equal(f.calls.length,0);assert.match(f.body.firstChild.firstChild.attributes.d,/M 20 20/);
 f.area.fire('pointerup',{clientX:240,clientY:210});assert.equal(f.body.children.length,0);const result=await f.root.NemoNativeShapeToolController.whenSettled();
 assert.equal(result.status,'applied');assert.equal(f.calls.length,1);assert.equal(f.session.info().contentRevision,1);assert.equal(f.records.size,1);
 const q=f.calls[0],r=q.payload.record;assert.equal(q.expectedRevision,0);assert.equal(r.target.layerUid,'L1');assert.equal(r.target.frameScope.frame,0);
 assert.deepEqual(r.geometry.segments.map(s=>[s.point.x,s.point.y]),[[20,20],[100,20],[100,80],[20,80]]);
 assert.deepEqual(r.vectorExt.paramShape,{kind:'rect',tl:0,tr:0,br:0,bl:0,box:{x1:20,y1:20,x2:100,y2:80}});
 assert.equal(r.vector.strokeCap,'butt');assert.equal(r.vector.strokeJoin,'miter');assert.equal(r.vector.opacity,0.5);
 assert.deepEqual(f.root.state.layers[0].frames,[]);assert.deepEqual(f.log.slice(-2),['publish-1','present-1']);
});
test('ellipse and Shift line reuse measured baseline handles/constraining and actual native viewport coordinates',async()=>{
 const e=await fixture('ellipse');assert.equal((await e.gesture()).status,'applied');const r=e.calls[0].payload.record;
 assert.deepEqual(r.geometry.segments.map(s=>[s.point.x,s.point.y]),[[100,50],[60,80],[20,50],[60,20]]);
 assert.equal(r.geometry.segments[0].handleIn.y,-16.569);assert.equal(r.geometry.segments[1].handleIn.x,22.091);assert.equal(r.geometry.closed,true);
 const l=await fixture('line');l.root.state.strokeStyle='dashed';l.root.state.dashOffset=1.5;const result=await l.gesture({clientX:240,clientY:210,shiftKey:true});assert.equal(result.status,'applied');
 const line=l.calls[0].payload.record;assert.deepEqual(line.geometry.segments.map(s=>[s.point.x,s.point.y]),[[20,20],[90.711,90.711]]);
 assert.equal(line.geometry.closed,false);assert.equal(line.fill,null);assert.deepEqual(line.vector.dashArray,[10,7.2]);assert.equal(line.vector.dashOffset,1.5);
});
test('pointercancel, lost capture, Escape, tiny click and unrelated pointer cannot commit',async()=>{
 for(const kind of ['pointercancel','lostpointercapture','Escape','tiny']){const f=await fixture();f.area.fire('pointerdown');
  if(kind==='Escape')f.doc.fire('keydown',{key:'Escape'});else if(kind==='tiny')f.area.fire('pointerup');else f.area.fire(kind);
  assert.equal(f.calls.length,0,kind);assert.equal(f.body.children.length,0,kind);assert.equal(f.area.captured.size,0,kind);
 }
 const f=await fixture();f.area.fire('pointerdown');assert.equal(f.area.fire('pointerup',{pointerId:2}).stopped,true);assert.equal(f.calls.length,0);f.area.fire('pointercancel');
});
test('stale frame, selection, tool, style, owner or viewport cancels without transport and no legacy fallback',async()=>{
 for(const change of [f=>f.root.state.currentFrame=1,f=>f.root._layerSel=[],f=>f.root.state.tool='ellipse',f=>f.root.state.opacity=70,
  f=>f.setRect({left:45,top:50,width:640,height:360}),async f=>f.session.newProject({canvasW:320,canvasH:180,fps:24},{commit:meta=>Files.projectView(f.root,meta)})]){
  const f=await fixture();f.area.fire('pointerdown');await change(f);const up=f.area.fire('pointerup',{clientX:240,clientY:210});assert.equal(up.stopped,true);assert.equal(f.calls.length,0);assert.equal(f.body.children.length,0);
 }
});
test('native unavailable modes and unconfirmed presentation are consumed before any legacy writer',async()=>{
 for(const change of [f=>f.root.state.tool='star',f=>f.root.state.symmetryEnabled=true,f=>f.root.state.shadowMode=true,f=>f.root.state.maskMode=true,
  f=>f.root.state.perspectiveEnabled=true,f=>f.root.state.strokeStyle='dashed',f=>f.root.state.layers[0].locked=true,
  f=>f.root.state.strokeEnabled=f.root.state.fillEnabled=false,f=>f.root.document.documentElement.classList.remove('nemo-native-underlay')]){
  const f=await fixture();change(f);assert.equal(f.area.fire('pointerdown').stopped,true);f.area.fire('pointerup',{clientX:240,clientY:210});assert.equal(f.calls.length,0);assert.equal(f.body.children.length,0);
 }
});
test('pending native receipt blocks new gestures; typed refusal and false ACK never invoke a Paper writer',async()=>{
 const f=await fixture(),wait=gate();f.hold(wait);f.area.fire('pointerdown');f.area.fire('pointerup',{clientX:240,clientY:210});await turn();assert.equal(f.calls.length,1);
 f.area.fire('pointerdown');f.area.fire('pointerup',{clientX:300,clientY:250});assert.equal(f.calls.length,1);wait.resolve();assert.equal((await f.root.NemoNativeShapeToolController.whenSettled()).status,'applied');
 const refused=await fixture();refused.refuse('unavailable');assert.equal((await refused.gesture()).status,'unavailable');assert.equal(refused.calls.length,1);assert.equal(refused.records.size,0);
 const bad=await fixture();bad.mutate(r=>r.result.strokeId='foreign');assert.equal((await bad.gesture()).status,'indeterminate');assert.equal(bad.calls.length,2);assert.equal(bad.calls[0].requestId,bad.calls[1].requestId);assert.equal(bad.records.size,1);assert.equal(bad.session.info().contentRevision,1);
 bad.area.fire('pointerdown');bad.area.fire('pointerup',{clientX:300,clientY:240});await turn();assert.equal(bad.calls.length,2,'uncertain receipt blocks a new command identity');
});
test('external revision ACK fence refuses pointerdown until actual transport synchronization completes',async()=>{
 const f=await fixture(),wait=gate(),external=f.external(wait);await turn();assert.equal(f.area.fire('pointerdown').stopped,true);assert.equal(f.calls.length,0);wait.resolve();await external;
 assert.equal((await f.gesture()).status,'applied');assert.equal(f.calls[0].expectedRevision,1);
});
test('private ordinary loader registers helper before shape bridge and keeps the baseline tools geometry source',()=>{
 const html=fs.readFileSync(path.join(ROOT,'src/index.html'),'utf8');assert.ok(html.indexOf('js/adapters/native-object-shape-tool.js')<html.indexOf('js/shape-bridge.js'));
 const profile=JSON.parse(fs.readFileSync(path.join(ROOT,'engineering/boundaries/profiles/app-js.profile.json')));const members=profile.modules.filter(m=>m.files.includes('native-object-shape-tool.js'));
 assert.equal(members.length,1);assert.deepEqual(members[0].publicApi,['native-object-shape-tool.js']);
 const paper=require('./helpers/native-import-paper.cjs').load(),p=new paper.Path({insert:false});assert.equal(p.strokeCap,'butt');assert.equal(p.strokeJoin,'miter');p.remove();
});

test('native inactive keeps the original browser/legacy shape path available',async()=>{
 const f=await fixture('rect',false);assert.throws(()=>f.area.fire('pointerdown'),/legacy writer reached: pushUndo/);assert.equal(f.calls.length,0);
});

test('missing native shape client stays closed even if the legacy guard is absent',async()=>{
 const f=await fixture();vm.runInContext('window.NemoNativeObjectShapeTool = undefined; delete window.SMEngineBridge.nativeEditGuard;',f.context);
 assert.equal(f.area.fire('pointerdown').stopped,true);assert.equal(f.calls.length,0);assert.equal(f.body.children.length,0);
});
test('native fill-only rectangle keeps explicit no-stroke paint, while unsupported local motion refuses',async()=>{
 const f=await fixture();f.root.state.strokeEnabled=false;assert.equal((await f.gesture()).status,'applied');
 const r=f.calls[0].payload.record;assert.equal(r.vector.stroke,null);assert.equal(r.vector.hasRealStroke,false);assert.equal(r.fill.a,128/255);
 const moving=await fixture();moving.root.state.layers[0].motion={position:{}};assert.equal(moving.area.fire('pointerdown').stopped,true);assert.equal(moving.calls.length,0);
});
test('selection change after pointer-up but before queued resource dispatch refuses the captured intent',async()=>{
 const f=await fixture();f.area.fire('pointerdown');f.area.fire('pointerup',{clientX:240,clientY:210});f.root._layerSel=[];
 const result=await f.root.NemoNativeShapeToolController.whenSettled();assert.equal(result.status,'unavailable');assert.equal(f.calls.length,0);assert.equal(f.records.size,0);
});

test('real nested zoom input, Fit pill and ruler chrome retain usable pointer events without native or legacy drawing',async()=>{
 const f=await fixture();assert.equal(f.zoom.parent.parent.parent,f.area);
 for(const target of [f.zoom,f.fit,f.ruler]){
  let used=0;target.addEventListener('pointerdown',()=>{used++;});target.addEventListener('pointerup',()=>{used++;});
  const down=target.fire('pointerdown'),up=target.fire('pointerup',{clientX:240,clientY:210});
  assert.equal(down.prevented,false,target.id);assert.equal(down.stopped,false,target.id);assert.equal(up.prevented,false,target.id);
  assert.equal(used,2,target.id);assert.equal(f.calls.length,0);assert.equal(f.body.children.length,0);
 }
 delete f.root.SMEngineBridge.nativeEditGuard;
 vm.runInContext('window.NemoNativeObjectShapeTool=undefined;',f.context);
 const missing=await fixture();vm.runInContext('window.NemoNativeObjectShapeTool=undefined;delete window.SMEngineBridge.nativeEditGuard;',missing.context);
 assert.equal(missing.zoom.fire('pointerdown').prevented,false);assert.equal(missing.calls.length,0);
});
test('stage canvas pointer origin still creates exactly one native rectangle through actual capture ancestry',async()=>{
 const f=await fixture();f.canvas.fire('pointerdown');f.canvas.fire('pointermove',{clientX:240,clientY:210});f.canvas.fire('pointerup',{clientX:240,clientY:210});
 assert.equal((await f.root.NemoNativeShapeToolController.whenSettled()).status,'applied');assert.equal(f.calls.length,1);
});
test('all42 retained bootstrap identities match their explicit amended ordinal literal without metadata substitution',()=>{
 const html=fs.readFileSync(path.join(ROOT,'src/index.html'),'utf8'),fixture=fs.readFileSync(path.join(ROOT,'scripts/nemo/boundaries-application.test.cjs'),'utf8');
 const start=fixture.indexOf("test('N20 contract, authority, adapters and bootstrap load in their frozen order'");assert.ok(start>0);
 const block=fixture.slice(start),paths=Array.from(vm.runInNewContext(block.match(/const paths = (\[[\s\S]*?\]);/)[1]));assert.equal(paths.length,42);
 const expected=JSON.parse(block.match(/assert\.deepEqual\(paths\.map\([\s\S]*?scriptOrdinal\),\s*(\[[\d,\s]+\])\);/)[1]);
 const scripts=[...html.matchAll(/<script\b([^>]*)>/gi)].map((m,i)=>({src:m[1].match(/\bsrc=["']([^"']+)/)?.[1],ordinal:i+1}));
 assert.deepEqual(paths.map(p=>scripts.find(s=>'src/'+s.src===p).ordinal),expected);
 // Read actual HTML directly: one helper after SDK, before the following consumer.
 assert.equal(scripts.filter(s=>s.src==='js/adapters/native-frame-edit-desktop.js').length,1);
 assert.deepEqual(['native-application','native-frame-edit-desktop','native-text-read'].map(name=>scripts.find(s=>s.src==='js/adapters/'+name+'.js').ordinal),[222,223,224]);
 assert.equal(scripts.find(s=>s.src==='js/adapters/i18n-runtime.js').ordinal,111);assert.equal(scripts.find(s=>s.src==='js/i18n.js').ordinal,112);
 assert.match(fixture,/retained\.loadSites\[0\]\.scriptOrdinal, 111/);
 assert.match(fixture,/assert\.deepEqual\(positions, \[13, 15, 16, 17, 18, 19, 20, 21, 22\]/);
});
