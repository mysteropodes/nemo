'use strict';
// Real Motion ancestor capture, LayerInOut handlers, and native Range/Desktop/Intent/Project/SDK.
// DOM propagation/native I/O are modeled; no installed/native product acceptance is claimed.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {createRequire}=require('node:module');
const Selection=require('../src/js/layer-inout-selection.js');
const fixturePath=path.join(__dirname,'native-layer-range-desktop.test.cjs'),fixtureSource=fs.readFileSync(fixturePath,'utf8');
const firstTest=fixtureSource.indexOf('\ntest(');assert.ok(firstTest>0);
const makeHarness=new Function('require','__dirname',fixtureSource.slice(0,firstTest)+'\nreturn harness;')(createRequire(fixturePath),__dirname);
const motionSource=fs.readFileSync(path.join(__dirname,'../src/js/motion.js'),'utf8');
function matches(node,selector){
 if(selector[0]==='#')return node.id===selector.slice(1);
 return selector[0]==='.'&&selector.slice(1).split('.').every(name=>String(node.className||'').split(' ').includes(name));
}
async function captured(options={}){
 const h=await makeHarness(options),Node=h.grid.constructor;
 Node.prototype.closest=function(selectors){for(let n=this;n;n=n.parentNode)if(selectors.split(',').some(s=>matches(n,s.trim())))return n;return null;};
 const wrap=new Node('div');wrap.id='fg-wrap';wrap.clientWidth=500;wrap.clientHeight=100;
 wrap.getBoundingClientRect=()=>({left:0,top:0,right:500,bottom:100});h.doc.body.appendChild(wrap);wrap.appendChild(h.grid);
 const captures=[];wrap.addEventListener=(type,fn,capture)=>{assert.equal(type,'mousedown');assert.equal(capture,true);captures.push(fn);};
 h.doc.getElementById=id=>id==='fg-wrap'?wrap:null;
 let legacyStarts=0;
 if(options.browser)h.root.NemoNativeObjectProject={desktop:()=>null};
 if(options.omitHelper)delete h.root.NemoNativeLayerRangeDesktop;
 const a=motionSource.indexOf('  function routeNativeLayerBarMarquee('),b=motionSource.indexOf("  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', initGridMarquee)",a);
 assert.ok(a>0&&b>a);
 vm.runInContext(motionSource.slice(a,b)+'\ninitGridMarquee();',vm.createContext({window:h.root,document:h.doc,state:h.root.state,startMarquee(){legacyStarts++;}}));
 assert.equal(captures.length,1);
 function down(target,extra={}){
  let stopped=false;const event={target,button:0,clientX:95,clientY:0,defaultPrevented:false,
   preventDefault(){this.defaultPrevented=true;},stopPropagation(){stopped=true;},...extra};
  // Capture executes before target/bubble, just as fg-wrap does in the shipped UI.
  for(const callback of captures)callback(event);
  if(!stopped){for(let n=target;n;n=n.parentNode){for(const callback of n.events.get('mousedown')||[])callback(event);if(stopped)break;}}
  return event;
 }
 function geometry(i,left,right){const bar=h.bar(i),top=i*22;bar.getBoundingClientRect=()=>({left,right,top,bottom:top+20});
  bar.querySelector('.layer-inout-handle.left').getBoundingClientRect=()=>({left,right:left+4,top,bottom:top+20});
  bar.querySelector('.layer-inout-handle.right').getBoundingClientRect=()=>({left:right-4,right,top,bottom:top+20});}
 geometry(0,100,300);geometry(1,-100,110);
 return {h,wrap,down,legacyStarts:()=>legacyStarts};
}

test('actual Motion ancestor capture routes nested native row backgrounds and empty wrap before any legacy marquee',async()=>{
 for(const targetName of ['row','wrap']){
  const c=await captured(),h=c.h,target=targetName==='row'?h.rows()[0]:c.wrap;
  assert.equal(c.down(target).defaultPrevented,true);
  h.doc.fire('mousemove',{clientX:115,clientY:43});h.doc.fire('mouseup',{clientX:115,clientY:43});
  assert.equal(c.legacyStarts(),0);assert.equal(h.legacy(),0);assert.equal(h.commands().length,0);
  assert.deepEqual(h.root.SMLayerInOut.getBarSelection(),[{li:0,part:'in'},{li:1,part:'out'}]);
  c.down(h.bar(0).querySelector('.layer-inout-handle.left'),{clientX:100});h.move(120);h.up(120);await h.settle();
  assert.equal(h.commands().length,1);assert.equal(h.commands()[0].q.payload.part,'in');
  assert.deepEqual(h.commands()[0].q.payload.barSelection,[{layerUid:'alpha',part:'in'},{layerUid:'beta',part:'out'}]);
 }
});

test('Motion bar and handle exemptions still reach their own native drag once; scrollbar and other gestures retain guards',async()=>{
 for(const handle of [false,true]){
  const c=await captured(),h=c.h,target=handle?h.bar(0).querySelector('.layer-inout-handle.right'):h.bar(0);
  c.down(target,{clientX:100});h.move(120);h.up(120);await h.settle();
  assert.equal(c.legacyStarts(),0);assert.equal(h.commands().length,1);assert.equal(h.commands()[0].q.payload.part,handle?'out':'both');assert.equal(h.legacy(),0);
 }
 const c=await captured();assert.equal(c.down(c.wrap,{clientX:501}).defaultPrevented,false);
 assert.equal(c.down(c.wrap,{clientY:101}).defaultPrevented,false);assert.equal(c.down(c.wrap,{button:2}).defaultPrevented,false);
 assert.equal(c.legacyStarts(),0);assert.equal(c.h.commands().length,0);
 for(const name of ['motion-key-connect','native-opacity-diamond','native-opacity-connector','motion-graph-resize']){
  const node=new c.h.grid.constructor();if(name==='motion-graph-resize')node.id=name;else node.className=name;
  c.wrap.appendChild(node);assert.equal(c.down(node).defaultPrevented,false);
 }
 assert.equal(c.legacyStarts(),0);assert.equal(c.h.commands().length,0);
});

test('native missing helper fails closed at actual ancestor; ordinary browser keeps legacy route and required shared UI geometry',async()=>{
 const native=await captured({omitHelper:true});assert.equal(native.down(native.h.rows()[0]).defaultPrevented,true);
 assert.equal(native.legacyStarts(),0);assert.equal(native.h.commands().length,0);assert.ok(native.h.root.notices.length);
 const browser=await captured({browser:true,omitHelper:true});assert.equal(browser.down(browser.wrap).defaultPrevented,true);
 assert.equal(browser.legacyStarts(),1);assert.equal(browser.h.commands().length,0);
 const bar={querySelector:()=>null},rectangle={left:0,right:100};
 assert.equal(Selection.partForBoxHit(bar,rectangle,0,40),'in');assert.equal(Selection.partForBoxHit(bar,rectangle,60,100),'out');
 const left={getBoundingClientRect:()=>({left:0,right:4})},right={getBoundingClientRect:()=>({left:96,right:100})};
 const handles={querySelector:s=>s.endsWith('.left')?left:right};
 assert.equal(Selection.partForBoxHit(handles,rectangle,10,10),'in','exact six-pixel handle tolerance');
 assert.equal(Selection.partForBoxHit(handles,rectangle,10.01,89.99),null,'middle alone selects neither edge');
 assert.equal(Selection.partForBoxHit(handles,rectangle,-1,101),'both');
 const layerSource=fs.readFileSync(path.join(__dirname,'../src/js/layer-inout.js'),'utf8');
 const a=layerSource.indexOf('  function partForBoxHit('),b=layerSource.indexOf('  function applyMarqueeSelection(',a);
 const context=vm.createContext({window:{NemoLayerInOutSelection:Selection}});vm.runInContext(layerSource.slice(a,b),context);
 assert.equal(context.partForBoxHit(handles,rectangle,10,10),'in','real legacy delegate works with no native helper API');
});

test('captured native marquee fences UI frame/FC/owner changes without old selection fallback',async()=>{
 for(const drift of [h=>h.root.state.currentFrame++,h=>h.root.FC++,h=>h.root.state.activeSymbolId='nested',h=>{h.root.NemoNativeObjectProject={desktop:()=>null};}]){
  const c=await captured();c.down(c.h.rows()[0]);drift(c.h);c.h.doc.fire('mousemove',{clientX:115,clientY:43});
  assert.equal(c.legacyStarts(),0);assert.deepEqual(c.h.root.SMLayerInOut.getBarSelection(),[]);assert.equal(c.h.commands().length,0);
 }
});


test('actual Motion capture uses the same rectangle for real inspector property keys and native bar handles',async()=>{
 const c=await captured({propertyKeys:[9,10,11]}),h=c.h;
 c.down(h.keyNodes()[1],{clientX:105});assert.deepEqual(h.keyEditor.captureSelectedKeys().keys.map(k=>k.frame),[10]);
 assert.equal(c.legacyStarts(),0);assert.equal(h.commands().length,0);
 h.keyNodes().forEach(n=>{const x=(Number(n.dataset.frame)+0.5)*h.root.FC;n.getBoundingClientRect=()=>({left:x-4.5,top:6,width:9,height:9});});
 c.down(c.wrap,{clientX:95,clientY:0});h.doc.fire('mousemove',{clientX:115,clientY:43});h.doc.fire('mouseup',{clientX:115,clientY:43});
 assert.deepEqual(h.keyEditor.captureSelectedKeys().keys.map(k=>k.frame),[9,10,11]);
 assert.deepEqual(h.root.SMLayerInOut.getBarSelection(),[{li:0,part:'in'},{li:1,part:'out'}]);
 assert.equal(c.legacyStarts(),0);assert.equal(h.legacy(),0);assert.equal(h.commands().length,0);
 c.down(h.bar(0).querySelector('.layer-inout-handle.left'),{clientX:100});h.move(120);h.up(120);await h.settle();
 assert.deepEqual(h.commands()[0].q.payload.selectedKeys.map(k=>k.frame),[9,10,11]);
});
