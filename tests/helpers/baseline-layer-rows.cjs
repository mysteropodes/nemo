'use strict';
// Runs the BASELINE (1065e9d8) renderLayerList from its own source text in a vm with a small fake DOM, so a test can diff the
// native presenter against what the baseline actually draws. Only the row builder and the pure helpers it calls are extracted
// (computeLayerRenderOrder, folderLayerChildMap, matteSourceIndicesInUse); window/app services are stubbed.
const vm=require('node:vm'),cp=require('node:child_process');
class Node{
  constructor(tag){this.tagName=String(tag).toUpperCase();this.children=[];this.parentNode=null;this.style={setProperty:(k,v)=>{this.styleProps[k]=v;}};this.styleProps={};
    this.dataset={};this.attrs={};this.innerHTMLValue='';this.textContent='';this.title='';this.className='';this.hidden=false;
    this.classList={add:(...k)=>{this.className=(this.className+' '+k.join(' ')).trim();},remove:()=>{},contains:k=>this.className.split(/\s+/).includes(k),toggle(){}};}
  appendChild(n){if(n.parentNode)n.parentNode.removeChild(n);this.children.push(n);n.parentNode=this;return n;}
  removeChild(n){this.children.splice(this.children.indexOf(n),1);n.parentNode=null;}
  replaceChildren(...k){this.children.forEach(c=>{c.parentNode=null;});this.children=[];k.forEach(c=>this.appendChild(c));}
  setAttribute(k,v){this.attrs[k]=String(v);}
  addEventListener(){}
  set innerHTML(v){this.innerHTMLValue=String(v);this.children.forEach(c=>{c.parentNode=null;});this.children=[];}
  get innerHTML(){return this.innerHTMLValue;}
  querySelector(){return null;}
}
function fakeDocument(){return {createElement:t=>new Node(t),getElementById:()=>null};}
// A row's comparable decoration: only the nodes #2371 draws, class order normalised (as CLAUDE.md 5bis), no handlers.
const KEEP=/\b(larrow|larrow-spacer|layer-color-dot|lkind|comp-badge)\b/;
function project(row){
  const norm=c=>c.split(/\s+/).filter(Boolean).sort().join(' ');
  const rowClasses=norm(row.className).split(' ').filter(c=>['is-comp','in-folder'].includes(c)).join(' ');
  const nodes=row.children.filter(c=>KEEP.test(c.className)).map(c=>({cls:norm(c.className),html:c.innerHTML,text:c.textContent,dot:c.styleProps['--dot-color']||null,title:c.title}));
  return {rowClasses,nodes};
}
function source(commit,file){return cp.execFileSync('git',['show',commit+':'+file],{encoding:'utf8',maxBuffer:64<<20,cwd:__dirname});}
function fn(text,name){
  const a=text.indexOf('\nfunction '+name+'(');if(a<0)throw new Error('baseline function missing: '+name);
  const b=text.indexOf('\nfunction ',a+10);return text.slice(a,b<0?text.length:b);
}
// layers: baseline layer-data objects in storage order. Returns [{uid, row: projection}] for the rows renderLayerList emits.
function renderBaseline(text,layers){
  const doc=fakeDocument(),list=new Node('div');doc.getElementById=id=>id==='layer-list'?list:null;
  const noop=()=>{};
  const win={SMLayerKind:null,SMExprPanel:null,SMCamera:null,SMMotion:null,SM:{t:k=>k,toggleLayerVis:noop,toggleLayerLock:noop,toggleLayerSolo:noop,toggleFolderLayerCollapsed:noop,setActiveLayer:noop,enterSymbol:noop},
    showContextMenu:noop,SMAudio:null,_layerShapesExpanded:null};
  const ctx={window:win,SM:win.SM,document:doc,state:{layers,activeLayerIdx:-1,appMode:'animation',layerFolders:{},layerLinkGroups:{},shyEnabled:false,currentFrame:0},
    _layerSel:[],ROW_H:22,ICO_EYE:'',ICO_EYE_CLOSED:'',ICO_LOCK:'',ICO_UNLOCK:'',ICO_3D:'',ICO_DUP:'',Date,Math,
    reconcileLayerIndexState:noop,reconcileFrameSelection:noop,_tlScrollSnapshot:noop,_tlScrollRestore:noop,layerCurveRowExtraHeight:()=>0,
    buildParentCell:noop,armLayerReorder:noop,renderShapeTreeRowsInto:noop,matteModeLabel:m=>m,activateUL:noop,updatePropsContext:noop,openMatteDropdownAt:noop,
    startLayerRename:noop,selectLayersByColor:noop,openLayerColorSwatches:noop,applyLayerColor:noop,_layerKeyOf:()=>null,_indexOfLayerKey:()=>-1};
  vm.createContext(ctx);
  win.SMLayerKind=ctx.SMLayerKind=require('./layer-kind-global.cjs')(win);
  const code=[fn(text,'folderLayerChildMap'),fn(text,'computeLayerRenderOrder'),fn(text,'matteSourceIndicesInUse'),fn(text,'renderLayerList')].join('\n');
  vm.runInContext(code+'\nrenderLayerList();',ctx);
  return list.children.map(row=>({layerUid:layers[+row.dataset.layer].layerUid,row:project(row)}));
}
module.exports={Node,fakeDocument,project,renderBaseline,source};
