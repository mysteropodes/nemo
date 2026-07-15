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
  onionSkin:true,onionPrevOpacity:30,onionNextOpacity:30,onionMode:'tinted',
  onionIn:0,onionOut:23,
  brushSize:3,fillBrushSize:40,strokeColor:'#000000',fillColor:'#ff0000',fillEnabled:true,strokeEnabled:true,shadowMode:false,brushPreset:'none',
  smoothing:10,stabilizer:2,strokeCap:'round',strokeJoin:'round',opacity:100,
  miterLimit:10,dashOffset:0,paintOrder:'fillFirst',
  strokeStyle:'solid',vectorBrush:false,taperEnds:false,
  drawMode:'front',fillBrushMode:'below',pressureMin:30,pressureMax:170,pressureInvert:false,pressureCurve:'linear',
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
  canvasW:1920,canvasH:1080,canvasBg:'#ffffff',canvasClip:false,safetyZones:false,ghostAllFrames:false,
  // Multi-palette swatch library (Shade-for-AE-style, palette-panel.js) —
  // an array (not a map) so tab order IS display order, no separate sort
  // field needed. colorPalette (flat array) is kept as a legacy field ONLY
  // for reading pre-multi-palette project files — see importJSON's own
  // migration comment; never written to going forward.
  palettes:[{id:'p0',name:'Palette 1',colors:['#000000','#ffffff','#ff0000','#ff8800','#ffee00','#00cc44','#0088ff','#8833ff']}],
  activePaletteIdx:0,
  customBrushPresets:{}, // user-saved procedural brush presets, keyed by generated id — see brush-editor.js
  layers:[],activeLayerIdx:0,
  motionArcs:{},easingCurve:{points:[{x:0,y:0},{x:.42,y:0},{x:.58,y:1},{x:1,y:1}]},
  // v16: manual inbetween/tween reassignment — {layer+':'+fA+'-'+fB: [{aId,bId},...]}
  // forces autoMatch (tweens.js generateTweens) to pair a specific stroke
  // (by stable data.strokeId) on keyframe A with a specific stroke on
  // keyframe B, overriding the auto-matcher for that one pair when it
  // misidentifies correspondence (large shape change, size change).
  tweenOverrides:{},
  selectedStrokeIndices:[],
  undoStack:[],redoStack:[],maxUndo:60,
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
  perspectiveMode:'2pt', // '1pt' | '2pt' | '3pt'
  perspectiveDensity:24, // guide lines radiating from each vanishing point
  perspectiveVPs:null, // lazily seeded to sane defaults for canvasW/H on first enable — see ensurePerspectiveVPs() in perspective-bridge.js
  audioTracks:[], // {name,dataB64,offsetFrames,volume,muted} — playback/waveform in audio-bridge.js
  xformAnchorKey:'mc', // rotate/scale pivot preset (9-dot widget) — tl/tc/tr/ml/mc/mr/bl/bc/br, see tools.js xformAnchorPoint
  xformAnchorCustom:null, // [x,y] world-space override from Alt+click (select-bridge.js) — takes priority over xformAnchorKey when set
  refMedia:null, // rotoscopy reference {type:'video'|'imageseq'|'image',...} — reference-bridge.js
  mediaLibrary:[], // {id,name,kind:'image'|'video',thumb,layerName} — browsable catalog of imports, media-library.js
  symbols:{},activeSymbolId:null,openSymbolTabs:[],
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
  state.layers.push({id:state.layers.length,name:name,visible:true,locked:false,frames:frames,color:nextLayerColor()});
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
function fitCanvas(){var vw=view.viewSize.width,vh=view.viewSize.height;var z=Math.min(vw*.85/state.canvasW,vh*.85/state.canvasH,3);view.zoom=z;view.center=new Point(state.canvasW/2,state.canvasH/2);state.canvasRotation=0;updZoom();}
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
// Copy-pasted across 13 call sites (select-bridge.js, tools.js, timeline.js)
// before this — the exclusion rule for "is this child a real, independently
// selectable stroke, or a rendering artifact of its anchor stroke" (linked-
// fill companions, brush-texture dab copies). One helper so a future sibling
// tag (CLAUDE.md's own example) can't be added to one call site and
// forgotten in the other twelve.
function isSelectablePathChild(c){return !(c.data&&(c.data.isLinkedFillCompanion||c.data.isBrushTextureCopy));}
function serP(p){var isVB=!!(p.data&&p.data.isVectorBrush);var center=isVB&&p.data.centerSegments?p.data.centerSegments:undefined;
  var widthProfile=isVB&&p.data.widthProfile?p.data.widthProfile:undefined;
  var fillSeed=(p.data&&p.data.fillSeed)?p.data.fillSeed:undefined,fillGapPx=(p.data&&p.data.fillGapPx!==undefined)?p.data.fillGapPx:undefined;
  var fillWalls=(p.data&&p.data.fillWalls&&p.data.fillWalls.length)?p.data.fillWalls:undefined,strokeId=(p.data&&p.data.strokeId)?p.data.strokeId:undefined;
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
  var isTexAnchor=!!(p.data&&(p.data.brushTexturePreset||p.data.isBrushTextureCopy));
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
  var isNoStrokeChannel=!!(pChLayer&&pChLayer.channel==='fill');
  var isShadowNoStroke=!!(pChLayer&&pChLayer.channel==='shadow'&&!p.strokeColor);
  var channelTag=(p.data&&p.data.channelTag)?p.data.channelTag:undefined;
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
  // hasRealStroke records whether p ACTUALLY had a stroke color before the
  // '#ffffff' fallback below (kept for legacy/rendering reasons — see the
  // colorHex8/serP comments above) papers over that with a non-null string.
  // Any consumer that needs to know "did the user really draw a stroke here"
  // (e.g. convertLayerToStrokeFillShadowFolder's splitFrame, which classifies
  // items into the Stroke vs Fill channel) must read THIS, not strokeColor —
  // otherwise every fill-only shape's phantom '#ffffff' gets misread as a
  // real stroke and the shape is wrongly cloned into the Stroke channel too.
  var hasRealStroke=!!p.strokeColor;
  return{segments:p.segments.map(function(s){return{point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};}),closed:!!p.closed,strokeColor:(isVB||isNoStrokeChannel||isShadowNoStroke||isTexAnchor&&!p.strokeColor)?null:(p.strokeColor?colorHex8(p.strokeColor):'#ffffff'),hasRealStroke:hasRealStroke,strokeWidth:p.strokeWidth,strokeCap:p.strokeCap||'round',strokeJoin:p.strokeJoin||'round',miterLimit:p.miterLimit,fillColor:p.fillColor?colorHex8(p.fillColor):null,opacity:p.opacity!==undefined?p.opacity:1,dashArray:(p.dashArray&&p.dashArray.length)?p.dashArray.slice():undefined,dashOffset:p.dashOffset,paintOrder:(p.data&&p.data.paintOrder)?p.data.paintOrder:undefined,isVectorBrush:isVB||undefined,centerSegments:center,widthProfile:widthProfile,fillSeed:fillSeed,fillGapPx:fillGapPx,fillWalls:fillWalls,strokeId:strokeId,brushGroupId:brushGroupId,boxAngle:(p.data&&p.data.boxAngle)?p.data.boxAngle:undefined,isBrushTextureCopy:isBrushTextureCopy,brushTexturePreset:brushTexturePreset,preTextureOpacity:preTextureOpacity,preTextureStroke:preTextureStroke,channelTag:channelTag,ownerId:ownerId,ownerName:ownerName,ownerColor:ownerColor,revisionParentId:revisionParentId,isRevisionGhost:isRevisionGhost,revisionAction:revisionAction};}
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
  p.strokeColor=d.hasRealStroke===false?null:(d.strokeColor||((d.isVectorBrush||d.brushTexturePreset||d.isBrushTextureCopy||dNoStrokeChannel||dIsShadowChannel)?null:'#fff'));p.strokeWidth=d.strokeWidth||3;p.strokeCap=d.strokeCap||'round';p.strokeJoin=d.strokeJoin||'round';if(d.miterLimit!==undefined)p.miterLimit=d.miterLimit;if(d.fillColor)p.fillColor=d.fillColor;else p.fillColor=null;p.opacity=op!==undefined?op:(d.opacity!==undefined?d.opacity:1);if(d.dashArray&&d.dashArray.length)p.dashArray=d.dashArray;if(d.dashOffset!==undefined)p.dashOffset=d.dashOffset;if(d.paintOrder){p.data.paintOrder=d.paintOrder;}if(d.isVectorBrush){p.data.isVectorBrush=true;if(d.centerSegments)p.data.centerSegments=d.centerSegments;if(d.widthProfile)p.data.widthProfile=d.widthProfile;}if(d.fillSeed){p.data.fillSeed=d.fillSeed;p.data.fillGapPx=d.fillGapPx;}if(d.fillWalls)p.data.fillWalls=d.fillWalls;if(d.strokeId)p.data.strokeId=d.strokeId;if(d.brushGroupId)p.data.brushGroupId=d.brushGroupId;if(d.boxAngle)p.data.boxAngle=d.boxAngle;if(d.isBrushTextureCopy)p.data.isBrushTextureCopy=true;if(d.brushTexturePreset)p.data.brushTexturePreset=d.brushTexturePreset;if(d.preTextureOpacity!==undefined)p.data.preTextureOpacity=d.preTextureOpacity;if(d.preTextureStroke!==undefined)p.data.preTextureStroke=d.preTextureStroke;if(d.channelTag)p.data.channelTag=d.channelTag;if(d.ownerId)p.data.ownerId=d.ownerId;if(d.ownerName)p.data.ownerName=d.ownerName;if(d.ownerColor)p.data.ownerColor=d.ownerColor;if(d.revisionParentId)p.data.revisionParentId=d.revisionParentId;if(d.isRevisionGhost)p.data.isRevisionGhost=true;if(d.revisionAction)p.data.revisionAction=d.revisionAction;prev.activate();return p;}
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
}
// Applies an affine matrix to one raw stroke-data dict's geometry in place
// — point gets the full affine, handleIn/handleOut are deltas so only the
// linear (non-translating) part applies to them.
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
function layerInPoint(ld){
  if(ld.inPoint!=null)return ld.inPoint;
  var auto=autoInPointFromBlankKeyframe(ld);
  return auto!=null?auto:0;
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
function layerOutPoint(ld){
  if(ld.outPoint!=null)return ld.outPoint;
  var auto=autoOutPointFromBlankKeyframe(ld);
  return auto!=null?auto:state.totalFrames-1;
}
function getEffectiveStrokes(layerIdx,frameIdx){
  var ld=state.layers[layerIdx];if(!ld)return[];
  if((ld.inPoint||ld.outPoint!=null)&&(frameIdx<layerInPoint(ld)||frameIdx>layerOutPoint(ld)))return[];
  // EXPERIMENTAL (native-video-decode): a natively-decoded video layer has
  // no vector strokes at all — its picture is an engine-side image item
  // (buildSceneJson, engine-bridge.js), not frame data.
  if(ld.nativeVideo)return[];
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
    var ownF=ld.frames[frameIdx];
    if(ownF&&ownF.isKeyframe&&ownF.blankOverride)return[];
    var ii=resolveSymbolFrameIdx(sym,ld,frameIdx);
    // Composite EVERY visible sub-layer of the component, not just the
    // first — a multi-layer component (convertLayersToComponent) used to
    // only ever render sym.layers[0] into the parent scene, silently
    // dropping the rest even though they're all visible and fully editable
    // inside the component itself.
    var out=[];
    sym.layers.forEach(function(symLayer){
      if(!symLayer||symLayer.visible===false)return;
      var sf=symLayer.frames[ii];if(!sf)return;
      if(sf.isKeyframe||sf.isInterpolated){out=out.concat(sf.strokes);return;}
      for(var k=ii-1;k>=0;k--){if(symLayer.frames[k].isKeyframe){out=out.concat(symLayer.frames[k].strokes);break;}}
    });
    // The instance transform (symMatrixOf) — skip entirely (and the clone
    // it requires) when it's identity, the common case. Strokes returned
    // here are live references into the SYMBOL'S OWN stored frame data, so
    // transforming in place would permanently corrupt the symbol itself;
    // every stroke is deep-cloned first.
    if(ld.symMatrix){
      var m=symMatrixOf(ld);
      out=out.map(function(sd){return applyMatrixToStrokeData(JSON.parse(JSON.stringify(sd)),m);});
    }
    return out;
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
  // 'once' = play through the component's own full defined duration exactly
  // once, then hold the last frame — no looping. Default per explicit
  // request (a freshly-made component shouldn't loop by default; the old
  // 'loop' default made a component's parent-timeline keyframe misleadingly
  // look "empty/short" when the component itself kept cycling underneath).
  ld.symbolId=symId;ld.locked=true;ld.symPlayMode='once';ld.symSpeed=1;ld.symPlacedAt=0;ld.symSingleFrame=0;
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
  var newLd={name:'Composant',symbolId:symId,locked:true,visible:true,symPlayMode:'once',symSpeed:1,symPlacedAt:0,symSingleFrame:0,
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
function _collectLayerStrokes(li,ld){
  var strokes=[];
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
function _maybePromoteInterpolated(f,strokes){
  if(f.isInterpolated&&!strokesEqual(strokes,f.strokes)){f.isKeyframe=true;f.isInterpolated=false;}
}
function saveActiveLayerFrame(){
  window._sceneVersion++;
  var ld=state.layers[state.activeLayerIdx];if(ld.symbolId||ld.nativeVideo||ld.montageId)return;
  _writeBackGhostProxies(state.activeLayerIdx);
  var f=ld.frames[state.currentFrame];
  if(!f.isKeyframe&&!f.isInterpolated)return;
  var strokes=_collectLayerStrokes(state.activeLayerIdx,ld);
  _maybePromoteInterpolated(f,strokes);
  f.strokes=strokes;
}
function saveAllLayerFrames(){
  _writeBackGhostProxies(state.activeLayerIdx);
  for(var i=0;i<state.layers.length;i++){if(state.layers[i].symbolId||state.layers[i].nativeVideo||state.layers[i].montageId)continue;var f=state.layers[i].frames[state.currentFrame];if(!f||(!f.isKeyframe&&!f.isInterpolated))continue;
  var strokes=_collectLayerStrokes(i,state.layers[i]);
  _maybePromoteInterpolated(f,strokes);
  f.strokes=strokes;}
}
function loadFrame(idx){
  window._sceneVersion++;
  // Rotoscopy reference follows the playhead (video seek / sequence frame
  // pick) — loadFrame is the one choke point every frame change goes
  // through, scrub and playback alike.
  if(window.SMReference)SMReference.onFrameChanged(idx);
  // EXPERIMENTAL (native-video-decode branch): natively-decoded video
  // LAYERS follow the playhead through the same choke point.
  if(window.SMNativeVideo)SMNativeVideo.onFrameChanged(idx);
  for(var i=0;i<state.layers.length;i++){userLayers[i].removeChildren();if(!layerIsEffectivelyVisible(i))continue;
  // No explicit `op` override here (unlike renderOS()'s onion-skin ghosts,
  // which intentionally force a computed fade-opacity regardless of the
  // object's own value) — omitting it lets desR/desP fall through to the
  // stroke's ACTUAL stored opacity. Passing a hardcoded 1 here silently
  // discarded every real per-object opacity (including tween-interpolated
  // opacity blends) on every normal frame load, and the resulting mismatch
  // against the correctly-stored interpolated value permanently flagged
  // untouched inbetweens as "manually edited" the moment you navigated
  // through them.
  var strokes=getEffectiveStrokes(i,idx);strokes.forEach(function(sd){if(sd.isRaster)desR(sd,userLayers[i]);else desP(sd,userLayers[i]);});relinkBrushCompanions(userLayers[i]);}
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
  pushUndoLayers();
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
  showToast('Calque séparé en Stroke / Fill / Shadow');
}
function goToFrame(idx){
  if(idx<0||idx>=state.totalFrames)return;
  saveAllLayerFrames();state.currentFrame=idx;window._curFrame=idx;
  if(window.SMAudio&&!state.playing)SMAudio.scrubAt(idx); // scrub audio au deplacement du playhead (v19)
  loadFrame(idx);if(!state.playing){renderOS();renderArcs();updateUI();}else{updatePlayhead();}
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
  var targets=(typeof _layerSel!=='undefined'&&_layerSel.length)?_layerSel.slice():state.layers.map(function(_ld,i){return i;});
  pushUndoLayers();
  var cf=state.currentFrame;
  for(var i=0;i<state.layers.length;i++){
    var blank={strokes:[],isKeyframe:false,isInterpolated:false};
    if(targets.indexOf(i)>=0)state.layers[i].frames.splice(cf+1,0,blank);
    else state.layers[i].frames.push(blank);
  }
  state.totalFrames++;
  if(state.waOut<state.totalFrames-1)state.waOut++;
  window._waOut=state.waOut;window._totalF=state.totalFrames;
  goToFrame(cf+1);
  showToast('Frame insérée (F5)'+(targets.length<state.layers.length?' — '+targets.length+' calque(s)':''));
}
// Core per-layer step shared by insertKeyframeAt (single layer+frame, own
// undo/render — used by the span-end drag handle in timeline.js) and
// insertKeyframe below (multi-layer, ONE undo/render for the whole batch).
// No undo/render side effects of its own — callers own that.
function _insertKeyframeCore(layerIdx,frameIdx){
  var ld=state.layers[layerIdx];var f=ld.frames[frameIdx];
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
    if(ld.locked){lockedHit=true;return false;}
    return !ld.frames[cf].isKeyframe;
  });
  if(!eligible.length){showToast(lockedHit?'Calque verrouillé':'Déjà une keyframe');return;}
  pushUndoLayers();
  eligible.forEach(function(li){_insertKeyframeCore(li,cf);});
  loadFrame(cf);renderOS();renderArcs();updateUI();
  showToast(eligible.length>1?eligible.length+' keyframes insérées':'Keyframe insérée');
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
  if(ld.locked){showToast('Calque verrouillé');return false;}
  if(f.isKeyframe){showToast('Déjà une keyframe');return false;}
  pushUndoLayers();
  _insertKeyframeCore(layerIdx,frameIdx);
  if(layerIdx===state.activeLayerIdx)loadFrame(state.currentFrame);
  renderOS();renderArcs();updateUI();showToast('Keyframe insérée');
  return true;
}
function insertBlankKeyframe(){
  var ld=state.layers[state.activeLayerIdx];
  if(ld.locked){showToast('Calque verrouillé');return;}
  saveAllLayerFrames();pushUndoLayers();
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
function removeFrame(){if(state.totalFrames<=1)return;pushUndoLayers();var cf=state.currentFrame;for(var i=0;i<state.layers.length;i++)state.layers[i].frames.splice(cf,1);state.totalFrames--;if(state.waOut>=state.totalFrames)state.waOut=state.totalFrames-1;window._waOut=state.waOut;window._totalF=state.totalFrames;if(state.currentFrame>=state.totalFrames)state.currentFrame=state.totalFrames-1;loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast('Frame supprimée');}
// Animate's "Clear Keyframe" — demotes a keyframe back into a plain
// extended frame (content reverts to whatever the previous keyframe holds),
// without removing the frame slot itself (unlike removeFrame/removeFrameSpan).
function clearKeyframe(){
  var ld=state.layers[state.activeLayerIdx];var cf=state.currentFrame;var f=ld.frames[cf];
  if(ld.locked){showToast('Calque verrouillé');return;}
  if(!f||!f.isKeyframe){showToast('Pas une keyframe');return;}
  pushUndo();f.strokes=[];f.isKeyframe=false;f.isInterpolated=false;
  syncLinkedKeyframeFolder(state.activeLayerIdx,cf);
  loadFrame(cf);renderOS();renderArcs();updateUI();showToast('Keyframe effacée');
}
// Animate's "Convert to Keyframes" — bakes whatever's currently displayed
// (a tweened inbetween, or content inherited from an earlier keyframe) into
// an independent real keyframe on every frame in the selection (or just the
// current frame if nothing's selected), so it can be hand-adjusted without
// disturbing the rest of the tween.
function convertToKeyframes(){
  var li=state.activeLayerIdx;var ld=state.layers[li];
  if(ld.locked){showToast('Calque verrouillé');return;}
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

