'use strict';
// Independent DOM oracle for native presentation ownership and geometry.
const test=require('node:test'),assert=require('node:assert/strict');
const {create,desktop}=require('../src/js/adapters/native-viewport-underlay.js');
const CLASS='nemo-native-underlay',KEYS=['left','top','right','bottom'].map(x=>'--nemo-native-'+x);
const A=Object.freeze({instanceId:'instance-A',documentId:'document-A',lifecycleGeneration:1});
const B=Object.freeze({instanceId:'instance-A',documentId:'document-B',lifecycleGeneration:2});
function fixture(initial={}){
 const classes=new Set(initial.classes||[]),styles=new Map(Object.entries(initial.styles||{})),priorities=new Map(Object.entries(initial.priorities||{}));
 const classList={add(...xs){xs.forEach(x=>classes.add(x));},remove(...xs){xs.forEach(x=>classes.delete(x));},contains:x=>classes.has(x),toggle(x,on){if(on===undefined)on=!classes.has(x);on?classes.add(x):classes.delete(x);return on;}};
 const style={setProperty(k,v,p=''){styles.set(k,String(v));priorities.set(k,p);},getPropertyValue:k=>styles.get(k)||'',removeProperty(k){const old=styles.get(k)||'';styles.delete(k);priorities.delete(k);return old;},getPropertyPriority:k=>priorities.get(k)||''};
 const document={documentElement:{classList,style,clientWidth:1280,clientHeight:900}};
 let rect={left:40,top:60,right:1000,bottom:700,width:960,height:640};
 const api=create({document,measure:()=>rect,...(initial.setBackdropHidden?{setBackdropHidden:initial.setBackdropHidden}:{})});
 return {api,document,classes,styles,priorities,set rect(v){rect=v;},get rect(){return rect;}};
}
function opaque(f){assert.equal(f.classes.has(CLASS),false);for(const k of KEYS)assert.equal(f.styles.has(k),false);}
function edges(f,values){assert.equal(f.classes.has(CLASS),true);assert.deepEqual(KEYS.map(k=>f.styles.get(k)),values.map(v=>v+'px'));}
test('construction leaves ordinary document opaque and does not invent a presentation',()=>{const f=fixture();opaque(f);f.api.refresh(A);opaque(f);});
test('valid correlated owner presentation enables only the measured canvas hole',()=>{const f=fixture();f.api.present(A);edges(f,[40,60,1000,700]);});
test('same-owner resize refresh uses fresh measured edges',()=>{const f=fixture();f.api.present(A);f.rect={left:72.5,top:103.25,right:1150.75,bottom:801,width:1078.25,height:697.75};f.api.refresh({...A});edges(f,[72.5,103.25,1150.75,801]);});
test('stale owner cannot refresh or clear a newer document',()=>{const f=fixture();f.api.present(A);f.rect={left:10,top:20,right:800,bottom:600,width:790,height:580};f.api.present(B);edges(f,[10,20,800,600]);f.rect={left:90,top:90,right:900,bottom:800,width:810,height:710};f.api.refresh(A);f.api.clear(A);edges(f,[10,20,800,600]);f.api.clear({...B});opaque(f);});
test('instance and lifecycle generation are both part of owner identity',()=>{for(const changed of[{...A,instanceId:'other'},{...A,lifecycleGeneration:2}]){const f=fixture();f.api.present(A);f.api.clear(changed);edges(f,[40,60,1000,700]);f.api.refresh(changed);edges(f,[40,60,1000,700]);}});
for(const [name,rect]of Object.entries({missing:null,nan:{left:NaN,top:0,right:10,bottom:10,width:10,height:10},infinite:{left:0,top:0,right:Infinity,bottom:10,width:Infinity,height:10},zeroWidth:{left:0,top:0,right:0,bottom:10,width:0,height:10},zeroHeight:{left:0,top:0,right:10,bottom:0,width:10,height:0},negativeWidth:{left:10,top:0,right:0,bottom:10,width:-10,height:10}})){
 test('invalid '+name+' measurement refuses and removes prior transparent hole',()=>{const f=fixture();f.api.present(A);f.rect=rect;assert.throws(()=>f.api.refresh(A));opaque(f);assert.throws(()=>f.api.present(B));opaque(f);});
}
test('clear without owner restores ordinary opacity and preserves unrelated styling',()=>{const f=fixture({classes:['unrelated'],styles:{'--unrelated':'yes'}});f.api.present(A);f.api.clear();opaque(f);assert.ok(f.classes.has('unrelated'));assert.equal(f.styles.get('--unrelated'),'yes');});
test('clear restores preexisting managed property values and priorities',()=>{const initial=Object.fromEntries(KEYS.map((k,i)=>[k,(i+1)+'px'])),priorities=Object.fromEntries(KEYS.map(k=>[k,'important'])),f=fixture({styles:initial,priorities});f.api.present(A);f.api.clear(A);assert.equal(f.classes.has(CLASS),false);assert.deepEqual(Object.fromEntries(f.styles),initial);for(const k of KEYS)assert.equal(f.priorities.get(k),'important');});
test('dispose removes owned hole and later presentation cannot re-enable it',()=>{const f=fixture();f.api.present(A);f.api.dispose();opaque(f);assert.throws(()=>f.api.present(B));opaque(f);f.api.dispose();opaque(f);});
test('clipped measured rectangle uses viewport intersection as absolute CSS edges',()=>{const f=fixture();f.rect={left:-20,top:-10,width:1500,height:1000};f.api.present(A);edges(f,[0,0,1280,900]);f.document.documentElement.clientWidth=800;f.document.documentElement.clientHeight=600;f.api.refresh(A);edges(f,[0,0,800,600]);});
test('positive rectangle outside viewport refuses and clears prior hole',()=>{const f=fixture();f.api.present(A);f.rect={left:1280,top:0,width:10,height:20};assert.throws(()=>f.api.refresh(A));opaque(f);});
test('invalid owner cannot activate a new hole',()=>{for(const owner of[null,{...A,instanceId:''},{...A,documentId:'x'.repeat(129)},{...A,lifecycleGeneration:0},{...A,lifecycleGeneration:1.5}]){const f=fixture();assert.throws(()=>f.api.present(owner));opaque(f);}});
test('preexisting active class refuses second ownership',()=>{assert.throws(()=>fixture({classes:[CLASS]}));});

test('backdrop hiding is owner-scoped and cleanup restores it while foreign clear cannot',()=>{const changes=[],f=fixture({setBackdropHidden:on=>changes.push(on)});f.api.present(A);assert.deepEqual(changes,[true]);assert.equal(f.api.clear(B),false);assert.deepEqual(changes,[true]);f.api.clear(A);assert.deepEqual(changes,[true,false]);f.api.dispose();assert.deepEqual(changes,[true,false]);});
test('failed backdrop preparation clears the hole and asks the original backdrop to restore',()=>{const changes=[],f=fixture({setBackdropHidden:on=>{changes.push(on);if(on)throw Error('controlled backdrop refusal');}});assert.throws(()=>f.api.present(A),/controlled backdrop refusal/);opaque(f);assert.deepEqual(changes,[true,false]);});
test('mapping failure restores a previously hidden backdrop and cannot retain transparent hole',()=>{const changes=[],f=fixture({setBackdropHidden:on=>changes.push(on)});f.api.present(A);f.rect=null;assert.throws(()=>f.api.refresh(A));opaque(f);assert.deepEqual(changes,[true,false]);});

function desktopFixture(visible=true){const f=fixture(),updates=[],stage={visible},root={document:f.document,stageLayer:stage,paper:{view:{update(){updates.push(stage.visible);}}}};return {...f,stage,root,updates,desktop:desktop(root)};}
const desktopBounds={x:40,y:60,width:320,height:180};
test('desktop hides only the captured stage, repeated presentation is idempotent, clear restores exact prior visibility',()=>{for(const visible of [true,false]){const f=desktopFixture(visible);f.desktop.surfaceChanged(A,desktopBounds);assert.equal(f.stage.visible,false);f.desktop.surfaceChanged(A,desktopBounds);assert.equal(f.stage.visible,false);assert.deepEqual(f.updates,visible?[false]:[]);f.desktop.surfaceChanged(null);assert.equal(f.stage.visible,visible);assert.deepEqual(f.updates,visible?[false,true]:[false]);const count=f.updates.length;f.desktop.surfaceChanged(null);assert.equal(f.updates.length,count);opaque(f);}});
test('desktop stage identity replacement fails closed and restores the original captured stage without touching the replacement',()=>{const f=desktopFixture();f.desktop.surfaceChanged(A,desktopBounds);const replacement={visible:true};f.root.stageLayer=replacement;assert.throws(()=>f.desktop.surfaceChanged(A,desktopBounds),/stage changed/);assert.equal(f.stage.visible,true);assert.equal(replacement.visible,true);opaque(f);});
test('desktop refuses missing presentation backdrop without inventing a document or hiding unrelated layers',()=>{const f=desktopFixture();delete f.root.stageLayer;assert.throws(()=>f.desktop.surfaceChanged(A,desktopBounds),/backdrop is unavailable/);assert.equal(f.stage.visible,true);assert.deepEqual(f.updates,[]);opaque(f);});

test('native presented camera affine maps both directions after real CSS stretch without Paper',()=>{
 const M=require('../src/js/adapters/native-viewport-underlay.js').canvasMapping;
 const presentation={view:{mode:'stored-camera',documentToComposition:[2,0,0,2,-160,-90],compositionToDocument:[.5,0,0,.5,80,45]}};
 const c={bounds:{left:20,top:30,width:640,height:360},canvasW:320,canvasH:180,presentation};
 assert.deepEqual(M.point({clientX:100,clientY:90},c),{x:100,y:60});
 assert.deepEqual(M.screen({x:100,y:60},c),{x:100,y:90});
 const rotated={...c,presentation:{view:{mode:'stored-camera',documentToComposition:[0,2,-2,0,300,-100],compositionToDocument:[0,-.5,.5,0,50,150]}}};
 assert.deepEqual(M.screen({x:100,y:60},rotated),{x:380,y:230});
 assert.deepEqual(M.point({clientX:380,clientY:230},rotated),{x:100,y:60});
 assert.equal(M.matchesView({state:{cameraView:true}},presentation),true);
 assert.equal(M.matchesView({state:{cameraView:false}},presentation),false);
 assert.equal(M.matchesView({state:{cameraView:true}},{frame:0}),false);
 assert.deepEqual(M.point({clientX:100,clientY:90},{...c,presentation:{frame:0}}),{x:40,y:30});
 assert.throws(()=>M.point({clientX:100,clientY:90},{...c,presentation:{view:{mode:'stored-camera',compositionToDocument:[1,0,0,NaN,0,0]}}}));
});

test('native CSS inversion removes translation first and retains literal integer and fractional world coordinates',()=>{
 const M=require('../src/js/adapters/native-viewport-underlay.js').canvasMapping;
 function context(bounds,css){
  const [a,b,c,d,e,f]=css,m=[a*320/bounds.width,b*180/bounds.height,c*320/bounds.width,d*180/bounds.height,e*320/bounds.width,f*180/bounds.height];
  const [A,B,C,D,E,F]=m,det=A*D-B*C;
  return {bounds,canvasW:320,canvasH:180,presentation:{view:{mode:'stored-camera',documentToComposition:m,
   compositionToDocument:[D/det,-B/det,-C/det,A/det,(C*F-D*E)/det,(B*E-A*F)/det]}}};
 }
 const cases=[
  [{left:0,top:0,width:400,height:240},[2,0,0,2,100,100],[40,48],[-30,-26]],
  [{left:20,top:30,width:400,height:240},[2,0,0,2,100,100],[140,170],[10,20]],
  [{left:20,top:30,width:400,height:240},[2,0,0,2,100,100],[140.25,171],[10.125,20.5]],
  [{left:66,top:102,width:1038,height:504},[1,0,0,1,359,162],[500,284],[75,20]],
  [{left:0,top:0,width:400,height:240},[0,2,-2,0,300,20],[180,100],[40,60]]
 ];
 for(const [bounds,css,client,world]of cases)assert.deepEqual(M.point({clientX:client[0],clientY:client[1]},context(bounds,css)),{x:world[0],y:world[1]});
});

test('native CSS inversion retains inverse-pair, finite and nonzero-determinant refusals',()=>{
 const M=require('../src/js/adapters/native-viewport-underlay.js').canvasMapping;
 const c={bounds:{left:20,top:30,width:640,height:360},canvasW:320,canvasH:180,
  presentation:{view:{mode:'stored-camera',documentToComposition:[2,0,0,2,-160,-90],compositionToDocument:[.5,0,0,.5,80,45]}}};
 const point={clientX:100,clientY:90};
 assert.throws(()=>M.point(point,{...c,presentation:{view:{...c.presentation.view,compositionToDocument:[.5,0,0,.5,81,45]}}}));
 assert.throws(()=>M.point(point,{...c,presentation:{view:{...c.presentation.view,documentToComposition:[0,0,0,0,0,0]}}}));
 assert.throws(()=>M.point(point,{...c,bounds:{...c.bounds,width:Infinity}}));
 assert.throws(()=>M.point({clientX:NaN,clientY:90},c));
 assert.throws(()=>M.point({clientX:10,clientY:0},{bounds:{left:0,top:0,width:1e-308,height:1},canvasW:1,canvasH:1,
  presentation:{view:{mode:'canvas',documentToComposition:[1,0,0,1,0,0],compositionToDocument:[1,0,0,1,0,0]}}}));
});


function navigationSourceHarness(native=true){
 const fs=require('node:fs'),vm=require('node:vm'),{extractFunction}=require('./fixtures/lib/sandbox.cjs');
 const calls=[],listeners={},blur={},state={tool:'hand',spaceDown:false,playing:false,isPanning:false};
 const target={addEventListener(name,fn){listeners[name]=fn;}},document={readyState:'complete',activeElement:null,getElementById:()=>target,
  documentElement:{classList:{add(){},remove(){}},style:{setProperty(){},removeProperty(){}}}};
 const session={blocksLegacy:()=>native,isActive:()=>true,readPresentedObjectSession:()=>null,
  beginViewportNavigation(kind,point){calls.push(['begin',kind,point]);return {kind};},
  updateViewportNavigation(token,point,factor,aim){calls.push(['update',token.kind,point,factor,aim]);return Promise.resolve();}};
 const root={document,state,NemoNativeObjectProject:{desktop:()=>session},addEventListener(name,fn){blur[name]=fn;}};
 const sandbox={window:root,document,state,canvasEl:target,Promise,Math,Number,showToast(){}};
 Object.defineProperty(sandbox,'view',{configurable:true,get(){throw Error('native navigation must not read or mutate Paper viewport');}});
 const context=vm.createContext(sandbox),tools=fs.readFileSync(require.resolve('../src/js/tools.js'),'utf8');
 vm.runInContext(extractFunction(tools,'nativeViewportNavigation'),context);
 vm.runInContext(tools.slice(tools.indexOf("canvasEl.addEventListener('wheel',function(e){"),tools.indexOf("},{passive:false});",tools.indexOf("canvasEl.addEventListener('wheel',function(e){"))+19),context);
 const wheel=listeners.wheel;
 vm.runInContext(fs.readFileSync(require.resolve('../src/js/viewtools-bridge.js'),'utf8'),context);
 function event(patch={}){return {type:'pointerdown',clientX:100,clientY:60,movementX:0,movementY:0,preventDefault(){},stopImmediatePropagation(){},...patch};}
 return {calls,listeners,state,context,root,blur,wheel,event,vm,fs};
}
test('actual wheel and capture hand/zoom handlers consume native session coordinates without a Paper fallback',()=>{
 const f=navigationSourceHarness();
 f.wheel(f.event({deltaY:100,ctrlKey:false}));assert.equal(f.calls.at(-1)[3],0.9);
 f.wheel(f.event({deltaY:-25,ctrlKey:false}));assert.equal(f.calls.at(-1)[3],1.025);
 f.wheel(f.event({deltaY:40,ctrlKey:true}));assert.equal(f.calls.at(-1)[3],0.94);
 f.listeners.pointerdown(f.event());f.listeners.pointermove(f.event({type:'pointermove',clientX:80,clientY:70}));
 assert.deepEqual(JSON.parse(JSON.stringify(f.calls.at(-1))),['update','pan',{x:80,y:70},null,null]);
 f.listeners.pointerup(f.event({type:'pointerup'}));assert.equal(f.state.isPanning,false);
 f.state.tool='zoom';f.listeners.pointerdown(f.event());f.listeners.pointermove(f.event({type:'pointermove',clientX:220}));
 assert.equal(f.calls.at(-1)[3],2);f.listeners.pointerup(f.event({type:'pointerup',clientX:220}));
 f.listeners.pointerdown(f.event());const n=f.calls.length;f.listeners.pointercancel(f.event({type:'pointercancel'}));assert.equal(f.calls.length,n);
 f.state.tool='hand';f.listeners.pointerdown(f.event());f.blur.blur();assert.equal(f.state.isPanning,false);
});
test('actual keyboard, Fit/Reset and zoom scrub route to native navigation while legacy mode retains its original math',()=>{
 const f=navigationSourceHarness(),timeline=f.fs.readFileSync(require.resolve('../src/js/timeline.js'),'utf8');
 const zoomKeys=timeline.split('\n').filter(line=>line.includes("event.code==='Equal'")||line.includes("event.code==='Minus'")||line.includes("event.code==='Digit0'"));
 const member=timeline.split('\n').find(line=>line.includes('  fitCanvas:function(){'));
 f.context.fitCanvas=()=>{throw Error('legacy fit');};f.context.resetView=()=>{throw Error('legacy reset');};
 f.vm.runInContext('window.SM={'+member.trim()+'};',f.context);
 f.vm.runInContext('function keys(event){'+zoomKeys.join('\n')+'}',f.context);
 for(const code of ['Equal','Minus','Digit0']){f.context.event=f.event({metaKey:true,ctrlKey:false,altKey:true,code});f.vm.runInContext('keys(event)',f.context);}
 assert.deepEqual(f.calls.filter(c=>c[0]==='update').map(c=>[c[3],c[4]]),[[1.1,undefined],[1/1.1,undefined],[undefined,'fit']]);
 f.root.SM.resetView();assert.equal(f.calls.at(-1)[4],'reset');
 const marker="document.getElementById('zoom-scrub').addEventListener('input',function(){",start=timeline.indexOf(marker),end=timeline.indexOf('\n});',start)+4;
 const input={value:'250',addEventListener(name,fn){this.fn=fn;}};f.context.document.getElementById=()=>input;
 f.vm.runInContext(timeline.slice(start,end),f.context);input.fn.call(input);assert.equal(f.calls.at(-1)[4],2.5);
});

test('absent native path preserves the existing wheel zoom and Fit/Reset legacy consumers',()=>{
 const f=navigationSourceHarness(false);let fit=0,reset=0;
 class P{constructor(x,y){this.x=x;this.y=y;}subtract(p){return new P(this.x-p.x,this.y-p.y);}add(p){return new P(this.x+p.x,this.y+p.y);}}
 const view={zoom:1,center:new P(160,90),viewToProject(p){return new P(this.center.x+p.x/this.zoom,this.center.y+p.y/this.zoom);}};
 Object.defineProperty(f.context,'view',{value:view});f.context.Point=P;f.context.updZoom=()=>{};f.context.renderArcs=()=>{};
 f.wheel(f.event({deltaY:100,ctrlKey:false,offsetX:0,offsetY:0}));assert.equal(view.zoom,.9);assert.equal(f.calls.length,0);
 const timeline=f.fs.readFileSync(require.resolve('../src/js/timeline.js'),'utf8'),member=timeline.split('\n').find(line=>line.includes('  fitCanvas:function(){'));
 f.context.fitCanvas=()=>++fit;f.context.resetView=()=>++reset;f.vm.runInContext('window.SM={'+member.trim()+'};',f.context);
 f.root.SM.fitCanvas();f.root.SM.resetView();assert.deepEqual([fit,reset],[1,1]);
});

test('native inverse navigation preserves original absolute zoom clamps and rotated world-pan directions',()=>{
 const Exposure=require('../src/js/adapters/native-object-open-exposure.js'),state=Exposure.navigation(()=>'stored-camera');
 const info={instanceId:'i',documentId:'d',contentRevision:0,documentSnapshotId:'s',lifecycleGeneration:1,canvasW:320,canvasH:180};
 const bounds={x:0,y:0,width:320,height:180},mapping={cssBounds:bounds};
 const p={frame:0,view:{mode:'stored-camera',navigation:{zoom:1,pan:[0,0]},documentToComposition:[2,0,0,2,-160,-90],compositionToDocument:[.5,0,0,.5,80,45]}};
 let t=state.begin(p,info,0,mapping,bounds,'zoom',null);assert.equal(state.update(t,info,0,mapping,null,100).zoom,10);
 const fresh=Exposure.navigation(()=>'stored-camera');t=fresh.begin(p,info,0,mapping,bounds,'zoom',null);
 assert.equal(fresh.update(t,info,0,mapping,null,.001).zoom,.025);
 const rotated={frame:0,view:{...p.view,documentToComposition:[0,2,-2,0,300,-100],compositionToDocument:[0,-.5,.5,0,50,150]}};
 const turn=Exposure.navigation(()=>'stored-camera');t=turn.begin(rotated,info,0,mapping,bounds,'pan',{x:100,y:60});
 assert.deepEqual(turn.update(t,info,0,mapping,{x:120,y:60}).pan,[0,10]);
 assert.throws(()=>turn.update(t,{...info,documentSnapshotId:'foreign'},0,mapping,{x:120,y:60}),/source changed/);
 const oldOwner={...info,instanceId:'a/b',documentId:'c'},newOwner={...info,instanceId:'a',documentId:'b/c'};
 const owners=Exposure.navigation(()=>'stored-camera');t=owners.begin(p,oldOwner,0,mapping,bounds,'zoom',null);owners.update(t,oldOwner,0,mapping,null,2);
 assert.deepEqual(owners.read(newOwner).navigation,{zoom:1,pan:[0,0]});
});


const CameraDesktop=require('../src/js/adapters/native-object-project-desktop.js');
function cameraToolbarRoot(){
 const buttons=Object.fromEntries(['btn-camera','btn-cam-addkey','btn-cam-ease-edit'].map(id=>[id,{
  hidden:false,setAttribute(k,v){this[k]=v;},addEventListener(kind,fn){this[kind]=fn;}
 }]));
 const root={state:{cameraView:false,currentFrame:0},SM:{t:key=>key},
  document:{readyState:'complete',getElementById:id=>buttons[id]||null},NemoNativeViewportUnderlay:require('../src/js/adapters/native-viewport-underlay.js')};
 return {root,buttons};
}
test('bound camera toolbar uses the real Project preview and refuses legacy actions while a frame is pending',async()=>{
 const Project=require('../src/js/adapters/native-object-project.js'),{root,buttons}=cameraToolbarRoot(),calls=[];
 let installed=false,held=null,serial=0;
 const pin={apiVersion:2,instanceId:'camera-instance',documentId:'camera-document',lifecycleGeneration:1,
  contentRevision:0,documentSnapshotId:'camera-snapshot',documentDigest:'a'.repeat(64),byteCount:100,
  canvasW:320,canvasH:180,fps:24,totalFrames:8,layers:[],reference:null};
 async function invoke(command,args){
  const r=args&&JSON.parse(args.requestJson);calls.push({command,r});
  if(command==='nemo_native_object_client_status')return {...pin,available:installed};
  if(command==='nemo_native_object_bootstrap'){installed=true;return {...pin,requestId:r.requestId,resourceCount:0,viewportAvailable:false};}
  if(command==='nemo_native_object_reference')return {...pin,requestId:r.requestId};
  assert.equal(command,'nemo_native_object_preview');if(held)await held;
  return {status:'presented',workId:'camera-work-'+(++serial),viewGeneration:1,
   view:{mode:r.viewMode||'canvas',cameraAvailable:true,navigation:{zoom:1,pan:[0,0]},
    documentToComposition:[1,0,0,1,0,0],compositionToDocument:[1,0,0,1,0,0]}};
 }
 const session=Project.create({invoke,frame:()=>root.state.currentFrame,viewMode:()=>root.state.cameraView?'stored-camera':'canvas',
  mapping:()=>({cssBounds:{x:0,y:0,width:320,height:180},physicalExtent:{width:320,height:180}}),
  presented:()=>CameraDesktop.renderCameraViewLock(root)});
 root.NemoNativeObjectProject={desktop:()=>session};root.__TAURI__={core:{invoke}};
 Object.defineProperties(root.state,{cameraKeys:{get(){assert.fail('native legacy cameraKeys read');}},cameraLayerOn:{get(){assert.fail('native legacy cameraLayerOn read');}}});
 CameraDesktop.bindCameraUI(root,{toggle:()=>assert.fail('native legacy toggle'),addKey:()=>assert.fail('native legacy key'),editEase:()=>assert.fail('native legacy easing')});
 await session.openPath('/camera.json');const info=session.info();
 assert.equal(buttons['btn-camera'].hidden,false);assert.equal(buttons['btn-camera']['aria-pressed'],'false');
 root.state.currentFrame=4;let release;held=new Promise(resolve=>release=resolve);
 const preview=buttons['btn-camera'].click();assert.equal(root.state.cameraView,true);
 assert.equal(session.readPresentedObjectSession(),null);assert.equal(buttons['btn-camera'].hidden,true);
 const count=calls.length;buttons['btn-camera'].click();buttons['btn-cam-addkey'].click();buttons['btn-cam-ease-edit'].click();
 assert.equal(calls.length,count);assert.equal(root.state.cameraView,true);
 release();await preview;held=null;assert.equal(buttons['btn-camera'].hidden,false);
 assert.equal(session.readPresentedObjectSession().presentation.view.mode,'stored-camera');
 assert.equal(buttons['btn-camera']['aria-pressed'],'true');
 await buttons['btn-camera'].click();assert.equal(root.state.cameraView,false);
 assert.equal(session.readPresentedObjectSession().presentation.view.mode,'canvas');assert.strictEqual(session.info(),info);
 const frames=calls.filter(c=>c.command==='nemo_native_object_preview');assert.deepEqual(frames.map(c=>c.r.frame),[0,4,4]);
 assert.equal(frames[1].r.viewMode,'stored-camera');assert.equal(Object.hasOwn(frames[2].r,'viewMode'),false);
 assert.ok(calls.every(c=>!c.command.includes('dispatch')));assert.equal(info.contentRevision,0);
});
test('native camera toolbar stays unavailable for idle, blocked, missing and unpresented owners without legacy fallback',()=>{
 for(const [session,tauri] of [[null,true],[{isActive:()=>false,blocksLegacy:()=>false,readPresentedObjectSession:()=>null},true],
  [{isActive:()=>false,blocksLegacy:()=>true,readPresentedObjectSession:()=>null},false],
  [{isActive:()=>true,readPresentedObjectSession:()=>({presentation:{view:{cameraAvailable:false}}})},false]]){
  const {root,buttons}=cameraToolbarRoot();if(tauri)root.__TAURI__={core:{}};root.NemoNativeObjectProject={desktop:()=>session};
  CameraDesktop.bindCameraUI(root,{toggle:()=>assert.fail('unready legacy toggle'),addKey:()=>assert.fail('unready legacy key'),editEase:()=>assert.fail('unready legacy easing')});
  assert.equal(CameraDesktop.renderCameraViewLock(root),true);assert.equal(buttons['btn-camera'].hidden,true);
  for(const button of Object.values(buttons))button.click();assert.equal(root.state.cameraView,false);
 }
});
test('actual legacy camera callbacks preserve toggle, key undo and shared easing editor through the Desktop binding',()=>{
 const fs=require('node:fs'),vm=require('node:vm'),{root,buttons}=cameraToolbarRoot(),calls=[];
 Object.assign(root.state,{cameraLayerOn:false,cameraKeys:[],canvasW:320,canvasH:180,tool:'select'});
 root.NemoNativeObjectProjectDesktop=CameraDesktop;root.window=root;
 Object.assign(root,{pushUndo:()=>calls.push('undo'),renderLayerList:()=>calls.push('layers'),renderTimeline:()=>calls.push('timeline'),
  showToast:message=>calls.push(message),SMEngineBridge:{renderNow:()=>calls.push('render')},
  _curveEditor:{editCameraSeg:(segment,label)=>calls.push([segment.frame,label])}});
 root.SM.setTool=tool=>{root.state.tool=tool;};
 vm.runInNewContext(fs.readFileSync(require.resolve('../src/js/camera.js'),'utf8'),root);
 buttons['btn-camera'].click();assert.equal(root.state.cameraLayerOn,true);assert.equal(root.state.tool,'camera');
 assert.deepEqual(JSON.parse(JSON.stringify(root.state.cameraKeys)),[{frame:0,x:160,y:90,w:320,rot:0,hOut:[0,0],hIn:[0,0],ease:[.42,0,.58,1]}]);
 assert.deepEqual(calls,['undo','layers','timeline','layers','render']);
 buttons['btn-camera'].click();assert.equal(root.state.tool,'select');assert.equal(calls.filter(c=>c==='undo').length,1);
 buttons['btn-cam-addkey'].click();assert.equal(root.state.cameraKeys.length,0);
 root.state.currentFrame=4;buttons['btn-cam-addkey'].click();assert.equal(root.state.cameraKeys[0].frame,4);
 assert.equal(calls.filter(c=>c==='undo').length,3);
 root.SMCamera.setKey(12,{x:200,y:90,w:160});buttons['btn-cam-ease-edit'].click();
 assert.deepEqual(calls.at(-1),[4,'Caméra : clé 5 → 13']);assert.equal(root.state.cameraView,false);
});
test('Desktop completion paints its own bound camera lock without calling the legacy camera facade',async()=>{
 const {root,buttons}=cameraToolbarRoot(),info={},presentation={view:{cameraAvailable:true}},events=[];
 root.__TAURI__={core:{invoke(){assert.fail('no native request for lock painting');}}};
 const session={isActive:()=>true,info:()=>info,readSubscriptionGeneration:()=>1,readPresentedObjectSession:()=>({presentation})};
 root.NemoNativeObjectProject={desktop:()=>session};
 root.SMCamera={renderNativeViewLock(){assert.fail('completion must use Desktop painter');}};
 root.NemoNativeProjectSettingsController={presented:value=>events.push(value)};
 root.NemoNativeObjectUI={refreshPolicy:()=>events.push('policy')};
 CameraDesktop.bindCameraUI(root,{toggle(){},addKey(){},editEase(){}});
 CameraDesktop.createPorts(root).presented(info);await new Promise(resolve=>setTimeout(resolve,0));
 assert.equal(buttons['btn-camera'].hidden,false);assert.equal(buttons['btn-camera']['aria-pressed'],'false');
 assert.equal(buttons['btn-camera'].title,'titleCameraViewLockHint');assert.deepEqual(events,[info,'policy']);
});


test('actual no-move Zoom click after a real frame4 Canvas relock presents the original 1.25 affine once',async()=>{
 const Project=require('../src/js/adapters/native-object-project.js'),f=navigationSourceHarness();
 const toolbar=cameraToolbarRoot(),buttons=toolbar.buttons,root=f.root,calls=[],errors=[],observations=[];
 const canvas=root.document.getElementById('canvas-area'),pill={value:'500'};
 canvas.getBoundingClientRect=()=>({left:66,top:102,right:866,bottom:552,width:800,height:450});
 const nodes={...buttons,'drawing-canvas':canvas,'canvas-area':canvas,'zoom-scrub':pill};
 root.document.getElementById=id=>nodes[id]||null;
 Object.assign(root,{SM:toolbar.root.SM,NemoNativeViewportUnderlay:toolbar.root.NemoNativeViewportUnderlay,devicePixelRatio:2});
 Object.assign(root.state,{cameraView:false,currentFrame:4,tool:'zoom'});
 Object.defineProperty(root,'paper',{get(){assert.fail('native click must not read or mutate Paper');}});
 f.context.showToast=message=>errors.push(message);
 root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__=(event,code)=>observations.push([event,code]);
 function frozen(value){
  if(value&&typeof value==='object'){Object.values(value).forEach(frozen);Object.freeze(value);}
  return value;
 }
 const pin=frozen({apiVersion:2,instanceId:'click-instance',documentId:'click-document',lifecycleGeneration:1,
  contentRevision:0,documentSnapshotId:'click-snapshot',documentDigest:'a'.repeat(64),byteCount:4269,
  canvasW:320,canvasH:180,fps:24,totalFrames:21,layers:[],reference:null,undoDepth:0,redoDepth:0});
 const canvasView=frozen({mode:'canvas',cameraAvailable:true,navigation:{zoom:1,pan:[0,0]},
  documentToComposition:[1,0,0,1,0,0],compositionToDocument:[1,0,0,1,0,0]});
 const lockedView=frozen({mode:'stored-camera',cameraAvailable:true,navigation:{zoom:1,pan:[0,0]},
  documentToComposition:[2,0,0,2,-160,-90],compositionToDocument:[.5,0,0,.5,80,45]});
 const zoomedView=frozen({mode:'stored-camera',cameraAvailable:true,navigation:{zoom:1.25,pan:[0,0]},
  documentToComposition:[2.5,0,0,2.5,-240,-135],compositionToDocument:[.4,0,0,.4,96,54]});
 // Closed literal native receipts: no matrix or navigation is derived from a request.
 const replies=frozen([
  {status:'presented',workId:'click-open',viewGeneration:1,view:canvasView},
  {status:'presented',workId:'click-initial-lock',viewGeneration:1,view:lockedView},
  {status:'presented',workId:'click-canvas',viewGeneration:1,view:canvasView},
  {status:'presented',workId:'click-relock',viewGeneration:1,view:lockedView},
  {status:'presented',workId:'click-zoom',viewGeneration:1,view:zoomedView}
 ]);
 let installed=false,previewIndex=0;
 async function invoke(command,args){
  const request=args&&JSON.parse(args.requestJson);calls.push({command,request});
  if(command==='nemo_native_object_client_status')return {...pin,available:installed};
  if(command==='nemo_native_object_bootstrap'){
   assert.equal(request.sourcePath,'/Camera-view.json');assert.equal(request.projectOnly,true);
   installed=true;return {...pin,requestId:request.requestId,resourceCount:0,viewportAvailable:false};
  }
  if(command==='nemo_native_object_reference'){
   assert.deepEqual(request.action,{kind:'inspect'});return {...pin,requestId:request.requestId};
  }
  assert.equal(command,'nemo_native_object_preview');
  assert.equal(request.frame,previewIndex===0?0:4);assert.equal(request.contentRevision,0);
  assert.equal(request.documentSnapshotId,'click-snapshot');assert.equal(request.lifecycleGeneration,1);
  assert.equal(request.instanceId,'click-instance');assert.equal(request.documentId,'click-document');
  assert.deepEqual(request.viewport,{cssBounds:{x:66,y:102,width:800,height:450},
   physicalExtent:{width:1600,height:900},compositionExtent:{width:320,height:180},reportedDpr:2});
  const reply=replies[previewIndex++];assert.ok(reply,'only the five original preview intents are admitted');
  assert.equal(request.viewMode||'canvas',reply.view.mode);
  if(previewIndex===5)assert.deepEqual(request.navigation,{zoom:1.25,pan:[0,0]});
  else assert.equal(Object.hasOwn(request,'navigation'),false);
  return reply;
 }
 const underlay=desktop(root),session=Project.create({invoke,frame:()=>root.state.currentFrame,
  mapping:info=>underlay.mapping(info),viewMode:()=>root.state.cameraView?'stored-camera':'canvas',
  openDiagnostic:(event,code)=>CameraDesktop.openDiagnostic(root,event,code),
  presented:()=>CameraDesktop.renderCameraViewLock(root)});
 root.NemoNativeObjectProject={desktop:()=>session};root.__TAURI__={core:{invoke}};
 CameraDesktop.bindCameraUI(root,{toggle:()=>assert.fail('legacy camera toggle'),
  addKey:()=>assert.fail('legacy camera key'),editEase:()=>assert.fail('legacy camera easing')});
 await session.openPath('/Camera-view.json');
 assert.equal(session.readPresentedObjectSession().presentation.frame,0);
 assert.equal(session.readPresentedObjectSession().presentation.view.mode,'canvas');
 await buttons['btn-camera'].click();
 assert.equal(session.readPresentedObjectSession().presentation.view.mode,'stored-camera');
 await buttons['btn-camera'].click();
 assert.equal(session.readPresentedObjectSession().presentation.view.mode,'canvas');
 await buttons['btn-camera'].click();
 assert.equal(buttons['btn-camera']['aria-pressed'],'true');assert.equal(previewIndex,4);
 assert.deepEqual(session.readPresentedObjectSession().presentation.view,lockedView);
 const info=session.info(),source=JSON.stringify(info);
 f.listeners.pointerdown(f.event({clientX:466,clientY:327}));
 assert.equal(previewIndex,4,'pointerdown alone does not apply the click step');
 f.listeners.pointerup(f.event({type:'pointerup',clientX:466,clientY:327}));
 assert.equal(session.readPresentedObjectSession(),null,'the real pending frame hides the old public proof');
 // Drain completion of this one real session request, without another gesture or preview.
 await new Promise(resolve=>setImmediate(resolve));
 const shown=session.readPresentedObjectSession();assert.ok(shown);
 assert.equal(previewIndex,5);assert.equal(shown.presentation.frame,4);
 assert.deepEqual(shown.presentation.view,zoomedView);
 assert.deepEqual(shown.presentation.view.documentToComposition,[2.5,0,0,2.5,-240,-135]);
 assert.equal(pill.value,625);assert.equal(root.state.isPanning,false);
 assert.strictEqual(session.info(),info);assert.equal(JSON.stringify(session.info()),source);
 assert.deepEqual([info.contentRevision,info.undoDepth,info.redoDepth],[0,0,0]);
 assert.deepEqual(errors,[]);assert.deepEqual(f.calls,[],'no stub navigation provider was used');
 assert.deepEqual(observations.filter(([event])=>event.startsWith('navigation-')),
  [['navigation-begin','zoom:admitted'],['navigation-update','view-updated']]);
 assert.equal(calls.filter(call=>call.command==='nemo_native_object_preview').length,5);
 assert.equal(calls.some(call=>call.command.includes('dispatch')||call.command.includes('export')),false);
});

test('navigation observation preserves inactive guard order and callback failures cannot change refusal',async()=>{
 const Project=require('../src/js/adapters/native-object-project.js'),observations=[],reads=[];
 const session=Project.create({invoke(){assert.fail('inactive observation cannot invoke native');},
  frame(){reads.push('frame');return 4;},mapping(){reads.push('mapping');return null;},
  projectionReady(){reads.push('projection');return true;},subscriptionActive(){reads.push('subscription');return true;},
  openDiagnostic(event,code){observations.push([event,code]);throw Error('observer failed');}});
 assert.equal(session.beginViewportNavigation('zoom',{x:466,y:327}),null);
 assert.equal(session.beginViewportNavigation('/private/document-name',{x:466,y:327}),null);
 const token=Object.defineProperty({},'kind',{get(){assert.fail('refused navigation must not add a token read');}});
 await assert.rejects(session.updateViewportNavigation(token,{x:466,y:327},1.25),
  error=>error.message==='Native navigation owner is unavailable');
 assert.deepEqual(reads,[]);
 assert.deepEqual(observations,[['navigation-begin','owner-unavailable'],['navigation-begin','owner-unavailable'],
  ['navigation-update','rejected']]);
});

test('navigation diagnostic storage and fixture observation retain only the closed vocabulary',()=>{
 const observed=[],root={__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__(event,code){observed.push([event,code]);throw Error('ignored observer');}};
 const hostile=Object.defineProperty({},'toString',{get(){assert.fail('diagnostics must not coerce untrusted data');}});
 const cases=[['navigation-begin','zoom:admitted','zoom:admitted'],['navigation-begin','pan:view-refused','pan:view-refused'],
  ['navigation-begin','/private/document-name','unknown'],['navigation-begin','zoom:admitted:private','unknown'],
  ['navigation-begin',hostile,'unknown'],['navigation-update','view-updated','view-updated'],
  ['navigation-update','/private/source-path','unknown'],['navigation-update',hostile,'unknown']];
 for(const [event,code]of cases)CameraDesktop.openDiagnostic(root,event,code);
 assert.deepEqual(observed,cases.map(([event,,expected])=>[event,expected]));
});

test('a throwing diagnostic getter cannot change native navigation refusal or read guarded state',async()=>{
 const Project=require('../src/js/adapters/native-object-project.js');let reads=0;
 const ports={invoke(){assert.fail('inactive navigation cannot invoke native');},
  frame(){assert.fail('inactive navigation cannot read frame');},mapping(){assert.fail('inactive navigation cannot read mapping');}};
 Object.defineProperty(ports,'openDiagnostic',{get(){reads++;throw Error('diagnostic getter failed');}});
 const session=Project.create(ports);
 assert.equal(session.beginViewportNavigation('zoom',{x:466,y:327}),null);
 await assert.rejects(session.updateViewportNavigation(null,{x:466,y:327},1.25),
  error=>error.message==='Native navigation owner is unavailable');
 assert.equal(reads,2);
});

test('desktop mapping fits the canvas to the document aspect before measuring; ending the presentation restores it (#2343 B1)',()=>{
 const f=fixture(),seen=[];
 const canvas={getBoundingClientRect(){seen.push([f.classes.has('nemo-native-fit'),f.styles.get('--nemo-native-aspect')]);return {left:40,top:60,width:640,height:360};}};
 const root={document:{...f.document,getElementById:id=>id==='drawing-canvas'?canvas:null},stageLayer:{visible:true},paper:{view:{update(){}}},devicePixelRatio:2};
 const ports=desktop(root),viewport=ports.mapping({canvasW:1920,canvasH:1080});
 assert.deepEqual(seen,[[true,String(1920/1080)]],'the document aspect applies before the one measurement');
 assert.deepEqual(viewport.cssBounds,{x:40,y:60,width:640,height:360});
 assert.deepEqual(viewport.compositionExtent,{width:1920,height:1080});
 assert.throws(()=>ports.mapping({canvasW:0,canvasH:1080}),/aspect/);
 ports.surfaceChanged(null);
 assert.equal(f.classes.has('nemo-native-fit'),false);assert.equal(f.styles.has('--nemo-native-aspect'),false);
});
