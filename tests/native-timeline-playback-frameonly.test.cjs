'use strict';
// #2371: playback and scrub ticks (only the frame moved) must not rebuild the native timeline grid, its bars, its key buttons or the ruler strip.
const test=require('node:test'),assert=require('node:assert/strict');
const Range=require('../src/js/adapters/native-layer-range-desktop.js'),Cells=require('../src/js/adapters/native-frame-cell-selection.js');
const turn=()=>new Promise(setImmediate);
function node(counter){const n={children:[],style:{},dataset:{},attributes:{},textContent:'',className:'',hidden:false,parentNode:null,events:{},
  classList:{toggle(){},add(){},remove(){}},
  addEventListener(k,f){(n.events[k]=n.events[k]||[]).push(f);},removeEventListener(){},
  appendChild(c){if(c.parentNode)c.parentNode.removeChild(c);n.children.push(c);c.parentNode=n;return c;},
  removeChild(c){n.children.splice(n.children.indexOf(c),1);c.parentNode=null;},
  replaceChildren(...v){n.children.forEach(c=>c.parentNode=null);n.children=[];v.forEach(c=>n.appendChild(c));},
  contains(c){while(c){if(c===n)return true;c=c.parentNode;}return false;},setAttribute(k,v){n.attributes[k]=String(v);},
  querySelector(){return null;},getBoundingClientRect:()=>({left:0,top:0,width:400,height:20}),focus(){}};
  if(counter)counter.created++;return n;}
function fixture(){
  const counter={created:0,reads:0,bars:0},doc=node(),grid=node(),header=node(),caption=node(),wrap=node();
  doc.body=node();wrap.clientWidth=400;wrap.scrollLeft=0;header.appendChild(caption);
  doc.getElementById=id=>id==='frame-hdr'?header:id==='fg-wrap'?wrap:null;doc.createElement=()=>node(counter);doc.addEventListener=()=>{};
  const holder={info:{instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:3,documentSnapshotId:'s3',totalFrames:12,layers:[{layerUid:'a'},{layerUid:'b'}]}},info=holder.info;
  const root={document:doc,FC:20,state:{currentFrame:0,appMode:'animation',activeLayerIdx:0,totalFrames:12,activeSymbolId:null,activeCompositionId:null},_layerSel:[0],_sel:{frames:[]},addEventListener(){}};
  const session={isActive:()=>true,info:()=>holder.info,
    readPresentedObjectSession:()=>({...holder.info,available:true,presentation:{...holder.info,owner:'native',status:'presented',frame:root.state.currentFrame}}),
    async readLayerRange(uid,frame){counter.reads++;return {kind:'layerRange',instanceId:'i',documentId:'d',atRevision:3,documentSnapshotId:'s3',contextId:'scene-root',layerUid:uid,frame,
      range:{rawInPoint:1,rawOutPoint:8,inPoint:1,outPoint:8,hasCustomRange:true,hasTimeLink:false}};}};
  root.NemoNativeObjectProject={desktop:()=>session};root.NemoNativeObjectProjectFile={projectionReadyForUI:()=>true};
  const barUi={getBarSelection:()=>[],setBarSelection(){},marqueeSelect(){},onDown(){},buildBar(row){counter.bars++;const bar=node();bar.className='layer-inout-bar';row.appendChild(bar);return true;}};
  return {root,grid,header,caption,get info(){return holder.info;},set info(v){holder.info=v;},counter,session,barUi,ports:{barUi}};
}
function tick(f,frame,frameOnly=true){
  f.root.state.currentFrame=frame;
  Range.mount(f.root,f.grid,f.info,f.ports);
  f.caption.textContent='Frame '+(frame+1)+' / 12';
  Cells.mount(f.root,f.caption,f.info,{currentSession:()=>f.session,projectionReady:()=>true,frameEditPending:()=>false,frameOnly});
}
test('N frame-only ticks keep the grid rows, bars and ruler strip: 0 rebuilds, N playhead-label updates, reads only once',async()=>{
  const f=fixture();
  tick(f,0,false);await turn();
  const rows=f.grid.children[0],rowNodes=rows.children.slice(),strip=f.caption.children[0],cellNodes=strip.children[1].children.slice();
  assert.equal(rowNodes.length,2);assert.equal(f.counter.reads,2,'one range read per row at the first mount');
  assert.ok(rowNodes.every(r=>r.nativeRange),'bars painted from the first read');
  const created=f.counter.created,bars=f.counter.bars,labels=[];
  for(let frame=1;frame<=10;frame++){
    tick(f,frame);await turn();
    assert.equal(f.grid.children[0],rows,'same row container at frame '+frame);
    assert.deepEqual(f.grid.children[0].children,rowNodes,'same row nodes');
    assert.ok(rowNodes.every(r=>r.nativeRange),'bars stay painted');
    assert.equal(f.caption.children[0],strip,'same ruler strip');assert.deepEqual(strip.children[1].children,cellNodes,'same ruler cells');
    labels.push(strip.children[3].textContent);
  }
  assert.equal(f.counter.created,created,'no element is created by any playback tick');
  assert.equal(f.counter.bars,bars,'no bar is rebuilt');
  assert.equal(f.counter.reads,2,'no range is re-read');
  assert.deepEqual(labels,Array.from({length:10},(_,i)=>'Frame '+(i+2)+' / 12'),'the frame label follows every tick');
});
test('a content change (new revision pin) still rebuilds the rows, bars and strip',async()=>{
  const f=fixture();tick(f,0,false);await turn();
  const rows=f.grid.children[0],strip=f.caption.children[0];
  f.info={...f.info,contentRevision:4,documentSnapshotId:'s4'};
  tick(f,1,false);
  assert.notEqual(f.grid.children[0],rows);assert.notEqual(f.caption.children[0],strip);
});
test('a row whose range never arrived is asked again on the next frame, others are not',async()=>{
  const f=fixture();let failFirst=true;const read=f.session.readLayerRange;
  f.session.readLayerRange=async(uid,frame,info)=>{if(failFirst&&uid==='b'){f.counter.reads++;return null;}return read(uid,frame,info);};
  tick(f,0,false);await turn();failFirst=false;
  const rows=f.grid.children[0];assert.ok(!rows.children[1].nativeRange);const before=f.counter.reads;
  tick(f,1);await turn();
  assert.equal(f.grid.children[0],rows);assert.equal(f.counter.reads,before+1,'only the unpainted row is re-read');assert.ok(rows.children[1].nativeRange);
});
test('without frameOnly the ruler strip is rebuilt as before (full renders keep their semantics)',async()=>{
  const f=fixture();tick(f,0,false);await turn();const strip=f.caption.children[0];
  tick(f,1,false);assert.notEqual(f.caption.children[0],strip);
});
test('Motion key buttons survive frame-only ticks and read the live projection; a selection change rebuilds them',async()=>{
  const f=fixture();f.root.state.appMode='motion';let selected=false,selectCalls=0;
  const view=()=>({pin:f.info,frame:f.root.state.currentFrame,viewGeneration:f.root.state.currentFrame,isCurrent:()=>true,
    selectKeys:keys=>{selectCalls++;f.last=keys;return true;},
    tracks:[{target:{contextId:'scene-root',layerUid:'a'},property:'position',items:[{frame:2,selected},{frame:6,selected:false}]}]});
  f.ports.readMotionKeyTimelines=view;f.ports.setKeyTimelineRows=()=>true;
  tick(f,0,false);await turn();
  const row=f.grid.children[0].children[0],keys=row.children.filter(c=>c.className.includes('motion-key'));
  assert.equal(keys.length,2);
  const created=f.counter.created;
  for(let frame=1;frame<=8;frame++){tick(f,frame);assert.deepEqual(row.children.filter(c=>c.className.includes('motion-key')),keys,'same key buttons at frame '+frame);}
  assert.equal(f.counter.created,created,'no key button is rebuilt by a tick');
  keys[0].events.mousedown[0]({preventDefault(){},stopPropagation(){},button:0});
  assert.equal(selectCalls,1,'a surviving button still selects through the live projection');
  selected=true;tick(f,9);
  const after=row.children.filter(c=>c.className.includes('motion-key'));
  assert.equal(after.length,2);assert.notEqual(after[0],keys[0],'a selection change rebuilds the buttons');
});
