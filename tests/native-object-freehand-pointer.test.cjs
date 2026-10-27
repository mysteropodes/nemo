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
async function fixture(tool='draw',native=true){
 let clock=1000;let installed=false,generation=0,revision=0,hold=null,refusal=null,replyChange=null,wrongOwner=false,ack=null,binding=null;
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
  brushSize:4,opacity:50,strokeCap:'round',strokeJoin:'round',strokeStyle:'solid',miterLimit:10,dashOffset:0,paintOrder:'fillFirst',
  vectorBrush:false,smoothing:0,stabilizer:0,taperEnds:false,drawMode:'front',brushPreset:'none',pressureMin:5,pressureMax:170,pressureInvert:false,pressureCurve:'linear',brushAngle:0,brushAngleFactor:0},userLayers:[],selectedPaths:[],devicePixelRatio:2,
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
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/adapters/native-object-shape-tool.js'),'utf8'),context,{filename:'native-object-shape-tool.js'});
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/draw-bridge.js'),'utf8'),context,{filename:'draw-bridge.js'});
 vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/shape-bridge.js'),'utf8'),context,{filename:'shape-bridge.js'});
 return {perspectiveMode:value=>perspectiveMode=value,root,context,paper,advance:(ms=16)=>clock+=ms,session,area,canvas,zoom,fit,ruler,doc,body,calls,records,log,events,info,setRect:r=>rect=r,refuse:code=>refusal=code,hold:g=>hold=g,mutate:fn=>replyChange=fn,
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
test('both existing moving-average and Ready modeler input paths retain the actual release endpoint and one transaction',async()=>{
 for(const stabilizer of [2,4]){const f=await fixture();Object.assign(f.root.state,{stabilizer,smoothing:2,fillEnabled:false});
  send(f,'pointerdown',20,20);f.advance();send(f,'pointermove',40,40);f.advance();send(f,'pointermove',60,30);f.advance();send(f,'pointerup',80,50);
  assert.equal((await settled(f)).status,'applied');assert.equal(f.calls.length,1);const last=f.calls[0].payload.record.geometry.segments.at(-1).point;
  if(stabilizer===2)assert.deepEqual(last,{x:80,y:50});else assert.ok(Math.hypot(last.x-80,last.y-50)<1,'same reference modeler EOS catchup');
  assert.equal(f.paper.project.activeLayer.children.length,0);
 }
});
test('256 outline anchors and 65536 full envelope are hard refusals with no trimming, split commit or local writer',async()=>{
 const anchors=await fixture();send(anchors,'pointerdown',20,20);
 const many=Array.from({length:256},(_,i)=>({clientX:40+(21+i)*2,clientY:100+i%2}));send(anchors,'pointermove',276,25,{getCoalescedEvents:()=>many});send(anchors,'pointerup',278,25);
 assert.equal((await settled(anchors)).status,'unmapped');assert.equal(anchors.calls.length,0);assert.equal(anchors.records.size,0);
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
 const cases=[s=>Object.assign(s,{tool:'fillbrush',fillBrushMode:'front',fillBrushSize:40}),s=>s.bitmapBrushOn=true,s=>s.brushPreset='my-custom-brush',s=>Object.assign(s,{brushPreset:'charcoal-smooth',symmetryEnabled:true}),
  // Trim Stroke Ends commits natively for a plain stroke and a lone pressure ribbon; a trimmed ribbon's symmetry copies stay refused.
  s=>Object.assign(s,{trimStrokeEnds:true,vectorBrush:true,fillEnabled:false,symmetryEnabled:true,symmetryExtend:true}),
  // With stop-at-axis symmetry the original trims the whole stroke before clipping; native would trim the clipped one.
  s=>Object.assign(s,{trimStrokeEnds:true,symmetryEnabled:true,symmetryExtend:false}),
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

test('exactly256 ordinary polyline anchors are admitted as one full record without a size-driven fit',async()=>{
 const f=await fixture();send(f,'pointerdown',20,20);const points=Array.from({length:254},(_,i)=>({clientX:82+i*2,clientY:90+i%3}));
 send(f,'pointermove',274,21,{getCoalescedEvents:()=>points});send(f,'pointerup',276,21);assert.equal((await settled(f)).status,'applied');
 assert.equal(f.calls.length,1);assert.equal(f.calls[0].payload.record.geometry.segments.length,256);assert.ok(Buffer.byteLength(JSON.stringify(f.calls[0]))<=65536);
});

test('pressure taps refuse before dispatch, finalize pending/current UI, and allow the next valid gesture',async()=>{
 for(const smoothing of [2,10,100])for(const stabilizer of [0,4]){
  const f=await fixture();Object.assign(f.root.state,{vectorBrush:true,fillEnabled:false,smoothing,stabilizer});
  send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});send(f,'pointerup',20,20,{pointerType:'pen',pressure:0});
  const tap=await settled(f);assert.equal(tap.status,'unavailable',JSON.stringify({smoothing,stabilizer,tap}));assert.equal(tap.reason,'pre_dispatch_failed');
  assert.equal(f.calls.length,0);assert.equal(f.records.size,0);assert.equal(f.body.children.length,0);assert.equal(f.area.captured.size,0);
  assert.equal(f.root.NemoNativeFreehandToolController.isDragging(),false);assert.ok(f.session.readPresentedObjectSession(),'no queue pending remains');
  f.advance();send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.3});f.advance();send(f,'pointermove',60,40,{pointerType:'pen',pressure:.7});f.advance();send(f,'pointerup',100,60,{pointerType:'pen',pressure:0});
  assert.equal((await settled(f)).status,'applied',JSON.stringify({smoothing,stabilizer}));assert.equal(f.calls.length,1);assert.equal(f.records.size,1);assert.equal(f.session.info().contentRevision,1);
  assert.ok(f.session.readPresentedObjectSession());assert.equal(f.paper.project.activeLayer.children.length,0);
 }
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
 assert.deepEqual(payload.ops.map(op=>[op.op,op.record.target.frameScope.frame,op.keyframe||null]),[['create',0,null],['create',3,'copy'],['create',5,'copy']]);
 assert.deepEqual(payload.ops[1].record.geometry,payload.ops[0].record.geometry,'every stamp is the drawn stroke');
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
 const masked=await fixture();Object.assign(masked.root.state,{vectorBrush:true,maskMode:true});
 send(masked,'pointerdown',20,20);send(masked,'pointermove',50,30);send(masked,'pointerup',80,45);await settled(masked);
 assert.equal(masked.calls.length,0);assert.ok(masked.log.some(s=>/Mask mode with a pressure fill/.test(s)),masked.log.join(','));
});
test('a pressure stroke with Stroke off is one closed fill of its centreline',async()=>{
 const f=await fixture();Object.assign(f.root.state,{vectorBrush:true,strokeEnabled:false});
 send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:0});
 await settled(f);assert.equal(f.calls.length,1);const r=plain(f.calls[0].payload.record);
 assert.equal(f.calls[0].payload.command,'object.vector.create');
 assert.deepEqual([r.geometry.closed,r.vector.hasRealStroke,r.fill.r,r.vectorExt&&r.vectorExt.pressure],[true,false,0x44/255,undefined]);
 assert.deepEqual(r.geometry.segments.map(s=>[s.point.x,s.point.y]),[[20,20],[50,30],[80,45]]);
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
test('a trimmed pressure ribbon with multi-frame draw sends one trimmed batch: its cell, then each stamp',async()=>{
 const f=await fixture();multiframe(f,true);Object.assign(f.root.state,{trimStrokeEnds:true,trimStrokeEndsMax:24,vectorBrush:true});
 send(f,'pointerdown',20,20,{pointerType:'pen',pressure:.5});f.advance();send(f,'pointermove',50,30,{pointerType:'pen',pressure:.8});f.advance();send(f,'pointerup',80,45,{pointerType:'pen',pressure:.4});
 await settled(f);assert.equal(f.calls.length,1);const p=plain(f.calls[0].payload);
 assert.deepEqual([p.command,p.trimEnds],['object.vector.batch',24]);
 const frames=p.ops.map(o=>o.record.target.frameScope.frame);assert.deepEqual(frames,[0,0,3,3,5,5]);
 assert.ok(p.ops.every((o,i)=>i%2?o.record.vectorExt.pressure:o.record.vectorExt.isLinkedFillCompanion),'each cell: the fill, then its ribbon');
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
 // Stop-at-axis clips only a ribbon's outline in the baseline, never its centreline: refused rather than inconsistent.
 const stop=await fixture();Object.assign(stop.root.state,{vectorBrush:true,symmetryEnabled:true,symmetryMode:'y',symmetryExtend:false});
 pen(stop);await settled(stop);assert.equal(stop.calls.length,0);assert.ok(stop.log.some((s)=>/Stop-at-axis symmetry with pressure brushes/.test(s)),stop.log.join(','));
});
