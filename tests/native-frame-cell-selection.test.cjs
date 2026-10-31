'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Selection=require('../src/js/adapters/native-frame-cell-selection.js');
const fs=require('node:fs'),vm=require('node:vm');

// BEGIN REUSABLE NATIVE CELL HEADER FIXTURE
function selectionHarness(options={}){
  function node(){const events=new Map(),n={children:[],style:{},dataset:{},attributes:{},textContent:'',hidden:false,
    addEventListener(k,f){if(!events.has(k))events.set(k,[]);events.get(k).push(f);},
    removeEventListener(k,f){events.set(k,(events.get(k)||[]).filter(v=>v!==f));},
    fire(k,e={}){for(const f of (events.get(k)||[]).slice())f({target:n,button:0,preventDefault(){},stopPropagation(){},...e});},
    appendChild(c){if(c.parentNode)c.parentNode.removeChild(c);n.children.push(c);c.parentNode=n;return c;},
    removeChild(c){n.children.splice(n.children.indexOf(c),1);c.parentNode=null;},
    replaceChildren(...values){n.children.forEach(c=>c.parentNode=null);n.children=[];values.forEach(c=>n.appendChild(c));},
    contains(c){while(c){if(c===n)return true;c=c.parentNode;}return false;},
    setAttribute(k,v){n.attributes[k]=String(v);},focus(){},
    getBoundingClientRect:()=>({left:100,top:0,width:400,height:20})};return n;}
  const doc=node(),header=node(),caption=node(),wrap=node(),cycle=node(),tasks=new Map();let serial=0;
  doc.body=node();
  cycle.disabled=options.cycleDisabled===true;cycle.title='Original Cycle';
  header.appendChild(caption);caption.textContent='Frame 1 / '+(options.total||6);wrap.clientWidth=400;wrap.scrollLeft=0;
  doc.getElementById=id=>id==='frame-hdr'?header:id==='fg-wrap'?wrap:id==='btn-cycle'?cycle:null;doc.createElement=node;
  let info={instanceId:'instance',documentId:'document',lifecycleGeneration:1,contentRevision:0,
    documentSnapshotId:'snapshot-0',totalFrames:options.total||6,layers:[{layerUid:'alpha'},{layerUid:'beta'}]};
  const root=node();Object.assign(root,{document:doc,FC:20,state:{currentFrame:0,appMode:'motion',activeSymbolId:null,activeCompositionId:null},
    _layerSel:[0,1],_sel:{frames:[],clipboard:{retain:'clipboard'},clipOp:'copy'},
    setTimeout(f){tasks.set(++serial,f);return serial;},clearTimeout(i){tasks.delete(i);}});
  let shown={...info,owner:'native',status:'presented',frame:0},available=true,active=true;
  let hidden=false;
  const session={isActive:()=>active,info:()=>info,readPresentedObjectSession:(refused)=>{
    if(hidden){if(typeof refused==='function')refused('queue-pending');return null;}
    return available?{...info,available:true,presentation:shown}:null;}};
  const ports={currentSession:()=>session,projectionReady:()=>true,frameEditPending:()=>false};
  if(options.wait){const slice=root.setTimeout,toasts=root.toasts=[];root.NemoNativePresentationWait=require('../src/js/adapters/native-presentation-wait.js');
    root.showToast=t=>toasts.push(t);root.setTimeout=(f,ms)=>ms>0?(setImmediate(f),0):slice(f,ms);}
  function mount(){Selection.mount(root,caption,info,ports);return caption.children[0];}
  let strip=mount();if(options.selectionMode!==false)strip.children[4].fire('click');
  return {root,doc,header,caption,wrap,cycle,ports,session,node,info:()=>info,strip:()=>strip,
    mount(){strip=mount();return strip;},down(frame,extra={}){strip.fire('mousedown',{clientX:100+(frame+.5)*root.FC,...extra});},
    move(frame){doc.fire('mousemove',{clientX:100+(frame+.5)*root.FC});},up(frame){doc.fire('mouseup',{clientX:100+(frame+.5)*root.FC});},
    settle(){let count=0;while(tasks.size){assert.ok(count++<100);const [id,f]=tasks.entries().next().value;tasks.delete(id);f();}},
    tasks:()=>tasks.size,changeInfo(change){info={...info,...change};},setShown(v){shown=v;},setAvailable(v){available=v;},setActive(v){active=v;},setHidden(v){hidden=v;}};
}
// END REUSABLE NATIVE CELL HEADER FIXTURE

test('native header drag publishes all rectangular cells and visible one-based labels without touching clipboard',()=>{
  const h=selectionHarness(),selection=h.root._sel,clip=selection.clipboard;h.root._layerActiveExplicit=true;
  h.down(0);h.move(1);assert.deepEqual(selection.frames,[]);
  assert.equal(h.strip().children[0].style.width,'40px');
  assert.equal(h.strip().children[1].children[0].textContent,'1');
  assert.equal(h.strip().children[1].children[1].textContent,'2');
  h.up(1);
  assert.deepEqual(selection.frames,[{layer:0,frame:0},{layer:0,frame:1},{layer:1,frame:0},{layer:1,frame:1}]);
  assert.equal(h.root._sel,selection);assert.equal(selection.clipboard,clip);assert.equal(selection.clipOp,'copy');assert.equal(h.root._layerActiveExplicit,false);
  assert.match(h.strip().attributes['aria-label'],/Selected frames 1 through 2/);
  assert.equal(h.info().contentRevision,0);
});
test('single click and Shift-click extend the actual selected frame interval across chosen layer bounds',()=>{
  const h=selectionHarness();h.root._layerSel=[1];h.down(1);h.up(1);
  assert.deepEqual(h.root._sel.frames,[{layer:1,frame:1}]);
  h.down(3,{shiftKey:true});h.up(3);
  assert.deepEqual(h.root._sel.frames,[{layer:1,frame:1},{layer:1,frame:2},{layer:1,frame:3}]);
});
test('Escape, focus loss and detached or stale gestures preserve the exact prior cell array',()=>{
  for(const fault of ['escape','blur','node-blur','owner','revision','snapshot','generation','presentation','frame','FC','rows','detached','mode','unavailable']){
    const h=selectionHarness(),prior=[{layer:0,frame:4}];h.root._sel.frames=prior;h.root._layerActiveExplicit=true;h.down(0);
    if(fault==='escape')h.doc.fire('keydown',{key:'Escape'});
    if(fault==='blur')h.root.fire('blur');if(fault==='node-blur')h.strip().fire('blur');
    if(fault==='owner')h.changeInfo({documentId:'foreign'});
    if(fault==='revision')h.changeInfo({contentRevision:1});
    if(fault==='snapshot')h.changeInfo({documentSnapshotId:'other'});
    if(fault==='generation')h.changeInfo({lifecycleGeneration:2});
    if(fault==='presentation')h.setShown({...h.info(),owner:'native',status:'presented',frame:0});
    if(fault==='frame')h.root.state.currentFrame=1;if(fault==='FC')h.root.FC=21;
    if(fault==='rows')h.root._layerSel=[1];if(fault==='detached')h.header.replaceChildren();
    if(fault==='mode')h.root.state.appMode='animation';if(fault==='unavailable')h.setAvailable(false);
    h.up(1);h.settle();assert.equal(h.root._sel.frames,prior,fault);assert.equal(h.root._layerActiveExplicit,true,fault);assert.equal(h.info().contentRevision,fault==='revision'?1:0);
  }
});
test('selection above clipboard size builds in cancellable detached chunks without a new Repeat cap',()=>{
  const h=selectionHarness({total:5000}),prior=h.root._sel.frames;h.root._layerActiveExplicit=true;
  h.down(0);h.up(2499);assert.equal(h.root._sel.frames,prior);assert.equal(h.root._layerActiveExplicit,true);assert.ok(h.tasks()>0);
  assert.equal(h.strip().attributes['aria-busy'],'true');assert.match(h.strip().children[2].textContent,/Escape cancels/);
  h.settle();assert.equal(h.root._sel.frames.length,5000);assert.equal(h.root._layerActiveExplicit,false);assert.deepEqual(h.root._sel.frames[4999],{layer:1,frame:2499});
  const keep=h.root._sel.frames;h.down(0);h.up(4999);h.root.fire('blur');h.settle();assert.equal(h.root._sel.frames,keep);
});
test('scroll, invalid coordinates and retired event listeners cannot publish stale selection',()=>{
  const h=selectionHarness(),prior=h.root._sel.frames;h.down(0);h.wrap.fire('scroll');h.up(1);assert.equal(h.root._sel.frames,prior);
  h.down(-1);h.up(1);assert.equal(h.root._sel.frames,prior);
  h.down(0,{clientX:NaN});h.up(1);assert.equal(h.root._sel.frames,prior);
  const old=h.strip();h.mount();old.fire('mousedown',{clientX:110,shiftKey:true});h.up(1);assert.equal(h.root._sel.frames,prior);
  h.down(0);Selection.retire(h.root);h.up(1);assert.equal(h.root._sel.frames,prior);
});

test('actual parent ruler scrub receives normal input while selection mode and its button stay local',()=>{
  const h=selectionHarness({selectionMode:false}),scrub=[],source=fs.readFileSync(require.resolve('../src/js/ui.js'),'utf8');
  const start=source.indexOf("  document.getElementById('frame-hdr').addEventListener('mousedown'");
  const end=source.indexOf('  // Playhead flag handle',start);assert.ok(start>=0&&end>start);
  h.root._totalF=6;h.root.SM={stopPlay(){},goToFrame:f=>scrub.push(f)};
  vm.runInNewContext(source.slice(start,end),{window:h.root,document:h.doc,FC:h.root.FC,scrubbing:false});
  function down(shiftKey){let stopped=false;const e={button:0,clientX:130,shiftKey,
    preventDefault(){},stopPropagation(){stopped=true;}};h.strip().fire('mousedown',e);if(!stopped)h.header.fire('mousedown',e);}
  assert.equal(h.strip().children[4].attributes['aria-pressed'],'false');
  down(false);assert.deepEqual(scrub,[1]);assert.deepEqual(h.root._sel.frames,[]);
  let stopped=false;h.strip().children[4].fire('mousedown',{stopPropagation(){stopped=true;}});assert.equal(stopped,true);
  h.strip().children[4].fire('click');down(false);h.up(1);assert.deepEqual(scrub,[1]);assert.equal(h.root._sel.frames.length,2);
  assert.match(h.strip().attributes['aria-label'],/Frame selection mode/);
  h.strip().children[4].fire('click');down(false);assert.deepEqual(scrub,[1,1]);
  assert.equal(h.strip().children[3].textContent,'Frame 1 / 6');
});

test('range followed by new component row and plain selection click starts singleton frame zero',()=>{
  const h=selectionHarness({total:3});h.down(0);h.move(1);h.up(1);assert.equal(h.root._sel.frames.length,4);
  h.root._layerSel=[1];h.mount();h.down(0);h.up(0);
  assert.deepEqual(h.root._sel.frames,[{layer:1,frame:0}]);
  assert.equal(h.caption.style.minWidth,'100%');assert.equal(h.strip().style.width,'400px');
  assert.equal(h.strip().children[1].children[2].style.left,'40px');
});

test('selection mode exits on owner revision changes and blur without discarding selected cells',()=>{
  const h=selectionHarness();h.down(1);h.up(1);const cells=h.root._sel.frames;
  h.root.fire('blur');assert.equal(h.strip().children[4].attributes['aria-pressed'],'false');assert.equal(h.root._sel.frames,cells);
  h.strip().children[4].fire('click');assert.equal(h.strip().children[4].attributes['aria-pressed'],'true');
  h.changeInfo({contentRevision:1,documentSnapshotId:'snapshot-1'});h.mount();
  assert.equal(h.strip().children[4].attributes['aria-pressed'],'false');assert.equal(h.root._sel.frames,cells);
});

test('actual metadata presenter mounts and retires the helper after caption refresh without changing Motion grid nodes',()=>{
  const source=fs.readFileSync(require.resolve('./native-object-ui.test.cjs'),'utf8'),end=source.indexOf("test('");
  const make=vm.compileFunction(source.slice(0,end)+'\nreturn {view,fixturePorts,metadata,UI};',['require'])(require);
  const root=make.view(),selection=root._sel={frames:[],clipboard:{keep:true}};root.FC=20;let retired=0;
  root.NemoNativeFrameCellSelection={mount:Selection.mount,retire(r){retired++;Selection.retire(r);}};
  const info=make.metadata();make.UI.render(root,info,false,make.fixturePorts(root));
  const header=root.document.nodes.get('frame-hdr'),caption=header.children[0],grid=root.document.nodes.get('frame-grid').children[0],old=caption.children[0];
  assert.equal(header.children.length,1);assert.equal(old.className,'native-frame-selection-strip');
  make.UI.render(root,info,true,make.fixturePorts(root));assert.equal(header.children[0],caption);assert.notEqual(caption.children[0],old);
  assert.equal(root.document.nodes.get('frame-grid').children[0],grid);assert.equal(root._sel,selection);assert.deepEqual(selection.frames,[]);
  make.UI.restoreInspector(root);assert.equal(retired,1);
});

test('capture/build pending disables Cycle and restores its exact earlier state on cancellation or completion',()=>{
  for(const disabled of [false,true]){
    const h=selectionHarness({total:2048,cycleDisabled:disabled}),prior=h.root._sel.frames;
    h.down(0);assert.equal(Selection.isPending(h.root),true);assert.equal(h.cycle.disabled,true);
    h.up(1024);assert.equal(h.root._sel.frames,prior);assert.ok(h.tasks()>0);
    h.root.fire('blur');h.settle();assert.equal(Selection.isPending(h.root),false);assert.equal(h.cycle.disabled,disabled);assert.equal(h.cycle.title,'Original Cycle');
    h.strip().children[4].fire('click');h.down(0);h.up(1024);h.settle();assert.equal(h.root._sel.frames.length,2050);assert.equal(h.cycle.disabled,disabled);
  }
});
test('missing/throwing pending port refuses capture and retired attached nodes neither suppress input nor affect successor',()=>{
  for(const port of [undefined,()=>{throw Error('pending unavailable');},()=>true]){
    const h=selectionHarness(),prior=h.root._sel.frames;h.ports.frameEditPending=port;h.down(0);h.up(1);assert.equal(h.root._sel.frames,prior);assert.equal(Selection.isPending(h.root),false);
  }
  const h=selectionHarness(),old=h.strip(),button=old.children[4];Selection.retire(h.root);
  let stopped=false;const event={clientX:110,stopPropagation(){stopped=true;}};
  button.fire('mousedown',event);button.fire('click',event);old.fire('mousedown',event);assert.equal(stopped,false);assert.equal(Selection.isPending(h.root),false);
  h.mount();h.strip().children[4].fire('click');h.down(0);old.fire('mousedown',event);assert.equal(Selection.isPending(h.root),true);h.up(1);assert.equal(h.root._sel.frames.length,4);
});


test('shipped Cmd+D after native header selection reaches the wrapped frame duplicate; next native row restores layer intent',async()=>{
  const desktopSource=fs.readFileSync(require.resolve('./native-frame-edit-desktop.test.cjs'),'utf8');
  const make=vm.compileFunction(desktopSource.slice(0,desktopSource.indexOf("test('"))+'\nreturn harness;',['require'])(require);
  const Desktop=require('../src/js/adapters/native-frame-edit-desktop.js');
  const Surface=require('../src/js/adapters/native-opacity-legacy-surface.js');
  const UI=require('../src/js/adapters/native-object-ui.js');
  const h=make();await h.open();const cells=selectionHarness();Selection.retire(cells.root);
  let frameLegacy=0,layerWrites=0,last;
  Object.assign(h.root,{document:cells.doc,FC:20,_layerSel:[0,1],_layerActiveExplicit:true,selectedPaths:[],
    addEventListener:cells.root.addEventListener,removeEventListener:cells.root.removeEventListener,
    NemoNativeFrameEditDesktop:Desktop,__TAURI__:{core:{invoke(){throw Error('PRIVATE WRITER');}}},
    SM:{importJSON(){},duplicateSelectedFrames(){frameLegacy++;throw Error('PAPER FRAME WRITER');},
      duplicateLayer(){layerWrites++;throw Error('LAYER WRITER');}}});
  h.root.state.appMode='animation';h.root.state.activeLayerIdx=0;h.root._sel.frames=[];
  const dispose=Surface.desktopPorts(h.root,{}).surface.wrap('timeline',[],'timeline-',()=>false,()=>{});
  const wrapped=h.root.SM.duplicateSelectedFrames;h.root.SM.duplicateSelectedFrames=()=>(last=wrapped());
  const ports={currentSession:()=>h.session,projectionReady:()=>true,frameEditPending:()=>h.controller.isPending()};
  Selection.mount(h.root,cells.caption,h.session.info(),ports);const strip=cells.caption.children[0];
  strip.children[4].fire('click');strip.fire('mousedown',{clientX:110});cells.doc.fire('mouseup',{clientX:130});
  assert.equal(h.root._sel.frames.length,4);
  const timeline=fs.readFileSync(require.resolve('../src/js/timeline.js'),'utf8'),start=timeline.indexOf('function onKeyDown(event){');
  const end=timeline.indexOf('function onKeyUp(event)',start);assert.ok(start>=0&&end>start);
  const context={window:h.root,state:h.root.state,_sel:h.root._sel,selectedPaths:h.root.selectedPaths,
    duplicateSelection(){throw Error('CANVAS WRITER');},NemoKeyboardArbiter:require('../src/js/application/keyboard-arbiter.js')};
  vm.runInNewContext(timeline.slice(start,end),context);
  const key={key:'d',metaKey:true,target:{tagName:'DIV'},preventDefault(){}};
  try{
    context.onKeyDown(key);
    assert.equal((await last).status,'applied-presented');
    assert.equal(h.commands().length,1);assert.equal(h.commands()[0].q.operation,'command.document.object.frames.clipboard.duplicate');
    assert.deepEqual(h.commands()[0].q.payload,{command:'object.frames.clipboard.duplicate',layerUid:'alpha',selection:[
      {layerUid:'alpha',frame:0},{layerUid:'alpha',frame:1},{layerUid:'beta',frame:0},{layerUid:'beta',frame:1}]});
    assert.equal(frameLegacy,0);assert.equal(layerWrites,0);
    // The real native row selector must reclaim row intent without changing shortcut precedence.
    Selection.retire(h.root);
    const uiSource=fs.readFileSync(require.resolve('./native-object-ui.test.cjs'),'utf8');
    const view=vm.compileFunction(uiSource.slice(0,uiSource.indexOf("test('"))+'\nreturn view;',['require'])(require)();
    h.root.document=view.document;
    const rowSession={info:h.session.info,isActive:h.session.isActive,readPresentedObjectSession:h.session.readPresentedObjectSession};
    const rowPorts={currentSession:()=>rowSession,projectionReady:()=>true,selectLayer(){},repaint(){return true;}};
    assert.equal(UI.select(h.root,h.session.info(),1,false,'beta',h.session.readPresentedObjectSession(),rowPorts),true);
    assert.equal(h.root._layerActiveExplicit,true);
    assert.throws(()=>context.onKeyDown(key),/LAYER WRITER/);
    assert.equal(layerWrites,1);assert.equal(h.commands().length,1);assert.equal(frameLegacy,0);
    // Motion retains its existing layer-duplicate route even with completed frame cells.
    h.root.state.appMode='motion';h.root._layerActiveExplicit=false;
    assert.throws(()=>context.onKeyDown(key),/LAYER WRITER/);assert.equal(layerWrites,2);assert.equal(h.commands().length,1);
  }finally{Selection.retire(h.root);dispose();}
});

// #2372: the shared queue-pending wait is wired to the ruler selection's press and its document-level move / release listeners.
const ticks=async(n=6)=>{for(let i=0;i<n;i++)await new Promise(done=>setImmediate(done));};
test('queue-pending: a ruler press held for several ticks selects once when the presentation returns, with the drag that followed it',async()=>{
  const h=selectionHarness({wait:true});h.setHidden(true);
  h.down(0);h.move(1);h.up(2);await ticks();
  assert.deepEqual(h.root._sel.frames,[],'nothing published while the presentation is hidden');
  h.setHidden(false);await ticks();
  assert.deepEqual(h.root._sel.frames.map(c=>c.frame),[0,1,2,0,1,2],'one replayed gesture: rows x frames 0..2');
  assert.deepEqual(h.root.toasts,[]);
  h.down(3);h.up(3);assert.deepEqual(h.root._sel.frames,[{layer:0,frame:3},{layer:1,frame:3}],'the next click, with the queue idle, runs straight through');
});
test('queue-pending: a gesture already under way survives the window and still publishes; a real change still ends it',async()=>{
  const h=selectionHarness({wait:true});h.down(0);h.setHidden(true);h.move(1);h.up(1);
  assert.deepEqual(h.root._sel.frames.map(c=>c.frame),[0,1,0,1],'drag and release inside the hidden window are not dropped');
  h.setHidden(false);const prior=h.root._sel.frames;h.down(0);h.setHidden(true);h.root.state.currentFrame=1;h.up(2);
  assert.equal(h.root._sel.frames,prior,'a frame change during the window ends the gesture');
  h.root.state.currentFrame=0;h.setHidden(false);h.down(0);h.setHidden(true);h.changeInfo({contentRevision:1});h.up(2);
  assert.equal(h.root._sel.frames,prior,'a pin change during the window ends the gesture');
});
test('queue-pending: a context change while the press waits drops it silently',async()=>{
  for(const change of ['frame','rows','mode','pin']){
    const h=selectionHarness({wait:true}),prior=h.root._sel.frames;h.setHidden(true);h.down(0);h.up(1);await ticks(2);
    if(change==='frame')h.root.state.currentFrame=1;if(change==='rows')h.root._layerSel=[1];if(change==='mode')h.root.state.appMode='animation';
    if(change==='pin')h.changeInfo({contentRevision:2});
    h.setHidden(false);await ticks();assert.equal(h.root._sel.frames,prior,change);assert.deepEqual(h.root.toasts,[],change);
  }
});
test('queue-pending: a queue that never frees gives ONE notice and no selection; any other refusal is not waited out',async()=>{
  const h=selectionHarness({wait:true}),prior=h.root._sel.frames;h.setHidden(true);h.down(0);h.up(1);await ticks(40);
  assert.equal(h.root._sel.frames,prior);assert.deepEqual(h.root.toasts,['The canvas is still busy - try again.']);
  const g=selectionHarness({wait:true});g.setAvailable(false);g.down(0);g.up(1);await ticks(3);
  assert.deepEqual(g.root._sel.frames,[]);assert.deepEqual(g.root.toasts,[],'an unavailable presentation is refused by the handler at once, not held');
});
