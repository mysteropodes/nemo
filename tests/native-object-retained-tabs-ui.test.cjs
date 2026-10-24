'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const Entry=require('../src/js/adapters/native-object-project-entry-contract.js');
const UI=require('../src/js/adapters/native-object-project-tabs-ui.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const clone=value=>JSON.parse(JSON.stringify(value));
function target(){return {kind:'native-object-inactive',instanceId:'instance',documentId:'native-B',ownerDocumentId:'native-A',
  lifecycleGeneration:4,contentRevision:7,documentSnapshotId:'B-snapshot-7',preparationToken:'B-token',digest:'b'.repeat(64),
  totalFrames:8,layerUids:['B0','B1'],view:{frame:6,layerUids:['B1'],activeLayerUid:'B1',anchorLayerUid:'B0'},
  path:'/PRIVATE/DO-NOT-SEND.json',name:'Private UI name'};}
const previous={apiVersion:2,instanceId:'instance',documentId:'native-A',contentRevision:3,
  documentSnapshotId:'A-snapshot-3',lifecycleGeneration:4,documentDigest:'a'.repeat(64)};
function retainedReceipt(){return {apiVersion:2,requestId:'activate',instanceId:'instance',documentId:'native-B',
  contentRevision:7,documentSnapshotId:'B-snapshot-7',lifecycleGeneration:5,viewportAvailable:false,retryExecution:false,
  inactiveProject:{documentId:'native-A',contentRevision:3,documentSnapshotId:'A-snapshot-3',
    preparationToken:'parked-A',preparationRequestId:'activate',documentDigest:'a'.repeat(64)}};}

test('retained intent is a detached immutable native-pin/view value and never forwards UI filenames',()=>{
  const original=target(),pinned=Entry.target(original);original.documentId='foreign';original.view.frame=0;original.view.layerUids[0]='foreign';
  assert.equal(pinned.documentId,'native-B');assert.equal(pinned.view.frame,6);assert.deepEqual(pinned.view.layerUids,['B1']);
  assert.throws(()=>{pinned.view.frame=1;},TypeError);assert.throws(()=>pinned.view.layerUids.push('B0'),TypeError);
  const request=Entry.request({retainedProject:pinned},previous,previous,'activate');
  assert.deepEqual(request,{apiVersion:2,requestId:'activate',instanceId:'instance',documentId:'native-A',expectedRevision:3,
    documentSnapshotId:'A-snapshot-3',lifecycleGeneration:4,action:{kind:'activate',preparationToken:'B-token',
      targetDocumentId:'native-B',targetRevision:7,targetSnapshotId:'B-snapshot-7'}});
  assert.doesNotMatch(JSON.stringify(request),/PRIVATE|Private UI|documentJson|layerUids/);
});

test('New and Open keep revision-zero/resource/fresh-document rules while only retained activation accepts revision seven',()=>{
  const ordinary={apiVersion:2,requestId:'open',instanceId:'instance',documentId:'native-C',contentRevision:0,resourceCount:1,viewportAvailable:false};
  assert.equal(Entry.receipt(ordinary,previous,previous,'open',null),0);
  for(const change of [{contentRevision:7},{resourceCount:2},{resourceCount:-1},{documentId:'native-A'},
    {viewportAvailable:true},{requestId:'foreign'},{instanceId:'foreign'}]){
    assert.throws(()=>Entry.receipt({...ordinary,...change},previous,previous,'open',null));
  }
  const retained=Entry.target(target()),receipt=retainedReceipt();
  assert.equal(Entry.receipt(receipt,previous,previous,'activate',retained),7);
  assert.equal(Object.isFrozen(receipt),true);assert.equal(Object.isFrozen(receipt.inactiveProject),true);
  const installed={apiVersion:2,available:true,instanceId:'instance',documentId:'native-B',contentRevision:7,lifecycleGeneration:5};
  Entry.installed(installed,previous,previous,receipt,7,retained);
  assert.throws(()=>Entry.installed({...installed,lifecycleGeneration:6},previous,previous,receipt,7,retained));
  assert.throws(()=>Entry.installed({...installed,contentRevision:0},previous,previous,receipt,7,retained));
  assert.throws(()=>Entry.candidate({canvasW:100,canvasH:100,fps:24,totalFrames:8},
    {newProject:{canvasW:100,canvasH:100,fps:24}},{resourceCount:1}));
});

test('retained receipts reject stale target/outgoing pins, unknown fields and invalid native view context',()=>{
  for(const mutate of [r=>{r.contentRevision=0;},r=>{r.documentSnapshotId='stale';},r=>{r.documentId='foreign';},
    r=>{r.inactiveProject.contentRevision=2;},r=>{r.inactiveProject.documentSnapshotId='stale';},
    r=>{r.inactiveProject.preparationRequestId='foreign';},r=>{r.inactiveProject.documentDigest='invalid';},r=>{r.inactiveProject.documentDigest='c'.repeat(64);},
    r=>{r.extra='unaccepted';},r=>{r.inactiveProject.extra='unaccepted';},r=>{r.retryExecution=true;}]){
    const receipt=retainedReceipt();mutate(receipt);assert.throws(()=>Entry.receipt(receipt,previous,previous,'activate',target()));
  }
  for(const mutate of [t=>{t.view.frame=8;},t=>{t.view.layerUids=['unknown'];},t=>{t.view.layerUids=['B1','B1'];},
    t=>{t.view.activeLayerUid='unknown';},t=>{t.view.anchorLayerUid='unknown';},t=>{t.layerUids=['B0','B0'];}]){
    const intent=target();mutate(intent);assert.throws(()=>Entry.target(intent));
  }
});

test('view projection validates every native layer identity before changing the previous UI',()=>{
  const root={state:{marker:'previous'},_layerSel:[9],_curFrame:9,_layerSelAnchor:9,selectedPaths:['old']};
  const info={instanceId:'instance',documentId:'native-B',lifecycleGeneration:5,contentRevision:7,
    documentSnapshotId:'B-snapshot-7',canvasW:640,canvasH:360,fps:30,totalFrames:8,
    layers:[{layerUid:'B0',name:'one'},{layerUid:'B1',name:'two'}]};
  const before=clone(root);
  for(const view of [{frame:8,layerUids:['B0'],activeLayerUid:'B0',anchorLayerUid:null},
    {frame:6,layerUids:['missing'],activeLayerUid:'B0',anchorLayerUid:null},
    {frame:6,layerUids:['B1'],activeLayerUid:'missing',anchorLayerUid:null}]){
    assert.throws(()=>Files.projectView(root,info,view));assert.deepEqual(root,before);
  }
  Files.projectView(root,info,target().view);assert.equal(root.state.currentFrame,6);assert.equal(root.state.activeLayerIdx,1);
  assert.deepEqual(root._layerSel,[1]);assert.equal(root._layerSelAnchor,0);assert.deepEqual(root.selectedPaths,[]);
  assert.equal(Files.projectionReadyForUI(root,info),true);
});

class Element {
  constructor(tag){this.tagName=tag;this.children=[];this.events={};this.attributes={};this.dataset={};this.className='';
    this.classList={add:name=>this.className+=' '+name,toggle:(name,on)=>{
      const set=new Set(this.className.split(/\s+/).filter(Boolean));on?set.add(name):set.delete(name);this.className=[...set].join(' ');
    }};}
  set innerHTML(value){assert.equal(value,'');this.children=[];}
  appendChild(child){this.children.push(child);child.parentNode=this;return child;}
  setAttribute(name,value){this.attributes[name]=value;}
  getAttribute(name){return this.attributes[name];}
  addEventListener(name,fn){this.events[name]=fn;}
  fire(name,event={}){return this.events[name]&&this.events[name](event);}
  replaceWith(other){const at=this.parentNode.children.indexOf(this);this.parentNode.children[at]=other;other.parentNode=this.parentNode;}
  focus(){this.focused=true;}select(){this.selected=true;}blur(){this.fire('blur');}
}
function presentation(){
  const list=new Element('div'),add=new Element('button');
  const current={activeTabId:'A',tabs:[{id:'A',name:'First',dirty:true},{id:'B',name:'Second',dirty:false}]};
  const document={getElementById:id=>id==='project-tabs-list'?list:id==='project-tab-add'?add:null,createElement:tag=>new Element(tag),
    querySelector:selector=>{
      const match=/data-tab="([^"]+)"/.exec(selector),row=match&&list.children.find(el=>el.dataset.tab===match[1]);
      return selector.includes('.pt-name')?row&&row.children.find(el=>el.className==='pt-name'):row;
    }};
  const root={document,SM:{t:key=>'translated:'+key}};
  vm.runInNewContext(fs.readFileSync(require.resolve('../src/js/project-entry.js'),'utf8'),{window:root});
  const calls=[];
  const ports={read:()=>current,native:()=>true,dirty:t=>t.dirty,switchTab:id=>calls.push(['switch',id]),
    closeTab:id=>calls.push(['close',id]),renameTab:id=>calls.push(['rename',id])};
  const render=()=>UI.render(root,ports);render();return {root,current,list,add,calls,render,ports};
}

test('extracted existing tab renderer keeps ARIA, dirty dots, click, close and rename callbacks',()=>{
  const f=presentation();assert.equal(f.list.getAttribute('role'),'tablist');assert.equal(f.list.getAttribute('aria-label'),'Project tabs');
  assert.equal(f.add.title,'translated:newProjectHdr');assert.equal(f.list.children.length,2);
  const [a,b]=f.list.children;assert.equal(a.getAttribute('aria-selected'),'true');assert.equal(b.getAttribute('aria-selected'),'false');
  assert.equal(a.className.includes('dirty'),true);assert.equal(b.className.includes('dirty'),false);
  b.fire('click');let stopped=0;b.children.at(-1).fire('click',{stopPropagation(){stopped++;}});
  a.fire('dblclick',{stopPropagation(){stopped++;}});assert.equal(stopped,2);
  assert.deepEqual(f.calls,[['switch','B'],['close','B'],['rename','A']]);
  const empty={document:{getElementById:()=>null},SMProjectEntry:f.root.SMProjectEntry};
  UI.render(empty,{native:()=>false,read:()=>{throw Error('missing view must not read tab state');}});
});

test('extracted ordinary rename preserves active identity and Escape/Enter behavior; dirty refresh stays lazy',()=>{
  const f=presentation();let activeName='First',renders=0;
  const ports={read:()=>f.current,renameActive:name=>{activeName=name;},render:()=>{renders++;f.render();}};
  UI.rename(f.root,'A',ports);let input=f.list.children[0].children.find(el=>el.tagName==='input');
  assert.equal(input.focused,true);assert.equal(input.selected,true);input.value='  Renamed A  ';
  input.fire('keydown',{key:'Enter',stopPropagation(){}});assert.equal(activeName,'Renamed A');assert.equal(f.current.tabs[0].name,'Renamed A');
  UI.rename(f.root,'B',ports);input=f.list.children[1].children.find(el=>el.tagName==='input');input.value='Discard';
  input.fire('keydown',{key:'Escape',stopPropagation(){}});assert.equal(f.current.tabs[1].name,'Second');assert.equal(activeName,'Renamed A');
  assert.equal(renders,2);let reads=0;UI.refreshDirty(f.root,'A',()=>{reads++;return false;});
  assert.equal(f.list.children[0].className.includes('dirty'),false);assert.equal(reads,1);
  UI.refreshDirty(f.root,'missing',()=>{throw Error('missing active row must not read/serialize dirty state');});
});
