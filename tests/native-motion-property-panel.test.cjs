'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const UI=require('../src/js/adapters/native-object-ui.js'),Binding=require('../src/js/adapters/native-motion-property-binding.js'),SDK=require('../src/js/adapters/native-application.js');
const fixture=require('../engineering/application/examples/native-property-metadata-v1/cases.json');
const clone=structuredClone;
class Node{
 constructor(doc,tag){this.ownerDocument=doc;this.tagName=tag.toUpperCase();this.children=[];this.parentNode=null;this.style={};this.dataset={};this.attrs={};this.listeners=new Map();this._text='';this.value='';this.classes=new Set();this.classList={add:x=>this.classes.add(x),remove:x=>this.classes.delete(x),contains:x=>this.classes.has(x),toggle:(x,on)=>{if(on===undefined)on=!this.classes.has(x);on?this.classes.add(x):this.classes.delete(x);return on;}};}
 set textContent(v){this.replaceChildren();this._text=String(v);}get textContent(){return this._text+this.children.map(c=>c.textContent).join('');}
 appendChild(n){if(n.parentNode)n.parentNode.removeChild(n);n.parentNode=this;this.children.push(n);return n;}
 removeChild(n){const i=this.children.indexOf(n);if(i<0)throw Error('not child');this.children.splice(i,1);n.parentNode=null;return n;}
 insertBefore(n,next){if(n.parentNode)n.parentNode.removeChild(n);const i=next?this.children.indexOf(next):this.children.length;if(i<0)throw Error('missing next');this.children.splice(i,0,n);n.parentNode=this;return n;}
 replaceChildren(...nodes){this.children.forEach(n=>n.parentNode=null);this.children=[];this._text='';nodes.forEach(n=>this.appendChild(n));}
 get nextSibling(){if(!this.parentNode)return null;const c=this.parentNode.children;return c[c.indexOf(this)+1]||null;}
 querySelector(selector){return tree(this).slice(1).find(n=>selector[0]==='.'&&String(n.className||'').split(' ').includes(selector.slice(1)))||null;}
 setAttribute(k,v){this.attrs[k]=String(v);}contains(n){for(;n;n=n.parentNode)if(n===this)return true;return false;}
 addEventListener(type,fn,capture=false){if(!this.listeners.has(type))this.listeners.set(type,[]);if(!this.listeners.get(type).some(e=>e.fn===fn&&e.capture===!!capture))this.listeners.get(type).push({fn,capture:!!capture});}
 removeEventListener(type,fn,capture=false){this.listeners.set(type,(this.listeners.get(type)||[]).filter(e=>e.fn!==fn||e.capture!==!!capture));}
 focus(){this.ownerDocument.activeElement=this;return this.fire('focus');}
 async fire(type,extra={}){let stop=false,immediate=false;const event={type,target:this,defaultPrevented:false,preventDefault(){this.defaultPrevented=true;},stopPropagation(){stop=true;},stopImmediatePropagation(){stop=true;immediate=true;},...extra};
  const path=[];for(let n=this;n;n=n.parentNode)path.push(n);
  async function phase(node,capture){event.currentTarget=node;immediate=false;for(const e of [...node.listeners.get(type)||[]])if(e.capture===capture&&!immediate)await e.fn.call(node,event);}
  for(const n of path.slice().reverse()){await phase(n,true);if(stop)return event;}
  for(const n of path){await phase(n,false);if(stop)return event;}
  return event;
 }
}
function document(){const doc=new Node(null,'#document');doc.ownerDocument=doc;doc.activeElement=null;doc.createElement=tag=>new Node(doc,tag);doc.createComment=()=>new Node(doc,'#comment');doc.body=doc.createElement('body');doc.appendChild(doc.body);doc.getElementById=function(id){function find(n){if(n.id===id)return n;for(const c of n.children){const found=find(c);if(found)return found;}return null;}return find(doc);};return doc;}
function tree(n){return[n,...n.children.flatMap(tree)];}

const tick=()=>new Promise(r=>setImmediate(r));
const motion=fs.readFileSync(path.join(__dirname,'../src/js/motion.js'),'utf8');
function host(options={}){
 const doc=document();doc.querySelector=()=>doc.modal||null;doc.readyState='complete';
 const ids=['props-panel','layer-list','frame-hdr','frame-grid','motion-props-sec','motion-props-body','drawing-canvas'];
 const nodes=Object.fromEntries(ids.map(id=>{const n=doc.createElement('div');n.id=id;return [id,n];}));
 for(const id of ['props-panel','layer-list','frame-hdr','frame-grid','drawing-canvas'])doc.body.appendChild(nodes[id]);
 nodes['props-panel'].appendChild(nodes['motion-props-sec']);const header=doc.createElement('div');header.className='phdr';nodes['motion-props-sec'].appendChild(header);nodes['motion-props-sec'].appendChild(nodes['motion-props-body']);
 const old=doc.createElement('span');old.textContent='legacy inspector';nodes['motion-props-body'].appendChild(old);
 const first=fixture.cases.find(c=>c.expectedResult),q=first.request;
 let info={instanceId:q.instanceId,documentId:q.documentId,lifecycleGeneration:1,contentRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,totalFrames:21,canvasW:320,canvasH:180,fps:24,layers:[{layerUid:'L0',name:'Blue',visible:true},{layerUid:'L1',name:'Red',visible:true}]},active=true,hold=!!options.hold,nonce=0,transportGeneration=0,ready=true;
 const root=options.root||{document:doc,state:{appMode:'motion',activeLayerIdx:0,activeSymbolId:null,currentFrame:0},_layerSel:[0],NemoNativeObjectUI:UI,NemoNativeMotionBinding:options.omitBinding?null:Binding},calls=[],waiting=[],menus=[],storage={},notices=[];
 Object.assign(root,{document:doc,NemoNativeObjectUI:UI,NemoNativeMotionBinding:options.omitBinding?null:Binding});root.state.appMode='motion';root.state.activeSymbolId=null;root._layerSel=[0];
 if(!options.root)Object.defineProperty(root.state,'layers',{get(){throw Error('LEGACY HOLDER READ');}});
 const receipt=()=>({...info,available:ready,kind:'native-object',presentation:{...info,owner:'native',status:ready?'presented':'pending',frame:root.state.currentFrame}});
 const session={isActive:()=>active,blocksLegacy:()=>true,info:()=>info,readPresentedObjectSession:receipt};
 const targets=fixture.cases.filter(c=>['targets-small-first-page','targets-next-page-deduplicates-authored-frames-excludes-reference-only'].includes(c.name)).flatMap(c=>clone(c.expectedResult.items));
 const authored=fixture.cases.filter(c=>c.documentVariant==='authored-facts').flatMap(c=>c.expectedResult.items),rows=fixture.propertyPageOracle.map(row=>clone(authored.find(v=>v.property===row.property)||row));
 const categories=clone(first.expectedResult.unavailableCategories);
 function reply(request){const selector=request.payload,view=selector.view,list=view.kind==='targets'?targets:rows;
  let count=Math.min(view.limit,5,list.length-view.offset),result;
  do{result={instanceId:request.instanceId,documentId:request.documentId,atRevision:selector.atRevision,documentSnapshotId:selector.documentSnapshotId,kind:view.kind,target:view.kind==='targets'?null:clone(view.target),items:clone(list.slice(view.offset,view.offset+count)),nextOffset:view.offset+count<list.length?view.offset+count:null,unavailableCategories:categories};if(Buffer.byteLength(JSON.stringify(result))<=3072)break;count--;}while(count>0);
  return {apiVersion:2,requestId:request.requestId,instanceId:request.instanceId,documentId:request.documentId,contentRevision:selector.atRevision,ok:true,result};
 }
 const sdk=SDK.createNativeApplicationAdapter('ui',{dispatch(request){calls.push(clone(request));if(hold)return new Promise(resolve=>waiting.push({request,resolve}));return Promise.resolve(reply(request));}});
 const ports={currentSession:()=>session,projectionReady:value=>ready&&value===info,readTransportGeneration:()=>transportGeneration,
  readPropertyMetadata:async selector=>{assert.deepEqual(Object.keys(selector).sort(),['atRevision','documentId','documentSnapshotId','instanceId','view']);const response=await sdk.dispatch({apiVersion:2,requestId:'motion-'+(++nonce),instanceId:selector.instanceId,documentId:selector.documentId,operation:'query.document.object.property.metadata',payload:{atRevision:selector.atRevision,documentSnapshotId:selector.documentSnapshotId,view:selector.view}});if(!response.ok)throw Error('refused');return response.result;},
  selectLayer:(index,preserve,uid,expected)=>UI.select(root,info,index,preserve,uid,receipt(),ports),repaint:()=>UI.render(root,info,true,ports)};
 if(options.missingReader)delete ports.readPropertyMetadata;
 root.showContextMenu=(x,y,items)=>menus.push(items);root.showToast=message=>notices.push(message);
 const context=vm.createContext({window:root,document:doc,localStorage:{setItem:(k,v)=>storage[k]=v},PROP_LABEL:{position:'Position'},_motionFilterMode:options.preference||'all'});
 const a=motion.indexOf('  (function installNativeMotionMetadata(){'),b=motion.indexOf('  // Workspace continuity, restore half',a);assert.ok(a>0&&b>a);vm.runInContext(motion.slice(a,b),context);
 return {root,doc,nodes,header,old,calls,waiting,menus,notices,storage,ports,
  async settle(){await tick();const until=Date.now()+1500;while(/^Loading/.test(tree(nodes['motion-props-body']).find(n=>n.attrs.role==='status')?.textContent||'')){if(Date.now()>until)throw Error('Metadata did not settle');await new Promise(r=>setTimeout(r,1));}},
  paint:()=>UI.render(root,info,true,ports),rows:()=>tree(nodes['motion-props-body']).filter(n=>n.className==='native-motion-property-row'),
  button:label=>tree(nodes['motion-props-body']).find(n=>n.tagName==='BUTTON'&&n.textContent===label),
  status:()=>tree(nodes['motion-props-body']).find(n=>n.attrs.role==='status')?.textContent,
  set hold(v){hold=v;},set ready(v){ready=v;},replace(){info={...info,lifecycleGeneration:info.lifecycleGeneration+1};},transport(){transportGeneration++;},release(){const item=waiting.shift();if(item)item.resolve(reply(item.request));},retire(){UI.restoreInspector(root);},set active(v){active=v;}};
}
test('actual ordinary Motion installer reaches visible existing inspector through native UI paint and SDK',async()=>{const h=host();h.paint();await h.settle();assert.ok(h.rows().length>5);assert.equal(h.nodes['motion-props-sec'].hidden,false);assert.equal(h.status(),'Select Keys to inspect native animation.');assert.ok(h.button('Keys'));assert.ok(h.calls.every(c=>c.operation==='query.document.object.property.metadata'));assert.equal(h.header.listeners.get('contextmenu').length,1);h.retire();assert.deepEqual(h.nodes['motion-props-body'].children,[h.old]);});
test('repeated frame paints retain controls, focus and one header listener',async()=>{const h=host();h.paint();await h.settle();const pick=h.button('Red'),before=h.calls.length;await pick.focus();h.root.state.currentFrame=10;h.paint();await h.settle();assert.equal(h.calls.length,before);assert.equal(h.doc.activeElement,pick);assert.equal(h.header.listeners.get('contextmenu').length,1);h.retire();assert.equal(h.header.listeners.get('contextmenu').length,0);});
test('document capture routes P Shift R and U with no legacy bubble shortcut',async()=>{const h=host();let legacy=0;h.doc.addEventListener('keydown',()=>legacy++);h.paint();await h.settle();await h.doc.body.fire('keydown',{key:'p'});await h.settle();assert.equal(h.rows().length,1);assert.equal(h.rows()[0].dataset.property,'position');await h.doc.body.fire('keydown',{key:'r',shiftKey:true});await h.settle();assert.deepEqual([...new Set(h.rows().map(r=>r.dataset.property))],['position','rotation']);await h.doc.body.fire('keydown',{key:'u'});await h.settle();assert.match(h.status(),/^Partial reveal:/);assert.equal(legacy,0);assert.equal(tree(h.nodes['motion-props-body']).filter(n=>n.className==='native-motion-unavailable-group').length,4);h.retire();});
test('header filter dispatch uses ordinary preference and refuses unsupported facts',async()=>{const h=host();h.paint();await h.settle();await h.header.fire('contextmenu');assert.equal(h.menus.length,1);h.menus[0].find(i=>i.label==='expressions').action();await h.settle();assert.equal(h.rows().length,1);assert.equal(h.rows()[0].dataset.property,'xc_number');assert.equal(h.storage['nemo-motion-filter'],'expressions');h.menus[0].find(i=>i.label==='errors').action();await h.settle();assert.equal(h.rows().length,0);assert.equal(h.status(),'This Motion filter is unavailable.');h.retire();});
test('layer selection uses confirmed native UID and invalidates stale selected controls',async()=>{const h=host();h.paint();await h.settle();const old=h.button('Red');await old.fire('click');await h.settle();assert.equal(h.root.state.activeLayerIdx,1);assert.deepEqual(h.root._layerSel,[1]);assert.ok(h.calls.some(c=>c.payload.view.target?.layerUid==='L1'));const count=h.calls.length;await old.fire('click');assert.equal(h.calls.length,count);h.retire();});
test('search is local and editable/modified keys retain their normal route',async()=>{const h=host();h.paint();await h.settle();const search=tree(h.nodes['motion-props-body']).find(n=>n.tagName==='INPUT');search.value='amount';await search.fire('input');assert.equal(h.rows().length,1);assert.equal(h.rows()[0].dataset.property,'xc_number');let e=await search.fire('keydown',{key:'p'});assert.equal(e.defaultPrevented,false);e=await h.doc.body.fire('keydown',{key:'p',metaKey:true});assert.equal(e.defaultPrevented,false);h.retire();});
for(const kind of ['replacement','transport','presentation','frame','selection','retirement'])test('late SDK metadata cannot publish after '+kind,async()=>{const h=host({hold:true});h.paint();await tick();assert.equal(h.waiting.length,1);if(kind==='replacement')h.replace();if(kind==='transport')h.transport();if(kind==='presentation')h.ready=false;if(kind==='frame')h.root.state.currentFrame=10;if(kind==='selection')h.root._layerSel=[1];if(kind==='retirement')h.retire();h.hold=false;h.release();await tick();assert.equal(h.rows().length,0);if(kind==='retirement')assert.deepEqual(h.nodes['motion-props-body'].children,[h.old]);h.retire();});
test('missing SDK read port stays unavailable without showing legacy inspector rows',async()=>{const h=host({missingReader:true});h.paint();await h.settle();assert.equal(h.calls.length,0);assert.equal(h.rows().length,0);assert.equal(h.nodes['motion-props-sec'].hidden,true);assert.match(h.nodes['props-panel'].textContent,/Native Motion property metadata is unavailable/);h.retire();});
test('direct ordinary render call cannot read legacy holders during native ownership',async()=>{const h=host();h.paint();await h.settle();const a=motion.indexOf('  function renderMotionPropsPanel() {'),b=motion.indexOf("    body.innerHTML = '';",a);assert.ok(a>0&&b>a);const fn=motion.slice(a,b)+"throw Error('LEGACY FALLTHROUGH');}";vm.runInContext(fn,vm.createContext({window:h.root,document:h.doc,n20AllowLegacyWrite:()=>false}));const context=vm.createContext({window:h.root,document:h.doc,n20AllowLegacyWrite:()=>false});vm.runInContext(fn+';renderMotionPropsPanel();',context);h.retire();});

test('stale header menu cannot change preferences after selection or ownership changes',async()=>{const h=host();h.paint();await h.settle();await h.header.fire('contextmenu');h.root._layerSel=[1];h.menus[0].find(i=>i.label==='animated').action();assert.equal(h.storage['nemo-motion-filter'],undefined);h.retire();});
test('leaving Motion restores previous DOM and removes all owned listeners; return binds once',async()=>{const h=host();h.paint();await h.settle();h.root.state.appMode='drawing';h.paint();assert.deepEqual(h.nodes['motion-props-body'].children,[h.old]);assert.equal(h.header.listeners.get('contextmenu').length,0);h.root.state.appMode='motion';h.paint();await h.settle();assert.ok(h.rows().length>5);assert.equal(h.header.listeners.get('contextmenu').length,1);h.retire();});
test('unpresented native session cannot start metadata reads',async()=>{const h=host();h.ready=false;assert.throws(()=>h.paint(),/uncorrelated/);assert.equal(h.calls.length,0);h.retire();});

test('actual drawing canvas is an accepted reveal focus and modal capture prevents legacy routing',async()=>{const h=host();h.paint();await h.settle();const e=await h.nodes['drawing-canvas'].fire('keydown',{key:'p'});await h.settle();assert.equal(e.defaultPrevented,true);assert.equal(h.rows().length,1);let legacy=0;h.doc.addEventListener('keydown',()=>legacy++);h.doc.modal=h.doc.createElement('dialog');await h.nodes['drawing-canvas'].fire('keydown',{key:'r'});await h.settle();assert.equal(h.rows()[0].dataset.property,'position');assert.equal(legacy,0);h.retire();});
test('external inspector DOM replacement retires old controls and builds one current binding',async()=>{const h=host();h.paint();await h.settle();const old=h.button('Red');h.nodes['motion-props-body'].replaceChildren();h.paint();await h.settle();assert.ok(h.rows().length>5);assert.equal(h.header.listeners.get('contextmenu').length,1);assert.equal(old.listeners.get('click').length,0);h.retire();});
test('unselected reveal uses all native layers and preserves multiple selected targets',async()=>{const h=host();h.root._layerSel=[];h.paint();await h.settle();await h.doc.body.fire('keydown',{key:'p'});await h.settle();assert.equal(h.rows().length,2);assert.deepEqual(h.rows().map(r=>JSON.parse(r.dataset.targetKey)[1]),['L0','L1']);h.root._layerSel=[1,0];h.paint();await h.settle();await h.doc.body.fire('keydown',{key:'r'});await h.settle();assert.equal(h.rows().length,2);assert.ok(h.rows().every(r=>r.dataset.property==='rotation'));h.retire();});

test('actual A S T reveal routes and E M remain explicitly unavailable',async()=>{const h=host();h.paint();await h.settle();for(const [key,property] of [['a','anchor'],['s','scale'],['t','opacity']]){await h.doc.body.fire('keydown',{key});await h.settle();assert.equal(h.rows().length,1);assert.equal(h.rows()[0].dataset.property,property);}for(const key of ['e','m']){await h.doc.body.fire('keydown',{key});await h.settle();assert.equal(h.status(),'This Motion reveal is unavailable.');}h.retire();});
test('module absence leaves the native inspector unavailable and never activates legacy metadata',async()=>{const h=host({omitBinding:true});h.paint();await h.settle();assert.equal(h.calls.length,0);assert.equal(h.rows().length,0);assert.equal(h.nodes['motion-props-sec'].hidden,true);assert.match(h.nodes['props-panel'].textContent,/Native Motion property values and edits are unavailable/);const a=motion.indexOf('  function renderMotionPropsPanel() {'),b=motion.indexOf("    body.innerHTML = '';",a),context=vm.createContext({window:h.root,document:h.doc,n20AllowLegacyWrite:()=>false});vm.runInContext(motion.slice(a,b)+"throw Error('LEGACY FALLTHROUGH');};renderMotionPropsPanel();",context);assert.equal(h.calls.length,0);h.retire();});
test('duplicate native Motion registration is rejected',()=>{const h=host();assert.throws(()=>UI.bindMotion(h.root,{paint(){return true;},retire(){}}),/already bound/);h.retire();});

test('direct public reveal entrypoints consume native ownership even with missing binding',async()=>{const h=host({omitBinding:true}),a=motion.indexOf('  function routeNativeMotionRead('),b=motion.indexOf('  function handlePropShortcut(',a),helper=motion.slice(a,b);const ctx=vm.createContext({window:h.root,state:h.root.state,PROP_SHORTCUT:{p:'position'},n20AllowLegacyWrite:()=>false});for(const [name,end] of [['handlePropShortcut','    // Filtering'],['handleRevealAnimatedShortcut','    // U is a TOGGLE'],['handleRevealEffectsShortcut','    return revealByLayerPredicate'],['handleRevealMattesShortcut','    return revealByLayerPredicate']]){const start=motion.indexOf('  function '+name+'('),finish=motion.indexOf(end,start);assert.ok(start>0&&finish>start);vm.runInContext(helper+motion.slice(start,finish)+"throw Error('LEGACY FALLTHROUGH');}",ctx);let consumed;assert.doesNotThrow(()=>{consumed=vm.runInContext(name+"('p',false)",ctx);});assert.equal(consumed,true);}h.retire();});
test('ordinary UI paint and public reveal use actual sole Project cached port and SDK owner request IDs',async()=>{
 const {createRequire}=require('node:module'),ownerPath=path.join(__dirname,'native-object-project-desktop-ports.test.cjs'),ownerSource=fs.readFileSync(ownerPath,'utf8');
 const makeOwner=new Function('require',ownerSource.slice(0,ownerSource.indexOf('\ntest('))+';return fixture;')(createRequire(ownerPath));
 const f=makeOwner();await f.open();const h=host({root:f.root}),Project=require('../src/js/adapters/native-object-project.js'),before=f.session.info();
 f.mutateReply(reply=>{const request=f.calls.at(-1).r;if(request.operation!=='query.document.object.property.metadata')return;const v=request.payload.view,list=v.kind==='targets'?[{kind:'context',contextId:'scene-root'},{kind:'layer',target:{contextId:'scene-root',layerUid:'drawing'}}]:fixture.propertyPageOracle.slice(0,5);reply.result={instanceId:request.instanceId,documentId:request.documentId,atRevision:request.payload.atRevision,documentSnapshotId:request.payload.documentSnapshotId,kind:v.kind,target:v.kind==='targets'?null:v.target,items:clone(list.slice(v.offset,v.offset+v.limit)),nextOffset:null,unavailableCategories:clone(fixture.cases.find(c=>c.expectedResult).expectedResult.unavailableCategories)};});
 assert.equal(Project.paintUI(f.root,false),true);await h.settle();assert.equal(h.rows().length,5);assert.equal(h.status(),'Select Keys to inspect native animation.');assert.ok(h.button('Keys'));
 await h.doc.body.fire('keydown',{key:'p'});await h.settle();assert.equal(h.rows().length,1);assert.equal(h.rows()[0].dataset.property,'position');assert.equal(Project.desktop(f.root),f.session);assert.strictEqual(f.session.info(),before);assert.equal(f.transports.length,1);
 const queries=f.calls.filter(c=>c.r?.operation==='query.document.object.property.metadata');assert.ok(queries.length>=3);assert.ok(queries.every(c=>/^project-metadata-/.test(c.r.requestId)&&!('expectedRevision'in c.r)));assert.ok(f.calls.every(c=>!c.r?.operation?.startsWith('command.')));h.retire();
});

test('ordinary Motion hooks expose known cleared selection only after native editor retirement',async()=>{
 const h=host();assert.equal(typeof UI.readSelectedMotionKeys,'function');
 const ordinary=Binding.createOrdinary(h.root,{});const info=h.ports.currentSession().info();assert.equal(ordinary.captureSelectedKeys(info),null,'Motion with no qualified editor is unknown');
 h.root.state.appMode='animation';const cleared=ordinary.captureSelectedKeys(info);assert.deepEqual(cleared.keys,[]);assert.equal(cleared.isCurrent(),true);
 ordinary.dispose();assert.equal(cleared.isCurrent(),false);assert.equal(ordinary.captureSelectedKeys(info),null);h.retire();
});


test('actual ordinary Drawing paint retires native keys and exposes only a current known-cleared selection',async()=>{
 const h=host();h.paint();await h.settle();h.root.state.appMode='drawing';h.paint();
 const snapshot=UI.readSelectedMotionKeys(h.root,h.ports.currentSession().info());
 assert.ok(snapshot);assert.deepEqual(snapshot.keys,[]);assert.equal(snapshot.isCurrent(),true);
 h.root.state.appMode='motion';assert.equal(snapshot.isCurrent(),false);h.retire();
});
