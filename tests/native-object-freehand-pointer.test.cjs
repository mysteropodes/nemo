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
async function fixture(tool='draw',native=true,options={}){
 let clock=1000;let installed=false,generation=0,revision=0,hold=null,refusal=null,replyChange=null,wrongOwner=false,ack=null,binding=null;
 let rect=options.rect||{left:40,top:50,width:640,height:360};const records=new Map(),receipts=new Map(),calls=[],events=new Map(),log=[];
 const canvas=new Element(),area=new Element(),body=new Element(),doc=new Element();canvas.getBoundingClientRect=()=>rect;
 const pills=new Element(),pill=new Element(),zoom=new Element(),fit=new Element(),ruler=new Element();
 zoom.id='zoom-scrub';zoom.tagName='INPUT';zoom.value='100';fit.id='canvas-fit-btn';ruler.id='ruler-h';
 // The actual index333 structure: input inside pill inside zoom-pills inside canvas-area.
 area.appendChild(canvas);area.appendChild(pills);pills.appendChild(fit);pills.appendChild(pill);pill.appendChild(zoom);area.appendChild(ruler);
 doc.readyState='complete';doc.body=body;doc.getElementById=id=>({'drawing-canvas':canvas,'canvas-area':area,'zoom-scrub':zoom,'canvas-fit-btn':fit,'ruler-h':ruler}[id]||null);
 doc.createElementNS=()=>new Element();doc.documentElement={clientWidth:1200,clientHeight:900};
 const flags={locked:false,visible:true},undos=[];
 const info=()=>({apiVersion:2,instanceId:'instance-A',documentId:'document-'+generation,lifecycleGeneration:generation,contentRevision:revision,
  documentSnapshotId:'native-object:document-'+generation+':'+revision,documentDigest:String(revision%10).repeat(64),byteCount:1000,
  canvasW:320,canvasH:180,totalFrames:16,fps:24,undoDepth:revision,redoDepth:0,reference:null,
  layers:[{layerUid:'L1',name:'Drawing',visible:flags.visible,locked:flags.locked,shy:false,color:'#5FA875',parentLayerUid:null}]});
 const root={document:doc,state:{tool,appMode:'anim2d',currentFrame:0,strokeEnabled:true,fillEnabled:true,strokeColor:'#112233',fillColor:'#44556680',
  brushSize:4,opacity:50,strokeCap:'round',strokeJoin:'round',strokeStyle:'solid',miterLimit:10,dashOffset:0,paintOrder:'fillFirst',
  vectorBrush:false,smoothing:0,stabilizer:0,taperEnds:false,drawMode:'front',brushPreset:'none',pressureMin:5,pressureMax:170,pressureInvert:false,pressureCurve:'linear',brushAngle:0,brushAngleFactor:0},userLayers:[],selectedPaths:[],devicePixelRatio:2,
  NemoNativeObjectProject:Project,NemoNativeObjectProjectFile:Files,NemoApplicationMcpTransport:Transport,NemoNativeApplicationAdapter:SDK,NemoNativeObjectVector:Vector,
  NemoNativeObjectUI:{refreshTiming(){},refreshPolicy(){}},updateUI:()=>log.push('publish-'+revision),showToast:s=>log.push(s)};
 require('./helpers/native-viewport-underlay.cjs').install(root);
 async function invoke(command,args){const q=args&&(args.request||JSON.parse(args.requestJson));
  if(command==='nemo_native_status'||command==='nemo_native_object_client_status')return {...info(),available:installed};
  if(command==='nemo_native_object_bootstrap'){installed=true;generation++;revision=0;records.clear();return {...info(),requestId:q.requestId,resourceCount:0,viewportAvailable:false};}
  if(command==='nemo_native_object_reference')return {...info(),requestId:q.requestId};
  if(command==='nemo_native_object_preview'){log.push('present-'+q.contentRevision);return {status:'presented',workId:'work-'+generation+'-'+revision,viewGeneration:1,...(q.viewMode==='stored-camera'?{view:{mode:'stored-camera',cameraAvailable:true,documentToComposition:[2,0,0,2,-160,-90],compositionToDocument:[.5,0,0,.5,80,45]}}:options.canvasView?{view:options.canvasView}:{})};}
  if(command==='nemo_native_revision_sync'){
   if(q.action==='binding')return {...info(),subscriptionId:'subscription-'+generation};
   if(q.action==='subscribe'){binding=q.binding;return q.binding;}
   if(q.action==='acknowledge'){if(ack)await ack.promise;return null;}if(q.action==='disconnect')return null;
  }
  if(command==='nemo_native_object_dispatch'){assert.equal(q.operation,'command.document.object.undo');undos.push(revision);revision++;
   return {apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:revision,ok:true,result:{applied:true}};}
  assert.equal(command,'nemo_native_dispatch');calls.push(structuredClone(q));
  if(q.operation==='command.document.object.fill.brush'||q.operation==='command.document.object.brush.bitmap.stroke'){assert.equal(q.expectedRevision,revision);revision++;
   return {apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:revision,ok:true,result:{applied:true,historyEntriesAdded:1}};}
  assert.equal(q.operation,'command.document.object.vector.edit');
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
 const paper=require('./helpers/native-import-paper.cjs').load();root.Point=paper.Point;root.Segment=paper.Segment;root.CompoundPath=paper.CompoundPath;
 root.Path=function(options){assert.equal(options&&options.insert,false,'all geometry must be detached');return new paper.Path(options);};
 root.Path.Arc=function(options){assert.equal(options.insert,false);return new paper.Path.Arc(options);};
 for(const name of ['pushUndo','ensureKeyframe','saveActiveLayerFrame','tagOwner','renderOS','applyShadowBrushTag'])root[name]=()=>assert.fail('legacy writer reached: '+name);
 root.SMEngineBridge={isEnabled:()=>true,nativeEditGuard:{allow:()=>!native},screenToWorld:()=>assert.fail('stale legacy viewport used'),suspend:()=>assert.fail('legacy engine suspended')};
 root.window=root;root.console=console;root.TextEncoder=TextEncoder;root.Date=class extends Date{static now(){return clock;}};root.performance={now:()=>clock};const context=vm.createContext(root),tools=fs.readFileSync(path.join(ROOT,'src/js/tools.js'),'utf8');
 for(const name of ['constrainSquare','constrainAngle45','applyStrokeStyle','applyPressureCurve','brushAngleWidthMul','brushAngleWidths',
  'buildWidthProfile','widthAtFrac','simplifyIfNeeded','buildCenterSegmentsFromRawStroke','taperFloorWidth','applyTaperToCenterSegments',
  'sampleVectorBrushCenterline','_polylineCrossCandidates','_ribbonHasRealSelfIntersections','_flattenResolvedRibbon','_cleanupSelfIntersectingOutline',
  'buildVariableWidthPath','buildClosedRingOutline','_polyClosestPair'])vm.runInContext(baselineFunction(tools,name),context,{filename:'tools.js:'+name});
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/stroke-modeler.js'),'utf8'),context,{filename:'stroke-modeler.js'});
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/adapters/native-object-draw-paint.js'),'utf8'),context,{filename:'native-object-draw-paint.js'});
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/adapters/native-object-draw-symmetry.js'),'utf8'),context,{filename:'native-object-draw-symmetry.js'});
 // Required like the Fill desktop: its command payload must be built in the SDK's own realm.
 root.NemoNativeDrawBitmap=require('../src/js/adapters/native-object-draw-bitmap.js');
 root.NemoNativeFillDesktop=require('../src/js/adapters/native-fill-desktop.js');
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/adapters/native-object-draw-resize.js'),'utf8'),context,{filename:'native-object-draw-resize.js'});
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/adapters/native-object-shape-tool.js'),'utf8'),context,{filename:'native-object-shape-tool.js'});
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/draw-bridge.js'),'utf8'),context,{filename:'draw-bridge.js'});
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/shape-bridge.js'),'utf8'),context,{filename:'shape-bridge.js'});
 return {perspectiveMode:value=>perspectiveMode=value,root,context,paper,advance:(ms=16)=>clock+=ms,session,area,canvas,zoom,fit,ruler,doc,body,calls,records,log,events,info,flags,undos,setRect:r=>rect=r,refuse:code=>refusal=code,hold:g=>hold=g,mutate:fn=>replyChange=fn,
  async external(g){ack=g;const from=revision;revision++;return events.get('nemo-native-revision')({payload:{instanceId:'instance-A',documentId:info().documentId,lifecycleGeneration:generation,requestId:'external-'+revision,fromRevision:from,toRevision:revision}});},
  async gesture(end={clientX:240,clientY:210}){area.fire('pointerdown');area.fire('pointermove',end);area.fire('pointerup',end);return root.NemoNativeFreehandToolController.whenSettled();}};
}
function send(f,kind,x,y,extra={}){return f.canvas.fire(kind,{clientX:40+x*2,clientY:50+y*2,...extra});}
function settled(f){return f.root.NemoNativeFreehandToolController.whenSettled();}
function plain(value){return JSON.parse(JSON.stringify(value));}
test('ordinary draw capture reaches actual Project queue/SDK once with the full unsmoothed baseline polyline and both paints',async()=>{
 const f=await fixture();send(f,'pointerdown',20,20);send(f,'pointermove',45,30);send(f,'pointermove',80,65);
 assert.equal(f.calls.length,0);assert.equal(f.records.size,0);assert.equal(f.paper.project.activeLayer.children.length,0);assert.equal(f.body.children.length,1);
 send(f,'pointerup',100,80);assert.equal((await settled(f)).status,'applied');assert.equal(f.calls.length,1);
 const r=f.calls[0].payload.record;assert.deepEqual(r.geometry.segments.map(s=>[s.point.x,s.point.y]),[[20,20],[45,30],[80,65],[100,80]]);
 assert.equal(r.geometry.closed,false);assert.equal(r.vector.hasRealStroke,true);assert.equal(r.vector.opacity,.5);assert.equal(r.fill.a,128/255);
 assert.deepEqual(f.root.state.layers[0].frames,[]);assert.equal(f.paper.project.activeLayer.children.length,0);assert.deepEqual(f.log.slice(-2),['publish-1','present-1']);
});
test('actual pen pressure, OneEuro filter, last-pressure hold and the full dense width profile survive one native ribbon',async()=>{
 const f=await fixture();Object.assign(f.root.state,{vectorBrush:true,fillEnabled:false});
 // Frozen baseline8ce29633 pressure(.2@1000,.8@1016,.8@1032), width4,min5%,max170%.
 const expected=[1.52,4.984255439279336,5.419698207570726];
 send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.2});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:0});
 assert.equal((await settled(f)).status,'applied');assert.equal(f.calls.length,1);const r=f.calls[0].payload.record,p=r.vectorExt.pressure;
 assert.equal(r.vector.hasRealStroke,false);assert.equal(r.vector.stroke,null);assert.equal(r.geometry.closed,true);assert.equal(r.fill.r,17/255);
 assert.equal(r.vector.strokeWidth,1);assert.equal(r.vector.strokeCap,'butt');assert.equal(r.vector.strokeJoin,'miter');
 assert.deepEqual(p.widthProfile.map(s=>s.width),expected);assert.deepEqual(p.centerSegments.map(s=>s.width),expected);
 assert.deepEqual(p.centerSegments.map(s=>[s.point.x,s.point.y]),[[20,20],[50,30],[80,45]]);assert.equal(p.widthProfile.length,3);
 const dict=require('../src/js/adapters/native-object-vector-stroke.js').toDict(r);assert.equal(dict.isVectorBrush,true);assert.deepEqual(dict.widthProfile,p.widthProfile);
 assert.equal(f.paper.project.activeLayer.children.length,0);
});
test('smoothing keeps baseline detached geometry while the dense pressure sequence is preserved independently of anchor fit',async()=>{
 const f=await fixture();Object.assign(f.root.state,{vectorBrush:true,fillEnabled:false,smoothing:10,taperEnds:true,brushAngleFactor:40,brushAngle:30});
 send(f,'pointerdown',10,10,{pointerType:'pen',pressure:.25});const coalesced=[];
 for(let i=1;i<=20;i++)coalesced.push({clientX:40+(10+i*3)*2,clientY:50+(10+i)*2,pointerType:'pen',pressure:i%2?.35:.75});
 f.advance();send(f,'pointermove',70,30,{getCoalescedEvents:()=>coalesced});f.advance();send(f,'pointerup',74,33,{pointerType:'pen',pressure:.4});
 assert.equal((await settled(f)).status,'applied');const p=f.calls[0].payload.record.vectorExt.pressure;
 assert.equal(p.widthProfile.length,22,'no coalesced pressure samples dropped');assert.ok(p.centerSegments.length<22,'baseline explicit Smooth fits centerline');
 assert.equal(p.widthProfile[0].width,.6);assert.equal(p.widthProfile.at(-1).width,.6);assert.ok(p.widthProfile.some(s=>s.width>.6));assert.equal(f.paper.project.activeLayer.children.length,0);
});
test('the Ready modeler takes the presented scale; a stretched canvas still refuses (#2343 B1)',async()=>{
 async function stroke(options,camera){const f=await fixture('draw',true,options),scales=[],create=f.root.SMStrokeModeler.create;
  if(camera){f.root.state.cameraView=true;await f.session.presentPreview(0);}
  f.root.SMStrokeModeler.create=(level,scale)=>{scales.push(scale);return create(level,scale);};
  Object.assign(f.root.state,{stabilizer:4,smoothing:2,fillEnabled:false});
  send(f,'pointerdown',20,20);f.advance();send(f,'pointermove',40,40);f.advance();send(f,'pointermove',60,30);f.advance();send(f,'pointerup',80,50);
  return {f,scales,status:(await settled(f)).status};}
 // The canvas has the document's aspect (2 CSS px per unit); the locked camera view presents the document at 2x within
 // it, so the modeler's scale is 4 CSS px per document unit, the baseline's view.zoom. The element box alone gives 2.
 const plain=await stroke({}),camera=await stroke({},true);
 assert.equal(plain.status,'applied');assert.deepEqual(plain.scales,[2]);
 assert.equal(camera.status,'applied');assert.equal(camera.f.calls.length,1);assert.deepEqual(camera.scales,[4]);
 // CSS layout rounding of the fitted canvas is not a stretch.
 const rounded=await stroke({rect:{left:40,top:50,width:640,height:360.25}});
 assert.equal(rounded.status,'applied');assert.equal(rounded.f.calls.length,1);
 // A canvas stretched to another shape (B1: 1038x504 for 1920x1080) is presented non-uniformly and refuses before dispatch.
 const stretched=await stroke({rect:{left:40,top:50,width:640,height:270}});
 assert.notEqual(stretched.status,'applied');assert.equal(stretched.f.calls.length,0,'nothing dispatched');assert.deepEqual(stretched.scales,[]);
});
test('both existing moving-average and Ready modeler input paths retain the actual release endpoint and one transaction',async()=>{
 for(const stabilizer of [2,4]){const f=await fixture();Object.assign(f.root.state,{stabilizer,smoothing:2,fillEnabled:false});
  send(f,'pointerdown',20,20);f.advance();send(f,'pointermove',40,40);f.advance();send(f,'pointermove',60,30);f.advance();send(f,'pointerup',80,50);
  assert.equal((await settled(f)).status,'applied');assert.equal(f.calls.length,1);const last=f.calls[0].payload.record.geometry.segments.at(-1).point;
  if(stabilizer===2)assert.deepEqual(last,{x:80,y:50});else assert.ok(Math.hypot(last.x-80,last.y-50)<1,'same reference modeler EOS catchup');
  assert.equal(f.paper.project.activeLayer.children.length,0);
 }
});
test('an outline past 256 anchors is refitted into one record; the 65536 full envelope stays a hard refusal with no split commit or local writer',async()=>{
 const anchors=await fixture();send(anchors,'pointerdown',20,20);
 const many=Array.from({length:256},(_,i)=>({clientX:40+(21+i)*2,clientY:100+i%2}));send(anchors,'pointermove',276,25,{getCoalescedEvents:()=>many});send(anchors,'pointerup',278,25);
 assert.equal((await settled(anchors)).status,'applied');assert.equal(anchors.calls.length,1);
 const fitted=anchors.calls[0].payload.record.geometry.segments;assert.ok(fitted.length>=2&&fitted.length<=256,String(fitted.length));
 assert.deepEqual([plain(fitted[0].point),plain(fitted[fitted.length-1].point)],[{x:20,y:20},{x:278,y:25}],'the refit keeps both ends');
 assert.equal(anchors.paper.project.activeLayer.children.length,0);
 const bytes=await fixture();Object.assign(bytes.root.state,{vectorBrush:true,fillEnabled:false});send(bytes,'pointerdown',20,20,{pointerType:'pen',pressure:.5});
 const dense=Array.from({length:900},(_,i)=>({clientX:80+i/20,clientY:90+i%2/100,pointerType:'pen',pressure:.3+i%10/20}));
 send(bytes,'pointermove',42,21,{getCoalescedEvents:()=>dense});send(bytes,'pointerup',43,21,{pointerType:'pen',pressure:.5});const result=await settled(bytes);
 assert.equal(result.status,'too_large');assert.equal(result.limit,65536);assert.ok(result.bytes>65536);assert.equal(bytes.calls.length,0);assert.equal(bytes.records.size,0);
 assert.equal(bytes.paper.project.activeLayer.children.length,0);
});
test('pressure record between4096 and65536 travels as one unchanged request through the real SDK',async()=>{
 const f=await fixture();Object.assign(f.root.state,{vectorBrush:true,fillEnabled:false});send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});
 const moves=Array.from({length:35},(_,i)=>({clientX:82+i,clientY:90+i/2,pointerType:'pen',pressure:.2+i%5/10}));send(f,'pointermove',40,28,{getCoalescedEvents:()=>moves});send(f,'pointerup',41,29,{pointerType:'pen',pressure:.6});
 assert.equal((await settled(f)).status,'applied');assert.equal(f.calls.length,1);assert.ok(Buffer.byteLength(JSON.stringify(f.calls[0]))>4096);assert.ok(Buffer.byteLength(JSON.stringify(f.calls[0]))<=65536);
 assert.equal(f.calls[0].payload.record.vectorExt.pressure.widthProfile.length,37);
});
test('chrome, cancellation, Escape and lost capture remain usable or abort without commands or legacy writes',async()=>{
 const chrome=await fixture();for(const target of [chrome.zoom,chrome.fit,chrome.ruler]){assert.equal(target.fire('pointerdown').prevented,false);target.fire('pointerup');}assert.equal(chrome.calls.length,0);
 for(const action of ['pointercancel','lostpointercapture','Escape']){const f=await fixture();send(f,'pointerdown',20,20);send(f,'pointermove',45,30);
  if(action==='Escape')f.doc.fire('keydown',{key:'Escape'});else send(f,action,50,40);
  send(f,'pointerup',60,50);assert.equal(f.calls.length,0,action);assert.equal(f.body.children.length,0,action);assert.equal(f.paper.project.activeLayer.children.length,0);}
});
test('unsupported brush cells are explicit without native or legacy fallback',async()=>{
 const cases=[s=>Object.assign(s,{tool:'fillbrush',fillBrushMode:'front',fillBrushSize:40}),s=>s.bitmapBrushOn=true,s=>s.brushPreset='my-custom-brush',
  // Plain-stroke symmetry commits natively since G23 (#2361); with a pressure brush it stays refused.
  s=>s.shadowMode=true,s=>Object.assign(s,{vectorBrush:true,fillEnabled:false,pressureCurve:'unknown'}),s=>s.stabilizer=7];
 for(const change of cases){const f=await fixture();change(f.root.state);const e=send(f,'pointerdown',20,20);send(f,'pointerup',60,50);assert.equal(e.prevented,true);assert.equal(f.calls.length,0);assert.ok(f.log.some(s=>/unavailable|not available|requires/.test(s)),f.log.join(','));}
});
test('owner/frame/style/selection changes and queued external ACK fences cannot commit a stale drawing',async()=>{
 for(const change of [f=>f.root.state.currentFrame=1,f=>f.root.state.brushSize=6,f=>f.root._layerSel=[],f=>f.root.state={...f.root.state,brushSize:6},async f=>f.session.newProject({canvasW:320,canvasH:180,fps:24},{commit:meta=>Files.projectView(f.root,meta)})]){
  const f=await fixture();send(f,'pointerdown',20,20);await change(f);send(f,'pointerup',80,50);assert.equal(f.calls.length,0);assert.equal(f.body.children.length,0);
 }
 const f=await fixture(),wait=gate(),ack=f.external(wait);await turn();send(f,'pointerdown',20,20);assert.equal(f.calls.length,0);wait.resolve();await ack;
 send(f,'pointerdown',20,20);send(f,'pointerup',60,50);assert.equal((await settled(f)).status,'applied');assert.equal(f.calls[0].expectedRevision,1);
 const queued=await fixture();send(queued,'pointerdown',20,20);send(queued,'pointerup',80,50);queued.root._layerSel=[];assert.equal((await settled(queued)).status,'unavailable');assert.equal(queued.calls.length,0);
});
test('pending/refused/indeterminate results stay on the existing native queue and same-ID recovery protocol',async()=>{
 const f=await fixture(),hold=gate();f.hold(hold);send(f,'pointerdown',20,20);send(f,'pointerup',60,50);await turn();assert.equal(f.calls.length,1);assert.equal(f.session.readPresentedObjectSession(),null);
 send(f,'pointerdown',20,20);send(f,'pointerup',70,60);assert.equal(f.calls.length,1);hold.resolve();assert.equal((await settled(f)).status,'applied');
 const refused=await fixture();refused.refuse('unavailable');send(refused,'pointerdown',20,20);send(refused,'pointerup',60,50);assert.equal((await settled(refused)).status,'unavailable');assert.equal(refused.records.size,0);
 const bad=await fixture();bad.mutate(r=>r.result.strokeId='foreign');send(bad,'pointerdown',20,20);send(bad,'pointerup',60,50);assert.equal((await settled(bad)).status,'indeterminate');assert.equal(bad.calls.length,2);assert.equal(bad.calls[0].requestId,bad.calls[1].requestId);assert.equal(bad.records.size,1);
 send(bad,'pointerdown',20,20);send(bad,'pointerup',70,60);await turn();assert.equal(bad.calls.length,2);
});

test('invalid raw coordinates/pressure and foreign coalesced pointers never normalize into document geometry',async()=>{
 for(const extra of [{clientX:NaN},{pressure:NaN},{pressure:1.1},{pressure:-.1},{getCoalescedEvents:()=>[{clientX:120,clientY:100,pointerId:2}]}]){
  const f=await fixture();Object.assign(f.root.state,{vectorBrush:true,fillEnabled:false});send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});
  send(f,'pointermove',40,30,extra);send(f,'pointerup',60,50);assert.equal(f.calls.length,0);assert.equal(f.body.children.length,0);assert.equal(f.paper.project.activeLayer.children.length,0);
 }
 const f=await fixture();send(f,'pointerdown',20,20,{pressure:NaN});send(f,'pointerup',60,50);assert.equal(f.calls.length,0);
});
test('the extracted shared filter is exactly the retained baseline implementation over duplicate timestamps, jitter and fast pressure changes',()=>{
 const Helper=require('../src/js/adapters/native-object-shape-tool.js'),now=Helper.createPressureFilter();
 // Exact outputs captured from the unchanged baseline8ce29633 function.
 const inputs=[[.2,100],[.2,100],[.25,116],[.18,132],[.9,148],[.1,164],[1,1000]];
 const expected=[.2,.2,.22014008091625614,.21456524470540866,.8240423075783248,.427282064505994,.9900262035983824];
 inputs.forEach(([p,t],i)=>assert.equal(now(p,t),expected[i]));
});
test('ordinary inactive native still reaches the retained browser draw route; missing native helper remains closed',async()=>{
 const inactive=await fixture('draw',false);assert.throws(()=>send(inactive,'pointerdown',20,20),/stale legacy viewport used/);assert.equal(inactive.calls.length,0);
 const missing=await fixture();vm.runInContext('window.NemoNativeObjectShapeTool=undefined;delete window.SMEngineBridge.nativeEditGuard;',missing.context);
 assert.equal(send(missing,'pointerdown',20,20).prevented,true);send(missing,'pointerup',60,50);assert.equal(missing.calls.length,0);assert.equal(missing.zoom.fire('pointerdown').prevented,false);
});

test('native solid fill-only and dashed plain brush carry their actual baseline styles without pressure fields',async()=>{
 const fill=await fixture();fill.root.state.strokeEnabled=false;send(fill,'pointerdown',20,20);send(fill,'pointerup',60,50);assert.equal((await settled(fill)).status,'applied');
 const r=fill.calls[0].payload.record;assert.equal(r.vector.stroke,null);assert.equal(r.vector.hasRealStroke,false);assert.equal(r.geometry.closed,false);assert.equal(r.fill.a,128/255);assert.equal(r.vectorExt,undefined);
 const dash=await fixture();Object.assign(dash.root.state,{fillEnabled:false,strokeStyle:'dashed',dashOffset:1.5,strokeCap:'square'});
 send(dash,'pointerdown',20,20);send(dash,'pointerup',60,50);assert.equal((await settled(dash)).status,'applied');assert.deepEqual(dash.calls[0].payload.record.vector.dashArray,[10,7.2]);assert.equal(dash.calls[0].payload.record.vector.dashOffset,1.5);assert.equal(dash.calls[0].payload.record.vector.strokeCap,'square');
});

test('a whole freehand loop remains one transaction even when its release returns to the initial point',async()=>{
 const f=await fixture();send(f,'pointerdown',20,20);send(f,'pointermove',60,20);send(f,'pointermove',60,50);send(f,'pointermove',20,50);send(f,'pointerup',20,20);
 assert.equal((await settled(f)).status,'applied');assert.equal(f.calls.length,1);assert.equal(f.calls[0].payload.record.geometry.segments.length,5);assert.equal(f.calls[0].payload.record.geometry.closed,false);
});

test('a closed pressure fill past 256 anchors commits one closed refitted record (#2343 B1)',async()=>{
 const f=await fixture();Object.assign(f.root.state,{vectorBrush:true,strokeEnabled:false,fillEnabled:true,smoothing:0});
 const at=(i)=>[20+i*.9,90+40*Math.sin(i/12)];send(f,'pointerdown',...at(0),{pointerType:'pen',pressure:.5});
 const dense=Array.from({length:300},(_,i)=>{const [x,y]=at(i+1);return {clientX:40+x*2,clientY:50+y*2,pointerType:'pen',pressure:.5};});
 send(f,'pointermove',...at(300),{getCoalescedEvents:()=>dense});send(f,'pointerup',...at(301),{pointerType:'pen',pressure:.5});
 const result=await settled(f);assert.equal(result.status,'applied',JSON.stringify(result).slice(0,300));assert.equal(f.calls.length,1);
 const g=plain(f.calls[0].payload).record.geometry;assert.equal(g.closed,true);assert.ok(g.segments.length>=2&&g.segments.length<=256,String(g.segments.length));
});
test('exactly256 ordinary polyline anchors are admitted as one full record without a size-driven fit',async()=>{
 const f=await fixture();send(f,'pointerdown',20,20);const points=Array.from({length:254},(_,i)=>({clientX:82+i*2,clientY:90+i%3}));
 send(f,'pointermove',274,21,{getCoalescedEvents:()=>points});send(f,'pointerup',276,21);assert.equal((await settled(f)).status,'applied');
 assert.equal(f.calls.length,1);assert.equal(f.calls[0].payload.record.geometry.segments.length,256);assert.ok(Buffer.byteLength(JSON.stringify(f.calls[0]))<=65536);
});

test('pressure taps draw the baseline dot, never throw, and allow the next gesture (#2361 BRUSH-CLICK)',async()=>{
 for(const smoothing of [0,2,10,100])for(const stabilizer of [0,4]){
  const f=await fixture();Object.assign(f.root.state,{vectorBrush:true,fillEnabled:false,smoothing,stabilizer});
  const key=JSON.stringify({smoothing,stabilizer});
  send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});send(f,'pointerup',20,20,{pointerType:'pen',pressure:0});
  const tap=await settled(f);assert.equal(tap.status,'applied',key+JSON.stringify(f.log));
  assert.ok(!f.log.some(m=>/getDistance|not a function/.test(String(m))),key+JSON.stringify(f.log));
  assert.equal(f.calls.length,1);assert.equal(f.records.size,1);assert.equal(f.body.children.length,0);assert.equal(f.area.captured.size,0);
  // The closed ribbon outline around the tapped point: where the baseline draws its dot.
  const dot=f.calls[0].payload.record.geometry;assert.equal(dot.closed,true);
  const xs=dot.segments.map(g=>g.point.x),ys=dot.segments.map(g=>g.point.y);
  assert.ok(Math.min(...xs)<20&&Math.max(...xs)>20&&Math.min(...ys)<20&&Math.max(...ys)>20,key);
  assert.equal(f.root.NemoNativeFreehandToolController.isDragging(),false);assert.ok(f.session.readPresentedObjectSession(),'no queue pending remains');
  f.advance();send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.3});f.advance();send(f,'pointermove',60,40,{pointerType:'pen',pressure:.7});f.advance();send(f,'pointerup',100,60,{pointerType:'pen',pressure:0});
  assert.equal((await settled(f)).status,'applied',key);assert.equal(f.session.info().contentRevision,2);
 }
});
test('a pressure tap with Fill on sends the dot, and its one-anchor linked fill is left out instead of refused (#2361 BRUSH-CLICK)',async()=>{
 // The fixture's fake host only serves a single create, so the request is read from the recorded call.
 for(const [smoothing,records] of [[0,2],[2,1]]){
  const f=await fixture();Object.assign(f.root.state,{vectorBrush:true,fillEnabled:true,smoothing,stabilizer:0});
  send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});send(f,'pointerup',20,20,{pointerType:'pen',pressure:0});await settled(f);
  assert.ok(!f.log.some(m=>/getDistance|not a function/.test(String(m))),JSON.stringify(f.log));
  assert.equal(f.calls.length,1);const q=plain(f.calls[0].payload);
  assert.equal(q.command,records===2?'object.vector.createMany':'object.vector.create');
  const ribbon=(q.records||[q.record]).slice(-1)[0];assert.equal(ribbon.geometry.closed,true);assert.equal((q.records||[1]).length,records);
 }
});
test('a mouse click with a plain smoothed brush leaves the document untouched and shows no error (#2361 BRUSH-CLICK)',async()=>{
 for(const smoothing of [0,2,50]){
  const f=await fixture();Object.assign(f.root.state,{vectorBrush:false,fillEnabled:false,smoothing});
  send(f,'pointerdown',20,20);send(f,'pointerup',20,20);const tap=await settled(f);
  // smoothing 0 keeps the baseline's two coincident anchors; a smoothed tap collapses to one, which has no extent and is dropped.
  if(smoothing===0)assert.equal(tap.status,'applied');else{assert.equal(tap.status,'cancelled');assert.equal(f.calls.length,0);assert.equal(f.records.size,0);}
  assert.ok(!f.log.some(m=>typeof m==='string'&&!/^(publish|present)-/.test(m)),JSON.stringify(f.log));
  f.advance();send(f,'pointerdown',20,20);f.advance();send(f,'pointermove',60,40);f.advance();send(f,'pointerup',100,60);
  assert.equal((await settled(f)).status,'applied');
 }
});
test('the brush draws on the active layer after the row selection was cleared, and still refuses other or several rows (#2361 BRUSH-CLICK)',async()=>{
 const f=await fixture();f.root._layerSel=[];
 send(f,'pointerdown',20,20);send(f,'pointermove',60,40);send(f,'pointerup',100,60);assert.equal((await settled(f)).status,'applied');assert.equal(f.calls.length,1);
 assert.ok(!f.log.some(m=>/Select one native Drawing layer/.test(String(m))),JSON.stringify(f.log));
 for(const selection of [[1],[0,1]]){
  const g=await fixture();g.root._layerSel=selection;send(g,'pointerdown',20,20);
  assert.ok(g.log.some(m=>/Select one native Drawing layer/.test(String(m))),JSON.stringify(selection));assert.equal(g.root.NemoNativeFreehandToolController.isDragging(),false);
 }
 const h=await fixture();h.root._layerSel=[];send(h,'pointerdown',20,20);send(h,'pointermove',60,40);h.root._layerSel=[0];send(h,'pointerup',100,60);
 assert.equal((await settled(h)).status,'idle');assert.equal(h.calls.length,0);assert.equal(h.root.NemoNativeFreehandToolController.isDragging(),false);
});
test('native drawing refuses unavailable, stale and enabled Perspective projections before a document command',async()=>{
 for(const mode of ['unavailable','stale','enabled']){
  const f=await fixture();f.perspectiveMode(mode);
  Object.defineProperty(f.root.state,'perspectiveEnabled',{get(){throw new Error('legacy Perspective read');},set(){throw new Error('legacy Perspective write');}});
  f.area.fire('pointerdown');f.area.fire('pointerup',{clientX:240,clientY:210});await turn();
  assert.equal(f.calls.length,0,mode);assert.equal(f.body.children.length,0,mode);
 }
});

test('locked camera inversely maps actual freehand samples and cancels a view-changed stroke',async()=>{
 const f=await fixture();f.root.state.cameraView=true;await f.session.presentPreview(0);
 send(f,'pointerdown',20,20);f.advance();send(f,'pointermove',60,40);f.advance();send(f,'pointerup',100,80);
 assert.equal((await settled(f)).status,'applied');
 const segments=f.calls[0].payload.record.geometry.segments;assert.deepEqual(segments[0].point,{x:90,y:55});
 assert.deepEqual(segments.at(-1).point,{x:130,y:85});
 const stale=await fixture();send(stale,'pointerdown',20,20);stale.root.state.cameraView=true;send(stale,'pointerup',100,80);
 assert.equal(stale.calls.length,0);assert.equal(stale.body.children.length,0);
});


test('native freehand capture refuses an absent injected mapping even with the global underlay present',async()=>{
 const f=await fixture(),tool=f.root.NemoNativeObjectShapeTool;
 f.root.NemoNativeObjectShapeTool={...tool,create:(root,ports)=>tool.create(root,{...ports,mapping:undefined})};
 assert.ok(f.root.NemoNativeViewportUnderlay.canvasMapping);
 const down=f.area.fire('pointerdown');f.area.fire('pointerup',{clientX:240,clientY:210});await turn();
 assert.equal(down.prevented,true);assert.equal(f.calls.length,0);assert.equal(f.records.size,0);assert.equal(f.body.children.length,0);
});


test('queued freehand uses the actual action lease while public presentation stays hidden and rejects a changed view',async()=>{
 for(const changed of [false,true]){
  const f=await fixture(),vector=f.root.NemoNativeObjectVector;let queued=0;
  f.root.NemoNativeObjectVector={...vector,create:owner=>vector.create({...owner,runResourceCommand:action=>owner.runResourceCommand(ctx=>{
   queued++;assert.equal(f.session.readPresentedObjectSession(),null,'public proof stays fenced during the actual resource action');
   if(changed)f.root.state.cameraView=true;
   return action(ctx);
  })})};
  f.area.fire('pointerdown');f.area.fire('pointerup',{clientX:240,clientY:210});
  const result=await f.root.NemoNativeFreehandToolController.whenSettled();
  assert.equal(queued,1);assert.equal(result.status,changed?'unavailable':'applied');
  assert.equal(f.calls.length,changed?0:1);assert.equal(f.records.size,changed?0:1);assert.equal(f.session.info().contentRevision,changed?0:1);
  assert.equal(f.body.children.length,0);
 }
});

test('drawMode behind sends the stroke under the layer artwork in the same single native create (G15)',async()=>{
 const f=await fixture();f.root.state.drawMode='behind';send(f,'pointerdown',20,20);send(f,'pointermove',45,30);send(f,'pointerup',100,80);
 assert.equal((await settled(f)).status,'applied');assert.equal(f.calls.length,1);
 assert.equal(f.calls[0].payload.placement,'behind');assert.equal(f.calls[0].payload.command,'object.vector.create');
 const front=await fixture();send(front,'pointerdown',20,20);send(front,'pointermove',45,30);send(front,'pointerup',100,80);await settled(front);
 assert.equal(front.calls[0].payload.placement,undefined,'front stays the absent default');
});
test('Trim Stroke Ends asks native to trim the Draw commit with the original limit, and only when it is on',async()=>{
 const f=await fixture();Object.assign(f.root.state,{trimStrokeEnds:true,trimStrokeEndsMax:24});
 send(f,'pointerdown',20,20);send(f,'pointermove',45,30);send(f,'pointerup',100,80);
 assert.equal((await settled(f)).status,'applied');assert.equal(f.calls.length,1);
 assert.equal(f.calls[0].payload.trimEnds,24);assert.equal(f.calls[0].payload.command,'object.vector.create');
 for(const state of [{trimStrokeEnds:false,trimStrokeEndsMax:24},{trimStrokeEnds:true,trimStrokeEndsMax:0}]){
  const off=await fixture();Object.assign(off.root.state,state);send(off,'pointerdown',20,20);send(off,'pointermove',45,30);send(off,'pointerup',100,80);
  await settled(off);assert.equal(off.calls[0].payload.trimEnds,undefined,JSON.stringify(state));
 }
 const sym=await fixture();Object.assign(sym.root.state,{trimStrokeEnds:true,trimStrokeEndsMax:24,symmetryEnabled:true,symmetryExtend:true});
 send(sym,'pointerdown',20,20);send(sym,'pointermove',45,30);send(sym,'pointerup',100,80);
 // This fixture's native accepts single creates only; the outgoing request is what is checked here.
 await settled(sym);assert.equal(sym.calls.length,1);assert.equal(sym.calls[0].payload.command,'object.vector.createMany');
 assert.equal(sym.calls[0].payload.trimEnds,24,'the copies share the original stroke\'s cuts natively');
});
// U33 (#2362): Labs predictive stroke reshapes the native freehand stroke before its single commit (the baseline runs
// onStroke on the committed path before the frame is saved), through the same recognizer as the legacy hook.
function predictive(f,on){
 const protos={};f.root.SMLabs={isOn:name=>on&&name==='predictive-stroke',register(n,d){protos[n]=d;},
  unmigratedDrawHooks:()=>Object.keys(protos).filter(n=>(protos[n].onStroke||protos[n].onPreview)&&!protos[n].nativeDraw&&f.root.SMLabs.isOn(n))};
 f.root.Path.Ellipse=function(options){assert.equal(options.insert,false);return new f.paper.Path.Ellipse(options);};
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/labs/predictive-stroke.js'),'utf8'),f.context,{filename:'predictive-stroke.js'});
}
test('a pressure ribbon asks native to trim it with its linked fill in the one createMany',async()=>{
 const f=await fixture();Object.assign(f.root.state,{trimStrokeEnds:true,trimStrokeEndsMax:24,vectorBrush:true});
 send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:.4});
 // The simulated host answers single creates only; the request is what is under test (see the createMany test below).
 await settled(f);assert.equal(f.calls.length,1);const p=plain(f.calls[0].payload);
 assert.deepEqual([p.command,p.trimEnds,p.records.length],['object.vector.createMany',24,2]);
 assert.equal(p.records[0].vectorExt.isLinkedFillCompanion,true);assert.ok(p.records[1].vectorExt.pressure,'the ribbon, above its fill');
});
test('Labs predictive stroke snaps a near-line and a near-circle in the one native commit, and is inert when off',async()=>{
 const line=await fixture();predictive(line,true);
 send(line,'pointerdown',10,40);for(const [x,y] of [[40,41],[70,40],[100,39],[130,40]])send(line,'pointermove',x,y);send(line,'pointerup',160,40);
 assert.equal((await settled(line)).status,'applied');assert.equal(line.calls.length,1);
 assert.deepEqual(line.calls[0].payload.record.geometry.segments.map(s=>[s.point.x,s.point.y]),[[10,40],[160,40]]);
 const circle=await fixture();predictive(circle,true);
 const ring=Array.from({length:25},(_,i)=>[100+60*Math.cos(i/24*2*Math.PI),90+60*Math.sin(i/24*2*Math.PI)]);
 send(circle,'pointerdown',...ring[0]);ring.slice(1,-1).forEach(p=>send(circle,'pointermove',...p));send(circle,'pointerup',...ring.at(-1));
 assert.equal((await settled(circle)).status,'applied');const g=circle.calls[0].payload.record.geometry;
 assert.equal(g.closed,true);assert.equal(g.segments.length,4,'the baseline Path.Ellipse over the stroke bounds');
 const off=await fixture();predictive(off,false);
 send(off,'pointerdown',10,40);for(const [x,y] of [[40,41],[70,40],[100,39],[130,40]])send(off,'pointermove',x,y);send(off,'pointerup',160,40);
 assert.equal((await settled(off)).status,'applied');assert.equal(off.calls[0].payload.record.geometry.segments.length,6);
});
// U34 (#2362): Labs multi-frame draw stamps the native freehand stroke on the other selected frames in the same single
// commit: the drawn frame takes Draw's key, each stamped frame its own copy key; the prototype names the frames only.
function multiframe(f,on){
 const protos={},toasts=[];f.root.SMLabs={isOn:name=>on&&name==='multiframe-draw',register(n,d){protos[n]=d;},
  unmigratedDrawHooks:()=>Object.keys(protos).filter(n=>(protos[n].onStroke||protos[n].onPreview)&&!protos[n].nativeDraw&&f.root.SMLabs.isOn(n))};
 f.root._sel={frames:[{layer:0,frame:3},{layer:2,frame:5},{layer:0,frame:0},{layer:0,frame:3},{layer:0,frame:99}]};
 f.root.SM=f.root.SM||{t:k=>k};f.root.showToast=t=>toasts.push(t);
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/labs/multiframe-draw.js'),'utf8'),f.context,{filename:'multiframe-draw.js'});
 return toasts;
}
test('Labs multi-frame draw stamps the native stroke on the other selected frames in one copy-keyed batch, and is inert when off',async()=>{
 const f=await fixture();multiframe(f,true);
 send(f,'pointerdown',10,40);for(const [x,y] of [[40,41],[70,40]])send(f,'pointermove',x,y);send(f,'pointerup',100,40);
 await settled(f);assert.equal(f.calls.length,1,'the native Drawing guard leaves a migrated hook available');
 const payload=plain(f.calls[0].payload);
 assert.equal(payload.command,'object.vector.batch');assert.equal(payload.keyframe,undefined,'the drawn frame keeps Draw\'s key');
 assert.deepEqual(payload.ops.map(op=>[op.op,op.op==='copy'?op.frame:op.record.target.frameScope.frame,op.keyframe||null]),[['create',0,null],['copy',3,'copy'],['copy',5,'copy']]);
 assert.deepEqual(payload.ops.slice(1).map(op=>[op.fromStep,op.record]),[[0,undefined],[0,undefined]],'every stamp copies the drawn step instead of resending it');
 const trim=await fixture();multiframe(trim,true);trim.root.state.trimStrokeEnds=true;trim.root.state.trimStrokeEndsMax=24;
 send(trim,'pointerdown',10,40);send(trim,'pointermove',40,41);send(trim,'pointerup',100,40);
 await settled(trim);assert.equal(trim.calls[0].payload.trimEnds,24,'the drawn stroke and its stamps share one native cut');
 const SDK=require('../src/js/adapters/native-application.js'),req=plain(trim.calls[0]);
 req.payload.ops.push({op:'delete',stableTarget:{contextId:'scene-root',frameScope:{kind:'authored',frame:0},layerUid:req.payload.ops[0].record.target.layerUid,strokeId:'s'}});
 assert.throws(()=>SDK.validateRequest(req),TypeError,'a trimmed batch creates only');
 const off=await fixture();multiframe(off,false);
 send(off,'pointerdown',10,40);send(off,'pointermove',40,41);send(off,'pointerup',100,40);
 await settled(off);assert.equal(off.calls[0].payload.command,'object.vector.create');
});

// #2361 G15 (draw-bridge.js commitStroke @1065e9d8): the fresh-profile default, a pressure brush with Fill on, commits
// the ribbon and its linked fill companion (the sampled centreline, closed, in the fill colour, below the ribbon) as one
// createMany; with Stroke off the stroke is one closed fill of its centreline, without a ribbon.
test('a pressure stroke with Fill on commits its ribbon and linked fill in one createMany, companion first',async()=>{
 const f=await fixture();Object.assign(f.root.state,{vectorBrush:true});
 send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:0});
 await settled(f);assert.equal(f.calls.length,1);const q=plain(f.calls[0].payload);
 assert.equal(q.command,'object.vector.createMany');assert.equal(q.records.length,2);
 const [fill,ribbon]=q.records;
 assert.deepEqual([fill.vectorExt.isLinkedFillCompanion,ribbon.vectorExt.isLinkedFillCompanion===true],[true,false]);
 assert.equal(fill.vectorExt.linkedFillId,ribbon.vectorExt.linkedFillId);assert.ok(fill.vectorExt.linkedFillId.length>0);
 assert.deepEqual([fill.geometry.closed,fill.vector.hasRealStroke,fill.fill.r],[true,false,0x44/255]);
 assert.equal(fill.vectorExt.pressure,undefined,'the companion is a plain fill');
 // The companion is the baseline sampler's centreline of the ribbon's own pressure centreline (rebuildVectorBrushOutline).
 const cs=ribbon.vectorExt.pressure.centerSegments.map(s=>({point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y],width:s.width}));
 const sampled=f.root.sampleVectorBrushCenterline(cs,ribbon.vectorExt.pressure.widthProfile).centerSegments;
 assert.deepEqual(fill.geometry.segments.map(s=>[s.point.x,s.point.y,s.handleIn.x,s.handleIn.y,s.handleOut.x,s.handleOut.y]),
   plain(sampled.map(s=>[s.point.x,s.point.y,s.handleIn.x,s.handleIn.y,s.handleOut.x,s.handleOut.y])),'the companion follows the sampled centreline');
 assert.equal(ribbon.fill.r,0x11/255,'the ribbon is painted in the stroke colour');
 // Mask mode: tagOwner tags the ribbon (the committed path); its linked fill stays an ordinary fill.
 const masked=await fixture();Object.assign(masked.root.state,{vectorBrush:true,maskMode:true,maskModeType:'subtract'});
 send(masked,'pointerdown',20,20);send(masked,'pointermove',50,30);send(masked,'pointerup',80,45);await settled(masked);
 assert.equal(masked.calls.length,1,masked.log.join(','));const [mfill,mribbon]=plain(masked.calls[0].payload.records);
 assert.deepEqual([mribbon.vectorExt.isMask,mribbon.vectorExt.maskMode],[true,'subtract']);
 assert.deepEqual([mfill.vectorExt.isMask,mfill.vectorExt.isLinkedFillCompanion],[undefined,true]);
});
test('a pressure stroke with Stroke off is one closed fill of its centreline',async()=>{
 const f=await fixture();Object.assign(f.root.state,{vectorBrush:true,strokeEnabled:false});
 send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:0});
 await settled(f);assert.equal(f.calls.length,1);const r=plain(f.calls[0].payload.record);
 assert.equal(f.calls[0].payload.command,'object.vector.create');
 assert.deepEqual([r.geometry.closed,r.vector.hasRealStroke,r.fill.r,r.vectorExt&&r.vectorExt.pressure],[true,false,0x44/255,undefined]);
 assert.deepEqual(r.geometry.segments.map(s=>[s.point.x,s.point.y]),[[20,20],[50,30],[80,45]]);
});

// A preset key the confirmed native catalog lacks (a deleted custom preset) draws plain, as the baseline's resolveBrushPreset
// null leaves applyBrushTexture a no-op. An unknown, changing or unreadable catalog keeps the key unavailable.
test('a preset key absent from the settled native catalog draws a plain stroke; an unconfirmed catalog refuses',async()=>{
 const draw=async(catalog)=>{const f=await fixture();f.root.NemoNativeCustomBrushUI={controller:()=>catalog.controller,exists:()=>catalog.exists,record:()=>null};
  f.root.state.brushPreset='deleted-custom';send(f,'pointerdown',10,40);send(f,'pointermove',40,41);send(f,'pointerup',100,40);await settled(f);return f;};
 const plainDraw=await draw({controller:{isPending:()=>false},exists:false});assert.equal(plainDraw.calls.length,1,plainDraw.log.join(','));
 const r=plain(plainDraw.calls[0].payload).record;
 assert.equal(r.vectorExt&&r.vectorExt.brushTexture,undefined,'no texture fields, as the baseline writes none');
 assert.deepEqual([r.vector.opacity,r.vector.stroke.r],[0.5,0x11/255],'the stroke is not camouflaged');
 for(const catalog of [{controller:null,exists:null},{controller:{isPending:()=>true},exists:false},{controller:{isPending:()=>false},exists:null},
  {controller:{isPending:()=>false},exists:true}]){
  const f=await draw(catalog);assert.equal(f.calls.length,0,JSON.stringify(catalog.exists));assert.ok(f.log.some(s=>/unavailable/.test(s)),f.log.join(','));
 }
});
// #2361 G16 option C: a built-in preset on a plain stroke commits one seeded, camouflaged anchor (applyBrushTexture
// @1065e9d8); native stamps its dabs from the preset and seed. No dab is sent.
test('a textured plain stroke commits one seeded anchor, camouflaged as the baseline hides it',async()=>{
 for(const fillEnabled of [true,false]){
  const f=await fixture();f.root.BRUSH_PRESETS={'charcoal-smooth':{}};Object.assign(f.root.state,{brushPreset:'charcoal-smooth',fillEnabled});
  send(f,'pointerdown',10,40);for(const [x,y] of [[40,41],[70,40]])send(f,'pointermove',x,y);send(f,'pointerup',100,40);
  await settled(f);assert.equal(f.calls.length,1);const q=plain(f.calls[0].payload);
  assert.equal(q.command,'object.vector.create','one anchor, no dab records');
  const t=q.record.vectorExt.brushTexture;
  assert.equal(t.brushTexturePreset,'charcoal-smooth');assert.ok(Number.isInteger(t.strokeSeed)&&t.strokeSeed>=0&&t.strokeSeed<=0xFFFFFFFF);
  assert.match(t.brushGroupId,/^bg[0-9a-z]+_[0-9]+$/);
  if(fillEnabled){
   assert.deepEqual([t.preTextureStroke,q.record.vector.stroke,q.record.vector.opacity],['#112233ff',null,0.5],'a filled anchor hides only its stroke');
  }else{
   assert.deepEqual([t.preTextureOpacity,q.record.vector.opacity,q.record.vector.stroke.r],[0.5,0,0x11/255],'an unfilled anchor hides itself');
  }
 }
 const off=await fixture();off.root.BRUSH_PRESETS={'charcoal-smooth':{}};Object.assign(off.root.state,{brushPreset:'charcoal-smooth',strokeEnabled:false});
 send(off,'pointerdown',10,40);send(off,'pointermove',40,41);send(off,'pointerup',100,40);await settled(off);
 assert.equal(off.calls[0].payload.record.vectorExt,undefined,'with Stroke off the baseline applies no texture');
});

test('a textured pressure ribbon commits one seeded, hidden ribbon anchor; its linked fill stays visible',async()=>{
 for(const fillEnabled of [false,true]){
  const f=await fixture();f.root.BRUSH_PRESETS={'chalk-round':{}};Object.assign(f.root.state,{brushPreset:'chalk-round',vectorBrush:true,fillEnabled});
  send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:0});
  await settled(f);assert.equal(f.calls.length,1);const q=plain(f.calls[0].payload);
  const ribbon=fillEnabled?q.records[1]:q.record;
  assert.equal(q.command,fillEnabled?'object.vector.createMany':'object.vector.create');
  assert.ok(ribbon.vectorExt.pressure.centerSegments.length>=2,'the anchor is the ribbon');
  assert.deepEqual([ribbon.vectorExt.brushTexture.brushTexturePreset,ribbon.vectorExt.brushTexture.preTextureOpacity,ribbon.vector.opacity],['chalk-round',0.5,0]);
  assert.ok(Number.isInteger(ribbon.vectorExt.brushTexture.strokeSeed));
  if(fillEnabled){const fill=q.records[0];assert.equal(fill.vectorExt.brushTexture,undefined);assert.equal(fill.vector.opacity,0.5,'the linked fill is not hidden');}
 }
});

// #2361 ribbon + texture A/B: the committed ribbon record carries the camouflage the baseline applyBrushTexture writes (the oracle's
// serP of a hidden pressure ribbon in the same ink and Opacity), with the seed and group the fixed Math.random derives.
test('a textured pressure ribbon commits the oracle camouflage: ink fill kept, no stroke, opacity 0, remembered opacity, seeded preset',async()=>{
 const oracle=JSON.parse(fs.readFileSync(path.join(__dirname,'native-object-vector-fixtures','brush-dab-oracle.json'),'utf8'));
 for(const c of oracle.ribbonCases){
  const f=await fixture();f.root.BRUSH_PRESETS={[c.preset]:{}};Object.assign(f.root.state,{brushPreset:c.preset,vectorBrush:true,fillEnabled:false});
  vm.runInContext('Math.random=function(){return .25;}',f.context);
  send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:0});
  await settled(f);assert.equal(f.calls.length,1);const r=plain(f.calls[0].payload.record);
  assert.deepEqual(r.fill,c.record.fill,c.id+' the ribbon stays the ink fill');
  assert.deepEqual(r.vector,c.record.vector,c.id+' no stroke, butt/miter defaults, opacity 0');
  const {brushGroupId,...tags}=r.vectorExt.brushTexture;assert.match(brushGroupId,/^bg[0-9a-z]+_250000$/,c.id);
  assert.deepEqual(tags,{brushTexturePreset:c.preset,preTextureOpacity:c.opacity,strokeSeed:1073741823},c.id);
  assert.equal(r.geometry.closed,true);assert.deepEqual(Object.keys(r.vectorExt).sort(),['brushTexture','pressure']);
 }
});

// #2361 Shadow Brush: applyShadowBrushTag @1065e9d8 over draw-bridge's commitStroke. The active swatch inks a plain
// stroke (max(1, width), round ends) or, for the Stroke-off pressure fill, fills it; a ribbon goes back to its open
// centreline; Shadow mode draws neither texture nor linked fill. Every record keeps its channel and swatch.
// The bake and the host's scoped temp files are simulated; the PNG stage, SDK, contract and command route are the real ones.
function bitmapPorts(f){
 const files=new Map(),removed=[],baked=[];
 const hex=fs.readFileSync(path.join(ROOT,'native-engine/tests/fixtures/object-raster-asset-attach/oracle.txt'),'utf8').split('\n')
  .filter(l=>l&&!l.startsWith('#')).map(l=>JSON.parse(l)).find(r=>r.name==='blue-2x1').hex,png=Buffer.from(hex,'hex');
 Object.assign(f.root.__TAURI__,{fs:{mkdir:async()=>{},writeFile:async(p,b)=>{files.set(p,b);},writeTextFile:async(p,t)=>{files.set(p,t);},
  remove:async p=>{removed.push(p);files.delete(p);},exists:async p=>files.has(p)},path:{tempDir:async()=>'/tmp',join:async(a,b)=>a+'/'+b}});
 f.root.crypto={randomUUID:()=>'00000000-0000-4000-8000-000000000001'};f.root.NemoNativeImportPngStage=require('../src/js/adapters/native-import-png-stage.js');
 const canvas={width:2,height:1,toBlob(cb){cb(new Blob([png]));}};
 f.root.SMBitmapBrush={bake(p,spec){baked.push({segments:p.segments.length,closed:p.closed,profile:p.data.bitmapPressureProfile,spec});return {canvas,minX:5,minY:6,w:40,h:20};}};
 return {files,removed,baked};
}
test('a bitmap-brush Draw stroke commits its camouflaged anchor, baked companion and staged PNG in one native command',async()=>{
 const f=await fixture(),{removed,baked}=bitmapPorts(f);
 f.root.BRUSH_PRESETS={'charcoal-smooth':{}};
 Object.assign(f.root.state,{bitmapBrushOn:true,bitmapTip:'grain',bitmapSpacing:10,bitmapScatter:5,bitmapOpacity:80,bitmapPressure:false,brushPreset:'charcoal-smooth'});
 send(f,'pointerdown',20,20);send(f,'pointermove',50,30);send(f,'pointerup',80,45);
 const done=await settled(f);assert.equal(done.status,'applied',JSON.stringify(done));assert.equal(f.calls.length,1);
 const q=f.calls[0],p=q.payload,ext=p.record.vectorExt;assert.equal(q.operation,'command.document.object.brush.bitmap.stroke');
 assert.deepEqual(Object.keys(p).sort(),['companion','documentSnapshotId','lifecycleGeneration','record','source']);
 // applyBitmapBrushTexture: the bake reads the committed path, and the bitmap replaces the texture preset.
 assert.equal(baked.length,1);assert.equal(baked[0].segments,p.record.geometry.segments.length);assert.equal(baked[0].profile,undefined);
 const spec=ext.bitmapBrush.bitmapBrushSpec;assert.deepEqual({...spec,seed:0},{tip:'grain',size:4,spacing:10,scatter:5,opacity:.8,color:spec.color,pressure:false,seed:0});
 assert.match(spec.color,/^#[0-9a-f]{8}$/);assert.ok(Number.isInteger(spec.seed));assert.equal(ext.brushTexture,undefined);
 // With a fill the anchor keeps it and only its stroke goes, remembered.
 assert.equal(p.record.vector.stroke,null);assert.equal(p.record.vector.hasRealStroke,false);assert.equal(ext.bitmapBrush.preTextureStroke,spec.color);assert.equal(p.record.vector.opacity,.5);
 assert.match(ext.bitmapBrush.brushGroupId,/^bmb/);assert.equal(p.record.target.strokeId,'');
 assert.deepEqual(p.companion,{x:25,y:16,width:40,height:20,opacity:1});
 assert.equal(p.source.kind,'file');assert.match(p.source.path,/^\/tmp\/nemo-native-import-00000000-0000-4000-8000-000000000001\/source-0000\.png$/);
 assert.ok(removed.includes(p.source.path),'a confirmed stroke cleans its staged PNG');
 assert.equal(f.paper.project.activeLayer.children.length,0,'no Raster or path is inserted');
});
test('a bitmap stroke drawn behind asks native for the bottom of the layer, its companion above it',async()=>{
 const f=await fixture();bitmapPorts(f);Object.assign(f.root.state,{bitmapBrushOn:true,drawMode:'behind'});
 send(f,'pointerdown',20,20);send(f,'pointermove',50,30);send(f,'pointerup',80,45);
 assert.equal((await settled(f)).status,'applied');assert.equal(f.calls[0].payload.placement,'behind');
 const front=await fixture();bitmapPorts(front);Object.assign(front.root.state,{bitmapBrushOn:true});
 send(front,'pointerdown',20,20);send(front,'pointermove',50,30);send(front,'pointerup',80,45);
 await settled(front);assert.equal(Object.prototype.hasOwnProperty.call(front.calls[0].payload,'placement'),false,'front is the absent default');
});
test('an unfilled bitmap stroke hides its anchor and carries its pressure profile; unsupported bitmap combinations stay refused',async()=>{
 const f=await fixture(),{baked}=bitmapPorts(f);
 Object.assign(f.root.state,{bitmapBrushOn:true,bitmapPressure:true,fillEnabled:false});
 send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:.5});
 assert.equal((await settled(f)).status,'applied');
 const r=f.calls[0].payload.record,ext=r.vectorExt.bitmapBrush;
 assert.equal(r.vector.opacity,0);assert.equal(ext.preTextureOpacity,.5);assert.equal(ext.preTextureStroke,undefined);assert.notEqual(r.vector.stroke,null);assert.equal(r.vector.hasRealStroke,true);
 assert.equal(ext.bitmapBrushSpec.pressure,true);assert.ok(ext.bitmapPressureProfile.length>=2);assert.deepEqual(plain(baked[0].profile),ext.bitmapPressureProfile);
 assert.ok(ext.bitmapPressureProfile.every(([x,y,m])=>Number.isFinite(x)&&Number.isFinite(y)&&m>=.2&&m<=2.5));
 for(const change of [s=>s.symmetryEnabled=true,s=>s.trimStrokeEnds=true,s=>s.maskMode=true]){
  const g=await fixture();bitmapPorts(g);Object.assign(g.root.state,{bitmapBrushOn:true});change(g.root.state);
  const e=send(g,'pointerdown',20,20);send(g,'pointerup',60,50);assert.equal(e.prevented,true);assert.equal(g.calls.length,0);assert.ok(g.log.some(x=>/unavailable/.test(x)),g.log.join(','));
 }
});
test('Shadow mode commits construction ink in the active swatch with its channel and swatch id',async()=>{
 const swatch={id:'sh2',ordinal:2,color:'#ff8800'},ink=[1,0x88/255,0],shadow={channelTag:'shadow',shadowSwatchId:'sh2'};
 const colour=p=>[p.r,p.g,p.b];
 async function draw(state,pen){
  const f=await fixture();f.root.SMShadowBrush={activeSwatch:()=>swatch};f.root.BRUSH_PRESETS={'charcoal-smooth':{}};
  Object.assign(f.root.state,{shadowMode:true},state);const o=pen?{pointerType:'pen',pressure:.6}:{};
  send(f,'pointerdown',20,20,o);f.advance();send(f,'pointermove',50,30,o);f.advance();send(f,'pointerup',80,45,pen?{pointerType:'pen',pressure:0}:{});
  await settled(f);assert.equal(f.calls.length,1,f.log.join(','));
  assert.equal(f.calls[0].payload.command,'object.vector.create','one record: no linked fill, no texture');return plain(f.calls[0].payload.record);
 }
 const line=await draw({brushPreset:'charcoal-smooth'});
 assert.deepEqual([colour(line.vector.stroke),line.vector.hasRealStroke,line.fill,line.vector.strokeWidth,line.vector.strokeCap,line.vector.strokeJoin],
   [ink,true,null,4,'round','round']);
 assert.deepEqual(line.vectorExt,{shadow},'the preset draws no texture');
 const ribbon=await draw({vectorBrush:true},true);
 assert.deepEqual([ribbon.geometry.closed,ribbon.vectorExt,colour(ribbon.vector.stroke),ribbon.vector.strokeWidth,ribbon.fill],[false,{shadow},ink,1,null],
   'the ribbon is its open centreline at the new Path width');
 const fillOnly=await draw({vectorBrush:true,strokeEnabled:false},true);
 assert.deepEqual([fillOnly.geometry.closed,fillOnly.vector.hasRealStroke,colour(fillOnly.fill),fillOnly.vectorExt],[true,false,ink,{shadow}],
   'the Stroke-off pressure fill keeps its fill (preferFill)');
 const none=await fixture();none.root.SMShadowBrush={activeSwatch:()=>null};none.root.state.shadowMode=true;
 send(none,'pointerdown',20,20);send(none,'pointerup',60,50);assert.equal(none.calls.length,0);
 assert.ok(none.log.some(s=>/Shadow Brush swatch is unavailable/.test(s)),none.log.join(','));
});

// #2361 Fill Brush: draw-bridge's isFillBrush branch @1065e9d8. The samples' ribbon outline, filled in the Fill colour,
// goes to the native Fill Brush command with the chosen placement; in Shadow mode it is the centreline in the swatch.
test('Fill Brush commits its outline through the native Fill Brush command with the chosen placement',async()=>{
 for(const mode of ['below','merge','above']){
  const f=await fixture('fillbrush');Object.assign(f.root.state,{fillBrushMode:mode,fillBrushSize:40,strokeEnabled:false,fillEnabled:false,brushPreset:'charcoal-smooth',trimStrokeEnds:true});
  send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:0});
  await settled(f);assert.equal(f.calls.length,1,f.log.join(','));const q=plain(f.calls[0]);
  assert.equal(q.operation,'command.document.object.fill.brush');
  assert.deepEqual([q.payload.placement,q.payload.color,q.payload.opacity],[mode,'#44556680',0.5],'the Fill colour, whatever Stroke/Fill say, at the brush opacity');
  assert.ok(q.payload.outline.length>=4&&q.payload.outline.every((s)=>['point','handleIn','handleOut'].every((k)=>s[k].length===2)),'a closed ribbon outline');
  assert.equal(f.root.state.layers[0].frames.length,0,'no legacy frame was written');
 }
 // Mask mode (tagOwner): the commit names the mask mode for the path the native merge hands back.
 const masked=await fixture('fillbrush');Object.assign(masked.root.state,{fillBrushMode:'above',fillBrushSize:40,maskMode:true,maskModeType:'intersect'});
 send(masked,'pointerdown',20,20,{pointerType:'pen',pressure:.5});masked.advance();send(masked,'pointermove',50,30,{pointerType:'pen',pressure:.8});masked.advance();send(masked,'pointerup',80,45,{pointerType:'pen',pressure:0});
 await settled(masked);assert.equal(plain(masked.calls[0]).payload.mask,'intersect');
 const shadow=await fixture('fillbrush');shadow.root.SMShadowBrush={activeSwatch:()=>({id:'sh3',ordinal:3,color:'#ffdd00'})};Object.assign(shadow.root.state,{fillBrushMode:'below',fillBrushSize:40,shadowMode:true});
 send(shadow,'pointerdown',20,20,{pointerType:'pen',pressure:.5});shadow.advance();send(shadow,'pointermove',50,30,{pointerType:'pen',pressure:.8});shadow.advance();send(shadow,'pointerup',80,45,{pointerType:'pen',pressure:0});
 await settled(shadow);const r=plain(shadow.calls[0].payload.record);
 assert.deepEqual([shadow.calls[0].payload.command,r.geometry.closed,r.vectorExt.shadow.shadowSwatchId,r.fill,r.vector.strokeWidth],['object.vector.create',false,'sh3',null,1],
   'Shadow mode inks the Fill Brush centreline in the swatch');
 const front=await fixture('fillbrush');Object.assign(front.root.state,{fillBrushMode:'front',fillBrushSize:40});
 const e=send(front,'pointerdown',20,20);send(front,'pointerup',60,50);assert.equal(e.prevented,true);assert.equal(front.calls.length,0);
 assert.ok(front.log.some((s)=>/Fill Brush combination is unavailable/.test(s)),front.log.join(','));
 // G23 (symmetry-bridge onStrokeCommitted on the Fill Brush result): the copies travel as affine maps the native owner applies
 // to the path its merge hands back; stop-at-axis would clip that committed path and stays refused.
 const draw=async(state)=>{const f=await fixture('fillbrush');Object.assign(f.root.state,{fillBrushMode:'below',fillBrushSize:40,symmetryEnabled:true},state);
  send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:0});
  await settled(f);return f;};
 const mirror=await draw({symmetryMode:'y',symmetryExtend:true});assert.deepEqual(plain(mirror.calls[0]).payload.copies,[[-1,0,0,1,320,0]],'the x = 160 mirror');
 const radial=await draw({symmetryMode:'radial',symmetryRadialSectors:3,symmetryRadialCenter:{x:160,y:90}});
 const rot=(a)=>{const c=Math.cos(a),s=Math.sin(a);return [c,s,-s,c,160-(c*160-s*90),90-(s*160+c*90)];};
 plain(radial.calls[0]).payload.copies.forEach((m,i)=>m.forEach((v,k)=>assert.ok(Math.abs(v-rot((2-i)*2*Math.PI/3)[k])<1e-9,'copy '+i+' term '+k)));
 const stop=await draw({symmetryMode:'y',symmetryExtend:false});assert.equal(stop.calls.length,0);
 assert.ok(stop.log.some((s)=>/Stop-at-axis symmetry with the Fill Brush/.test(s)),stop.log.join(','));
});

// #2361 G23: symmetry-bridge.js onStrokeCommitted @1065e9d8 over a pressure ribbon. Each copy carries its mapped centreline;
// a ribbon with a linked fill gets a fresh fill of its own, its outline rebuilt from that centreline (adoptFreshIdentity).
test('Fill Brush with Labs on: predictive leaves the fill shape alone, multi-frame stamps it, an unrelated prototype is no refusal',async()=>{
 const stroke=f=>{send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:0});};
 const off=await fixture('fillbrush');Object.assign(off.root.state,{fillBrushMode:'above',fillBrushSize:40});stroke(off);await settled(off);
 const on=await fixture('fillbrush');Object.assign(on.root.state,{fillBrushMode:'above',fillBrushSize:40});predictive(on,true);stroke(on);await settled(on);
 assert.equal(on.calls.length,1,on.log.join(','));assert.deepEqual(plain(on.calls[0].payload.outline),plain(off.calls[0].payload.outline),'predictive-stroke skips isFillShape');
 const grid=await fixture('fillbrush');Object.assign(grid.root.state,{fillBrushMode:'above',fillBrushSize:40});
 grid.root.SMLabs={list:()=>[{name:'canvas-grid',on:true}],unmigratedDrawHooks:()=>[]};stroke(grid);await settled(grid);assert.equal(grid.calls.length,1,grid.log.join(','));
 const many=await fixture('fillbrush');Object.assign(many.root.state,{fillBrushMode:'above',fillBrushSize:40});const toasts=multiframe(many,true);
 stroke(many);assert.equal((await settled(many)).status,'applied',toasts.join(','));assert.equal(many.calls.length,1);
 // multiframeTargets: the other selected frames of the drawn one (3 and 5; 0 is the drawn frame, 99 is out of range).
 assert.deepEqual(plain(many.calls[0].payload.stamps),[3,5]);assert.deepEqual(plain(off.calls[0].payload.stamps),[]);
 assert.deepEqual(toasts,['labsToastMultiframePrefix2labsToastMultiframeSuffix']);
});
test('a trimmed pressure ribbon with symmetry sends its copies in the one trimmed createMany; a radial copy never clips',async()=>{
 const f=await fixture();Object.assign(f.root.state,{trimStrokeEnds:true,trimStrokeEndsMax:24,vectorBrush:true,fillEnabled:false,
  symmetryEnabled:true,symmetryMode:'radial',symmetryExtend:false,symmetryRadialSectors:3,symmetryRadialCenter:{x:160,y:90}});
 send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:.4});
 await settled(f);assert.equal(f.calls.length,1,f.log.join(','));const p=plain(f.calls[0].payload);
 assert.deepEqual([p.command,p.trimEnds,p.records.length],['object.vector.createMany',24,3]);
 assert.ok(p.records.every(r=>r.vectorExt.pressure&&r.vectorExt.pressure.centerSegments.length>1),'the ribbon and its two rotations');
});
test('a trimmed pressure ribbon with multi-frame draw sends one trimmed batch: its cell, then each stamp',async()=>{
 const f=await fixture();multiframe(f,true);Object.assign(f.root.state,{trimStrokeEnds:true,trimStrokeEndsMax:24,vectorBrush:true});
 send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:.4});
 await settled(f);assert.equal(f.calls.length,1);const p=plain(f.calls[0].payload);
 assert.deepEqual([p.command,p.trimEnds],['object.vector.batch',24]);
 const frames=p.ops.map(o=>o.op==='copy'?o.frame:o.record.target.frameScope.frame);assert.deepEqual(frames,[0,0,3,3,5,5]);
 const rec=(o)=>o.op==='copy'?p.ops[o.fromStep].record:o.record;
 assert.ok(p.ops.every((o,i)=>i%2?rec(o).vectorExt.pressure:rec(o).vectorExt.isLinkedFillCompanion),'each cell: the fill, then its ribbon');
 assert.deepEqual(p.ops.slice(2).map(o=>[o.op,o.fromStep]),[['copy',0],['copy',1],['copy',0],['copy',1]],'each stamp copies its drawn step');
});
test('a mirrored or rotated pressure ribbon commits each copy with its own centreline and linked fill',async()=>{
 const pen=(f)=>{send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();
  send(f,'pointerup',80,45,{pointerType:'pen',pressure:0});};
 const f=await fixture();Object.assign(f.root.state,{vectorBrush:true,symmetryEnabled:true,symmetryMode:'y',symmetryExtend:true});
 pen(f);await settled(f);assert.equal(f.calls.length,1,f.log.join(','));const q=plain(f.calls[0].payload);
 assert.equal(q.command,'object.vector.createMany');assert.equal(q.records.length,4,'fill, ribbon, mirrored fill, mirrored ribbon');
 const [fill0,ribbon0,fill1,ribbon1]=q.records;
 assert.deepEqual([fill0.vectorExt.isLinkedFillCompanion,fill1.vectorExt.isLinkedFillCompanion,ribbon0.vectorExt.isLinkedFillCompanion,ribbon1.vectorExt.isLinkedFillCompanion],[true,true,undefined,undefined]);
 assert.equal(fill0.vectorExt.linkedFillId,ribbon0.vectorExt.linkedFillId);assert.equal(fill1.vectorExt.linkedFillId,ribbon1.vectorExt.linkedFillId);
 assert.notEqual(ribbon0.vectorExt.linkedFillId,ribbon1.vectorExt.linkedFillId,'the copy has a fill of its own');
 // The y axis of the 320-wide canvas is x = 160: every centreline point is reflected, widths kept.
 const c0=ribbon0.vectorExt.pressure.centerSegments,c1=ribbon1.vectorExt.pressure.centerSegments;
 assert.equal(c1.length,c0.length);
 c0.forEach((a,i)=>{assert.ok(Math.abs(c1[i].point.x-(320-a.point.x))<1e-6&&Math.abs(c1[i].point.y-a.point.y)<1e-6,'reflected '+i);assert.equal(c1[i].width,a.width);});
 const xs=(r)=>r.geometry.segments.map((s)=>s.point.x);
 assert.ok(Math.min(...xs(ribbon1))>160&&Math.max(...xs(ribbon0))<160,'the copy outline lies across the axis');
 // Radial without a fill: the ribbon then copy n-1 down to copy 1, each rotated about the centre.
 const r=await fixture();Object.assign(r.root.state,{vectorBrush:true,fillEnabled:false,symmetryEnabled:true,symmetryMode:'radial',symmetryRadialSectors:3,symmetryRadialCenter:{x:160,y:90}});
 pen(r);await settled(r);const rq=plain(r.calls[0].payload);assert.equal(rq.records.length,3);
 const at=(rec)=>rec.vectorExt.pressure.centerSegments[0].point,o=at(rq.records[0]);
 [[2,4*Math.PI/3],[1,2*Math.PI/3]].forEach(([k,a],i)=>{const p=at(rq.records[i+1]),dx=o.x-160,dy=o.y-90;
  assert.ok(Math.abs(p.x-(160+dx*Math.cos(a)-dy*Math.sin(a)))<1e-6&&Math.abs(p.y-(90+dx*Math.sin(a)+dy*Math.cos(a)))<1e-6,'copy '+k);});
 // Stop-at-axis clips only a ribbon's outline, never its centreline (live against the baseline in
 // native-object-draw-symmetry-stop-axis.test.cjs). This stroke stays left of x = 160, so it commits as with Extend.
 const stop=await fixture();Object.assign(stop.root.state,{vectorBrush:true,symmetryEnabled:true,symmetryMode:'y',symmetryExtend:false});
 pen(stop);await settled(stop);assert.equal(stop.calls.length,1,stop.log.join(','));const sq=plain(stop.calls[0].payload);
 assert.equal(sq.records.length,4);assert.deepEqual(sq.records[1].geometry,ribbon0.geometry);
 assert.deepEqual(sq.records[1].vectorExt.pressure.centerSegments,c0);
});

// #2361 draw-cancel-matrix (freehand). Baseline draw-bridge.js @1065e9d8 keeps one module-level `dragging`/`samples` and onUp/commitStroke
// consult neither the tool, the frame, the layer's lock/visibility nor the history. Cells the baseline decides on purpose are matched;
// the rest are baseline accidents (a stroke written into a locked layer, into whichever frame is current at release, or after a
// history step) that the native owner cannot reproduce: it writes only the pinned layer/frame/revision it started on, so those cancel.
async function released(f,next){const control=f;send(control,'pointerdown',20,20);send(control,'pointermove',45,30);if(next)next(control);return send(control,'pointerup',80,65);}
test('cancel matrix: a tool switch mid-stroke still commits the one stroke on release, as draw-bridge keeps dragging (#2361)',async()=>{
 const ref=await fixture();await released(ref);assert.equal((await settled(ref)).status,'applied');
 for(const next of ['select','hand','eraser']){
  const f=await fixture();await released(f,c=>{c.root.state.tool=next;});
  assert.equal((await settled(f)).status,'applied',next);assert.equal(f.calls.length,1,next);assert.equal(f.session.info().contentRevision,1,'exactly one revision');
  assert.deepEqual(f.calls[0].payload.record.geometry,ref.calls[0].payload.record.geometry,'the stroke is the one drawn, not a refit');
  assert.equal(f.body.children.length,0,'the draft is cleared');assert.equal(f.area.captured.size,0,'capture released');assert.equal(f.root.state.tool,next);
 }
});
test('cancel matrix: a layer locked or hidden mid-stroke cancels with zero revisions; the baseline writes into the locked layer, the native owner refuses (#2361)',async()=>{
 for(const [key,value] of [['locked',true],['visible',false]]){
  const f=await fixture();send(f,'pointerdown',20,20);send(f,'pointermove',45,30);f.flags[key]=value;const wait=gate();wait.resolve();await f.external(wait);
  const toasts=f.log.length;send(f,'pointerup',80,65);await settled(f);
  assert.equal(f.calls.length,0,key);assert.equal(f.undos.length,0,key);assert.equal(f.body.children.length,0,key);assert.equal(f.session.info().contentRevision,1,'only the external change');
  send(f,'pointerdown',20,20);assert.equal(f.body.children.length,0,'a press on the unavailable layer starts nothing');assert.ok(f.log.length>toasts,'and says so');assert.equal(f.calls.length,0);
 }
});
test('cancel matrix: undo mid-stroke drops the stroke, undo while the commit is pending runs after it (#2361)',async()=>{
 // Mid-stroke the revision moves under the pin, so the stroke cancels (the baseline would still commit it after the step).
 let f=await fixture();send(f,'pointerdown',20,20);send(f,'pointermove',45,30);await f.session.historyFromUi('undo');send(f,'pointerup',80,65);await settled(f);
 assert.equal(f.calls.length,0);assert.equal(f.undos.length,1);assert.equal(f.body.children.length,0);
 // A commit already pending is one revision on the existing queue; the undo queues behind it and removes exactly that stroke.
 f=await fixture();const wait=gate();f.hold(wait);send(f,'pointerdown',20,20);send(f,'pointermove',45,30);send(f,'pointerup',80,65);await turn();
 assert.equal(f.calls.length,1);const undone=f.session.historyFromUi('undo');await turn();assert.equal(f.undos.length,0,'not before the stroke settles');
 wait.resolve();assert.equal((await settled(f)).status,'applied');await undone;
 assert.equal(f.calls.length,1,'one stroke');assert.deepEqual(f.undos,[1],'undone on top of its revision');assert.equal(f.session.info().contentRevision,2);
});
test('cancel matrix: a second pointer during a stroke is ignored; the first pointer still commits exactly one stroke (#2361)',async()=>{
 const f=await fixture();send(f,'pointerdown',20,20);send(f,'pointermove',45,30);
 const second={pointerId:2,isPrimary:false};
 for(const kind of ['pointerdown','pointermove','pointerup','pointercancel','lostpointercapture'])assert.equal(send(f,kind,100,100,second).stopped,true,kind+' of the other pointer is consumed');
 assert.equal(f.calls.length,0);assert.equal(f.body.children.length,1,'the stroke is still being drawn');
 send(f,'pointerup',80,65);assert.equal((await settled(f)).status,'applied');assert.equal(f.calls.length,1);
 assert.deepEqual(f.calls[0].payload.record.geometry.segments.map(s=>[s.point.x,s.point.y]),[[20,20],[45,30],[80,65]],'none of the other pointer samples joined');
});

// #2361 draw-texture-symmetry-trim: a built-in preset with symmetry or Trim commits natively. The baseline copies the anchor
// (symmetry-bridge.js adoptFreshIdentity drops its brush group) and trims it before any dab is stamped; native stamps each anchor's
// dabs from its own group and seed (seededRng, never Math.random), so every copy is an anchor of its own in the ONE createMany.
const SEED_ID=/^bg[0-9a-z]+_[0-9]+(_c[0-9]+)?$/;
async function textured(over,pen){
 const f=await fixture();f.root.BRUSH_PRESETS={'charcoal-smooth':{},'chalk-round':{}};Object.assign(f.root.state,{brushPreset:'charcoal-smooth'},over);
 if(pen){send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:.4});}
 else{send(f,'pointerdown',20,20);send(f,'pointermove',50,30);send(f,'pointerup',100,80);}
 await settled(f);return f;
}
test('a textured plain stroke with mirror symmetry sends the anchor and its mirror in one createMany, each seeded and grouped',async()=>{
 for(const fillEnabled of [true,false]){
  const f=await textured({fillEnabled,symmetryEnabled:true,symmetryMode:'y',symmetryExtend:true});
  assert.equal(f.calls.length,1,f.log.join(','));const q=plain(f.calls[0].payload);
  assert.deepEqual([q.command,q.records.length],['object.vector.createMany',2],'one revision and one undo for the whole mirrored stroke');
  const [a,b]=q.records.map(r=>r.vectorExt.brushTexture);
  for(const t of [a,b]){assert.equal(t.brushTexturePreset,'charcoal-smooth');assert.match(t.brushGroupId,SEED_ID);assert.ok(Number.isInteger(t.strokeSeed)&&t.strokeSeed>=0&&t.strokeSeed<=0xFFFFFFFF);
   if(fillEnabled)assert.equal(t.preTextureStroke,'#112233ff');else assert.equal(t.preTextureOpacity,0.5);}
  assert.notEqual(a.brushGroupId,b.brushGroupId,'the copy is a group of its own');assert.notEqual(a.strokeSeed,b.strokeSeed,'and has its own seed');
  assert.ok(q.records.every(r=>r.vector.opacity===(fillEnabled?0.5:0)),'every anchor is camouflaged as the drawn one');
  const mx=q.records[1].geometry.segments.map(s=>s.point.x);assert.ok(Math.min(...mx)>160&&Math.max(...q.records[0].geometry.segments.map(s=>s.point.x))<160);
 }
});
test('textured symmetry copies derive their seed from the drawn stroke deterministically and radial copies each get their own group',async()=>{
 const f=await textured({fillEnabled:false,symmetryEnabled:true,symmetryMode:'radial',symmetryExtend:false,symmetryRadialSectors:4,symmetryRadialCenter:{x:160,y:90}});
 assert.equal(f.calls.length,1,f.log.join(','));const q=plain(f.calls[0].payload);assert.equal(q.records.length,4);
 const t=q.records.map(r=>r.vectorExt.brushTexture);
 assert.equal(new Set(t.map(x=>x.brushGroupId)).size,4,'four groups');assert.equal(new Set(t.map(x=>x.strokeSeed)).size,4,'four seeds');
 // The same drawn stroke gives the same copies' seeds again (no Math.random in the derivation).
 const Symmetry=require('../src/js/adapters/native-object-draw-symmetry.js'),root={state:{symmetryEnabled:true,symmetryMode:'radial',symmetryRadialSectors:4,symmetryRadialCenter:{x:160,y:90}}};
 const dict={segments:[{point:[20,20],handleIn:[0,0],handleOut:[0,0]},{point:[50,30],handleIn:[0,0],handleOut:[0,0]}],closed:false,brushTexturePreset:'p',brushGroupId:'bg1_2',strokeSeed:12345,opacity:0};
 const run=()=>Symmetry.copies(root,dict,{canvasW:320,canvasH:180},null).map(d=>[d.brushGroupId,d.strokeSeed]);
 assert.deepEqual(run(),run());assert.deepEqual(run()[0],['bg1_2',12345],'the drawn stroke keeps its own identity, first in the stack');
 assert.deepEqual(run().slice(1).map(x=>x[0]),['bg1_2_c3','bg1_2_c2','bg1_2_c1'],'copy n-1 down to copy 1 above it');
 assert.equal(dict.brushGroupId,'bg1_2');assert.equal(dict.strokeSeed,12345);
});
test('a textured pressure ribbon with symmetry mirrors the hidden ribbon anchor with its own seed and keeps each linked fill plain',async()=>{
 const f=await textured({vectorBrush:true,fillEnabled:true,symmetryEnabled:true,symmetryMode:'y',symmetryExtend:true},true);
 assert.equal(f.calls.length,1,f.log.join(','));const q=plain(f.calls[0].payload);
 assert.deepEqual([q.command,q.records.length],['object.vector.createMany',4]);
 const [fill0,ribbon0,fill1,ribbon1]=q.records;
 assert.equal(fill0.vectorExt.brushTexture,undefined);assert.equal(fill1.vectorExt.brushTexture,undefined);
 const [a,b]=[ribbon0,ribbon1].map(r=>r.vectorExt.brushTexture);
 assert.deepEqual([a.brushTexturePreset,b.brushTexturePreset,a.preTextureOpacity,b.preTextureOpacity,ribbon0.vector.opacity,ribbon1.vector.opacity],['charcoal-smooth','charcoal-smooth',0.5,0.5,0,0]);
 assert.notEqual(a.brushGroupId,b.brushGroupId);assert.notEqual(a.strokeSeed,b.strokeSeed);
});
test('Trim Stroke Ends on a textured stroke asks native to trim the anchor, which then stamps its dabs, with and without symmetry',async()=>{
 const one=await textured({trimStrokeEnds:true,trimStrokeEndsMax:24});
 assert.equal(one.calls.length,1,one.log.join(','));const q=plain(one.calls[0].payload);
 assert.deepEqual([q.command,q.trimEnds],['object.vector.create',24]);assert.equal(q.record.vectorExt.brushTexture.brushTexturePreset,'charcoal-smooth');
 const sym=await textured({trimStrokeEnds:true,trimStrokeEndsMax:24,symmetryEnabled:true,symmetryMode:'y',symmetryExtend:true});
 const m=plain(sym.calls[0].payload);assert.deepEqual([m.command,m.trimEnds,m.records.length],['object.vector.createMany',24,2]);
 assert.ok(m.records.every(r=>r.vectorExt.brushTexture.brushTexturePreset==='charcoal-smooth'));
 const ribbon=await textured({vectorBrush:true,trimStrokeEnds:true,trimStrokeEndsMax:24},true);
 assert.equal(ribbon.calls[0].payload.trimEnds,24);
 const stop=await fixture();stop.root.BRUSH_PRESETS={'charcoal-smooth':{}};Object.assign(stop.root.state,{brushPreset:'charcoal-smooth',trimStrokeEnds:true,trimStrokeEndsMax:24,symmetryEnabled:true,symmetryMode:'y',symmetryExtend:false});
 send(stop,'pointerdown',20,20);send(stop,'pointerup',60,50);await settled(stop);assert.equal(stop.calls.length,1,'stop-at-axis Trim now commits: native trims the whole stroke, then clips');
 assert.deepEqual(plain(stop.calls[0].payload.trimClip),{axis:[160,0,160,180],mirrors:[[0,1]]});
});
// #2361: the original trims the WHOLE stroke (draw-bridge.js) and only then clips it at the axis (symmetry-bridge.js clipPathAtAxis @1065e9d8).
test('Trim Stroke Ends with a stop-at-axis mirror sends the whole stroke and its copy, and native clips them after its trim',async()=>{
 const f=await fixture();Object.assign(f.root.state,{trimStrokeEnds:true,trimStrokeEndsMax:24,symmetryEnabled:true,symmetryMode:'y',symmetryExtend:false});
 send(f,'pointerdown',20,20);send(f,'pointermove',100,40);send(f,'pointermove',150,60);send(f,'pointerup',230,80);
 await settled(f);assert.equal(f.calls.length,1,f.log.join(','));const p=plain(f.calls[0].payload);
 assert.deepEqual([p.command,p.trimEnds,p.records.length,p.trimClip],['object.vector.createMany',24,2,{axis:[160,0,160,180],mirrors:[[0,1]]}]);
 const xs=r=>r.geometry.segments.map(s=>s.point.x);
 // Unclipped: the drawn stroke still runs past the axis, and its copy is the reflection of that whole stroke.
 assert.ok(Math.max(...xs(p.records[0]))>160,'the drawn stroke is not clipped in JS');assert.ok(Math.min(...xs(p.records[1]))<160);
 assert.deepEqual(xs(p.records[1]).map(x=>Math.round(320-x)),xs(p.records[0]).map(Math.round));
 for(const [mode,axis] of [['x',[0,90,320,90]],['free',[160-72,90-72,160+72,90+72]]]){
  const g=await fixture();Object.assign(g.root.state,{trimStrokeEnds:true,trimStrokeEndsMax:24,symmetryEnabled:true,symmetryMode:mode,symmetryExtend:false});
  send(g,'pointerdown',20,20);send(g,'pointermove',100,40);send(g,'pointerup',230,80);await settled(g);
  assert.deepEqual(plain(g.calls[0].payload.trimClip),{axis,mirrors:[[0,1]]},mode);
 }
 // Without Trim, Extend or a mirror the clip stays in JS: no trimClip, and the stroke leaves clipped at the axis.
 for(const state of [{trimStrokeEnds:false},{symmetryExtend:true},{symmetryMode:'radial',symmetryRadialSectors:3,symmetryRadialCenter:{x:160,y:90}}]){
  const g=await fixture();Object.assign(g.root.state,{trimStrokeEnds:true,trimStrokeEndsMax:24,symmetryEnabled:true,symmetryMode:'y',symmetryExtend:false},state);
  send(g,'pointerdown',20,20);send(g,'pointermove',100,40);send(g,'pointerup',230,80);await settled(g);
  const q=plain(g.calls[0].payload);assert.equal(q.trimClip,undefined,JSON.stringify(state));
  if(!state.symmetryExtend&&state.trimStrokeEnds===false)assert.ok(Math.max(...q.records[0].geometry.segments.map(x=>x.point.x))<=160,'clipped in JS');
 }
});
test('a stop-at-axis mirror with Trim and a Labs multi-frame stamp clips only the drawn pair; the stamp copies the whole drawn stroke',async()=>{
 const f=await fixture();Object.assign(f.root.state,{trimStrokeEnds:true,trimStrokeEndsMax:24,symmetryEnabled:true,symmetryMode:'y',symmetryExtend:false});
 f.root.SMLabs={isOn:()=>false,register(){},unmigratedDrawHooks:()=>[],multiframeTargets:()=>[3],multiframeStamped(){}};
 send(f,'pointerdown',20,20);send(f,'pointermove',100,40);send(f,'pointerup',230,80);await settled(f);
 assert.equal(f.calls.length,1,f.log.join(','));const p=plain(f.calls[0].payload);
 assert.deepEqual([p.command,p.trimEnds,p.trimClip],['object.vector.batch',24,{axis:[160,0,160,180],mirrors:[[0,1]]}]);
 assert.deepEqual(p.ops.map(o=>[o.op,o.fromStep,o.frame]),[['create',undefined,undefined],['create',undefined,undefined],['copy',0,3]]);
});
test('a texture preset still refuses a key neither the built-ins nor the catalog hold, with symmetry too',async()=>{
 const f=await fixture();Object.assign(f.root.state,{brushPreset:'not-a-preset',symmetryEnabled:true,symmetryMode:'y',symmetryExtend:true});
 send(f,'pointerdown',20,20);send(f,'pointerup',60,50);assert.equal(f.calls.length,0);
});

// #2361: Alt+drag brush resize (draw-bridge.js `sizing` @1065e9d8): 0.5px of size per px of drag, clamped to 1..80, a live
// circle at the press point, the Size field mirrored, a toast at release, and no stroke, command or revision.
function sizeField(f){const field={value:'4'},was=f.doc.getElementById;f.doc.getElementById=id=>id==='p-sw'?field:was(id);return field;}
test('Alt+drag resizes the brush with the baseline math and a live circle, committing no stroke and no revision',async()=>{
 for(const tool of ['draw','fillbrush']){
  const f=await fixture(tool),field=sizeField(f),revision=f.info().contentRevision;
  const e=send(f,'pointerdown',60,40,{altKey:true});assert.equal(e.stopped,true);assert.equal(e.prevented,true);
  assert.equal(f.body.children.length,1,'the circle');const circle=f.body.children[0].children[0];
  assert.deepEqual([circle.attributes.cx,circle.attributes.cy].map(Number),[120,80]);assert.equal(Number(circle.attributes.r),4/2*2,'size/2 document units, 2px each');
  const baseline=fs.readFileSync(path.join(ROOT,'src/js/draw-bridge.js'),'utf8');
  assert.match(baseline,/Math\.max\(1, Math\.min\(80, sizeStartVal \+ \(e\.clientX - sizeStartX\) \* 0\.5\)\)/,'the legacy gesture this one mirrors');
  const drag=dx=>send(f,'pointermove',(120+dx)/2,40);
  for(const dx of [40,-8,1000,-1000,-6]){
   drag(dx);const want=Math.max(1,Math.min(80,4+dx*0.5));
   assert.equal(f.root.state.brushSize,want,tool+' dx '+dx);assert.equal(field.value,Math.round(want));assert.equal(Number(circle.attributes.r),want/2*2);
  }
  drag(40);send(f,'pointerup',100,40);
  assert.equal(f.body.children.length,0,'the circle goes at release');assert.equal(f.log.at(-1),'Taille du pinceau : 24px');
  assert.equal(f.root.state.brushSize,24);assert.equal(f.calls.length,0);assert.equal(f.records.size,0);assert.equal(f.info().contentRevision,revision);
  assert.equal(f.paper.project.activeLayer.children.length,0);assert.equal(f.undos.length,0);
  // The next ordinary stroke is drawn with the resized brush.
  send(f,'pointerdown',20,20);send(f,'pointermove',45,30);send(f,'pointerup',100,80);
  if(tool==='draw'){assert.equal((await settled(f)).status,'applied');assert.equal(f.calls[0].payload.record.vector.strokeWidth,24);}
 }
});
test('Alt+drag only resizes: release without a move keeps the size, a cancel keeps the last size, and an unready canvas says so',async()=>{
 const f=await fixture(),field=sizeField(f);
 send(f,'pointerdown',60,40,{altKey:true});send(f,'pointerup',60,40,{altKey:true});
 assert.equal(f.root.state.brushSize,4);assert.equal(f.log.at(-1),'Taille du pinceau : 4px');assert.equal(f.body.children.length,0);
 send(f,'pointerdown',60,40,{altKey:true});send(f,'pointermove',80,40);send(f,'pointercancel',80,40);
 assert.equal(f.root.state.brushSize,24);assert.equal(f.body.children.length,0);assert.equal(f.calls.length,0);assert.equal(field.value,24);
 f.root.state.playing=true;f.root.state.brushSize=4;
 const e=send(f,'pointerdown',60,40,{altKey:true});assert.equal(e.prevented,true);assert.equal(f.body.children.length,0);
 send(f,'pointermove',80,40);assert.equal(f.root.state.brushSize,4);assert.equal(f.log.at(-1),'Native brush resize is unavailable.');assert.equal(f.calls.length,0);
});
// #2361: every Labs draw hook without a native route is a typed refusal that names it (Labs.unmigratedDrawHooks).
test('an enabled Labs draw hook with no native route refuses with a typed reason that names it, and nothing is drawn',async()=>{
 const Paint=require('../src/js/adapters/native-object-draw-paint.js');
 const f=await fixture(),protos={'bulb-hook':{onPreview(){}},'ink-hook':{onStroke(){}},'migrated':{onStroke(){},nativeDraw:true},'view-aid':{}};
 f.root.SMLabs={isOn:()=>true,list:()=>Object.keys(protos).map(name=>({name,on:true})),
  unmigratedDrawHooks:()=>Object.keys(protos).filter(n=>(protos[n].onStroke||protos[n].onPreview)&&!protos[n].nativeDraw)};
 const refusal=Paint.labsRefusal(f.root);assert.equal(refusal.code,'native-draw-labs-hook-unmigrated');assert.deepEqual(refusal.hooks,['bulb-hook','ink-hook']);
 assert.match(refusal.message,/bulb-hook, ink-hook/);
 send(f,'pointerdown',20,20);send(f,'pointermove',45,30);send(f,'pointerup',100,80);
 assert.equal(f.calls.length,0);assert.ok(f.log.some(m=>/Labs draw hooks bulb-hook, ink-hook/.test(m)),f.log.join('|'));
 const fb=await fixture('fillbrush');Object.assign(fb.root.state,{fillBrushMode:'above',fillBrushSize:40});
 fb.root.SMLabs={list:()=>[{name:'solo',on:true}],unmigratedDrawHooks:()=>['solo']};
 send(fb,'pointerdown',20,20);send(fb,'pointerup',60,50);assert.equal(fb.calls.length,0);assert.ok(fb.log.some(m=>/Labs draw hook solo yet/.test(m)),fb.log.join('|'));
 f.root.SMLabs.unmigratedDrawHooks=()=>[];send(f,'pointerdown',20,20);send(f,'pointermove',45,30);send(f,'pointerup',100,80);
 assert.equal((await settled(f)).status,'applied','once the hook is off (or migrated) native Drawing is available');
 assert.equal(Paint.labsRefusal({}),null);
 assert.equal(Paint.labsRefusal({SMLabs:{list:()=>[{name:'x',on:false}]}}),null);
 assert.deepEqual(Paint.labsRefusal({SMLabs:{list:()=>[{name:'x',on:true},{name:'y',on:false}]}}).hooks,['x'],'without the registry, any enabled prototype blocks, by name');
});
test('the bitmap brush names the enabled Labs draw hooks in its refusal, under its own code', () => {
 const Bitmap=require('../src/js/adapters/native-object-draw-bitmap.js'),state={tool:'draw',bitmapBrushOn:true,strokeEnabled:true,vectorBrush:false};
 // predictive-stroke is migrated for native Draw, but the bitmap command runs no Labs hook: refused, not mislabelled as unmigrated.
 const root={SMBitmapBrush:{bake(){}},SMLabs:{list:()=>[{name:'predictive-stroke',on:true},{name:'canvas-grid',on:true}],
  enabledDrawHooks:()=>['predictive-stroke'],unmigratedDrawHooks:()=>[]}};
 assert.throws(()=>Bitmap.style(root,state,{}),e=>e.code==='native-bitmap-brush-labs-unsupported'&&e.hooks.join()==='predictive-stroke'&&/predictive-stroke is on/.test(e.message));
 // A view-aid prototype with no draw hook does not block the bitmap brush.
 const aid={SMBitmapBrush:{bake(){}},SMLabs:{list:()=>[{name:'canvas-grid',on:true}],enabledDrawHooks:()=>[]}};
 assert.equal(Bitmap.style(aid,state,{}).bitmap.tip,'soft');
});
test('the Labs registry lists enabled draw hooks, migrated or not, and skips view aids', () => {
 const Model=require('../src/js/adapters/labs-registry-model.js');
 const L=Model.create({getItem:()=>'1',setItem(){},warn(){},toast(){},describe:k=>k});
 L.register('hooked-native',{flag:'a',onStroke(){},nativeDraw:true});L.register('hooked',{flag:'b',onPreview(){}});L.register('aid',{flag:'c'});
 assert.deepEqual(L.enabledDrawHooks().sort(),['hooked','hooked-native']);
 assert.deepEqual(L.unmigratedDrawHooks(),['hooked']);
});
test('every production Labs prototype with a draw hook declares its native route, so none is left unnamed',()=>{
 const dir=path.join(ROOT,'src/js/labs'),hooked=[];
 for(const name of fs.readdirSync(dir).filter(n=>n.endsWith('.js'))){
  const text=fs.readFileSync(path.join(dir,name),'utf8');
  if(/^\s*(onStroke|onPreview)\s*[:(]/m.test(text)||/\bonStroke:\s|\bonPreview:\s/.test(text))hooked.push([name,/nativeDraw:\s*true/.test(text)]);
 }
 assert.deepEqual(hooked.sort(),[['multiframe-draw.js',true],['predictive-stroke.js',true]],'a new draw hook needs a native route or the typed refusal above names it');
});
