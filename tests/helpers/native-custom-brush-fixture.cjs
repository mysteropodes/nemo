'use strict';
const fs = require('node:fs'), vm = require('node:vm'), assert = require('node:assert/strict');
const {isDeepStrictEqual} = require('node:util');
const C = require('../../src/js/adapters/native-custom-brush-contract.js');
const copy = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
function node(tag = 'div') {
  const listeners = new Map();
  const el = {tagName:tag.toUpperCase(),style:{},dataset:{},children:[],parentNode:null,className:'',id:'',value:'',width:150,height:20,
    offsetWidth:300,offsetHeight:200,textContent:'',disabled:false,
    addEventListener(type,fn){if(!listeners.has(type))listeners.set(type,[]);listeners.get(type).push(fn);},
    removeEventListener(type,fn){listeners.set(type,(listeners.get(type)||[]).filter(f=>f!==fn));},
    fire(type,extra={}){const e={type,target:el,preventDefault(){},stopPropagation(){},...extra};for(const fn of [...(listeners.get(type)||[])])fn.call(el,e);},
    dispatchEvent(e){this.fire(e.type,e);},click(){this.fire('click');},focus(){},
    count(){return [...listeners.values()].reduce((sum,list)=>sum+list.length,0);},
    appendChild(child){this.children.push(child);child.parentNode=this;return child;},
    remove(){if(this.parentNode)this.parentNode.children.splice(this.parentNode.children.indexOf(this),1);this.parentNode=null;},
    contains(child){return child===this||this.children.some(row=>row.contains(child));},
    getBoundingClientRect(){return {left:20,bottom:30,top:0,right:200,width:180,height:30};},
    querySelectorAll(selector){
      const parts=selector.split(' '),last=parts.pop();let all=[];
      function walk(parent){for(const child of parent.children){all.push(child);walk(child);}}walk(this);
      return all.filter(child=>matches(child,last)&&(!parts.length||ancestor(child,parts.join(' '),this)));
    },
    querySelector(selector){return this.querySelectorAll(selector)[0]||null;},
    getContext(){return new Proxy({}, {get:(target,key)=>key==='fillText'?(text)=>{this.previewText=text;}:()=>{},set:()=>true});}
  };
  function matches(child,selector){
    const attr=selector.match(/\[data-([\w-]+)="([^"]*)"\]/),clean=selector.replace(/\[.*\]/,'');
    const ok=clean[0]==='#'?child.id===clean.slice(1):clean[0]==='.'?child.classList.contains(clean.slice(1)):child.tagName===clean.toUpperCase();
    return ok&&(!attr||child.dataset[attr[1].replace(/-([a-z])/g,(_,x)=>x.toUpperCase())]===attr[2]);
  }
  function ancestor(child,selector,bound){for(let p=child.parentNode;p&&p!==bound;p=p.parentNode)if(matches(p,selector))return true;return matches(bound,selector);}
  el.classList={contains:value=>el.className.split(/\s+/).includes(value),
    toggle(value,force){const items=new Set(el.className.split(/\s+/).filter(Boolean));if(force===undefined)force=!items.has(value);if(force)items.add(value);else items.delete(value);el.className=[...items].join(' ');return force;},
    add(...values){values.forEach(value=>this.toggle(value,true));},remove(...values){values.forEach(value=>this.toggle(value,false));}};
  Object.defineProperty(el,'innerHTML',{get(){return '';},set(source){
    this.children.forEach(child=>child.parentNode=null);this.children=[];
    // Controlled DOM parser for the original editor's static markup, not replacement UI callbacks.
    const stack=[this],tokens=source.match(/<[^>]+>/g)||[];
    for(const token of tokens){if(token.startsWith('</')){if(stack.length>1)stack.pop();continue;}
      const match=token.match(/^<([\w-]+)/);if(!match)continue;const child=node(match[1]);
      for(const a of token.matchAll(/([\w-]+)="([^"]*)"/g)){
        if(a[1].startsWith('data-'))child.dataset[a[1].slice(5).replace(/-([a-z])/g,(_,x)=>x.toUpperCase())]=a[2];
        else if(a[1]==='class')child.className=a[2];else child[a[1]]=a[2];
      }
      stack.at(-1).appendChild(child);if(!['input','br','hr','img'].includes(match[1]))stack.push(child);
    }
  }});return el;
}
function actualOrder(){
  const expected=['adapters/native-custom-brush-ui.js','tools.js','brush-preset-picker.js','brush-tool-presets.js','brush-editor.js','timeline.js','brush-menu-bridge.js',
    'adapters/native-custom-brush-contract.js','adapters/native-application.js','adapters/native-custom-brush-desktop.js'];
  const html=fs.readFileSync(require.resolve('../../src/index.html'),'utf8');
  const found=[...html.matchAll(/<script\b[^>]*src="js\/([^"\n]+)"[^>]*>/g)].map(m=>m[1]).filter(name=>expected.includes(name));
  assert.deepEqual(found,expected);return found;
}
function extract(source,name){
  const start=source.indexOf('function '+name+'('),end=source.indexOf('\n}',start);
  assert(start>=0&&end>start,name+' must be the actual production function');return source.slice(start,end+2);
}
// Controlled queue, owner, native metadata and detached preview ports. No installed/native acceptance inference.
async function fixture(initial,options={}){
  const doc=node('document'),events=node('window'),body=node('body'),ids={};doc.body=body;doc.documentElement=node('html');doc.readyState='loading';
  doc.createElement=tag=>node(tag);doc.createTextNode=text=>Object.assign(node('span'),{textContent:text});
  doc.getElementById=id=>ids[id]||body.querySelector('#'+id);doc.querySelectorAll=selector=>body.querySelectorAll(selector);
  for(const id of ['p-brushpreset-btn','p-brushpreset-preview','p-brushpreset-label','p-brushfav-row','p-sw','p-vecbrush','toolpreset-grid','btn-toolpreset-save']){
    ids[id]=node(id==='p-brushpreset-preview'?'canvas':'div');ids[id].id=id;body.appendChild(ids[id]);
  }
  const storage=new Map([['sm-brush-favorites',JSON.stringify(options.favorites||[])],['nemo-brush-tool-presets',JSON.stringify({personal:{label:'Personal',fields:{brushSize:8}}})]]);
  const calls=[],notices=[],previews=[];let catalog=copy(initial),revision=0,serial=0,ticket=1,running=0,active=true,held=null,refusal=false,broken=false;
  let tail=Promise.resolve(),context,controller,history=[copy(initial)],historyAt=0;
  let info={instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'s0'};
  const shown=pin=>({...pin,presentation:{...pin,owner:'native',status:'presented'}});
  const root={document:doc,state:{brushPreset:'none',brushSize:12,tool:'select'},innerWidth:1200,innerHeight:900,TextEncoder,
    localStorage:{getItem:key=>storage.get(key)||null,setItem:(key,value)=>storage.set(key,value)},
    showToast:text=>notices.push(text),prompt:()=> 'New personal bundle',SM:{t:key=>key},Event:function(type){this.type=type;},
    getComputedStyle:()=>({getPropertyValue:()=> '#fff'}),setTimeout:fn=>fn(),clearTimeout(){},
    addEventListener:events.addEventListener.bind(events),removeEventListener:events.removeEventListener.bind(events),
    Path:function(spec){assert.equal(spec.insert,false);this.segments=[];this.add=()=>{};this.smooth=()=>{};},Point:function(x,y){this.x=x;this.y=y;},
    buildBrushDabs:(path,preset)=>{previews.push(copy(preset));return [];},renderLabsFloatPanel(){},
    selectedPaths:new Proxy([],{get(){throw Error('legacy live selection read');}}),
    stripAnyBrushTexture(){throw Error('legacy texture mutation');},pushUndo(){throw Error('legacy history mutation');},
    saveActiveLayerFrame(){throw Error('legacy frame mutation');},updateUI(){},n20AllowLegacyWrite:()=>false};
  if(!options.browser){root.__TAURI__={core:{}};Object.defineProperty(root.state,'customBrushPresets',{get(){throw Error('legacy catalog read');},set(){throw Error('legacy catalog write');}});}
  else {root.state.customBrushPresets=Object.fromEntries(initial||[]);root.selectedPaths=[];}
  async function dispatch(q){
    calls.push(copy(q));if(held){const gate=held;held=null;await gate;}
    const r={apiVersion:2,requestId:q.requestId,instanceId:info.instanceId,documentId:info.documentId,contentRevision:revision,ok:true};
    if(refusal||q.documentId!==info.documentId||q.operation===C.READ&&q.payload.selector.kind==='record'&&options.unavailableIds?.includes(q.payload.selector.id)){
      r.ok=false;r.error={code:q.documentId!==info.documentId?'wrong_document':typeof refusal==='string'?refusal:'unavailable',message:'controlled refusal'};
    }else if(q.operation===C.READ){
      const selector=q.payload.selector,entries=catalog||[];let result;
      if(selector.kind==='settings')result={customBrushPresetsPresent:catalog!==undefined,presetCount:entries.length};
      else if(selector.kind==='catalogPage'){
        const items=entries.slice(selector.cursor,selector.cursor+selector.limit).map(([id,preset],i)=>({ordinal:selector.cursor+i+1,id,...('label' in preset?{label:preset.label}:{})}));
        result={kind:'customBrushPresetPage',customBrushPresetsPresent:catalog!==undefined,presetCount:entries.length,cursor:selector.cursor,items,
          nextCursor:selector.cursor+items.length<entries.length?selector.cursor+items.length:null};
      }else{const row=entries.find(row=>row[0]===selector.id);if(!row){r.ok=false;r.error={code:'not_found',message:'missing'};}
        else result={kind:'customBrushPresetRecord',id:row[0],preset:row[1]};}
      if(r.ok)r.result={instanceId:info.instanceId,documentId:info.documentId,atRevision:revision,documentSnapshotId:info.documentSnapshotId,contextId:'scene-root',result};
    }else{
      assert.equal(q.operation,C.EDIT);assert.equal(q.expectedRevision,revision);const action=q.payload.action,next=copy(catalog||[]),index=next.findIndex(row=>row[0]===action.id);
      if(action.kind==='delete'){if(index>=0)next.splice(index,1);}else if(index<0)next.push([action.id,copy(action.preset)]);else next[index]=[action.id,copy(action.preset)];
      const applied=action.kind==='delete'&&index<0?false:!isDeepStrictEqual(catalog,next);
      if(applied){catalog=next;revision++;history.splice(historyAt+1);history.push(copy(catalog));historyAt++;}
      r.contentRevision=revision;r.result={applied,historyEntriesAdded:Number(applied)};
    }
    if(broken&&r.ok&&q.operation===C.READ)r.result.documentSnapshotId='old';
    return vm.runInContext('JSON.parse('+JSON.stringify(JSON.stringify(r))+')',context);
  }
  const session={info:()=>info,isActive:()=>active,requestId:()=> 'catalog-'+(++serial),readSubscriptionGeneration:()=>ticket,
    readPresentedObjectSession:()=>active&&!running?shown(info):null,
    runResourceCommand(fn){running++;const job=tail.then(()=>{const origin=info;return fn({
      dispatch:q=>root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('catalog-fixture',{dispatch}).dispatch(q),
      readPresentedObjectSession:()=>active&&running===1&&info===origin?shown(origin):null,
      reconcile:async next=>{assert.equal(next,revision);info={...info,contentRevision:revision,documentSnapshotId:'s'+revision};controller.willProject();controller.presented();return info;}
    });}).finally(()=>{running--;if(controller)controller.presented();});tail=job.catch(()=>{});return job;}};
  root.NemoNativeObjectProject={desktop:()=>session};root.NemoNativeObjectProjectFile={projectionReadyForUI:()=>active&&!running};
  root.window=root;root.globalThis=root;context=vm.createContext(root);
  for(const file of actualOrder()){
    let source=fs.readFileSync(require.resolve('../../src/js/'+file),'utf8');
    if(file==='tools.js'){
      const start=source.indexOf('var BRUSH_PRESETS='),end=source.indexOf('\n};',start);assert(end>start);
      source=source.slice(start,end+3)+'\n'+extract(source,'resolveBrushPreset')+'\n'+extract(source,'applyBrushTexture');
    }else if(file==='timeline.js'){
      const start=source.indexOf('  setBrushPreset:function('),end=source.indexOf('\n  setSmoothing:',start);assert(start>=0&&end>start);
      source='Object.assign(SM,{'+source.slice(start,end)+'});\n'+extract(source,'applyVectorBrushToSelection');
    }
    vm.runInContext(source,context,{filename:file});
  }
  doc.fire('DOMContentLoaded');controller=root.NemoNativeCustomBrushController;
  async function settled(){for(let i=0;i<30;i++){await tail;await new Promise(resolve=>setImmediate(resolve));if(!running&&!controller?.isPending())return;}throw Error('catalog queue did not settle');}
  if(!options.browser)await settled();
  return {root,doc,body,ids,events,storage,calls,notices,previews,controller,settled,session,catalog:()=>copy(catalog),checkpoints:()=>history.length-1,
    edits:()=>calls.filter(q=>q.operation===C.EDIT),popover:kind=>body.querySelector(kind==='editor'?'.bp-editor-pop':kind==='menu'?'.brush-menu-pop':'.bp-picker-pop'),
    action:value=>vm.runInContext('JSON.parse('+JSON.stringify(JSON.stringify(value))+')',context),
    hold(){let release;held=new Promise(resolve=>release=resolve);return release;},refuse(code='unavailable'){refusal=code;},staleRead(){broken=true;},resubscribe(){ticket++;},
    replace(value){catalog=copy(value);revision++;info={...info,documentId:'other',lifecycleGeneration:info.lifecycleGeneration+1,contentRevision:revision,documentSnapshotId:'s'+revision};controller.willProject();controller.presented();},
    history(delta){historyAt+=delta;catalog=copy(history[historyAt]);revision++;info={...info,contentRevision:revision,documentSnapshotId:'s'+revision};controller.willProject();controller.presented();},
    retire(){active=false;controller.dispose();}};
}
module.exports={fixture,copy};
