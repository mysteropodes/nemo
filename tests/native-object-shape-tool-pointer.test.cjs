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
  if(command==='nemo_native_object_preview'){log.push('present-'+q.contentRevision);return {status:'presented',workId:'work-'+generation+'-'+revision,viewGeneration:1,...(q.viewMode==='stored-camera'?{view:{mode:'stored-camera',cameraAvailable:true,documentToComposition:[2,0,0,2,-160,-90],compositionToDocument:[.5,0,0,.5,80,45]}}:{})};}
  if(command==='nemo_native_revision_sync'){
   if(q.action==='binding')return {...info(),subscriptionId:'subscription-'+generation};
   if(q.action==='subscribe'){binding=q.binding;return q.binding;}
   if(q.action==='acknowledge'){if(ack)await ack.promise;return null;}if(q.action==='disconnect')return null;
  }
  assert.equal(command,'nemo_native_dispatch');calls.push(structuredClone(q));assert.equal(q.operation,'command.document.object.vector.edit');
  const prior=receipts.get(q.requestId);if(prior){assert.deepEqual(prior.request,q);const r=structuredClone(prior.response);if(replyChange)replyChange(r);return r;}
  assert.equal(q.expectedRevision,revision);const many=q.payload.command==='object.vector.createMany';
  if(!many){assert.equal(q.payload.command,'object.vector.create');assert.equal(q.payload.record.target.strokeId,'');}
  if(refusal)return {apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:revision,ok:false,error:{code:refusal,message:'Native refusal'}};
  revision++;const ids=(many?q.payload.records:[q.payload.record]).map((rec,n)=>{const strokeId='native-stroke:'+revision+':'+n,record=structuredClone(rec);record.target.strokeId=strokeId;records.set(strokeId,record);return strokeId;});
  const strokeId=ids[0];
  let r={apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:revision,ok:true,result:many?{applied:true,historyEntriesAdded:1,operation:'createMany',strokeId,strokeIds:ids}:{applied:true,historyEntriesAdded:1,operation:'create',strokeId}};
  receipts.set(q.requestId,{request:structuredClone(q),response:structuredClone(r)});if(replyChange)replyChange(r);if(hold)await hold.promise;if(wrongOwner)r.documentId='replacement';return r;
 }
 root.__TAURI__={core:{invoke},event:{listen:async(name,fn)=>{events.set(name,fn);return ()=>events.delete(name);}}};
 const session=Project.desktop(root);if(native)await session.openPath('/simulated.json',{commit:meta=>Files.projectView(root,meta)});
 // VM global proxies have a different WeakMap identity; resolve the actual cached
 // desktop session explicitly, without substituting any Project/queue method.
 root.NemoNativeObjectProject={...Project,desktop:()=>session};
 // Controlled immutable native Perspective projection; never read legacy Perspective fields.
 let perspectiveMode='disabled',perspectiveOwner=session;
 const perspectiveKeys=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'],perspectiveProofs=new WeakMap();
 root.NemoNativePerspectiveController=Object.freeze({read(){
  const owner=perspectiveOwner,info=owner.info(),shown=owner.readPresentedObjectSession();
  if(perspectiveMode==='unavailable'||!owner.isActive()||!shown||perspectiveKeys.some(k=>shown[k]!==info[k]))return null;
  const pin=Object.fromEntries(perspectiveKeys.map(k=>[k,info[k]]));
  if(perspectiveMode==='stale')pin.documentSnapshotId='stale-perspective-snapshot';
  const value=Object.freeze({info:Object.freeze(pin),perspective:Object.freeze({settings:Object.freeze({perspectiveEnabled:perspectiveMode==='enabled'})})});
  perspectiveProofs.set(value,typeof owner.readSubscriptionGeneration==='function'?owner.readSubscriptionGeneration():0);return value;
 },readForCommand(value){
  const owner=perspectiveOwner,info=owner.info(),ticket=typeof owner.readSubscriptionGeneration==='function'?owner.readSubscriptionGeneration():0;
  return perspectiveMode==='disabled'&&owner.isActive()&&value&&perspectiveProofs.has(value)&&perspectiveProofs.get(value)===ticket&&perspectiveKeys.every(k=>value.info[k]===info[k])?value:null;
 }});

 root.NemoNativeObjectProjectFile={...Files,projectionReadyForUI:(_proxy,pin)=>Files.projectionReadyForUI(root,pin)};
 // Baseline Paper defaults are verified separately; Point is geometry-only here.
 root.Point=function(x,y){this.x=x;this.y=y;};
 for(const name of ['Path','Segment','pushUndo','ensureKeyframe','saveActiveLayerFrame','tagOwner','renderOS','applyShadowBrushTag'])root[name]=()=>assert.fail('legacy writer reached: '+name);
 root.SMEngineBridge={isEnabled:()=>true,nativeEditGuard:{allow:()=>!native},screenToWorld:()=>assert.fail('stale legacy viewport used'),suspend:()=>assert.fail('legacy engine suspended')};
 root.window=root;root.console=console;root.TextEncoder=TextEncoder;const context=vm.createContext(root),tools=fs.readFileSync(path.join(ROOT,'src/js/tools.js'),'utf8');
 for(const name of ['constrainSquare','constrainAngle45','applyStrokeStyle'])vm.runInContext(baselineFunction(tools,name),context,{filename:'tools.js:'+name});
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/adapters/native-object-draw-paint.js'),'utf8'),context,{filename:'native-object-draw-paint.js'});
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/adapters/native-object-shape-tool.js'),'utf8'),context,{filename:'native-object-shape-tool.js'});
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/shape-bridge.js'),'utf8'),context,{filename:'shape-bridge.js'});
 if(tool==='pen'){root.SM={setTool(t){root.state.tool=t;}};vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/pen-bridge.js'),'utf8'),context,{filename:'pen-bridge.js'});}
 return {perspectiveMode:value=>perspectiveMode=value,root,context,session,area,canvas,zoom,fit,ruler,doc,body,calls,records,log,events,info,setRect:r=>rect=r,refuse:code=>refusal=code,hold:g=>hold=g,mutate:fn=>replyChange=fn,
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
 for(const change of [f=>f.root.state.tool='star',f=>f.root.state.symmetryEnabled=true,f=>f.root.state.shadowMode=true,
  f=>f.perspectiveMode('enabled'),f=>f.root.state.strokeStyle='dashed',f=>f.root.state.layers[0].locked=true,
  f=>f.root.state.strokeEnabled=f.root.state.fillEnabled=false,f=>f.root.document.documentElement.classList.remove('nemo-native-underlay')]){
  const f=await fixture();change(f);assert.equal(f.area.fire('pointerdown').stopped,true);f.area.fire('pointerup',{clientX:240,clientY:210});assert.equal(f.calls.length,0);assert.equal(f.body.children.length,0);
 }
});
// #2361 G15 Mask mode (tools.js tagOwner @1065e9d8): a shape drawn while Mask is on commits once as a mask of the layer.
test('Mask mode commits the shape as a mask with the chosen combine mode, add by default',async()=>{
 for(const [type,mode] of [[undefined,'add'],['intersect','intersect']]){
  const f=await fixture();f.root.state.maskMode=true;if(type)f.root.state.maskModeType=type;
  assert.equal((await f.gesture()).status,'applied');assert.equal(f.calls.length,1);
  const ext=f.calls[0].payload.record.vectorExt||{};assert.deepEqual([ext.isMask,ext.maskMode],[true,mode]);
 }
 const plainShape=await fixture();assert.equal((await plainShape.gesture()).status,'applied');
 assert.equal((plainShape.calls[0].payload.record.vectorExt||{}).isMask,undefined,'without Mask mode nothing is tagged');
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
test('all 45 retained bootstrap identities keep their frozen runs and adjacency in the actual page',()=>{
 const html=fs.readFileSync(path.join(ROOT,'src/index.html'),'utf8'),fixture=fs.readFileSync(path.join(ROOT,'scripts/nemo/boundaries-application.test.cjs'),'utf8');
 const start=fixture.indexOf("test('N20 contract, authority, adapters and bootstrap load in their frozen order'");assert.ok(start>0);
 const block=fixture.slice(start),paths=Array.from(vm.runInNewContext(block.match(/const paths = (\[[\s\S]*?\]);/)[1]));assert.equal(paths.length,45);
 const runs=JSON.parse(block.match(/assertRuns\(paths\.map\([\s\S]*?scriptOrdinal\),\s*(\[[\d,\s]+\])/)[1]);
 const scripts=[...html.matchAll(/<script\b([^>]*)>/gi)].map((m,i)=>({src:m[1].match(/\bsrc=["']([^"']+)/)?.[1],ordinal:i+1}));
 const ordinals=paths.map(p=>scripts.find(s=>'src/'+s.src===p).ordinal);let at=0;
 for(const size of runs){for(let k=1;k<size;k++)assert.equal(ordinals[at+k],ordinals[at+k-1]+1);if(at)assert.ok(ordinals[at]>ordinals[at-1]+1);at+=size;}
 assert.equal(at,45);
 // Read actual HTML directly: one helper after SDK, before the following consumer.
 assert.equal(scripts.filter(s=>s.src==='js/adapters/native-frame-edit-desktop.js').length,1);
 const ord=name=>scripts.find(s=>s.src==='js/adapters/'+name+'.js').ordinal;
 assert.equal(ord('native-frame-edit-desktop'),ord('native-application')+1);
 assert.ok(ord('native-frame-edit-desktop')<ord('native-fill-desktop')&&ord('native-fill-desktop')<ord('native-text-read'));
 assert.equal(scripts.find(s=>s.src==='js/i18n.js').ordinal,ord('i18n-runtime')+1);
 assert.match(fixture,/retained\.loadSites\[0\]\.scriptOrdinal, runtime\[0\]\.ordinal/);
 assert.match(fixture,/assertRuns\(positions, \[1, 10, 2\]/);
});

test('native drawing refuses unavailable, stale and enabled Perspective projections before a document command',async()=>{
 for(const mode of ['unavailable','stale','enabled']){
  const f=await fixture();f.perspectiveMode(mode);
  Object.defineProperty(f.root.state,'perspectiveEnabled',{get(){throw new Error('legacy Perspective read');},set(){throw new Error('legacy Perspective write');}});
  f.area.fire('pointerdown');f.area.fire('pointerup',{clientX:240,clientY:210});await turn();
  assert.equal(f.calls.length,0,mode);assert.equal(f.body.children.length,0,mode);
 }
});

test('locked native camera maps real shape pointer and SVG draft through the same presented transform',async()=>{
 const f=await fixture();f.root.state.cameraView=true;assert.equal(f.session.readPresentedObjectSession(),null);await f.session.presentPreview(0);
 f.area.fire('pointerdown');f.area.fire('pointermove',{clientX:240,clientY:210});
 assert.match(f.body.firstChild.firstChild.attributes.d,/M 90 55/);
 assert.equal(f.body.firstChild.firstChild.attributes.transform,'matrix(2 0 0 2 -160 -90)');
 f.area.fire('pointerup',{clientX:240,clientY:210});assert.equal((await f.root.NemoNativeShapeToolController.whenSettled()).status,'applied');
 assert.deepEqual(f.calls[0].payload.record.geometry.segments.map(s=>[s.point.x,s.point.y]),[[90,55],[130,55],[130,85],[90,85]]);
 const stale=await fixture();stale.area.fire('pointerdown');stale.root.state.cameraView=true;stale.area.fire('pointerup',{clientX:240,clientY:210});
 assert.equal(stale.calls.length,0);assert.equal(stale.body.children.length,0);
});


test('native shape capture refuses an absent injected mapping even with the global underlay present',async()=>{
 const f=await fixture(),tool=f.root.NemoNativeObjectShapeTool;
 f.root.NemoNativeObjectShapeTool={...tool,create:(root,ports)=>tool.create(root,{...ports,mapping:undefined})};
 assert.ok(f.root.NemoNativeViewportUnderlay.canvasMapping);
 const down=f.area.fire('pointerdown');f.area.fire('pointerup',{clientX:240,clientY:210});await turn();
 assert.equal(down.prevented,true);assert.equal(f.calls.length,0);assert.equal(f.records.size,0);assert.equal(f.body.children.length,0);
});


test('queued shape uses the actual action lease while public presentation stays hidden and rejects a changed view',async()=>{
 for(const changed of [false,true]){
  const f=await fixture(),vector=f.root.NemoNativeObjectVector;let queued=0;
  f.root.NemoNativeObjectVector={...vector,create:owner=>vector.create({...owner,runResourceCommand:action=>owner.runResourceCommand(ctx=>{
   queued++;assert.equal(f.session.readPresentedObjectSession(),null,'public proof stays fenced during the actual resource action');
   if(changed)f.root.state.cameraView=true;
   return action(ctx);
  })})};
  f.area.fire('pointerdown');f.area.fire('pointerup',{clientX:240,clientY:210});
  const result=await f.root.NemoNativeShapeToolController.whenSettled();
  assert.equal(queued,1);assert.equal(result.status,changed?'unavailable':'applied');
  assert.equal(f.calls.length,changed?0:1);assert.equal(f.records.size,changed?0:1);assert.equal(f.session.info().contentRevision,changed?0:1);
  assert.equal(f.body.children.length,0);
 }
});

// Pen (G15, #2361): tools.js Pen @1065e9d8 over the same native create. Client (80,90) is document (20,20); scale 2.
const pts=r=>r.geometry.segments.map(s=>[s.point.x,s.point.y]);
async function pen(f,steps){let t=1000;const now=Date.now;Date.now=()=>t;
 try{for(const [kind,x,y,extra] of steps){if(kind==='wait'){t+=x;continue;}f.area.fire(kind,{clientX:x,clientY:y,...(extra||{})});t+=kind==='pointerdown'?1:0;}}finally{Date.now=now;}
 return f.root.NemoNativePenToolController&&f.root.NemoNativePenToolController.whenSettled();}
// #2361 Shadow Brush: tools.js tags the committed Pen/Shape path (applyShadowBrushTag @1065e9d8): the active swatch
// strokes it at max(1, width) with round ends and clears its fill; the channel and swatch id travel with the record.
test('Shadow mode commits Pen and Shape paths as construction ink in the active swatch, never in the preview',async()=>{
 const swatch={id:'sh5',ordinal:5,color:'#2288ff'},shadow={channelTag:'shadow',shadowSwatchId:'sh5'};
 const ink=r=>[r.vector.stroke.r,r.vector.stroke.g,r.vector.stroke.b,r.vector.hasRealStroke,r.fill,r.vector.strokeWidth,r.vector.strokeCap,r.vector.strokeJoin];
 const want=[0x22/255,0x88/255,1,true,null,4,'round','round'];
 const rect=await fixture();rect.root.SMShadowBrush={activeSwatch:()=>swatch};rect.root.state.shadowMode=true;
 assert.equal((await rect.gesture()).status,'applied');const r=rect.calls[0].payload.record;
 assert.deepEqual(ink(r),want,'the rectangle is ink, not a fill');assert.deepEqual(r.vectorExt.shadow,shadow);
 assert.equal(r.vectorExt.paramShape.kind,'rect','its parametric shape survives');
 const off=await fixture();off.root.SMShadowBrush={activeSwatch:()=>swatch};Object.assign(off.root.state,{shadowMode:true,strokeEnabled:false});
 assert.equal((await off.gesture()).status,'applied');assert.deepEqual(ink(off.calls[0].payload.record),want,'Stroke off still inks in Shadow mode');
 const p=await fixture('pen');p.root.SMShadowBrush={activeSwatch:()=>swatch};p.root.state.shadowMode=true;
 const res=await pen(p,[['pointerdown',80,90],['pointerup',80,90],['wait',500],['pointerdown',240,90],['pointerup',240,90],['pointerdown',240,90]]);
 assert.equal(res.status,'applied');const q=p.calls[0].payload.record;assert.deepEqual(ink(q),want);assert.deepEqual(q.vectorExt.shadow,shadow);
 // The baseline tags at pointer-up: while dragging, the preview keeps the drawing colours and never names the swatch.
 const live=await fixture();live.root.SMShadowBrush={activeSwatch:()=>swatch};live.root.state.shadowMode=true;
 live.area.fire('pointerdown');live.area.fire('pointermove',{clientX:240,clientY:210});const shown=live.body.firstChild.firstChild.attributes;
 assert.deepEqual([shown.stroke,shown.fill],['#112233','#44556680'],'the preview is not the committed ink');
 live.area.fire('pointerup',{clientX:240,clientY:210});await live.root.NemoNativeShapeToolController.whenSettled();
 assert.deepEqual(live.calls[0].payload.record.vectorExt.shadow,shadow);
 const none=await fixture();none.root.SMShadowBrush={activeSwatch:()=>null};none.root.state.shadowMode=true;
 none.area.fire('pointerdown');none.area.fire('pointerup',{clientX:240,clientY:210});assert.equal(none.calls.length,0,'no swatch, no commit');
});
test('Pen: clicks place anchors, a drag sets symmetric handles, a double press commits one open native path',async()=>{
 const f=await fixture('pen');
 const r=await pen(f,[['pointerdown',80,90],['pointerup',80,90],['wait',500],['pointerdown',240,90],['pointermove',260,110],['pointerup',260,110],['wait',500],
  ['pointerdown',240,210],['pointerup',240,210],['pointerdown',240,210]]);
 assert.equal(r.status,'applied');assert.equal(f.calls.length,1);const rec=f.calls[0].payload.record;
 assert.deepEqual(pts(rec),[[20,20],[100,20],[100,80]]);assert.equal(rec.geometry.closed,false);
 assert.deepEqual([rec.geometry.segments[1].handleOut.x,rec.geometry.segments[1].handleOut.y],[10,10]);
 assert.deepEqual([rec.geometry.segments[1].handleIn.x,rec.geometry.segments[1].handleIn.y],[-10,-10]);
 assert.equal(rec.fill,null);assert.equal(rec.vector.strokeCap,'round');assert.equal(rec.vector.opacity,0.5);
 assert.deepEqual(f.root.state.layers[0].frames,[],'no legacy frame was written');
});
test('Pen: pressing near the first anchor closes; Shift snaps to 45 degrees; Alt keeps the incoming handle',async()=>{
 const f=await fixture('pen');
 const r=await pen(f,[['pointerdown',80,90],['pointerup',80,90],['wait',500],['pointerdown',240,100,{shiftKey:true}],['pointermove',250,100,{altKey:true}],['pointerup',250,100],['wait',500],
  ['pointerdown',240,210],['pointerup',240,210],['wait',500],['pointerdown',84,94]]);
 assert.equal(r.status,'applied');const rec=f.calls[0].payload.record;assert.equal(rec.geometry.closed,true);
 assert.deepEqual(pts(rec),[[20,20],[100.156,20],[100,80]],'Shift snaps the angle and keeps the distance (constrainAngle45)');
 const second=rec.geometry.segments[1];assert.deepEqual([second.handleOut.x,second.handleOut.y],[4.844,5]);assert.deepEqual([second.handleIn.x,second.handleIn.y],[0,0]);
});
test('Pen: Escape and a single anchor commit nothing; Backspace drops the last anchor; a tool switch commits; taper refuses',async()=>{
 let f=await fixture('pen');await pen(f,[['pointerdown',80,90],['pointerup',80,90],['wait',500],['pointerdown',240,90],['pointerup',240,90]]);
 f.doc.listeners.get('keydown').forEach(l=>l.fn({key:'Escape',stopImmediatePropagation(){},preventDefault(){}}));
 await pen(f,[['wait',500],['pointerdown',240,210],['pointerup',240,210]]);assert.equal(f.calls.length,0,'Escape dropped the gesture; the next press starts a new one');
 f=await fixture('pen');const single=await pen(f,[['pointerdown',80,90],['pointerup',80,90],['pointerdown',80,90]]);
 assert.equal(single.status,'cancelled');assert.equal(f.calls.length,0);
 f=await fixture('pen');await pen(f,[['pointerdown',80,90],['pointerup',80,90],['wait',500],['pointerdown',240,90],['pointerup',240,90],['wait',500],['pointerdown',240,210],['pointerup',240,210]]);
 f.doc.listeners.get('keydown').forEach(l=>l.fn({key:'Backspace',stopImmediatePropagation(){},preventDefault(){}}));
 f.root.SM.setTool('rect');const switched=await f.root.NemoNativePenToolController.whenSettled();
 assert.equal(switched.status,'applied','switching tools commits the open path, as finalizePen does');
 assert.deepEqual(pts(f.calls[0].payload.record),[[20,20],[100,20]],'Backspace removed the last anchor first');
 f=await fixture('pen');f.root.state.taperEnds=true;await pen(f,[['pointerdown',80,90]]);
 assert.ok(f.log.includes('Tapered Pen strokes are unavailable in native Drawing.'));assert.equal(f.calls.length,0);
});

test('Pen: Enter commits; Alt reshapes a placed anchor; a direct tool change still commits; another pointer type cannot join',async()=>{
 let f=await fixture('pen');await pen(f,[['pointerdown',80,90],['pointerup',80,90],['wait',500],['pointerdown',240,90],['pointerup',240,90],['wait',500],
  ['pointerdown',240,90,{altKey:true}],['pointermove',260,110,{altKey:true}],['pointerup',260,110],['wait',500],['pointerdown',240,210,{pointerType:'touch'}]]);
 f.doc.listeners.get('keydown').forEach(l=>l.fn({key:'Enter',stopImmediatePropagation(){},preventDefault(){}}));
 let r=await f.root.NemoNativePenToolController.whenSettled();assert.equal(r.status,'applied');
 const rec=f.calls[0].payload.record;assert.deepEqual(pts(rec),[[20,20],[100,20]],'the touch press did not join the mouse gesture');
 assert.deepEqual([rec.geometry.segments[1].handleOut.x,rec.geometry.segments[1].handleOut.y],[10,10]);
 assert.deepEqual([rec.geometry.segments[1].handleIn.x,rec.geometry.segments[1].handleIn.y],[0,0],'Alt reshape leaves the incoming handle');
 f=await fixture('pen');await pen(f,[['pointerdown',80,90],['pointerup',80,90],['wait',500],['pointerdown',240,90],['pointerup',240,90]]);
 f.root.state.tool='hand';f.area.fire('pointerdown',{clientX:100,clientY:100});r=await f.root.NemoNativePenToolController.whenSettled();
 assert.equal(r.status,'applied','a tool change that bypassed SM.setTool still commits at the next event the Pen sees');
 f=await fixture('pen');await pen(f,[['pointerdown',80,90],['pointerup',80,90],['wait',500],['pointerdown',240,90],['pointerup',240,90]]);
 f.root.state.currentFrame=1;f.root.SM.setTool('rect');r=await f.root.NemoNativePenToolController.whenSettled();
 assert.notEqual(r.status,'applied','a frame change after the gesture voids the commit');assert.equal(f.calls.length,0);
 f=await fixture('pen');const hover=f.area.fire('pointermove',{clientX:100,clientY:100});assert.equal(hover.stopped,false,'hover is not consumed without a gesture');
});

// G23 symmetry (#2361): the stroke and its copies as ONE createMany. Canvas 320x180, default axes through the centre.
test('symmetry: a mirrored Pen path and a radial rectangle each commit as one native createMany',async()=>{
 let f=await fixture('pen');Object.assign(f.root.state,{symmetryEnabled:true,symmetryMode:'y',symmetryExtend:false});
 let r=await pen(f,[['pointerdown',80,90],['pointerup',80,90],['wait',500],['pointerdown',240,90],['pointerup',240,90],['pointerdown',240,90]]);
 assert.equal(r.status,'applied');assert.equal(f.calls.length,1);let q=f.calls[0].payload;assert.equal(q.command,'object.vector.createMany');
 assert.deepEqual(q.records.map(pts),[[[20,20],[100,20]],[[300,20],[220,20]]],'reflected across x = 160');
 f=await fixture('rect');Object.assign(f.root.state,{symmetryEnabled:true,symmetryMode:'radial',symmetryRadialSectors:4,symmetryRadialCenter:{x:160,y:90}});
 r=await f.gesture();assert.equal(r.status,'applied');q=f.calls[0].payload;assert.equal(q.records.length,4);
 assert.deepEqual(pts(q.records[0]),[[20,20],[100,20],[100,80],[20,80]]);
 assert.deepEqual(pts(q.records[2]),[[300,160],[220,160],[220,100],[300,100]],'180 degrees about (160,90)');
 assert.deepEqual(pts(q.records[1])[0],[90,230],'copies stack as insertAbove leaves them: 270 degrees right above the original');
 assert.equal(f.session.info().contentRevision,1,'four strokes, one revision');
});

test('symmetry: a stroke the axis clip leaves with one anchor refuses with its reason instead of dropping silently',async()=>{
 let f=await fixture('pen');Object.assign(f.root.state,{symmetryEnabled:true,symmetryMode:'y',symmetryExtend:false});
 await pen(f,[['pointerdown',400,90],['pointerup',400,90],['wait',500],['pointerdown',240,90],['pointerup',240,90],['pointerdown',240,90]]);
 assert.equal(f.calls.length,0);assert.ok(f.log.some(m=>/starts at the symmetry axis/.test(m)));
});

test('symmetry preview draws the stroke and its mirror without writing anything, then clears on commit',async()=>{
 const f=await fixture('rect');Object.assign(f.root.state,{symmetryEnabled:true,symmetryMode:'y',symmetryExtend:true});
 f.area.fire('pointerdown');f.area.fire('pointermove',{clientX:240,clientY:210});
 const overlay=f.body.firstChild;assert.equal(overlay.children.length,2,'stroke plus its mirror');
 assert.match(overlay.children[0].attributes.d,/^M 20 20/);assert.match(overlay.children[1].attributes.d,/^M 300 80/,'the mirrored closed copy is re-oriented');
 assert.equal(f.calls.length,0,'preview writes nothing');
 f.area.fire('pointerup',{clientX:240,clientY:210});await f.root.NemoNativeShapeToolController.whenSettled();
 assert.equal(f.body.children.length,0);assert.equal(f.calls.length,1);assert.equal(f.calls[0].payload.records.length,2);
});
