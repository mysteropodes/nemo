paper.install(window);
// paper.install(window) overwrites window.Event and window.MouseEvent with
// Paper's own internal classes of the same name (it also clobbers
// Point/Path/Color/Rectangle/Size/Symbol, but the rest of the app relies on
// Paper's versions of those for geometry, so only the two true DOM
// constructors are restored here, via a pristine iframe). Any app code doing
// `new Event(...)` after this point would otherwise silently construct an
// incompatible object; passing it to dispatchEvent() throws synchronously —
// this is what broke the properties-panel drag-to-scrub inputs (ui.js): the
// exception aborted endScrub() before it could clear the drag state,
// leaving the value tracking every future pointer move anywhere on the page.
(function(){
  var iframe = document.createElement('iframe');
  iframe.style.display = 'none';
  document.documentElement.appendChild(iframe);
  window.Event = iframe.contentWindow.Event;
  window.MouseEvent = iframe.contentWindow.MouseEvent;
  document.documentElement.removeChild(iframe);
})();
paper.setup(document.getElementById('drawing-canvas'));

// Paper's own `resize` attribute on the canvas is meant to keep its backing
// resolution (view.viewSize, i.e. canvas.width/height) in sync with its CSS
// box automatically — but it can end up stuck at whatever size canvas-area
// happened to have at the exact moment paper.setup() just ran above (before
// the app's flex layout has settled) and never recover on a later window
// resize. Reported directly by the user as a resize bug, and it's also the
// root cause behind the Rust-engine mirror (engine-bridge.js) computing
// wildly wrong world coordinates for an intercepted tool's live preview: it
// derives its own canvas size from this same stuck value. A ResizeObserver
// on the container is more reliable than depending on Paper's own listener:
// it fires for ANY container size change (window resize, side-panel
// drag-resize, etc.), and assigning view.viewSize directly is the documented
// way to make Paper recompute the canvas's backing resolution without
// touching zoom/center — so the artwork doesn't jump/rescale on a resize,
// only the visible viewport area changes, matching what the `resize`
// attribute was always supposed to do.
function syncCanvasSize(){
  var ca=document.getElementById('canvas-area');
  var w=ca.clientWidth,h=ca.clientHeight;
  if(w>0&&h>0&&(Math.round(view.viewSize.width)!==w||Math.round(view.viewSize.height)!==h)){
    view.viewSize=new Size(w,h);
  }
}
if(window.ResizeObserver)new ResizeObserver(syncCanvasSize).observe(document.getElementById('canvas-area'));
window.addEventListener('resize',syncCanvasSize);

var FC=14;
var state={
  totalFrames:24,currentFrame:0,fps:12,playing:false,loopPlayback:true,
  tool:'draw',
  onionSkin:true,onionPrevOpacity:30,onionNextOpacity:30,onionMode:'tinted',
  onionIn:0,onionOut:23,
  brushSize:3,strokeColor:'#000000',fillColor:'#ff0000',fillEnabled:true,
  smoothing:10,stabilizer:2,strokeCap:'round',strokeJoin:'round',opacity:100,
  miterLimit:10,dashOffset:0,paintOrder:'fillFirst',
  strokeStyle:'solid',vectorBrush:false,taperEnds:false,
  drawMode:'front',fillBrushMode:'below',pressureMin:30,pressureMax:170,pressureInvert:false,
  // eraseAtPoint() already does a REAL vector boolean subtract (Paper.js
  // .subtract() / Rust erase_at_point), not item.remove() — verified by
  // reading it end to end. The "gomme supprime des objets entiers" report
  // traced back to this default: at eraserSize=24 vs the default stroke
  // width of 3, a single click's capsule/circle is 8x wider than a typical
  // stroke, so it fully consumes the stroke's whole remaining fill area in
  // one hit — a technically-correct empty-result subtract that's visually
  // indistinguishable from full deletion. Lowered so partial erasing is
  // actually visible at default settings; still user-adjustable via the
  // Eraser field in Tool Options exactly as before.
  eraserSize:14,selRotAccum:0,tweenSkipManual:true,
  resamplePts:50,tweenStep:1,
  canvasW:1920,canvasH:1080,canvasBg:'#ffffff',
  layers:[],activeLayerIdx:0,
  motionArcs:{},easingCurve:{points:[{x:0,y:0},{x:.42,y:0},{x:.58,y:1},{x:1,y:1}]},
  selectedStrokeIndices:[],
  undoStack:[],redoStack:[],maxUndo:60,
  waIn:0,waOut:23,
  isPanning:false,spaceDown:false,
  symbols:{},activeSymbolId:null,openSymbolTabs:[],
};

var stageLayer=new Layer({name:'stage'});
var onionPrevLayer=new Layer({name:'onion-prev'});
var onionNextLayer=new Layer({name:'onion-next'});
var userLayers=[];
var arcLayer=new Layer({name:'arcs'});
var nodeLayer=new Layer({name:'nodes'});
var marqueeLayer=new Layer({name:'marquee'});
var xformLayer=new Layer({name:'xform'});

function createUserLayer(name){
  arcLayer.activate();var l=new Layer({name:'user-'+userLayers.length});l.insertBelow(arcLayer);userLayers.push(l);
  var frames=[];for(var i=0;i<state.totalFrames;i++)frames.push({strokes:[],isKeyframe:i===0,isInterpolated:false});
  state.layers.push({id:state.layers.length,name:name,visible:true,locked:false,frames:frames});
  return state.layers.length-1;
}
createUserLayer('Layer 1');
// First free "Layer N" name — the old 'Layer '+length scheme produced
// duplicates (a second layer was also named "Layer 1") and reused names
// after renames/deletes, which read as layers "stealing" each other's
// names in the panel.
function nextLayerName(){
  var names=state.layers.map(function(l){return l.name;});
  var n=1;while(names.indexOf('Layer '+n)>=0)n++;
  return 'Layer '+n;
}
function activateUL(idx){state.activeLayerIdx=idx;if(userLayers[idx])userLayers[idx].activate();}
activateUL(0);
// Drag-to-reorder in the layer panel — array index i in state.layers/
// userLayers means "rendered above index i-1" (see createUserLayer's
// insertBelow(arcLayer)), so after splicing both arrays into the new order
// the actual Paper.js z-order is rebuilt by re-inserting every layer below
// the overlay stack in ascending index order (each call bumps that layer
// above everything inserted so far, ending with the highest index on top).
function reorderLayer(fromIdx,toIdx){
  if(fromIdx===toIdx||fromIdx<0||toIdx<0||fromIdx>=state.layers.length||toIdx>=state.layers.length)return;
  saveAllLayerFrames();pushUndoLayers();
  var ld=state.layers.splice(fromIdx,1)[0];state.layers.splice(toIdx,0,ld);
  var ul=userLayers.splice(fromIdx,1)[0];userLayers.splice(toIdx,0,ul);
  userLayers.forEach(function(l){l.insertBelow(arcLayer);});
  if(state.activeLayerIdx===fromIdx)state.activeLayerIdx=toIdx;
  else if(fromIdx<state.activeLayerIdx&&toIdx>=state.activeLayerIdx)state.activeLayerIdx--;
  else if(fromIdx>state.activeLayerIdx&&toIdx<=state.activeLayerIdx)state.activeLayerIdx++;
  activateUL(state.activeLayerIdx);
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast('Calque réordonné');
}

// Batch version of reorderLayer() — moves every index in `fromIndices` as a
// single contiguous block to wherever `toIdx` currently sits, preserving
// their relative order. Fixes "impossible de réordonner par lot": the
// single-item drag (reorderLayer above) only ever knew about the one row
// literally under the mouse at mousedown, ignoring the rest of a multi-
// selection entirely.
function reorderLayersBatch(fromIndices,toIdx){
  fromIndices=fromIndices.slice().sort(function(a,b){return a-b;});
  if(!fromIndices.length||fromIndices.indexOf(toIdx)>=0)return;
  saveAllLayerFrames();pushUndoLayers();
  var destLd=state.layers[toIdx];
  var activeLd=state.layers[state.activeLayerIdx];
  var movedLd=[],movedUL=[];
  for(var i=fromIndices.length-1;i>=0;i--){
    var idx=fromIndices[i];
    movedLd.unshift(state.layers.splice(idx,1)[0]);
    movedUL.unshift(userLayers.splice(idx,1)[0]);
  }
  var destNewIdx=state.layers.indexOf(destLd);
  if(destNewIdx<0)destNewIdx=state.layers.length; // dest was itself moved (shouldn't happen, guarded above) — append as fallback
  Array.prototype.splice.apply(state.layers,[destNewIdx,0].concat(movedLd));
  Array.prototype.splice.apply(userLayers,[destNewIdx,0].concat(movedUL));
  userLayers.forEach(function(l){l.insertBelow(arcLayer);});
  var newActiveIdx=state.layers.indexOf(activeLd);
  state.activeLayerIdx=newActiveIdx>=0?newActiveIdx:0;
  activateUL(state.activeLayerIdx);
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast('Calques réordonnés');
}
function drawStage(){stageLayer.removeChildren();stageLayer.activate();
  new Path.Rectangle({point:[0,0],size:[state.canvasW,state.canvasH],fillColor:state.canvasBg,strokeColor:new Color(0,0,0,.08),strokeWidth:1});
  userLayers[state.activeLayerIdx].activate();}
drawStage();
function fitCanvas(){var vw=view.viewSize.width,vh=view.viewSize.height;var z=Math.min(vw*.85/state.canvasW,vh*.85/state.canvasH,3);view.zoom=z;view.center=new Point(state.canvasW/2,state.canvasH/2);updZoom();}
setTimeout(fitCanvas,50);
function resetView(){view.zoom=1;view.center=new Point(state.canvasW/2,state.canvasH/2);updZoom();}
function updZoom(){document.getElementById('zoom-ind').textContent=Math.round(view.zoom*100)+'%';}

function serP(p){var isVB=!!(p.data&&p.data.isVectorBrush);var center=isVB&&p.data.centerSegments?p.data.centerSegments:undefined;
  var widthProfile=isVB&&p.data.widthProfile?p.data.widthProfile:undefined;
  var fillSeed=(p.data&&p.data.fillSeed)?p.data.fillSeed:undefined,fillGapPx=(p.data&&p.data.fillGapPx!==undefined)?p.data.fillGapPx:undefined;
  var fillWalls=(p.data&&p.data.fillWalls&&p.data.fillWalls.length)?p.data.fillWalls:undefined,strokeId=(p.data&&p.data.strokeId)?p.data.strokeId:undefined;
  return{segments:p.segments.map(function(s){return{point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};}),strokeColor:isVB?null:(p.strokeColor?p.strokeColor.toCSS(true):'#ffffff'),strokeWidth:p.strokeWidth,strokeCap:p.strokeCap||'round',strokeJoin:p.strokeJoin||'round',miterLimit:p.miterLimit,fillColor:p.fillColor?p.fillColor.toCSS(true):null,opacity:p.opacity!==undefined?p.opacity:1,dashArray:(p.dashArray&&p.dashArray.length)?p.dashArray.slice():undefined,dashOffset:p.dashOffset,paintOrder:(p.data&&p.data.paintOrder)?p.data.paintOrder:undefined,isVectorBrush:isVB||undefined,centerSegments:center,widthProfile:widthProfile,fillSeed:fillSeed,fillGapPx:fillGapPx,fillWalls:fillWalls,strokeId:strokeId};}
function desP(d,layer,op){var prev=project.activeLayer;layer.activate();var p=new Path({insert:true});for(var i=0;i<d.segments.length;i++){var s=d.segments[i];p.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));}p.strokeColor=d.strokeColor||(d.isVectorBrush?null:'#fff');p.strokeWidth=d.strokeWidth||3;p.strokeCap=d.strokeCap||'round';p.strokeJoin=d.strokeJoin||'round';if(d.miterLimit!==undefined)p.miterLimit=d.miterLimit;if(d.fillColor)p.fillColor=d.fillColor;else p.fillColor=null;p.opacity=op!==undefined?op:(d.opacity!==undefined?d.opacity:1);if(d.dashArray&&d.dashArray.length)p.dashArray=d.dashArray;if(d.dashOffset!==undefined)p.dashOffset=d.dashOffset;if(d.paintOrder){p.data.paintOrder=d.paintOrder;}if(d.isVectorBrush){p.data.isVectorBrush=true;if(d.centerSegments)p.data.centerSegments=d.centerSegments;if(d.widthProfile)p.data.widthProfile=d.widthProfile;}if(d.fillSeed){p.data.fillSeed=d.fillSeed;p.data.fillGapPx=d.fillGapPx;}if(d.fillWalls)p.data.fillWalls=d.fillWalls;if(d.strokeId)p.data.strokeId=d.strokeId;prev.activate();return p;}
// Bitmap import (Section 6 gap): a Raster is stored by its dataURL (fully
// self-contained in the project JSON, no external file dependency after
// import) plus the position/size the user left it at — width/height are
// re-applied explicitly on load since Raster sources decode async and may
// briefly report their natural size instead of the saved one.
function serR(r){return{isRaster:true,src:r.data&&r.data.src?r.data.src:r.source,x:r.position.x,y:r.position.y,width:r.bounds.width,height:r.bounds.height,opacity:r.opacity!==undefined?r.opacity:1};}
function desR(d,layer,op){var prev=project.activeLayer;layer.activate();var r=new Raster(d.src);r.data.src=d.src;r.position=new Point(d.x,d.y);r.opacity=op!==undefined?op:(d.opacity!==undefined?d.opacity:1);var w=d.width,h=d.height;r.onLoad=function(){r.size=new Size(w,h);r.position=new Point(d.x,d.y);};prev.activate();return r;}

// ---- COMPONENTS / SYMBOLS ----
// A "component" is a layer whose content lives in its own mini-timeline
// (state.symbols[id] — same shape as the top-level project: layers/
// totalFrames/fps) instead of directly in state.layers[i].frames. Editing
// a component swaps the global state.layers/userLayers to point at the
// symbol's own data (see enterSymbol/exitToScene below) so every existing
// drawing/timeline/tween function keeps working unmodified inside it.
function resolveSymbolFrameIdx(sym,layer,mainFrameIdx){
  var elapsed=Math.max(0,(mainFrameIdx-(layer.symPlacedAt||0)))*(layer.symSpeed||1);
  var total=Math.max(1,sym.totalFrames);
  if(layer.symPlayMode==='single')return Math.min(total-1,Math.max(0,Math.floor(layer.symSingleFrame||0)));
  if(layer.symPlayMode==='once')return Math.min(total-1,Math.floor(elapsed));
  if(layer.symPlayMode==='pingpong'){
    if(total<2)return 0;
    var cycle=(total-1)*2;
    var pos=Math.floor(elapsed)%cycle;
    return pos<total?pos:cycle-pos;
  }
  return Math.floor(elapsed)%total;
}
// Resolves a single LFS (Ligne/Plein/Ombre) sub-timeline's effective strokes
// for one of the 3 sub-symbols, mirroring the symbolId branch above but
// keyed off ld.lfsIds[key]/ld.lfsSettings[key] instead of ld.symbolId/ld
// itself (resolveSymbolFrameIdx only reads playMode/speed/placedAt/singleFrame
// off its "layer" arg, so the settings sub-object satisfies it unmodified).
function getLFSSubStrokes(ld,key,frameIdx){
  var symId=ld.lfsIds&&ld.lfsIds[key];if(!symId)return[];
  var sym=state.symbols[symId];if(!sym)return[];
  var symLayer=sym.layers[0];if(!symLayer)return[];
  var ii=resolveSymbolFrameIdx(sym,ld.lfsSettings[key],frameIdx);
  var sf=symLayer.frames[ii];if(!sf)return[];
  if(sf.isKeyframe||sf.isInterpolated)return sf.strokes;
  for(var k=ii-1;k>=0;k--){if(symLayer.frames[k].isKeyframe)return symLayer.frames[k].strokes;}
  return[];
}
function getEffectiveStrokes(layerIdx,frameIdx){
  var ld=state.layers[layerIdx];if(!ld)return[];
  if(ld.lfsGroup){
    // Traditional cel stacking: shadow at back, full (flat colors) in the
    // middle, line (ink) on top.
    return getLFSSubStrokes(ld,'shadow',frameIdx)
      .concat(getLFSSubStrokes(ld,'full',frameIdx))
      .concat(getLFSSubStrokes(ld,'line',frameIdx));
  }
  if(ld.symbolId){
    var sym=state.symbols[ld.symbolId];if(!sym)return[];
    var symLayer=sym.layers[0];if(!symLayer)return[];
    var ii=resolveSymbolFrameIdx(sym,ld,frameIdx);
    var sf=symLayer.frames[ii];if(!sf)return[];
    if(sf.isKeyframe||sf.isInterpolated)return sf.strokes;
    for(var k=ii-1;k>=0;k--){if(symLayer.frames[k].isKeyframe)return symLayer.frames[k].strokes;}
    return[];
  }
  var f=ld.frames[frameIdx];if(!f)return[];
  if(f.isKeyframe||f.isInterpolated)return f.strokes;
  for(var i=frameIdx-1;i>=0;i--){if(ld.frames[i].isKeyframe)return ld.frames[i].strokes;}
  return [];
}
var _symbolPaperLayers={},_sceneSnapshot=null;
function ensureSymbolPaperLayers(symId){
  if(_symbolPaperLayers[symId])return _symbolPaperLayers[symId];
  var sym=state.symbols[symId];var arr=[];
  arcLayer.activate();
  sym.layers.forEach(function(ld,idx){var l=new Layer({name:'sym-'+symId+'-'+idx});l.insertBelow(arcLayer);l.visible=false;arr.push(l);});
  _symbolPaperLayers[symId]=arr;
  return arr;
}
function genSymbolId(){return 'sym_'+Date.now()+'_'+Math.floor(Math.random()*10000);}
function convertLayerToComponent(layerIdx){
  if(state.activeSymbolId){showToast('Fermez d\'abord le composant en cours d\'édition');return;}
  var ld=state.layers[layerIdx];if(!ld||ld.symbolId){showToast('Déjà un composant ou calque invalide');return;}
  saveAllLayerFrames();pushUndo();
  var symId=genSymbolId();
  var symLayer={name:'Layer 1',visible:true,locked:false,frames:JSON.parse(JSON.stringify(ld.frames))};
  state.symbols[symId]={name:ld.name+' (Comp)',totalFrames:state.totalFrames,fps:state.fps,layers:[symLayer]};
  ld.symbolId=symId;ld.locked=true;ld.symPlayMode='loop';ld.symSpeed=1;ld.symPlacedAt=0;ld.symSingleFrame=0;
  ld.frames=[];for(var i=0;i<state.totalFrames;i++)ld.frames.push({strokes:[],isKeyframe:i===0,isInterpolated:false});
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();if(window.renderSymbolTabs)renderSymbolTabs();
  showToast('Composant créé: '+state.symbols[symId].name);
}
// Batch version: merges every selected layer into ONE new component (a
// symbol whose `layers` array — already supported, symbols aren't limited
// to a single sub-layer — gets one symLayer per selected source layer,
// preserving their relative stacking order), replacing the whole selected
// block with a single component-layer in the parent, positioned where the
// block used to be. "select plusieurs layers puis clic composant" from the
// single-layer version leaving the rest of the selection untouched and
// creating N separate one-layer components instead of one N-layer component.
function convertLayersToComponent(indices){
  if(state.activeSymbolId){showToast('Fermez d\'abord le composant en cours d\'édition');return;}
  indices=indices.slice().sort(function(a,b){return a-b;}).filter(function(i){return state.layers[i]&&!state.layers[i].symbolId;});
  if(indices.length<2){convertLayerToComponent(indices[0]!==undefined?indices[0]:state.activeLayerIdx);return;}
  saveAllLayerFrames();pushUndo();
  var symId=genSymbolId();
  var symLayers=indices.map(function(i){return{name:state.layers[i].name,visible:state.layers[i].visible,locked:false,frames:JSON.parse(JSON.stringify(state.layers[i].frames))};});
  state.symbols[symId]={name:'Composant',totalFrames:state.totalFrames,fps:state.fps,layers:symLayers};
  var insertAt=indices[0];
  for(var k=indices.length-1;k>=0;k--){state.layers.splice(indices[k],1);userLayers.splice(indices[k],1);}
  var newLd={name:'Composant',symbolId:symId,locked:true,visible:true,symPlayMode:'loop',symSpeed:1,symPlacedAt:0,symSingleFrame:0,
    frames:[]};
  for(var i=0;i<state.totalFrames;i++)newLd.frames.push({strokes:[],isKeyframe:i===0,isInterpolated:false});
  var newUl=new Layer({name:'layer-'+state.layers.length});
  state.layers.splice(insertAt,0,newLd);
  userLayers.splice(insertAt,0,newUl);
  userLayers.forEach(function(l){l.insertBelow(arcLayer);});
  state.activeLayerIdx=insertAt;_layerSel=[insertAt];
  activateUL(state.activeLayerIdx);
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();if(window.renderSymbolTabs)renderSymbolTabs();
  showToast('Composant créé avec '+indices.length+' calques');
}
// Inverse of convertLayerToComponent: bakes what the component instance
// actually displays on each main-timeline frame (play mode, speed and
// single-frame settings included) back into plain layer keyframes, then
// detaches the layer from the symbol. Consecutive identical frames are
// collapsed into one keyframe + held frames, so a looping symbol becomes
// repeating keyframes rather than a keyframe on every single frame. The
// symbol itself stays in the library for any other instance.
function convertComponentToLayer(layerIdx){
  if(state.activeSymbolId){showToast('Fermez d\'abord le composant en cours d\'édition');return;}
  var ld=state.layers[layerIdx];if(!ld||!ld.symbolId){showToast('Pas un composant');return;}
  if(!state.symbols[ld.symbolId]){showToast('Composant introuvable');return;}
  pushUndoLayers();
  var frames=[],prevJson=null;
  for(var fi=0;fi<state.totalFrames;fi++){
    var strokes=getEffectiveStrokes(layerIdx,fi);
    var j=JSON.stringify(strokes);
    if(j!==prevJson){frames.push({strokes:JSON.parse(j),isKeyframe:true,isInterpolated:false});prevJson=j;}
    else frames.push({strokes:[],isKeyframe:false,isInterpolated:false});
  }
  delete ld.symbolId;delete ld.symPlayMode;delete ld.symSpeed;delete ld.symPlacedAt;delete ld.symSingleFrame;
  ld.locked=false;ld.frames=frames;
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();if(window.renderSymbolTabs)renderSymbolTabs();
  showToast('Composant décomposé en calque normal');
}
// Groups a plain layer into 3 independent sub-timelines (Ligne/Plein/Ombre),
// each modeled as its own state.symbols entry so enterSymbol/exitToScene/
// getEffectiveStrokes' existing symbolId machinery is reused verbatim — see
// getLFSSubStrokes above. The layer's current content becomes the "Plein"
// (Full/flat-color) pass by default since that's normally the bulk of the
// linework already on a layer being grouped; Ligne/Ombre start empty.
function convertLayerToLFSGroup(layerIdx){
  if(state.activeSymbolId){showToast('Fermez d\'abord le composant en cours d\'édition');return;}
  var ld=state.layers[layerIdx];if(!ld||ld.symbolId||ld.lfsGroup){showToast('Calque déjà groupé ou invalide');return;}
  saveAllLayerFrames();pushUndo();
  var lfsIds={},lfsSettings={};
  ['line','full','shadow'].forEach(function(key){
    var symId=genSymbolId();
    var frames;
    if(key==='full')frames=JSON.parse(JSON.stringify(ld.frames));
    else{frames=[];for(var i=0;i<state.totalFrames;i++)frames.push({strokes:[],isKeyframe:i===0,isInterpolated:false});}
    var symLayer={name:'Layer 1',visible:true,locked:false,frames:frames};
    var label={line:'Ligne',full:'Plein',shadow:'Ombre'}[key];
    state.symbols[symId]={name:ld.name+' ('+label+')',totalFrames:state.totalFrames,fps:state.fps,layers:[symLayer]};
    lfsIds[key]=symId;
    lfsSettings[key]={symPlayMode:'loop',symSpeed:1,symPlacedAt:0,symSingleFrame:0};
  });
  ld.lfsGroup=true;ld.lfsIds=lfsIds;ld.lfsSettings=lfsSettings;ld.locked=true;
  ld.frames=[];for(var i=0;i<state.totalFrames;i++)ld.frames.push({strokes:[],isKeyframe:i===0,isInterpolated:false});
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();if(window.renderSymbolTabs)renderSymbolTabs();
  showToast('Calque groupé en Ligne/Plein/Ombre');
}
// Inverse: bakes the composited Ligne+Plein+Ombre result of each main-timeline
// frame (via getEffectiveStrokes, which already stacks all 3 sub-symbols)
// back into plain layer keyframes, then detaches the 3 sub-symbols. Mirrors
// convertComponentToLayer's collapse-identical-frames logic.
function convertLFSGroupToLayer(layerIdx){
  if(state.activeSymbolId){showToast('Fermez d\'abord le composant en cours d\'édition');return;}
  var ld=state.layers[layerIdx];if(!ld||!ld.lfsGroup){showToast('Pas un groupe Ligne/Plein/Ombre');return;}
  pushUndoLayers();
  var frames=[],prevJson=null;
  for(var fi=0;fi<state.totalFrames;fi++){
    var strokes=getEffectiveStrokes(layerIdx,fi);
    var j=JSON.stringify(strokes);
    if(j!==prevJson){frames.push({strokes:JSON.parse(j),isKeyframe:true,isInterpolated:false});prevJson=j;}
    else frames.push({strokes:[],isKeyframe:false,isInterpolated:false});
  }
  delete ld.lfsGroup;delete ld.lfsIds;delete ld.lfsSettings;
  ld.locked=false;ld.frames=frames;
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();if(window.renderSymbolTabs)renderSymbolTabs();
  showToast('Groupe Ligne/Plein/Ombre décomposé en calque normal');
}
// Propagation engine: takes whichever Plein/Ombre keyframes the user already
// painted by hand (identified by data.fillSeed, exactly like
// fillRegenerateLinked's own linked-fill detection) and repeats them onto
// every OTHER main-timeline frame of that sub-timeline, re-running
// fillVectorFind against THAT frame's Ligne geometry — so it tracks the
// shape rather than copying literal geometry, which is what makes it work
// across interpolated/tweened Ligne frames too (getLFSSubStrokes already
// resolves those through the normal held-keyframe/tween machinery).
// Frames that already have their own hand-painted fillSeed strokes are never
// overwritten — those are the sources, not propagation targets.
// Bounding box straight from raw stroke JSON (segment anchor points only,
// ignoring bezier handles — an approximation, but enough to remap a seed
// point's relative position when the Ligne shape has translated/scaled
// between frames, e.g. under a motion tween).
function _lineBoundsFromStrokes(strokes){
  var minX=Infinity,minY=Infinity,maxX=-Infinity,maxY=-Infinity;
  strokes.forEach(function(sd){
    if(!sd.segments)return;
    sd.segments.forEach(function(s){
      var x=s.point[0],y=s.point[1];
      if(x<minX)minX=x;if(x>maxX)maxX=x;if(y<minY)minY=y;if(y>maxY)maxY=y;
    });
  });
  if(minX===Infinity)return null;
  return{x:minX,y:minY,width:maxX-minX,height:maxY-minY};
}
function propagateLFSFill(layerIdx,which){
  if(state.activeSymbolId){showToast('Fermez d\'abord le composant en cours d\'édition');return;}
  var ld=state.layers[layerIdx];if(!ld||!ld.lfsGroup){showToast('Pas un groupe Ligne/Plein/Ombre');return;}
  var targetSymId=ld.lfsIds[which];var targetSym=targetSymId&&state.symbols[targetSymId];
  if(!targetSym){showToast('Sous-timeline introuvable');return;}
  var targetLayer=targetSym.layers[0];
  var sourceFrames={};
  targetLayer.frames.forEach(function(fr,fi){
    if(!fr.isKeyframe)return;
    var items=fr.strokes.filter(function(sd){return sd.fillSeed;});
    if(!items.length)return;
    // Capture each source fill's seed as a FRACTION of the Ligne shape's own
    // bounding box at that frame, not as an absolute point — a literal
    // absolute point stops landing inside the shape the moment it moves
    // (e.g. a tweened walk cycle), which silently starves propagation at
    // every frame except the one where the shape happens to still overlap
    // the original click.
    var lineBounds=_lineBoundsFromStrokes(getLFSSubStrokes(ld,'line',fi));
    items.forEach(function(it){
      if(lineBounds&&lineBounds.width>0&&lineBounds.height>0){
        it.fracX=(it.fillSeed[0]-lineBounds.x)/lineBounds.width;
        it.fracY=(it.fillSeed[1]-lineBounds.y)/lineBounds.height;
      }
    });
    sourceFrames[fi]=items;
  });
  var sourceFis=Object.keys(sourceFrames).map(Number).sort(function(a,b){return a-b;});
  if(!sourceFis.length){showToast('Aucun remplissage source — peignez au moins un '+(which==='full'?'Plein':'Ombre')+' avec le Pot de peinture d\'abord');return;}
  saveAllLayerFrames();pushUndoLayers();
  var scratch=new Layer({name:'lfs-propagate-scratch'});scratch.visible=false;
  var newFrames=targetLayer.frames.slice();
  var applied=0;
  for(var tfi=0;tfi<state.totalFrames;tfi++){
    var bestFi=null,bestDist=Infinity;
    sourceFis.forEach(function(sfi){var dd=Math.abs(sfi-tfi);if(dd<bestDist){bestDist=dd;bestFi=sfi;}});
    if(bestFi===null||bestFi===tfi)continue;
    var lineStrokes=getLFSSubStrokes(ld,'line',tfi);
    if(!lineStrokes.length)continue;
    var targetBounds=_lineBoundsFromStrokes(lineStrokes);
    scratch.removeChildren();
    lineStrokes.forEach(function(sd){desP(sd,scratch,1);});
    var strokeDatas=[];
    sourceFrames[bestFi].forEach(function(it){
      var seedX=it.fillSeed[0],seedY=it.fillSeed[1];
      if(it.fracX!==undefined&&targetBounds&&targetBounds.width>0&&targetBounds.height>0){
        seedX=targetBounds.x+it.fracX*targetBounds.width;
        seedY=targetBounds.y+it.fracY*targetBounds.height;
      }
      var seedPt=new Point(seedX,seedY);
      var res=fillVectorFind(seedPt,scratch,null,it.fillGapPx);
      if(!res)return;
      res.path.fillColor=it.fillColor;res.path.strokeColor=null;res.path.opacity=it.opacity!==undefined?it.opacity:1;
      res.path.data.fillSeed=[seedPt.x,seedPt.y];res.path.data.fillGapPx=res.gapPx;
      strokeDatas.push(serP(res.path));res.path.remove();
    });
    if(!strokeDatas.length)continue;
    newFrames[tfi]={strokes:strokeDatas,isKeyframe:true,isInterpolated:false};
    applied++;
  }
  scratch.remove();
  targetLayer.frames=newFrames;
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
  showToast(applied+' image(s) '+(which==='full'?'Plein':'Ombre')+' généré(s) par propagation');
}
function enterSymbol(symId){
  if(state.activeSymbolId===symId)return;
  if(state.activeSymbolId){showToast('Composants imbriqués non supportés — fermez le composant courant');return;}
  if(!state.symbols[symId])return;
  saveAllLayerFrames();
  _sceneSnapshot={layers:state.layers,totalFrames:state.totalFrames,waIn:state.waIn,waOut:state.waOut,activeLayerIdx:state.activeLayerIdx,fps:state.fps,currentFrame:state.currentFrame,userLayers:userLayers};
  userLayers.forEach(function(l){l.opacity=0.25;});
  var sym=state.symbols[symId];
  var symPaperLayers=ensureSymbolPaperLayers(symId);
  symPaperLayers.forEach(function(l){l.visible=true;l.opacity=1;});
  state.layers=sym.layers;state.totalFrames=sym.totalFrames;state.waIn=0;state.waOut=sym.totalFrames-1;
  window._waIn=0;window._waOut=state.waOut;window._totalF=state.totalFrames;
  state.activeLayerIdx=0;state.currentFrame=0;state.fps=sym.fps||state.fps;
  userLayers=symPaperLayers;
  state.activeSymbolId=symId;
  if(state.openSymbolTabs.indexOf(symId)<0)state.openSymbolTabs.push(symId);
  activateUL(0);drawStage();loadFrame(0);renderOS();renderArcs();updateUI();if(window.renderSymbolTabs)renderSymbolTabs();
}
function exitToScene(){
  if(!state.activeSymbolId||!_sceneSnapshot)return;
  saveAllLayerFrames();
  var symId=state.activeSymbolId;
  // totalFrames/fps are primitives copied into state at enterSymbol() time,
  // not a live binding back to state.symbols[symId] — write them back now
  // so frame-count/fps edits made while inside the symbol aren't lost.
  var sym=state.symbols[symId];if(sym){sym.totalFrames=state.totalFrames;sym.fps=state.fps;}
  var symLayers=_symbolPaperLayers[symId];if(symLayers)symLayers.forEach(function(l){l.visible=false;});
  state.layers=_sceneSnapshot.layers;state.totalFrames=_sceneSnapshot.totalFrames;state.waIn=_sceneSnapshot.waIn;state.waOut=_sceneSnapshot.waOut;
  window._waIn=state.waIn;window._waOut=state.waOut;window._totalF=state.totalFrames;
  state.activeLayerIdx=_sceneSnapshot.activeLayerIdx;state.currentFrame=_sceneSnapshot.currentFrame;state.fps=_sceneSnapshot.fps;
  userLayers=_sceneSnapshot.userLayers;
  userLayers.forEach(function(l){l.opacity=1;});
  state.activeSymbolId=null;_sceneSnapshot=null;
  activateUL(state.activeLayerIdx);drawStage();loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();if(window.renderSymbolTabs)renderSymbolTabs();
}
function closeSymbolTab(symId){
  if(state.activeSymbolId===symId)exitToScene();
  var idx=state.openSymbolTabs.indexOf(symId);if(idx>=0)state.openSymbolTabs.splice(idx,1);
  if(window.renderSymbolTabs)renderSymbolTabs();
}

// Cheap "did anything change" signal for engine-bridge.js's tick() loop,
// which otherwise has no way to know whether it's worth paying for a full
// buildSceneJson() rebuild (walks every live Paper.js item on every layer)
// without either rebuilding it anyway just to diff it, or missing a real
// edit. saveActiveLayerFrame/loadFrame are called at the end of virtually
// every tool action AND on every frame/layer navigation — the two
// checkpoints together bracket everything that can change what's on screen
// OUTSIDE of an active drag (every bridge already suspends tick() for the
// duration of a drag itself and renders its own live overlay directly, so
// tick()'s diffing only ever matters in the resting state between
// gestures, which these two functions fully cover).
window._sceneVersion=0;
function saveActiveLayerFrame(){
  window._sceneVersion++;
  var ld=state.layers[state.activeLayerIdx];if(ld.symbolId)return;
  var f=ld.frames[state.currentFrame];
  if(!f.isKeyframe&&!f.isInterpolated)return;
  var strokes=[];userLayers[state.activeLayerIdx].children.forEach(function(c){if(c instanceof Path&&c.segments.length>0)strokes.push(serP(c));else if(c instanceof Raster)strokes.push(serR(c));});
  // saveActiveLayerFrame also runs on plain navigation (goToFrame calls it
  // to persist whatever's on screen before switching away) — flagging
  // "manually edited" just because this ran would mark every tween frame
  // green the moment you scrub past it. Only a REAL content change counts.
  if(f.isInterpolated&&JSON.stringify(strokes)!==JSON.stringify(f.strokes))f.isManualEdit=true;
  f.strokes=strokes;
}
function saveAllLayerFrames(){
  for(var i=0;i<state.layers.length;i++){if(state.layers[i].symbolId)continue;var f=state.layers[i].frames[state.currentFrame];if(!f||(!f.isKeyframe&&!f.isInterpolated))continue;
  var strokes=[];userLayers[i].children.forEach(function(c){if(c instanceof Path&&c.segments.length>0)strokes.push(serP(c));else if(c instanceof Raster)strokes.push(serR(c));});
  if(f.isInterpolated&&JSON.stringify(strokes)!==JSON.stringify(f.strokes))f.isManualEdit=true;
  f.strokes=strokes;}
}
function loadFrame(idx){
  window._sceneVersion++;
  for(var i=0;i<state.layers.length;i++){userLayers[i].removeChildren();if(!state.layers[i].visible)continue;
  var strokes=getEffectiveStrokes(i,idx);strokes.forEach(function(sd){if(sd.isRaster)desR(sd,userLayers[i],1);else desP(sd,userLayers[i],1);});}
  userLayers[state.activeLayerIdx].activate();
}
function goToFrame(idx){
  if(idx<0||idx>=state.totalFrames)return;
  saveAllLayerFrames();state.currentFrame=idx;window._curFrame=idx;
  loadFrame(idx);if(!state.playing){renderOS();renderArcs();updateUI();}else{updatePlayhead();}
}

function insertFrame(){pushUndoLayers();var cf=state.currentFrame;for(var i=0;i<state.layers.length;i++)state.layers[i].frames.splice(cf+1,0,{strokes:[],isKeyframe:false,isInterpolated:false});state.totalFrames++;if(state.waOut<state.totalFrames-1)state.waOut++;window._waOut=state.waOut;window._totalF=state.totalFrames;goToFrame(cf+1);showToast('Frame insérée (F5)');}
function insertKeyframe(){insertKeyframeAt(state.activeLayerIdx,state.currentFrame);}
// Same as insertKeyframe() but for an arbitrary layer/frame instead of only
// the active layer at the current playhead — used to shorten a held/
// extended keyframe's span by dropping a new keyframe partway through it
// (the frame grid's small drag handle on the span's end tick), since hold
// spans have no stored length of their own: they're purely derived by
// scanning forward to wherever the next real keyframe happens to be
// (renderTimeline() in timeline.js), so "shrinking" one is really "insert a
// keyframe earlier, which becomes the new boundary".
function insertKeyframeAt(layerIdx,frameIdx){
  saveAllLayerFrames();
  var ld=state.layers[layerIdx];var f=ld.frames[frameIdx];
  if(f.isKeyframe){showToast('Déjà une keyframe');return false;}
  pushUndoLayers();
  f.strokes=JSON.parse(JSON.stringify(getEffectiveStrokes(layerIdx,frameIdx)));
  f.isKeyframe=true;f.isInterpolated=false;
  if(layerIdx===state.activeLayerIdx)loadFrame(state.currentFrame);
  renderOS();renderArcs();updateUI();showToast('Keyframe insérée');
  return true;
}
function insertBlankKeyframe(){saveAllLayerFrames();pushUndoLayers();state.layers[state.activeLayerIdx].frames[state.currentFrame]={strokes:[],isKeyframe:true,isInterpolated:false};loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast('Blank keyframe (F7)');}
function removeFrame(){if(state.totalFrames<=1)return;pushUndoLayers();var cf=state.currentFrame;for(var i=0;i<state.layers.length;i++)state.layers[i].frames.splice(cf,1);state.totalFrames--;if(state.waOut>=state.totalFrames)state.waOut=state.totalFrames-1;window._waOut=state.waOut;window._totalF=state.totalFrames;if(state.currentFrame>=state.totalFrames)state.currentFrame=state.totalFrames-1;loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast('Frame supprimée');}
// Animate's "Clear Keyframe" — demotes a keyframe back into a plain
// extended frame (content reverts to whatever the previous keyframe holds),
// without removing the frame slot itself (unlike removeFrame/removeFrameSpan).
function clearKeyframe(){
  var ld=state.layers[state.activeLayerIdx];var cf=state.currentFrame;var f=ld.frames[cf];
  if(!f||!f.isKeyframe){showToast('Pas une keyframe');return;}
  pushUndo();f.strokes=[];f.isKeyframe=false;f.isInterpolated=false;
  loadFrame(cf);renderOS();renderArcs();updateUI();showToast('Keyframe effacée');
}
// Animate's "Convert to Keyframes" — bakes whatever's currently displayed
// (a tweened inbetween, or content inherited from an earlier keyframe) into
// an independent real keyframe on every frame in the selection (or just the
// current frame if nothing's selected), so it can be hand-adjusted without
// disturbing the rest of the tween.
function convertToKeyframes(){
  var li=state.activeLayerIdx;var ld=state.layers[li];
  var frames=_sel.frames.length?_sel.frames.filter(function(s){return s.layer===li;}).map(function(s){return s.frame;}):[state.currentFrame];
  if(!frames.length){showToast('Aucune sélection');return;}
  pushUndo();saveAllLayerFrames();var count=0;
  frames.forEach(function(fi){
    var f=ld.frames[fi];if(!f||f.isKeyframe)return;
    f.strokes=JSON.parse(JSON.stringify(getEffectiveStrokes(li,fi)));f.isKeyframe=true;f.isInterpolated=false;count++;
  });
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast(count+' image(s) clé(s) créée(s)');
}
// Animate's "Remove Frames" on a range selection — removes the whole
// selected frame span (across every layer, since frame count is global
// here) rather than clearing content in place like deleteSelectedFrames.
function removeFrameSpan(){
  var b=selBounds();
  if(!b){removeFrame();return;}
  var count=b.maxF-b.minF+1;
  if(state.totalFrames-count<1){showToast('Il faut garder au moins 1 frame');return;}
  pushUndoLayers();
  for(var i=0;i<state.layers.length;i++)state.layers[i].frames.splice(b.minF,count);
  state.totalFrames-=count;
  if(state.waOut>=state.totalFrames)state.waOut=state.totalFrames-1;window._waOut=state.waOut;window._totalF=state.totalFrames;
  if(state.currentFrame>=state.totalFrames)state.currentFrame=state.totalFrames-1;
  selClear();loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast(count+' frame(s) supprimée(s)');
}
// Duplicate the current frame selection in place, right after itself —
// equivalent to copy + paste-at-selection-end, exposed as one menu action.
function duplicateSelectedFrames(){
  var b=selBounds();
  if(!b){showToast('Aucune sélection');return;}
  window.SM.copyFrames();
  var span=b.maxF-b.minF+1;
  state.currentFrame=b.minF+span;window._curFrame=state.currentFrame;
  window.SM.pasteFrames();
}

