'use strict';
// CS5 (#2347) pilot surface: the native layer list. The DOM fixture was captured from the
// pre-seam renderer (commit before the seam); the seam path must reproduce it exactly.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const UI=require('../src/js/adapters/native-object-ui.js'),Files=require('../src/js/adapters/native-object-project-file.js');
const FIXTURE=path.join(__dirname,'fixtures','native-layer-list-dom.json');
const A={layerUid:'opaque A:雪',name:'<b>A</b>',visible:true},B={layerUid:'B',name:'B',visible:false},C={layerUid:'C',visible:true};
// Element.children is a live HTMLCollection, never an Array: index access, length and item() only.
function collection(kids){const out=Object.create({item(i){return kids[i]||null;}});kids.forEach((k,i)=>{out[i]=k;});
  Object.defineProperty(out,'length',{value:kids.length});return Object.freeze(out);}
function element(tagName){
  const classes=new Set(),listeners={},kids=[];
  const el={tagName,style:{},dataset:{},attributes:{},textContent:'',type:'',value:'',hidden:false,disabled:false,
    classList:{add:x=>classes.add(x),remove:x=>classes.delete(x),contains:x=>classes.has(x),toggle(x,on){if(on===undefined)on=!classes.has(x);if(on)classes.add(x);else classes.delete(x);return on;}},
    setAttribute(k,v){this.attributes[k]=String(v);},addEventListener(k,f){listeners[k]=f;},removeEventListener(){},
    replaceChildren(...next){kids.splice(0).forEach(k=>{k.parentNode=null;});next.forEach(k=>this.appendChild(k));},
    appendChild(k){if(k.parentNode)k.parentNode.removeChild(k);kids.push(k);k.parentNode=this;return k;},
    removeChild(k){kids.splice(kids.indexOf(k),1);k.parentNode=null;},
    querySelector:()=>null,contains(k){while(k){if(k===this)return true;k=k.parentNode;}return false;},
    click(){return listeners.click&&listeners.click({target:this,preventDefault(){},stopPropagation(){}});}};
  Object.defineProperty(el,'className',{get:()=>[...classes].join(' '),set(v){classes.clear();String(v).split(/\s+/).filter(Boolean).forEach(x=>classes.add(x));}});
  Object.defineProperty(el,'children',{get:()=>collection(kids)});
  Object.defineProperty(el,'innerHTML',{set(){this.replaceChildren();}});
  return el;
}
function documentFor(){const nodes=new Map(['layer-list','frame-hdr','frame-grid','tl-cf','tl-tf','info-frame','info-badge','info-strokes','info-sel']
  .map(id=>[id,Object.assign(element('div'),{id})]));
  return {nodes,activeElement:null,getElementById:id=>nodes.get(id)||null,createElement:element,createComment:()=>element('#comment'),addEventListener(){},removeEventListener(){}};}
function metadata(layers){return {apiVersion:2,instanceId:'instance',documentId:'document',lifecycleGeneration:1,contentRevision:3,documentSnapshotId:'document-snapshot-3',
  documentDigest:'4'.repeat(64),byteCount:1000,canvasW:320,canvasH:180,fps:24,totalFrames:24,reference:null,undoDepth:0,redoDepth:0,layers:layers.map(l=>({...l}))};}
function serialize(node){
  const out={tag:node.tagName,text:node.textContent};
  for(const k of ['className','type','hidden','disabled'])if(node[k])out[k]=node[k];
  for(const k of ['display','flex','textAlign'])if(node.style[k]!==undefined)out['style.'+k]=node.style[k];
  const sorted=o=>Object.fromEntries(Object.entries(o).sort(([a],[b])=>a<b?-1:1));
  if(Object.keys(node.attributes).length)out.attributes=sorted(node.attributes);
  if(Object.keys(node.dataset).length)out.dataset=sorted(node.dataset);
  if(node.children.length)out.children=Array.from(node.children,serialize);
  return out;
}
const CASES=[
  {name:'two layers, no selection, no policy owner',layers:[A,B],selection:[],shy:false,policy:false},
  {name:'three layers with flags, multi-selection, policy owner',layers:[{...A,locked:true},{...B,shy:true},C],selection:[0,2],shy:false,policy:true},
  {name:'shy mode hides shy rows',layers:[{...A,locked:true},{...B,shy:true},C],selection:[1],shy:true,policy:true},
];
function render(c,setup){
  const root={state:{currentFrame:7,activeLayerIdx:c.selection[0]||0},userLayers:[],document:documentFor(),_layerSel:c.selection.slice(),
    NemoNativeObjectProjectFile:Files,NemoNativeObjectUI:UI};
  const info=metadata(c.layers);Files.projectView(root,info);root._layerSel=c.selection.slice();
  const ports={projectionReady:i=>Files.projectionReadyForUI(root,i),currentSession:()=>null,selectLayer:()=>true,repaint:()=>true,
    readShyMode:()=>c.shy,...(c.policy?{layerPolicy:()=>({edit(){return Promise.resolve();}})}:{})};
  if(setup)setup(root);
  assert.equal(UI.render(root,info,false,ports),true);
  return Object.fromEntries(['layer-list','frame-hdr','frame-grid'].map(id=>[id,serialize(root.document.nodes.get(id))]));
}

test('layer list DOM through the presenter seam is identical to the pre-seam characterization',()=>{
  const expected=JSON.parse(fs.readFileSync(FIXTURE,'utf8'));
  assert.deepEqual(CASES.map(c=>c.name),Object.keys(expected));
  for(const c of CASES)assert.deepEqual(render(c),expected[c.name],c.name);
});
module.exports={CASES,render};
