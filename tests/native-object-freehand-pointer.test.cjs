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
 const cases=[s=>s.vectorBrush=true,s=>s.tool='fillbrush',s=>s.drawMode='behind',s=>s.bitmapBrushOn=true,s=>s.brushPreset='charcoal-smooth',s=>s.trimStrokeEnds=true,
  s=>s.symmetryEnabled=true,s=>s.shadowMode=true,s=>Object.assign(s,{vectorBrush:true,fillEnabled:false,pressureCurve:'unknown'}),s=>s.stabilizer=7];
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
