'use strict';
// Actual comment controls, Select capture callbacks, pure mapping, model and SDK.
// DOM/native metadata/presentation ports are controlled; no installed/native acceptance claim.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), vm = require('node:vm');
const UI = require('../src/js/adapters/native-comments-ui.js'), C = require('../src/js/adapters/native-comments-contract.js');
const Presentation = require('../src/js/adapters/project-comments-presentation.js');
const SDK = require('../src/js/adapters/native-application.js');
const { extractFunction } = require('./fixtures/lib/sandbox.cjs');
const copy = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const turn = () => new Promise(setImmediate);
function node() {
  const listeners = new Map(), classes = new Set();
  return { style: {}, dataset: {}, children: [], value: '', checked: false,
    classList: { add: v => classes.add(v), remove: v => classes.delete(v), contains: v => classes.has(v) },
    addEventListener(k, fn) { const list = listeners.get(k) || []; list.push(fn); listeners.set(k, list); },
    removeEventListener(k, fn) { listeners.set(k, (listeners.get(k) || []).filter(f => f !== fn)); },
    fire(k, extra = {}) { const e = { type: k, target: this, preventDefault() {}, stopPropagation() {}, stopImmediatePropagation() {}, ...extra };
      for (const fn of [...(listeners.get(k) || [])]) fn.call(this, e); return e; },
    appendChild(child) { this.children.push(child); return child; }, contains(child) { return this === child || this.children.includes(child); },
    getBoundingClientRect() { return { left: 20, top: 30, width: 640, height: 360, bottom: 390, right: 660 }; }, focus() {} };
}
async function fixture(initial, browser = false) {
  const elements = new Map(), events = node(), doc = node();
  for (const id of ['drawing-canvas','canvas-area','comment-popover','comment-author-row','comment-text','comment-resolved',
    'comment-delete','comment-save','comment-save-feedback','comment-fb-name','comment-fb-email','comment-fb-blocking',
    'comment-record','comment-record-status','comment-shot-thumbs','comment-shot-drop','comment-shot-input','fb-avatars','fb-avatars-pop']) elements.set(id,node());
  const pop = elements.get('comment-popover'); pop.style.display = 'none';
  for (const id of ['comment-text','comment-resolved','comment-save','comment-delete']) pop.children.push(elements.get(id));
  let rect = { left: 20, top: 30, width: 640, height: 360 };
  elements.get('drawing-canvas').getBoundingClientRect = () => ({ ...rect });
  elements.get('canvas-area').getBoundingClientRect = () => ({ ...rect });
  doc.readyState = 'loading'; doc.getElementById = id => elements.get(id) || null; doc.querySelectorAll = () => [];
  doc.createElement = () => node(); doc.createTextNode = text => ({ textContent: text });
  doc.documentElement = { clientWidth: 1200, clientHeight: 900, classList: { contains: n => n === 'nemo-native-underlay' },
    style: { getPropertyValue: name => String(({ left: rect.left, top: rect.top, right: rect.left+rect.width, bottom: rect.top+rect.height })[name.replace('--nemo-native-','')]) } };
  let metadata = copy(initial), revision = 0, ticket = 1, serial = 0, running = 0, tail = Promise.resolve(), gate = null, refuse = false;
  const calls = [], checkpoints = [], notices = [], navigation = []; let select = 0, text = 0, feedback = 0;
  let info = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 's0', canvasW: 320, canvasH: 180, totalFrames: 40 };
  let presentation = { ...info, owner: 'native', status: 'presented', frame: 0, contextId: 'scene-root', workId: 'w0' };
  const raw = { document: doc, state: { tool: 'comment', currentFrame: 0, appMode: 'anim2d', userProfile: { id: 'author', name: '', color: '#AbC' } },
    innerWidth: 1200, innerHeight: 900, console, setTimeout, clearTimeout, Promise,
    showToast: message => notices.push(message), updateUI() {}, saveActiveLayerFrame() { if (!browser) throw Error('legacy history'); },
    goToFrame: frame => navigation.push(frame), SM: { t: k => k }, SMFeedback: { submitFeedback() { feedback++; throw Error('unauthorized feedback'); } },
    view: { zoom: 2, projectToView: point => ({ x: point.x*2, y: point.y*2 }) },
    Point: class Point { constructor(x,y) { this.x=x;this.y=y; } getDistance(p) { return Math.hypot(this.x-p.x,this.y-p.y); } },
    addEventListener: events.addEventListener.bind(events), removeEventListener: events.removeEventListener.bind(events),
    NemoNativeCommentsUI: UI, NemoProjectCommentsPresentation: Presentation,
    NemoNativeOpacityCutover: { blocksLegacy: () => !browser }, SMEngineBridge: { renderNow() {} } };
  if (!browser) raw.__TAURI__ = { core: {} };
  if (!browser) Object.defineProperty(raw.state,'comments',{ get() { throw Error('legacy comments read'); }, set() { throw Error('legacy comments write'); } });
  else raw.state.comments = copy(initial) || [];
  raw.window = raw; const context = vm.createContext(raw), root = vm.runInContext('window',context);
  let controller;
  const shown = pin => ({ ...pin, presentation });
  async function dispatch(q) {
    calls.push(copy(q)); C.payload(q.operation,q.payload);
    let response = { apiVersion: 2, requestId: q.requestId, instanceId: info.instanceId, documentId: info.documentId, contentRevision: revision, ok: true };
    if (refuse) return { ...response, ok: false, error: { code: 'unavailable', message: 'refused' } };
    if (q.operation === C.READ) {
      const selector=q.payload.selector, items=metadata || []; let body;
      if (selector.kind==='settings') body={commentsPresent:metadata!==undefined,commentCount:items.length};
      else if (selector.kind==='commentsPage') {
        const end=Math.min(items.length,selector.cursor+selector.limit);
        body={kind:'projectCommentsPage',commentsPresent:metadata!==undefined,commentCount:items.length,cursor:selector.cursor,
          items:items.slice(selector.cursor,end).map((comment,i)=>({ordinal:selector.cursor+i+1,comment:copy(comment)})),nextCursor:end<items.length?end:null};
      } else body={kind:'projectCommentRecord',ordinal:selector.ordinal,comment:copy(items[selector.ordinal-1])};
      response.result={instanceId:info.instanceId,documentId:info.documentId,atRevision:revision,documentSnapshotId:info.documentSnapshotId,contextId:'scene-root',result:body};
    } else {
      assert.equal(q.expectedRevision,revision); const next=copy(metadata)||[],action=q.payload.action;
      if(action.kind==='append')next.push(copy(action.comment));
      else if(action.kind==='update')Object.assign(next[action.ordinal-1],{text:action.text,resolved:action.resolved});
      else next.splice(action.ordinal-1,1);
      const changed=JSON.stringify(next)!==JSON.stringify(metadata);
      if(changed){checkpoints.push(copy(metadata));metadata=next;revision++;}
      response.contentRevision=revision;response.result={applied:changed,historyEntriesAdded:Number(changed)};
    }
    return response;
  }
  const adapter=SDK.createNativeApplicationAdapter('comments-control-test',{dispatch});
  const session={ info:()=>info,isActive:()=>true,requestId:()=>`comments-${++serial}`,readSubscriptionGeneration:()=>ticket,
    readPresentedObjectSession:()=>running?null:shown(info),
    runResourceCommand(fn){running++;const job=tail.then(async()=>{
      if(gate){const held=gate;gate=null;await held;}
      const pin=info;return fn({dispatch:q=>adapter.dispatch(q),readPresentedObjectSession:()=>pin===info?shown(pin):null,
        reconcile:async next=>{assert.equal(next,revision);info={...info,contentRevision:revision,documentSnapshotId:'s'+revision};
          presentation={...info,owner:'native',status:'presented',frame:root.state.currentFrame,contextId:'scene-root',workId:'w'+revision};
          controller.willProject();controller.presented();return info;}});
    }).finally(()=>{running--;});tail=job.catch(()=>{});return job;}}
  root.NemoNativeObjectProject={desktop:()=>session}; root.NemoNativeObjectProjectFile={projectionReadyForUI:()=>!running};
  root.NemoNativeTextToolDesktop={handle(_root,method){text++;return root.state.tool==='text';}};
  root.nativePointerIntent={cancel(){},snapshot(){return null;}};
  root.nativePointer=()=>({down(){select++;},move(){select++;},up(){select++;},cancelEvent(){select++;}});
  root.onDown=()=>{throw Error('legacy native pointer');};root.onMove=()=>{};root.onUp=()=>{};root.onContext=()=>{};
  const timeline=fs.readFileSync(require.resolve('../src/js/timeline.js'),'utf8');
  const start=timeline.indexOf('var _activeComment=null;'),end=timeline.indexOf('// ---- Outil texte',start);
  vm.runInContext(timeline.slice(start,end),context);
  const avatars=timeline.indexOf('function refreshFbAvatars(){');
  vm.runInContext(timeline.slice(avatars,timeline.indexOf('// ---- GitHub feedback triage dashboard',avatars)),context);
  const bridge=fs.readFileSync(require.resolve('../src/js/select-bridge.js'),'utf8');
  vm.runInContext(extractFunction(bridge,'nativePointerEvent'),context);
  root.target=elements.get('canvas-area');
  const listeners=bridge.split('\n').filter(line=>line.includes('target.addEventListener')&&line.includes('nativePointerEvent'));
  vm.runInContext(listeners.join('\n'),context);
  if(!browser)controller=UI.install(root);
  doc.fire('DOMContentLoaded');
  async function settled(){for(let i=0;i<20;i++){await tail;await turn();if(!running) return;}throw Error('comments queue unsettled');}
  if(!browser)await settled();
  const pointer=(x=100,y=100,extra={})=>root.target.fire('pointerdown',{target:elements.get('drawing-canvas'),pointerId:1,isPrimary:true,button:0,clientX:x,clientY:y,...extra});
  return {root,doc,elements,events,pointer,settled,calls,checkpoints,notices,navigation,metadata:()=>copy(metadata),counts:()=>({select,text,feedback}),
    hold(){let release;gate=new Promise(resolve=>release=resolve);return release;},refuse(value){refuse=value;},
    pan(){presentation={...presentation,workId:presentation.workId+'-pan'};},resize(){rect={...rect,left:rect.left+1};},
    replace(){info={...info,documentId:'other',lifecycleGeneration:2};ticket++;controller.willProject();},
    subscription(){ticket++;},
    revision(){revision++;info={...info,contentRevision:revision,documentSnapshotId:'s'+revision};controller.willProject();},controller};
}

test('actual pointer and original Save/Delete callbacks use native append, empty update, resolve and ordinal identity',async()=>{
  const f=await fixture([{id:'same',x:40,y:35,frame:0,text:'first'},{id:'same',x:80,y:35,frame:0},{}]);
  f.pointer(180,100);assert.equal(f.root._activeComment.id,'same');
  f.elements.get('comment-text').value='';f.elements.get('comment-resolved').checked=true;f.elements.get('comment-save').fire('click');
  await f.settled();assert.deepEqual(f.metadata()[1],{id:'same',x:80,y:35,frame:0,text:'',resolved:true});
  assert.equal(f.calls.filter(q=>q.operation===C.EDIT)[0].payload.action.ordinal,2);
  f.pointer(400,250);assert.equal(f.root._activeComment.x,190);assert.equal(f.root._activeComment.y,110);
  f.elements.get('comment-text').value='New';f.elements.get('comment-save').fire('click');await f.settled();
  assert.equal(f.metadata().length,4);assert.equal(f.metadata()[3].authorName,'');assert.equal(f.metadata()[3].authorColor,'#AbC');
  f.pointer(180,100);f.elements.get('comment-delete').fire('click');await f.settled();assert.equal(f.metadata().length,3);
  assert.deepEqual(f.metadata()[0],{id:'same',x:40,y:35,frame:0,text:'first'});assert.equal(f.checkpoints.length,3);
  assert.equal(f.counts().feedback,0);assert(Object.isFrozen(f.controller.read().comments.items[0].comment));
  const fresh=await fixture(f.metadata());assert.deepEqual(fresh.controller.read().comments.items.map(i=>i.comment),f.metadata());
});
test('blank drafts, lookalikes, queued viewport changes and refusal never mutate legacy state or resend',async()=>{
  const f=await fixture(undefined);f.pointer();f.elements.get('comment-text').value=' \n';f.elements.get('comment-save').fire('click');await f.settled();
  assert.equal(f.metadata(),undefined);assert.equal(f.checkpoints.length,0);
  assert.equal(UI.begin(f.root,{id:'forged'}),false);UI.save(f.root,{id:'forged'},'x',false);await f.settled();assert.equal(f.checkpoints.length,0);
  f.pointer();const release=f.hold();f.elements.get('comment-text').value='pending';f.elements.get('comment-save').fire('click');
  await turn();f.pan();release();await f.settled();assert.equal(f.checkpoints.length,0);
  await f.controller.presented();await f.settled();f.pointer();f.refuse(true);f.elements.get('comment-text').value='refused';f.elements.get('comment-save').fire('click');
  await f.settled();assert.equal(f.metadata(),undefined);assert.equal(f.calls.filter(q=>q.operation===C.EDIT).length,1);
  assert.equal(f.controller.read(),null);assert.equal(f.elements.get('fb-avatars').dataset.status,'unavailable');assert.equal(f.counts().feedback,0);
});
test('native mapping keeps the original 14 pixel radius, frame and all other dispatch paths',async()=>{
  const f=await fixture([{id:'same',x:40,y:35,frame:0,text:'hit'},{id:'same',x:40,y:35,frame:1,text:'other frame'}]);
  f.pointer(113.9,100);assert.equal(f.root._activeComment.text,'hit');f.root.closeCommentPopover();
  f.pointer(114,100);assert.equal(f.root._activeComment.text,'');f.root.closeCommentPopover();
  for(const retire of [()=>f.events.fire('blur'),()=>f.events.fire('keydown',{key:'Escape'}),()=>f.doc.fire('focusin',{target:node()}),()=>f.resize(),()=>f.pan(),()=>f.replace()]){
    await f.controller.presented();await f.settled();f.pointer();const draft=f.root._activeComment;retire();
    UI.save(f.root,draft,'stale',false);await f.settled();assert.equal(f.checkpoints.length,0);
  }
  f.root.state.tool='select';const before=f.counts();f.pointer();assert.equal(f.counts().select,before.select+1);
  f.root.state.tool='text';f.pointer();assert.equal(f.counts().select,before.select+1);assert(f.counts().text>before.text);
  assert.equal(f.counts().feedback,0);
});
test('browser extraction preserves world hit radius, partial avatar display and raw Save behavior',async()=>{
  const f=await fixture([{id:'same',x:40,y:35,frame:0,text:'old',authorId:'a',authorName:'Writer'}],true);
  const p=new f.root.Point(46.9,35);assert.equal(Presentation.openAt(f.root,p,2),true);assert.equal(f.root._activeComment.text,'old');
  f.elements.get('comment-text').value='';f.elements.get('comment-resolved').checked=true;f.elements.get('comment-save').fire('click');
  assert.equal(f.root.state.comments[0].text,'');assert.equal(f.root.state.comments[0].resolved,true);
  f.root.renderFbAvatarPopover();assert(f.elements.get('fb-avatars-pop').children.length);assert.equal(f.counts().feedback,0);
});
test('immutable multi-page projection preserves all raw records and refuses an oversized read as unavailable',async()=>{
  const records=Array.from({length:18},(_,i)=>i%3===0?{}:{id:'same',authorName:'',text:String(i),resolved:false});
  const f=await fixture(records);const projection=f.controller.read();
  assert.deepEqual(projection.comments.items.map(item=>item.comment),records);
  assert.deepEqual(f.calls.filter(q=>q.operation===C.READ).map(q=>q.payload.selector),[
    {kind:'settings'},{kind:'commentsPage',cursor:0,limit:16},{kind:'commentsPage',cursor:16,limit:16}]);
  assert(Object.isFrozen(projection.comments.items));
  const huge=await fixture([{text:'x'.repeat(4096)}]);assert.equal(huge.controller.read(),null);
  assert.equal(huge.elements.get('fb-avatars').dataset.status,'unavailable');assert.equal(huge.checkpoints.length,0);
  const absent=await fixture(undefined),empty=await fixture([]);
  assert.equal(absent.controller.read().comments.settings.commentsPresent,false);
  assert.equal(empty.controller.read().comments.settings.commentsPresent,true);
});

test('the existing capture listeners retire canceled pointers, frame, subscription and tab intents',async()=>{
  for(const retire of [f=>f.root.target.fire('pointercancel',{pointerId:1}),f=>f.root.target.fire('lostpointercapture',{pointerId:1}),
    f=>{f.root.state.currentFrame=1;},f=>f.subscription(),f=>f.replace(),f=>f.events.fire('pagehide')]){
    const f=await fixture([]);f.pointer();const held=f.root._activeComment;retire(f);
    UI.save(f.root,held,'retired',false);await f.settled();assert.equal(f.checkpoints.length,0);
  }
});

test('queued retirement closes the matching pending popover without a checkpoint or replay',async()=>{
  for(const retire of [f=>f.pan(),f=>{f.resize();f.events.fire('resize');},f=>f.revision(),
    f=>f.replace(),f=>f.events.fire('keydown',{key:'Escape'}),f=>f.events.fire('pagehide')]){
    const f=await fixture([]);f.pointer();const record=f.root._activeComment;
    const release=f.hold();f.elements.get('comment-text').value='queued';f.elements.get('comment-save').fire('click');
    await turn();assert.equal(f.elements.get('comment-popover').style.display,'block');
    retire(f);release();await f.settled();
    assert.equal(f.root._activeComment,null);assert.equal(f.elements.get('comment-popover').style.display,'none');
    assert.equal(f.checkpoints.length,0);assert.equal(f.calls.filter(q=>q.operation===C.EDIT).length,0);
    UI.save(f.root,record,'must not replay',false);UI.remove(f.root,record);await f.settled();
    assert.equal(f.checkpoints.length,0);assert.equal(f.calls.filter(q=>q.operation===C.EDIT).length,0);
  }
});
test('a valid pending completion keeps its one-shot proof consumed and closes only after settlement',async()=>{
  const f=await fixture([]);f.pointer();const record=f.root._activeComment;
  const release=f.hold();f.elements.get('comment-text').value='one command';f.elements.get('comment-save').fire('click');
  await turn();assert.equal(f.root._activeComment,record);assert.equal(f.elements.get('comment-popover').style.display,'block');
  f.elements.get('comment-save').fire('click');f.elements.get('comment-delete').fire('click');
  f.root.target.fire('pointermove',{pointerId:1,isPrimary:true,clientX:100,clientY:100});
  assert.equal(f.root._activeComment,record);assert.equal(f.checkpoints.length,0);
  release();await f.settled();assert.equal(f.checkpoints.length,1);
  assert.equal(f.calls.filter(q=>q.operation===C.EDIT).length,1);assert.equal(f.metadata()[0].text,'one command');
  assert.equal(f.root._activeComment,null);assert.equal(f.elements.get('comment-popover').style.display,'none');
  UI.save(f.root,record,'must not replay',false);await f.settled();assert.equal(f.checkpoints.length,1);
});
test('pending refusal retires project controls without killing an active debug recording',async()=>{
  const f=await fixture([]);f.pointer();const record=f.root._activeComment;
  const release=f.hold();f.elements.get('comment-text').value='queued';f.elements.get('comment-save').fire('click');await turn();
  f.root._recording=true;f.events.fire('keydown',{key:'Escape'});release();await f.settled();
  assert.equal(f.root._recording,true);assert.equal(f.root._activeComment,record);
  assert.equal(f.elements.get('comment-popover').style.display,'block');assert.equal(f.checkpoints.length,0);
  assert.equal(f.calls.filter(q=>q.operation===C.EDIT).length,0);assert.equal(f.counts().feedback,0);
  f.root._recording=false;f.events.fire('keydown',{key:'Escape'});
  assert.equal(f.root._activeComment,null);assert.equal(f.elements.get('comment-popover').style.display,'none');
  const refused=await fixture([]);refused.pointer();refused.refuse(true);
  refused.elements.get('comment-text').value='refused';refused.elements.get('comment-save').fire('click');await refused.settled();
  assert.equal(refused.root._activeComment,null);assert.equal(refused.elements.get('comment-popover').style.display,'none');
  assert.equal(refused.checkpoints.length,0);assert.equal(refused.calls.filter(q=>q.operation===C.EDIT).length,1);
});
