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


function navigationSourceHarness(native=true){
 const fs=require('node:fs'),vm=require('node:vm'),{extractFunction}=require('./fixtures/lib/sandbox.cjs');
 const calls=[],listeners={},blur={},state={tool:'hand',spaceDown:false,playing:false,isPanning:false};
 const target={addEventListener(name,fn){listeners[name]=fn;}},document={readyState:'complete',activeElement:null,getElementById:()=>target};
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
