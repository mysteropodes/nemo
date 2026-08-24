// One-time localStorage key migration: StrokeMotion -> Nemo rename. Copies
// every existing 'sm-'/'sm_' key forward to its 'nemo-'/'nemo_' equivalent
// (profile, recents, shortcuts, feedback, sync folders, etc.) so existing
// local data isn't silently orphaned by the rename — old keys are left in
// place (untouched, not deleted) rather than removed, so this stays safe
// to run every load even after migration already happened once. Must run
// before ANYTHING else touches localStorage (initUserProfile/i18n's
// language load both do, at parse time on the very next scripts) — hence
// first line of the first app script, before even paper.install.
(function migrateStorageKeysToNemo(){
  try{
    var keys=[];
    for(var i=0;i<localStorage.length;i++)keys.push(localStorage.key(i));
    keys.forEach(function(k){
      if(k.indexOf('sm-')!==0&&k.indexOf('sm_')!==0)return;
      var nk=k.replace(/^sm[-_]/,function(m){return m[2]==='-'?'nemo-':'nemo_';});
      if(localStorage.getItem(nk)===null)localStorage.setItem(nk,localStorage.getItem(k));
    });
  }catch(e){}
})();
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

// Both mirror the timeline's CSS custom props (--fc, and .lrow/.frow/.fc's
// height) — every click/drag position in timeline.js is computed from
// these, so a mismatch here silently desyncs ALL timeline pointer math from
// what's actually on screen (confirmed real bug: FC was left at the old
// 14 after --fc moved to 16 in a redesign pass, producing a growing
// left-right drift the further right you clicked/dragged). Keep these two
// in lockstep with style.css by hand — there's no single source of truth
// shared between CSS and JS in this codebase.
var FC=30;// static fallback matching timeline-zoom.js's DEFAULT_FC — overwritten on load by that module's init() (persisted value or this same default), kept in sync to avoid a flash-of-wrong-zoom before that script runs
var ROW_H=34;
var state={
  totalFrames:24,currentFrame:0,fps:12,playing:false,loopPlayback:true,pingPongPlayback:false,playDir:1,
  tool:'draw',
  appMode:'anim2d', // 'anim2d' | 'motion' | 'storyboard' — see #app-mode-switch (index.html) / motion.js
  // Off by default (feedback 2026-07: "désactive l'onion skin par default")
  // — #btn-os's own static HTML class updated to match (index.html), so
  // there's no flash of an "active" toolbar icon before this state loads.
  onionSkin:false,onionPrevOpacity:30,onionNextOpacity:30,onionMode:'tinted',
  onionIn:0,onionOut:23,
  brushSize:3,fillBrushSize:40,strokeColor:'#000000',fillColor:'#ff0000',fillEnabled:true,strokeEnabled:true,shadowMode:false,brushPreset:'none',
  smoothing:10,stabilizer:2,strokeCap:'round',strokeJoin:'round',opacity:100,
  miterLimit:10,dashOffset:0,paintOrder:'fillFirst',
  // 2026-07 feedback ("dans Moho je peux commencer un trait tout fin et
  // finir tout fin ou finir gros et inversement, en fonction de la
  // pression"): the plain Draw tool used to ignore stylus pressure for
  // width entirely unless this checkbox was manually enabled every
  // session, and even then P.min=30 meant a light touch never got
  // genuinely thin. Pressure-driven width is now the Draw tool's default
  // feel (matching Moho/other pro apps out of the box); P.min dropped to
  // 5 so a feather-light touch can taper close to a point instead of
  // floor-ing at 30% width. Purely a default — both remain user-editable
  // (checkbox + P.min field, right panel) same as before.
  strokeStyle:'solid',vectorBrush:true,taperEnds:false,
  drawMode:'front',fillBrushMode:'below',pressureMin:5,pressureMax:170,pressureInvert:false,pressureCurve:'linear',
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
  canvasW:1920,canvasH:1080,canvasBg:'#ffffff',canvasClip:false,safetyZones:false,ghostAllFrames:false,currentFrameOutline:false,
  // Multi-palette swatch library (Shade-for-AE-style, palette-panel.js) —
  // an array (not a map) so tab order IS display order, no separate sort
  // field needed. colorPalette (flat array) is kept as a legacy field ONLY
  // for reading pre-multi-palette project files — see importJSON's own
  // migration comment; never written to going forward.
  palettes:[{id:'p0',name:'Palette 1',colors:['#000000','#ffffff','#ff0000','#ff8800','#ffee00','#00cc44','#0088ff','#8833ff']}],
  activePaletteIdx:0,
  // Shadow Brush (2026-07, shadow-brush-bridge.js) — a dedicated guide-line
  // brush for the existing Stroke/Fill/Shadow layer-separation workflow
  // (state.shadowMode/data.channelTag='shadow', already wired in
  // Draw/Pen/Shape's commit paths). Distinct from the main color palette
  // above: each swatch carries a stable `id` (not just a hex value) so a
  // stroke can be tagged data.shadowSwatchId and stay attributable to "this
  // named shadow bucket" even if the swatch's own color is edited later.
  // 6 defaults — bright/saturated (2026-07: the original muted tones read
  // as too close to ordinary linework at a glance; a guide line's whole
  // point is to stand out unmistakably from the real drawing) — same vivid
  // rainbow family as the main palette's own defaults just above, so it
  // reads as "this app's vivid-color convention", not a separate look.
  // User-extendable via the "+" swatch in the popover.
  shadowPalette:[{id:'sh1',color:'#ff3355'},{id:'sh2',color:'#ff8800'},{id:'sh3',color:'#ffdd00'},{id:'sh4',color:'#22cc55'},{id:'sh5',color:'#2288ff'},{id:'sh6',color:'#aa33ff'}],
  shadowActiveId:'sh1',
  // Live-view-only (like onionSkin, never persisted to the project file):
  // hides every data.channelTag==='shadow' item across ALL layers/
  // components at once, so the guide lines can be checked-off before a
  // final look/export without touching the actual document content.
  showShadowGuides:true,
  // Export-time equivalent, OFF by default — a shadow guide line's entire
  // purpose is to disappear once its enclosed area is filled (see the
  // module's own header comment), so a fresh project shouldn't need this
  // turned on just to get a clean render.
  exportIncludeShadowGuides:false,
  customBrushPresets:{}, // user-saved procedural brush presets, keyed by generated id — see brush-editor.js
  layers:[],activeLayerIdx:0,
  // Default tween easing (2026-07 fix, "le moteur tween gère mal des
  // intervalles"): the old default points {.42,0}/{.58,1} were CSS
  // cubic-bezier(.42,0,.58,1) CONTROL points pasted in as-is — but
  // evalPointsCurve (ui.js) treats points as ON-CURVE knots of a
  // Catmull-Rom-tangent spline, so that default actually meant "y stays 0
  // until x=.42, cliff to 1 by x=.58, flat after" plus spline under/
  // overshoot on the outer segments (measured live: eased t of -0.074 and
  // 1.074, and adjacent inbetweens jumping 0.02 -> 0.98). On any span with
  // several inbetweens the motion visibly parked, teleported mid-span,
  // then parked again. These are ui.js's own 'ease-in-out' PRESET knots —
  // the same curve family, expressed correctly for this evaluator.
  motionArcs:{},easingCurve:{points:[{x:0,y:0},{x:.3,y:.05},{x:.7,y:.95},{x:1,y:1}]},
  // v16: manual inbetween/tween reassignment — {layer+':'+fA+'-'+fB: [{aId,bId},...]}
  // forces autoMatch (tweens.js generateTweens) to pair a specific stroke
  // (by stable data.strokeId) on keyframe A with a specific stroke on
  // keyframe B, overriding the auto-matcher for that one pair when it
  // misidentifies correspondence (large shape change, size change).
  tweenOverrides:{},
  // Per-keyframe-pair easing override — {layer+':'+fA+'-'+fB: {points:[...]}}.
  // COMPLEMENTS state.easingCurve, never replaces it: a pair with no entry
  // here just falls back to the global curve (getEasingForPair, tweens.js).
  // Same on-curve-waypoint shape as easingCurve/motion's curvePoints, so
  // the one shared widget (_curveEditor.editTweenSeg, ui.js) edits it too.
  tweenEasing:{},
  // Timeline toggle (per-project, not per-layer) — show/hide the inline
  // mini-curve strip under every layer that has at least one tween span.
  // Purely a display preference; never affects generateTweens() output.
  showTweenCurves:false,
  selectedStrokeIndices:[],
  undoStack:[],redoStack:[],maxUndo:60,
  // Parallel to undoStack/redoStack (same indices, same push/pop/shift ops
  // — kept in sync by hand in tweens.js, never derived) — human-readable
  // {label,tool,frame,layer,t} describing each snapshot, for the History
  // Panel (history-panel.js). Session-only like the stacks themselves.
  undoLabels:[],redoLabels:[],
  // Session-only, deliberately NEVER included in exportJSON()'s field list
  // (see that function) — a rolling human-readable trail of what the user
  // actually DID, feeding the feedback/debug-log system (feedback-bridge.js)
  // so a bug report comes with real repro context instead of just a static
  // screenshot. Lives here (not its own module-local array) so it survives
  // exactly as long as undoStack/redoStack do and needs no separate reset
  // wiring on New/Open project.
  actionLog:[],
  waIn:0,waOut:23,
  isPanning:false,spaceDown:false,altDown:false,
  canvasRotation:0, // radians — stage/canvas rotation (Animate "Rotate Stage" style), independent of zoom/pan; only the Rust engine's viewport actually rotates, see engine-bridge.js syncViewport()
  // Perspective drawing guides (Sketchbook-style) — persist ACROSS tool
  // switches once enabled (matches Sketchbook: the grid/snap stays active
  // while you draw with Draw/Pen/Line, not just while the Perspective tool
  // itself is selected — only dragging a vanishing point needs that tool).
  // VPs are stored in WORLD coordinates so they pan/zoom/rotate with the
  // canvas like any other reference geometry, not screen-fixed.
  perspectiveEnabled:false,
  perspectiveMode:'2pt', // '1pt' | '2pt' | '3pt' | 'fisheye'
  perspectiveDensity:24, // guide lines radiating from each vanishing point
  perspectiveVPs:null, // lazily seeded to sane defaults for canvasW/H on first enable — see ensurePerspectiveVPs() in perspective-bridge.js
  // Symmetry drawing guide (Sketchbook-style, 2026-07) — same "persists
  // across tool switches once enabled, only DRAGGING the axis needs the
  // dedicated tool" convention as perspectiveEnabled above. Promoted from
  // the earlier Labs prototype (src/js/labs/symmetry-mirror.js, now
  // removed) into a real shipped feature — see symmetry-bridge.js.
  symmetryEnabled:false,
  symmetryMode:'y', // 'y' | 'x' | 'free' | 'radial'
  symmetryAxis:null, // {x1,y1,x2,y2} world coords, the mirror line's endpoints — y/x/free modes; lazily seeded, see ensureSymmetryAxis() in symmetry-bridge.js
  symmetryRadialCenter:null, // {x,y} world coords — radial (mandala) mode only
  symmetryRadialSectors:6,
  symmetryExtend:true, // true = a stroke may freely cross the axis (default); false = it's clipped the instant it crosses, so original+mirror never overlap at the fold
  audioTracks:[], // {name,dataB64,offsetFrames,volume,muted} — playback/waveform in audio-bridge.js
  xformAnchorKey:'mc', // rotate/scale pivot preset (9-dot widget) — tl/tc/tr/ml/mc/mr/bl/bc/br, see tools.js xformAnchorPoint
  xformAnchorCustom:null, // [x,y] world-space override from Alt+click (select-bridge.js) — takes priority over xformAnchorKey when set
  refMedia:null, // rotoscopy reference {type:'video'|'imageseq'|'image',...} — reference-bridge.js
  mediaLibrary:[], // {id,name,kind:'image'|'video',thumb,layerName} — browsable catalog of imports, media-library.js
  symbols:{},activeSymbolId:null,openSymbolTabs:[],
  activeMontageViewId:null, // StoryBoard montage currently entered (enterMontageView) — see app.js
  // Layer folders: purely organizational metadata, not a real tree — each
  // layer optionally carries ld.folderId pointing into this map. Keeping
  // state.layers/userLayers as the same flat, 1:1-indexed arrays every
  // other function in the app already assumes (loadFrame, buildSceneJson,
  // saveAllLayerFrames, reorderLayersBatch, undo/redo — dozens of call
  // sites) was the deciding factor over inserting real folder "layers" into
  // that array, which would have meant retrofitting folder-awareness into
  // every one of them at real risk of new bugs this late. A folder's
  // membership is just "however many CONSECUTIVE layers currently share
  // this folderId in render order" — group/ungroup only ever assigns or
  // clears that field, never reshuffles the array on its own.
  layerFolders:{},
  // Stroke/Fill/Shadow channel linking: deliberately NOT layerFolders — the
  // 3 generated layers must render and behave as ordinary independent rows
  // (own working eye/lock/solo, no collapse arrow, no aggregate header),
  // only their keyframe TIMING stays synced. ld.linkGroupId points here;
  // ld.channel ('stroke'|'fill'|'shadow') drives the auto-strip enforced in
  // saveActiveLayerFrame/saveAllLayerFrames (app.js).
  layerLinkGroups:{},
  // Team review (Phase 1): who's currently "you" on this install, and which
  // subset of ownership the canvas currently shows. userProfile is NOT part
  // of the project JSON (it's per-installation, like sm-shortcuts) — see
  // initUserProfile() right below, which loads/generates it from
  // localStorage independently of any project load/save.
  userProfile:null,
  revisionView:'all', // 'all' | 'mine' | 'revisions' — session-only, not persisted
  // Anchored comments: {id,x,y,frame,authorId,authorName,authorColor,text,
  // createdAt,resolved}. Canvas-space x/y (world coords, not screen), so
  // they stay pinned to the artwork through pan/zoom — same coordinate
  // space every stroke already lives in.
  comments:[],
};
// Local identity, one per installation — generated once, then stable across
// every project this machine ever opens. Only name/color are user-editable
// (Settings > Profil); id is internal and never shown.
function initUserProfile(){
  try{
    var saved=JSON.parse(localStorage.getItem('nemo-profile')||'null');
    if(saved&&saved.id){if(!saved.role)saved.role='animator';state.userProfile=saved;return;}
  }catch(e){}
  state.userProfile={id:'u_'+Date.now().toString(36)+'_'+Math.random().toString(36).slice(2,8),name:'Animateur',color:'#4a9eff',role:'animator'};
  try{localStorage.setItem('nemo-profile',JSON.stringify(state.userProfile));}catch(e){}
}
function saveUserProfile(){
  try{localStorage.setItem('nemo-profile',JSON.stringify(state.userProfile));}catch(e){}
}
initUserProfile();

var stageLayer=new Layer({name:'stage'});
var onionPrevLayer=new Layer({name:'onion-prev'});
var onionNextLayer=new Layer({name:'onion-next'});
var ghostAllLayer=new Layer({name:'ghost-all'});
var userLayers=[];
var arcLayer=new Layer({name:'arcs'});
var nodeLayer=new Layer({name:'nodes'});
var marqueeLayer=new Layer({name:'marquee'});
var xformLayer=new Layer({name:'xform'});

// Layer color label (redesign 2026-07-09) — every layer gets a color from
// this rotating palette at creation time (never an unset/gray placeholder,
// matching the mockup where every row already has a dot), overridable per
// layer via the color-picker popover wired in timeline.js renderLayerList.
var LAYER_COLOR_PALETTE=['#5FA875','#B49BE6','#F0A585','#E88BAE','#E7C74C','#EE7C58','#7EA0FA','#5FD1C4'];
function nextLayerColor(){return LAYER_COLOR_PALETTE[state.layers.length%LAYER_COLOR_PALETTE.length];}
function createUserLayer(name){
  arcLayer.activate();var l=new Layer({name:'user-'+userLayers.length});l.insertBelow(arcLayer);userLayers.push(l);
  var frames=[];for(var i=0;i<state.totalFrames;i++)frames.push({strokes:[],isKeyframe:i===0,isInterpolated:false});
  // layerUid: stable identity for parenting (motion.js's setLayerParent/
  // parentChainMats) — a raw array index would silently repoint at the
  // wrong layer the moment reorderLayer/delete/duplicate splices
  // state.layers. parentLayerUid stays null (no parent) until the user
  // picks one in the Motion properties panel.
  state.layers.push({id:state.layers.length,name:name,visible:true,locked:false,frames:frames,color:nextLayerColor(),layerUid:'ly_'+Date.now().toString(36)+'_'+Math.floor(Math.random()*1e6),parentLayerUid:null});
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
  saveAllLayerFrames();pushUndoLayers(true);
  var ld=state.layers.splice(fromIdx,1)[0];state.layers.splice(toIdx,0,ld);
  var ul=userLayers.splice(fromIdx,1)[0];userLayers.splice(toIdx,0,ul);
  userLayers.forEach(function(l){l.insertBelow(arcLayer);});
  if(state.activeLayerIdx===fromIdx)state.activeLayerIdx=toIdx;
  else if(fromIdx<state.activeLayerIdx&&toIdx>=state.activeLayerIdx)state.activeLayerIdx--;
  else if(fromIdx>state.activeLayerIdx&&toIdx<=state.activeLayerIdx)state.activeLayerIdx++;
  activateUL(state.activeLayerIdx);
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast(SM.t('toastLayerReordered'));
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
  saveAllLayerFrames();pushUndoLayers(true);
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
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast(SM.t('toastLayersReordered'));
}
// Gap-based counterpart used by the layer-panel drag affordance. A gap is
// unambiguous (0 = before the first row, layers.length = after the last),
// unlike a destination row which cannot distinguish its upper/lower edge.
// Keeping this translation beside the actual array mutation guarantees the
// blue insertion line and the committed order describe the same operation.
function reorderLayersAtGap(fromIndices,gapIdx){
  var moving=fromIndices.filter(function(i){return i>=0&&i<state.layers.length;})
    .filter(function(i,p,a){return a.indexOf(i)===p;}).sort(function(a,b){return a-b;});
  if(!moving.length)return;
  gapIdx=Math.max(0,Math.min(state.layers.length,Math.round(gapIdx)));
  var insertIdx=0;
  for(var i=0;i<gapIdx;i++)if(moving.indexOf(i)<0)insertIdx++;
  var activeLd=state.layers[state.activeLayerIdx];
  var movedLd=moving.map(function(i){return state.layers[i];});
  var movedUL=moving.map(function(i){return userLayers[i];});
  var keptLd=state.layers.filter(function(_l,i){return moving.indexOf(i)<0;});
  var keptUL=userLayers.filter(function(_l,i){return moving.indexOf(i)<0;});
  // Dropping back into the block's existing gap is a real no-op — avoid an
  // unnecessary undo entry and full scene rebuild.
  var same=moving.every(function(orig,j){return orig===insertIdx+j;});
  if(same)return;
  saveAllLayerFrames();pushUndoLayers(true);
  var nextLd=keptLd.slice(0,insertIdx).concat(movedLd,keptLd.slice(insertIdx));
  var nextUL=keptUL.slice(0,insertIdx).concat(movedUL,keptUL.slice(insertIdx));
  // Mutate the existing arrays instead of replacing them: inside a
  // Component, state.layers is deliberately aliased to sym.layers.
  Array.prototype.splice.apply(state.layers,[0,state.layers.length].concat(nextLd));
  Array.prototype.splice.apply(userLayers,[0,userLayers.length].concat(nextUL));
  userLayers.forEach(function(l){l.insertBelow(arcLayer);});
  var newActiveIdx=state.layers.indexOf(activeLd);
  state.activeLayerIdx=newActiveIdx>=0?newActiveIdx:0;
  activateUL(state.activeLayerIdx);
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast(SM.t(moving.length>1?'toastLayersReordered':'toastLayerReordered'));
}
function drawStage(){stageLayer.removeChildren();stageLayer.activate();
  new Path.Rectangle({point:[0,0],size:[state.canvasW,state.canvasH],fillColor:state.canvasBg,strokeColor:new Color(0,0,0,.08),strokeWidth:1});
  userLayers[state.activeLayerIdx].activate();}
drawStage();
// Found live (2026-07, user console dump from the real Tauri app — never
// reproduced in the Chrome-based browser preview, only WKWebView): a flood
// of "null is not an object (evaluating 'this._matrix.inverted()
// ._transformBounds')" on nearly every mouse move, fitCanvas call, and
// newProject. Root cause, confirmed against Paper.js's own source
// (paper-full.min.js): view.zoom=z with z===0 (or NaN) bakes a
// non-invertible (singular) view matrix — Matrix.inverted() on a singular
// matrix returns null by design, and getBounds() immediately dereferences
// that null. z is a ratio of view.viewSize to canvasW/H — exactly 0 when
// this fires before WKWebView's flex layout has given #canvas-area (and
// therefore the canvas) a real size, unlike Chrome which apparently
// settles layout fast enough that the pre-existing 50ms setTimeout below
// always masked the race there. Once zoom is stuck at 0 every OTHER
// screen<->project coordinate conversion breaks too (hit-testing, hover),
// which is why mouse events kept throwing long after the single fitCanvas
// call that caused it — not four separate bugs, one bad zoom value with a
// permanent blast radius until a later valid fitCanvas call overwrites it.
function fitCanvas(){
  var vw=view.viewSize.width,vh=view.viewSize.height;
  if(!(vw>0)||!(vh>0)){requestAnimationFrame(fitCanvas);return;}
  var z=Math.min(vw*.85/state.canvasW,vh*.85/state.canvasH,3);
  if(!(z>0)){requestAnimationFrame(fitCanvas);return;} // canvasW/H not set yet either — same defensive wait
  view.zoom=z;view.center=new Point(state.canvasW/2,state.canvasH/2);state.canvasRotation=0;updZoom();
}
setTimeout(fitCanvas,50);
function resetView(){view.zoom=1;view.center=new Point(state.canvasW/2,state.canvasH/2);state.canvasRotation=0;updZoom();}
function updZoom(){
  // v13: #zoom-ind (plain-text readout) is gone — the scrubbable pill below
  // is the only zoom display now, so this function only ever writes there.
  // Mirror into the canvas's own scrubbable zoom pill (v10) — skip while the
  // user is actively dragging/typing it themselves, same guard pattern used
  // for every other live-updated numeric field in this app (sp-x etc.), so
  // an in-progress edit never gets clobbered by a value echoed back from
  // Paper's own view.zoom.
  var zs=document.getElementById('zoom-scrub');
  if(zs&&document.activeElement!==zs)zs.value=Math.round(view.zoom*100);
}

// Paper's own Color#toCSS(true) always forces alpha to 1 (see its source:
// the `hex` flag short-circuits the alpha check), so a color with a real
// alpha channel silently came back fully opaque on every serialize —
// invisible in the UI (each path re-set path.fillColor from a live Color
// object, alpha intact in memory) until the NEXT save/load or onion-skin/
// tween rebuild round-tripped it through serP/desP, which is where it
// actually got flattened to opaque. toCSS(false)'s rgba(...) string still
// carries the real value; this reads it back out into an 8-digit hex
// (#rrggbbaa) — desP/Paper.js already parses that format natively (needs no
// desP change), and every other reader of these hex strings (engine-bridge's
// cssColorToRgba, draw/shape-bridge's hexToRgba) now checks for byte 4.
function colorHex8(c){
  if(!c)return null;
  if(c.alpha===undefined||c.alpha>=1)return c.toCSS(true);
  var rgba=c.toCSS(false).match(/[\d.]+/g);
  function h(n){return Math.max(0,Math.min(255,Math.round(n))).toString(16).padStart(2,'0');}
  return '#'+h(rgba[0])+h(rgba[1])+h(rgba[2])+h(parseFloat(rgba[3])*255);
}
// Writes an 8-digit hex (#rrggbbaa) to a native <input type="color"> the
// CLAUDE.md §2-documented way: dataset.hex8 carries the real alpha-inclusive
// value, .value gets the plain 6-digit color the input actually accepts.
// Every call site used to assign the full 8-digit string straight to
// .value too, "relying on" the input silently truncating it — it does
// apply the truncated color, but Chrome/WebKit ALSO logs a console warning
// on every single assignment ("The specified value ... does not conform to
// the required format"), which is what feedback #44 was actually seeing
// flood the console every time a stroke/fill color changed. One helper so
// the ~11 copies of this pattern (timeline.js, tools.js, viewtools-bridge.js)
// converge on the same fix instead of needing it repeated at each site.
function setHex8Input(el,hex){
  if(!el)return;
  el.value=(hex&&hex.length>7)?hex.slice(0,7):hex;
  el.dataset.hex8=hex;
}
// Copy-pasted across 13 call sites (select-bridge.js, tools.js, timeline.js)
// before this — the exclusion rule for "is this child a real, independently
// selectable stroke, or a rendering artifact of its anchor stroke" (linked-
// fill companions, brush-texture dab copies). One helper so a future sibling
// tag (CLAUDE.md's own example) can't be added to one call site and
// forgotten in the other twelve.
function isSelectablePathChild(c){return !(c.data&&(c.data.isLinkedFillCompanion||c.data.isBrushTextureCopy));}
// Pressure-brush ribbons are already the exact visible outline of the
// authored stroke. A second, darker Paper stroke around that outline read as
// an unintended double contour (feedback #52), especially after changing
// width/opacity. Keep this choke point because every rebuild/load calls it,
// but make its contract explicit: a pressure ribbon has no cosmetic keyline.
function applyBrushKeyline(p){
  if(!p.data||!p.data.isVectorBrush||p.data.isBrushTextureCopy||p.data.isFillShape)return;
  p.strokeColor=null;
}
function serP(p){var isVB=!!(p.data&&p.data.isVectorBrush);var center=isVB&&p.data.centerSegments?p.data.centerSegments:undefined;
  var widthProfile=isVB&&p.data.widthProfile?p.data.widthProfile:undefined;
  var fillSeed=(p.data&&p.data.fillSeed)?p.data.fillSeed:undefined,fillGapPx=(p.data&&p.data.fillGapPx!==undefined)?p.data.fillGapPx:undefined;
  var fillWalls=(p.data&&p.data.fillWalls&&p.data.fillWalls.length)?p.data.fillWalls:undefined,strokeId=(p.data&&p.data.strokeId)?p.data.strokeId:undefined;
  // fillSeeds (PLURAL — a same-color merge tracking multiple original click
  // points, tools.js's fillMergeSameColor) was never round-tripped at all —
  // only the singular fillSeed was, so a multi-seed merged fill's
  // auto-regen-on-edit tracking silently lost every seed but the first one
  // on the very next save/reload. Confirmed live (round-trip test):
  // fillSeeds absent after saveActiveLayerFrame despite being set.
  var fillSeeds=(p.data&&p.data.fillSeeds&&p.data.fillSeeds.length)?p.data.fillSeeds:undefined;
  // Vector-brush fill backdrop pairing (draw-bridge.js) — see that file's
  // own comment on linkedFillId for the full "live reference can't survive
  // JSON" story; isLinkedFillCompanion alone (without the id) was ALSO
  // silently dropped, letting a save/reload resurrect the backdrop as an
  // independently-selectable path (the marquee "parallax" double-move bug
  // that flag exists specifically to prevent).
  var isLinkedFillCompanion=(p.data&&p.data.isLinkedFillCompanion)?true:undefined;
  var linkedFillId=(p.data&&p.data.linkedFillId)?p.data.linkedFillId:undefined;
  // Brush-texture-preset companions (tools.js applyBrushTexture) — a live
  // `data.brushCompanions` array of Path OBJECT references can't survive a
  // JSON round-trip; brushGroupId is what relinkBrushCompanions() (below)
  // uses to regroup after desP() rebuilds fresh Path instances. Without
  // persisting these three fields, the whole group silently degraded into
  // independent untracked strokes on the very next frame save.
  var brushGroupId=(p.data&&p.data.brushGroupId)?p.data.brushGroupId:undefined;
  var isBrushTextureCopy=(p.data&&p.data.isBrushTextureCopy)?true:undefined;
  var brushTexturePreset=(p.data&&p.data.brushTexturePreset)?p.data.brushTexturePreset:undefined;
  var preTextureOpacity=(p.data&&p.data.preTextureOpacity!==undefined)?p.data.preTextureOpacity:undefined;
  var preTextureStroke=(p.data&&p.data.preTextureStroke!==undefined)?p.data.preTextureStroke:undefined;
  // A brush-texture anchor with a fill deliberately carries strokeColor=null
  // (the dabs ARE its stroke — applyBrushTexture) — the '#ffffff' fallback
  // below would resurrect a white outline on it after every save/reload.
  // The dabs themselves (isBrushTextureCopy) need the SAME exemption —
  // missed originally because only brushTexturePreset (anchor-only) was
  // checked, so every dab silently grew a phantom white strokeColor on the
  // very next save/reload. Invisible on the default white canvas (easy to
  // miss), but a REAL stroke nonetheless: it made Fill/Stroke Select's
  // stroke hit-test match individual dabs instead of falling through to
  // the fill-hit path, letting a click select/delete/recolor one dab
  // fleck instead of the whole textured stroke.
  // bitmapBrushSpec anchors (bitmap-brush.js) carry a legitimately-null
  // strokeColor exactly like vector-preset anchors — same white-stroke-
  // resurrection hazard, same exemption.
  var isTexAnchor=!!(p.data&&(p.data.brushTexturePreset||p.data.isBrushTextureCopy||p.data.bitmapBrushSpec));
  var bitmapBrushSpec=(p.data&&p.data.bitmapBrushSpec)?p.data.bitmapBrushSpec:undefined;
  // Real-pressure profile (bitmap-brush.js) — a parallel [x,y,ratio] point
  // cloud captured from the actual draw gesture, looked up by nearest-
  // position at bake/regenerate time. Without persisting it, a save/reload
  // would still show the CURRENT baked pixels fine (they're already in
  // src) but any FUTURE regenerate (a subselect node edit) would silently
  // lose pressure shaping and fall back to flat size.
  var bitmapPressureProfile=(p.data&&p.data.bitmapPressureProfile)?p.data.bitmapPressureProfile:undefined;
  // Same fallback hazard for a Fill/Shadow channel layer (Stroke/Fill/
  // Shadow split, convertLayerToStrokeFillShadowFolder): enforceChannelStrip
  // nulls strokeColor on every non-stroke-channel path on purpose — without
  // this check the '#ffffff' fallback below would resurrect a visible white
  // outline on every Fill/Shadow shape the instant it saves, defeating the
  // whole point of the split. Derived from the CURRENT layer (via p.layer),
  // not a persisted per-path flag, so it can never go stale if the layer's
  // channel/link changes later.
  var pLayerIdx=p.layer?userLayers.indexOf(p.layer):-1;
  var pChLayer=pLayerIdx>=0?state.layers[pLayerIdx]:null;
  // Only the Fill channel unconditionally strips stroke — Shadow used to be
  // treated the same (always no-stroke) back when it only ever held empty
  // frames, but now it can hold a real shadow-boundary STROKE (see
  // channelTag below), so it must keep whatever the user actually drew.
  // isShadowNoStroke still exempts a genuinely strokeless shadow FILL from
  // the '#ffffff' fallback a few lines down, same reasoning as isTexAnchor.
  // Also true for a Shadow Brush "fill only" guide (preferFill, tools.js's
  // applyShadowBrushTag) drawn in an ORDINARY not-yet-channel-split layer —
  // channelTag on the STROKE itself, not just pChLayer.channel, since that
  // split may never have happened yet. Missing this half produced a phantom
  // white outline around the very first save of such a guide (confirmed
  // live): the on-canvas Path correctly had strokeColor=null, but serP's
  // fallback saw a falsy strokeColor with no exemption and wrote '#ffffff'
  // into the stored dict immediately, before any desP round-trip could
  // self-heal it via hasRealStroke.
  var isNoStrokeChannel=!!(pChLayer&&pChLayer.channel==='fill');
  var isShadowNoStroke=!!(((pChLayer&&pChLayer.channel==='shadow')||(p.data&&p.data.channelTag==='shadow'))&&!p.strokeColor);
  var channelTag=(p.data&&p.data.channelTag)?p.data.channelTag:undefined;
  // Which shadow swatch produced this stroke (tools.js's Shadow Brush commit
  // site) — without it, swatch attribution silently dies at the first
  // save/reload/undo while channelTag alone survives.
  var shadowSwatchId=(p.data&&p.data.shadowSwatchId)?p.data.shadowSwatchId:undefined;
  // Team review (Phase 1) — who drew/owns this stroke, and if it's an
  // active reviewer correction, which original strokeId it's a revision
  // of (see forkIfForeignOwner, tools.js). isRevisionGhost/revisionAction
  // mark the frozen "before" copy a fork leaves behind — never mutated
  // again once created, only accepted (removed) or rejected (un-ghosted).
  var ownerId=(p.data&&p.data.ownerId)?p.data.ownerId:undefined;
  var ownerName=(p.data&&p.data.ownerName)?p.data.ownerName:undefined;
  var ownerColor=(p.data&&p.data.ownerColor)?p.data.ownerColor:undefined;
  var revisionParentId=(p.data&&p.data.revisionParentId)?p.data.revisionParentId:undefined;
  var isRevisionGhost=(p.data&&p.data.isRevisionGhost)?true:undefined;
  var revisionAction=(p.data&&p.data.revisionAction)?p.data.revisionAction:undefined;
  // The ghost's exact pre-fork opacity (forkIfForeignOwner/markDeleteAsRevision,
  // tools.js — "so Reject can restore it exactly, not just guess 1") was
  // never round-tripped: a revision ghost survives a save (it's a normal
  // layer child) but rejectRevision/rejectDeleteRevision fall back to a
  // hardcoded 1 once this is gone, silently restoring the WRONG opacity
  // for any stroke that wasn't originally fully opaque. Confirmed live.
  var preRevisionOpacity=(p.data&&p.data.preRevisionOpacity!==undefined)?p.data.preRevisionOpacity:undefined;
  // hasRealStroke records whether p ACTUALLY had a stroke color before the
  // '#ffffff' fallback below (kept for legacy/rendering reasons — see the
  // colorHex8/serP comments above) papers over that with a non-null string.
  // Any consumer that needs to know "did the user really draw a stroke here"
  // (e.g. convertLayerToStrokeFillShadowFolder's splitFrame, which classifies
  // items into the Stroke vs Fill channel) must read THIS, not strokeColor —
  // otherwise every fill-only shape's phantom '#ffffff' gets misread as a
  // real stroke and the shape is wrongly cloned into the Stroke channel too.
  // Vector-brush ribbons paint through fillColor and never carry a real
  // stroke channel. isVB therefore forces hasRealStroke false regardless of
  // any legacy live strokeColor left by an older session.
  var hasRealStroke=isVB?false:!!p.strokeColor;
  return{segments:p.segments.map(function(s){return{point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};}),closed:!!p.closed,strokeColor:(isVB||isNoStrokeChannel||isShadowNoStroke||isTexAnchor&&!p.strokeColor)?null:(p.strokeColor?colorHex8(p.strokeColor):'#ffffff'),hasRealStroke:hasRealStroke,strokeWidth:p.strokeWidth,strokeCap:p.strokeCap||'round',strokeJoin:p.strokeJoin||'round',miterLimit:p.miterLimit,fillColor:p.fillColor?colorHex8(p.fillColor):null,opacity:p.opacity!==undefined?p.opacity:1,dashArray:(p.dashArray&&p.dashArray.length)?p.dashArray.slice():undefined,dashOffset:p.dashOffset,paintOrder:(p.data&&p.data.paintOrder)?p.data.paintOrder:undefined,isVectorBrush:isVB||undefined,isFillShape:(p.data&&p.data.isFillShape)?true:undefined,centerSegments:center,widthProfile:widthProfile,strokeProfile:(p.data&&p.data.strokeProfile)||undefined,profileBase:(p.data&&p.data.profileBase)||undefined,fillSeed:fillSeed,fillSeeds:fillSeeds,fillGapPx:fillGapPx,fillWalls:fillWalls,strokeId:strokeId,brushGroupId:brushGroupId,isLinkedFillCompanion:isLinkedFillCompanion,linkedFillId:linkedFillId,tweenOn:(p.data&&p.data.tweenOn)?true:undefined,boxAngle:(p.data&&p.data.boxAngle)?p.data.boxAngle:undefined,
  // Rotate/scale anchor choice (2026-07, "la position du point d'ancrage
  // n'est pas mise en mémoire si je désélectionne et resélectionne
  // l'élément") — same persistence pattern as boxAngle right above: was
  // only ever session UI state (state.xformAnchorKey/xformAnchorCustom),
  // reset to center by clearSel() on every new selection with nothing
  // remembering what the artist last picked for THIS stroke specifically.
  xformAnchorKey:(p.data&&p.data.xformAnchorKey)?p.data.xformAnchorKey:undefined,
  xformAnchorCustom:(p.data&&p.data.xformAnchorCustom)?p.data.xformAnchorCustom:undefined,
  isBrushTextureCopy:isBrushTextureCopy,brushTexturePreset:brushTexturePreset,bitmapBrushSpec:bitmapBrushSpec,bitmapPressureProfile:bitmapPressureProfile,preTextureOpacity:preTextureOpacity,preTextureStroke:preTextureStroke,channelTag:channelTag,shadowSwatchId:shadowSwatchId,ownerId:ownerId,ownerName:ownerName,ownerColor:ownerColor,revisionParentId:revisionParentId,isRevisionGhost:isRevisionGhost,revisionAction:revisionAction,preRevisionOpacity:preRevisionOpacity,
  // Gradient fill (2026-07, palette-panel.js's gradient editor) — {kind:
  // 'linear'|'radial', from:[x,y], to:[x,y], stops:[{offset,color}]}, world
  // coordinates, plain data so it round-trips through JSON like any other
  // p.data.* field here.
  fillGradient:(p.data&&p.data.fillGradient)?p.data.fillGradient:undefined,
  // Group membership (2026-07, group-bridge.js's Cmd+G) — a stable id
  // shared by every member, same pattern as strokeId/brushGroupId above.
  groupId:(p.data&&p.data.groupId)?p.data.groupId:undefined,
  // Dynamic shape (2026-08-18, "shape dynamique round corner sur rect...")
  // — plain data like fillGradient just above, round-trips through JSON
  // as-is. The item's REAL segments already encode the CURRENT radii
  // (applyParamShapeRect bakes them in, tools.js) — this is only the
  // declared intent (which corner is which, for the Coins panel to read
  // back and for per-corner Motion to target), not a second source of
  // truth for the geometry itself.
  paramShape:(p.data&&p.data.paramShape)?p.data.paramShape:undefined,
  // Per-ELEMENT effects (2026-07, effects-panel.js — "possible de
  // différencié les effet par éléments sélectionné"): same {type,enabled,
  // p1,p2,p3,p4}[] shape as ld.effects, but scoped to just this one shape
  // instead of the whole layer — see engine.rs's paint_layer_items for how
  // an item carrying this gets isolated and effect-processed on its own
  // before rejoining the rest of the layer's paint order.
  effects:(p.data&&p.data.effects&&p.data.effects.length)?p.data.effects:undefined,
  // Vector text (2026-07, vector-text-bridge.js) — a placed block is just
  // ordinary Paths sharing groupId (above); isVectorText/vectorChar mark
  // every glyph, isTextRoot+the rest are ONLY set on the group's first
  // Path (the one openTextPopoverForEdit/commitVectorText look for when
  // re-editing or persisting the block's original string/font/layout).
  isVectorText:(p.data&&p.data.isVectorText)?true:undefined,
  vectorChar:(p.data&&p.data.vectorChar)?p.data.vectorChar:undefined,
  // Text Animator (2026-08-17) grouping — which letter/word/line this glyph
  // (or, on a raster split Raster, this character) belongs to, stamped at
  // build time (vector-text-bridge.js/timeline.js's splitTextIntoCharacters)
  // so text-animator.js can group getEffectiveStrokes' stroke dicts by unit
  // without re-deriving word/line boundaries from strings at animate time.
  charIndex:(p.data&&p.data.charIndex!=null)?p.data.charIndex:undefined,
  wordIndex:(p.data&&p.data.wordIndex!=null)?p.data.wordIndex:undefined,
  lineIndex:(p.data&&p.data.lineIndex!=null)?p.data.lineIndex:undefined,
  isText:(p.data&&p.data.isText)?true:undefined,
  isTextRoot:(p.data&&p.data.isTextRoot)?true:undefined,
  text:(p.data&&p.data.isTextRoot)?p.data.text:undefined,
  vectorFont:(p.data&&p.data.isTextRoot)?p.data.vectorFont:undefined,
  textSize:(p.data&&p.data.isTextRoot)?p.data.size:undefined,
  textColor:(p.data&&p.data.isTextRoot)?p.data.color:undefined,
  textAlign:(p.data&&p.data.isTextRoot)?p.data.align:undefined,
  textFixedWidth:(p.data&&p.data.isTextRoot&&p.data.fixedWidth)?p.data.fixedWidth:undefined,
  anchorTopLeft:(p.data&&p.data.isTextRoot&&p.data.anchorTopLeft)?{x:p.data.anchorTopLeft.x,y:p.data.anchorTopLeft.y}:undefined,
  // Vector mask (2026-08, AE-style "Mask") — see engine-bridge.js's
  // buildSceneJson mask-extraction comment for how these three drive the
  // engine's clip. §1 consumer check: buildSceneJson reads them (done),
  // saveActiveLayerFrame/selectedPaths/tween-matching treat a mask as an
  // ordinary Path (no exclusion needed — it's meant to select/tween/undo
  // exactly like any other shape, only RENDERING treats it specially).
  isMask:(p.data&&p.data.isMask)?true:undefined,
  maskMode:(p.data&&p.data.maskMode)?p.data.maskMode:undefined,
  maskFeather:(p.data&&p.data.maskFeather)?p.data.maskFeather:undefined,
  // Stroke gradient along path (2026-08, "gradient qui tire du début à la
  // fin du trait") — {from,to} hex strings, distinct from fillGradient
  // (spatial 2-point ramp) above.
  strokeGradientAlongPath:(p.data&&p.data.strokeGradientAlongPath)?p.data.strokeGradientAlongPath:undefined};}
// `closed` was missing from this round-trip entirely — every path rebuilt
// via desP() (onion-skin ghosts, tween/inbetween generation, undo/redo
// snapshots, project load: everything that goes through serP/desP) silently
// lost its closed flag and came back as an open path. Filled shapes hid the
// symptom (fill rendering implicitly closes the boundary regardless of the
// flag), but stroked closed shapes — and Outline onion-skin mode, which
// deliberately clears fillColor to show just the silhouette — visibly
// showed a gap at the seam. Must set it AFTER the segment loop: Path.add()
// only appends open-ended, closing has to happen explicitly once all
// segments exist.
function desP(d,layer,op){var prev=project.activeLayer;layer.activate();var p=new Path({insert:true});for(var i=0;i<d.segments.length;i++){var s=d.segments[i];p.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));}if(d.closed)p.closed=true;
  // Mirrors serP's own isNoStrokeChannel check — `layer` here IS the Paper
  // layer being deserialized into, so the same userLayers-index lookup
  // applies without needing any extra param.
  var dLayerIdx=userLayers.indexOf(layer);var dChLayer=dLayerIdx>=0?state.layers[dLayerIdx]:null;
  var dNoStrokeChannel=!!(dChLayer&&dChLayer.channel==='fill');
  var dIsShadowChannel=!!(dChLayer&&dChLayer.channel==='shadow');
  // hasRealStroke===false is AUTHORITATIVE: the path genuinely had no stroke
  // when serialized, so restore none — even when the stored strokeColor is
  // serP's phantom '#ffffff' fallback (see serP's own comment). Without this,
  // every strokeless FILL (paint-bucket fills, fill-brush shapes — which
  // carry none of the exemption flags below) came back from ANY desP
  // round-trip (undo/redo, reload, onion ghosts, export) wearing a white
  // outline it never had ("un trait blanc apparaît autour du fill après
  // ctrl+Z"). Legacy data predating the field (undefined) keeps the old
  // fallback chain untouched.
  p.strokeColor=d.hasRealStroke===false?null:(d.strokeColor||((d.isVectorBrush||d.brushTexturePreset||d.bitmapBrushSpec||d.isBrushTextureCopy||dNoStrokeChannel||dIsShadowChannel)?null:'#fff'));p.strokeWidth=d.strokeWidth||3;p.strokeCap=typeof d.strokeCap==='string'?d.strokeCap:'round';p.strokeJoin=typeof d.strokeJoin==='string'?d.strokeJoin:'round';if(d.miterLimit!==undefined)p.miterLimit=d.miterLimit;if(d.fillColor)p.fillColor=d.fillColor;else p.fillColor=null;p.opacity=op!==undefined?op:(d.opacity!==undefined?d.opacity:1);if(d.dashArray&&d.dashArray.length)p.dashArray=d.dashArray;if(d.dashOffset!==undefined)p.dashOffset=d.dashOffset;if(d.paintOrder){p.data.paintOrder=d.paintOrder;}if(d.isVectorBrush){p.data.isVectorBrush=true;if(d.centerSegments)p.data.centerSegments=d.centerSegments;if(d.widthProfile)p.data.widthProfile=d.widthProfile;if(d.strokeProfile)p.data.strokeProfile=d.strokeProfile;if(d.profileBase)p.data.profileBase=d.profileBase;if(d.isFillShape)p.data.isFillShape=true;applyBrushKeyline(p);}if(d.fillSeed)p.data.fillSeed=d.fillSeed;if(d.fillSeeds&&d.fillSeeds.length)p.data.fillSeeds=d.fillSeeds;if((d.fillSeed||(d.fillSeeds&&d.fillSeeds.length))&&d.fillGapPx!==undefined)p.data.fillGapPx=d.fillGapPx;if(d.fillWalls)p.data.fillWalls=d.fillWalls;if(d.strokeId)p.data.strokeId=d.strokeId;if(d.brushGroupId)p.data.brushGroupId=d.brushGroupId;if(d.isLinkedFillCompanion)p.data.isLinkedFillCompanion=true;if(d.linkedFillId)p.data.linkedFillId=d.linkedFillId;if(d.tweenOn)p.data.tweenOn=true;if(d.boxAngle)p.data.boxAngle=d.boxAngle;if(d.xformAnchorKey)p.data.xformAnchorKey=d.xformAnchorKey;if(d.xformAnchorCustom)p.data.xformAnchorCustom=d.xformAnchorCustom;if(d.isBrushTextureCopy)p.data.isBrushTextureCopy=true;if(d.brushTexturePreset)p.data.brushTexturePreset=d.brushTexturePreset;if(d.bitmapBrushSpec)p.data.bitmapBrushSpec=d.bitmapBrushSpec;if(d.bitmapPressureProfile)p.data.bitmapPressureProfile=d.bitmapPressureProfile;if(d.preTextureOpacity!==undefined)p.data.preTextureOpacity=d.preTextureOpacity;if(d.preTextureStroke!==undefined)p.data.preTextureStroke=d.preTextureStroke;if(d.channelTag)p.data.channelTag=d.channelTag;if(d.shadowSwatchId)p.data.shadowSwatchId=d.shadowSwatchId;if(d.ownerId)p.data.ownerId=d.ownerId;if(d.ownerName)p.data.ownerName=d.ownerName;if(d.ownerColor)p.data.ownerColor=d.ownerColor;if(d.revisionParentId)p.data.revisionParentId=d.revisionParentId;if(d.isRevisionGhost)p.data.isRevisionGhost=true;if(d.revisionAction)p.data.revisionAction=d.revisionAction;if(d.preRevisionOpacity!==undefined)p.data.preRevisionOpacity=d.preRevisionOpacity;if(d.fillGradient)p.data.fillGradient=d.fillGradient;if(d.groupId)p.data.groupId=d.groupId;if(d.paramShape)p.data.paramShape=d.paramShape;if(d.effects&&d.effects.length)p.data.effects=d.effects;
  if(d.isVectorText)p.data.isVectorText=true;if(d.vectorChar)p.data.vectorChar=d.vectorChar;if(d.isText)p.data.isText=true;
  if(d.charIndex!=null)p.data.charIndex=d.charIndex;if(d.wordIndex!=null)p.data.wordIndex=d.wordIndex;if(d.lineIndex!=null)p.data.lineIndex=d.lineIndex;
  // Mograph duplicator copy tags (applyLayerDuplicator) — belt-and-suspenders
  // for future layer.children consumers; the layer is force-locked so no
  // interactive path reads these today.
  if(d.isDuplicatorCopy)p.data.isDuplicatorCopy=true;if(d.dupIndex!=null)p.data.dupIndex=d.dupIndex;
  // dupCloneId ("ID de chaque cloner") and dup3D (per-clone 3D delta) —
  // same "carried for future/other consumers, force-locked so nothing
  // interactive reads these today" reasoning as isDuplicatorCopy/dupIndex
  // just above. buildSceneJson (engine-bridge.js) DOES read dup3D off the
  // live object here for its per-clone 3D projector cache — the one real,
  // current consumer.
  if(d.dupCloneId)p.data.dupCloneId=d.dupCloneId;if(d.dup3D)p.data.dup3D=d.dup3D;
  if(d.isTextRoot){p.data.isTextRoot=true;p.data.text=d.text||'';p.data.vectorFont=d.vectorFont||'Roboto-Regular';p.data.size=d.textSize||48;p.data.color=d.textColor||'#000000';p.data.align=d.textAlign||'left';if(d.textFixedWidth)p.data.fixedWidth=d.textFixedWidth;if(d.anchorTopLeft)p.data.anchorTopLeft={x:d.anchorTopLeft.x,y:d.anchorTopLeft.y};}
  if(d.isMask){p.data.isMask=true;p.data.maskMode=d.maskMode||'add';if(d.maskFeather)p.data.maskFeather=d.maskFeather;}
  if(d.strokeGradientAlongPath)p.data.strokeGradientAlongPath=d.strokeGradientAlongPath;
  // Retained-path stamp (engine-bridge.js, 2026-07-28): the stored stroke
  // dict this Paper item was built FROM. Dict object identity is the
  // geometry identity — getEffectiveStrokes returns the SAME stored dict
  // objects for a frame until an edit replaces the whole strokes array
  // (f.strokes=..., saveActiveLayerFrame), the same insight that made `src`
  // string identity work for images. engine-bridge maps dict→engine path
  // key and emits a pathRef instead of re-serializing segments every frame.
  // Live-mutation safety: a Paper Item.prototype._changed hook (installed +
  // self-tested in engine-bridge) NULLS this stamp on any geometry change,
  // so a sculpted/dragged item falls back to inline serialization until the
  // next desP rebuild re-stamps it. Set LAST — construction above fires
  // _changed per added segment, which would clear an earlier stamp.
  // §1 consumers checked: serP reads named fields only (no wholesale data
  // copy — no cycle into saves), onionLayerItems/tween matching/select
  // bridges ignore unknown data.* fields.
  p.data.__engineSrcDict=d;
  prev.activate();return p;}
// Phase 2 (async multi-user sync): merges a remote collaborator's exported
// project JSON into the CURRENT live document at the data level (state.layers
// serialized strokes — not live Paper objects), so it works the same whether
// called after reading a real file (desktop) or fed a plain object (tests).
// Layers are matched by NAME (no stable cross-user layer id exists yet — a
// known simplification: collaborators are assumed to share the same layer
// list, same as e.g. TVPaint's exposure-sheet-driven workflow). Frames are
// matched by index, and only merged where the LOCAL frame is already a real
// keyframe (isKeyframe) — merging into a tween inbetween would just get
// overwritten by the next retween, and creating brand-new keyframes via merge
// is out of scope here.
//
// Per remote stroke (matched by data.strokeId, the same id forkIfForeignOwner
// already relies on):
//   - not present locally at all         -> pure addition, appended as-is
//   - present locally with identical data -> no-op, already in sync
//   - present locally but DIFFERENT       -> genuine conflict: local is left
//     untouched (still the visible/active content) and the remote version is
//     appended as a new stroke carrying revisionParentId = the local
//     strokeId. This deliberately reuses Phase 1's ghost/revision data shape
//     UNCHANGED (buildRevisionOutlineItems, acceptRevision/rejectRevision) —
//     the remote edit renders as a dashed correction outline in the remote
//     author's color, and Accept/Reject resolves it exactly like a
//     supervisor's in-app revision, without local ever needing an
//     isRevisionGhost flag (it isn't superseded, just contested).
function mergeRemoteSnapshot(remoteData, remoteProfile) {
  var report = { added: 0, conflicts: 0, identical: 0, layersSkipped: 0 };
  if (!remoteData || !remoteData.layers) return report;
  remoteData.layers.forEach(function (remoteLd) {
    var localIdx = state.layers.findIndex(function (ld) { return ld.name === remoteLd.name; });
    if (localIdx < 0 || !remoteLd.frames) { report.layersSkipped++; return; }
    var localLd = state.layers[localIdx];
    var frameCount = Math.min(localLd.frames.length, remoteLd.frames.length);
    for (var fi = 0; fi < frameCount; fi++) {
      var localFrame = localLd.frames[fi];
      var remoteFrame = remoteLd.frames[fi];
      if (!localFrame || !localFrame.isKeyframe || !remoteFrame || !remoteFrame.strokes) continue;
      if (!localFrame.strokes) localFrame.strokes = [];
      remoteFrame.strokes.forEach(function (rs) {
        if (!rs.strokeId) return; // no stable id -> can't merge safely, skip
        var localMatch = localFrame.strokes.find(function (ls) { return ls.strokeId === rs.strokeId; });
        if (!localMatch) {
          var added = JSON.parse(JSON.stringify(rs));
          added.ownerId = added.ownerId || remoteProfile.id;
          added.ownerName = added.ownerName || remoteProfile.name;
          added.ownerColor = added.ownerColor || remoteProfile.color;
          localFrame.strokes.push(added);
          report.added++;
          return;
        }
        if (JSON.stringify(localMatch) === JSON.stringify(rs)) { report.identical++; return; }
        var conflictClone = JSON.parse(JSON.stringify(rs));
        conflictClone.strokeId = 'merge_' + rs.strokeId + '_' + Date.now().toString(36) + Math.random().toString(36).slice(2, 6);
        conflictClone.ownerId = remoteProfile.id;
        conflictClone.ownerName = remoteProfile.name;
        conflictClone.ownerColor = remoteProfile.color;
        conflictClone.revisionParentId = localMatch.strokeId;
        conflictClone.isRevisionGhost = undefined;
        localFrame.strokes.push(conflictClone);
        report.conflicts++;
      });
    }
  });
  if (report.added || report.conflicts) {
    loadFrame(state.currentFrame); renderOS(); renderArcs(); updateUI();
  }
  return report;
}
// relinkBrushCompanions's counterpart to fillRegenerateLinked/strokeId —
// desP() restores brushGroupId/isBrushTextureCopy/brushTexturePreset onto
// each freshly-reconstructed Path, but the PRIMARY's live
// `data.brushCompanions` array (of Path object references, used by
// select-bridge.js/timeline.js to move/re-texture the whole group as one
// unit) is inherently un-serializable — this rebuilds it by regrouping
// `layer`'s current children by brushGroupId. Call once after a layer's
// children have all been desP()'d (loadFrame, undo/redo restore, onion
// skin/ghost layers if they ever need companions to move together).
function relinkBrushCompanions(layer){
  if(!layer)return;
  var groups={};
  layer.children.forEach(function(c){
    var gid=c.data&&c.data.brushGroupId;if(!gid)return;
    (groups[gid]=groups[gid]||[]).push(c);
  });
  Object.keys(groups).forEach(function(gid){
    var members=groups[gid];
    var primary=members.filter(function(c){return!(c.data&&c.data.isBrushTextureCopy);})[0];
    if(!primary)return;
    primary.data.brushCompanions=members.filter(function(c){return c!==primary;});
  });
}
// Same "live reference can't survive JSON, stable id + relink" pattern as
// relinkBrushCompanions just above, for a vector-brush ribbon's fill
// backdrop (draw-bridge.js's linkedFillId) — re-establishes
// ribbon.data.linkedFill after every layer rebuild (desP only restores the
// id-based tags, never the live pairing itself). Without this, every
// rebuildVectorBrushOutline call after a save/reload/undo silently stopped
// re-syncing the backdrop's shape to the ribbon's (the exact "fill pas
// attaché au stroke" desync draw-bridge.js's own comment already names).
function relinkLinkedFills(layer){
  if(!layer)return;
  var byId={};
  layer.children.forEach(function(c){
    if(c.data&&c.data.isLinkedFillCompanion&&c.data.linkedFillId)byId[c.data.linkedFillId]=c;
  });
  layer.children.forEach(function(c){
    var lid=c.data&&c.data.linkedFillId;
    if(!lid||(c.data&&c.data.isLinkedFillCompanion))return; // only the ribbon side looks its pair up
    var fill=byId[lid];
    if(fill&&!fill.removed)c.data.linkedFill=fill;
  });
}
// Strips ANY brush texture (vector-preset dabs OR Bitmap Brush's raster —
// same camouflage convention, applyBrushTexture/applyBitmapBrushTexture)
// off an anchor back to a plain stroke, restoring whatever strokeColor/
// opacity the camouflage remembered. Shared by both "Apply to selection"
// buttons (timeline.js's vector-preset one, bitmap-brush.js's own) so
// switching FROM either kind TO the other, or to no texture, always goes
// through the same clean strip-then-reapply path instead of two separate
// half-implementations drifting apart.
function stripAnyBrushTexture(p){
  if(!p||!p.data)return;
  if(p.data.brushCompanions)p.data.brushCompanions.forEach(function(c){c.remove();});
  p.data.brushCompanions=null;
  if(p.data.preTextureOpacity!==undefined){p.opacity=p.data.preTextureOpacity;delete p.data.preTextureOpacity;}
  if(p.data.preTextureStroke!==undefined){p.strokeColor=p.data.preTextureStroke;delete p.data.preTextureStroke;}
  delete p.data.brushTexturePreset;delete p.data.bitmapBrushSpec;delete p.data.brushGroupId;delete p.data.bitmapPressureProfile;
}
// Bitmap import (Section 6 gap): a Raster is stored by its dataURL (fully
// self-contained in the project JSON, no external file dependency after
// import) plus the position/size the user left it at — width/height are
// re-applied explicitly on load since Raster sources decode async and may
// briefly report their natural size instead of the saved one.
// isBitmapBrush + its preset metadata (bitmap-brush.js) round-trip through
// here too — found live: without this, a saved-then-reloaded bitmap-brush
// stroke still rendered fine (the baked pixels ARE the src) but silently
// lost the tag marking it as one, same "new tag handled at the write site,
// forgotten at the read site" bug class CLAUDE.md's family-of-bug-#1
// documents for exactly this file.
function serR(r){
  // Mid-decode race (bug #9, 2026-07): a Raster built by desR() during a
  // fast scrub may still be `!r.loaded` when saveAllLayerFrames() serializes
  // it right back out (goToFrame() saves the frame being LEFT before
  // navigating). Before place() runs (async, in onLoad), r.position/r.bounds
  // still hold the pre-resize placeholder geometry — reading them here
  // produced a spurious x/y/width/height divergence from the stored frame
  // data, which _maybePromoteInterpolated() then read as a real content
  // change, permanently flipping an untouched tween in-between to
  // isKeyframe:true (shown green, "modifié"). desR stashes the INTENDED
  // geometry on r.data synchronously (before the async gap) precisely so
  // this fallback has something correct to read meanwhile.
  var pending=r.data&&r.data._pendingGeom;
  // Rotation-aware sizing (2026-07, images join the transform box): once
  // the select tool's rotate ring has spun a raster, r.bounds is the
  // INFLATED axis-aligned envelope — the honest display size is the
  // un-rotated natural size × |scaling| (same math as engine-bridge's
  // rasterImageRect), with the spin saved separately in d.rotation.
  // Unrotated rasters keep the historical bounds read untouched.
  var rot=(!r.loaded&&pending)?(pending.rotation||0):((r.matrix&&r.matrix.rotation)||0);
  var useX=(!r.loaded&&pending)?pending.x:r.position.x;
  var useY=(!r.loaded&&pending)?pending.y:r.position.y;
  var useW=(!r.loaded&&pending)?pending.width:(rot?Math.abs(r.scaling.x)*r.width:r.bounds.width);
  var useH=(!r.loaded&&pending)?pending.height:(rot?Math.abs(r.scaling.y)*r.height:r.bounds.height);
  var d={isRaster:true,src:r.data&&r.data.src?r.data.src:r.source,x:useX,y:useY,width:useW,height:useH,opacity:r.opacity!==undefined?r.opacity:1};if(r.data&&r.data.isBitmapBrush)d.isBitmapBrush=true;
  if(rot)d.rotation=rot;
  // Companion linkage (v2 anchor+companion architecture, bitmap-brush.js):
  // brushGroupId is how relinkBrushCompanions() regroups this raster with
  // its anchor path after desR/desP rebuild everything fresh on loadFrame —
  // without persisting these two, a reload silently orphaned the texture
  // from its anchor (independent selection, no group move, no regen).
  if(r.data&&r.data.brushGroupId)d.brushGroupId=r.data.brushGroupId;
  if(r.data&&r.data.isBrushTextureCopy)d.isBrushTextureCopy=true;
  // Group membership (2026-07, group-bridge.js's Cmd+G) — same stable id
  // as serP's own groupId, so a group can freely mix Path and Raster members.
  if(r.data&&r.data.groupId)d.groupId=r.data.groupId;
  // Text tool (2026-07 rework) — isText/text/font/size/color/align/fixedWidth
  // are the ONLY way a re-baked-to-PNG text block stays re-editable (double-
  // click via openTextPopoverForEdit, timeline.js) instead of degrading into
  // an indistinguishable plain imported raster after one save/reload — the
  // exact gap flagged by this session's own text-tool audit. fixedWidth is
  // world-space (matches x/y/width/height's own units), null for point text.
  if(r.data&&r.data.isText){
    d.isText=true;d.text=r.data.text||'';d.font=r.data.font||'sans-serif';
    d.size=r.data.size||48;d.color=r.data.color||'#000000';d.align=r.data.align||'left';
    if(r.data.fixedWidth)d.fixedWidth=r.data.fixedWidth;
    // Per-character split (2026-07, "Découper par caractère") — isTextChar/
    // textGroupId are the ONLY thing distinguishing a split letter from any
    // other tiny imported raster; without persisting them a reload would
    // silently forget these were ever a sentence (harmless for rendering,
    // but would break any future "recombine"/select-siblings tooling).
    if(r.data.isTextChar)d.isTextChar=true;
    if(r.data.textGroupId)d.textGroupId=r.data.textGroupId;
    // Text Animator grouping (2026-08-17) — see serP's own charIndex comment,
    // same contract for a raster-split character.
    if(r.data.charIndex!=null)d.charIndex=r.data.charIndex;
    if(r.data.wordIndex!=null)d.wordIndex=r.data.wordIndex;
    if(r.data.lineIndex!=null)d.lineIndex=r.data.lineIndex;
  }
  return d;}
// r.onLoad attached AFTER `new Raster(d.src)` can miss an ALREADY-loaded
// image (a data: URI, like bitmap-brush.js's baked textures, often decodes
// fast enough that this races) — same bug bitmap-brush.js's own stamp
// function had, found live ("pas bonne taille" after releasing a Bitmap
// Brush stroke): position happened to still look right here (also set
// synchronously below, before this comment was added), but .size silently
// kept Paper's default natural-pixel dimensions instead of the intended
// world-space w/h whenever onLoad never fired. Checking `.loaded` first
// covers both cases.
// ---- DECODED-IMAGE CACHE (2026-07-28) ----
// loadFrame() rebuilds every item through desR on EVERY scrub tick, and desR
// used to hand Paper a data: URL — one fresh PNG decode per raster per frame,
// forever. Measured on a real 15.9MB project: 57 raster reads across the
// component's keyframes for only 24 distinct images, 4.7MB of base64 decoded
// just to visit frame 0. That is the "il redessine à chaque fois" cost.
//
// Keyed on the data URL itself, which is already this codebase's image
// identity (engine-bridge.js's registerRasterIfNeeded keys the GPU texture
// the same way), so nothing new has to stay in sync. Every path that CHANGES
// a raster's pixels — eraseBite, flushEraseDirty, liveRestamp, regenerate —
// writes a fresh toDataURL string, i.e. a new key: a mutated image can never
// read a stale entry, and no explicit invalidation is needed.
//
// Sharing one decoded <img> across many Rasters is safe: Paper's _setImage
// copies the element into the Raster's OWN canvas (setSize does
// getCanvas + drawImage(previousElement)), so a later r.size/rotate on one
// Raster cannot disturb another. It also makes the Raster synchronously
// `loaded`, which is what removes the async gap the onLoad hook below exists
// to paper over.
//
// Bounded: an imported video mints ~one distinct frame per video frame
// (images.js), so an unbounded map would grow to the whole decoded movie.
// LRU by insertion order, capped on BOTH count and approximate bytes.
var _imgCacheMax=120, _imgCacheMaxBytes=192*1024*1024;
var _imgCache=new Map(), _imgCacheBytes=0;
function _imgCacheGet(src){
  var e=_imgCache.get(src);
  if(!e)return null;
  // Re-insert to mark most-recently-used (Map preserves insertion order).
  _imgCache.delete(src);_imgCache.set(src,e);
  // Only a fully decoded element is usable synchronously; a pending one is
  // reported as a miss so the caller takes the normal async path.
  return (e.img&&e.img.complete&&e.img.naturalWidth)?e.img:null;
}
// Decode ONE image we own, in the background. Deliberately not Paper's own
// r.image: after decode Paper swaps its internal element for a canvas
// (_setImage -> setSize -> getCanvas + drawImage), so the element reachable
// from a loaded Raster has no .complete/.naturalWidth and is useless as a
// cache entry — verified live (loaded:true, complete:false, naturalWidth:null).
// Owning the element also means Paper can never mutate it under us.
function _imgCacheWarm(src){
  if(!src||src.indexOf('data:')!==0||_imgCache.has(src)||_imgCacheWarming[src])return;
  _imgCacheWarming[src]=1;
  var im=new Image();
  im.onload=function(){delete _imgCacheWarming[src];_imgCachePut(src,im);};
  im.onerror=function(){delete _imgCacheWarming[src];};
  im.src=src;
}
var _imgCacheWarming={};
function _imgCachePut(src,img){
  if(!src||!img||_imgCache.has(src))return;
  var bytes=src.length;
  _imgCache.set(src,{img:img,bytes:bytes});_imgCacheBytes+=bytes;
  while(_imgCache.size>_imgCacheMax||_imgCacheBytes>_imgCacheMaxBytes){
    var oldest=_imgCache.keys().next();
    if(oldest.done)break;
    var ev=_imgCache.get(oldest.value);
    _imgCache.delete(oldest.value);_imgCacheBytes-=(ev&&ev.bytes)||0;
  }
}
// Exposed for measurement and for the tests that prove the hit rate.
window.__imgCacheStats=function(){return{entries:_imgCache.size,mo:+( _imgCacheBytes/1e6).toFixed(2),hits:_imgCacheHits,misses:_imgCacheMisses};};
window.__imgCacheReset=function(){_imgCache.clear();_imgCacheBytes=0;_imgCacheHits=0;_imgCacheMisses=0;};
var _imgCacheHits=0,_imgCacheMisses=0;
function desR(d,layer,op){var prev=project.activeLayer;layer.activate();
  // Cache hit: hand Paper the ALREADY-DECODED element, which makes the
  // Raster synchronously `loaded` — place() runs inline below and the
  // async onLoad path (and its self-healing renderNow) is never needed.
  var _cached=_imgCacheGet(d.src);
  if(_cached)_imgCacheHits++;else{_imgCacheMisses++;_imgCacheWarm(d.src);}
  var r=new Raster(_cached||d.src);r.data.src=d.src;if(d.isBitmapBrush)r.data.isBitmapBrush=true;if(d.brushGroupId)r.data.brushGroupId=d.brushGroupId;if(d.isBrushTextureCopy)r.data.isBrushTextureCopy=true;if(d.groupId)r.data.groupId=d.groupId;
  if(d.isText){r.data.isText=true;r.data.text=d.text||'';r.data.font=d.font||'sans-serif';r.data.size=d.size||48;r.data.color=d.color||'#000000';r.data.align=d.align||'left';if(d.fixedWidth)r.data.fixedWidth=d.fixedWidth;if(d.isTextChar)r.data.isTextChar=true;if(d.textGroupId)r.data.textGroupId=d.textGroupId;if(d.charIndex!=null)r.data.charIndex=d.charIndex;if(d.wordIndex!=null)r.data.wordIndex=d.wordIndex;if(d.lineIndex!=null)r.data.lineIndex=d.lineIndex;}
  r.position=new Point(d.x,d.y);r.opacity=op!==undefined?op:(d.opacity!==undefined?d.opacity:1);var w=d.width,h=d.height;
  // serR()'s mid-decode fallback reads this — see its own comment. Cleared
  // once place() actually applies the real geometry so serR immediately
  // goes back to trusting live r.position/r.bounds afterward (a post-load
  // drag/edit must still be captured normally).
  r.data._pendingGeom={x:d.x,y:d.y,width:w,height:h,rotation:d.rotation||0};
  // Rotation applied AFTER size+position (the raster is a fresh un-rotated
  // object here) — r.rotate's default pivot is r.position, i.e. the rect
  // center, matching serR's decomposition and the engine's own draw pivot.
  var place=function(){r.size=new Size(w,h);r.position=new Point(d.x,d.y);if(d.rotation)r.rotate(d.rotation);r.data._pendingGeom=null;};
  if(r.loaded)place();
  else r.onLoad=function(){
    place();

    // Found live (2026-07-17, "un scrub dans la timeline fait disparaitre
    // la texture"): loadFrame() rebuilds EVERY item fresh via desR/desP on
    // every scrub tick, so a bitmap-brush texture's Raster is a BRAND NEW
    // object each time — decode is async (Raster.onLoad), and
    // engine-bridge.js's registerRasterIfNeeded() skips (`continue`, no
    // retry loop) any raster that isn't `.loaded` yet at serialize time.
    // A fast scrub reliably outran the decode, so the very frame that
    // should show the texture rendered without it — and since place()
    // alone never told the engine anything changed, nothing repainted it
    // once decode DID finish, so it silently stayed missing instead of
    // popping back in. Re-render once decode completes so the picture
    // self-heals instead of depending on the next unrelated repaint.
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
  };
  prev.activate();return r;}

// ---- COMPONENTS / SYMBOLS ----
// A "component" is a layer whose content lives in its own mini-timeline
// (state.symbols[id] — same shape as the top-level project: layers/
// totalFrames/fps) instead of directly in state.layers[i].frames. Editing
// a component swaps the global state.layers/userLayers to point at the
// symbol's own data (see enterSymbol/exitToScene below) so every existing
// drawing/timeline/tween function keeps working unmodified inside it.
// v17: a component instance's own placement (position/scale/rotation) is
// separate from its internal animated content — like an Animate "graphic"
// symbol instance, moving/transforming the instance on the parent stage
// must NOT bake into the symbol's own frame data (that would only affect
// whichever internal frame happened to be resolved at the moment of the
// move, and gets silently discarded the instant playback resolves a
// different internal frame — the reported "moves it, then it snaps back").
// Stored as a plain serializable 6-number affine (a,b,c,d,tx,ty, same
// convention as paper.Matrix) on the layer itself, applied fresh on top of
// the symbol's own (untransformed) content every time it's composited.
function symMatrixOf(ld){
  var m=ld.symMatrix;
  if(!m)return new Matrix(1,0,0,1,0,0);
  return new Matrix(m[0],m[1],m[2],m[3],m[4],m[5]);
}
function symMatrixSet(ld,m){ld.symMatrix=[m.a,m.b,m.c,m.d,m.tx,m.ty];}
// Shared by select-bridge.js (engine path) and tools.js (Paper-native path)
// — one definition so the two don't drift (see CLAUDE.md on duplicated
// pairs). Folds one gesture-tick's world-space operation into the active
// layer's persistent symMatrix, a no-op unless that layer is a component.
function symGestureAccumulate(opMatrix){
  var ld=state.layers[state.activeLayerIdx];
  if(!ld||!ld.symbolId)return;
  var m=symMatrixOf(ld);
  opMatrix.append(m);
  symMatrixSet(ld,opMatrix);
  // NOTE: no _sceneVersion bump here. symMatrix is materialised by
  // getEffectiveStrokes at loadFrame time; engine-bridge.js never reads it
  // (grep: zero references), so bumping the version only forces a rebuild
  // that produces the same bytes. What makes a component drag visible live
  // is the geometry translate in select-bridge.js's move branch — see the
  // selectedPaths re-sync there.
}
// Applies an affine matrix to one raw stroke-data dict's geometry in place
// — point gets the full affine, handleIn/handleOut are deltas so only the
// linear (non-translating) part applies to them.
// Deep-clone a stroke dict for transforming, WITHOUT copying its image
// payload. A raster stroke's `src` is a base64 data URL — on a real project
// that is ~190KB per raster and 15.3MB across a component (the next-largest
// field, `segments`, totals 66KB: src is 99.5% of the bytes). Every
// transform site here only ever rewrites geometry, so round-tripping that
// string through JSON was pure waste: measured 1.9ms per call for one
// component's 20 strokes, paid twice per frame on every scrub/playback tick
// (2026-07-28, "la lecture dans motion de cet élément est saccadée").
//
// `src` is immutable by contract — desR reads it, nothing rewrites it in
// place — so the clone shares the same string reference. Any future field
// that is both large and immutable belongs in HEAVY too.
// `src` is a base64 data URL (~190KB per raster); bitmapPressureProfile is
// the per-dab pressure sampling a bitmap-brush stroke carries (62KB across
// the reference project, second only to src). Both are written once at
// creation and only ever read afterwards, so a clone can share them.
var _HEAVY_STROKE_FIELDS = ['src', 'bitmapPressureProfile'];
// Exported so the undo snapshot (_cloneLayersForUndo, tweens.js) shares the
// same list — two copies of it would drift the moment a third heavy field
// appears (CLAUDE.md §3).
window._HEAVY_STROKE_FIELDS = _HEAVY_STROKE_FIELDS;
function cloneStrokeForTransform(sd) {
  var heavy = null, k, i;
  for (i = 0; i < _HEAVY_STROKE_FIELDS.length; i++) {
    k = _HEAVY_STROKE_FIELDS[i];
    if (sd[k] !== undefined) { (heavy || (heavy = {}))[k] = sd[k]; }
  }
  if (!heavy) return JSON.parse(JSON.stringify(sd));
  var light = {};
  for (k in sd) { if (heavy[k] === undefined) light[k] = sd[k]; }
  var out = JSON.parse(JSON.stringify(light));
  for (k in heavy) out[k] = heavy[k];
  return out;
}
function applyMatrixToStrokeData(sd,m){
  if(sd.isRaster){
    // A raster stroke has no `segments` (see serR/desR — just a center x/y
    // and a width/height rect, no rotation field anywhere in this schema:
    // engine-bridge's image item builder and the Rust image-draw path both
    // only ever compose a translate+scale, see engine.rs's ItemIn.image
    // handling). The early-return below would otherwise silently no-op a
    // symMatrix on an imported-video component (confirmed live: dragging
    // the component moved nothing at all) — translate the center and scale
    // width/height by the matrix's own linear-part magnitude so move/resize
    // gestures work correctly; a rotate gesture degrades to scale-only
    // (matches the schema's real capability rather than silently doing
    // nothing or crashing).
    var c=m.transform(new Point(sd.x,sd.y));
    var scaleX=Math.sqrt(m.a*m.a+m.b*m.b),scaleY=Math.sqrt(m.c*m.c+m.d*m.d);
    return{isRaster:true,src:sd.src,x:c.x,y:c.y,width:sd.width*scaleX,height:sd.height*scaleY,opacity:sd.opacity};
  }
  if(!sd.segments)return sd;
  sd.segments=sd.segments.map(function(s){
    var p=m.transform(new Point(s.point[0],s.point[1]));
    var hiV=new Point(s.handleIn[0],s.handleIn[1]);
    var hoV=new Point(s.handleOut[0],s.handleOut[1]);
    // linear-only transform of a vector: apply the matrix to (0,0)->v and
    // to (0,0), then subtract — strips translation, keeps rotate/scale/skew
    var origin=m.transform(new Point(0,0));
    var hiT=m.transform(hiV).subtract(origin);
    var hoT=m.transform(hoV).subtract(origin);
    return{point:[p.x,p.y],handleIn:[hiT.x,hiT.y],handleOut:[hoT.x,hoT.y]};
  });
  return sd;
}
// ---- MOGRAPH DUPLICATOR (2026-07-29) ----
// Grid/radial/path array duplication of a layer's content (AE shape
// Repeater family), applied at loadFrame time via
// getEffectiveStrokesRendered below — the stored frames NEVER contain the
// copies. See toggleLayerDuplicator (motion.js) for the lock/edit-source
// safety model, and saveActiveLayerFrame's guard for why it matters.
//
// Fourth mirror of the same closed-form affine as affineFromMotion
// (engine-bridge.js), transformSegments (motion.js) and camera.js's own
// matrix build: scale in the pivot's local frame, rotate around the pivot,
// translate last. Built as ONE expression, never chained
// Matrix.translate/rotate/scale calls — a bare Matrix's own methods APPEND
// while Item methods PREPEND, a real shipped-bug class documented in
// CLAUDE.md §8 point 2.
// "N'importe quel property" (2026-07-30) — an Effector's contribution used
// to be 4 hardcoded fields (offsetPos/offsetRot/offsetScale/offsetOpacity);
// now it's an arbitrary list of {prop,value} channels, any of
// SMMotion.DUP_TARGET_PROPS. An effector created before this change has no
// `channels` array yet — migrate it ONCE, in place (eff IS the live
// dup.effectors[i] object, so this sticks and gets saved forward on the
// next project save), by folding whichever legacy fields are non-zero into
// the equivalent channel entries. Idempotent: eff.channels is truthy after
// the first call (even an empty array for an all-zero legacy effector), so
// every later call just returns it.
function effectorChannels(eff){
  if(eff.channels)return eff.channels;
  var ch=[];
  if(eff.offsetPos&&(eff.offsetPos[0]||eff.offsetPos[1]))ch.push({prop:'position',value:[eff.offsetPos[0]||0,eff.offsetPos[1]||0]});
  if(eff.offsetRot)ch.push({prop:'rotation',value:[eff.offsetRot]});
  if(eff.offsetScale&&(eff.offsetScale[0]||eff.offsetScale[1]))ch.push({prop:'scale',value:[eff.offsetScale[0]||0,eff.offsetScale[1]||0]});
  if(eff.offsetOpacity)ch.push({prop:'opacity',value:[eff.offsetOpacity]});
  eff.channels=ch;
  return ch;
}
function dupMatrixFromDescriptor(m,pivot){
  var rad=(m.rot||0)*Math.PI/180,cs=Math.cos(rad),sn=Math.sin(rad);
  var a=cs*m.sx,b=sn*m.sx,c=-sn*m.sy,d=cs*m.sy;
  var tx=pivot.x+(m.dx||0)-a*pivot.x-c*pivot.y;
  var ty=pivot.y+(m.dy||0)-b*pivot.x-d*pivot.y;
  return new Matrix(a,b,c,d,tx,ty);
}
// Anchor-point bounds of the seed strokes — a pivot, not a render bound,
// so segment anchor points are precise enough (no bezier-extrema math, no
// throwaway Paper Path).
function _boundsCenterOfStrokes(strokes){
  var minX=Infinity,minY=Infinity,maxX=-Infinity,maxY=-Infinity;
  strokes.forEach(function(sd){
    if(sd.isRaster){
      minX=Math.min(minX,sd.x-sd.width/2);maxX=Math.max(maxX,sd.x+sd.width/2);
      minY=Math.min(minY,sd.y-sd.height/2);maxY=Math.max(maxY,sd.y+sd.height/2);
      return;
    }
    if(!sd.segments)return;
    sd.segments.forEach(function(s){
      minX=Math.min(minX,s.point[0]);maxX=Math.max(maxX,s.point[0]);
      minY=Math.min(minY,s.point[1]);maxY=Math.max(maxY,s.point[1]);
    });
  });
  if(minX===Infinity)return{x:state.canvasW/2,y:state.canvasH/2};
  return{x:(minX+maxX)/2,y:(minY+maxY)/2};
}
// Path-mode distribution source: another layer referenced by stable
// layerUid (same mechanism as AE-style parenting, parentLayerUid /
// SMMotion.findLayerIndexByUid) — its BASE content deliberately (a
// duplicator can't reference another duplicator; the panel's <select>
// also excludes them, this is the runtime backstop).
function _resolveDuplicatorPath(dup,frameIdx){
  var idx=window.SMMotion?SMMotion.findLayerIndexByUid(dup.pathLayerUid):-1;
  if(idx<0)return null;
  var srcLd=state.layers[idx];
  if(!srcLd||srcLd.duplicator)return null;
  var strokes=getEffectiveStrokes(idx,frameIdx);
  var sd=null;
  for(var i=0;i<strokes.length;i++){if(!strokes[i].isRaster&&strokes[i].segments&&strokes[i].segments.length>=2){sd=strokes[i];break;}}
  if(!sd)return null;
  var p=new Path({insert:false});
  sd.segments.forEach(function(s){p.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
  if(sd.closed)p.closed=true;
  if(!p.length){p.remove();return null;}
  return{path:p,length:p.length,closed:!!sd.closed,start:p.getPointAt(0)};
}
// Shared by applyLayerDuplicator (below, the strokes/rasters path) AND
// engine-bridge.js's nativeVideo render branch (2026-07-30 — a native video
// layer's picture is ONE image item pushed directly, entirely bypassing
// getEffectiveStrokesRendered, so the Duplicator previously had zero effect
// on a video layer: enabling it changed nothing on screen). This is ONE
// clone's placement (position/rotation/scale/opacity delta + this clone's
// own effector contributions), given its index `k` and the pivot ITS
// content is centered on — `pivotK` is a parameter rather than always
// `pivot` because the stroke path's temporal stagger (tOffOn below)
// re-samples the seed layer's own content at a per-copy shifted frame, so
// different copies can be centered on different bounds. nativeVideo has
// exactly one decoded image per frame tick (no per-clone re-sampling is
// possible), so its caller always passes the same pivot for every k —
// temporal stagger is a no-op for video, same reason it can't animate.
// MUST stay the only place this math lives — mirrors CLAUDE.md §3's rule
// for render()/render_to_pixels(): if this ever forks into two copies, the
// RNG draw order (7919-decorrelated per index, exactly 9 draws every time
// regardless of which properties are actually random) and the effector
// summation must be changed in both, or the two paths silently diverge.
function _duplicatorClonePlacement(dup,k,pivotK,baseDx,baseDy,baseRot,dPos,dRot,dScale,dOpacity,dPosZ,dRotX,dRotY,randMode,is3DLayer){
  var rngK=seededRng(((dup.seed||0)+k*7919)>>>0);
  var rx=rngK(),ry=rngK(),rrr=rngK(),rsx=rngK(),rsy=rngK(),rop=rngK();
  var posK=randMode.position?[(2*rx-1)*dPos[0],(2*ry-1)*dPos[1]]:[k*dPos[0],k*dPos[1]];
  var rotK=randMode.rotation?(2*rrr-1)*dRot:k*dRot;
  var scaleK=randMode.scale?[(2*rsx-1)*dScale[0],(2*rsy-1)*dScale[1]]:[k*dScale[0],k*dScale[1]];
  var opK=randMode.opacity?(2*rop-1)*dOpacity:k*dOpacity;
  var rz=rngK(),rrx=rngK(),rry=rngK();
  var dzK=randMode.position?(2*rz-1)*dPosZ:k*dPosZ;
  var drxK=randMode.rotation?(2*rrx-1)*dRotX:k*dRotX;
  var dryK=randMode.rotation?(2*rry-1)*dRotY:k*dRotY;
  var instX=pivotK.x+baseDx,instY=pivotK.y+baseDy;
  (dup.effectors||[]).forEach(function(eff){
    var ddx=instX-(eff.pos?eff.pos.x:0),ddy=instY-(eff.pos?eff.pos.y:0);
    var w;
    if(eff.falloff==='linear'){
      var rad=(eff.angle||0)*Math.PI/180;
      var proj=ddx*Math.cos(rad)+ddy*Math.sin(rad);
      w=Math.max(0,Math.min(1,1-proj/(eff.radius||1)));
    }else{
      var dist=Math.hypot(ddx,ddy);
      w=Math.max(0,Math.min(1,1-dist/(eff.radius||1)));
    }
    w*=(eff.strength!=null?eff.strength:100)/100;
    if(!w)return;
    effectorChannels(eff).forEach(function(ch){
      var v=ch.value||[];
      switch(ch.prop){
        case 'position':posK[0]+=w*(v[0]||0);posK[1]+=w*(v[1]||0);break;
        case 'positionZ':dzK+=w*(v[0]||0);break;
        case 'rotation':rotK+=w*(v[0]||0);break;
        case 'rotationX':drxK+=w*(v[0]||0);break;
        case 'rotationY':dryK+=w*(v[0]||0);break;
        case 'scale':scaleK[0]+=w*(v[0]||0);scaleK[1]+=w*(v[1]||0);break;
        case 'opacity':opK+=w*(v[0]||0);break;
      }
    });
  });
  var opFactor=Math.max(0,Math.min(1,1+opK/100));
  var dup3D=(is3DLayer&&(dzK||drxK||dryK))?{dz:dzK,drx:drxK,dry:dryK}:null;
  return{dx:baseDx+posK[0],dy:baseDy+posK[1],rot:baseRot+rotK,sx:1+scaleK[0]/100,sy:1+scaleK[1]/100,opacityFactor:opFactor,dup3D:dup3D};
}
// Hard cap mirrors _registerCap's "never unbounded" philosophy
// (engine-bridge.js) — a typo'd count degrades to "big", never "hangs".
// Shared so the nativeVideo render path can't drift to a different ceiling.
function _duplicatorCount(dup){
  var mode=dup.mode||'grid';
  return mode==='grid'
    ?Math.min(900,Math.max(1,dup.rows||1)*Math.max(1,dup.cols||1))
    :Math.min(500,Math.max(1,dup.count||1));
}
// The mode-specific (grid/radial/path) base offset for clone `k`, BEFORE
// any RNG stagger or effector contribution — shared with the nativeVideo
// render path for the same reason as _duplicatorClonePlacement above.
// `pathInfo` is _resolveDuplicatorPath's result (null unless mode==='path').
function _duplicatorModeOffset(dup,mode,k,count,cols,pathInfo){
  var baseDx=0,baseDy=0,baseRot=0;
  if(mode==='grid'){
    baseDx=(k%cols)*(dup.spacingX||0);
    baseDy=Math.floor(k/cols)*(dup.spacingY||0);
  }else if(mode==='radial'){
    var span=dup.endAngle!=null?(dup.endAngle-dup.startAngle):360;
    var ang=(dup.startAngle||0)+k*(span/count);
    var rr=ang*Math.PI/180;
    baseDx=(dup.radius||0)*Math.cos(rr);
    baseDy=(dup.radius||0)*Math.sin(rr);
    if(dup.radialOrient)baseRot=ang;
  }else{ // path
    var t=pathInfo.closed?k/count:(count>1?k/(count-1):0);
    var off=Math.min(pathInfo.length,t*pathInfo.length);
    var pt=pathInfo.path.getPointAt(off)||pathInfo.start;
    baseDx=pt.x-pathInfo.start.x;
    baseDy=pt.y-pathInfo.start.y;
    if(dup.pathAlignTangent){var tan=pathInfo.path.getTangentAt(off);if(tan)baseRot=tan.angle;}
  }
  return{baseDx:baseDx,baseDy:baseDy,baseRot:baseRot};
}
// opts (2026-07-30 fix, found live: a Duplicator set on a Component's OWN
// inner sub-layer rendered correctly only while editing INSIDE the symbol
// — state.layers is aliased to sym.layers there, so loadFrame's normal
// getEffectiveStrokesRendered call reaches it. Composited from OUTSIDE
// (placed instance, StoryBoard montage), getEffectiveStrokes's ld.symbolId
// branch below reads every symLayer directly and never applied its
// duplicator at all — same "consumer A handles it, consumer B doesn't"
// shape as CLAUDE.md §1). That branch has no `layerIdx` into state.layers
// for a symLayer (it isn't one), so the temporal-offset sub-feature's
// getEffectiveStrokes(layerIdx,shiftedIdx) re-sample can't work unmodified
// — opts.resample/opts.spanStart/opts.spanLen let a caller substitute its
// own frame lookup and span bounds instead. Omitted (as getEffectiveStrokes
// Rendered's call below still does), this behaves exactly as before.
function applyLayerDuplicator(ld,base,frameIdx,layerIdx,opts){
  opts=opts||{};
  var dup=ld.duplicator,mode=dup.mode||'grid';
  var count=_duplicatorCount(dup);
  if(count<=1)return base;
  var pivot=_boundsCenterOfStrokes(base);
  var M=window.SMMotion;
  // Temporal stagger (2026-07-29, LottieFiles Duplicator "Animation" tab
  // equivalent — the user explicitly asked for this after seeing it there):
  // each copy re-samples the SEED LAYER'S OWN content (getEffectiveStrokes,
  // NOT the placement/index stagger below — that stays a separate axis,
  // same as LottieFiles' own Pattern/Animation tab split) at a per-copy
  // TIME-SHIFTED frame instead of reusing `base` for every copy. This is
  // the natural fit for Nemo's frame-based drawn content (a hand-drawn walk
  // cycle, a tweened sequence, or a Component's own internal timeline —
  // getEffectiveStrokes' symbolId branch already resolves that) rather than
  // LottieFiles' purely-numeric position/rotation loop. Wraps within the
  // seed layer's own effective span (layerInPoint..layerOutPoint) so the
  // "wave" effect actually loops instead of freezing at a hard edge.
  var tOff=dup.timeOffset,tOffOn=tOff&&tOff.enabled&&(opts.resample||layerIdx!=null)&&(tOff.offsetFrames|0)!==0;
  var tInF,tSpan;
  if(tOffOn){
    tInF=opts.spanStart!=null?opts.spanStart:layerInPoint(ld);
    tSpan=Math.max(1,opts.spanLen!=null?opts.spanLen:(layerOutPoint(ld)-tInF+1));
  }
  var dPos=M?M.valueAtFrame(ld,'dupOffsetPos',frameIdx):[0,0];
  var dRot=M?M.valueAtFrame(ld,'dupOffsetRot',frameIdx)[0]:0;
  var dScale=M?M.valueAtFrame(ld,'dupOffsetScale',frameIdx):[0,0];
  var dOpacity=M?M.valueAtFrame(ld,'dupOffsetOpacity',frameIdx)[0]:0;
  // positionZ/rotationX/rotationY (2026-07-30, "en 3D aussi avec ID de
  // chaque cloner") — same k×delta/±delta stagger as the original 4,
  // ADDED alongside them rather than folded into one generic per-property
  // loop: a generic loop would change how many values seededRng draws
  // before reaching scale/opacity's own draws below, silently reshuffling
  // every EXISTING project's random-mode scale/opacity pattern the moment
  // this shipped (seededRng's stream position is call-count-order
  // sensitive) — appending new draws AFTER the original 6 keeps every
  // pre-existing duplicator byte-identical.
  var dPosZ=M?M.valueAtFrame(ld,'dupOffsetPosZ',frameIdx)[0]:0;
  var dRotX=M?M.valueAtFrame(ld,'dupOffsetRotX',frameIdx)[0]:0;
  var dRotY=M?M.valueAtFrame(ld,'dupOffsetRotY',frameIdx)[0]:0;
  var is3DLayer=!!ld.threeD;
  var randMode=dup.staggerRandom||{};
  var cols=Math.max(1,dup.cols||1);
  var pathInfo=mode==='path'?_resolveDuplicatorPath(dup,frameIdx):null;
  if(mode==='path'&&!pathInfo)return base; // unconfigured/broken path ref: show the seed, not nothing
  var out=[];
  for(var k=0;k<count;k++){
    var baseK=base,pivotK=pivot;
    if(tOffOn){
      var shiftFrames;
      if(tOff.direction==='backward')shiftFrames=(count-1-k)*tOff.offsetFrames;
      else if(tOff.direction==='centerOut')shiftFrames=Math.round(Math.abs(k-(count-1)/2)*tOff.offsetFrames);
      else if(tOff.direction==='random'){
        // Same seeded-per-index precedent as the position/rotation/scale/
        // opacity stagger above (a different prime multiplier so it draws
        // an independent sequence, not the same jitter values reused) —
        // never Math.random() at render time, or the wave reshuffles on
        // every scrub tick instead of holding a stable pattern.
        var rngT=seededRng(((dup.seed||0)+k*104729)>>>0);
        shiftFrames=Math.floor(rngT()*count)*tOff.offsetFrames;
      } else shiftFrames=k*tOff.offsetFrames; // 'forward' (default): copy 0 = current frame, each next copy lags further behind
      var shiftedIdx=tInF+(((frameIdx-tInF-shiftFrames)%tSpan)+tSpan)%tSpan;
      var shifted=opts.resample?opts.resample(shiftedIdx):getEffectiveStrokes(layerIdx,shiftedIdx);
      if(shifted.length){baseK=shifted;pivotK=_boundsCenterOfStrokes(baseK);}
      // Nothing drawn at the shifted frame (a hold-blank span) — fall back
      // to the current frame's own content rather than vanishing that copy.
    }
    var modeOff=_duplicatorModeOffset(dup,mode,k,count,cols,pathInfo);
    var baseDx=modeOff.baseDx,baseDy=modeOff.baseDy,baseRot=modeOff.baseRot;
    // Per-clone RNG draws, effector summation and the resulting placement
    // descriptor all live in _duplicatorClonePlacement (shared with the
    // nativeVideo render path, engine-bridge.js) — see its own comment for
    // why pivotK is passed in rather than always using `pivot`.
    var place=_duplicatorClonePlacement(dup,k,pivotK,baseDx,baseDy,baseRot,dPos,dRot,dScale,dOpacity,dPosZ,dRotX,dRotY,randMode,is3DLayer);
    // Stagger folded into ONE matrix with the mode's own placement:
    // rotate/scale in place around the seed's own bounds-center first, the
    // placement translate last — same inner-transform-then-outer-placement
    // order as a Component's own camera followed by symMatrix.
    var mat=dupMatrixFromDescriptor(place,pivotK);
    var opFactor=place.opacityFactor;
    // Per-clone 3D delta (2026-07-30) — only attached when it can actually
    // do anything (layer is 3D-enabled AND at least one axis actually
    // moved), so an ordinary 2D duplicator's clones carry no extra field.
    // buildSceneJson (engine-bridge.js) reads data.dup3D to build a
    // per-dupIndex 3D projector instead of sharing the single layer-wide
    // one every other item on a 3D layer uses.
    var dup3D=place.dup3D;
    baseK.forEach(function(sd){
      var sd2=applyMatrixToStrokeData(cloneStrokeForTransform(sd),mat);
      sd2.opacity=(sd2.opacity!==undefined?sd2.opacity:1)*opFactor;
      sd2.isDuplicatorCopy=true;sd2.dupIndex=k;
      // Stable per-clone identity ("ID de chaque cloner") — deterministic
      // from the duplicator's own seed + this copy's index, so it's
      // reproducible across re-materializations (every render rebuilds
      // this array from scratch) without a persisted counter. Distinct
      // from strokeId, which every clone inherits VERBATIM from the seed
      // shape (cloneStrokeForTransform, deliberate — see group-bridge.js's
      // own composite-key comment): dupCloneId is what actually tells
      // copies of the SAME seed shape apart.
      sd2.dupCloneId='dup'+(dup.seed||0)+'_'+k;
      if(dup3D)sd2.dup3D=dup3D;
      out.push(sd2);
    });
  }
  if(pathInfo)pathInfo.path.remove();
  return out;
}
// The ONLY sanctioned way to get a layer's content WITH the duplicator
// applied. getEffectiveStrokes itself stays untouched — its ~16 other call
// sites (tween baking, ensureKeyframe, onion ghosts, status-bar count…)
// must keep seeing the single real seed shape. Only loadFrame and
// export.js's two frame builders substitute this in.
function getEffectiveStrokesRendered(layerIdx,frameIdx){
  var base=getEffectiveStrokes(layerIdx,frameIdx);
  var ld=state.layers[layerIdx];
  if(!ld||!ld.duplicator||ld._dupEditSource||!base.length)return base;
  return applyLayerDuplicator(ld,base,frameIdx,layerIdx);
}
// ---- RIG (2026-07-29) — "un système de rig à la Shapper", promoted from
// the dormant src/js/labs/rig-deform.js prototype. Shapper (the studio's
// own AE mask-influence extension) has no bone HIERARCHY at all: a handful
// of movable control points/paths, each shape vertex carries a 0..1
// influence weight per control, and posing offsets every vertex it
// influences by weight × the control's own delta from rest. Two
// deliberate departures from both Shapper and the labs prototype, per
// explicit request: bones are drawn as real VECTOR PATHS via a Pen-tool
// interaction (rig-bridge.js), not abstract point placement; and a bone's
// LOCAL TANGENT (sampled continuously along its curve, not one chord angle
// for the whole bone) drives rotation-aware skinning when a bind has
// rotate mode on — Shapper itself only ever used the straight chord
// between two mask points.
//
// Persisted shape, plain JSON throughout (CLAUDE.md §1: a live Paper
// reference never survives save/reload — the labs prototype's
// `binds[i].path` was exactly this anti-pattern, fixed here from the
// start): ld.rig = {bones:{id:{segments,restSegments,closed}},
// ikChains:{endId:{...}}, binds:[{strokeId,rest,weights,rotate}], nextId}.
// `binds[i]._live` (underscore, runtime-only) is the resolved live Paper
// item, rebuilt by relinkRigBinds after every loadFrame — never persisted.
function ensureLayerRig(ld){
  if(!ld.rig)ld.rig={bones:{},ikChains:{},binds:[],nextId:1};
  return ld.rig;
}
function rigFreshId(rig,prefix){return prefix+(rig.nextId++);}
// A bone's segments (or restSegments) as a throwaway Paper Path, purely for
// getNearestLocation/getLocationAt/tangent sampling — never inserted into
// any layer.
function _boneSegsToPath(segs,closed){
  var p=new Path({insert:false});
  segs.forEach(function(s){p.add(new Segment(new Point(s.point[0],s.point[1]),new Point((s.handleIn||[0,0])[0],(s.handleIn||[0,0])[1]),new Point((s.handleOut||[0,0])[0],(s.handleOut||[0,0])[1])));});
  if(closed)p.closed=true;
  return p;
}
// Rebuilds each bind's LIVE Paper reference after loadFrame replaced
// userLayers[i].children (the old references are orphaned the moment a
// layer rebuilds) — same strokeId-lookup pattern as
// relinkBrushCompanions/relinkLinkedFills, called right alongside them. A
// bind whose target vanished, or whose vertex COUNT no longer matches its
// stored `rest` (the underlying stroke was sculpted/erased/boolean'd since
// bind time), is DROPPED rather than risk deforming the wrong vertices —
// "in doubt, drop," the same posture §5quater's `_canReuseMaterialized`
// takes for "in doubt, rebuild."
function relinkRigBinds(ld,layer){
  if(!ld.rig||!ld.rig.binds||!ld.rig.binds.length)return;
  var byId={};
  layer.children.forEach(function(c){if(c.data&&c.data.strokeId)byId[c.data.strokeId]=c;});
  ld.rig.binds=ld.rig.binds.filter(function(b){
    var item=byId[b.strokeId];
    if(!item||!item.segments||item.segments.length!==b.rest.length)return false;
    b._live=item;
    return true;
  });
}
// Binds `path`'s own vertices to one or more bones, weighted by distance
// to the NEAREST POINT ANYWHERE ALONG each bone's path (getNearestLocation
// — already precedented elsewhere in this codebase, e.g. tools.js's
// t-junction snapping, tweens.js's tween feature matching), not just to a
// bone's own control points — a straight/curved bone influences vertices
// along its whole length, not only at its ends. `offset` (the bone's own
// path-length offset of the nearest point) is captured once here and
// re-sampled at pose time via getLocationAt — same offset, current bone
// geometry — so both position AND local tangent stay correctly anchored
// to the SAME point on the bone as it's posed.
function rigBindStroke(ld,path,boneIds,radius,rotate,softness){
  var rig=ensureLayerRig(ld);
  // CompoundPath BEFORE the segments/length guard, deliberately: a
  // CompoundPath has no `.segments` of its own (its geometry lives in
  // `.children`, each a Path) — checking segments first would catch every
  // CompoundPath as "invalid or empty target" and this dedicated,
  // more-useful message would never fire (verified live: it didn't, until
  // this reorder).
  if(!path){console.warn('[rig] cible invalide');return false;}
  if(path.className==='CompoundPath'){console.warn('[rig] les CompoundPath (résultats booléens à trous/îles) ne sont pas encore supportés par le rig');return false;}
  if(!path.segments||!path.segments.length){console.warn('[rig] cible invalide ou vide');return false;}
  // Vector-brush ribbons (isVectorBrush) and their linked-fill companion
  // (isLinkedFillCompanion) are supported like any other Path (2026-07-29,
  // Cyril: "l'idée c'est la même que pour Shapper... les points de vecteurs
  // suivent les bones") — applyRigDeform below is fully generic, moving
  // whatever Path is bound vertex-by-vertex with no type-specific logic, and
  // desP (app.js) reconstructs a vector-brush stroke straight from its
  // stored `.segments` (the ribbon's actual outline), not from
  // centerSegments/widthProfile, so a rig-posed deformation persists through
  // save/reload exactly like any other shape's.
  ensureStrokeId(path);
  // Re-binding the SAME shape (canvas click-to-bind makes this the common
  // case, not a rare mistake — clicking a shape again to change which bone
  // it follows, or to widen the radius, must replace, not stack) used to
  // just push a second entry: applyRigDeform iterates ld.rig.binds and SUMS
  // every entry touching a given strokeId, so a duplicate silently doubled
  // that shape's deformation instead of cleanly superseding it.
  rig.binds=rig.binds.filter(function(b){return b.strokeId!==path.data.strokeId;});
  radius=radius||200;
  // Falloff softness (2026-07-30, "il n'y a qu'un rayon dur... aucune
  // courbe d'adoucissement" — a hard linear taper was the only option,
  // unlike the Duplicator's own Effector, which already has a proper
  // falloff convention elsewhere in this app). 0 = the original plain
  // linear taper, BYTE-IDENTICAL to every project bound before this
  // existed; 1 = full smoothstep (3t²-2t³, zero derivative at both the
  // bone itself and the radius edge — the standard "soft" falloff used
  // by every painted-weight rig tool, Blender included). Linearly
  // blended between the two rather than a free exponent: two clearly-
  // named ends (hard vs soft) are easier to reason about while dragging
  // a single slider than an unbounded gamma value would be. One value
  // for the whole layer's bind pass, not per-bone — the per-bone knob
  // that actually varies limb-to-limb is the RADIUS (bone.radius), which
  // already exists; needing a DIFFERENT falloff shape per bone is a much
  // rarer ask than needing a different reach.
  softness=Math.max(0,Math.min(1,softness||0));
  // Shared by the ribbon's own boundary AND (below) its centerSegments —
  // same per-point "nearest offset on each bone's rest curve, falloff by
  // distance" weighting, just fed a different point list.
  function weighForPoints(pts){
    return pts.map(function(pt){
      var w=[],p=new Point(pt[0],pt[1]);
      boneIds.forEach(function(bid){
        var bone=rig.bones[bid];if(!bone)return;
        var bp=_boneSegsToPath(bone.restSegments,bone.closed);
        var loc=bp.getNearestLocation(p);
        bp.remove();
        if(!loc)return;
        var dist=loc.point.getDistance(p);
        // Per-bone radius override (2026-07-29, Shapper-style influence
        // circles, rig-bridge.js Assign mode) — bone.radius is set by dragging
        // that bone's own on-canvas circle; falls back to the passed-in
        // default (the panel's Rayon de poids field) for a bone that's never
        // been adjusted, so existing single-radius projects are unaffected.
        var boneRadius=bone.radius||radius;
        var t=Math.max(0,1-dist/boneRadius);
        var infl=softness?t+(t*t*(3-2*t)-t)*softness:t;
        if(infl>0)w.push({boneId:bid,offset:loc.offset,w:infl});
      });
      return w;
    });
  }
  var rest=path.segments.map(function(s){return[s.point.x,s.point.y];});
  var weights=weighForPoints(rest);
  var bind={strokeId:path.data.strokeId,rest:rest,weights:weights,rotate:!!rotate,_live:path};
  // Keep centerSegments (the source data any future brush-width edit
  // regenerates the boundary FROM, via rebuildVectorBrushOutline) in sync
  // too — 2026-07-29 fix, Cyril: attaque le chantier of a later width edit
  // silently discarding a rig pose. Deformed independently from its OWN few
  // points (see applyRigDeform's own comment for why a straight per-vertex
  // copy from the boundary isn't used: buildVariableWidthPath's smoothing +
  // round-cap arcs make the boundary's point correspondence to the
  // centerline non-trivial to invert) — same known limitation the ribbon's
  // full many-point boundary doesn't have: a 2-point bezier centerline
  // can't exactly represent an arbitrary multi-joint bend, only approximate
  // its overall displacement. Good enough for what this is actually FOR
  // (a plausible starting point for the NEXT regeneration, not the on-
  // screen render — that stays the boundary's own precise per-vertex deform).
  if(path.data.isVectorBrush&&path.data.centerSegments&&path.data.centerSegments.length){
    // Original (pre-pose) point AND handles — handle rotation in
    // applyRigDeform must always rotate FROM these fixed rest vectors, never
    // from the current live ones, or repeated calls (every drag tick) would
    // compound rotation on top of already-rotated handles instead of
    // recomputing the full rest-to-current rotation each time.
    var centerRest=path.data.centerSegments.map(function(s){return{point:[s.point[0],s.point[1]],handleIn:s.handleIn?[s.handleIn[0],s.handleIn[1]]:null,handleOut:s.handleOut?[s.handleOut[0],s.handleOut[1]]:null};});
    bind.centerRest=centerRest;
    bind.centerWeights=weighForPoints(centerRest.map(function(r){return r.point;}));
  }
  rig.binds.push(bind);
  return true;
}
// Auto-assign (2026-07-29, Cyril's own spec — "un bouton d'assignation qui
// assigne automatiquement les éléments du layer sélectionné aux bones comme
// dans Shapper par rapport à la proximité") — step 2 of the Rig tool's 3-step
// flow. Binds EVERY selectable shape on the layer against EVERY bone in one
// pass (rigBindStroke's own per-bone-radius weighting already produces a
// sensible "nearest bone wins, blended near a boundary" result without
// needing per-shape bone selection) — replaces whatever binds already
// existed (rigBindStroke's own re-bind-replaces-not-stacks fix above), so
// running this again after adjusting an influence circle is the expected
// "try again" gesture, not something that piles up stale binds.
function rigAutoAssignLayer(ld,layer,defaultRadius,rotate,softness){
  var rig=ensureLayerRig(ld);
  var boneIds=Object.keys(rig.bones);
  if(!boneIds.length)return 0;
  // Auto-size the fallback radius (2026-07-29 fix, "l'autoassign marche pas
  // alors que j'ai mis les bones sur la shape") — rigBindStroke's per-vertex
  // weight is a LINEAR falloff clamped to 0 past the radius (its own
  // `Math.max(0,1-dist/boneRadius)`), and the fixed 200px default is
  // narrower than half the width of any shape over ~400px — the common
  // case for a torso/limb on the default 1920x1080 canvas. Auto-assign
  // still created a bind (the toast said "N shape(s) assigned"), but with
  // every outer vertex at 0 weight it visibly did nothing when posed —
  // indistinguishable from "doesn't work" to the artist. Scan every
  // selectable shape's vertices once and grow the radius to the farthest
  // one actually needs, so every vertex gets at least SOME pull from its
  // nearest non-overridden bone; a bone the artist already resized via its
  // own influence circle (Assigner mode) keeps that manual radius, exactly
  // as before.
  var neededRadius=defaultRadius||200;
  layer.children.forEach(function(c){
    if(!(c instanceof Path)||!isSelectablePathChild(c))return;
    c.segments.forEach(function(s){
      boneIds.forEach(function(bid){
        var bone=rig.bones[bid];
        if(!bone||bone.radius)return;
        var bp=_boneSegsToPath(bone.restSegments,bone.closed);
        var loc=bp.getNearestLocation(s.point);
        bp.remove();
        if(!loc)return;
        var dist=loc.point.getDistance(s.point);
        if(dist>neededRadius)neededRadius=dist;
      });
    });
  });
  // Small margin so the single farthest vertex gets a sliver of pull
  // instead of landing exactly on the falloff's zero boundary.
  neededRadius=Math.ceil(neededRadius*1.05);
  // CompoundPaths (boolean results with holes/islands) are the one
  // remaining unsupported case — rigBindStroke's own guard rejects them
  // (no single `.segments` to bind), counted here so the caller can surface
  // an honest toast instead of a silent "0 forme(s) assignée(s)" (2026-07-29:
  // vector-brush strokes used to hit this same silent gap too — now bound
  // like any other Path, see rigBindStroke's own comment).
  var n=0,skippedUnsupported=0;
  layer.children.forEach(function(c){
    if(c instanceof CompoundPath){skippedUnsupported++;return;}
    if(!(c instanceof Path)||!isSelectablePathChild(c))return;
    if(rigBindStroke(ld,c,boneIds,neededRadius,rotate,softness))n++;
    // A vector-brush ribbon's linked-fill companion is NOT independently
    // bound (2026-07-29, superseded attempt below this comment used to try
    // it) — QA-confirmed live on a multi-joint pose: the companion's own
    // sparse 2-point geometry (just its centerline endpoints) can't
    // represent an arbitrarily bent multi-joint curve the same way the
    // ribbon's many finely-sampled points can, so its own weighted
    // deformation traced a near-straight line cutting across the bend
    // instead of following it — a visible mismatched red sliver in the
    // gap. applyRigDeform (below) instead copies the ribbon's OWN already-
    // correctly-deformed boundary onto the companion after every deform,
    // guaranteeing an exact match by construction instead of a second,
    // independently-approximated deformation that can diverge.
  });
  return {n:n,skippedUnsupported:skippedUnsupported};
}
// Live per-vertex deform — called on every pose drag tick AND once more
// right before a commit, so the committed keyframe always matches exactly
// what was last seen live (no drift between preview and bake).
function applyRigDeform(ld){
  var rig=ld.rig;if(!rig||!rig.binds.length)return;
  var boneCache={};
  function bonePaths(bid){
    if(boneCache[bid])return boneCache[bid];
    var bone=rig.bones[bid];
    var entry={rest:_boneSegsToPath(bone.restSegments,bone.closed),cur:_boneSegsToPath(bone.segments,bone.closed)};
    boneCache[bid]=entry;
    return entry;
  }
  rig.binds.forEach(function(b){
    if(!b._live||!b._live.segments)return;
    var segs=b._live.segments;
    for(var i=0;i<segs.length&&i<b.rest.length;i++){
      var restPt=b.rest[i],dx=0,dy=0,sumW=0;
      (b.weights[i]||[]).forEach(function(wentry){
        var bp=bonePaths(wentry.boneId);
        var curLoc=bp.cur.getLocationAt(wentry.offset),restLoc=bp.rest.getLocationAt(wentry.offset);
        if(!curLoc||!restLoc)return;
        sumW+=wentry.w;
        if(b.rotate){
          // Rotate the vertex's own rest-offset FROM the bone by the bone's
          // LOCAL tangent delta at this exact point (sampled continuously
          // along the curve, not one angle for the whole bone — the
          // capability Shapper's own chord-angle model doesn't have), then
          // translate by how far that point itself moved. This is
          // rotation-aware LBS, generalized from "one bone-angle" to "a
          // tangent field along the curve."
          var restAng=Math.atan2(restLoc.tangent.y,restLoc.tangent.x);
          var curAng=Math.atan2(curLoc.tangent.y,curLoc.tangent.x);
          var da=curAng-restAng,cosA=Math.cos(da),sinA=Math.sin(da);
          var offX=restPt[0]-restLoc.point.x,offY=restPt[1]-restLoc.point.y;
          var rOffX=offX*cosA-offY*sinA,rOffY=offX*sinA+offY*cosA;
          dx+=wentry.w*((curLoc.point.x+rOffX)-restPt[0]);
          dy+=wentry.w*((curLoc.point.y+rOffY)-restPt[1]);
        }else{
          dx+=wentry.w*(curLoc.point.x-restLoc.point.x);
          dy+=wentry.w*(curLoc.point.y-restLoc.point.y);
        }
      });
      // Normalize by total weight — but ONLY when it exceeds 1 (2026-07-30
      // fix, "il manque pas mal de chose les tangents", root cause: at a
      // joint where two bones both reach full influence — dist≈0 for each,
      // so infl≈1 for each — sumW≈2 and this vertex used to get displaced
      // ~2x, the ballooning/tearing right at elbows/knees, exactly the
      // failure the "désactive si une forme se déchire sur une courbure"
      // tooltip was quietly asking the user to work around instead of
      // fixing. Clamping the divisor to max(1,sumW) instead of ALWAYS
      // dividing by sumW is deliberate: a single bone's own linear falloff
      // (infl=1-dist/radius, weighForPoints) already tapers smoothly to 0
      // at its radius edge — dividing by a sub-1 sumW there would undo that
      // taper and turn it into a hard cutoff. Only genuine over-saturation
      // (sumW>1, multiple bones both pulling at meaningful strength) gets
      // scaled back down.
      var norm=sumW>1?sumW:1;
      segs[i].point=new Point(restPt[0]+dx/norm,restPt[1]+dy/norm);
    }
    // Vector-brush companion sync (2026-07-29, "les points de vecteurs
    // suivent les bones comme dans Shapper") — copy the ribbon's OWN just-
    // deformed boundary onto its linked-fill companion instead of deforming
    // the companion independently off its own sparse 2-point geometry
    // (QA-confirmed live: that diverged into a visible straight sliver
    // across a multi-joint bend the many-point ribbon correctly followed).
    // Guarantees an exact match by construction, matching this SAME
    // "companion mirrors the ribbon" convention move/scale/rotate already
    // use elsewhere in the app (select-bridge.js/tools.js) — just done here
    // with a full segment copy instead of one shared rigid transform, since
    // a per-vertex LBS deform has no single transform to mirror.
    var bLive=b._live;
    if(bLive.data&&bLive.data.isVectorBrush&&bLive.data.linkedFill&&!bLive.data.linkedFill.removed){
      bLive.data.linkedFill.segments=segs.map(function(s){return new Segment(s.point,s.handleIn,s.handleOut);});
      bLive.data.linkedFill.closed=true;
    }
    // centerSegments sync (2026-07-29, "attaque le chantier" of a later
    // brush-width edit discarding a rig pose — see rigBindStroke's own
    // comment for why this is bound/deformed separately from the boundary
    // above rather than derived from it). Unlike the boundary's per-vertex
    // loop (left untouched — already verified correct, no reason to risk
    // it), this ALSO rotates handleIn/handleOut by each point's own
    // weighted-average local rotation: centerSegments has only 2-4 points
    // with large handles forming real bezier curves, so leaving handles
    // unrotated (fine for the boundary's hundreds of near-straight,
    // already-smoothed segments) would visibly warp the curve the moment a
    // future width edit rebuilds the ribbon from it.
    if(bLive.data&&bLive.data.isVectorBrush&&bLive.data.centerSegments&&b.centerRest&&b.centerWeights){
      var centerSegs=bLive.data.centerSegments;
      for(var ci=0;ci<centerSegs.length&&ci<b.centerRest.length;ci++){
        var cRest=b.centerRest[ci],cRestP=cRest.point,cdx=0,cdy=0,cSumW=0,cSumDaW=0,cTotalW=0;
        (b.centerWeights[ci]||[]).forEach(function(wentry){
          var bp=bonePaths(wentry.boneId);
          var curLoc=bp.cur.getLocationAt(wentry.offset),restLoc=bp.rest.getLocationAt(wentry.offset);
          if(!curLoc||!restLoc)return;
          cTotalW+=wentry.w;
          if(b.rotate){
            var restAng=Math.atan2(restLoc.tangent.y,restLoc.tangent.x);
            var curAng=Math.atan2(curLoc.tangent.y,curLoc.tangent.x);
            var da=curAng-restAng,cosA=Math.cos(da),sinA=Math.sin(da);
            var offX=cRestP[0]-restLoc.point.x,offY=cRestP[1]-restLoc.point.y;
            var rOffX=offX*cosA-offY*sinA,rOffY=offX*sinA+offY*cosA;
            cdx+=wentry.w*((curLoc.point.x+rOffX)-cRestP[0]);
            cdy+=wentry.w*((curLoc.point.y+rOffY)-cRestP[1]);
            cSumDaW+=da*wentry.w;cSumW+=wentry.w;
          }else{
            cdx+=wentry.w*(curLoc.point.x-restLoc.point.x);
            cdy+=wentry.w*(curLoc.point.y-restLoc.point.y);
          }
        });
        // Same over-saturation clamp as the boundary loop above (2026-07-30
        // fix) — cTotalW is a SEPARATE accumulator from cSumW on purpose:
        // cSumW only accumulates in rotate mode (it also drives the handle-
        // rotation average just below) and would wrongly stay 0 in non-
        // rotate mode, which is fine for ITS OWN purpose but wrong as a
        // position-normalization divisor.
        var cNorm=cTotalW>1?cTotalW:1;
        centerSegs[ci].point=[cRestP[0]+cdx/cNorm,cRestP[1]+cdy/cNorm];
        // Always rotated FROM the immutable rest handles (cRest.handleIn/
        // Out), never from the current live ones — see rigBindStroke's own
        // comment on why (idempotent across repeated calls, no compounding).
        if(cSumW>0){
          var avgDa=cSumDaW/cSumW,c2=Math.cos(avgDa),s2=Math.sin(avgDa);
          if(cRest.handleIn)centerSegs[ci].handleIn=[cRest.handleIn[0]*c2-cRest.handleIn[1]*s2,cRest.handleIn[0]*s2+cRest.handleIn[1]*c2];
          if(cRest.handleOut)centerSegs[ci].handleOut=[cRest.handleOut[0]*c2-cRest.handleOut[1]*s2,cRest.handleOut[0]*s2+cRest.handleOut[1]*c2];
        }else{
          if(cRest.handleIn)centerSegs[ci].handleIn=cRest.handleIn.slice();
          if(cRest.handleOut)centerSegs[ci].handleOut=cRest.handleOut.slice();
        }
      }
    }
  });
  Object.keys(boneCache).forEach(function(bid){boneCache[bid].rest.remove();boneCache[bid].cur.remove();});
}
function rigResetPose(ld){
  var rig=ld.rig;if(!rig)return;
  Object.keys(rig.bones).forEach(function(bid){
    var bone=rig.bones[bid];
    bone.segments=JSON.parse(JSON.stringify(bone.restSegments));
  });
  applyRigDeform(ld);
  ld._rigPoseLive=false;
}
// Bakes the current LIVE pose into a real keyframe. Order matters — the
// labs prototype had this backwards (ensureKeyframe AFTER the pose was
// already live), so _smGeomDirty forced loadFrame to rebuild from the OLD
// stored data before saveActiveLayerFrame ran, silently discarding the
// pose. ensureKeyframe MUST run first (mirrors rigMagnetBrush's own,
// already-correct ordering in the same labs file), THEN re-apply the
// deform (ensureKeyframe's own loadFrame just rebuilt fresh, undeformed
// Paper items from the newly-promoted keyframe's stored data), THEN save.
function rigCommitFrame(ld){
  if(!ld.rig||!ld.rig.binds.length){showToast(SM.t('toastNoRiggedStroke'));return false;}
  if(!canEditActiveLayer())return false;
  // Skip when a pose-drag already pushed its own checkpoint (rig-bridge.js,
  // 2026-07-29 fix) — by NOW, bone.segments has already been live-mutated
  // by that drag, so a checkpoint taken here would pair that already-posed
  // rig with the frame's still-unbaked (pre-Commit) strokes: an internally
  // inconsistent snapshot that made undo revert the shape but leave the
  // bone visually still posed. Still pushed here for the rarer case of
  // Commit being clicked with no preceding drag this session.
  if(!ld._rigPoseUndoPushed)pushUndo();
  ld._rigPoseUndoPushed=false;
  ensureKeyframe();
  relinkRigBinds(ld,userLayers[state.activeLayerIdx]);
  applyRigDeform(ld);
  ld._rigPoseLive=false;
  saveActiveLayerFrame();updateUI();
  if(window.SMEngineBridge)SMEngineBridge.renderNow();
  showToast(SM.t('toastRigPoseFrozenOnFrame'));
  return true;
}
// ---- IK (3-point chain, 2-bone law-of-cosines) — ported verbatim from
// the labs prototype's own rigSetIK/rigDragIKEnd; operates on bone-anchor
// POSITIONS only, indifferent to whether they're freeform dots (as in the
// prototype) or a bone path's own segment points (here).
function rigSetIK(ld,rootRef,jointRef,endRef,flip){
  var rig=ensureLayerRig(ld);
  var r=rigAnchorPoint(rig,rootRef),j=rigAnchorPoint(rig,jointRef),e=rigAnchorPoint(rig,endRef);
  if(!r||!j||!e)return false;
  var l1=Math.hypot(j.x-r.x,j.y-r.y),l2=Math.hypot(e.x-j.x,e.y-j.y);
  rig.ikChains[endRef.boneId+':'+endRef.vi]={root:rootRef,joint:jointRef,end:endRef,l1:l1,l2:l2,flip:!!flip};
  return true;
}
function rigAnchorPoint(rig,ref){
  var bone=rig.bones[ref.boneId];if(!bone||!bone.restSegments[ref.vi])return null;
  var s=bone.segments[ref.vi]||bone.restSegments[ref.vi];
  return{x:s.point[0],y:s.point[1]};
}
function rigDragIKEnd(ld,endKey,x,y){
  var rig=ld.rig;var chain=rig&&rig.ikChains[endKey];if(!chain)return false;
  var rootS=rig.bones[chain.root.boneId].segments[chain.root.vi];
  var dx=x-rootS.point[0],dy=y-rootS.point[1],dist=Math.hypot(dx,dy);
  var l1=chain.l1,l2=chain.l2,maxD=l1+l2,minD=Math.abs(l1-l2);
  var clamped=Math.max(minD+1e-6,Math.min(maxD-1e-6,dist));
  var baseAngle=Math.atan2(dy,dx);
  var cosA=Math.max(-1,Math.min(1,(l1*l1+clamped*clamped-l2*l2)/(2*l1*clamped)));
  var a=Math.acos(cosA);
  var jointAngle=baseAngle+(chain.flip?-a:a);
  var jointX=rootS.point[0]+l1*Math.cos(jointAngle),jointY=rootS.point[1]+l1*Math.sin(jointAngle);
  rig.bones[chain.joint.boneId].segments[chain.joint.vi].point=[jointX,jointY];
  // End effector goes to (rootS + clamped distance along baseAngle), NOT the
  // raw (x,y) target — verified live: using the raw target here silently
  // stretched the joint-end segment past l2 whenever the drag went further
  // than the chain's reach (l1+l2), breaking the whole point of a rigid
  // 2-bone IK solve. This reconstructs the exact raw (x,y) when the target
  // was already reachable (clamped===dist, so this equals rootS+(dx,dy)),
  // and the closest reachable point on the baseAngle ray otherwise — always
  // exactly l2 from the joint by the same law-of-cosines construction used
  // for jointAngle above.
  var endX=rootS.point[0]+clamped*Math.cos(baseAngle),endY=rootS.point[1]+clamped*Math.sin(baseAngle);
  rig.bones[chain.end.boneId].segments[chain.end.vi].point=[endX,endY];
  applyRigDeform(ld);
  return true;
}
function resolveSymbolFrameIdx(sym,layer,mainFrameIdx){
  // A key on the parent Animation 2D timeline may explicitly pick the
  // component frame displayed for the whole held span. This is deliberately
  // stored on the OUTER frame entry: symbol-internal drawing keys remain in
  // the component editor and never leak into the parent timeline.
  if(layer.frames){
    for(var keyFi=Math.min(mainFrameIdx,layer.frames.length-1);keyFi>=0;keyFi--){
      var timingKey=layer.frames[keyFi];
      if(!timingKey||!timingKey.isKeyframe)continue;
      if(timingKey.componentFrame!=null){
        return Math.min(Math.max(1,sym.totalFrames)-1,Math.max(0,Math.floor(timingKey.componentFrame)));
      }
      break;
    }
  }
  var elapsed=Math.max(0,(mainFrameIdx-(layer.symPlacedAt||0)))*(layer.symSpeed||1);
  var total=Math.max(1,sym.totalFrames);
  // Time Remap (motion.js) overrides play mode / speed / placement entirely
  // — that is the point of it: a keyframed curve names the internal frame
  // directly, so freeze, hold, reverse and ramp all come from the curve
  // instead of from a fixed mode. Single chokepoint on purpose: every
  // consumer of a component's content (canvas, export, StoryBoard
  // thumbnails) already resolves its frame through here.
  if(layer.timeRemap&&window.SMMotion&&SMMotion.timeRemapValue){
    var rv=SMMotion.timeRemapValue(layer,mainFrameIdx);
    if(rv!=null)return Math.max(0,Math.min(total-1,Math.round(rv)));
  }
  if(layer.symPlayMode==='single')return Math.min(total-1,Math.max(0,Math.floor(layer.symSingleFrame||0)));
  if(layer.symPlayMode==='once'){
    // symTrimIn/symTrimOut are optional (default: play the WHOLE symbol
    // from frame 0, unchanged behavior for every pre-existing Component-
    // instance layer) — only enterMontageView (app.js) sets them, so a
    // montage segment plays back exactly the source range the StoryBoard
    // chain member was trimmed to (storyboard.js's own montageStrokesAt
    // formula: trimIn + floor(local*srcLen/duration), reproduced here via
    // symSpeed=srcLen/duration + symPlacedAt=that member's montage-local
    // start frame), holding at trimOut instead of rolling into unrelated
    // symbol frames beyond the trimmed range.
    var trimIn=layer.symTrimIn||0;
    var trimOut=(layer.symTrimOut!=null)?layer.symTrimOut:total-1;
    return Math.min(trimOut,trimIn+Math.floor(elapsed));
  }
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
// Layer in/out point (layer-inout.js, After-Effects-style layer bar): a
// layer with no explicit ld.inPoint/outPoint spans the whole timeline
// (unset = full range, so every project predating this feature keeps its
// exact old behavior with zero migration). This is the single choke point
// both the live path (loadFrame -> getEffectiveStrokes, app.js) and the
// export path (exportBuildFrame -> getEffectiveStrokes, export.js) already
// share, so gating rendering HERE covers both without touching either
// caller — see CLAUDE.md's "family of bug #1" for why a single shared
// chokepoint beats duplicating this check at every consumer.
// Mirrors autoOutPointFromBlankKeyframe below (same "why" — a manually-
// dragged ld.inPoint always wins once set, this is only the DEFAULT/display
// value when it hasn't been). Bug found 2026-07 ("les inpoints ne reflète
// pas le bon départ en détectant les keyframes vide"): unlike outPoint,
// inPoint had NO leading-blank detection at all — a layer that starts
// drawing at frame 20 still showed an in-point bar starting at frame 0,
// with the first 20 frames a blank stretch nobody could see was there
// without scrubbing to it.
function autoInPointFromBlankKeyframe(ld){
  if(!ld||ld.symbolId)return null;
  var frames=ld.frames,firstNonBlank=-1,curBlank=true;
  for(var f=0;f<frames.length;f++){
    var fr=frames[f];
    if(fr.isKeyframe)curBlank=!fr.strokes.length;
    if(!curBlank&&firstNonBlank<0)firstNonBlank=f;
  }
  return(firstNonBlank>0)?firstNonBlank:null;
}
// `!=null` (not `||0`, the old check) — `||0` couldn't tell "never set"
// apart from "explicitly dragged to exactly frame 0", so an explicit
// user override back to 0 would otherwise be silently reclaimed by the
// auto-detect below on the very next render.
// ---- PARENT IN TIME (Sander van Dijk 2.1, 2026-07-26 + 2026-07-30) ----
// "The Time Properties of a layer remain static. We manually drag, move and
// trim the static blocks called Layers." His idea: In/Out become VALUES that
// can be driven from elsewhere — a link to another layer plus an offset.
//
//   ld.timeLink = { uid, mode: 'both' | 'in' | 'out' }
//   ld.motion.timeLinkInOffset / ld.motionStatic.timeLinkInOffset  (+Out)
//
// The offset itself (2026-07-30) is a real Motion property — expression-
// linkable through the same hasExpr/evalExpressionFor every other property
// uses (motion.js), delivering Van Dijk's "linked together with
// Expressions" without a bespoke second expression system. Deliberately
// NOT true per-frame keyframing (motion.js's PROP_NO_STOPWATCH hides the
// stopwatch on these two rows): see resolveLinkedTime below and
// migrateTimeLinkOffsets for why, and [[project-nemo-parent-in-time-expr]]
// for the full scoping conversation with Cyril. inOffset/outOffset used to
// live as plain numbers directly on ld.timeLink — migrateTimeLinkOffsets
// carries old projects over once, non-destructively.
//
// Resolution lives in layerInPoint/layerOutPoint because they are the single
// chokepoint every consumer already goes through (13 call sites across
// app.js, engine-bridge.js, layer-inout.js, motion.js, timeline.js).
// Unlinked layers pay nothing: the very first check is `!ld.timeLink`.
function timeLinkSourceOf(ld){
  if(!ld||!ld.timeLink||!ld.timeLink.uid)return null;
  for(var i=0;i<state.layers.length;i++){
    var s=state.layers[i];
    if(s!==ld&&s.layerUid===ld.timeLink.uid)return s;
  }
  return null; // source deleted — fall through to the layer's own values
}
// One-time, idempotent — pre-2026-07-30 links stored a plain number at
// ld.timeLink.inOffset/outOffset. Now that those offsets are real Motion
// properties (timeLinkInOffset/timeLinkOutOffset, motion.js — Van Dijk 2.1,
// "linked together with Expressions"), resolveLinkedTime reads them through
// valueAtFrame like any other property. Without this migration an old
// project's offsets would silently reset to 0 the first time it's opened
// after this change (CLAUDE.md §1: a field moving location breaks every
// reader that doesn't know about the new one). Legacy fields are left in
// place, unread from now on, not deleted — non-destructive, same as
// effectorChannels' own migration (motion.js).
function migrateTimeLinkOffsets(ld){
  if(!ld||!ld.timeLink)return;
  if(ld.timeLink.inOffset&&!(ld.motion&&ld.motion.timeLinkInOffset)&&!(ld.motionStatic&&ld.motionStatic.timeLinkInOffset)){
    ld.motionStatic=ld.motionStatic||{};
    ld.motionStatic.timeLinkInOffset=[ld.timeLink.inOffset];
  }
  if(ld.timeLink.outOffset&&!(ld.motion&&ld.motion.timeLinkOutOffset)&&!(ld.motionStatic&&ld.motionStatic.timeLinkOutOffset)){
    ld.motionStatic=ld.motionStatic||{};
    ld.motionStatic.timeLinkOutOffset=[ld.timeLink.outOffset];
  }
}
// A link chain must terminate. Same shape as the spatial parenting guard
// (SMMotion.setLayerParent / parentDescendants): a visited set plus a hard
// depth cap, so a cycle degrades to "use my own value" instead of recursing
// until the stack dies.
function resolveLinkedTime(ld,which,seen,depth){
  var link=ld.timeLink;
  if(!link)return null;
  var mode=link.mode||'both';
  if(which==='in'&&mode==='out')return null;
  if(which==='out'&&mode==='in')return null;
  var src=timeLinkSourceOf(ld);
  if(!src)return null;
  seen=seen||[];
  if(seen.indexOf(ld)>=0||depth>16)return null;
  seen.push(ld);
  // Cross-type source (2026-08-16, spec: "un in-point peut suivre un
  // out-point") — link.srcAnchor overrides which of the SOURCE's edges to
  // read from, when it differs from `which` (the CHILD's own edge). Absent
  // for every link created before this existed (and for any 'both'-mode
  // link, which never sets it — see setLayerTimeLink's own comment for why
  // "both" stays same-type-only), so this is a pure fallback: old projects
  // and same-type links resolve EXACTLY as before.
  var srcWhich=(link.srcAnchor==='in'||link.srcAnchor==='out')?link.srcAnchor:which;
  var base=srcWhich==='in'?layerInPoint(src,seen,depth+1):layerOutPoint(src,seen,depth+1);
  // Expression-only, evaluated off state.currentFrame (confirmed scope with
  // Cyril): layerInPoint/layerOutPoint carry no frame parameter at any of
  // their 13 call sites, so this is "the offset as of right now", not a
  // value that sweeps per exported frame. An export pass that never moves
  // state.currentFrame resolves every frame with the SAME offset — accepted
  // limitation of the small-scope option, not a bug.
  migrateTimeLinkOffsets(ld);
  var offProp=which==='in'?'timeLinkInOffset':'timeLinkOutOffset';
  // valueAtFrame is motion.js-internal (that file's whole body is one IIFE,
  // unlike this one) — only reachable through the SMMotion export, not bare.
  var offVals=window.SMMotion?SMMotion.valueAtFrame(ld,offProp,state.currentFrame):null;
  var off=(offVals&&offVals[0])||0;
  return Math.max(0,Math.min(state.totalFrames-1,base+off));
}
// True when the layer's visible range comes from anywhere other than the
// full timeline — used by getEffectiveStrokes, whose own guard used to test
// ld.inPoint/ld.outPoint directly and would therefore skip the range check
// entirely for a layer whose range comes from a LINK (CLAUDE.md §1: a new
// field that one reader doesn't know about).
function layerHasTimeRange(ld){
  return !!(ld&&(ld.inPoint!=null||ld.outPoint!=null||ld.timeLink));
}
// Both resolvers clamp their result into the CURRENT timeline (2026-08-16).
// Shrinking the project length (SM.setTotalFrames, timeline.js) deliberately
// leaves per-layer data alone — the frames array itself is never truncated
// either, so pulling 120 frames down to 30 and back up restores everything.
// But a stored ld.outPoint of 100 then still RESOLVED to 100 on a 30-frame
// timeline, and layer-inout.js sizes the bar straight off this value: the
// green bar ran ~780px past the end of the ruler and inflated the grid's own
// scroll width with it. Clamping here (the one chokepoint every reader goes
// through — the bar, saveActiveLayerFrame's range guard, getEffectiveStrokes)
// hides the overflow everywhere at once while keeping ld.inPoint/outPoint
// untouched, so the layer's real range comes back if the timeline grows again.
function _clampToTimeline(f){var last=state.totalFrames-1;return f<0?0:(f>last?last:f);}
function layerInPoint(ld,_seen,_depth){
  var linked=resolveLinkedTime(ld,'in',_seen,_depth||0);
  if(linked!=null)return _clampToTimeline(linked);
  if(ld.inPoint!=null)return _clampToTimeline(ld.inPoint);
  var auto=autoInPointFromBlankKeyframe(ld);
  return auto!=null?_clampToTimeline(auto):0;
}
// When the user hasn't manually dragged an out point, default to where the
// layer's own drawing actually stops (its last blank keyframe — F7,
// insertBlankKeyframe — with no non-blank keyframe after it) instead of
// always the full project length. Purely a DEFAULT/display value: content
// past a blank keyframe was already invisible regardless (getEffectiveStrokes
// naturally returns [] for that whole tail, held-frame inheritance included),
// so this changes nothing about what renders — only what the layer-inout.js
// bar honestly shows by default, so a layer that stops drawing at frame 40
// doesn't visually claim to run the full timeline. A manually-dragged
// ld.outPoint always wins over this once set.
function autoOutPointFromBlankKeyframe(ld){
  if(!ld||ld.symbolId)return null; // components have their own Frame/Speed/Offset timing model
  var frames=ld.frames,lastNonBlank=-1,curBlank=true;
  for(var f=0;f<frames.length;f++){
    var fr=frames[f];
    if(fr.isKeyframe)curBlank=!fr.strokes.length;
    if(!curBlank)lastNonBlank=f;
  }
  return(lastNonBlank>=0&&lastNonBlank<frames.length-1)?lastNonBlank:null;
}
function layerOutPoint(ld,_seen,_depth){
  var linked=resolveLinkedTime(ld,'out',_seen,_depth||0);
  if(linked!=null)return _clampToTimeline(linked);
  if(ld.outPoint!=null)return _clampToTimeline(ld.outPoint);
  var auto=autoOutPointFromBlankKeyframe(ld);
  return auto!=null?_clampToTimeline(auto):state.totalFrames-1;
}
// Right-click unlink (2026-07-30, on-timeline anchors/badges/Temps row) must
// leave the layer exactly where it LOOKED while linked — a bare `delete
// ld.timeLink` makes layerInPoint/layerOutPoint above fall through to
// ld.inPoint/outPoint, which are UNSET on a linked layer (trySetLinkedEdge/
// reconcileTimeLinks, layer-inout.js, never write them while a link covers
// that edge) — so deleting the link alone silently reset the layer to full
// range. Found live: Cyril, "le calque se reset alors qu'il doit rester
// comme il était en étant parent, juste le parent in time désactivé."
// Capture the CURRENT resolved values before deleting the link and bake
// them as the new hard ld.inPoint/outPoint — every unlink call site should
// go through this instead of deleting the link directly.
function unlinkTimeLinkPreserveRange(ld){
  if(!ld||!ld.timeLink)return;
  var effIn=layerInPoint(ld),effOut=layerOutPoint(ld);
  delete ld.timeLink;
  ld.inPoint=effIn;ld.outPoint=effOut;
}
// Each branch below does an unconditional early `return`, so if a layer
// ever ended up with more than one of nativeVideo/montageId/lfsGroup/
// symbolId set (shouldn't happen — each is assigned by its own distinct,
// mutually-exclusive creation path: convertLayerToComponent, LFS setup,
// StoryBoard montage placement, native-video import), the FIRST match in
// source order wins silently rather than combining or asserting. Audited
// 2026-07 (code-quality pass): deliberately left as first-match-wins, not
// a runtime assert — no code path today can actually set two of these on
// one layer, so an assert would only add a NEW way for an old/migrated
// project with slightly stray state to crash, for a purely hypothetical
// benefit. Revisit if a future feature ever legitimately needs to combine
// two of these flags on the same layer.
// `countOnly` skips the transform stage (element motion, component camera,
// instance matrix). Every one of those is a 1:1 map — they rewrite geometry,
// never add or drop a stroke — so the COUNT is identical either way, and a
// caller that only needs the count should not pay for the clones. updateUI
// (timeline.js) was calling the full path once per frame purely to print the
// status-bar stroke count, doubling the per-frame transform cost of every
// scrub and playback tick (2026-07-28). Do NOT use countOnly to get strokes
// you intend to draw: the returned dicts are untransformed.
function getEffectiveStrokes(layerIdx,frameIdx,countOnly){
  var ld=state.layers[layerIdx];if(!ld)return[];
  if(layerHasTimeRange(ld)&&(frameIdx<layerInPoint(ld)||frameIdx>layerOutPoint(ld)))return[];
  // EXPERIMENTAL (native-video-decode): a natively-decoded video layer has
  // no vector strokes at all — its picture is an engine-side image item
  // (buildSceneJson, engine-bridge.js), not frame data.
  if(ld.nativeVideo)return[];
  // Null/Effect layer (2026-07, Motion) — never have real content by
  // design (see SM.addNullLayer/addEffectLayer's own comments); same
  // "no strokes to speak of" early-return as nativeVideo above.
  if(ld.isNullLayer||ld.isEffectLayer||ld.isGuideLayer)return[];
  // StoryBoard montage layer (storyboard.js, 2026-07): the layer's content
  // at frame f IS the montage's resolved frame (looping) — the montage
  // stays the single source of truth, edits in the node space show up
  // here live, like a precomp. Placement offset via symPlacedAt (the same
  // field component layers already use, already persisted).
  if(ld.montageId){
    if(!window.SMStoryboard)return[];
    var mtg=SMStoryboard.montageById(ld.montageId);if(!mtg)return[];
    var mtot=SMStoryboard.montageTotal(mtg);if(!mtot)return[];
    var mlf=frameIdx-(ld.symPlacedAt||0);
    if(mlf<0)return[];
    return SMStoryboard.montageStrokesAt(mtg,mlf%mtot);
  }
  if(ld.lfsGroup){
    // Traditional cel stacking: shadow at back, full (flat colors) in the
    // middle, line (ink) on top.
    return getLFSSubStrokes(ld,'shadow',frameIdx)
      .concat(getLFSSubStrokes(ld,'full',frameIdx))
      .concat(getLFSSubStrokes(ld,'line',frameIdx));
  }
  if(ld.symbolId){
    var sym=state.symbols[ld.symbolId];if(!sym)return[];
    // A user-placed blank keyframe (F7) on the component's OWN main-timeline
    // row now genuinely hides the component for that span, like a normal
    // layer — see insertBlankKeyframe's blankOverride flag. Every component
    // layer's frames[] is otherwise pure timing decoration, never read for
    // content, so this is the one deliberate exception.
    // Blank/component-frame choices are held until the next outer key,
    // exactly like ordinary Animation 2D keyframes. The former exact-frame
    // check made an F7 component disappear for one frame and immediately
    // reappear throughout what the timeline displayed as a blank span.
    for(var ownFi=Math.min(frameIdx,ld.frames.length-1);ownFi>=0;ownFi--){
      var ownF=ld.frames[ownFi];
      if(!ownF||!ownF.isKeyframe)continue;
      if(ownF.blankOverride)return[];
      break;
    }
    var ii=resolveSymbolFrameIdx(sym,ld,frameIdx);
    // Composite EVERY visible sub-layer of the component, not just the
    // first — a multi-layer component (convertLayersToComponent) used to
    // only ever render sym.layers[0] into the parent scene, silently
    // dropping the rest even though they're all visible and fully editable
    // inside the component itself.
    var out=[];
    if(countOnly){
      sym.layers.forEach(function(symLayer){
        if(!symLayer||symLayer.visible===false)return;
        var sf=symLayer.frames[ii];if(!sf)return;
        if(sf.isKeyframe||sf.isInterpolated){out=out.concat(sf.strokes);return;}
        for(var k=ii-1;k>=0;k--){if(symLayer.frames[k].isKeyframe){out=out.concat(symLayer.frames[k].strokes);break;}}
      });
      return out;
    }
    sym.layers.forEach(function(symLayer){
      if(!symLayer||symLayer.visible===false)return;
      var sf=symLayer.frames[ii];if(!sf)return;
      var layerStrokes=null;
      if(sf.isKeyframe||sf.isInterpolated){layerStrokes=sf.strokes;}
      else{for(var k=ii-1;k>=0;k--){if(symLayer.frames[k].isKeyframe){layerStrokes=symLayer.frames[k].strokes;break;}}}
      if(!layerStrokes)return;
      // Element-level Motion (2026-07, "precomp par calque"): a per-shape
      // animated Position/Anchor/Rotation/Scale/Opacity INSIDE this component
      // instance — same descriptor and nesting order exportBuildFrame
      // (export.js) already uses for plain layers (elMat applied first,
      // pivoted around the STROKE's own bounds+anchor, before any outer/
      // instance-level transform). Reused here via a throwaway Path since
      // these are still raw stroke-data dicts (loadFrame builds the real
      // Paper items from this function's return value, not before it).
      // elementMotionAt used to always return null for a ld.symbolId layer —
      // lifted in motion.js alongside this change. Applied PER SUB-LAYER
      // (not on the flattened `out`, see the layer-level step right below)
      // so the layer's own pivot, computed next, reflects each element's
      // CURRENT (already element-transformed) position — same "shape
      // group's transform nested inside its parent layer" order AE uses.
      if(window.SMMotion){
        layerStrokes=layerStrokes.map(function(sd){
          if(sd.isRaster||!sd.strokeId)return sd;
          var elMat=SMMotion.elementMotionAt(layerIdx,sd.strokeId,frameIdx);
          if(!elMat)return sd;
          var sd2=JSON.parse(JSON.stringify(sd));
          var tmp=new Path({insert:false});
          for(var si=0;si<sd2.segments.length;si++){var s=sd2.segments[si];tmp.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));}
          if(sd2.closed)tmp.closed=true;
          var epc=tmp.bounds.center;
          var elPivot=new Point(epc.x+elMat.ax,epc.y+elMat.ay);
          tmp.scale(elMat.sx,elMat.sy,elPivot);
          tmp.rotate(elMat.rot,elPivot);
          tmp.translate(elMat.dx,elMat.dy);
          sd2.segments=tmp.segments.map(function(s){return{point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};});
          sd2.opacity=(sd2.opacity!==undefined?sd2.opacity:1)*elMat.op;
          return sd2;
        });
      }
      // Layer-level Motion (2026-07-29 fix — found live while beta-testing:
      // a multi-layer component's sub-layer can carry a layer-level Motion
      // track exactly like any ordinary layer, set while editing INSIDE via
      // enterSymbol — this was never composed here at all, so the sub-
      // layer's own Position/Rotation/Scale/Opacity keyframes silently did
      // nothing the moment you exited back to the placed instance, even
      // though they still animated correctly while editing inside. One
      // nesting level further out than element motion above — mirrors
      // parentChainMats' own pivot convention (a layer's bounds-center, not
      // an individual stroke's).
      var layerMat=window.SMMotion?SMMotion.computeMotionMatFor(symLayer,ii):null;
      if(layerMat){
        var pivot=_boundsCenterOfStrokes(layerStrokes);
        pivot={x:pivot.x+layerMat.ax,y:pivot.y+layerMat.ay};
        layerStrokes=layerStrokes.map(function(sd){
          var sd2=JSON.parse(JSON.stringify(sd));
          if(sd2.isRaster){
            var rb=SMMotion.transformImageRect({x:sd2.x-sd2.width/2,y:sd2.y-sd2.height/2,width:sd2.width,height:sd2.height,rotation:sd2.rotation||0},pivot,layerMat);
            sd2.x=rb.x+rb.width/2;sd2.y=rb.y+rb.height/2;sd2.width=rb.width;sd2.height=rb.height;
            if(rb.rotation)sd2.rotation=rb.rotation;else delete sd2.rotation;
          }else if(sd2.segments){
            sd2.segments=SMMotion.transformSegments(sd2.segments,pivot,layerMat);
          }
          sd2.opacity=(sd2.opacity!==undefined?sd2.opacity:1)*layerMat.op;
          return sd2;
        });
      }
      // Duplicator (2026-07-30 fix, see applyLayerDuplicator's own header
      // comment) — applied AFTER element/layer motion above, same order a
      // top-level layer's duplicator sees relative to ITS OWN transform:
      // the seed is animated first, then multiplied. _dupEditSource mirrors
      // getEffectiveStrokesRendered's own guard (app.js) — a symLayer
      // mid-"edit the duplicator's source shape directly" session shows its
      // single seed, not N stale copies.
      if(symLayer.duplicator&&!symLayer._dupEditSource&&layerStrokes.length){
        layerStrokes=applyLayerDuplicator(symLayer,layerStrokes,ii,null,{
          spanStart:0,spanLen:symLayer.frames.length,
          resample:function(shiftedIdx){
            var rf=symLayer.frames[shiftedIdx];if(!rf)return[];
            if(rf.isKeyframe||rf.isInterpolated)return rf.strokes||[];
            for(var kk=shiftedIdx-1;kk>=0;kk--){if(symLayer.frames[kk].isKeyframe)return symLayer.frames[kk].strokes||[];}
            return[];
          }
        });
      }
      out=out.concat(layerStrokes);
    });
    // The instance transform (symMatrixOf) — skip entirely (and the clone
    // it requires) when it's identity, the common case. Strokes returned
    // here are live references into the SYMBOL'S OWN stored frame data, so
    // transforming in place would permanently corrupt the symbol itself;
    // every stroke is deep-cloned first.
    // A Component with its own camera keys (set while inside it via
    // enterSymbol's cameraKeys swap, see §8 CLAUDE.md) bakes that camera's
    // pan/zoom/roll into its rendered content HERE, at the one chokepoint
    // every consumer (buildSceneJson, StoryBoard thumbnails, export) already
    // shares — so the Component's camera move travels with it wherever the
    // instance is placed, same as its own layers/keyframes/tweens do.
    if(sym.cameraKeys&&sym.cameraKeys.length&&window.SMCamera){
      var camM=SMCamera.cameraMatrixAtFrame(sym.cameraKeys,ii,state.canvasW,state.canvasH);
      if(camM)out=out.map(function(sd){return applyMatrixToStrokeData(cloneStrokeForTransform(sd),camM);});
    }
    if(ld.symMatrix){
      var m=symMatrixOf(ld);
      out=out.map(function(sd){return applyMatrixToStrokeData(cloneStrokeForTransform(sd),m);});
    }
    // Exposed-property overrides (2026-08-18, see exposeSymbolProperty's own
    // comment) — resolved LAST, after every transform above, since these
    // touch paint/visibility fields a matrix never does. `ld` (the INSTANCE
    // layer, not the symbol) is what carries each property's actual value —
    // SMMotion.valueAtFrame already knows how to fall back through
    // animated→static→PROP_DEFAULT for any prop key, exposed ones included,
    // so this reuses that evaluator directly rather than re-deriving a
    // default here. Re-registers metadata every call (cheap, idempotent) —
    // see propsFor's identical reasoning for why this can't be a one-time
    // registration at exposure time alone.
    if(sym.exposedProps&&sym.exposedProps.length&&window.SMMotion){
      sym.exposedProps.forEach(function(ep){if(SMMotion.registerExposedPropMeta)SMMotion.registerExposedPropMeta(ep.key,ep.label,ep.default);});
      var epByStroke={};
      sym.exposedProps.forEach(function(ep){(epByStroke[ep.targetStrokeId]=epByStroke[ep.targetStrokeId]||[]).push(ep);});
      out=out.map(function(sd){
        if(!sd.strokeId||!epByStroke[sd.strokeId])return sd;
        var sd2=null,hide=false;
        epByStroke[sd.strokeId].forEach(function(ep){
          var val=SMMotion.valueAtFrame(ld,ep.key,frameIdx)[0];
          if(ep.targetField==='__visible'){if(val<50)hide=true;return;}
          if(!sd2)sd2=JSON.parse(JSON.stringify(sd));
          if(ep.targetField==='opacity')sd2.opacity=Math.max(0,Math.min(1,val/100));
          else sd2[ep.targetField]=val;
        });
        return hide?null:(sd2||sd);
      }).filter(function(sd){return sd;});
    }
    return out;
  }
  // Plain layer, the overwhelmingly common case — hot path, called every
  // frame load/scrub/playback tick for every layer, so deliberately NOT
  // cloned (unlike the symbolId branch above, which explicitly documents
  // why it skips cloning too "when it's identity, the common case" — same
  // perf reasoning CLAUDE.md §5 already went through a whole optimization
  // pass over). Returns a LIVE reference into the stored frame's own
  // `strokes` array (current keyframe, or an inherited earlier keyframe's
  // array via the loop below) — safe only as long as every caller treats
  // it as read-only or clones before mutating (every current caller does:
  // loadFrame's desR/desP build NEW Paper objects from it rather than
  // mutating it in place). A future caller that mutates this return value
  // directly would corrupt the STORED keyframe — and for the inherited-
  // frame branch, corrupt the keyframe at frameIdx-i, not the frame the
  // caller thinks it's touching. Clone with JSON.parse(JSON.stringify(...))
  // before any in-place mutation, same pattern used everywhere else in
  // this file that needs an independent copy.
  var f=ld.frames[frameIdx];if(!f)return[];
  if(f.isKeyframe||f.isInterpolated)return f.strokes;
  for(var i=frameIdx-1;i>=0;i--){if(ld.frames[i].isKeyframe)return ld.frames[i].strokes;}
  return [];
}
var _symbolPaperLayers={},_sceneSnapshot=null;
var _montageViewSnapshot=null; // enterMontageView/exitMontageView — see below
function ensureSymbolPaperLayers(symId){
  if(_symbolPaperLayers[symId])return _symbolPaperLayers[symId];
  var sym=state.symbols[symId];var arr=[];
  arcLayer.activate();
  sym.layers.forEach(function(ld,idx){var l=new Layer({name:'sym-'+symId+'-'+idx});l.insertBelow(arcLayer);l.visible=false;arr.push(l);});
  _symbolPaperLayers[symId]=arr;
  return arr;
}
function genSymbolId(){return 'sym_'+Date.now()+'_'+Math.floor(Math.random()*10000);}
// Refuses source types whose "content" isn't the plain `.frames` array a
// component's symLayer clones below — mirrors mergeLayersIntoOne's own
// refusal list (app.js, same reasoning: converting one of these would
// either silently drop what it actually stands for, since a raw
// JSON.parse(JSON.stringify(ld.frames)) clone doesn't capture a montage's
// StoryBoard graph or a native video's decode source, or double-wrap
// something already symbol-shaped). Missing here entirely before this fix
// (2026-07-30) — convertLayerToComponent/convertLayersToComponent had no
// type guard at all, unlike every sibling structural op.
function badComponentSourceReason(l){
  if(l.symbolId)return'un composant';
  if(l.lfsGroup)return'un groupe Ligne/Plein/Ombre';
  if(l.montageId)return'un calque de montage StoryBoard';
  if(l.nativeVideo)return'un calque vidéo';
  if(l.isNullLayer)return'un calque Null';
  if(l.isEffectLayer)return'un calque d\'effet';
  if(l.isGuideLayer)return'un calque Guide';
  return null;
}
// Whitelist-clones a rig for a symLayer, dropping binds[i]._live (the live
// Paper.js Path reference relinkRigBinds rebuilds after every loadFrame —
// see exportJSON's own identical clone, timeline.js) — same shape as the
// matteMode/blendMode/motion fields these two convert-to-Component
// functions already carry over, found missing entirely by live testing
// (2026-07-30): rigging a shape then triggering the Motion auto-conversion
// to Component (maybeAutoConvertToComponent) silently dropped every bone/
// bind/IK chain — the symbol's own inner layer came out with rig:undefined,
// while the outer instance kept its now-orphaned rig data on a locked,
// content-wiped layer. Binds reference strokeId, unaffected by the frames
// clone happening alongside this at each call site (same reasoning as
// duplicateLayer's own rig comment, timeline.js).
function cloneRigForSymbol(rig){
  if(!rig)return undefined;
  return{bones:rig.bones,ikChains:rig.ikChains,nextId:rig.nextId,
    binds:(rig.binds||[]).map(function(b){return{strokeId:b.strokeId,rest:b.rest,weights:b.weights,rotate:b.rotate};})};
}
// Component exposed properties (2026-08-18) — declares a property on the
// SYMBOL (state.symbols[symId].exposedProps), bound to one stroke inside it
// by its stable strokeId. Every instance of this symbol then gets it as an
// ordinary Motion property (propsFor, motion.js — registers PROP_LABEL/DIM/
// UNIT/DEFAULT there, not here, so a project reload rehydrates it the first
// time propsFor is called rather than needing a separate boot-time pass).
// `targetField` is a real field name on a serialized stroke dict
// ('fillColor','strokeColor','opacity'...) EXCEPT the sentinel '__visible',
// which getEffectiveStrokes reads as "drop this stroke from the instance's
// output entirely" rather than writing it onto the dict.
function exposeSymbolProperty(symId,targetStrokeId,targetField,label,defaultVal){
  var sym=state.symbols[symId];if(!sym)return null;
  if(!sym.exposedProps)sym.exposedProps=[];
  var key='ep_'+Date.now().toString(36)+'_'+Math.random().toString(36).slice(2,6);
  var entry={key:key,label:label,targetStrokeId:targetStrokeId,targetField:targetField,default:defaultVal};
  sym.exposedProps.push(entry);
  if(window.SMMotion&&SMMotion.registerExposedPropMeta)SMMotion.registerExposedPropMeta(key,label,defaultVal);
  return entry;
}
// NOT attached to window.SM here — timeline.js (loaded after this file,
// index.html) assigns `window.SM={...}` as a fresh object LITERAL, which
// silently wipes out anything attached to window.SM earlier (found live:
// `window.SM.exposeSymbolProperty is not a function` despite the bare
// function parsing fine). Registered in timeline.js's own literal instead.
function convertLayerToComponent(layerIdx){
  if(state.activeSymbolId){showToast(SM.t('toastCloseComponentFirst'));return;}
  var ld=state.layers[layerIdx];if(!ld){showToast('Calque invalide');return;}
  var bad=badComponentSourceReason(ld);
  if(bad){showToast('Impossible de convertir : '+bad);return;}
  saveAllLayerFrames();pushUndo(true);
  var symId=genSymbolId();
  // groups (2026-07-29 fix, QA-confirmed combine-groups regression): the
  // OUTER instance (`ld` itself, reused below) keeps its .groups by
  // accident since it's the same object — but state.layers/userLayers swap
  // to the SYMBOL's own layers while editing INSIDE the component
  // (enterSymbol), and symLayer had no .groups at all, so a combine-group
  // rendered raw/uncombined the moment you entered to edit it.
  var symLayer={name:'Layer 1',visible:true,locked:false,frames:JSON.parse(JSON.stringify(ld.frames)),groups:ld.groups?JSON.parse(JSON.stringify(ld.groups)):undefined,
    rig:cloneRigForSymbol(ld.rig)};
  // The rig binds specific strokes within ld's OWN content — once ld
  // becomes a symbolId-holding instance its frames are wiped (below), so a
  // rig left on `ld` itself would reference strokes that no longer exist
  // anywhere. Delete rather than leave it as dead data (unlike ld.motion,
  // deliberately KEPT here — see this function's own convertLayerToComponent
  // vs convertLayersToComponent distinction, CLAUDE.md §8: single-layer
  // conversion's ld stays the instance and its motion IS the instance's own
  // transform, but a rig has no equivalent "instance-level" meaning).
  delete ld.rig;
  state.symbols[symId]={name:ld.name+' (Comp)',totalFrames:state.totalFrames,fps:state.fps,layers:[symLayer]};
  if(window.SMStoryboard)SMStoryboard.addInstanceAuto(symId);
  // 'once' = play through the component's own full defined duration exactly
  // once, then hold the last frame — no looping. Default per explicit
  // request (a freshly-made component shouldn't loop by default; the old
  // 'loop' default made a component's parent-timeline keyframe misleadingly
  // look "empty/short" when the component itself kept cycling underneath).
  ld.symbolId=symId;ld.locked=true;ld.symPlayMode='once';ld.symSpeed=1;ld.symPlacedAt=0;ld.symSingleFrame=0;
  // NO componentFrame stamp here. resolveSymbolFrameIdx treats that field as
  // an explicit "hold this internal frame from here on" — the same thing the
  // frame strip writes when you deliberately pick a frame — and it is checked
  // BEFORE play mode. Stamping 0 on frame 0 therefore pinned every freshly
  // converted component to its own first frame for the whole timeline, and
  // the symPlayMode:'once' set two lines above never ran (2026-07-28: "il ne
  // lit pas toutes les frames du composant dans animation 2D"). Worse, when
  // the source layers' drawing starts LATER than frame 0 the pinned frame is
  // blank, so the component looked like it had vanished entirely — the same
  // report from the day before, same single cause.
  ld.frames=[];for(var i=0;i<state.totalFrames;i++)ld.frames.push({strokes:[],isKeyframe:i===0,isInterpolated:false});
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();if(window.renderSymbolTabs)renderSymbolTabs();
  showToast(SM.t('toastComponentCreated')+state.symbols[symId].name);
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
  if(state.activeSymbolId){showToast(SM.t('toastCloseComponentFirst'));return;}
  indices=indices.slice().sort(function(a,b){return a-b;}).filter(function(i){return state.layers[i]&&!state.layers[i].symbolId;});
  // Type guard (2026-07-30 fix): the filter above only ever dropped
  // already-symbolId layers silently — a montage/video/Null/effect layer
  // sailed straight into symLayers below with its `.frames` blindly cloned,
  // which for these types isn't the real content at all. Refuse with the
  // SAME reasons convertLayerToComponent/mergeLayersIntoOne already use.
  for(var bi=0;bi<indices.length;bi++){
    var badReason=badComponentSourceReason(state.layers[indices[bi]]);
    if(badReason){showToast(SM.t('toastCannotConvertSelectionContains')+badReason);return;}
  }
  if(indices.length<2){convertLayerToComponent(indices[0]!==undefined?indices[0]:state.activeLayerIdx);return;}
  saveAllLayerFrames();pushUndo(true);
  var symId=genSymbolId();
  // Combine-group remap + merge (2026-07-29 fix, QA-confirmed): each source
  // layer's own ld.groups may share id strings with another source's (same
  // reasoning as mergeLayersIntoOne's own remap below), AND — separately —
  // getEffectiveStrokes's symbolId branch flattens EVERY sub-layer's strokes
  // into the outer instance's own single userLayers[li].children, so the
  // OUTER instance needs the UNION of every source's registry to combine
  // any of them, not just whichever happened to survive. Confirmed bug: the
  // outer `newLd` below had no .groups property at all, so every
  // combine-group in a merged component rendered raw/uncombined.
  var mergedGroups={};
  var symLayers=indices.map(function(i){
    var src=state.layers[i];
    var framesClone=JSON.parse(JSON.stringify(src.frames));
    var groupsClone;
    if(src.groups){
      var remap={};
      Object.keys(src.groups).forEach(function(oldGid){remap[oldGid]='grp_'+Date.now().toString(36)+'_'+Math.floor(Math.random()*1e6);});
      framesClone.forEach(function(f){(f.strokes||[]).forEach(function(sd){if(sd.groupId&&remap[sd.groupId])sd.groupId=remap[sd.groupId];});});
      groupsClone={};
      Object.keys(src.groups).forEach(function(oldGid){
        var grp=src.groups[oldGid];
        var newOrder=(grp.order||[]).map(function(e){return remap[e]||e;});
        var newGid=remap[oldGid];
        groupsClone[newGid]={combineMode:grp.combineMode,order:newOrder};
        mergedGroups[newGid]=groupsClone[newGid];
      });
    }
    // Same "instance-level vs content-level" distinction as the single-
    // layer convertLayerToComponent right above (see its own comment) —
    // here EVERY source becomes pure symbol content (there's no surviving
    // single instance to keep an "own transform" reading), so unlike that
    // function's ld.motion, a rig here has nowhere sensible to live except
    // the symLayer. Deleted from src afterward for the same dead-data
    // reason.
    var rigClone=cloneRigForSymbol(src.rig);
    delete src.rig;
    return{name:src.name,visible:src.visible,locked:false,frames:framesClone,
      motion:src.motion?JSON.parse(JSON.stringify(src.motion)):undefined,
      motionStatic:src.motionStatic?JSON.parse(JSON.stringify(src.motionStatic)):undefined,
      elementMotion:src.elementMotion?JSON.parse(JSON.stringify(src.elementMotion)):undefined,
      groups:groupsClone,
      rig:rigClone,
      // matteMode/blendMode dropped entirely here (2026-07-30 fix, Cyril:
      // "je met 2 calques avec un matte sur l'un que je met dans componant,
      // je perd le matte") — same CLAUDE.md §1 shape as the Motion-loss bug
      // this function's own header comment already fixed once: a per-layer
      // field that only ever got threaded through the ORIGINAL state.layers
      // object, silently absent from the symLayer clone. matteMode's
      // adjacency convention (source = the layer directly above, engine-
      // bridge.js) is preserved for free since `indices` stays sorted and
      // symLayers keeps that same relative order, so this is purely a
      // missing-field copy, not a re-derivation.
      matteMode:src.matteMode,blendMode:src.blendMode};
  });
  state.symbols[symId]={name:'Composant',totalFrames:state.totalFrames,fps:state.fps,layers:symLayers};
  if(window.SMStoryboard)SMStoryboard.addInstanceAuto(symId);
  var insertAt=indices[0];
  var firstSrc=state.layers[indices[0]]; // captured before the splice below removes it
  for(var k=indices.length-1;k>=0;k--){state.layers.splice(indices[k],1);userLayers.splice(indices[k],1);}
  var newLd={name:'Composant',symbolId:symId,locked:true,visible:true,symPlayMode:'once',symSpeed:1,symPlacedAt:0,symSingleFrame:0,
    frames:[],groups:Object.keys(mergedGroups).length?mergedGroups:undefined};
  // Same field-drop shape as mergeLayersIntoOne/splitLayerIntoElementsCore
  // (2026-08-16 QA sweep) — this collapses indices.length sources into ONE
  // new outer instance the same way merge does, so it needs the same
  // "topmost source wins" inheritance for the old block's visibility window
  // and relationships, or converting a trimmed/linked/parented block into a
  // Component silently resets all of it. firstSrc is captured further up
  // (before the splice loop removes it from state.layers).
  if(firstSrc.inPoint!=null)newLd.inPoint=firstSrc.inPoint;
  if(firstSrc.outPoint!=null)newLd.outPoint=firstSrc.outPoint;
  if(firstSrc.timeLink)newLd.timeLink=JSON.parse(JSON.stringify(firstSrc.timeLink));
  if(firstSrc.parentLayerUid)newLd.parentLayerUid=firstSrc.parentLayerUid;
  if(firstSrc.parentLayerUidB)newLd.parentLayerUidB=firstSrc.parentLayerUidB;
  if(firstSrc.expressions)newLd.expressions=JSON.parse(JSON.stringify(firstSrc.expressions));
  // threeD/motionBlur/duplicator (2026-08-16): same "topmost source wins"
  // inheritance as everything else above — these three describe how the
  // OUTER instance itself renders/multiplies, independent of whatever now
  // lives inside the Component, so they carry over the same way timeLink/
  // parentLayerUid do rather than getting silently reset.
  if(firstSrc.threeD)newLd.threeD=true;
  if(firstSrc.motionBlur)newLd.motionBlur=true;
  if(firstSrc.duplicator)newLd.duplicator=JSON.parse(JSON.stringify(firstSrc.duplicator));
  for(var i=0;i<state.totalFrames;i++)newLd.frames.push({strokes:[],isKeyframe:i===0,isInterpolated:false}); // no componentFrame — see convertLayerToComponent
  var newUl=new Layer({name:'layer-'+state.layers.length});
  state.layers.splice(insertAt,0,newLd);
  userLayers.splice(insertAt,0,newUl);
  userLayers.forEach(function(l){l.insertBelow(arcLayer);});
  state.activeLayerIdx=insertAt;_layerSel=[insertAt];
  activateUL(state.activeLayerIdx);
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();if(window.renderSymbolTabs)renderSymbolTabs();
  showToast(SM.t('toastComponentCreatedWith')+indices.length+' calques');
}
// "Release to Layers" (Illustrator-style) — Motion's double-click-a-layer-row
// gesture (2026-07) EXPLODES the layer into N real top-level layers, one per
// element, rather than opening an in-place grouped view (explicit reversal
// of an earlier same-day decision: "affichage seulement" was tried first,
// the user came back and asked for real separate layers instead).
//
// strokeId (motion.js layerElements) is NOT a stable cross-frame shape
// identity — it's lazily stamped once, on whichever frame Motion's Éléments
// list last looked at. A classic hand-drawn multi-keyframe layer that was
// never opened in Motion often has no consistent strokeId from one keyframe
// to the next. So this splits each keyframe's OWN strokes array by POSITION
// (index 0..N-2 go 1-for-1 to the first N-1 new layers, everything from
// index N-1 onward — the "normal" last element plus any surplus — goes to
// the LAST new layer) rather than by strokeId matching across frames. This
// never invents or drops content: a keyframe with fewer strokes than
// reference elements leaves the extra new layers blank at that frame
// (same as any other blank keyframe); a keyframe with MORE strokes than
// expected keeps the overflow, just grouped into the last layer instead of
// losing it.
//
// Per-ELEMENT Motion keys (ld.elementMotion[strokeId]) ARE reliable at the
// reference frame (state.currentFrame, where layerElements() is evaluated)
// — promoted straight to the new layer's OWN motion/motionStatic, exactly
// the "répercuter les keyframes sur les layers comme d'habitude" the
// request called for: a shape already animated individually in Motion
// keeps that exact animation, now as a normal layer-level track.
function splitLayerIntoElements(li){
  if(state.activeSymbolId){showToast(SM.t('toastCloseComponentFirst'));return;}
  splitLayerIntoElementsCore(li);
}
// Core split, no activeSymbolId guard — reused by enterComponentLayer
// (motion.js) to SILENTLY auto-split a Component's own single layer into
// one real layer per shape the moment you enter it (2026-07-17, "je devrais
// avoir plusieurs calques séparés montés en fonction des keyframes" — the
// user confirmed the exact visual/structural target is this same flat
// "Layer 1 — Forme N" real-layers-with-real-bars result, not a nested
// montage view). Safe to call while state.activeSymbolId is set: this
// function only ever touches state.layers/userLayers, which by then ARE
// the entered symbol's own arrays (enterSymbol already swapped them) —
// same "current document" convention every other layer-editing function in
// this file already relies on. Idempotent from the caller's perspective:
// once split, each resulting layer has exactly 1 element, so a later
// re-entry's auto-split pass no-ops via the `els.length<2` guard below.
function splitLayerIntoElementsCore(li,opts){
  var silent=!!(opts&&opts.silent);
  var ld=state.layers[li];if(!ld||ld.symbolId){if(!silent)showToast(SM.t('toastNothingToSplitHere'));return false;}
  if(!window.SMMotion)return false;
  var els=SMMotion.layerElements(li,ld);
  if(!els||els.length<2){if(!silent)showToast(SM.t('toastNeedAtLeast2ElementsToSplit'));return false;}
  saveAllLayerFrames();if(!silent)pushUndo();
  var n=els.length;
  var newLayers=[];
  for(var e=0;e<n;e++){
    var frames=[];
    for(var fi=0;fi<ld.frames.length;fi++){
      var f=ld.frames[fi];
      if(!f.isKeyframe&&!f.isInterpolated){frames.push({strokes:[],isKeyframe:false,isInterpolated:false});continue;}
      var strokes=f.strokes||[];
      var part=(e<n-1)?(strokes[e]?[strokes[e]]:[]):strokes.slice(n-1);
      frames.push({strokes:JSON.parse(JSON.stringify(part)),isKeyframe:f.isKeyframe,isInterpolated:f.isInterpolated});
    }
    var entry=els[e];
    var emh=ld.elementMotion&&ld.elementMotion[entry.strokeId];
    var nl={
      // ld passed through (2026-07-31, group/shape tree panel) — a shape
      // custom-renamed via the tree panel keeps that name as the split-off
      // layer's own name instead of always falling back to "Forme N".
      name:(ld.name||'Layer')+' — '+SMMotion.elementLabel(entry,e,ld),
      visible:true,locked:false,frames:frames,color:nextLayerColor(),
      motion:(emh&&emh.motion)?JSON.parse(JSON.stringify(emh.motion)):undefined,
      motionStatic:(emh&&emh.motionStatic)?JSON.parse(JSON.stringify(emh.motionStatic)):undefined,
      // blendMode/parentLayerUid(+B)/timeLink (2026-07-30 fix, same
      // CLAUDE.md §1 shape already fixed once in convertLayersToComponent/
      // mergeLayersIntoOne): ld's own OUTWARD references describe the whole
      // original layer's spatial/time parenting, unrelated to which of the
      // N pieces a given stroke landed in — every split-off layer keeps
      // moving/timing with the same external parent ld had, or the group
      // visibly drifts apart the moment any of them gets keyed. blendMode
      // similarly composites per-layer against the accumulated frame below;
      // applying it to every split-off layer reproduces the original
      // single-pass blend exactly for non-overlapping shapes (the common
      // case here) and is a reasonable approximation otherwise — same
      // "flag rather than silently drop" tradeoff mergeLayersIntoOne makes.
      blendMode:ld.blendMode,
      parentLayerUid:ld.parentLayerUid,parentLayerUidB:ld.parentLayerUidB,
      timeLink:ld.timeLink?JSON.parse(JSON.stringify(ld.timeLink)):undefined,
      // Same gap as the fields above (2026-08-16 QA sweep): ld's in/out
      // point describes the whole original layer's visibility WINDOW, not
      // which piece a stroke landed in — every split-off layer needs the
      // same window or N-1 of them un-trim back to the full timeline the
      // instant the split runs.
      inPoint:ld.inPoint,outPoint:ld.outPoint,
      expressions:ld.expressions?JSON.parse(JSON.stringify(ld.expressions)):undefined,
      // threeD/motionBlur (2026-08-16): same per-layer render toggles as
      // blendMode a few lines up, and the same reasoning — every split-off
      // piece keeps rendering the way the whole original layer did.
      // duplicator is deliberately NOT copied here: unlike a boolean
      // render toggle, applying one duplicator descriptor to N independent
      // split-off layers would multiply each piece separately instead of
      // the original single N-way expansion — a real behavior change, not
      // a "keep what was already true of this content" carry-over.
      threeD:ld.threeD,motionBlur:ld.motionBlur,
    };
    // matteMode: with a frozen matteSourceLayerUid (2026-07-31, uid-based
    // mattes) the source no longer depends on array adjacency, so EVERY
    // split-off piece can keep the matte and still mask against the same
    // external source — this removes the old documented positional
    // limitation ("only means something on whichever split-off layer ends
    // up at ld's OLD highest index"). A legacy ld with matteMode but no
    // uid keeps the old last-piece-only behavior (adjacency is all it has).
    if(ld.matteSourceLayerUid){
      nl.matteMode=ld.matteMode;
      nl.matteSourceLayerUid=ld.matteSourceLayerUid;
    }
    if(e===n-1){
      // Legacy positional matte (no uid) — see the block above; adjacency
      // only survives on the last piece (newLayers[n-1] lands at li+n-1,
      // still directly below whatever used to sit above ld).
      // Keeping ld's layerUid string alive on this same
      // layer (instead of leaving it undefined on all N, as before) means
      // any OTHER layer's parentLayerUid/parentLayerUidB/timeLink.uid that
      // pointed at ld keeps resolving with zero extra re-point pass needed
      // — the uid itself never changes, only which layer object answers to it.
      if(!ld.matteSourceLayerUid)nl.matteMode=ld.matteMode;
      if(ld.layerUid)nl.layerUid=ld.layerUid;
    }
    newLayers.push(nl);
  }
  // groups (combine-group definitions): only the LAST new layer can hold
  // 2+ strokes (the "overflow" slice, strokes.slice(n-1)) — every other one
  // holds exactly 1 stroke by construction and can't combine with anything.
  // Even for that layer, only keep entries whose gid still appears on one
  // of ITS OWN strokes, dropping any whose combine partner landed on a
  // different single-stroke layer instead — a combine can't span layers,
  // same unrepresentable-once-split limit matteMode has, just data-driven
  // here since there's no single adjacency slot to assign it to.
  if(ld.groups&&newLayers.length){
    var lastLayer=newLayers[newLayers.length-1];
    var keepGids={};
    lastLayer.frames.forEach(function(f){(f.strokes||[]).forEach(function(sd){if(sd.groupId)keepGids[sd.groupId]=1;});});
    var keptGroups={};
    Object.keys(ld.groups).forEach(function(gid){if(keepGids[gid])keptGroups[gid]=ld.groups[gid];});
    if(Object.keys(keptGroups).length)lastLayer.groups=JSON.parse(JSON.stringify(keptGroups));
  }
  var newUls=[];
  arcLayer.activate();
  for(var e2=0;e2<n;e2++)newUls.push(new Layer({name:'user-split-'+Date.now()+'-'+e2}));
  userLayers[li].remove();
  userLayers.splice.apply(userLayers,[li,1].concat(newUls));
  userLayers.forEach(function(l){l.insertBelow(arcLayer);});
  state.layers.splice.apply(state.layers,[li,1].concat(newLayers));
  state.activeLayerIdx=li;
  _layerSel=newLayers.map(function(_x,idx){return li+idx;});
  activateUL(state.activeLayerIdx);
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
  if(!silent)showToast(SM.t('toastLayerSplitIntoSuffix')+n+' calques');
  return true;
}
// True inverse of splitLayerIntoElementsCore — merges N layers back into a
// single one. Added 2026-07-25 after "quand on double clic sur un calque
// avec plusieurs vecto/forme à l'intérieur impossible de revenir qu'à un
// seul calque après": the split had no inverse ANYWHERE in the app, and
// entering a Component auto-splits SILENTLY (enterComponentLayer, silent:true
// skips pushUndo), so merely LOOKING inside a component permanently
// rewrote its layer structure with no undo entry to walk back. The split
// side of that is fixed at its call site (it records undo now); this is the
// explicit way back, reachable from both timelines' layer context menu.
//
// Frame model: mirrors the split. A merged frame is a keyframe if ANY
// source has one there; its strokes are every source's strokes AT that
// frame, held-frame semantics included (a source that has no keyframe at fi
// still contributes what it VISIBLY shows there, inherited from its last
// keyframe — otherwise merging layers whose keyframes don't line up would
// silently drop content, which is exactly the "handled in one reader but
// not the others" family CLAUDE.md §1 warns about). Deliberately reads the
// RAW stored strokes rather than getEffectiveStrokes: the latter bakes
// layer/element motion into geometry, which would freeze the animation into
// the drawing instead of preserving it as editable keys.
//
// Layer-level Motion is DEMOTED to per-element motion (the exact mirror of
// the split's element→layer promotion) rather than dropped. For the
// round-trip case (each source holds exactly 1 shape, which is what a split
// produces) this is lossless: element motion pivots on the stroke's own
// bounds+anchor and the layer's bounds ARE that stroke's bounds. A source
// bundling several shapes gets the same transform copied onto each of them,
// which differs from a true group pivot — flagged in the toast rather than
// silently approximated.
function mergeLayersIntoOne(indices,opts){
  var silent=!!(opts&&opts.silent);
  var idx=(indices||[]).slice().sort(function(a,b){return a-b;});
  // de-dup — a caller can easily pass the active layer twice (selection + active)
  idx=idx.filter(function(v,i){return i===0||v!==idx[i-1];});
  if(idx.length<2){if(!silent)showToast(SM.t('toastSelectAtLeast2LayersToMerge'));return false;}
  var srcs=[],bad=null;
  for(var a=0;a<idx.length;a++){
    var l=state.layers[idx[a]];
    if(!l){if(!silent)showToast('Calque introuvable');return false;}
    // Non-stroke-holders have no `frames` content to concatenate — merging
    // one would quietly delete whatever it stands for (a component's whole
    // symbol, a montage, a decoded video). Refuse with a reason instead.
    if(l.symbolId)bad=bad||'un composant';
    else if(l.lfsGroup)bad=bad||'un groupe Ligne/Plein/Ombre';
    else if(l.montageId)bad=bad||'un calque de montage StoryBoard';
    else if(l.nativeVideo)bad=bad||'un calque vidéo';
    else if(l.isNullLayer)bad=bad||'un calque Null';
    else if(l.isEffectLayer)bad=bad||'un calque d\'effet';
    else if(l.isGuideLayer)bad=bad||'un calque Guide';
    srcs.push(l);
  }
  if(bad){if(!silent)showToast(SM.t('toastCannotMergeSelectionContains')+bad);return false;}
  saveAllLayerFrames();if(!silent)pushUndo();
  // Combine-group remap (2026-07-29): each source layer's OWN ld.groups may
  // share id strings with ANOTHER source's — e.g. merging a layer with its
  // own duplicateLayer() clone, which deliberately keeps the same groupId
  // strings (see that function's own comment). Mint a fresh gid per group
  // per source BEFORE concatenating strokes below, mirroring the strokeId
  // collision-avoidance a few lines down for the exact same reason: reusing
  // an id string across two originally-separate things silently fuses them
  // into one wrongly-combined group.
  var mergedGroups={};
  srcs.forEach(function(l){
    if(!l.groups)return;
    var remap={};
    Object.keys(l.groups).forEach(function(oldGid){remap[oldGid]='grp_'+Date.now().toString(36)+'_'+Math.floor(Math.random()*1e6);});
    (l.frames||[]).forEach(function(f){
      (f.strokes||[]).forEach(function(sd){if(sd.groupId&&remap[sd.groupId])sd.groupId=remap[sd.groupId];});
    });
    Object.keys(l.groups).forEach(function(oldGid){
      var grp=l.groups[oldGid];
      var newOrder=(grp.order||[]).map(function(e){return remap[e]||e;});
      mergedGroups[remap[oldGid]]={combineMode:grp.combineMode,order:newOrder};
    });
  });
  var target=idx[0];
  // Held-frame resolution WITHOUT motion baking — the plain-layer tail of
  // getEffectiveStrokes, kept separate on purpose (see header comment).
  function rawAt(ld,fi){
    var f=ld.frames[fi];if(!f)return null;
    if(f.isKeyframe||f.isInterpolated)return f.strokes||[];
    for(var i=fi-1;i>=0;i--){var pf=ld.frames[i];if(pf&&pf.isKeyframe)return pf.strokes||[];}
    return null;
  }
  var nf=0;srcs.forEach(function(l){nf=Math.max(nf,(l.frames||[]).length);});
  if(!nf)nf=state.totalFrames;
  var elMotion=JSON.parse(JSON.stringify(state.layers[target].elementMotion||{}));
  var approximated=0;
  var frames=[];
  for(var fi=0;fi<nf;fi++){
    var anyKey=false,anyInterp=false,strokes=[];
    for(var s=0;s<srcs.length;s++){
      var f=srcs[s].frames[fi];
      if(f&&f.isKeyframe)anyKey=true;
      if(f&&f.isInterpolated)anyInterp=true;
    }
    if(anyKey||anyInterp){
      for(var s2=0;s2<srcs.length;s2++){
        var part=rawAt(srcs[s2],fi);
        if(part&&part.length)strokes=strokes.concat(JSON.parse(JSON.stringify(part)));
      }
    }
    frames.push({strokes:strokes,isKeyframe:anyKey,isInterpolated:!anyKey&&anyInterp});
  }
  // Demote each source's layer-level Motion onto its own strokes.
  srcs.forEach(function(l){
    var hasMotion=(l.motion&&Object.keys(l.motion).length)||(l.motionStatic&&Object.keys(l.motionStatic).length);
    if(!hasMotion)return;
    var seen={},ids=[];
    (l.frames||[]).forEach(function(f){
      if(!f||!f.isKeyframe&&!f.isInterpolated)return;
      (f.strokes||[]).forEach(function(sd,i){
        if(sd.isBrushTextureCopy)return;
        if(!sd.strokeId)sd.strokeId='s'+Date.now().toString(36)+'_'+i+'_'+Math.floor(Math.random()*1e6);
        if(!seen[sd.strokeId]){seen[sd.strokeId]=1;ids.push(sd.strokeId);}
      });
    });
    if(ids.length>1)approximated++;
    ids.forEach(function(sid){
      var h=elMotion[sid]||(elMotion[sid]={});
      if(l.motion)h.motion=JSON.parse(JSON.stringify(l.motion));
      if(l.motionStatic)h.motionStatic=JSON.parse(JSON.stringify(l.motionStatic));
    });
  });
  // "Layer 1 — Forme 3" + "Layer 1 — Forme 4" → "Layer 1": recover the name
  // the split derived these from, so a split/merge round-trip is invisible.
  var name=srcs[0].name||'Layer 1';
  var m=/^(.*?)\s+—\s+(Forme|Image|Texte)\s+\d+$/.exec(name);
  if(m&&srcs.every(function(l){return (l.name||'').indexOf(m[1]+' — ')===0;}))name=m[1];
  var merged={
    name:name,visible:true,locked:false,frames:frames,
    color:srcs[0].color||nextLayerColor(),
    layerUid:srcs[0].layerUid,parentLayerUid:srcs[0].parentLayerUid,
    // parentLayerUidB (multi-parent blend): the OTHER-layers re-point pass
    // a few lines down already updates any layer pointing INTO this merge,
    // but the merged layer's OWN parentLayerUidB — its half of a 2-parent
    // blend — was never inherited from srcs[0] here, same class of gap as
    // the in/out point fix right below (2026-08-16 QA sweep).
    parentLayerUidB:srcs[0].parentLayerUidB,
    timeLink:srcs[0].timeLink,
    expressions:srcs[0].expressions?JSON.parse(JSON.stringify(srcs[0].expressions)):undefined,
    // matteMode/blendMode: same field-drop bug already fixed once in
    // convertLayersToComponent (§1) — recurred here in the sibling merge
    // path. Inherited from the topmost (first) source, matching how
    // everything else about the merged layer (color, layerUid, timeLink)
    // is taken from srcs[0].
    matteMode:srcs[0].matteMode,blendMode:srcs[0].blendMode,
    // threeD/duplicator/motionBlur (2026-08-16, found testing 3D+duplicator+
    // motionBlur combinations): same "topmost source wins" inheritance as
    // everything else in this object — none of the three were here at all,
    // so merging a 3D layer silently flattened it, merging a duplicator
    // layer lost its N-way expansion settings, and motionBlur silently
    // turned off.
    threeD:srcs[0].threeD,motionBlur:srcs[0].motionBlur,
    duplicator:srcs[0].duplicator?JSON.parse(JSON.stringify(srcs[0].duplicator)):undefined,
    // Travels with matteMode (2026-07-31, uid-based mattes) — dropping the
    // uid alone would silently downgrade the merged layer back to the
    // legacy adjacency behavior.
    matteSourceLayerUid:srcs[0].matteSourceLayerUid,
  };
  // A duplicator layer is always force-locked elsewhere (duplicateLayer,
  // applyLayerDuplicator) — inheriting the descriptor above without this
  // would produce the one combination the rest of the app never expects:
  // an EDITABLE layer with an active N-way expansion.
  if(merged.duplicator)merged.locked=true;
  // Same field-drop bug again (2026-08-16 QA sweep): a trimmed source's
  // in/out point wasn't in this list at all, so merging a layer manually
  // rogné à [5,50] silently un-trimmed it back to the full timeline —
  // content past the old out point that used to be hidden was suddenly
  // visible. Inherited from srcs[0], same "topmost source wins" convention
  // as color/matteMode/timeLink right above.
  if(srcs[0].inPoint!=null)merged.inPoint=srcs[0].inPoint;
  if(srcs[0].outPoint!=null)merged.outPoint=srcs[0].outPoint;
  if(Object.keys(elMotion).length)merged.elementMotion=elMotion;
  if(Object.keys(mergedGroups).length)merged.groups=mergedGroups;
  // Any OTHER layer parented to one of the layers about to disappear must
  // be re-pointed at the survivor, or its parenting silently goes dead
  // (parentChainMats resolves by uid and just finds nothing) — the exact
  // shape of bug CLAUDE.md §1 is about, one consumer updated and the rest
  // left dangling.
  var goneUids={};
  for(var g=1;g<idx.length;g++){var gu=state.layers[idx[g]].layerUid;if(gu)goneUids[gu]=1;}
  state.layers.forEach(function(other,oi){
    if(idx.indexOf(oi)>=0)return;
    if(other.parentLayerUid&&goneUids[other.parentLayerUid])other.parentLayerUid=merged.layerUid||null;
    // parentLayerUidB (multi-parent blend, 2026-07-30): the spatial parent
    // re-point above only ever touched parentLayerUid — parentLayerUidB is
    // the exact same kind of uid reference and needs the same treatment or
    // a blend's B side silently goes dead when its source gets merged away.
    if(other.parentLayerUidB&&goneUids[other.parentLayerUidB])other.parentLayerUidB=merged.layerUid||null;
    // Same re-point for the TIME link (Parent in Time, 2026-07-26): a layer
    // whose time source disappears into the merge must follow the survivor,
    // or its link goes dead exactly like the spatial one would.
    if(other.timeLink&&other.timeLink.uid&&goneUids[other.timeLink.uid]){
      if(merged.layerUid)other.timeLink.uid=merged.layerUid;else delete other.timeLink;
    }
    // Same re-point for the matte source (2026-07-31, uid-based mattes): a
    // matte whose source layer disappears into the merge follows the
    // survivor, same family as the three uid references above.
    if(other.matteSourceLayerUid&&goneUids[other.matteSourceLayerUid]){
      if(merged.layerUid)other.matteSourceLayerUid=merged.layerUid;else delete other.matteSourceLayerUid;
    }
  });
  // Paper layers: one fresh Layer replaces the N being removed, inserted
  // where the topmost source sat (same splice-from-the-end discipline the
  // split uses so no index shifts under an iteration still in progress).
  arcLayer.activate();
  var newUL=new Layer({name:'user-merge-'+Date.now()});
  for(var r=idx.length-1;r>=0;r--){
    userLayers[idx[r]].remove();
    userLayers.splice(idx[r],1);
    state.layers.splice(idx[r],1);
  }
  userLayers.splice(target,0,newUL);
  state.layers.splice(target,0,merged);
  userLayers.forEach(function(l){l.insertBelow(arcLayer);});
  state.activeLayerIdx=target;
  _layerSel=[target];
  activateUL(state.activeLayerIdx);
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
  if(!silent){
    showToast(approximated
      ? idx.length+SM.t('toastLayersMergedShapePivotSuffix')+approximated+' calque(s) multi-formes)'
      : idx.length+SM.t('toastLayersMergedIntoSuffix')+name+' »');
  }
  return true;
}
// Inverse of convertLayerToComponent: bakes what the component instance
// actually displays on each main-timeline frame (play mode, speed and
// single-frame settings included) back into plain layer keyframes, then
// detaches the layer from the symbol. Consecutive identical frames are
// collapsed into one keyframe + held frames, so a looping symbol becomes
// repeating keyframes rather than a keyframe on every single frame. The
// symbol itself stays in the library for any other instance.
function convertComponentToLayer(layerIdx){
  if(state.activeSymbolId){showToast(SM.t('toastCloseComponentFirst'));return;}
  var ld=state.layers[layerIdx];if(!ld||!ld.symbolId){showToast(SM.t('toastNotAComponent'));return;}
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
  showToast(SM.t('toastComponentBrokenToLayer'));
}
// Groups a plain layer into 3 independent sub-timelines (Ligne/Plein/Ombre),
// each modeled as its own state.symbols entry so enterSymbol/exitToScene/
// getEffectiveStrokes' existing symbolId machinery is reused verbatim — see
// getLFSSubStrokes above. The layer's current content becomes the "Plein"
// (Full/flat-color) pass by default since that's normally the bulk of the
// linework already on a layer being grouped; Ligne/Ombre start empty.
function convertLayerToLFSGroup(layerIdx){
  if(state.activeSymbolId){showToast(SM.t('toastCloseComponentFirst'));return;}
  var ld=state.layers[layerIdx];if(!ld||ld.symbolId||ld.lfsGroup){showToast(SM.t('toastLayerAlreadyGrouped'));return;}
  saveAllLayerFrames();pushUndo(true);
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
  showToast(SM.t('toastLayerGroupedLFS'));
}
// Inverse: bakes the composited Ligne+Plein+Ombre result of each main-timeline
// frame (via getEffectiveStrokes, which already stacks all 3 sub-symbols)
// back into plain layer keyframes, then detaches the 3 sub-symbols. Mirrors
// convertComponentToLayer's collapse-identical-frames logic.
function convertLFSGroupToLayer(layerIdx){
  if(state.activeSymbolId){showToast(SM.t('toastCloseComponentFirst'));return;}
  var ld=state.layers[layerIdx];if(!ld||!ld.lfsGroup){showToast(SM.t('toastNotALFSGroup'));return;}
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
  showToast(SM.t('toastGroupLFSBrokenToLayer'));
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
  if(state.activeSymbolId){showToast(SM.t('toastCloseComponentFirst'));return;}
  var ld=state.layers[layerIdx];if(!ld||!ld.lfsGroup){showToast(SM.t('toastNotALFSGroup'));return;}
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
  saveAllLayerFrames();pushUndoLayers(true);
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
  showToast(applied+' image(s) '+(which==='full'?'Plein':'Ombre')+SM.t('toastGeneratedByPropagationSuffix'));
}
function enterSymbol(symId){
  if(state.activeSymbolId===symId)return;
  if(state.activeSymbolId){showToast(SM.t('toastNestedComponentsUnsupported'));return;}
  if(!state.symbols[symId])return;
  saveAllLayerFrames();
  // "double clic sur un component le bounding box de la selection reste"
  // (2026-07) — a selection made in the OUTER scene stayed in
  // `selectedPaths` across this document-context swap; buildTransformBoxItems
  // (engine-bridge.js) only checks selectedPaths.length, not which document
  // is active, so it kept drawing a stale box/handles for items that still
  // exist (just dimmed to opacity 0.25 below) but belong to a scene that's
  // no longer the active one. Same fix applied on the way back out
  // (exitToScene) and for StoryBoard's own document swap (enterMontageView/
  // exitMontageView).
  if(typeof clearSel==='function')clearSel();
  // markers/motionArcs/tweenOverrides/tweenEasing (2026-07-30 fix, same
  // shape as cameraKeys just below): a symbol can hold plain vector layers
  // with real per-shape tween overrides and its own comp markers, exactly
  // like the outer scene — without swapping these too, they stayed the
  // OUTER scene's live values the whole time a symbol was open, so editing
  // easing/markers while inside silently wrote into the outer document's
  // data at whatever frame indices happened to line up, instead of the
  // symbol's own. See exitToScene for the write-back half of this pair —
  // required here because timeline.js/markers.js reassign these wholesale
  // in several places (trimToWorkArea, import, "clear all markers"), which
  // severs a plain object/array alias the same way undo's restoreLayersSnapshot
  // does for state.layers (see exitToScene's own sym.layers comment).
  _sceneSnapshot={layers:state.layers,totalFrames:state.totalFrames,waIn:state.waIn,waOut:state.waOut,activeLayerIdx:state.activeLayerIdx,fps:state.fps,currentFrame:state.currentFrame,userLayers:userLayers,cameraKeys:state.cameraKeys,
    markers:state.markers,motionArcs:state.motionArcs,tweenOverrides:state.tweenOverrides,tweenEasing:state.tweenEasing};
  userLayers.forEach(function(l){l.opacity=0.25;});
  var sym=state.symbols[symId];
  var symPaperLayers=ensureSymbolPaperLayers(symId);
  symPaperLayers.forEach(function(l){l.visible=true;l.opacity=1;});
  state.layers=sym.layers;state.totalFrames=sym.totalFrames;state.waIn=0;state.waOut=sym.totalFrames-1;
  state.cameraKeys=sym.cameraKeys||(sym.cameraKeys=[]);
  state.markers=sym.markers||(sym.markers=[]);
  state.motionArcs=sym.motionArcs||(sym.motionArcs={});
  state.tweenOverrides=sym.tweenOverrides||(sym.tweenOverrides={});
  state.tweenEasing=sym.tweenEasing||(sym.tweenEasing={});
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
  if(typeof clearSel==='function')clearSel(); // see enterSymbol's own comment — same stale-selection risk in reverse
  var symId=state.activeSymbolId;
  // totalFrames/fps are primitives copied into state at enterSymbol() time,
  // not a live binding back to state.symbols[symId] — write them back now
  // so frame-count/fps edits made while inside the symbol aren't lost.
  var sym=state.symbols[symId];if(sym){
    sym.totalFrames=state.totalFrames;sym.fps=state.fps;
    // ...and `layers` for the same reason, discovered 2026-07-25. enterSymbol
    // aliases state.layers TO sym.layers, so in-place edits (splice, push,
    // per-layer mutation) write straight through and this looked unnecessary
    // — but any code path that REPLACES the array instead of mutating it
    // silently severs that alias, and the leading example is
    // restoreLayersSnapshot (tweens.js), which does `state.layers=[]` then
    // rebuilds. Net effect before this line: EVERY layer-level undo made
    // inside a component (split, merge, add/delete/reorder/rename a layer)
    // was silently thrown away the moment you went back to the scene — the
    // screen showed the undone state right up until you left. Writing the
    // current array back here makes the alias an optimization rather than a
    // correctness requirement.
    sym.layers=state.layers;
    // markers/motionArcs/tweenOverrides/tweenEasing write-back — same
    // reasoning as sym.layers just above (see enterSymbol's comment for the
    // full explanation): several call sites reassign these wholesale rather
    // than mutating in place, which would otherwise silently strand any
    // edit made inside the symbol the instant one of those call sites ran.
    sym.markers=state.markers;sym.motionArcs=state.motionArcs;sym.tweenOverrides=state.tweenOverrides;sym.tweenEasing=state.tweenEasing;
    // cameraKeys write-back (2026-07-30 fix) — enterSymbol aliases
    // state.cameraKeys TO sym.cameraKeys (line ~2998, same pattern as
    // sym.layers above), so in-place edits (SMCamera.setKey/removeKey,
    // which push/splice) already write through without this. But camera.js's
    // "Supprimer le calque caméra" action does `state.cameraKeys=[]` — a
    // wholesale REPLACE, not a mutation — which silently severs the alias
    // exactly like restoreLayersSnapshot's own `state.layers=[]` did for
    // sym.layers before that write-back existed. Without this, deleting the
    // camera layer while inside a Component looked like it worked (the UI's
    // own state.cameraKeys really is empty) but sym.cameraKeys still held
    // every original key, and reappeared in full the next time the
    // Component was entered. exitMontageView's own identical write-back
    // (a few hundred lines down) already had this; only this sibling
    // function was missing it.
    sym.cameraKeys=state.cameraKeys;
  }
  var symLayers=_symbolPaperLayers[symId];if(symLayers)symLayers.forEach(function(l){l.visible=false;});
  state.layers=_sceneSnapshot.layers;state.totalFrames=_sceneSnapshot.totalFrames;state.waIn=_sceneSnapshot.waIn;state.waOut=_sceneSnapshot.waOut;
  window._waIn=state.waIn;window._waOut=state.waOut;window._totalF=state.totalFrames;
  state.activeLayerIdx=_sceneSnapshot.activeLayerIdx;state.currentFrame=_sceneSnapshot.currentFrame;state.fps=_sceneSnapshot.fps;
  userLayers=_sceneSnapshot.userLayers;
  state.cameraKeys=_sceneSnapshot.cameraKeys;
  state.markers=_sceneSnapshot.markers;state.motionArcs=_sceneSnapshot.motionArcs;state.tweenOverrides=_sceneSnapshot.tweenOverrides;state.tweenEasing=_sceneSnapshot.tweenEasing;
  userLayers.forEach(function(l){l.opacity=1;});
  state.activeSymbolId=null;_sceneSnapshot=null;
  activateUL(state.activeLayerIdx);drawStage();loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();if(window.renderSymbolTabs)renderSymbolTabs();
  // StoryBoard instance-module thumbnails are cached per symbolId — drop
  // the cache entry for whatever was just edited so the next render
  // re-rasterizes it instead of showing stale pre-edit content.
  if(window.SMStoryboard&&window.SMStoryboard.invalidateThumb)SMStoryboard.invalidateThumb(symId);
}
function closeSymbolTab(symId){
  if(state.activeSymbolId===symId)exitToScene();
  var idx=state.openSymbolTabs.indexOf(symId);if(idx>=0)state.openSymbolTabs.splice(idx,1);
  if(window.renderSymbolTabs)renderSymbolTabs();
}
// StoryBoard "entrer dans le montage" (2026-07: "si on rentre dans montage
// cela affiche les layers de component avec le bon montage de layer afin
// de le modifier"). Builds a temporary scene made of real Component-
// instance layers — one per chain member, in chain order — so the montage
// view is a normal Motion/Animation2D timeline the user already knows how
// to read (segments = layer bars), with zero new rendering path: each
// segment IS a real `ld.symbolId` layer, so Motion's existing "double-click
// a Component layer enters it" (enterComponentLayer/enterSymbol) works on
// a segment completely unmodified — no nested-nesting special-casing
// needed, since entering a REAL symbol from here just treats "the montage
// view's synthetic layers" as the scene to snapshot/restore, exactly like
// it would any other scene.
//
// Each segment's symPlacedAt/symSpeed reproduce storyboard.js's own
// montageStrokesAt formula exactly (symPlacedAt = that member's montage-
// local start frame, symSpeed = srcLen/duration), and symTrimIn/symTrimOut
// (resolveSymbolFrameIdx above) carry the member's own trim range so a
// mid-clip trim shows correctly instead of always starting the symbol's
// own frame 0.
function enterMontageView(montageId){
  if(state.activeSymbolId){showToast(SM.t('toastCloseComponentFirst'));return;}
  if(state.activeMontageViewId===montageId)return;
  if(state.activeMontageViewId){showToast(SM.t('toastCloseMontageFirst'));return;}
  if(!window.SMStoryboard)return;
  var m=SMStoryboard.montageById(montageId);if(!m)return;
  var mods=SMStoryboard.chainModsForView(m);
  if(!mods.length){showToast('Montage vide — accrochez des instances contre son bloc d\'abord');return;}
  saveAllLayerFrames();
  if(typeof clearSel==='function')clearSel(); // see enterSymbol's own comment — same document-swap stale-selection risk
  // markers/motionArcs/tweenOverrides/tweenEasing (2026-07-30 fix) — same
  // swap+write-back pair as enterSymbol/exitToScene, kept on the montage
  // module `m` itself (part of state.storyboard, already copied wholesale
  // by exportJSON, so no new export plumbing needed). Montage-view segments
  // are locked symbolId placements so genuine per-shape tweening rarely
  // applies here, but markers plainly can, and leaving any of the four live
  // against the outer scene's values for the swap's duration risked the
  // exact same silent cross-context write enterSymbol's own comment covers.
  _montageViewSnapshot={layers:state.layers,totalFrames:state.totalFrames,waIn:state.waIn,waOut:state.waOut,activeLayerIdx:state.activeLayerIdx,fps:state.fps,currentFrame:state.currentFrame,userLayers:userLayers,cameraKeys:state.cameraKeys,
    markers:state.markers,motionArcs:state.motionArcs,tweenOverrides:state.tweenOverrides,tweenEasing:state.tweenEasing};
  state.markers=m.markers||(m.markers=[]);
  state.motionArcs=m.motionArcs||(m.motionArcs={});
  state.tweenOverrides=m.tweenOverrides||(m.tweenOverrides={});
  state.tweenEasing=m.tweenEasing||(m.tweenEasing={});
  // cameraKeys (2026-07-30 fix, found live by a background exploration
  // agent — bug missed by the markers/motionArcs/tweenOverrides/tweenEasing
  // fix right above): unlike those four, cameraKeys was never actually
  // reassigned here — only CAPTURED by reference into the snapshot above,
  // exactly like the other four used to be before that fix. state.cameraKeys
  // stayed the SAME array the whole time inside the montage view, so a
  // camera key created "inside" wrote straight into the real outer scene's
  // camera track, and exitMontageView's restore below was a no-op (handing
  // the reference back to itself, already mutated). enterSymbol/exitToScene
  // already isolate camera per-symbol (sym.cameraKeys) — this gives montage
  // view the identical treatment, on the montage module `m` itself (same
  // storyboard.js persistence story as markers et al. above).
  state.cameraKeys=m.cameraKeys||(m.cameraKeys=[]);
  userLayers.forEach(function(l){l.opacity=0.25;});
  var total=SMStoryboard.montageTotal(m);
  var newLayers=[],newUls=[],acc=0;
  arcLayer.activate();
  mods.forEach(function(mod){
    var sym=state.symbols[mod.symbolId];
    var srcLen=mod.trimOut-mod.trimIn+1;
    var frames=[];for(var i=0;i<total;i++)frames.push({strokes:[],isKeyframe:i===0,isInterpolated:false});
    newLayers.push({
      name:(sym?sym.name:'Composant')+' — montage',
      symbolId:mod.symbolId,locked:true,visible:true,
      symPlayMode:'once',symSpeed:srcLen/mod.duration,symPlacedAt:acc,
      symTrimIn:mod.trimIn,symTrimOut:mod.trimOut,symSingleFrame:0,
      // Explicit in/out so Motion's layer bar shows this segment's own
      // [acc, acc+duration) span instead of defaulting to the full
      // combined timeline (layerOutPoint's auto-detect skips symbolId
      // layers entirely, so leaving these unset would draw every segment
      // as one full-width bar) — this also makes getEffectiveStrokes hide
      // the layer outside its own window for free (same inPoint/outPoint
      // early-return every other layer already goes through).
      inPoint:acc,outPoint:acc+mod.duration-1,
      frames:frames,
    });
    newUls.push(new Layer({name:'montageview-'+montageId+'-'+newUls.length}));
    acc+=mod.duration;
  });
  state.layers=newLayers;state.totalFrames=total;state.waIn=0;state.waOut=total-1;
  window._waIn=0;window._waOut=state.waOut;window._totalF=state.totalFrames;
  state.activeLayerIdx=0;state.currentFrame=0;
  userLayers=newUls;
  userLayers.forEach(function(l){l.insertBelow(arcLayer);l.visible=true;});
  state.activeMontageViewId=montageId;
  activateUL(0);drawStage();loadFrame(0);renderOS();renderArcs();updateUI();if(window.renderSymbolTabs)renderSymbolTabs();
}
function exitMontageView(){
  if(!state.activeMontageViewId||!_montageViewSnapshot)return;
  if(state.activeSymbolId){showToast(SM.t('toastCloseComponentFirst'));return;}
  saveAllLayerFrames();
  if(typeof clearSel==='function')clearSel(); // see enterSymbol's own comment
  // Write back markers/motionArcs/tweenOverrides/tweenEasing onto the
  // montage module itself — same pairing as enterMontageView's swap, same
  // reasoning as exitToScene's sym.* write-back.
  var mWB=window.SMStoryboard?SMStoryboard.montageById(state.activeMontageViewId):null;
  if(mWB){mWB.markers=state.markers;mWB.motionArcs=state.motionArcs;mWB.tweenOverrides=state.tweenOverrides;mWB.tweenEasing=state.tweenEasing;mWB.cameraKeys=state.cameraKeys;}
  userLayers.forEach(function(l){l.remove();});
  state.layers=_montageViewSnapshot.layers;state.totalFrames=_montageViewSnapshot.totalFrames;state.waIn=_montageViewSnapshot.waIn;state.waOut=_montageViewSnapshot.waOut;
  window._waIn=state.waIn;window._waOut=state.waOut;window._totalF=state.totalFrames;
  state.activeLayerIdx=_montageViewSnapshot.activeLayerIdx;state.currentFrame=_montageViewSnapshot.currentFrame;state.fps=_montageViewSnapshot.fps;
  userLayers=_montageViewSnapshot.userLayers;
  state.cameraKeys=_montageViewSnapshot.cameraKeys;
  state.markers=_montageViewSnapshot.markers;state.motionArcs=_montageViewSnapshot.motionArcs;state.tweenOverrides=_montageViewSnapshot.tweenOverrides;state.tweenEasing=_montageViewSnapshot.tweenEasing;
  userLayers.forEach(function(l){l.opacity=1;});
  state.activeMontageViewId=null;_montageViewSnapshot=null;
  activateUL(state.activeLayerIdx);drawStage();loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();if(window.renderSymbolTabs)renderSymbolTabs();
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
// Deep-equal with a numeric epsilon, used instead of a strict
// JSON.stringify(a)!==JSON.stringify(b) compare when deciding whether an
// interpolated frame was actually hand-edited. Plain string equality was
// too strict: loadFrame() rebuilds real Paper.js segments from stored
// numbers via desP(), and simply navigating away re-serializes those same
// segments via serP() — Paper.js's internal Point/Segment storage can
// return numbers that differ from the input by float noise (~1e-10) even
// with zero actual editing, which strict string comparison treated as a
// real change and permanently flagged the frame "manually edited" (green)
// from pure playhead scrubbing, no canvas interaction at all.
function _numClose(a,b){return Math.abs(a-b)<1e-6;}
function _deepCloseEqual(a,b){
  if(a===b)return true;
  if(typeof a==='number'&&typeof b==='number')return _numClose(a,b);
  if(Array.isArray(a)){
    if(!Array.isArray(b)||a.length!==b.length)return false;
    for(var i=0;i<a.length;i++)if(!_deepCloseEqual(a[i],b[i]))return false;
    return true;
  }
  if(a&&b&&typeof a==='object'&&typeof b==='object'){
    // Union of both objects' keys, not a strict key-COUNT match: serP()
    // always declares every optional field (dashArray/paintOrder/
    // centerSegments/strokeId/etc.), explicit `undefined` when N/A, while
    // generateTweens()'s own interpolated-stroke builder (tweens.js) only
    // ever sets the keys it actually needs — so a tween-generated frame
    // structurally has FEWER own keys than one produced by serP(), even
    // when every value that exists on both sides is identical. A strict
    // Object.keys().length check flagged that as a "real change" on the
    // very first save after generateTweens(), permanently marking freshly-
    // generated (never touched) inbetweens as manually edited. An absent
    // key and a key explicitly set to undefined are the same value in JS
    // (`obj.missing === undefined`), so treat them as equal here too.
    var seen={},k;
    for(k in a)seen[k]=true;
    for(k in b)seen[k]=true;
    for(k in seen){if(!_deepCloseEqual(a[k],b[k]))return false;}
    return true;
  }
  return false;
}
// generateTweens()'s interpolated-stroke builder (interpStroke, tweens.js)
// only ever sets the fields it actually blends between two keyframes —
// several fields desP() defaults internally when absent (miterLimit,
// strokeWidth, strokeCap/Join, opacity) never make it into an interpolated
// frame's stored data at all. Comparing raw stored values then finds a
// "real" mismatch between e.g. miterLimit:undefined (tween data) and
// miterLimit:10 (freshly serP()'d from the Paper.js object desP() built,
// which fell back to Paper's own default of 10) — a phantom diff with zero
// user action behind it. Normalizing both sides through the exact same
// fallbacks desP() itself uses (rather than chasing every individual field
// through interpStroke/resampleP, fragile and easy to miss one) makes the
// comparison apples-to-apples regardless of which producer built the data.
function _normStrokeForCompare(sd){
  if(!sd||typeof sd!=='object')return sd;
  var n={};for(var k in sd)n[k]=sd[k];
  n.closed=!!n.closed;
  n.miterLimit=n.miterLimit!==undefined?n.miterLimit:10;
  n.strokeWidth=n.strokeWidth||3;
  n.strokeCap=n.strokeCap||'round';
  n.strokeJoin=n.strokeJoin||'round';
  n.dashOffset=n.dashOffset!==undefined?n.dashOffset:0;
  n.opacity=n.opacity!==undefined?n.opacity:1;
  // interpStroke() (tweens.js) never sets hasRealStroke — only serP() does —
  // so a freshly-generated tween frame's stored data has it undefined while
  // the very next save() re-serializes the live Paper.js object through
  // serP() and gets hasRealStroke:true/false. Same fallback as the other
  // hasRealStroke consumer (isTexAnchor branch above): derive it from
  // strokeColor when absent, so this isn't seen as a real content change.
  n.hasRealStroke=n.hasRealStroke!==undefined?n.hasRealStroke:!!n.strokeColor;
  // Keyline fields of a pressure-brush ribbon are DERIVED, not stored
  // content: desP() calls applyBrushKeyline() on every load, which
  // recomputes strokeColor/strokeWidth/cap/join from fillColor +
  // centerSegments (fractional width, e.g. 0.53). A generated tween
  // frame's stored record meanwhile has whatever its producer left there
  // (typically nothing -> normalized to 3 above). Comparing a recomputed
  // cache against a stale stored copy is a guaranteed phantom diff —
  // confirmed live 2026-07 ("des clés tween qui deviennent vertes... un
  // bug qui revient sans cesse"): merely scrubbing past a tween frame of
  // pressure-brush strokes promoted it to a full keyframe every time,
  // via _maybePromoteInterpolated seeing strokeWidth 3 vs 0.53. Neutralize
  // exactly the fields applyBrushKeyline owns, under exactly its own
  // applicability condition (see that function's guard in this file).
  if(n.isVectorBrush&&!n.isBrushTextureCopy&&!n.isFillShape){
    n.strokeColor='__keyline__';n.strokeWidth=0;n.strokeCap='round';n.strokeJoin='round';n.hasRealStroke='__keyline__';
  }
  // isRaster width/height round-trip through Paper.js's Raster.size setter,
  // which snaps to whole pixels internally (a bitmap can't have fractional
  // backing-store dimensions) — and it FLOORS, never rounds: measured live
  // on this exact build, 496.6→496, 497.5→497, 100.9→100, systematically.
  // A bitmap-brush tween in-between's stored width/height comes straight
  // out of interpStroke()'s float lerp (tweens.js) and essentially never
  // lands on a whole number, so the very first desR()->serR() round-trip
  // (= merely scrubbing past the frame) always disagreed with the stored
  // value — past _numClose()'s 1e-6 tolerance — and
  // _maybePromoteInterpolated() flipped the untouched frame to a
  // manually-edited keyframe. First fix here used Math.round and STILL
  // promoted (caught live by the user, repro: continuous playhead DRAG on
  // the frame header, not cell-by-cell clicks): any stored fraction ≥ .5
  // (round 496.6→497) disagrees with Paper's floor (→496) by a full pixel.
  // Floor BOTH sides — same snap Paper actually applies — so the inherent
  // precision ceiling can never read as an edit. r.position (x/y) is NOT
  // snapped this way (float noise only, well inside tolerance).
  if(n.isRaster){
    if(typeof n.width==='number')n.width=Math.floor(n.width);
    if(typeof n.height==='number')n.height=Math.floor(n.height);
  }
  return n;
}
function strokesEqual(a,b){
  if(!Array.isArray(a)||!Array.isArray(b))return _deepCloseEqual(a,b);
  if(a.length!==b.length)return false;
  for(var i=0;i<a.length;i++)if(!_deepCloseEqual(_normStrokeForCompare(a[i]),_normStrokeForCompare(b[i])))return false;
  return true;
}
window._sceneVersion=0;
// "Ghost All" (Select All Keyframes, timeline.js's selectGhostAll) inserts
// real, editable proxy Paths for every OTHER keyframe of the active layer
// directly into userLayers[activeLayerIdx] alongside the current frame's
// own content, tagged with data.ghostFrame=<that frame's index>, so the
// existing select/move/scale/rotate machinery (which only ever knows how
// to operate on "whatever's in the active user layer") works on them
// unmodified. Writing back has to undo that flattening: pull each tagged
// proxy's (possibly just-transformed) geometry into ITS OWN frame — not the
// current one — before the normal save below runs, and exclude the proxies
// from the current frame's own serialization entirely (they're a working
// copy of a DIFFERENT frame's content, not new content on this one).
function _writeBackGhostProxies(layerIdx){
  var ld=state.layers[layerIdx];if(!ld||!userLayers[layerIdx])return;
  var groups={};
  userLayers[layerIdx].children.forEach(function(c){
    if(c.data&&c.data.ghostFrame!==undefined){
      var gf=c.data.ghostFrame;
      (groups[gf]=groups[gf]||[]).push(c);
    }
  });
  Object.keys(groups).forEach(function(gfKey){
    var gf=parseInt(gfKey,10);
    var targetFrame=ld.frames[gf];if(!targetFrame)return;
    targetFrame.strokes=groups[gfKey].map(function(c){return serP(c);});
  });
}
// Shared by saveActiveLayerFrame/saveAllLayerFrames — CLAUDE.md's "family of
// bug #1" names this exact split (an item/tag excluded in one save path but
// not the other) as the single most dangerous class of bug in this codebase,
// since an item dropped HERE disappears from persisted DATA, not just the
// screen. One collector, called from both, so a future consumer-list change
// (a new data.* tag to skip) can't be applied to one path and forgotten in
// the other.
// LAST-DITCH defence for CLAUDE.md §1's CompoundPath hazard (2026-07-26).
// The collect loop below only serializes `instanceof Path` and Raster, so a
// CompoundPath left in a layer is dropped ENTIRELY and SILENTLY here: measured
// on a rectangle-with-a-hole, 1 child of area 168575 -> 0 strokes saved -> 0
// children after the next loadFrame. The artwork is simply gone, no error, no
// warning, and it renders fine right up until the save.
//
// insertBooleanResult is the documented way to add a boolean result to a
// layer and every current call site uses it — but that is a convention every
// future author has to remember, and §1 exists precisely because this keeps
// recurring. Flattening here instead means it cannot cost artwork again no
// matter which path produced the CompoundPath (a new feature, an import, a
// script). Same island-splitting + keyhole-merge every other consumer already
// expects, so nothing downstream learns anything new.
function _flattenCompoundChildren(li){
  var layer=userLayers[li];
  if(typeof CompoundPath==='undefined'||typeof insertBooleanResult!=='function')return;
  var kids=layer.children.slice();
  for(var i=0;i<kids.length;i++){
    var c=kids[i];
    if(!(c instanceof CompoundPath))continue;
    var at=layer.children.indexOf(c);
    if(at<0)continue;
    var strokeInfo=c.strokeColor?{color:c.strokeColor,width:c.strokeWidth,cap:c.strokeCap,join:c.strokeJoin}:null;
    try{
      insertBooleanResult(layer,at,c,c.fillColor,c.opacity,strokeInfo,c.data);
      console.warn('[save] CompoundPath aplati avant sauvegarde (il aurait été perdu) — voir CLAUDE.md §1');
    }catch(e){console.warn('[save] aplatissement du CompoundPath impossible',e);}
  }
}
function _collectLayerStrokes(li,ld){
  var strokes=[];
  _flattenCompoundChildren(li);
  userLayers[li].children.forEach(function(c){if(c.data&&c.data.ghostFrame!==undefined)return;if(c instanceof Path&&c.segments.length>0){enforceChannelStrip(ld,c);strokes.push(serP(c));}else if(c instanceof Raster)strokes.push(serR(c));});
  return strokes;
}
// A hand-edited tween frame is promoted to a full keyframe outright rather
// than staying flagged isInterpolated (feedback: "une keyframe modifié d'une
// keyframe tween doit devenir une keyframe normal pleine") — but only on a
// REAL content change, since these save functions also run on plain
// navigation (goToFrame persists whatever's on screen before switching
// away); promoting on every run would turn every tween frame into a real
// keyframe the moment you scrub past it.
// Structural fix (2026-07, "une keyframe des tween est devenue verte...
// un bug qui revient sans cesse"): the !strokesEqual comparison alone can
// never be a safe promotion trigger, because it silently depends on the
// serP->desP round-trip being byte-idempotent for EVERY stroke type — and
// every new derived/defaulted field breaks that for one producer or
// another (keyline widths, raster size floors, hasRealStroke, miterLimit —
// see _normStrokeForCompare's growing list of patches for exactly this).
// Promotion now ALSO requires window._tweenFrameDirty — set by
// pushUndoLayers (tweens.js), the single choke point every real mutating
// action already goes through, and cleared on every loadFrame — so pure
// navigation/scrubbing can never promote no matter what phantom diff a
// future field introduces. The strokesEqual check stays as the second
// gate so a no-op action (tool click that aborted) doesn't promote either.
function _maybePromoteInterpolated(f,strokes){
  if(window._tweenFrameDirty&&f.isInterpolated&&!strokesEqual(strokes,f.strokes)){f.isKeyframe=true;f.isInterpolated=false;}
}
// Both save functions MUST skip layers that aren't effectively visible
// (hidden eye, or non-soloed while another layer is soloed) — loadFrame
// deliberately leaves those layers' LIVE Paper layers EMPTY
// (removeChildren + continue, see layerIsEffectivelyVisible there), so
// "saving" one reads an empty live layer and OVERWRITES the frame's real
// stored strokes with []. Confirmed live (2026-07, the reported "des
// pertes d'images après plusieurs actions" data loss): hide a layer with
// content, navigate anywhere (loadFrame empties it), then ANY action that
// saves (every pushUndoLayers — so every draw/erase/edit — plus
// generateTweens) silently wiped the hidden layer's current-frame content.
// The stored frame data is the source of truth for an unpopulated live
// layer — leave it alone.
// Motion caches a component's whole-duration union bounds (the gizmo's fixed
// reference box, symbolUnionBounds in motion.js); frame content is what it is
// derived from, so a write to that content must drop it.
//
// The gate matters as much as the call. These two writers are NOT a
// gesture-end path — goToFrame calls saveAllLayerFrames on EVERY frame
// advance, so invalidating unconditionally meant playback recomputed the
// 120-frame union on every tick: measured 7505 getEffectiveStrokes calls for
// 63 played frames (119 per frame = the union's whole loop), 75% of wall
// clock, 24fps target delivered at 20fps with a 47.7ms repaint interval.
//
// A write can only change a SYMBOL's content while you are inside that
// symbol — enterSymbol swaps state.layers/userLayers to sym.layers, so
// outside one these writers touch ordinary scene layers, which no
// symbolUnionBounds result is derived from. exitToScene is covered because it
// calls saveAllLayerFrames while activeSymbolId is still set. undo/redo are
// hooked separately (tweens.js) since they restore content without passing
// through either writer.
function _invalidateSymbolUnionIfEditingSymbol(){
  if(!state.activeSymbolId)return;
  if(window.SMMotion&&SMMotion.invalidateSymbolUnionBounds)SMMotion.invalidateSymbolUnionBounds();
}
function saveActiveLayerFrame(){
  window._sceneVersion++;
  _invalidateSymbolUnionIfEditingSymbol();
  // duplicator (unless in edit-source mode): the live Paper layer holds the
  // N-way EXPANSION (getEffectiveStrokesRendered, loadFrame) — reading it
  // back here would permanently bake N copies into the stored drawing on
  // the very first scrub. Same class of skip as symbolId (synthetic live
  // content, real data lives elsewhere).
  // ld._rigPoseLive (2026-07-29): a posed-but-not-yet-committed Rig drag —
  // see the identical guard in saveAllLayerFrames below for why this can't
  // just rely on "the user will click Commit quickly": found live, the
  // periodic 30s autosave (setInterval below, timeline.js) calls
  // saveAllLayerFrames unconditionally and would otherwise bake an
  // uncommitted pose into frame data on its own schedule, silently
  // defeating Reset Pose's whole "discard the live drag" contract — the
  // Rig tool is the one interaction in this app where "live but
  // uncommitted" is meant to survive far longer than a single gesture
  // (Commit/Reset are deliberate separate actions, not implied by mouseup).
  var ld=state.layers[state.activeLayerIdx];if(ld.symbolId||ld.nativeVideo||ld.montageId||ld.isNullLayer||ld.isEffectLayer||ld.isGuideLayer||ld.lfsGroup||(ld.duplicator&&!ld._dupEditSource)||ld._rigPoseLive)return;
  if(!layerIsEffectivelyVisible(state.activeLayerIdx))return;
  // Same class of bug as the eye/solo guard right above, found live
  // 2026-07-30 (Cyril: "avec plein d'aller retour, scrub, trim de layer
  // entre motion et animation 2D on perd des data de keyframes"):
  // getEffectiveStrokes returns [] for a frame outside layerInPoint/
  // layerOutPoint (a trimmed layer), so loadFrame leaves THIS layer's live
  // Paper layer empty whenever the playhead sits on one of its own hidden
  // frames — even though ld.frames[frame].strokes still holds real drawn
  // content. Without this gate, saving here (goToFrame's unconditional
  // pre-navigation save, or the 30s autosave interval, timeline.js) reads
  // that empty live layer and permanently overwrites the stored keyframe
  // with []. Reproduced: draw at frames 0/5/10, trim outPoint below 10,
  // navigate away — frame 10's 2 strokes silently became 0.
  // `_justEnsuredKeyframeAt` (2026-08-16, see ensureKeyframe's own comment,
  // tools.js): the ONE call right after a content-creating tool just
  // promoted this exact frame is never a stale navigation/autosave read —
  // it's the save that persists the edit the user just made. Consuming the
  // flag here (whether or not it actually matched) keeps it from lingering
  // into some later, unrelated saveActiveLayerFrame() call.
  var freshlyEnsured=ld._justEnsuredKeyframeAt===state.currentFrame;
  delete ld._justEnsuredKeyframeAt;
  if(!freshlyEnsured&&layerHasTimeRange(ld)&&(state.currentFrame<layerInPoint(ld)||state.currentFrame>layerOutPoint(ld)))return;
  _writeBackGhostProxies(state.activeLayerIdx);
  var f=ld.frames[state.currentFrame];
  if(!f.isKeyframe&&!f.isInterpolated)return;
  var strokes=_collectLayerStrokes(state.activeLayerIdx,ld);
  _maybePromoteInterpolated(f,strokes);
  f.strokes=strokes;
}
function saveAllLayerFrames(){
  _invalidateSymbolUnionIfEditingSymbol();
  _writeBackGhostProxies(state.activeLayerIdx);
  // duplicator skip: same reason as saveActiveLayerFrame's guard above.
  for(var i=0;i<state.layers.length;i++){if(state.layers[i].symbolId||state.layers[i].nativeVideo||state.layers[i].montageId||state.layers[i].isNullLayer||state.layers[i].isEffectLayer||state.layers[i].isGuideLayer||state.layers[i].lfsGroup||(state.layers[i].duplicator&&!state.layers[i]._dupEditSource)||state.layers[i]._rigPoseLive)continue;
  if(!layerIsEffectivelyVisible(i))continue;
  // Trim-range guard — see saveActiveLayerFrame's identical check for the
  // full explanation. Per-layer here (unlike the single active layer
  // above) since every OTHER layer can each have its own independent
  // in/out range while sharing the same global state.currentFrame.
  if(layerHasTimeRange(state.layers[i])&&(state.currentFrame<layerInPoint(state.layers[i])||state.currentFrame>layerOutPoint(state.layers[i])))continue;
  var f=state.layers[i].frames[state.currentFrame];if(!f||(!f.isKeyframe&&!f.isInterpolated))continue;
  var strokes=_collectLayerStrokes(i,state.layers[i]);
  _maybePromoteInterpolated(f,strokes);
  f.strokes=strokes;}
}
// Guards for loadFrame's rebuild skip — deliberately conservative: every one
// of these returning false costs a rebuild (slow), while a wrong `true` would
// paint stale content (silent). In doubt, rebuild.
function _canReuseMaterialized(lyr,strokes){
  // No dirty signal available (engine off, or the _changed probe failed) means
  // no way to know whether the live items were edited — never reuse.
  if(!window.__smGeomDirtyHookInstalled)return false;
  if(lyr._smGeomDirty)return false;
  if(!lyr._matStrokes||lyr._matStrokes!==strokes)return false;
  // An empty result is a FRESH array on every call (`return []`), so it could
  // never match anyway; excluded explicitly so the intent is readable.
  if(!strokes.length)return false;
  // Paranoia: desP/desR emit exactly one item per stroke. Any divergence
  // (a consumer that inserted or removed something) forces a rebuild rather
  // than trusting the identity test alone.
  if(lyr.children.length!==strokes.length)return false;
  return true;
}
// `dupOnly` (2026-07-29): renderNow()'s duplicator-refresh guard (engine-
// bridge.js) used to call loadFrame(idx) UNSCOPED — rebuilding EVERY layer,
// not just duplicator ones. loadFrame is also the chokepoint that makes a
// DIRTY layer (_smGeomDirty, e.g. a live Subselect/Eraser/Rig drag not yet
// saved) discard its uncommitted edit and rebuild from stored data — by
// design for actual frame navigation (scrub/playback/goToFrame), where
// abandoning an unsaved edit on the frame you're leaving is correct. But
// renderNow()'s guard calls loadFrame on the SAME frame purely to force the
// duplicator's fresh N-way expansion (dupOffset* is only read here) — not a
// navigation at all. Found live: with ANY duplicator-enabled layer anywhere
// in the document, every OTHER layer's live drag (Subselect node-drag, Rig
// pose-drag — anything calling the plain renderNow() per move tick) got
// silently reverted to its stored (rest) geometry on literally every
// pointermove, because this same forced rebuild ran across the whole
// document each time. `dupOnly=true` skips every non-duplicator layer
// entirely (no removeChildren, no rebuild, no dirty-flag reset) so the
// duplicator refresh no longer has this collateral blast radius.
function loadFrame(idx,dupOnly){
  window._sceneVersion++;
  // See _maybePromoteInterpolated's own comment — loadFrame is the one
  // choke point every frame navigation (scrub, playback, goToFrame) goes
  // through, same reasoning as the SMReference hook right below. Cleared
  // here so scrubbing OFF a frame never carries a stale "dirty" flag onto
  // whatever gets saved on the way out; pushUndoLayers sets it fresh the
  // moment a real action actually happens.
  window._tweenFrameDirty=false;
  // Rotoscopy reference follows the playhead (video seek / sequence frame
  // pick) — loadFrame is the one choke point every frame change goes
  // through, scrub and playback alike.
  if(window.SMReference)SMReference.onFrameChanged(idx);
  // EXPERIMENTAL (native-video-decode branch): natively-decoded video
  // LAYERS follow the playhead through the same choke point.
  if(window.SMNativeVideo)SMNativeVideo.onFrameChanged(idx);
  for(var i=0;i<state.layers.length;i++){
  if(dupOnly&&!state.layers[i].duplicator)continue;
  if(!layerIsEffectivelyVisible(i)){userLayers[i].removeChildren();userLayers[i]._matStrokes=null;continue;}
  // Skip the rebuild when this layer's effective content is literally the
  // SAME array it was last built from. getEffectiveStrokes returns the stored
  // `f.strokes` (or an inherited keyframe's), so every frame that HOLDS on a
  // keyframe hands back an identical array object — measured 83% of
  // layer-frames on a 6-frame-hold project. Rebuilding them was 88% of
  // loadFrame's cost (32.6ms of 37.0ms per frame at 4000 strokes), and it
  // produced Paper items identical to the ones already sitting there.
  //
  // This is an identity test, not a deep compare, and that is the point:
  // every writer REPLACES the array (f.strokes = strokes, saveActiveLayerFrame)
  // rather than mutating it in place, so a changed frame always presents a
  // different object. Component/montage/lfs layers synthesize a fresh array
  // per call and therefore never match — they keep rebuilding, unchanged.
  // getEffectiveStrokesRendered = getEffectiveStrokes + the mograph
  // duplicator's N-way expansion (no-op for every non-duplicator layer —
  // identical array, so the identity-reuse test below is untouched). A
  // duplicator layer returns a fresh array per call and therefore always
  // rebuilds, same accepted cost as component/montage/lfs layers.
  var strokes=getEffectiveStrokesRendered(i,idx);
  if(_canReuseMaterialized(userLayers[i],strokes))continue;
  userLayers[i].removeChildren();
  // No explicit `op` override here (unlike renderOS()'s onion-skin ghosts,
  // which intentionally force a computed fade-opacity regardless of the
  // object's own value) — omitting it lets desR/desP fall through to the
  // stroke's ACTUAL stored opacity. Passing a hardcoded 1 here silently
  // discarded every real per-object opacity (including tween-interpolated
  // opacity blends) on every normal frame load, and the resulting mismatch
  // against the correctly-stored interpolated value permanently flagged
  // untouched inbetweens as "manually edited" the moment you navigated
  // through them.
  strokes.forEach(function(sd){if(sd.isRaster)desR(sd,userLayers[i]);else desP(sd,userLayers[i]);});relinkBrushCompanions(userLayers[i]);relinkLinkedFills(userLayers[i]);if(state.layers[i].rig)relinkRigBinds(state.layers[i],userLayers[i]);
  userLayers[i]._matStrokes=strokes;userLayers[i]._smGeomDirty=false;}
  userLayers[state.activeLayerIdx].activate();
  // Vue caméra (v18) : loadFrame est LE point de passage de tout changement
  // de frame (scrub, lecture, goToFrame) — même raison que le hook
  // SMReference au-dessus. Le hook updatePlayhead seul ne couvrait que la
  // lecture (goToFrame hors lecture passe par updateUI, pas updatePlayhead).
  if(window.SMCamera)SMCamera.applyCameraView();
}
// Solo (After Effects-style): while ANY layer has ld.solo set, only soloed
// layers render — overriding each layer's own eye/visible toggle entirely
// (not combined with it), matching how every NLE/animation tool's solo
// behaves. A transient view-only concept, not part of the saved project
// (state.layers[i].solo is simply never read by serialize/save-project).
function anyLayerSoloed(){for(var i=0;i<state.layers.length;i++)if(state.layers[i].solo)return true;return false;}
function layerIsEffectivelyVisible(i){var ld=state.layers[i];if(!ld)return false;return anyLayerSoloed()?!!ld.solo:!!ld.visible;}
// Propagates a keyframe TIMING change (add/remove — not content) from one
// layer to its siblings in the same Stroke/Fill/Shadow link group
// (state.layerLinkGroups[id].linkedKeyframes) — NOT layerFolders, deliberately:
// these 3 layers must render/behave as ordinary independent rows (own
// working eye/lock/solo, no collapse arrow, no folder wrapper at all), only
// their keyframe TIMING stays synced. Each channel keeps its OWN
// independent content (a Fill sub-layer never gets the Stroke channel's
// geometry copied onto it) — only whether frameIdx IS a keyframe on that
// layer stays in sync, so scrubbing anywhere always shows all 3 channels in
// a consistent key/held state (distinct from the existing
// state.layers[i].lfsGroup/symbol-based system, whose 3 channels are each
// their own independent mini-timeline with NO shared timing at all).
function syncLinkedKeyframeFolder(sourceLayerIdx,frameIdx){
  var srcLd=state.layers[sourceLayerIdx];
  var gid=srcLd&&srcLd.linkGroupId;
  if(!gid)return;
  var gmeta=state.layerLinkGroups[gid];
  if(!gmeta||!gmeta.linkedKeyframes)return;
  var srcFrame=srcLd.frames[frameIdx];if(!srcFrame)return;
  state.layers.forEach(function(ld,i){
    if(i===sourceLayerIdx||ld.linkGroupId!==gid)return;
    var f=ld.frames[frameIdx];if(!f)return;
    if(srcFrame.isKeyframe&&!f.isKeyframe){
      f.isKeyframe=true;f.isInterpolated=false;if(!f.strokes)f.strokes=[];
    }else if(!srcFrame.isKeyframe&&f.isKeyframe){
      f.isKeyframe=false;f.strokes=[];f.isInterpolated=false;
    }
  });
}
// Strips whichever color doesn't belong to `ld`'s channel — called from
// saveActiveLayerFrame/saveAllLayerFrames on the LIVE Paper.js item (so the
// canvas itself, not just persisted data, reflects the constraint the
// instant a stroke commits), enforcing the Stroke/Fill/Shadow split ON
// GOING FORWARD, not just at the one-time conversion moment. Skipped for
// item kinds where fillColor/strokeColor don't mean "two independent paint
// channels": rasters (no stroke concept), vector-brush ribbons (fillColor
// IS the ribbon's paint, not a separate fill region), brush-texture
// anchors/dabs and linked-fill companions (already have their own valid
// color model — see tools.js applyBrushTexture / draw-bridge.js linkedFill).
function enforceChannelStrip(ld,c){
  if(!ld.channel)return;
  if(c instanceof Raster)return;
  if(c.data&&(c.data.isVectorBrush||c.data.brushTexturePreset||c.data.isBrushTextureCopy||c.data.isLinkedFillCompanion))return;
  // Shadow channel content can be a real shadow-boundary stroke, a shadow
  // fill, or both — unlike Fill, nothing gets forced off here.
  if(ld.channel==='shadow')return;
  if(ld.channel==='stroke')c.fillColor=null;
  else c.strokeColor=null; // 'fill'
}
// Splits ONE layer into 3 real, fully ordinary independent layers (Stroke/
// Fill/Shadow) linked ONLY by keyframe timing (syncLinkedKeyframeFolder) and
// an auto-strip enforcement (enforceChannelStrip) — deliberately NOT a
// layerFolders group: no collapse header, no aggregate eye/lock/solo, each
// one behaves exactly like any other layer in the list, matching what a
// "channel split" should feel like (3 sibling layers, not a sub-timeline).
// Each keyframe's existing strokes are duplicated into stroke-only
// (fillColor stripped) and fill-only (strokeColor stripped) clones for
// their respective channels; Shadow starts empty on every frame ("à
// définir plus tard" per the request this was built from — no shadow-
// generation algorithm exists yet, just the channel/slot for one to land in
// later). A raster (bitmap) has no stroke at all, so it goes to the Fill
// channel by convention (closest analogue: a bitmap IS a filled region, no
// separate outline).
function convertLayerToStrokeFillShadowFolder(layerIdx){
  var src=state.layers[layerIdx];
  if(!src)return;
  if(src.symbolId||src.lfsGroup){showToast('Impossible sur un composant ou un groupe LFS existant');return;}
  saveAllLayerFrames();
  pushUndoLayers(true);
  // Items drawn with the Draw tool's "Shadow" toggle on carry
  // data.channelTag='shadow' (persisted via serP as sd.channelTag) — those
  // route to the Shadow channel wholesale, keeping BOTH their stroke and
  // fill exactly as drawn (a shadow-boundary line, a shadow fill, or both),
  // and are excluded from the normal stroke/fill classification below so a
  // shadow stroke doesn't ALSO get duplicated into the Stroke channel.
  function splitFrame(f,channel){
    var strokes=f.strokes.map(function(sd){return JSON.parse(JSON.stringify(sd));}).filter(function(sd){
      if(sd.channelTag==='shadow')return channel==='shadow';
      if(channel==='shadow')return false;
      if(sd.isRaster)return channel==='fill';
      // sd.strokeColor can be the '#ffffff' fallback serP() writes for a
      // genuinely strokeless path (see serP's hasRealStroke comment) — using
      // it here would wrongly clone every fill-only shape into the Stroke
      // channel too. hasRealStroke is the pre-fallback truth; fall back to
      // the old (buggy) check only for frames saved before this field
      // existed, so already-persisted projects still classify SOMETHING.
      var hasStroke=sd.hasRealStroke!==undefined?sd.hasRealStroke:!!sd.strokeColor,hasFill=!!sd.fillColor;
      return channel==='stroke'?hasStroke:hasFill;
    }).map(function(sd){
      if(sd.isRaster||sd.channelTag==='shadow')return sd;
      if(channel==='stroke')sd.fillColor=null;
      if(channel==='fill')sd.strokeColor=null;
      return sd;
    });
    return{strokes:strokes,isKeyframe:f.isKeyframe,isInterpolated:f.isInterpolated,isManualEdit:f.isManualEdit||false};
  }
  var channels=[{suffix:'Stroke',key:'stroke'},{suffix:'Fill',key:'fill'},{suffix:'Shadow',key:'shadow'}];
  var newLayers=channels.map(function(ch){
    var idx=createUserLayer(src.name+' '+ch.suffix);
    state.layers[idx].channel=ch.key;
    state.layers[idx].frames=src.frames.map(function(f){return splitFrame(f,ch.key);});
    return state.layers[idx];
  });
  var gid='lg-'+Date.now()+'-'+Math.floor(Math.random()*1000);
  state.layerLinkGroups[gid]={linkedKeyframes:true,collapsed:false};
  newLayers.forEach(function(ld){ld.linkGroupId=gid;});
  // Remove the original layer now that its content lives split across the
  // 3 new ones — same removal pattern deleteLayer() already uses.
  var origIdx=state.layers.indexOf(src);
  userLayers[origIdx].remove();userLayers.splice(origIdx,1);state.layers.splice(origIdx,1);
  var newActiveIdx=state.layers.indexOf(newLayers[0]);
  state.activeLayerIdx=Math.max(0,newActiveIdx);
  activateUL(state.activeLayerIdx);
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
  if(window.renderLayerList)window.renderLayerList();if(window.renderTimeline)window.renderTimeline();
  showToast(SM.t('toastLayerSplitSFS'));
}
function goToFrame(idx){
  if(idx<0||idx>=state.totalFrames)return;
  // Already on this frame (e.g. a live-scrub tick that re-dispatched 'change'
  // without the value actually moving) — avoid a redundant save+reload pass.
  if(idx===state.currentFrame)return;
  // Bug found live (2026-07-29 QA sweep): an INTERRUPTED Subselect node-drag
  // (pointerdown+move, never released) left window._nodeDrag.active pointing
  // at a Path that loadFrame() is about to rebuild with a brand-new identity.
  // Any later stray pointermove/up kept applying the drag delta by segIndex
  // alone to whatever object now occupies that slot — silently corrupting the
  // new frame's stored geometry. Scrubbing frames must sever any in-progress
  // node edit the same way releasing the mouse would, before the rebuild.
  if(typeof _nodeDrag!=='undefined'){_nodeDrag.active=false;_nodeDrag.path=null;}
  if(typeof _nmq!=='undefined'){if(_nmq.rect){_nmq.rect.remove();_nmq.rect=null;}_nmq.active=false;}
  // Bug found live (2026-07-29 QA sweep): loadFrame() below rebuilds every
  // layer's live Paper.js children with brand-new object identities, but a
  // pre-existing shape selection (selectedPaths) kept pointing at the OLD,
  // now-detached objects (.parent===null) — the Transform/Fill/Stroke panel
  // kept showing and silently "editing" that ghost with zero effect on the
  // real, on-screen frame. clearSel(true) drops the shape selection (not the
  // layer selection — the `true` preserves _layerActiveExplicit, unlike a
  // canvas deselect, so layer-sec doesn't flicker shut on every scrub).
  if(selectedPaths.length||_nodeSel.length)clearSel(true);
  saveAllLayerFrames();state.currentFrame=idx;window._curFrame=idx;
  if(window.SMAudio&&!state.playing)SMAudio.scrubAt(idx); // scrub audio au deplacement du playhead (v19)
  // frameOnly: goToFrame changes the frame and nothing else — see updateUI.
  loadFrame(idx);if(!state.playing){renderOS();renderArcs();updateUI(true);}else{updatePlayhead();}
}

// F5/insertFrame — Animate's actual convention (corrected: an earlier pass
// here assumed this had to stay global, wrongly): F5 extends ONLY the
// selected layer(s)' timing at the cursor, or every layer if none is
// selected. Every layer's frames array must still stay EXACTLY
// state.totalFrames long (frame navigation/getEffectiveStrokes/save-load
// all assume that everywhere) — so a NON-targeted layer still grows by one
// slot to keep pace, but its slot is APPENDED at the very END of its own
// array instead of spliced at the cursor. That's a pure length pad, not a
// content change: getEffectiveStrokes already treats anything past a
// layer's last real keyframe as an ongoing hold of it, so one more blank
// hold-frame tacked on past the end never changes what's shown at any
// EXISTING index — the non-targeted layer's own keyframes stay at their
// exact original frame numbers, completely undisturbed by another layer's
// insert. Only a TARGETED layer gets the real splice at cf+1, pushing its
// own later keyframes right by one — exactly where the cursor sits.
function insertFrame(){
  var explicitSel=(typeof _layerSel!=='undefined'&&_layerSel.length);
  var targets=explicitSel?_layerSel.slice():state.layers.map(function(_ld,i){return i;});
  // 2026-07 feedback: "on a encore la possibilité de modifier les keyframe
  // dans la timeline quand celui-ci est lock" — a locked layer must never
  // get the real splice-at-cursor, only the harmless length-pad every
  // OTHER non-targeted layer already gets (see this function's own header
  // comment). An explicit selection of ONLY locked layer(s) has nothing
  // left to insert into, so it aborts with a toast instead of silently
  // becoming a no-op "insert into every layer" fallback.
  targets=targets.filter(function(i){return !state.layers[i].locked;});
  if(explicitSel&&!targets.length){showToast(SM.t('toastLayerLocked'));return;}
  pushUndoLayers();
  var cf=state.currentFrame;
  for(var i=0;i<state.layers.length;i++){
    var blank={strokes:[],isKeyframe:false,isInterpolated:false};
    var lyr=state.layers[i];
    if(targets.indexOf(i)>=0){
      lyr.frames.splice(cf+1,0,blank);
      // A manual trim/marker is a raw frame NUMBER (app.js/markers.js), not a
      // reference into the frames array — splicing a slot in at cf+1 moves
      // every stored keyframe at or after that point one frame later, but
      // left these untouched (2026-08-16, found live: trim a layer's in point
      // to frame 2, F5 at frame 0 — content slides to frame 3, the bar's
      // visible window still starts at 2, now blank). Only entries AT OR
      // AFTER the insertion point shift; anything already before cf+1 is
      // genuinely unaffected by the splice.
      if(lyr.inPoint!=null&&lyr.inPoint>cf)lyr.inPoint++;
      if(lyr.outPoint!=null&&lyr.outPoint>cf)lyr.outPoint++;
      if(lyr.markers)lyr.markers.forEach(function(m){if(m.frame>cf)m.frame++;});
    }
    else lyr.frames.push(blank);
  }
  if(state.markers)state.markers.forEach(function(m){if(m.frame>cf)m.frame++;});
  state.totalFrames++;
  if(state.waOut<state.totalFrames-1)state.waOut++;
  window._waOut=state.waOut;window._totalF=state.totalFrames;
  goToFrame(cf+1);
  showToast(SM.t('toastFrameInsertedF5')+(targets.length<state.layers.length?' — '+targets.length+' calque(s)':''));
}
// Core per-layer step shared by insertKeyframeAt (single layer+frame, own
// undo/render — used by the span-end drag handle in timeline.js) and
// insertKeyframe below (multi-layer, ONE undo/render for the whole batch).
// No undo/render side effects of its own — callers own that.
function _insertKeyframeCore(layerIdx,frameIdx){
  var ld=state.layers[layerIdx];var f=ld.frames[frameIdx];
  if(ld.symbolId){
    var sym=state.symbols[ld.symbolId];
    f.strokes=[];
    f.componentFrame=sym?resolveSymbolFrameIdx(sym,ld,frameIdx):0;
    delete f.blankOverride;
    f.isKeyframe=true;f.isInterpolated=false;
    syncLinkedKeyframeFolder(layerIdx,frameIdx);
    return;
  }
  f.strokes=JSON.parse(JSON.stringify(getEffectiveStrokes(layerIdx,frameIdx)));
  f.isKeyframe=true;f.isInterpolated=false;
  syncLinkedKeyframeFolder(layerIdx,frameIdx);
}
// F6 — targets _layerSel (the layer-panel batch selection, see the
// deselect-all click zones in timeline.js) when the user has explicitly
// picked layer(s), otherwise every layer — matching Animate/Harmony's "no
// selection = affects everything" convention. Previously always hit only
// state.activeLayerIdx no matter what was selected in the panel (reported:
// "peu importe le layer sélectionné ça ajoute des keyframes à tous" — the
// bug was the opposite direction, always-all instead of always-active, but
// same root cause: _layerSel was never consulted at all).
function insertKeyframe(){
  saveAllLayerFrames();
  var targets=(typeof _layerSel!=='undefined'&&_layerSel.length)?_layerSel.slice():state.layers.map(function(_ld,i){return i;});
  var cf=state.currentFrame;
  var lockedHit=false;
  var eligible=targets.filter(function(li){
    var ld=state.layers[li];
    if(!ld)return false;
    // Component instances are locked against editing their internal drawing
    // on the parent canvas, but their OUTER timing row must remain editable.
    if(ld.locked&&!ld.symbolId){lockedHit=true;return false;}
    return !ld.frames[cf].isKeyframe;
  });
  if(!eligible.length){showToast(lockedHit?SM.t('toastLayerLocked'):SM.t('toastAlreadyAKeyframe'));return;}
  pushUndoLayers();
  eligible.forEach(function(li){_insertKeyframeCore(li,cf);});
  loadFrame(cf);renderOS();renderArcs();updateUI();
  showToast(eligible.length>1?eligible.length+SM.t('toastKeyframesInsertedSuffix'):SM.t('toastKeyframeInserted'));
}
// Same as insertKeyframe() but for an arbitrary layer/frame instead of only
// the active layer at the current playhead — used to shorten a held/
// extended keyframe's span by dropping a new keyframe partway through it
// (the frame grid's small drag handle on the span's end tick), since hold
// spans have no stored length of their own: they're purely derived by
// scanning forward to wherever the next real keyframe happens to be
// (renderTimeline() in timeline.js), so "shrinking" one is really "insert a
// keyframe earlier, which becomes the new boundary".
// J/K transport (timeline.js onKeyDown): nearest real keyframe strictly
// before/after `fromFrame` on the given layer — clamped to stay in range
// rather than wrapping, so repeated J/K at either end just holds still on
// the first/last keyframe instead of jumping to the opposite end.
function prevKeyframeFrame(layerIdx,fromFrame){
  var ld=state.layers[layerIdx];if(!ld)return fromFrame;
  for(var i=fromFrame-1;i>=0;i--)if(ld.frames[i].isKeyframe)return i;
  return fromFrame;
}
function nextKeyframeFrame(layerIdx,fromFrame){
  var ld=state.layers[layerIdx];if(!ld)return fromFrame;
  for(var i=fromFrame+1;i<ld.frames.length;i++)if(ld.frames[i].isKeyframe)return i;
  return fromFrame;
}
function insertKeyframeAt(layerIdx,frameIdx){
  saveAllLayerFrames();
  var ld=state.layers[layerIdx];var f=ld.frames[frameIdx];
  if(ld.locked&&!ld.symbolId){showToast(SM.t('toastLayerLocked'));return false;}
  if(f.isKeyframe){showToast(SM.t('toastAlreadyAKeyframe'));return false;}
  pushUndoLayers();
  _insertKeyframeCore(layerIdx,frameIdx);
  if(layerIdx===state.activeLayerIdx)loadFrame(state.currentFrame);
  renderOS();renderArcs();updateUI();showToast(SM.t('toastKeyframeInserted'));
  return true;
}
function insertBlankKeyframe(){
  var ld=state.layers[state.activeLayerIdx];
  if(ld.locked&&!ld.symbolId){showToast(SM.t('toastLayerLocked'));return;}
  saveAllLayerFrames();pushUndoLayers(true);
  var f={strokes:[],isKeyframe:true,isInterpolated:false};
  // On a component layer this main-timeline row is otherwise dead timing
  // decoration (getEffectiveStrokes' symbolId branch never reads it) — the
  // explicit flag is what makes THIS blank keyframe actually hide the
  // component for its span, same as a normal layer, without also treating
  // the harmless isKeyframe:true seed every component starts with (frame 0,
  // see convertLayerToComponent) as an accidental hide.
  if(ld.symbolId)f.blankOverride=true;
  ld.frames[state.currentFrame]=f;
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast('Blank keyframe (F7)');
}
function removeFrame(){if(state.totalFrames<=1)return;pushUndoLayers();var cf=state.currentFrame;for(var i=0;i<state.layers.length;i++){var lyr=state.layers[i];lyr.frames.splice(cf,1);
  // Mirror of insertFrame's own fix (2026-08-16): removing the slot at cf
  // shifts every LATER frame number down by one, same shift a manual trim/
  // marker needs or it silently points one frame later than the content it
  // used to mark. A value AT cf needs no adjustment — whatever previously
  // sat at cf+1 now occupies cf, so the reference is still correct as-is.
  if(lyr.inPoint!=null&&lyr.inPoint>cf)lyr.inPoint--;
  if(lyr.outPoint!=null&&lyr.outPoint>cf)lyr.outPoint--;
  if(lyr.markers)lyr.markers.forEach(function(m){if(m.frame>cf)m.frame--;});
}
  if(state.markers)state.markers.forEach(function(m){if(m.frame>cf)m.frame--;});
  state.totalFrames--;if(state.waOut>=state.totalFrames)state.waOut=state.totalFrames-1;window._waOut=state.waOut;window._totalF=state.totalFrames;if(state.currentFrame>=state.totalFrames)state.currentFrame=state.totalFrames-1;loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast(SM.t('toastFrameDeleted'));}
// Animate's "Clear Keyframe" — demotes a keyframe back into a plain
// extended frame (content reverts to whatever the previous keyframe holds),
// without removing the frame slot itself (unlike removeFrame/removeFrameSpan).
function clearKeyframe(){
  var ld=state.layers[state.activeLayerIdx];var cf=state.currentFrame;var f=ld.frames[cf];
  if(ld.locked){showToast(SM.t('toastLayerLocked'));return;}
  if(!f||!f.isKeyframe){showToast(SM.t('toastNotAKeyframe'));return;}
  pushUndo();f.strokes=[];f.isKeyframe=false;f.isInterpolated=false;
  syncLinkedKeyframeFolder(state.activeLayerIdx,cf);
  loadFrame(cf);renderOS();renderArcs();updateUI();showToast(SM.t('toastKeyframeCleared'));
}
// Animate's "Convert to Keyframes" — bakes whatever's currently displayed
// (a tweened inbetween, or content inherited from an earlier keyframe) into
// an independent real keyframe on every frame in the selection (or just the
// current frame if nothing's selected), so it can be hand-adjusted without
// disturbing the rest of the tween.
function convertToKeyframes(){
  var li=state.activeLayerIdx;var ld=state.layers[li];
  if(ld.locked){showToast(SM.t('toastLayerLocked'));return;}
  var frames=_sel.frames.length?_sel.frames.filter(function(s){return s.layer===li;}).map(function(s){return s.frame;}):[state.currentFrame];
  if(!frames.length){showToast(SM.t('toastNoSelection'));return;}
  pushUndo();saveAllLayerFrames();var count=0;
  frames.forEach(function(fi){
    var f=ld.frames[fi];if(!f||f.isKeyframe)return;
    f.strokes=JSON.parse(JSON.stringify(getEffectiveStrokes(li,fi)));f.isKeyframe=true;f.isInterpolated=false;count++;
  });
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast(count+SM.t('toastKeyframesCreatedSuffix'));
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
  selClear();loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast(count+SM.t('toastFramesDeletedSuffix'));
}
// Duplicate the current frame selection in place, right after itself —
// equivalent to copy + paste-at-selection-end, exposed as one menu action.
function duplicateSelectedFrames(){
  var b=selBounds();
  if(!b){showToast(SM.t('toastNoSelection'));return;}
  window.SM.copyFrames();
  var span=b.maxF-b.minF+1;
  state.currentFrame=b.minF+span;window._curFrame=state.currentFrame;
  window.SM.pasteFrames();
}
