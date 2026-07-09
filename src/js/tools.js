// ---- TOOLS ----
var currentPath=null,selectedPaths=[],stabQueue=[],shapeStart=null;
var _moveDragStarted=false;
var _eraseDragActive=false;
var _eraseLastPt=null;
// Tracks the last component-layer click for double-click-to-enter timing,
// independent of the Pen tool's own _pen.lastClickTime (separate tools).
var _compClick={layerIdx:-1,time:0};
// Component instances behave like Animate's graphic-symbol objects: clicking
// one should select it as a whole regardless of which layer is "active" in
// the panel. Only scans layers flagged .symbolId (never a normal drawing
// layer — those stay restricted to explicit layer-switching, unchanged).
function hitTestComponentLayers(point){
  for(var i=state.layers.length-1;i>=0;i--){
    var ld=state.layers[i];
    // Component layers are always .locked (it blocks hand-editing the baked
    // strokes, since they're a resolved projection of the symbol's own
    // timeline) — that's unrelated to whether the instance itself should be
    // clickable/selectable, so locked is NOT checked here on purpose.
    if(!ld.symbolId||!ld.visible)continue;
    var hit=userLayers[i].hitTest(point,{stroke:true,fill:true,tolerance:8/view.zoom});
    if(hit)return{layerIdx:i,item:hit.item};
  }
  return null;
}

// ---- FILL/STROKE SELECT (v18) ----
// Animate's classic merge-drawing model treats a shape's fill and its
// stroke as independently selectable/editable things, and a stroke
// automatically "breaks" into separately-selectable segments wherever it
// crosses another stroke (its own or a different path's). This app's data
// model doesn't actually merge/split geometry live like Animate does — a
// drawn shape stays ONE Path with both a fillColor and a strokeColor — so
// this tool fakes the same UX on top of that: clicking a fill selects just
// state.fillColor's side of that one path, clicking a stroke selects just
// its strokeColor side, and clicking a stroke between two crossings
// computes those crossing points live (Paper's getIntersections) and
// scopes the selection (and, on Delete, an ACTUAL path split) to just that
// bounded arc — the rest of the stroke and any fill are left untouched.
var _fsSel=null; // {path, kind:'fill'|'stroke', segStart, segEnd, closed} | null
function fsClearSel(){_fsSel=null;}
// Every OTHER path in the same layer, plus this path's own self-crossings
// — each contributes its crossing points as offsets (0..path.length)
// along `path`'s own parametrization.
function fsIntersectionOffsets(path,layer){
  var offsets=[];
  try{
    (path.getIntersections()||[]).forEach(function(loc){offsets.push(loc.offset);});
  }catch(e){}
  layer.children.forEach(function(other){
    if(other===path||!(other instanceof Path)||other.segments.length<2)return;
    if(!(other.strokeColor||other.data&&other.data.isVectorBrush))return; // only strokes act as "cutting" edges, matching Animate (a bare fill never splits a stroke)
    try{
      (path.getIntersections(other)||[]).forEach(function(loc){offsets.push(loc.offset);});
    }catch(e){}
  });
  offsets.sort(function(a,b){return a-b;});
  // de-dupe near-identical offsets (two curves crossing exactly at a shared
  // anchor point report the same crossing twice)
  var out=[];
  offsets.forEach(function(o){if(!out.length||o-out[out.length-1]>0.01)out.push(o);});
  return out;
}
// Given the full sorted crossing-offset list and where the user actually
// clicked, returns the {segStart,segEnd} pair bracketing the click — or
// the whole path (0..length) when there are no crossings to bound it.
function fsSegmentBounds(path,clickPt,offsets){
  var len=path.length;
  if(!offsets.length)return{segStart:0,segEnd:len};
  var loc=path.getNearestLocation(clickPt);
  var co=loc?loc.offset:0;
  var start=0,end=len;
  for(var i=0;i<offsets.length;i++){
    if(offsets[i]<=co)start=offsets[i];
    if(offsets[i]>=co){end=offsets[i];break;}
  }
  if(path.closed&&start===0&&end===len&&offsets.length){
    // click landed before the first / after the last crossing on a CLOSED
    // path — that arc actually wraps around through offset 0, not the
    // open 0..len span every other bracket uses.
    start=offsets[offsets.length-1];end=offsets[0];
  }
  return{segStart:start,segEnd:end};
}
// Non-destructive: returns a standalone, non-inserted open Path tracing
// `path` from segStart to segEnd (wrapping through 0 if closed and
// segEnd<segStart). Shared by fsHighlightPath (arc-only highlight) and the
// fill-region builder below (boundary arcs) — same splitAt-clone technique
// fsDeleteSegment uses for the real split, just discarding the pieces
// instead of committing them.
function fsExtractArc(path,segStart,segEnd){
  var whole=(segStart===0&&segEnd===path.length&&!(path.closed&&segEnd<segStart));
  if(whole)return path.clone({insert:false,deep:false});
  var clone=path.clone({insert:true,deep:false}); // splitAt requires the item to be inserted in a project
  var arc;
  if(clone.closed){
    var loopLen=clone.length;
    clone.splitAt(clone.getLocationAt(segStart));
    var newEndOffset=((segEnd-segStart)+loopLen)%loopLen;
    if(newEndOffset<=0.001)newEndOffset=loopLen;
    var remainder=clone.splitAt(clone.getLocationAt(newEndOffset));
    arc=clone;
    if(remainder)remainder.remove();
  }else{
    var tail=clone.splitAt(clone.getLocationAt(segEnd));
    if(tail)tail.remove();
    arc=clone.splitAt(clone.getLocationAt(segStart));
    if(arc)clone.remove();
  }
  var result=(arc||clone).clone({insert:false,deep:false});
  if(clone&&clone.remove)clone.remove();
  if(arc&&arc!==clone&&arc.remove)arc.remove();
  return result;
}
// Animate divides a fill wherever a stroke crosses THROUGH it (not just
// touches its edge), so clicking either side of the crossing stroke selects
// only that side. This app's fill is still one Path underneath — so, like
// the stroke-segment case above, this computes the divided sub-region live
// (never mutates on a plain click) by walling off the clicked side with the
// cutting stroke's own chord: boundary arc of the fill from one crossing to
// the other, plus the cutter's chord between those same two points, joined
// into a closed region. Only handles the simple single-clean-crossing case
// (exactly 2 intersections with one open cutter) — multiple cutters or a
// cutter that weaves in/out more than once falls back to whole-fill
// selection (a real planar-subdivision engine is out of scope here, see
// CLAUDE.md's fragility notes on the rendering pipeline).
function fsBuildFillRegion(fillPath,boundaryStart,boundaryEnd,cutter,cutterA,cutterB){
  var arc=fsExtractArc(fillPath,boundaryStart,boundaryEnd);
  var lo=Math.min(cutterA,cutterB),hi=Math.max(cutterA,cutterB);
  var chord=fsExtractArc(cutter,lo,hi);
  if(!arc.segments.length||!chord.segments.length){arc.remove();chord.remove();return null;}
  // orient the chord so it continues from the arc's end point (join()
  // connects end-to-start; the raw chord's start/end could be either way).
  var dEnd=arc.lastSegment.point.getDistance(chord.firstSegment.point);
  var dEndRev=arc.lastSegment.point.getDistance(chord.lastSegment.point);
  if(dEndRev<dEnd)chord.reverse();
  arc.join(chord,4); // tolerance in world px — closes the loop when the chord's far end meets the arc's start
  if(!arc.closed){
    if(arc.firstSegment.point.getDistance(arc.lastSegment.point)<4)arc.closePath();
    else{arc.remove();return null;} // ends didn't meet — bail rather than fake a wrong shape
  }
  return arc;
}
function fsFindFillRegion(fillPath,pt,layer){
  for(var i=0;i<layer.children.length;i++){
    var c=layer.children[i];
    if(c===fillPath||!(c instanceof Path)||c.closed||!c.strokeColor||c.segments.length<2)continue;
    var ix;
    try{ix=fillPath.getIntersections(c);}catch(e){continue;}
    if(!ix||ix.length!==2)continue; // only the simple single-chord case
    var o1=ix[0].offset,o2=ix[1].offset;
    var co1=ix[0].intersection.offset,co2=ix[1].intersection.offset;
    if(o1>o2){var t=o1;o1=o2;o2=t;t=co1;co1=co2;co2=t;}
    var regionA=fsBuildFillRegion(fillPath,o1,o2,c,co1,co2);
    var regionB=fsBuildFillRegion(fillPath,o2,o1,c,co1,co2);
    if(!regionA||!regionB){if(regionA)regionA.remove();if(regionB)regionB.remove();continue;}
    var chosen=regionA.contains(pt)?regionA:(regionB.contains(pt)?regionB:null);
    if(!chosen){regionA.remove();regionB.remove();continue;}
    var other=chosen===regionA?regionB:regionA;
    other.remove();
    return{regionPath:chosen,cutter:c,boundaryStart:(chosen===regionA?o1:o2),boundaryEnd:(chosen===regionA?o2:o1),cutterA:co1,cutterB:co2};
  }
  return null;
}
// DESTRUCTIVE: turns a still-virtual 'fillregion' selection into two real,
// independent Path objects (matching Animate — recoloring/deleting one side
// of a divided fill genuinely separates it), inserts both at the original
// fillPath's stacking position, and returns the one the user actually
// clicked as a plain 'fill' selection so the caller can apply its edit
// exactly like an ordinary whole-fill selection.
function fsRealizeFillRegion(sel,layer){
  var fillPath=sel.path;
  var idx=layer.children.indexOf(fillPath);
  var a=fsExtractArc(fillPath,sel.boundaryStart,sel.boundaryEnd);
  var chordA=fsExtractArc(sel.cutter,Math.min(sel.cutterA,sel.cutterB),Math.max(sel.cutterA,sel.cutterB));
  var b=fsExtractArc(fillPath,sel.boundaryEnd,sel.boundaryStart);
  var chordB=chordA.clone({insert:false,deep:false});
  [[a,chordA],[b,chordB]].forEach(function(pair){
    var arc=pair[0],chord=pair[1];
    var dEnd=arc.lastSegment.point.getDistance(chord.firstSegment.point);
    var dEndRev=arc.lastSegment.point.getDistance(chord.lastSegment.point);
    if(dEndRev<dEnd)chord.reverse();
    arc.join(chord,4);
    if(!arc.closed&&arc.firstSegment.point.getDistance(arc.lastSegment.point)<4)arc.closePath();
  });
  a.fillColor=fillPath.fillColor;a.strokeColor=fillPath.strokeColor;a.strokeWidth=fillPath.strokeWidth;
  b.fillColor=fillPath.fillColor;b.strokeColor=fillPath.strokeColor;b.strokeWidth=fillPath.strokeWidth;
  // fsExtractArc clones fillPath internally, and Paper's clone() copies
  // .data along with it — if fillPath was a paint-bucket fill (fillSeed/
  // fillWalls/fillGapPx), BOTH split pieces would otherwise inherit that
  // stale tracking, still pointing at the ORIGINAL undivided seed+walls.
  // The next fillRegenerateLinked pass (any wall move) would then re-trace
  // straight past the cutter — which was never added to fillWalls — and
  // regenerate the full original undivided shape, silently undoing the
  // split. A divided fill is definitionally no longer that single bucket
  // region, on EITHER side, so both pieces must be fully disconnected from
  // auto-regen here, not just whichever one later gets deleted.
  fsUnlinkFillRegen(a);fsUnlinkFillRegen(b);
  layer.insertChild(idx,a);layer.insertChild(idx,b);
  fillPath.remove();
  return{path:a,kind:'fill'};
}
function fsHitTest(pt,layer){
  var strokeHit=layer.hitTest(pt,{stroke:true,tolerance:8/view.zoom});
  if(strokeHit&&strokeHit.item instanceof Path&&strokeHit.item.strokeColor){
    var sp=strokeHit.item;
    var offs=fsIntersectionOffsets(sp,layer);
    var seg=fsSegmentBounds(sp,pt,offs);
    return{path:sp,kind:'stroke',segStart:seg.segStart,segEnd:seg.segEnd,closed:sp.closed};
  }
  var fillHit=layer.hitTest(pt,{fill:true,tolerance:0});
  if(fillHit&&fillHit.item instanceof Path&&fillHit.item.fillColor){
    // A wide/scattered/fanned brush texture (esp. tipShape:'bristle' — dabs
    // deliberately spread well past the anchor's own stroke tolerance) can
    // put the click on an individual dab's fill rather than the anchor's
    // own geometry. Dabs are disposable rendering detail, not something the
    // user should be able to select/recolor/delete one-at-a-time here —
    // resolve back to the anchor exactly like subselect already does
    // (resolveBrushAnchor, used by the node-edit tool) so Fill/Stroke
    // Select always operates on "the brush stroke", not "the fleck I
    // happened to click".
    var fp=resolveBrushAnchor(fillHit.item,layer);
    if(fp!==fillHit.item){
      // The anchor of a stroke-only (no-fill) textured brush has no real
      // fillColor of its own (opacity:0 invisible-anchor trick) — a "fill"
      // selection on it would be meaningless. Re-run as a STROKE hit
      // against the anchor's own geometry instead, which is what the user
      // actually meant to select.
      if(!fp.fillColor&&fp.strokeColor){
        var offs2=fsIntersectionOffsets(fp,layer);
        var seg2=fsSegmentBounds(fp,pt,offs2);
        return{path:fp,kind:'stroke',segStart:seg2.segStart,segEnd:seg2.segEnd,closed:fp.closed};
      }
    }else{
      var region=fsFindFillRegion(fp,pt,layer);
      if(region){region.regionPath.remove();return{path:fp,kind:'fillregion',boundaryStart:region.boundaryStart,boundaryEnd:region.boundaryEnd,cutter:region.cutter,cutterA:region.cutterA,cutterB:region.cutterB};}
    }
    return{path:fp,kind:'fill'};
  }
  return null;
}
// Non-destructive: builds a standalone Path tracing just the selected arc,
// for the render-overlay highlight — operates on a throwaway clone, never
// touches the real path. Mirrors fsDeleteSegment's splitAt logic exactly
// (see that function's comment for the semantics), just keeping the arc
// piece here instead of discarding it.
function fsHighlightPath(sel){
  if(!sel)return null;
  if(sel.kind==='fill')return sel.path.clone({insert:false,deep:false});
  if(sel.kind==='fillregion'){
    var region=fsBuildFillRegion(sel.path,sel.boundaryStart,sel.boundaryEnd,sel.cutter,sel.cutterA,sel.cutterB);
    if(!region)return sel.path.clone({insert:false,deep:false}); // cutter/geometry changed since selection — fall back to whole fill
    var out=region.clone({insert:false,deep:false});
    region.remove();
    return out;
  }
  return fsExtractArc(sel.path,sel.segStart,sel.segEnd);
}
// DESTRUCTIVE: isolates the selected arc as its own Path and removes it,
// leaving the remainder of the stroke (and any fill — Paper.js copies
// style to both sides of a split, so the kept remainder keeps its own
// fillColor untouched) in the layer.
//
// Path#splitAt(location) semantics this relies on (Paper.js): on an OPEN
// path it MUTATES the receiver into [start..splitPoint] and RETURNS a new
// path [splitPoint..end]. On a CLOSED path it returns null and instead
// re-bases the path to start AT that location (same loop, same length,
// still closed) — the first splitAt on a closed path never removes
// anything by itself, it just picks a new "seam".
// Top-level entry point for the Delete key while this tool is active —
// dispatches to the fill-only or stroke/segment path, then clears the
// (now possibly stale, geometry-mutated) selection.
// A fill manually touched through this tool (deleted, recolored, toggled
// off) must STAY that way — Animate has no such thing as a fill spontaneously
// reappearing because a nearby stroke moved. This app's paint-bucket fills
// DO have that magic (data.fillSeed/fillWalls, see fillRegenerateLinked in
// this file), which is exactly backwards for anything the user just hand-
// edited here: if the touched path survives (kept for its strokeColor) with
// its old fillSeed/fillWalls still attached, the next stroke-move regen pass
// would silently repaint a fill right back over the user's own edit. Strip
// the tracking unconditionally so a manual edit always wins and stays put.
function fsUnlinkFillRegen(p){
  if(!p||!p.data)return;
  delete p.data.fillSeed;delete p.data.fillWalls;delete p.data.fillGapPx;
}
function fsApplyDelete(){
  if(!_fsSel)return;
  pushUndo();
  var layer=userLayers[state.activeLayerIdx];
  if(_fsSel.kind==='fillregion')_fsSel=fsRealizeFillRegion(_fsSel,layer);
  if(_fsSel.kind==='fill'){
    var p=_fsSel.path;
    fsUnlinkFillRegen(p);
    if(!markDeleteAsRevision(p)){ // foreign-owned fill: ghosted in place, keep its color so the ghost still reads correctly
      p.fillColor=null;
      if(!p.strokeColor)p.remove();
    }
    saveActiveLayerFrame();
  }else{
    fsDeleteSegment(_fsSel,layer);
  }
  fsClearSel();
  renderArcs();updateUI();
}
function fsDeleteSegment(sel,layer){
  var path=sel.path;
  if(sel.segStart===0&&sel.segEnd===path.length&&!(path.closed&&sel.segEnd<sel.segStart)){
    // whole stroke, no crossings — same as nulling strokeColor on the
    // combined path (matches Animate deleting an un-crossed stroke)
    if(!markDeleteAsRevision(path)){ // foreign-owned: ghosted in place instead
      path.strokeColor=null;
      if(!path.fillColor)path.remove();
    }
    saveActiveLayerFrame();
    return;
  }
  if(path.closed){
    var loopLen=path.length;
    path.splitAt(path.getLocationAt(sel.segStart)); // re-bases the seam to segStart; still closed, still one object
    var newEndOffset=((sel.segEnd-sel.segStart)+loopLen)%loopLen;
    if(newEndOffset<=0.001)newEndOffset=loopLen; // selected arc is the WHOLE (now-open) loop
    var remainder=path.splitAt(path.getLocationAt(newEndOffset)); // now open (re-based+split) -> real split: path=[segStart..segEnd] (the arc), remainder=[segEnd..segStart] (the kept tail)
    path.remove(); // `path` is the selected arc post-split — discard it
    // `remainder` (if any) is the kept, now-open remainder of the stroke —
    // already in the layer, nothing further to do.
  }else{
    var tail=path.splitAt(path.getLocationAt(sel.segEnd)); // path=[start..segEnd] (kept head + arc), tail=[segEnd..originalEnd] (kept tail)
    if(tail&&tail.length<0.001)tail.remove(); // segEnd was the original end -> "tail" is degenerate, drop it
    var arc=path.splitAt(path.getLocationAt(sel.segStart)); // path=[start..segStart] (kept head), arc=[segStart..segEnd] (the selected arc)
    if(arc)arc.remove();
    if(path.length<0.001)path.remove(); // segStart was 0 -> "head" is degenerate, drop it
  }
  saveActiveLayerFrame();
}

// ---- NODE / TANGENT EDIT (select tool, single-path selection) ----
// Circles = anchor points, squares = bezier tangent handles (connected by a
// thin guide line), exactly like Illustrator's Direct Selection. For
// centerline-driven variable-width strokes, the handles shown/edited are
// the *centerline* control points (data.centerSegments) — dragging one
// regenerates the whole visible outline, since the center drives the rest.
var nodeHandles=[];
var _nodeDrag={active:false,path:null,type:null,segIndex:-1};
var _marquee={active:false,start:null,rect:null};
// Multi-point selection for the Subselection tool: marquee over anchor
// points collects their segment indexes here; dragging any selected anchor
// then moves the whole set together (Animate's white-arrow behavior).
var _nodeSel=[];
var _nmq={active:false,start:null,rect:null};
// Node/tangent handles belong to the dedicated Subselection tool (white
// arrow, A) — the main Select tool (black arrow, V) owns move + the
// transform gizmo instead, mirroring Animate's two-arrow split so the two
// kinds of handles never fight over the same click.
// A brush-texture dab (data.isBrushTextureCopy) is what's actually visible
// and thus what hitTest lands on — but it's a tiny disposable stamp, not
// the real editable path. Its underlying anchor is USUALLY fully invisible
// (opacity 0, see applyBrushTexture in this file) so a click can never land
// on the anchor directly; this resolves a dab hit back to its real anchor
// (same layer, matching data.brushGroupId, itself NOT a dab) so subselect
// node-editing (tangents, points) operates on the real path instead of
// silently editing a disposable dab that gets regenerated on the next
// stroke edit anyway.
function resolveBrushAnchor(item,layer){
  if(!item||!item.data||!item.data.isBrushTextureCopy||!item.data.brushGroupId)return item;
  var gid=item.data.brushGroupId;
  for(var i=0;i<layer.children.length;i++){
    var c=layer.children[i];
    if(c.data&&c.data.brushGroupId===gid&&!c.data.isBrushTextureCopy)return c;
  }
  return item;
}
function nodeEditTargetPath(){
  if(state.tool!=='subselect'||selectedPaths.length!==1)return null;
  var p=selectedPaths[0];
  // CompoundPath (e.g. the result of subtract/exclude, which can produce
  // disjoint sub-loops) has no direct .segments — anchor editing on those
  // isn't supported, so bail out rather than crashing nodeEditSegmentsData.
  if(!p.segments)return null;
  return p;
}
function nodeEditSegmentsData(path){
  if(path.data&&path.data.isVectorBrush&&path.data.centerSegments)return path.data.centerSegments;
  return path.segments.map(function(s){return{point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};});
}
function renderNodeHandles(){
  nodeLayer.removeChildren();nodeHandles=[];
  var path=nodeEditTargetPath();if(!path)return;
  var segs=nodeEditSegmentsData(path);
  nodeLayer.activate();var zs=1/view.zoom;
  segs.forEach(function(s,i){
    var pt=new Point(s.point[0],s.point[1]);
    var hi=new Point(s.handleIn[0],s.handleIn[1]),ho=new Point(s.handleOut[0],s.handleOut[1]);
    if(hi.length>0.5){
      var hiPt=pt.add(hi);
      new Path.Line({from:pt,to:hiPt,strokeColor:'rgba(120,170,255,.7)',strokeWidth:1*zs,insert:true});
      var hd=new Path.Rectangle({center:hiPt,size:[6*zs,6*zs],fillColor:'#ffffff',strokeColor:'#4a9eff',strokeWidth:1*zs,insert:true});
      nodeHandles.push({type:'handleIn',segIndex:i,dot:hd,pos:hiPt});
    }
    if(ho.length>0.5){
      var hoPt=pt.add(ho);
      new Path.Line({from:pt,to:hoPt,strokeColor:'rgba(120,170,255,.7)',strokeWidth:1*zs,insert:true});
      var hd2=new Path.Rectangle({center:hoPt,size:[6*zs,6*zs],fillColor:'#ffffff',strokeColor:'#4a9eff',strokeWidth:1*zs,insert:true});
      nodeHandles.push({type:'handleOut',segIndex:i,dot:hd2,pos:hoPt});
    }
    var isSel=_nodeSel.indexOf(i)>=0;
    var ad=new Path.Circle({center:pt,radius:(isSel?5:4)*zs,fillColor:isSel?'#ffb86c':'#4a9eff',strokeColor:'#ffffff',strokeWidth:1*zs,insert:true});
    nodeHandles.push({type:'point',segIndex:i,dot:ad,pos:pt});
  });
  userLayers[state.activeLayerIdx].activate();
}

// ---- TRANSFORM (scale/rotate the current selection as a group) ----
// Shown for any non-empty selection alongside node handles (node handles
// edit a single path's own points; these handle the selection's bounding
// box as a whole — scale from 8 box handles, rotate from the handle above
// the box, exactly like Illustrator/Animate's free-transform box).
var xformHandles=[];
var _xform={active:false,type:null,dir:null,anchor:null,center:null,origHandlePos:null,lastSx:1,lastSy:1,startAngle:0,lastAngle:0};
function xformSelBounds(){
  if(!selectedPaths.length)return null;
  var b=null;
  selectedPaths.forEach(function(p){b=b?b.unite(p.bounds):p.bounds.clone();});
  return b;
}
// Align toolbar (redesign 2026-07-09) — each selected path moves by its OWN
// delta against the combined selection bounds (not a uniform group shift
// like selPropsApplyMove), so this can't reuse that function; the per-path
// translate block below is deliberately identical to it though (see its own
// comment) — same companion objects (vector-brush centerline, linkedFill
// backdrop, brush-texture dabs) need the same translate to avoid the
// "parallax drift" bug already fixed once this session.
function alignSelection(mode){
  if(selectedPaths.length<2)return;
  var b=xformSelBounds();if(!b)return;
  pushUndo();
  var moved=false;
  selectedPaths.forEach(function(p){
    var pb=p.bounds,dx=0,dy=0;
    if(mode==='left')dx=b.left-pb.left;
    else if(mode==='right')dx=b.right-pb.right;
    else if(mode==='centerH')dx=b.center.x-pb.center.x;
    else if(mode==='top')dy=b.top-pb.top;
    else if(mode==='bottom')dy=b.bottom-pb.bottom;
    else if(mode==='centerV')dy=b.center.y-pb.center.y;
    if(Math.abs(dx)<1e-6&&Math.abs(dy)<1e-6)return;
    moved=true;
    var d=new Point(dx,dy);
    p.translate(d);
    if(p.data&&p.data.isVectorBrush&&p.data.centerSegments)p.data.centerSegments.forEach(function(s){s.point=[s.point[0]+dx,s.point[1]+dy];});
    if(p.data&&p.data.linkedFill&&!p.data.linkedFill.removed)p.data.linkedFill.translate(d);
    if(p.data&&p.data.brushCompanions)p.data.brushCompanions.forEach(function(c){if(!c.removed)c.translate(d);});
  });
  if(!moved){state.undoStack.pop();return;}
  fillRegenerateLinked(userLayers[state.activeLayerIdx],null);
  saveActiveLayerFrame();renderArcs();updateUI();
  if(window.SMEngineBridge)SMEngineBridge.renderNow();
}
// Rotation/scale pivot picker (redesign 2026-07-09, AE-style 9-dot anchor
// widget) — a TOOL preference like state.tool, not document content: it
// resets to center on a fresh selection rather than being saved per-object,
// since this app's transform model doesn't carry a persistent per-path
// anchor offset the way After Effects' layers do. Paper.js Rectangle
// already exposes all 9 named points as getters, so no geometry math needed
// here — just the key->getter-name map both select-bridge.js (drag-rotate)
// and timeline.js (numeric Rotate field) read from.
var XFORM_ANCHOR_PROP={tl:'topLeft',tc:'topCenter',tr:'topRight',ml:'leftCenter',mc:'center',mr:'rightCenter',bl:'bottomLeft',bc:'bottomCenter',br:'bottomRight'};
function xformAnchorPoint(b){return b[XFORM_ANCHOR_PROP[state.xformAnchorKey]||'center'];}
function renderTransformHandles(){
  xformLayer.removeChildren();xformHandles=[];
  if(state.tool!=='select'||!selectedPaths.length)return;
  var b=xformSelBounds();if(!b)return;
  xformLayer.activate();var zs=1/view.zoom;
  new Path.Rectangle({rectangle:b,strokeColor:'rgba(74,158,255,.8)',strokeWidth:1*zs,dashArray:[4*zs,3*zs],insert:true});
  var corners={nw:b.topLeft,ne:b.topRight,sw:b.bottomLeft,se:b.bottomRight,n:b.topCenter,s:b.bottomCenter,e:b.rightCenter,w:b.leftCenter};
  Object.keys(corners).forEach(function(k){
    var pos=corners[k];
    new Path.Rectangle({center:pos,size:[7*zs,7*zs],fillColor:'#ffffff',strokeColor:'#4a9eff',strokeWidth:1.2*zs,insert:true});
    xformHandles.push({type:'scale',dir:k,pos:pos});
  });
  var rotOff=20*zs;
  var rotPos=b.topCenter.subtract(new Point(0,rotOff));
  new Path.Line({from:b.topCenter,to:rotPos,strokeColor:'rgba(74,158,255,.8)',strokeWidth:1*zs,insert:true});
  new Path.Circle({center:rotPos,radius:5*zs,fillColor:'#ffffff',strokeColor:'#4a9eff',strokeWidth:1.2*zs,insert:true});
  xformHandles.push({type:'rotate',pos:rotPos});
  userLayers[state.activeLayerIdx].activate();
}
function scaleCenterSegments(segs,sx,sy,cx,cy){
  segs.forEach(function(s){
    s.point=[cx+(s.point[0]-cx)*sx,cy+(s.point[1]-cy)*sy];
    s.handleIn=[s.handleIn[0]*sx,s.handleIn[1]*sy];
    s.handleOut=[s.handleOut[0]*sx,s.handleOut[1]*sy];
    if(s.width!==undefined)s.width=s.width*((Math.abs(sx)+Math.abs(sy))/2);
  });
}
function rotateCenterSegments(segs,angleDeg,cx,cy){
  var rad=angleDeg*Math.PI/180,cos=Math.cos(rad),sin=Math.sin(rad);
  function rotVec(x,y){return[x*cos-y*sin,x*sin+y*cos];}
  segs.forEach(function(s){
    var r=rotVec(s.point[0]-cx,s.point[1]-cy);
    s.point=[cx+r[0],cy+r[1]];
    s.handleIn=rotVec(s.handleIn[0],s.handleIn[1]);
    s.handleOut=rotVec(s.handleOut[0],s.handleOut[1]);
  });
}
function clearSel(){selectedPaths=[];state.selectedStrokeIndices=[];_nodeSel=[];}
function getSI(path){var ch=userLayers[state.activeLayerIdx].children;for(var i=0;i<ch.length;i++){if(ch[i]===path)return i;}return -1;}
var canvasEl=document.getElementById('drawing-canvas');

// ---- STYLUS PRESSURE (Pointer Events) ----
// Paper.js only relays plain mouse events, whose .pressure reads a constant
// 0.5 with most tablet drivers (XP-Pen, Huion, Wacom in mouse mode). Raw
// pointer events on the same canvas carry the true stylus pressure for
// every brand implementing the W3C Pointer Events spec, so the freshest
// sample is captured here and vbPressureOf() prefers it over everything
// else. touch-action:none stops the webview from hijacking pen gestures.
var _stylus={pressure:0,isPen:false,force:0,forceT:0};
function updPressureUI(v){var f=document.getElementById('p-live-fill');if(f)f.style.width=Math.round(Math.min(1,v)*100)+'%';}
canvasEl.style.touchAction='none';
['pointerdown','pointermove'].forEach(function(evt){
  canvasEl.addEventListener(evt,function(e){
    if(e.pointerType==='pen'){_stylus.isPen=true;_stylus.pressure=e.pressure;updPressureUI(e.pressure);}
    else if(e.pointerType==='mouse')_stylus.isPen=false;
  },{passive:true});
});
canvasEl.addEventListener('pointerup',function(e){if(e.pointerType==='pen'){_stylus.pressure=0;updPressureUI(0);}},{passive:true});
canvasEl.addEventListener('pointercancel',function(){_stylus.pressure=0;updPressureUI(0);},{passive:true});
// macOS/WKWebView second channel: most tablet drivers (XP-Pen, Huion…)
// inject synthesized *mouse* events whose NSEvent pressure WebKit surfaces
// as MouseEvent.webkitForce — NOT as a pointer-event pen. Sampling it here
// covers those drivers; values are 0..1 for tablets (Force Touch trackpads
// can exceed 1, hence the clamp).
['mousemove','mousedown'].forEach(function(evt){
  canvasEl.addEventListener(evt,function(e){
    var f=e.webkitForce;
    if(typeof f==='number'&&f>0){_stylus.force=Math.min(1,f);_stylus.forceT=Date.now();updPressureUI(_stylus.force);}
  },{passive:true});
});
canvasEl.addEventListener('webkitmouseforcechanged',function(e){
  var f=e.webkitForce;
  if(typeof f==='number'&&f>0){_stylus.force=Math.min(1,f);_stylus.forceT=Date.now();updPressureUI(_stylus.force);}
},{passive:true});
canvasEl.addEventListener('mouseup',function(){_stylus.force=0;},{passive:true});
// Third channel — native AppKit pressure streamed from the Rust side
// ('stylus-pressure' Tauri events). This is the channel that actually works
// with drivers whose pressure never reaches the webview (XP-Pen et al.);
// it feeds the same force/forceT slot with the freshest sample winning.
if(window.__TAURI__&&window.__TAURI__.event&&window.__TAURI__.event.listen){
  window.__TAURI__.event.listen('stylus-pressure',function(e){
    var p=e.payload;
    if(typeof p==='number'&&p>0){_stylus.force=Math.min(1,p);_stylus.forceT=Date.now();updPressureUI(_stylus.force);}
  });
}

// THE choke point for "a new keyframe appears" during normal drawing —
// every Draw/Pen/Shape/Fillbrush commit calls this first. A Stroke/Fill/
// Shadow-linked layer (convertLayerToStrokeFillShadowFolder, app.js) needs
// syncLinkedKeyframeFolder here too, not just on the explicit F6/insert-
// keyframe path (insertKeyframeAt) — without it, simply drawing a first
// stroke on the "XXX Stroke" channel at a plain frame never propagated the
// new keyframe to "XXX Fill"/"XXX Shadow" at all, breaking the "keyframes
// partagées" premise of the split for the single most common way a
// keyframe actually gets created.
function ensureKeyframe(){var curF=state.layers[state.activeLayerIdx].frames[state.currentFrame];if(!curF.isKeyframe&&!curF.isInterpolated){curF.isKeyframe=true;curF.strokes=JSON.parse(JSON.stringify(getEffectiveStrokes(state.activeLayerIdx,state.currentFrame)));loadFrame(state.currentFrame);syncLinkedKeyframeFolder(state.activeLayerIdx,state.currentFrame);}}

// ---- VECTOR FILL ENGINE ----
// The fill of an area IS just the closed loop formed by the strokes around
// it, plus a straight bridge wherever two nearby stroke ends don't quite
// touch — that's the whole model. No rasterizing, no pixels: every wall
// keeps its real bezier geometry, and the loop that encloses the click is
// built directly from the actual stroke segments + short bridge lines
// across genuine gaps, exactly like the reference sketch (traced strokes
// with the gap-bridges marked) this was calibrated against.
var _fill={active:false,clickPt:null};
// The wall used for graph/endpoint purposes: a pressure-brush stroke's
// on-screen shape is its filled outline (a closed band with no real
// endpoints to bridge), so its *centerline* stands in for it instead —
// that's the actual line the artist drew.
function fillWallPath(p){
  if(p.data&&p.data.isVectorBrush&&p.data.centerSegments&&p.data.centerSegments.length>1){
    var cl=new Path({insert:false});
    p.data.centerSegments.forEach(function(s){cl.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
    return cl;
  }
  return p.clone({insert:false});
}
// Stable per-stroke identity, assigned lazily the first time a stroke is
// considered as a fill wall and persisted through save/load (serP/desP) —
// this is what lets a fill record WHICH strokes bound it (data.fillWalls)
// rather than only where it was clicked (data.fillSeed).
var _strokeIdCounter=0;
function ensureStrokeId(p){
  if(!p.data)p.data={};
  if(!p.data.strokeId)p.data.strokeId='s'+(Date.now().toString(36))+'_'+(++_strokeIdCounter)+'_'+Math.floor(Math.random()*1e6);
  return p.data.strokeId;
}
// Team review (Phase 1): stamps a freshly-committed stroke with who drew it
// — denormalized name alongside the id so a machine that's never seen this
// profile before can still show a readable label (see forkIfForeignOwner
// for the other half: what happens when a DIFFERENT profile later edits
// this stroke). Also ensureStrokeId's the path: strokeId used to only get
// assigned lazily by fill-wall tracking, so most ordinary strokes never had
// one — forkIfForeignOwner needs a STABLE id from the moment of creation to
// find this exact stroke's pre-edit snapshot later (see
// findPreEditStrokeData); without it every fork silently fell back to
// cloning whatever the path's CURRENT — already mutated by the very edit
// being forked — geometry happened to be, producing a ghost with the wrong
// (post-edit) shape instead of the original.
function tagOwner(p){
  if(!p||!p.data||!state.userProfile)return;
  p.data.ownerId=state.userProfile.id;
  p.data.ownerName=state.userProfile.name;
  // Captured at authorship time, not resolved later — this machine may
  // never see the authoring profile again (no central identity server in
  // Phase 1), so the color has to travel WITH the stroke to render its
  // revision outline correctly.
  p.data.ownerColor=state.userProfile.color;
  ensureStrokeId(p);
}
// Locates a stroke's data as it was BEFORE the in-progress gesture, from
// the undo snapshot pushUndo() (tweens.js) already took at drag-start —
// necessary because Paper.js mutates the live Path in place on every
// pointermove, so by the time a drag commits at onUp, the path's CURRENT
// geometry already IS the edit; there's no "before" left to clone. The
// snapshot is a plain serP-shaped object per stroke (JSON, not a live
// Path), matched by the same strokeId ensureStrokeId hands out.
function findPreEditStrokeData(strokeId){
  var snap=state.undoStack[state.undoStack.length-1];
  if(!snap||snap.type!=='layers')return null;
  for(var li=0;li<snap.layers.length;li++){
    var frame=snap.layers[li].frames&&snap.layers[li].frames[state.currentFrame];
    if(!frame||!frame.strokes)continue;
    for(var i=0;i<frame.strokes.length;i++){
      if(frame.strokes[i].strokeId===strokeId)return frame.strokes[i];
    }
  }
  return null;
}
// The core review mechanic: editing a stroke someone ELSE owns doesn't
// silently overwrite their work — it freezes their pre-edit version as a
// "ghost" (data.isRevisionGhost, rendered desaturated, excluded from
// export — see engine-bridge.js/export.js) and turns the just-edited live
// path into a fresh, separately-owned revision linked back to that ghost
// via data.revisionParentId. A no-op if the path is unowned, already yours,
// or is itself a ghost (never fork a ghost — it's a frozen historical
// record, not something to keep editing forward).
function forkIfForeignOwner(path){
  if(!path||!path.data||!path.data.ownerId||!state.userProfile)return path;
  if(path.data.ownerId===state.userProfile.id)return path;
  if(path.data.isRevisionGhost)return path;
  // RBAC (Phase 3): a supervisor has editorial authority over any layer and
  // edits in place, no fork/ghost — the review safety-net is for ordinary
  // peer-to-peer edits (an animator touching another animator's stroke),
  // not for a supervisor doing their job. Honor-system only, no server-side
  // enforcement — see strokemotion CLAUDE.md-adjacent plan notes.
  if(state.userProfile.role==='supervisor'){
    path.data.ownerId=state.userProfile.id;
    path.data.ownerName=state.userProfile.name;
    path.data.ownerColor=state.userProfile.color;
    return path;
  }
  var layer=path.layer;
  if(!layer)return path;
  var sid=path.data.strokeId;
  var preEdit=sid?findPreEditStrokeData(sid):null;
  var ghost;
  if(preEdit){
    ghost=desP(preEdit,layer);
    ghost.insertBelow(path);
  }else{
    // No snapshot found (e.g. this stroke predates any undo entry this
    // session) — best-effort fallback: clone the current geometry. Not
    // perfectly accurate if this exact gesture also moved it, but still
    // correctly marks the fork so ownership isn't silently lost.
    ghost=path.clone({insert:true,deep:false});
    ghost.insertBelow(path);
  }
  ghost.data.isRevisionGhost=true;
  delete ghost.data.revisionParentId;
  ghost.data.preRevisionOpacity=ghost.opacity!==undefined?ghost.opacity:1; // so Reject can restore it exactly, not just guess 1
  ghost.opacity=ghost.data.preRevisionOpacity*0.35;
  var parentId=ensureStrokeId(ghost);
  delete path.data.strokeId; // the revision is a distinct stroke going forward, not a continuation of the ghost's id
  path.data.ownerId=state.userProfile.id;
  path.data.ownerName=state.userProfile.name;
  path.data.ownerColor=state.userProfile.color;
  path.data.revisionParentId=parentId;
  return path;
}
// Delete-time counterpart: a foreign-owned item never gets p.remove()'d
// outright — it becomes a ghost tagged revisionAction='delete' (hidden in
// the normal view, visible + explainable in "Corrections" view) so the
// original owner can still see and reject the deletion. Returns true if it
// handled the item as a ghost (caller must NOT also remove it); false for
// an item that's unowned/already-yours/already-a-ghost, meaning the
// caller's normal remove() path should proceed as before.
function markDeleteAsRevision(path){
  if(!path||!path.data||!path.data.ownerId||!state.userProfile)return false;
  if(path.data.ownerId===state.userProfile.id)return false;
  if(path.data.isRevisionGhost)return false;
  if(state.userProfile.role==='supervisor')return false; // deletes outright, like their own content
  path.data.isRevisionGhost=true;
  path.data.revisionAction='delete';
  path.data.preRevisionOpacity=path.opacity!==undefined?path.opacity:1;
  path.opacity=path.data.preRevisionOpacity*0.35;
  ensureStrokeId(path);
  return true;
}
// Accept a delete-revision ghost — the original owner agrees their stroke
// should go; NOW it actually gets removed (a delete-revision ghost, unlike
// a reshape/move one, has no paired "active revision" item to keep, so
// there's nothing else to do).
function acceptDeleteRevision(ghost){
  if(!ghost||!ghost.data||ghost.data.revisionAction!=='delete')return;
  if(ghost.data.linkedFill&&!ghost.data.linkedFill.removed)ghost.data.linkedFill.remove();
  if(ghost.data.brushCompanions)ghost.data.brushCompanions.forEach(function(c){if(!c.removed)c.remove();});
  ghost.remove();
}
// Reject a delete-revision ghost — un-ghosts it back to normal, restoring
// its exact pre-fork opacity (see preRevisionOpacity above).
function rejectDeleteRevision(ghost){
  if(!ghost||!ghost.data||ghost.data.revisionAction!=='delete')return;
  ghost.opacity=ghost.data.preRevisionOpacity!==undefined?ghost.data.preRevisionOpacity:1;
  delete ghost.data.isRevisionGhost;
  delete ghost.data.revisionAction;
  delete ghost.data.preRevisionOpacity;
}
// Accept a reshape/move revision — the correction wins outright: drop the
// paired ghost, and the active item just becomes normal content again
// (ownerId stays as the reviewer's, for attribution — matches the plan's
// "accept keeps reviewer ownership" call).
function acceptRevision(activeItem,layer){
  if(!activeItem||!activeItem.data||!activeItem.data.revisionParentId)return;
  var ghost=findStrokeById(layer,activeItem.data.revisionParentId);
  if(ghost)ghost.remove();
  delete activeItem.data.revisionParentId;
}
// Reject a reshape/move revision — the original wins: the reviewer's item
// is discarded, the ghost comes back to life at its exact original opacity.
function rejectRevision(activeItem,layer){
  if(!activeItem||!activeItem.data||!activeItem.data.revisionParentId)return;
  var ghost=findStrokeById(layer,activeItem.data.revisionParentId);
  if(ghost){
    ghost.opacity=ghost.data.preRevisionOpacity!==undefined?ghost.data.preRevisionOpacity:1;
    delete ghost.data.isRevisionGhost;
    delete ghost.data.preRevisionOpacity;
  }
  activeItem.remove();
}
function findStrokeById(layer,strokeId){
  if(!layer||!strokeId)return null;
  for(var i=0;i<layer.children.length;i++){
    if(layer.children[i].data&&layer.children[i].data.strokeId===strokeId)return layer.children[i];
  }
  return null;
}
// onlyIds (optional): restrict wall collection to strokes whose strokeId is
// in this list — used by fillRegenerateLinked to regenerate a fill against
// exactly the strokes it was originally bounded by, so no amount of gap
// escalation can make it grab unrelated artwork.
function fillCollectWalls(layer,excludePath,onlyIds){
  var open=[],closed=[],openSrc=[],closedSrc=[];
  layer.children.forEach(function(c){
    if(c===excludePath||!(c instanceof Path))return;
    if(!(c.strokeColor||c.fillColor||(c.data&&c.data.isVectorBrush)))return;
    if(c.segments.length<2)return;
    var sid=ensureStrokeId(c);
    if(onlyIds&&onlyIds.indexOf(sid)<0)return;
    var w=fillWallPath(c);
    if(w.segments.length<2){w.remove();return;}
    if(w.closed||w.firstSegment.point.getDistance(w.lastSegment.point)<0.5){w.closed=true;closed.push(w);closedSrc.push(sid);}
    else{open.push(w);openSrc.push(sid);}
  });
  return{open:open,closed:closed,openSrc:openSrc,closedSrc:closedSrc};
}
// Builds a graph over the OPEN walls' endpoints: real strokes are edges,
// and any two endpoints within gapThr of each other (but not already
// coincident) get an extra straight "gap" edge — the vector equivalent of
// the orange bridge lines in the reference sketch.
function fillBuildGraph(opens,gapThr){
  var joinEps=Math.max(1.5,gapThr*0.15);
  var nodes=[];
  function findOrCreateNode(pt){
    for(var i=0;i<nodes.length;i++){if(nodes[i].pt.getDistance(pt)<=joinEps)return i;}
    nodes.push({pt:pt.clone(),edges:[]});return nodes.length-1;
  }
  var edges=[];
  opens.forEach(function(s,i){
    var a=findOrCreateNode(s.firstSegment.point);
    var b=findOrCreateNode(s.lastSegment.point);
    edges.push({type:'stroke',strokeIdx:i,a:a,b:b,length:s.length});
  });
  for(var i=0;i<nodes.length;i++){
    for(var j=i+1;j<nodes.length;j++){
      var d=nodes[i].pt.getDistance(nodes[j].pt);
      if(d>joinEps&&d<=gapThr)edges.push({type:'gap',a:i,b:j,length:d});
    }
  }
  edges.forEach(function(e,idx){e.idx=idx;nodes[e.a].edges.push(idx);nodes[e.b].edges.push(idx);});
  return{nodes:nodes,edges:edges};
}
// direction of travel leaving `node` via `edge` (pointing away from node)
function fillEdgeDir(graph,opens,edge,node){
  if(edge.type==='gap'){
    var other=edge.a===node?edge.b:edge.a;
    return graph.nodes[other].pt.subtract(graph.nodes[node].pt).normalize();
  }
  var s=opens[edge.strokeIdx];
  if(edge.a===node)return s.getTangentAt(0);
  return s.getTangentAt(s.length).multiply(-1);
}
function fillPolyArea(pts){
  var a=0;for(var i=0;i<pts.length;i++){var p1=pts[i],p2=pts[(i+1)%pts.length];a+=p1.x*p2.y-p2.x*p1.y;}return Math.abs(a/2);
}
function fillPointInPoly(pt,pts){
  var inside=false;
  for(var i=0,j=pts.length-1;i<pts.length;j=i++){
    var xi=pts[i].x,yi=pts[i].y,xj=pts[j].x,yj=pts[j].y;
    var intersect=((yi>pt.y)!==(yj>pt.y))&&(pt.x<(xj-xi)*(pt.y-yi)/(yj-yi)+xi);
    if(intersect)inside=!inside;
  }
  return inside;
}
// Wall-follower: from `startNode` via `firstEdge`, always take the
// sharpest available turn (turnSign picks which rotational sense), which
// traces the tightest face touching that edge — repeated from every
// stroke/direction/sense as a seed, the smallest traced loop that contains
// the click point is the answer (an inner face, never an outer one that
// happens to also contain the point).
function fillTraceLoop(graph,opens,startNode,firstEdge,turnSign,maxSteps){
  var seq=[];
  var curNode=startNode,curEdge=firstEdge;
  var toNode=curEdge.a===curNode?curEdge.b:curEdge.a;
  seq.push({edge:curEdge,from:curNode,to:toNode});
  var steps=0;
  while(toNode!==startNode){
    steps++;if(steps>maxSteps)return null;
    var arrivalDir=fillEdgeDir(graph,opens,curEdge,curNode).multiply(-1);
    var backAngle=Math.atan2(arrivalDir.y,arrivalDir.x);
    var node=graph.nodes[toNode];
    var bestEdge=null,bestRel=Infinity;
    for(var i=0;i<node.edges.length;i++){
      var e2=graph.edges[node.edges[i]];
      if(e2===curEdge)continue;
      var outDir=fillEdgeDir(graph,opens,e2,toNode);
      var ang=Math.atan2(outDir.y,outDir.x);
      var rel=(turnSign>0)?(ang-backAngle):(backAngle-ang);
      rel=((rel%(2*Math.PI))+2*Math.PI)%(2*Math.PI);
      if(rel<1e-6)rel=2*Math.PI;
      if(rel<bestRel){bestRel=rel;bestEdge=e2;}
    }
    if(!bestEdge)return null;
    var nextNode=bestEdge.a===toNode?bestEdge.b:bestEdge.a;
    seq.push({edge:bestEdge,from:toNode,to:nextNode});
    curNode=toNode;curEdge=bestEdge;toNode=nextNode;
  }
  return seq;
}
function fillBuildPathFromSeq(graph,opens,seq){
  var chainPath=new Path({insert:false});
  seq.forEach(function(hop){
    var e=hop.edge;
    if(e.type==='gap'){
      chainPath.add(graph.nodes[hop.to].pt);
    }else{
      var s=opens[e.strokeIdx].clone({insert:false});
      if(e.a!==hop.from)s.reverse();
      chainPath.addSegments(s.segments);
      s.remove();
    }
  });
  chainPath.closed=true;
  return chainPath;
}
// Fill and stroke stay distinct objects (a fill is never merged into the
// strokes that bound it), but explicitly ASSOCIATED: a fill created by the
// bucket tool records both the click point it was seeded from
// (data.fillSeed) and the exact set of strokes whose walls formed its
// boundary (data.fillWalls, stroke ids — see ensureStrokeId). Regeneration
// after the surrounding strokes are edited then runs against ONLY those
// associated strokes, with gap escalation left fully uncapped: the wall
// restriction — not a distance cap — is what guarantees the fill can never
// balloon onto unrelated artwork, no matter how far its own strokes get
// dragged (even off-canvas) or how slowly/erratically they move. The old
// behavior (seed-only, gap capped at the original fillGapPx) is kept for
// legacy fills that predate the association.
function fillRegenerateLinked(layer,touchedPath){
  if(!layer)return;
  var fills=layer.children.filter(function(c){return c instanceof Path&&c.data&&c.data.fillSeed;});
  if(!fills.length)return;
  // `touchedPath`'s own strokeId, when known — lets the loop below skip
  // fills that are wall-restricted to OTHER strokes entirely, instead of
  // re-tracing (fillVectorFind: real boundary-walk/gap-search) every fill
  // bucket in the layer on every call. Matters most for the eraser, which
  // calls this once per pointermove sample during a drag (eraser-bridge.js)
  // — a layer with several unrelated fill buckets used to pay full
  // re-trace cost for ALL of them on every sample, not just the one(s)
  // actually bounded by the stroke being erased.
  var touchedId=touchedPath&&touchedPath.data&&touchedPath.data.strokeId;
  fills.forEach(function(f){
    var onlyIds=(f.data.fillWalls&&f.data.fillWalls.length)?f.data.fillWalls:undefined;
    if(touchedId&&onlyIds&&onlyIds.indexOf(touchedId)<0)return; // this fill's boundary doesn't involve the touched stroke at all — nothing to re-check
    var seed=new Point(f.data.fillSeed[0],f.data.fillSeed[1]);
    var col=f.fillColor,op=f.opacity;
    // With the wall restriction in place the gap cap is unnecessary (see
    // block comment above); without it (legacy fill), the cap remains the
    // only guard against grabbing unrelated strokes, so keep it.
    var gapCap=onlyIds?undefined:f.data.fillGapPx;
    var res=fillVectorFind(seed,layer,f,gapCap,onlyIds);
    if(!res){
      // The stored seed no longer lands inside the region — usually not
      // because the shape actually opened up, but because the edit moved
      // the boundary past the seed point. Retry from points that track the
      // CURRENT geometry: the fill's own last-good interior first, then the
      // actively-edited path's live interior (immune to multi-frame drift,
      // since it's recomputed from the live boundary every call).
      var fallbackSeed=f.interiorPoint;
      if(fallbackSeed)res=fillVectorFind(fallbackSeed,layer,f,gapCap,onlyIds);
      if(!res&&touchedPath&&touchedPath!==f){
        var touchedSeed=touchedPath.interiorPoint;
        if(touchedSeed)res=fillVectorFind(touchedSeed,layer,f,gapCap,onlyIds);
      }
      if(!res)return; // genuinely no enclosed region anymore (e.g. shape opened up) — leave the old fill as-is
    }
    var idx=layer.children.indexOf(f);
    f.remove();
    res.path.fillColor=col;res.path.strokeColor=null;res.path.opacity=op;
    // Re-anchor the seed to the NEW region's own interior point rather than
    // keeping the original click position — self-correcting, so the next
    // regeneration starts from wherever the fill actually lives now instead
    // of a possibly-stale original click point that keeps drifting further
    // outside the shape with each subsequent edit.
    var newSeed=res.path.interiorPoint||seed;
    res.path.data.fillSeed=[newSeed.x,newSeed.y];res.path.data.fillGapPx=res.gapPx;
    // Keep the ORIGINAL association, not the regeneration's own wallIds:
    // restricted regeneration by construction only ever sees the associated
    // walls, so its result can never widen the set, but a temporarily-open
    // configuration mid-drag could NARROW it (loop closes without touching
    // every original wall this frame) — overwriting would permanently
    // forget strokes that are still part of this fill's boundary.
    if(onlyIds)res.path.data.fillWalls=onlyIds;
    else if(res.wallIds&&res.wallIds.length)res.path.data.fillWalls=res.wallIds;
    layer.insertChild(Math.min(idx,layer.children.length),res.path);
  });
}
// gapThr is a plain world-space distance — "how far apart can two stroke
// ends be and still count as one closed shape" — matching the Gap Size
// presets directly, no scale/resolution conversion involved anywhere.
// ---- WASM path (falls back to the pure-JS fillVectorFind below on any
// failure — same pattern as booleanOp/booleanOpWasm). Rust builds its OWN
// flattened polylines and its OWN curve-accurate final result via
// vello::kurbo (geometry-wasm/src/fill.rs) — JS only ever hands over raw
// wall segments (Paper.js's own point/handleIn/handleOut, same shape
// serP/desP already use everywhere else) and gets back a finished segment
// list for the winning region, or a matched-wall index. No Paper.js curve
// API (flatten/getLocationAt/splitAt) is used for fill-finding anymore.
function _wallSegments(w){
  return w.segments.map(function(s){return{point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};});
}
// Exact wall-wall crossing points via Paper.js's OWN curve-curve
// intersection (Path#getIntersections) — precise to the real bezier
// curves. Rust's own find_crossings() only sees its internally-flattened
// polyline, so its crossing point can be off from the true curve
// intersection by up to the flatten tolerance; cutting each wall's real
// curve at its own slightly-different approximate crossing left a small
// but visible overshoot/flap right at the intersection (the two walls' cut
// points didn't quite coincide in world space — see the screenshot that
// reported this). Computing the exact intersection once, here, and handing
// BOTH walls the same precise point/fractions removes that mismatch
// entirely. kurbo has no general bezier-bezier intersection primitive, so
// this one seam stays JS-side deliberately (see fill.rs's own doc comment).
function _computeExactCrossings(opens){
  var crossings=[];
  for(var i=0;i<opens.length;i++){
    for(var j=i+1;j<opens.length;j++){
      var ix=opens[i].getIntersections(opens[j]);
      ix.forEach(function(loc){
        if(!loc.intersection)return;
        var pt=[loc.point.x,loc.point.y];
        crossings.push({wall:i,frac:loc.offset/opens[i].length,pt:pt});
        crossings.push({wall:j,frac:loc.intersection.offset/opens[j].length,pt:pt});
      });
    }
  }
  return crossings;
}
function fillVectorFindWasm(clickPt,gapThr,layer,excludePath,onlyIds){
  var walls=fillCollectWalls(layer,excludePath,onlyIds);
  var input={
    openWalls:walls.open.map(function(w){return{segments:_wallSegments(w)};}),
    closedWalls:walls.closed.map(function(w){return{segments:_wallSegments(w)};}),
    gapThr:gapThr,click:[clickPt.x,clickPt.y],
    crossings:_computeExactCrossings(walls.open),
  };
  var res=JSON.parse(window.GeometryWasm.fill_find(JSON.stringify(input)));
  if(res.kind==='notFound'){
    walls.open.forEach(function(w){w.remove();});walls.closed.forEach(function(w){w.remove();});
    return null;
  }
  if(res.kind==='closedWall'){
    var winner=walls.closed[res.index];
    walls.open.forEach(function(w){w.remove();});
    walls.closed.forEach(function(w){if(w!==winner)w.remove();});
    winner.closed=true;
    // A standalone closed wall is inherently "pure": it's a real stroke
    // already forming a loop, no bridging involved at all.
    return{path:winner,wallIds:[walls.closedSrc[res.index]],usedGap:false};
  }
  // traced: Rust already resolved the full curve-accurate loop — just
  // instantiate a Path from the segments it returned. res.walls names the
  // open-wall indices the loop is made of — mapped back to the source
  // strokes' ids so the caller can record the fill<->strokes association.
  var chainPath=new Path({insert:false});
  res.segments.forEach(function(sd){
    chainPath.add(new Segment(new Point(sd.point[0],sd.point[1]),new Point(sd.handleIn[0],sd.handleIn[1]),new Point(sd.handleOut[0],sd.handleOut[1])));
  });
  chainPath.closed=true;
  var wallIds=(res.walls||[]).map(function(i){return walls.openSrc[i];}).filter(Boolean);
  walls.open.forEach(function(w){w.remove();});walls.closed.forEach(function(w){w.remove();});
  return{path:chainPath,wallIds:wallIds,usedGap:!!res.usedGap};
}
// No more manual "Gap Size" preset: tries progressively larger gap
// thresholds (smallest first, so the fill result never bridges more than it
// needs to) until one finds a closed region, up to a generous cap — this is
// also what now handles real stroke-stroke CROSSINGS transparently, since
// those are found by fill_find's intersection detection regardless of
// gapThr (even gapThr=0 finds crossing-bounded loops); the escalation is
// only needed for genuine endpoint-to-endpoint gaps.
var FILL_GAP_STEPS=[0,10,24,48,90,160,280];
// Does NOT stop at the first gap threshold that finds any closed loop —
// real curve-curve CROSSINGS are found regardless of gapThr (see
// _computeExactCrossings), so an unrelated stray stroke that merely
// crosses near the click point can already form a small, topologically-
// WRONG closed loop at gapThr=0 (e.g. a small triangular "leak" into a
// nearby doodle via a crossing) — winning by default before the escalation
// ever reaches the higher gapThr that would have found the CORRECT,
// intended closure (needing an actual gap bridged). Since a higher gapThr's
// graph is always a strict superset of a lower one's (more gap edges,
// never fewer), every candidate found at a small gapThr is still available
// at any larger one — so trying every step up to the cap and keeping
// whichever result has the SMALLEST AREA (not the first one found) lets
// the algorithm's own existing "smallest enclosing loop wins" rule
// correctly prefer a tight, correct closure over an incidental, larger
// detour through unrelated nearby artwork, exactly like it already
// disambiguates between candidates WITHIN a single gapThr level.
function fillVectorFind(clickPt,layer,excludePath,maxGapThr,onlyIds){
  var best=null,bestArea=Infinity;
  for(var i=0;i<FILL_GAP_STEPS.length;i++){
    var gapThr=FILL_GAP_STEPS[i];
    if(maxGapThr!==undefined&&gapThr>maxGapThr)break;
    var res=null;
    if(window.GeometryWasm&&window.GeometryWasm.ready){
      try{res=fillVectorFindWasm(clickPt,gapThr,layer,excludePath,onlyIds);}
      catch(e){console.warn('[geometry-wasm] fill_find failed, falling back to JS',e);}
    }
    if(!res)res=fillVectorFindJS(clickPt,gapThr,layer,excludePath,onlyIds);
    if(res){
      var area=Math.abs(res.path.area);
      if(area<bestArea){
        if(best)best.path.remove();
        bestArea=area;best=res;best.gapPx=gapThr;
      }else{
        res.path.remove();
      }
      // A candidate that closed WITHOUT needing any gap-bridge (real
      // strokes/crossings alone) is provably the tightest possible closure
      // through the click point — nothing a larger gapThr finds can be a
      // more correct answer. Stop here instead of continuing to escalate:
      // trying larger gapThr values after this point was what let a
      // strictly worse, larger-gap "shortcut" loop occasionally win on
      // area alone (its two cut-across lobes making the shoelace area come
      // out artificially small), visibly leaking outside the real ink —
      // e.g. a region bounded entirely by 4 crossing curves fully closing
      // at gapThr=0, then a spurious smaller loop at gapThr=280 replacing
      // it and leaving visible white slivers along the real strokes.
      if(!best.usedGap)break;
    }
  }
  return best;
}
function fillVectorFindJS(clickPt,gapThr,layer,excludePath,onlyIds){
  var walls=fillCollectWalls(layer,excludePath,onlyIds);
  var candidates=[]; // {chainPath, area, fromClosedWall}
  walls.closed.forEach(function(w){
    var pts=w.segments.map(function(sg){return sg.point;});
    var area=fillPolyArea(pts);
    if(area>=1&&fillPointInPoly(clickPt,pts))candidates.push({chainPath:w,area:area,fromClosedWall:true});
  });
  if(walls.open.length){
    var graph=fillBuildGraph(walls.open,gapThr);
    var strokeEdges=graph.edges.filter(function(e){return e.type==='stroke';});
    var seedCap=Math.min(strokeEdges.length,150);
    var maxSteps=graph.edges.length*2+8;
    for(var s=0;s<seedCap;s++){
      var seedEdge=strokeEdges[s];
      [seedEdge.a,seedEdge.b].forEach(function(startNode){
        [1,-1].forEach(function(turnSign){
          var seq=fillTraceLoop(graph,walls.open,startNode,seedEdge,turnSign,maxSteps);
          if(!seq||seq.length<2)return;
          var chainPath=fillBuildPathFromSeq(graph,walls.open,seq);
          var pts=chainPath.segments.map(function(sg){return sg.point;});
          var area=fillPolyArea(pts);
          if(area<1||!fillPointInPoly(clickPt,pts)){chainPath.remove();return;}
          var usedGap=seq.some(function(h){return h.edge.type==='gap';});
          candidates.push({chainPath:chainPath,area:area,fromClosedWall:false,usedGap:usedGap});
        });
      });
    }
  }
  if(!candidates.length){
    walls.open.forEach(function(w){w.remove();});walls.closed.forEach(function(w){w.remove();});
    return null;
  }
  // the smallest enclosing loop wins (the innermost face touching the
  // click), matching how Animate/TVPaint resolve nested regions
  candidates.sort(function(a,b){return a.area-b.area;});
  var winner=candidates[0];
  candidates.slice(1).forEach(function(c){if(!c.fromClosedWall)c.chainPath.remove();});
  walls.open.forEach(function(w){w.remove();});
  walls.closed.forEach(function(w){if(w!==winner.chainPath)w.remove();});
  winner.chainPath.closed=true;
  // JS fallback doesn't track which specific walls made the loop — a
  // superset (every collected wall's source id) is safe: regeneration
  // restricted to a superset can never grab strokes that weren't at least
  // present when the fill was made. The WASM path returns the exact set.
  return{path:winner.chainPath,wallIds:walls.openSrc.concat(walls.closedSrc),usedGap:!!winner.usedGap};
}

// ---- STROKE STYLE (dash) ----
function applyStrokeStyle(p){
  var w=p.strokeWidth||state.brushSize;
  if(state.strokeStyle==='dashed')p.dashArray=[w*2.5,w*1.8];
  else if(state.strokeStyle==='dotted')p.dashArray=[w*0.5,w*1.2];
  else p.dashArray=[];
  p.dashOffset=state.dashOffset||0;
  p.miterLimit=state.miterLimit||10;
  p.data=p.data||{};p.data.paintOrder=state.paintOrder;
}

// ---- VARIABLE-WIDTH OUTLINE ENGINE ----
// Shared by the "Taper ends" stroke style and the pressure-simulated vector
// brush: both reduce to "build a filled outline around a centerline given a
// width at each sample point", just with a different width(t) source
// (taper profile vs. pressure/speed). Inspired by Cacani's stroke-taper /
// pressure-stroke pipeline (see research notes in conversation).
function buildVariableWidthPath(pts,widths){
  if(pts.length<2)return null;
  var left=[],right=[],tangents=[];
  for(var i=0;i<pts.length;i++){
    var tangent;
    if(i===0)tangent=pts[1].subtract(pts[0]);
    else if(i===pts.length-1)tangent=pts[i].subtract(pts[i-1]);
    else tangent=pts[i+1].subtract(pts[i-1]);
    if(tangent.length<0.0001)tangent=new Point(1,0);
    tangent=tangent.normalize();
    tangents.push(tangent);
    var normal=new Point(-tangent.y,tangent.x);
    var hw=Math.max(0.3,widths[i]/2);
    left.push(pts[i].add(normal.multiply(hw)));
    right.push(pts[i].subtract(normal.multiply(hw)));
  }
  // Smooth each edge (spine) independently, BEFORE adding the end caps
  // below — smoothing the whole closed outline in one pass (the original
  // approach) pulled the end-cap vertex into the same curve-fit as the
  // ribbon body, producing a sharp/notched tip instead of a clean round
  // cap. Matches the Rust engine's own live-preview cap construction
  // (add_semicircle_cap in engine.rs) so the shape doesn't visibly change
  // right at the moment of release.
  var leftPath=new Path({insert:false,segments:left});
  leftPath.smooth({type:'continuous'});
  var rightPath=new Path({insert:false,segments:right});
  rightPath.smooth({type:'continuous'});
  var outline=new Path({insert:false});
  leftPath.segments.forEach(function(s){outline.add(new Segment(s.point,s.handleIn,s.handleOut));});
  // The right edge is traversed in REVERSE below, so each segment's
  // handleIn/handleOut must be SWAPPED — a first version copied them
  // as-is, leaving every right-edge tangent pointing the wrong way, which
  // crumpled/zigzagged that whole side of the ribbon (the reported
  // "toujours un soucis" screenshot).
  var rightRev=[];
  for(var j=rightPath.segments.length-1;j>=0;j--){
    var rs=rightPath.segments[j];
    rightRev.push(new Segment(rs.point,rs.handleOut,rs.handleIn));
  }
  // Round cap as a true 3-point arc (from -> through -> to), the "through"
  // point bulging out along the local tangent by the LOCAL half-width — the
  // cap radius follows the pressure at that exact endpoint. Path.Arc's own
  // first/last segments sit exactly on points the outline already has (the
  // edge endpoints), so instead of re-adding them (duplicate consecutive
  // points with conflicting handles = the little knob/notch artifacts at
  // the tips), only the arc's INTERIOR segments are added and its entry/
  // exit tangents are transplanted onto the existing neighbor segments.
  function spliceCap(prevSeg,nextSeg,center,tangentDir,hw){
    if(hw<0.05)return[];
    var through=center.add(tangentDir.multiply(hw));
    var arc;
    try{arc=new Path.Arc({from:prevSeg.point,through:through,to:nextSeg.point,insert:false});}
    catch(e){return[];}
    var segs=arc.segments;
    prevSeg.handleOut=segs[0].handleOut;
    nextSeg.handleIn=segs[segs.length-1].handleIn;
    var mid=[];
    for(var k=1;k<segs.length-1;k++)mid.push(new Segment(segs[k].point,segs[k].handleIn,segs[k].handleOut));
    arc.remove();
    return mid;
  }
  var li=pts.length-1;
  var endCapMid=spliceCap(outline.lastSegment,rightRev[0],pts[li],tangents[li],Math.max(0.3,widths[li]/2));
  endCapMid.forEach(function(s){outline.add(s);});
  rightRev.forEach(function(s){outline.add(s);});
  var startCapMid=spliceCap(outline.lastSegment,outline.firstSegment,pts[0],tangents[0].multiply(-1),Math.max(0.3,widths[0]/2));
  startCapMid.forEach(function(s){outline.add(s);});
  outline.closed=true;
  leftPath.remove();rightPath.remove();
  return outline;
}
// Places a freshly-committed Fill Brush stroke per state.fillBrushMode —
// shared by draw-bridge.js and tools.js's own legacy commit so the three
// icon options (Above/Below/Merge) behave identically regardless of which
// engine path drew it. 'above' needs no repositioning (Paper.js already
// appends new items at the front/top by default). 'below' is the tool's
// original always-at-back behavior. 'merge' unites the new shape with the
// first existing fill it overlaps (last-drawn/topmost first, so it merges
// with whatever's visually on top at that spot) instead of stacking a
// separate object — falls back to 'above' if nothing underneath overlaps.
// ---- PROCEDURAL BRUSH ENGINE (dab/nib stamping, Sketchbook-style) ----
// A real raster texture brush stamps a small bitmap ("nib") repeatedly
// along the path — there's no bitmap primitive in a pure-vector renderer,
// so each "dab" here is a small vector ellipse instead, placed at
// arc-length intervals with randomized spacing/rotation/size/opacity/
// perpendicular offset (scatter). This is a genuine procedural stamp
// engine (matches Sketchbook's Nib model: Spacing/Space Randomize/
// Rotation/Rotation Randomize/Scatter/Roundness), a deliberate replacement
// for the earlier "a handful of whole-length jittered copies" approximation
// — that technique could only ever fake an even, blurry grain; discrete
// dabs read as real chalk/charcoal/pencil texture at any zoom level and
// are what applyBrushTexture (below) actually builds.
//
// Parameters (all in the SAME units regardless of preset, so the editor
// panel's sliders mean the same thing for every preset):
//   nibSize    — dab diameter, as a multiplier of the stroke's own width
//   roundness  — 0..1, dab height/width ratio (1 = round nib, <1 = flat/
//                calligraphic nib)
//   spacing    — gap between dab centers, as a fraction of nib diameter
//                (small = smooth solid coverage, large = visible individual
//                dabs/grain)
//   spaceJitter    — 0..1, randomizes spacing per step
//   rotationMode   — 'tangent' (follows the stroke direction, like a real
//                    angled nib), 'random' (each dab spun independently —
//                    reads as rougher grain), or 'fixed' (all dabs at the
//                    same angle, calligraphic look)
//   rotationJitter — degrees of random rotation added on top of the mode
//   sizeJitter     — 0..1, per-dab scale randomization
//   opacity/opacityJitter — per-dab alpha and its randomization
//   scatter    — perpendicular random offset from the centerline, as a
//                fraction of nib diameter (spreads dabs off the exact
//                path — real charcoal/chalk never lays down a perfectly
//                centered line)
//   dashGap    — 0..1 probability a given dab is skipped entirely (broken/
//                torn edge, replaces the old preset system's literal dash
//                array)
var BRUSH_PRESETS={
  'chalk-blunt':     {nibSize:1.3,roundness:.85,spacing:.55,spaceJitter:.25,rotationMode:'random',rotationJitter:60, sizeJitter:.25,opacity:.55,opacityJitter:.25,scatter:.18,dashGap:0},
  'chalk-round':     {nibSize:1.1,roundness:1,  spacing:.4, spaceJitter:.2, rotationMode:'random',rotationJitter:30, sizeJitter:.18,opacity:.5, opacityJitter:.2, scatter:.12,dashGap:0},
  'chalk-scribble':  {nibSize:1.0,roundness:.7, spacing:.7, spaceJitter:.4, rotationMode:'random',rotationJitter:180,sizeJitter:.4, opacity:.4, opacityJitter:.3, scatter:.3, dashGap:.15},
  'charcoal-feather':{nibSize:1.4,roundness:.6, spacing:.6, spaceJitter:.35,rotationMode:'random',rotationJitter:90, sizeJitter:.45,opacity:.3, opacityJitter:.35,scatter:.35,dashGap:.1},
  'charcoal-pencil': {nibSize:.8, roundness:.9, spacing:.3, spaceJitter:.15,rotationMode:'tangent',rotationJitter:15, sizeJitter:.15,opacity:.7, opacityJitter:.15,scatter:.08,dashGap:0},
  'charcoal-rough':  {nibSize:1.5,roundness:.65,spacing:.65,spaceJitter:.45,rotationMode:'random',rotationJitter:180,sizeJitter:.55,opacity:.4, opacityJitter:.35,scatter:.4, dashGap:.18},
  'charcoal-rounded':{nibSize:1.3,roundness:1,  spacing:.45,spaceJitter:.2, rotationMode:'random',rotationJitter:45, sizeJitter:.15,opacity:.55,opacityJitter:.2, scatter:.15,dashGap:0},
  'charcoal-smooth': {nibSize:1.2,roundness:1,  spacing:.25,spaceJitter:.1, rotationMode:'random',rotationJitter:20, sizeJitter:.1, opacity:.65,opacityJitter:.1, scatter:.06,dashGap:0},
  'charcoal-soft':   {nibSize:1.6,roundness:.75,spacing:.5, spaceJitter:.3, rotationMode:'random',rotationJitter:90, sizeJitter:.3, opacity:.32,opacityJitter:.3, scatter:.25,dashGap:.05},
  'charcoal-tapered':{nibSize:1.2,roundness:.8, spacing:.45,spaceJitter:.2, rotationMode:'tangent',rotationJitter:25, sizeJitter:.2, opacity:.55,opacityJitter:.2, scatter:.15,dashGap:0},
  'charcoal-thick':  {nibSize:2.2,roundness:.9, spacing:.4, spaceJitter:.2, rotationMode:'random',rotationJitter:40, sizeJitter:.15,opacity:.6, opacityJitter:.15,scatter:.12,dashGap:0},
  'charcoal-thin':   {nibSize:.6, roundness:.9, spacing:.35,spaceJitter:.15,rotationMode:'random',rotationJitter:30, sizeJitter:.15,opacity:.65,opacityJitter:.15,scatter:.1, dashGap:0},
  'charcoal-varied': {nibSize:1.3,roundness:.7, spacing:.6, spaceJitter:.5, rotationMode:'random',rotationJitter:180,sizeJitter:.65,opacity:.4, opacityJitter:.4, scatter:.3, dashGap:.12},
  'pencil-feather':  {nibSize:.7, roundness:.6, spacing:.6, spaceJitter:.35,rotationMode:'random',rotationJitter:90, sizeJitter:.3, opacity:.3, opacityJitter:.3, scatter:.2, dashGap:.15},
  'pencil-thick':    {nibSize:1.8,roundness:.9, spacing:.3, spaceJitter:.15,rotationMode:'tangent',rotationJitter:15, sizeJitter:.15,opacity:.7, opacityJitter:.15,scatter:.08,dashGap:0},
  'pencil-thin':     {nibSize:.5, roundness:.9, spacing:.3, spaceJitter:.1, rotationMode:'tangent',rotationJitter:10, sizeJitter:.1, opacity:.8, opacityJitter:.1, scatter:.05,dashGap:0},
  // Non-circular tip shapes (tipShape, added alongside the original
  // deformed-ellipse dabs) — the vector answer to Photoshop's flat/chisel,
  // angular, splatter and bristle tip families. See buildDabShape/
  // buildBrushDabs (above) for how each shape is actually constructed.
  'marker-flat':     {nibSize:1.6,roundness:.4, spacing:.25,spaceJitter:.05,rotationMode:'fixed',fixedAngle:35,rotationJitter:4,  sizeJitter:.05,opacity:.85,opacityJitter:.05,scatter:.02,dashGap:0, tipShape:'rect',tipCorner:.1},
  'ink-chisel':      {nibSize:1.2,roundness:.3, spacing:.3, spaceJitter:.08,rotationMode:'tangent',rotationJitter:8,  sizeJitter:.1, opacity:.9, opacityJitter:.05,scatter:.03,dashGap:0, tipShape:'rect',tipCorner:.05},
  'pastel-chip':     {nibSize:1.7,roundness:.85,spacing:.5, spaceJitter:.3, rotationMode:'random',rotationJitter:180,sizeJitter:.3, opacity:.5, opacityJitter:.25,scatter:.2, dashGap:.05,tipShape:'polygon',polySides:6,edgeNoise:.12},
  'chalk-facet':     {nibSize:1.3,roundness:.9, spacing:.45,spaceJitter:.25,rotationMode:'random',rotationJitter:180,sizeJitter:.25,opacity:.45,opacityJitter:.3, scatter:.15,dashGap:0, tipShape:'polygon',polySides:5,edgeNoise:.18},
  'ink-splatter':    {nibSize:1.4,roundness:1,  spacing:1.1,spaceJitter:.6, rotationMode:'random',rotationJitter:180,sizeJitter:.7, opacity:.75,opacityJitter:.2, scatter:.5, dashGap:.1, tipShape:'splatter',edgeNoise:.08},
  'drybrush-bristle':{nibSize:2.2,roundness:1,  spacing:.35,spaceJitter:.15,rotationMode:'tangent',rotationJitter:12, sizeJitter:.3, opacity:.55,opacityJitter:.3, scatter:.6, dashGap:0, tipShape:'bristle',bristleCount:7},
  'watercolor-edge': {nibSize:2.6,roundness:.7, spacing:.5, spaceJitter:.2, rotationMode:'random',rotationJitter:60, sizeJitter:.35,opacity:.22,opacityJitter:.3, scatter:.3, dashGap:0, tipShape:'ellipse',edgeNoise:.25},
};
// Hard ceiling on dabs per stroke, regardless of preset/length — protects
// against a very long stroke with tight spacing multiplying scene-
// serialization cost unboundedly (each dab is a real, separately-
// serialized Path — see the perf audit note in strokemotion/CLAUDE.md).
// Spacing is widened (never narrowed below the preset's own value) just
// enough to land under this cap rather than silently truncating the tail
// of the stroke.
var BRUSH_MAX_DABS=180;
// Single lookup point for a preset's parameters, whether built-in or one of
// the user's own saved presets (state.customBrushPresets, keyed by a
// generated id — see brush-editor.js) — every reader of BRUSH_PRESETS[key]
// (applyBrushTexture, brush-preset-picker.js's preview) goes through this
// instead of the raw dict so custom presets work everywhere built-ins do,
// with no separate code path to keep in sync.
function resolveBrushPreset(key){
  if(!key||key==='none')return null;
  return BRUSH_PRESETS[key]||(state.customBrushPresets&&state.customBrushPresets[key])||null;
}
// Stamps dabs along `pathLike` (anything with .length/.getPointAt/
// .getTangentAt — a live Path, works equally on a throwaway preview path)
// and returns an array of freshly-built, unstyled-position dab Paths (each
// already positioned/rotated/sized, but caller decides fill/opacity/
// insertion — see applyBrushTexture and brush-preset-picker.js's live
// preview, which both call this so the actual placement math can never
// drift between "what you see in the editor" and "what you actually get").
// Deterministic RNG (mulberry32) for dab placement when the SAME texture
// must be reproducible across calls — tween generation re-stamps the dabs
// on every generated inbetween frame from the interpolated centerline, and
// a fresh Math.random each frame would make the texture "boil" violently
// instead of sticking to the morphing stroke. Interactive application keeps
// true randomness (rng omitted → Math.random).
function seededRng(seed){
  var t=seed>>>0;
  return function(){
    t+=0x6D2B79F5;
    var r=Math.imul(t^t>>>15,1|t);
    r=r+Math.imul(r^r>>>7,61|r)^r;
    return((r^r>>>14)>>>0)/4294967296;
  };
}
// Tip-shape geometry, factored out of the dab-stamping loop so every shape
// (not just the original ellipse) gets rotation/scatter/jitter for free.
// `tipShape` is optional on a preset (defaults to 'ellipse', so every
// existing built-in/custom preset keeps its exact prior look with zero
// migration) — this is the vector answer to Photoshop's tip-shape picker
// (round/flat/angled/scatter-cluster brush categories), built as real
// bezier Path geometry rather than a raster stamp, per the explicit "stay
// vector" requirement. `edgeNoise` perturbs any shape's own segment
// points radially and re-smooths — a cheap, fully-vector way to get an
// organic broken edge (torn paper / dry-media chip) without a texture map.
function buildDabShape(w,h,preset,rand){
  var shape=preset.tipShape||'ellipse';
  var path;
  if(shape==='rect'){
    // Flat/chisel nib — a felt-tip or ink-marker style tip, the thing an
    // ellipse fundamentally can't produce (Photoshop's "Flat" tip family).
    var rx=Math.min(w,h)*(preset.tipCorner!==undefined?preset.tipCorner:.15);
    path=new Path.Rectangle({point:[-w/2,-h/2],size:[w,h],radius:rx,insert:false});
  }else if(shape==='polygon'){
    // Angular chip — pastel/chalk-corner look (a real physical chalk chip
    // has flat facets, not a disc), also usable for a faceted ink-brush tip.
    var sides=Math.max(3,preset.polySides||5);
    path=new Path({insert:false,closed:true});
    for(var i=0;i<sides;i++){
      var a=(i/sides)*Math.PI*2;
      var rr=(w/2)*(.8+rand()*.35),rh=(h/2)*(.8+rand()*.35);
      path.add(new Point(Math.cos(a)*rr,Math.sin(a)*rh));
    }
    path.closePath();
    path.smooth({type:'continuous'});
  }else if(shape==='splatter'){
    // Irregular jagged blob — ink-splatter/spray dabs, deliberately NOT
    // radially uniform (varies per-vertex more than 'polygon') so no two
    // splatter dabs read as the same stamped shape repeating.
    var pts=6+Math.floor(rand()*5);
    path=new Path({insert:false,closed:true});
    for(var j=0;j<pts;j++){
      var ang=(j/pts)*Math.PI*2;
      var rr2=(w/2)*(.35+rand()*1.1),rh2=(h/2)*(.35+rand()*1.1);
      path.add(new Point(Math.cos(ang)*rr2,Math.sin(ang)*rh2));
    }
    path.closePath();
    path.smooth({type:'catmull-rom',factor:.5});
  }else{
    path=new Path.Ellipse({center:[0,0],radius:[w/2,h/2],insert:false});
  }
  var edgeNoise=preset.edgeNoise||0;
  if(edgeNoise>0&&path.segments&&path.segments.length){
    path.segments.forEach(function(seg){
      seg.point=seg.point.multiply(1+(rand()*2-1)*edgeNoise);
    });
    if(shape!=='rect')path.smooth({type:'continuous'});
  }
  return path;
}
function buildBrushDabs(pathLike,preset,baseWidth,rng){
  var rand=rng||Math.random;
  var len=pathLike.length;
  if(!(len>0))return[];
  var nibDiam=Math.max(.5,baseWidth*(preset.nibSize!==undefined?preset.nibSize:1));
  // Spacing is deliberately NOT derived from nibDiam (i.e. not from the
  // live stroke width) — it used to be, which compounded with preset.nibSize
  // so a thicker line's dabs came out both bigger AND visibly further apart
  // (reported: "en fonction de la taille de la ligne les points s'écartent",
  // texture reading as sparser/falling apart at larger widths). Basing it on
  // a fixed reference nib diameter instead keeps the dab-to-dab spacing —
  // the texture's grain density — a constant property of the preset, while
  // dab SIZE still scales with width as expected.
  var BRUSH_SPACING_REF_NIB=3;
  var refNibDiam=Math.max(.5,BRUSH_SPACING_REF_NIB*(preset.nibSize!==undefined?preset.nibSize:1));
  var spacing=Math.max(.4,refNibDiam*(preset.spacing!==undefined?preset.spacing:.35));
  // Bristle tips emit several sub-strands PER stamp position (see below) —
  // divide the position budget by that count up front so total emitted
  // dabs still respects BRUSH_MAX_DABS regardless of tip shape.
  var bristleCount=preset.tipShape==='bristle'?Math.max(1,preset.bristleCount||5):1;
  var maxPositions=Math.max(1,Math.floor(BRUSH_MAX_DABS/bristleCount));
  if(len/spacing>maxPositions)spacing=len/maxPositions;
  var roundness=preset.roundness!==undefined?preset.roundness:1;
  var dabs=[],d=0,guard=0;
  while(d<=len&&guard++<maxPositions+2&&dabs.length<BRUSH_MAX_DABS){
    if(!(preset.dashGap&&rand()<preset.dashGap)){
      var at=Math.min(len,d);
      var pt=pathLike.getPointAt(at);
      var tan=pathLike.getTangentAt(at)||new Point(1,0);
      var normal=new Point(-tan.y,tan.x);
      var angleBase;
      if(preset.rotationMode==='random')angleBase=rand()*360;
      else if(preset.rotationMode==='fixed')angleBase=preset.fixedAngle||0;
      else angleBase=tan.angle; // 'tangent' (default): dab follows stroke direction, like a real angled nib
      if(preset.tipShape==='bristle'){
        // Dry-brush / watercolor-edge look: several thin parallel strands
        // fanned across the stroke width instead of one solid dab — the
        // thing a single ellipse (however deformed) structurally cannot
        // produce, closest vector analog to Photoshop's Bristle tip family.
        for(var b=0;b<bristleCount&&dabs.length<BRUSH_MAX_DABS;b++){
          var frac=bristleCount>1?(b/(bristleCount-1)-.5):0;
          var strandOffset=frac*nibDiam*(.5+ (preset.scatter||0));
          var jitterOffset=(rand()*2-1)*nibDiam*(preset.scatter||0)*.3;
          var center=pt.add(normal.multiply(strandOffset+jitterOffset));
          var sizeMul=1+(rand()*2-1)*(preset.sizeJitter||0);
          var w=Math.max(.3,(nibDiam/Math.max(2,bristleCount*.7))*sizeMul);
          var h=Math.max(.3,nibDiam*roundness*(.7+rand()*.5));
          var angle=angleBase+(rand()*2-1)*(preset.rotationJitter||0);
          var dab=buildDabShape(w,h,preset,rand);
          dab.rotate(angle);
          dab.position=center;
          dab.data={dabOpacity:Math.max(0,Math.min(1,(preset.opacity!==undefined?preset.opacity:.5)*(1+(rand()*2-1)*(preset.opacityJitter||0))*.7))};
          dabs.push(dab);
        }
      }else{
        var scatterAmt=(rand()*2-1)*nibDiam*(preset.scatter||0);
        var center2=pt.add(normal.multiply(scatterAmt));
        var sizeMul2=1+(rand()*2-1)*(preset.sizeJitter||0);
        var w2=Math.max(.3,nibDiam*sizeMul2);
        var h2=Math.max(.3,w2*roundness);
        var angle2=angleBase+(rand()*2-1)*(preset.rotationJitter||0);
        var dab2=buildDabShape(w2,h2,preset,rand);
        dab2.rotate(angle2);
        dab2.position=center2;
        dab2.data={dabOpacity:Math.max(0,Math.min(1,(preset.opacity!==undefined?preset.opacity:.5)*(1+(rand()*2-1)*(preset.opacityJitter||0))))};
        dabs.push(dab2);
      }
    }
    d+=spacing*(1+(rand()*2-1)*(preset.spaceJitter||0));
  }
  return dabs;
}
// Builds the real, layer-inserted texture from `basePath`'s own already-
// committed geometry — deliberately reads basePath's live Paper.js shape
// (via buildBrushDabs' .getPointAt/.getTangentAt walk) rather than needing
// the tool's raw pre-simplify samples, so this works identically whether
// called at draw-commit time OR later via "Apply to selection" on an
// arbitrary existing stroke (timeline.js). basePath itself becomes an
// invisible (opacity 0) anchor: still hit-testable (Paper.js hit-testing
// checks path geometry/style, not rendered opacity) so click-to-select and
// the existing move/duplicate/delete machinery all keep working on "the
// stroke" as one unit, while every dab is a real separate Path tagged
// data.isBrushTextureCopy + a shared data.brushGroupId (persisted by serP/
// desP, regrouped into a live data.brushCompanions array by
// relinkBrushCompanions() after every layer rebuild — see that function's
// own comment in app.js for why a live object-reference array can't
// survive a save/reload on its own).
function applyBrushTexture(basePath,presetKey){
  var preset=resolveBrushPreset(presetKey);
  if(!preset||!basePath.segments||basePath.segments.length<2)return basePath;
  var baseWidth=basePath.strokeWidth;
  // On RE-apply (switching preset on an already-textured stroke) the live
  // strokeColor may already be nulled by the fill-visible branch below —
  // fall back to the remembered original so the new dabs don't silently
  // come out colorless.
  var baseColor=basePath.strokeColor||(basePath.data&&basePath.data.preTextureStroke?new Color(basePath.data.preTextureStroke):null);
  var groupId='bg'+Date.now().toString(36)+'_'+Math.floor(Math.random()*1e6);
  var dabs=buildBrushDabs(basePath,preset,baseWidth);
  var companions=dabs.map(function(dab){
    dab.fillColor=baseColor;dab.strokeColor=null;dab.opacity=dab.data.dabOpacity;
    dab.data={isBrushTextureCopy:true,brushGroupId:groupId};
    // Above, not below: when basePath also carries a fill (kept visible,
    // see the fill-case branch just below), inserting dabs UNDER it left
    // its own opaque fill painting straight over the texture — only a thin
    // sliver of dabs peeking out past the fill's own edge was ever visible
    // (reported: texture looked like it wasn't applied at all). The no-fill
    // anchor is invisible (opacity 0) either way, so this is a no-op there.
    dab.insertAbove(basePath);
    return dab;
  });
  // The dabs replace the STROKE's look only. A path that also carries a
  // FILL must keep painting it — the old unconditional `opacity=0` hid the
  // fill along with the stroke (reported: "avec un preset le fill n'est pas
  // visible sauf avec l'onion skin", onion mode forcing its own opacity was
  // exactly why it showed up there). Fill case: keep the path visible and
  // null only the strokeColor (remembered in data.preTextureStroke, same
  // first-application-only convention as preTextureOpacity — whoever clears
  // back to 'none' restores both). No-fill case: the opacity-0 invisible-
  // anchor trick stays (a fill-less stroke-less path draws nothing anyway,
  // but opacity 0 also survives any later fill assignment).
  if(basePath.fillColor){
    if(basePath.data.preTextureStroke===undefined)basePath.data.preTextureStroke=basePath.strokeColor?colorHex8(basePath.strokeColor):null;
    basePath.strokeColor=null;
  }else{
    if(basePath.data.preTextureOpacity===undefined)basePath.data.preTextureOpacity=basePath.opacity!==undefined?basePath.opacity:1;
    basePath.opacity=0; // invisible anchor — the dabs are the whole visible stroke now
  }
  basePath.data.brushCompanions=companions;
  basePath.data.brushTexturePreset=presetKey;
  basePath.data.brushGroupId=groupId;
  return basePath;
}
// Node/tangent edits (subselect drag) mutate the anchor's segments directly
// but the dabs are disposable stamps computed once at apply-time — they
// never re-follow an edited anchor, so a tangent drag "did nothing"
// visually even once resolveBrushAnchor() let you select the real path.
// Re-stamps the whole texture from the anchor's current geometry; call on
// drag-END only (buildBrushDabs is a full rebuild, not cheap per mousemove).
function regenerateBrushTexture(basePath,layer){
  if(!basePath.data||!basePath.data.brushTexturePreset||!basePath.data.brushGroupId)return;
  var gid=basePath.data.brushGroupId;
  for(var i=layer.children.length-1;i>=0;i--){
    var c=layer.children[i];
    if(c.data&&c.data.isBrushTextureCopy&&c.data.brushGroupId===gid)c.remove();
  }
  applyBrushTexture(basePath,basePath.data.brushTexturePreset);
}
function applyFillBrushPlacement(path,layer){
  var mode=state.fillBrushMode;
  if(mode==='merge'){
    var target=null;
    for(var i=layer.children.length-1;i>=0;i--){
      var c=layer.children[i];
      if(c===path||!(c instanceof Path||c instanceof CompoundPath)||!c.fillColor)continue;
      if(c.bounds.intersects(path.bounds)){target=c;break;}
    }
    if(target){
      var united=target.unite(path,{insert:false});
      var idx=layer.children.indexOf(target);
      target.remove();path.remove();
      // unite() returns a CompoundPath the moment the two shapes don't
      // fully overlap (bbox-intersecting but not actually touching) — split
      // into flat Paths at insertion, same as eraseAtPoint/booleanOp, so it
      // isn't silently dropped by saveActiveLayerFrame's `instanceof Path`
      // filter the moment the frame next saves.
      var islands=insertBooleanResult(layer,Math.min(idx,layer.children.length),united,path.fillColor,path.opacity);
      return islands[0];
    }
    return path;
  }
  if(mode==='below')layer.insertChild(0,path);
  return path;
}
// Taper target: an ABSOLUTE floor width (not a percentage of whatever the
// local raw width happens to be) — this matters because the pressure-brush
// speed heuristic (vbPressureOf: slow movement -> thick line) makes a
// stroke's raw width balloon right where a mouse naturally decelerates
// before mouseup. A user typically starts a stroke moving quickly (a short
// high-pressure zone right at the very first sample or two) but SLOWS DOWN
// gradually over many samples before releasing — so the artificially thick
// zone at the tail is both wider in raw-sample-count AND stays close to the
// endpoint's pressure ceiling for longer than the equivalent zone at the
// head. A purely MULTIPLICATIVE taper (old: width *= max(0.15,factor)) only
// ever shrinks that already-inflated tail width by a fixed proportion, so
// the tail visibly stayed thicker/flatter than the head even though the
// same formula ran at both ends — this is what the user reported ("le
// début fait bien le taperends mais la fin du trait est toujours plate").
// Blending toward a fixed small floor instead guarantees both ends actually
// converge to the same thin point regardless of how thick the raw pressure
// sample there was.
function taperFloorWidth(){
  return Math.max(0.6,state.brushSize*0.08);
}
function taperWidths(pts,baseWidth,taperFrac){
  var n=pts.length;
  var lens=[0];for(var i=1;i<n;i++)lens.push(lens[i-1]+pts[i].getDistance(pts[i-1]));
  var total=lens[n-1]||1;
  var floor=taperFloorWidth();
  return pts.map(function(p,i){
    var t=lens[i]/total;
    var head=Math.min(1,t/taperFrac);
    var tail=Math.min(1,(1-t)/taperFrac);
    var f=Math.min(head,tail);
    return floor+(baseWidth-floor)*f;
  });
}
function combineTaper(pts,widths,taperFrac){
  var n=pts.length;
  var lens=[0];for(var i=1;i<n;i++)lens.push(lens[i-1]+pts[i].getDistance(pts[i-1]));
  var total=lens[n-1]||1;
  var floor=taperFloorWidth();
  return widths.map(function(w,i){
    var t=lens[i]/total;
    var head=Math.min(1,t/taperFrac);
    var tail=Math.min(1,(1-t)/taperFrac);
    var f=Math.min(head,tail);
    return floor+(w-floor)*f;
  });
}
function taperWidthAtFrac(frac,baseWidth,taperFrac){
  var head=Math.min(1,frac/taperFrac),tail=Math.min(1,(1-frac)/taperFrac);
  var f=Math.min(head,tail);
  var floor=taperFloorWidth();
  return floor+(baseWidth-floor)*f;
}

// ---- CENTERLINE-DRIVEN VARIABLE-WIDTH STROKES ----
// A variable-width stroke (vector brush or tapered draw/pen) is controlled
// by a single editable centerline path (data.centerSegments: normal
// anchor points + bezier tangent handles, like any other path) plus a
// per-anchor width. The visible filled outline is *derived* from that
// centerline and regenerated whenever a centerline point or handle is
// edited (node tool) or interpolated (tween engine) — matching/edit
// operate on the low-vertex centerline, never on the dense outline.
// Arc-length-parameterized width curve sampled from the RAW (pre-simplify)
// pressure samples — kept as its own array, deliberately independent of
// however few anchor points buildCenterSegmentsFromRawStroke's centerline
// simplification below ends up keeping. Reported bug: a fairly straight
// pressure stroke (e.g. a horizontal swipe) simplifies down to as few as 2-3
// geometric anchors — Paper's simplify() only cares about position, it has
// no idea those anchors also carry a `width` that swung from thin to thick
// and back, so nearly the entire pressure curve was discarded, leaving a
// near-constant width regardless of how the pen was actually pressed. Worse,
// on curved/looping strokes simplify() keeps MORE points near direction
// changes than along straight runs — so what widths DID survive correlated
// with the stroke's geometric shape (corners, curl direction) rather than
// with actual pressure, which is what read as "looks like calligraphy"
// (chisel-pen width-follows-angle) instead of a round brush whose width
// follows pressure. rebuildVectorBrushOutline() below now samples width from
// THIS profile by arc-length fraction instead of interpolating between the
// sparse centerline anchors' own .width, so brush thickness tracks the full
// pressure recording regardless of how aggressively the editable centerline
// itself got simplified.
function buildWidthProfile(rawPts,rawWidths){
  var n=rawPts.length;
  if(n<2)return[{t:0,width:rawWidths[0]||0},{t:1,width:rawWidths[0]||0}];
  var lens=[0];for(var i=1;i<n;i++)lens.push(lens[i-1]+rawPts[i].getDistance(rawPts[i-1]));
  var total=lens[n-1]||1;
  return rawPts.map(function(p,i){return{t:lens[i]/total,width:rawWidths[i]};});
}
function widthAtFrac(profile,frac){
  if(!profile||!profile.length)return null;
  if(frac<=profile[0].t)return profile[0].width;
  if(frac>=profile[profile.length-1].t)return profile[profile.length-1].width;
  var lo=0,hi=profile.length-1;
  while(hi-lo>1){var mid=(lo+hi)>>1;if(profile[mid].t<frac)lo=mid;else hi=mid;}
  var a=profile[lo],b=profile[hi];
  var span=Math.max(0.0000001,b.t-a.t);
  var lt=(frac-a.t)/span;
  return a.width+(b.width-a.width)*lt;
}
function buildCenterSegmentsFromRawStroke(rawPts,rawWidths,smoothingAmt){
  var raw=new Path({insert:false});
  rawPts.forEach(function(p){raw.add(p);});
  raw.simplify(smoothingAmt!==undefined?smoothingAmt:state.smoothing);
  var segs=raw.segments.map(function(seg){
    var best=0,bd=Infinity;
    for(var i=0;i<rawPts.length;i++){var d=seg.point.getDistance(rawPts[i]);if(d<bd){bd=d;best=i;}}
    return{point:[seg.point.x,seg.point.y],handleIn:[seg.handleIn.x,seg.handleIn.y],handleOut:[seg.handleOut.x,seg.handleOut.y],width:rawWidths[best]};
  });
  raw.remove();
  segs.widthProfile=buildWidthProfile(rawPts,rawWidths);
  return segs;
}
function buildCenterSegmentsFromPath(path,widthAtFrac){
  var len=path.length;
  return path.segments.map(function(seg){
    var off=path.getOffsetOf(seg.point);
    var frac=len>0?off/len:0;
    return{point:[seg.point.x,seg.point.y],handleIn:[seg.handleIn.x,seg.handleIn.y],handleOut:[seg.handleOut.x,seg.handleOut.y],width:widthAtFrac(frac)};
  });
}
function applyTaperToCenterSegments(segs,taperFrac){
  var n=segs.length;var lens=[0];
  for(var i=1;i<n;i++)lens.push(lens[i-1]+new Point(segs[i].point).getDistance(new Point(segs[i-1].point)));
  var total=lens[n-1]||1;
  var floor=taperFloorWidth();
  segs.forEach(function(s,i){
    var t=lens[i]/total;
    var f=Math.min(Math.min(1,t/taperFrac),Math.min(1,(1-t)/taperFrac));
    s.width=floor+(s.width-floor)*f;
  });
  // The dense width profile (buildWidthProfile) takes priority over these
  // sparse anchor widths at render time (rebuildVectorBrushOutline), so the
  // taper has to be applied there too or it'd have no visible effect at all
  // once a profile is present.
  if(segs.widthProfile){
    segs.widthProfile.forEach(function(pr){
      var f=Math.min(Math.min(1,pr.t/taperFrac),Math.min(1,(1-pr.t)/taperFrac));
      pr.width=floor+(pr.width-floor)*f;
    });
  }
}
// Rebuilds the visible filled outline of a centerline-driven path from its
// current data.centerSegments + per-anchor widths. Call after any edit to
// the centerline (node drag) or after generating an interpolated frame.
function rebuildVectorBrushOutline(path){
  var cs=path.data&&path.data.centerSegments;
  if(!cs||cs.length<2)return;
  var profile=path.data.widthProfile;
  var center=new Path({insert:false});
  cs.forEach(function(s){center.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
  var len=center.length;
  var n=Math.max(8,Math.round(len/6));
  var segLens=[0];for(var i=1;i<cs.length;i++)segLens.push(segLens[i-1]+new Point(cs[i].point).getDistance(new Point(cs[i-1].point)));
  var totalApprox=segLens[segLens.length-1]||1;
  var pts=[],widths=[];
  for(var k=0;k<=n;k++){
    var d=len*k/n;
    var pt=center.getPointAt(d);if(!pt)pt=center.getPointAt(len)||cs[cs.length-1].point;
    pts.push(pt);
    // Prefer the dense raw-pressure profile (arc-length fraction along the
    // CURRENT centerline — a reasonable proxy for "fraction along the
    // original stroke" even after node edits) over interpolating between
    // the sparse centerline anchors' own .width, which loses most of the
    // pressure curve whenever simplify() kept few anchors (see
    // buildWidthProfile's comment for the full story).
    var frac=d/Math.max(len,0.0001);
    var w=widthAtFrac(profile,frac);
    if(w==null){
      var targetLen=frac*totalApprox;
      var wi=0;while(wi<segLens.length-2&&segLens[wi+1]<targetLen)wi++;
      var span=Math.max(0.0001,segLens[wi+1]-segLens[wi]);
      var lt=Math.min(1,Math.max(0,(targetLen-segLens[wi])/span));
      w=cs[wi].width+(cs[wi+1].width-cs[wi].width)*lt;
    }
    widths.push(w);
  }
  // Sync the linked fill backdrop (Fill enabled while drawing with the
  // Pressure brush — see draw-bridge.js's commitStroke) to the SAME curve
  // just fit through the centerline anchors, rather than an independently-
  // simplified copy of the raw points: guarantees the fill boundary always
  // matches the ribbon's actual current shape, including after any edit
  // that lands here (node drag, scale, rotate) — this single choke point
  // is already called by every one of those, so this is the one place that
  // needed to know about the link. Pure translation (drag-to-move, which
  // never changes the shape) is handled separately at its own two call
  // sites (select-bridge.js / timeline.js selPropsApplyMove) since it never
  // goes through here.
  var linkedFill=path.data&&path.data.linkedFill;
  if(linkedFill&&!linkedFill.removed){
    linkedFill.segments=center.segments.map(function(s){return new Segment(s.point,s.handleIn,s.handleOut);});
    linkedFill.closed=true;
  }
  center.remove();
  var outline=buildVariableWidthPath(pts,widths);
  if(outline){
    path.segments=outline.segments;path.closed=true;outline.remove();
    // Fill Brush's whole point is to draw a genuine filled SHAPE — a clean
    // editable path with a handful of real bezier anchors/tangent handles,
    // the same as a hand-plotted Pen path — not a dense point-cloud ribbon
    // outline (buildVariableWidthPath emits one point per width-profile
    // sample, easily 50-200+ points for an ordinary stroke). The pressure
    // STROKE brush (isVectorBrush without isFillShape) deliberately keeps
    // the dense outline as-is: its centerSegments/widthProfile editing
    // model depends on this exact geometry being regenerated identically on
    // every edit. Only fill-brush shapes get this extra simplify pass,
    // right here in the single choke point every edit (scale/rotate/node-
    // drag) already funnels through, so it stays smooth after edits too.
    if(path.data&&path.data.isFillShape)path.simplify(2.5);
  }
}

// ---- POINT TYPE CONVERSION (corner / smooth / symmetric) ----
// centerSegments (pressure-brush centerline) store point/handleIn/handleOut
// as plain [x,y] arrays, while normal Path.segments use real Paper Points —
// these accessors paper over that so the conversion logic itself doesn't care.
function _ptGet(isVB,seg){return isVB?new Point(seg.point[0],seg.point[1]):seg.point;}
function _ptHandleIn(isVB,seg){return isVB?new Point(seg.handleIn?seg.handleIn[0]:0,seg.handleIn?seg.handleIn[1]:0):seg.handleIn;}
function _ptHandleOut(isVB,seg){return isVB?new Point(seg.handleOut?seg.handleOut[0]:0,seg.handleOut?seg.handleOut[1]:0):seg.handleOut;}
function _ptSetHandleIn(isVB,seg,pt){if(isVB)seg.handleIn=[pt.x,pt.y];else seg.handleIn=pt;}
function _ptSetHandleOut(isVB,seg,pt){if(isVB)seg.handleOut=[pt.x,pt.y];else seg.handleOut=pt;}
function setPointType(type){
  var path=nodeEditTargetPath();
  if(!path||!_nodeSel.length){showToast('Sélectionnez un ou plusieurs points (Subselect)');return;}
  pushUndo();
  var isVB=!!(path.data&&path.data.isVectorBrush&&path.data.centerSegments);
  var arr=isVB?path.data.centerSegments:path.segments;
  var n=arr.length,closed=isVB?false:path.closed;
  _nodeSel.forEach(function(idx){
    var seg=arr[idx];
    if(type==='corner'){_ptSetHandleIn(isVB,seg,new Point(0,0));_ptSetHandleOut(isVB,seg,new Point(0,0));return;}
    var prevSeg=idx>0?arr[idx-1]:(closed?arr[n-1]:null);
    var nextSeg=idx<n-1?arr[idx+1]:(closed?arr[0]:null);
    var pt=_ptGet(isVB,seg);
    var diff;
    if(prevSeg&&nextSeg)diff=_ptGet(isVB,nextSeg).subtract(_ptGet(isVB,prevSeg));
    else if(nextSeg)diff=_ptGet(isVB,nextSeg).subtract(pt);
    else if(prevSeg)diff=pt.subtract(_ptGet(isVB,prevSeg));
    else diff=new Point(1,0);
    var dir=diff.length>0.01?diff.normalize():new Point(1,0);
    var curOut=_ptHandleOut(isVB,seg),curIn=_ptHandleIn(isVB,seg);
    var outLen=curOut.length||(nextSeg?_ptGet(isVB,nextSeg).getDistance(pt)/3:30);
    var inLen=curIn.length||(prevSeg?pt.getDistance(_ptGet(isVB,prevSeg))/3:30);
    if(type==='symmetric'){var len=(outLen+inLen)/2||30;outLen=len;inLen=len;}
    _ptSetHandleOut(isVB,seg,dir.multiply(outLen));
    _ptSetHandleIn(isVB,seg,dir.multiply(-inLen));
  });
  if(isVB)rebuildVectorBrushOutline(path);
  renderNodeHandles();
  fillRegenerateLinked(userLayers[state.activeLayerIdx],path);
  saveActiveLayerFrame();updateUI();
}

// ---- BOOLEAN PATH OPERATIONS (union/subtract/intersect/exclude) ----
// Paper.js already implements the geometry (PathItem#unite/subtract/
// intersect/exclude) — this just exposes it as a Select-tool command over
// the current multi-selection, applied pairwise front-to-back, replacing
// the operands with a single result styled after the topmost shape.
// ---- Rust/WASM geometry integration (Phase B — see geometry-wasm/) ----
// Pure JS<->polygon conversion helpers used only by the WASM path; the
// pure-JS Paper.js boolean ops below are untouched and remain the fallback.
function _pathToPolygonInput(path){
  if(path.className==='CompoundPath')throw new Error('compound path input not supported by the WASM op yet');
  var flat=path.clone({insert:false});
  flat.flatten(1);
  var exterior=flat.segments.map(function(s){return[s.point.x,s.point.y];});
  flat.remove();
  return{exterior:exterior,holes:[]};
}
function _polygonsToPaperItem(polys){
  if(!polys.length)return null;
  function buildLoop(pts){var p=new Path();pts.forEach(function(pt,i){var pp=new Point(pt[0],pt[1]);if(i===0)p.moveTo(pp);else p.lineTo(pp);});p.closed=true;return p;}
  if(polys.length===1&&!polys[0].holes.length)return buildLoop(polys[0].exterior);
  var cp=new CompoundPath({insert:false});
  polys.forEach(function(poly){
    var ext=buildLoop(poly.exterior);
    ext.clockwise=true;
    cp.addChild(ext);
    // Paper.js's CompoundPath uses the nonzero winding rule to decide fill —
    // a hole loop only actually subtracts if it winds OPPOSITE to its
    // exterior; the raw point order coming back from geo_booleanop/Rust
    // isn't guaranteed to already be that way (a same-winding "hole" just
    // ADDS its area back on top of the exterior instead of cutting it out,
    // which is what an eraser bite silently did before this fix — the
    // erased notch came back bigger than the original shape). Forcing
    // opposite windings here makes it correct regardless of the source's
    // orientation convention.
    poly.holes.forEach(function(hole){var h=buildLoop(hole);h.clockwise=false;cp.addChild(h);});
  });
  return cp;
}
function _polyArea(poly){
  var pts=poly.exterior,a=0,n=pts.length;
  for(var i=0;i<n;i++){var j=(i+1)%n;a+=pts[i][0]*pts[j][1]-pts[j][0]*pts[i][1];}
  return Math.abs(a)/2;
}
function booleanOpWasm(op,paths){
  var accumulator=_pathToPolygonInput(paths[0]);
  var finalPolys=[accumulator];
  for(var i=1;i<paths.length;i++){
    var b=_pathToPolygonInput(paths[i]);
    var json=window.GeometryWasm.boolean_op(op,JSON.stringify(accumulator),JSON.stringify(b));
    var polys=JSON.parse(json);
    if(!polys.length)return null;
    // Exclude/subtract can legitimately produce several disjoint polygons
    // (e.g. two crescents from an XOR) — every one of them belongs in the
    // final result. Only the accumulator carried into the NEXT fold (for a
    // 3rd+ operand) collapses to the single largest-by-area polygon, since
    // that's a reasonable simplification and matches how the plain-2-shape
    // case (the common one) behaves either way.
    finalPolys=polys;
    polys=polys.slice().sort(function(x,y){return _polyArea(y)-_polyArea(x);});
    accumulator=polys[0];
  }
  return _polygonsToPaperItem(finalPolys);
}
function booleanOp(op){
  if(selectedPaths.length<2){showToast('Sélectionnez au moins 2 formes');return;}
  pushUndo();
  var paths=selectedPaths.slice();
  var result=null;
  if(window.GeometryWasm&&window.GeometryWasm.ready){
    try{result=booleanOpWasm(op,paths);}
    catch(e){console.warn('[geometry-wasm] boolean op failed, falling back to Paper.js',e);result=null;}
  }
  if(!result){
    result=paths[0];
    for(var i=1;i<paths.length;i++){
      var r=result[op](paths[i],{insert:false});
      if(result!==paths[0])result.remove();
      result=r;
    }
  }
  var style=paths[paths.length-1];
  result.strokeColor=style.strokeColor;result.strokeWidth=style.strokeWidth;
  result.strokeCap=style.strokeCap;result.strokeJoin=style.strokeJoin;
  result.fillColor=style.fillColor;result.opacity=style.opacity;
  var boolLayer=userLayers[state.activeLayerIdx];
  // subtract/exclude routinely produce a CompoundPath (disjoint remainders,
  // or a hole) — split into flat Paths at insertion, same as eraseAtPoint,
  // so it isn't silently dropped by saveActiveLayerFrame's `instanceof
  // Path` filter (or selection/click-to-pick) the moment the frame saves.
  var islands=insertBooleanResult(boolLayer,boolLayer.children.length,result,style.fillColor,style.opacity);
  paths.forEach(function(p){p.remove();});
  selectedPaths=islands;state.selectedStrokeIndices=[];
  fillRegenerateLinked(boolLayer,islands[0]);
  saveActiveLayerFrame();updateUI();showToast('Opération booléenne appliquée');
}

// ---- PRECISION VECTOR ERASER ----
// A real vector eraser bites a round notch out of whatever it touches —
// including fills — the way Illustrator/Animate's eraser actually works:
// it's a boolean subtraction of a circle (the "brush") from the target's
// fill geometry, not a path.split() at right angles to the line. A plain
// stroked path (pen/line/rect — real Paper.js stroke, not a filled
// pressure-brush outline) has no fill to subtract from, so it's first
// expanded into an equivalent filled ink shape (same technique the
// pressure brush already uses) — after that, ALL erasable ink is just
// "a filled path", one code path handles strokes, brush ink, and fills.
function eraseExpandStrokeToFill(path){
  var flat=path.clone({insert:false});
  flat.flatten(Math.max(0.4,(path.strokeWidth||1)*0.15));
  var pts=flat.segments.map(function(s){return s.point;});
  flat.remove();
  if(pts.length<2)return null;
  var w=path.strokeWidth||state.brushSize;
  var outline=buildVariableWidthPath(pts,pts.map(function(){return w;}));
  if(!outline)return null;
  outline.fillColor=path.strokeColor;outline.strokeColor=null;outline.opacity=path.opacity;
  return outline;
}
// Builds the filled "capsule" (stadium shape: a rectangle with two
// semicircular caps) swept between two points at the given radius, reusing
// eraseExpandStrokeToFill's existing constant-width-outline machinery
// (just fed a 2-point line instead of a real stroke) — this is what lets
// eraseAtPoint subtract ONE continuous shape per drag segment instead of a
// lone circle at the current point, which is what produced the "chain of
// separate circles" scalloped look reported when erasing continuously: at
// normal drag speed, pointermove samples are spaced farther apart than the
// circle's own radius, so consecutive discrete circle-subtracts don't fully
// overlap and leave rounded bites between them instead of one smooth edge.
function buildEraserCapsule(fromPt,toPt,radius){
  if(fromPt.getDistance(toPt)<0.5)return null;
  var line=new Path({insert:false});
  line.strokeWidth=radius*2;
  line.add(fromPt);line.add(toPt);
  var capsule=eraseExpandStrokeToFill(line);
  line.remove();
  return capsule;
}
// ---- Rust/WASM erase (mirrors booleanOpWasm/fillVectorFindWasm: try WASM,
// fall back to Paper.js's own subtract on any failure or unsupported input).
// _itemInFromPath rejects CompoundPath up front (returns null, no throw) —
// same punt _pathToPolygonInput takes for booleanOp, since geometry-wasm's
// ItemIn/build_bezpath is a single BezPath with no holes concept yet. That
// falls back to the JS path below, so erasing repeatedly into an
// already-notched shape still works, just without the WASM path once a
// hole exists.
function _itemInFromPath(p){
  if(p.className!=='Path')return null;
  return{
    segments:p.segments.map(function(s){return{point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};}),
    closed:true,
    fillColor:[0,0,0,255] // value unused by erase_at_point — presence is what marks the shape as "already filled" (skip stroke expansion)
  };
}
// Returns {ok:false} when the WASM op can't handle this input (CompoundPath)
// so the caller falls back to Paper.js — distinct from {ok:true,item:null},
// which means WASM ran fine and the shape was erased away entirely.
function eraseAtPointWasm(target,worldPt,radius,fromPt){
  var itemIn=_itemInFromPath(target);
  if(!itemIn)return{ok:false};
  var input={item:itemIn,eraserX:worldPt.x,eraserY:worldPt.y,eraserRadius:radius};
  if(fromPt)input.eraserFrom=[fromPt.x,fromPt.y];
  var polys=JSON.parse(window.GeometryWasm.erase_at_point(JSON.stringify(input)));
  return{ok:true,item:_polygonsToPaperItem(polys)};
}
function eraseAtPoint(path,worldPt,radius,fromPt){
  var layer=userLayers[state.activeLayerIdx];
  var idx=layer.children.indexOf(path);
  var target=path;
  var isVB=!!(path.data&&path.data.isVectorBrush);
  // An OPEN path has zero real fill area no matter what fillColor says —
  // since Draw-tool strokes now get a fillColor by default (fillEnabled
  // defaults true), an open hand-drawn line can reach here with
  // path.fillColor set but path.closed===false. Subtracting directly
  // against that degenerate zero-area "fill" always empties out, which
  // deleted the ENTIRE stroke on any erase touch instead of notching it.
  // Still expand to a proper capsule/ribbon whenever the path isn't closed.
  //
  // A CLOSED path with BOTH a fill AND a visible stroke (the default for
  // almost anything drawn — fillEnabled defaults true) used to skip this
  // whole branch and erase only the interior fill polygon, then unconditionally
  // null out strokeColor on the result below. Since a Path can only carry one
  // strokeColor for its whole outline, that didn't "notch" the border where
  // touched — it deleted the visible stroke everywhere on the shape in one
  // touch, which read as "the eraser deletes the whole stroke+fill". Fix:
  // whenever there IS a strokeColor, always expand it into real ink geometry
  // and UNION it with the existing fill first, so the border becomes part of
  // the erasable area instead of being discarded outright.
  if(!isVB&&path.strokeColor){
    var expanded=eraseExpandStrokeToFill(path);
    if(expanded){
      if(path.fillColor&&path.closed){
        var fillCopy=path.clone({insert:false});
        fillCopy.strokeColor=null;
        var merged=fillCopy.unite(expanded,{insert:false});
        fillCopy.remove();expanded.remove();
        merged.fillColor=path.fillColor;merged.opacity=path.opacity;
        layer.insertChild(idx,merged);
        path.remove();
        target=merged;
      }else{
        layer.insertChild(idx,expanded);
        path.remove();
        target=expanded;
      }
    }else if(!path.fillColor||!path.closed){
      return;
    }
  }
  if(!target.fillColor)return;
  var col=target.fillColor,op=target.opacity;
  var tIdx=layer.children.indexOf(target);
  var wasmRes=null;
  if(window.GeometryWasm&&window.GeometryWasm.ready){
    try{wasmRes=eraseAtPointWasm(target,worldPt,radius,fromPt);}
    catch(e){console.warn('[geometry-wasm] erase_at_point failed, falling back to Paper.js',e);wasmRes=null;}
  }
  var result;
  if(wasmRes&&wasmRes.ok){
    result=wasmRes.item;
    target.remove();
  }else{
    var eraserShape=(fromPt&&buildEraserCapsule(fromPt,worldPt,radius))||new Path.Circle({center:worldPt,radius:radius,insert:false});
    result=target.subtract(eraserShape,{insert:false});
    eraserShape.remove();
    target.remove();
  }
  var hasArea=result&&((result.children&&result.children.length)||(result.segments&&result.segments.length));
  if(hasArea){
    result.fillColor=col;result.strokeColor=null;result.opacity=op;
    // A notch cut through a pressure-brush stroke's middle no longer has a
    // single consistent centerline, so it becomes a plain filled shape from
    // here on (still fully colorable/selectable/erasable, just not
    // re-editable as a tapered centerline) — the same tradeoff Illustrator
    // makes when you erase into a live stroke.
    insertBooleanResult(layer,Math.min(tIdx,layer.children.length),result,col,op);
  }else if(result)result.remove();
}
// Inserts the result of a Paper.js/WASM boolean op (subtract/unite/
// intersect/exclude — used by eraseAtPoint above, booleanOp, and
// applyFillBrushPlacement's "merge" mode) into `layer` at `insertAt`.
// A boolean op routinely returns a CompoundPath the moment its result has
// more than one contour (disjoint islands) or a hole — but every OTHER
// consumer of layer children (saveActiveLayerFrame/saveAllLayerFrames's
// `instanceof Path` filter, selection construction in select-bridge.js/
// tools.js/timeline.js, tween matching) only ever expected flat Paths, the
// same historical assumption every one of these boolean-op call sites used
// to make. A raw CompoundPath left in the layer renders fine on the CURRENT
// frame (engine-bridge.js's buildSceneJson/onionLayerItems both flatten
// CompoundPath already) but silently vanishes from persisted frame data —
// and from every selection/click-to-pick path — the moment the frame next
// saves, which reads exactly like "the shape deletes itself" with a delay.
// Splitting each island into its own independent Path here, once, at every
// insertion point, means nothing downstream needs to learn about
// CompoundPath at all. A genuinely fully-enclosed hole (rare outside a
// deliberate "punch a hole" op) loses its cutout this way (each island
// paints solid) — an accepted tradeoff, since every other consumer already
// can't represent a hole through this same flat-Path convention.
function insertBooleanResult(layer,insertAt,result,fillColor,opacity){
  if(!(result instanceof CompoundPath)){
    layer.insertChild(insertAt,result);
    return[result];
  }
  var islands=result.children.slice();
  islands.forEach(function(isl,k){
    isl.remove(); // detach from the CompoundPath (only removes it from ITS parent, the island itself survives)
    if(fillColor!==undefined)isl.fillColor=fillColor;
    isl.strokeColor=null;
    if(opacity!==undefined)isl.opacity=opacity;
    layer.insertChild(insertAt+k,isl);
  });
  result.remove(); // now-empty wrapper, nothing left to discard but itself
  return islands;
}

// ---- PRESSURE-SIMULATED VECTOR BRUSH ----
var _vb={pts:[],widths:[],lastT:0,lastPt:null};
// Mirrors draw-bridge.js's smoothPressure()/stabilizePoint() — this Paper-
// native path is only a fallback (the engine is on by default), but the two
// must stay in phase per this file's own duplication-hazard convention, or
// falling back here mid-session would visibly change how a stroke feels.
var _vbSmoothedPressure=null;
function vbSmoothPressure(p){
  if(_vbSmoothedPressure==null){_vbSmoothedPressure=p;return p;}
  _vbSmoothedPressure+=(p-_vbSmoothedPressure)*0.45;
  return _vbSmoothedPressure;
}
function vbStabilizePoint(pt){
  var stab=state.stabilizer;
  if(!stab){stabQueue.length=0;return pt;}
  stabQueue.push(pt);
  var maxQ=stab===1?3:stab===2?6:10;
  while(stabQueue.length>maxQ)stabQueue.shift();
  var avg=new Point(0,0);
  stabQueue.forEach(function(p){avg=avg.add(p);});
  return avg.divide(stabQueue.length);
}
// See draw-bridge.js's pressureOf() for the full story: once a real pen
// sample has been seen this gesture, a later 0/missing reading (most
// tablets report exactly that for the sample right as contact ends at
// lift-off) HOLDS the last true pressure instead of falling through to the
// mouse-speed heuristic below — which misreads the natural deceleration of
// lifting the pen as "slow = max pressure", ballooning a fat round blob
// exactly where the stroke should taper to a point.
var _vbLastPenPressure=null;
function vbPressureOf(event){
  if(_stylus.isPen){
    if(_stylus.pressure>0){_vbLastPenPressure=Math.min(1,_stylus.pressure);return _vbLastPenPressure;}
    if(_vbLastPenPressure!=null)return _vbLastPenPressure;
  }
  // tablet pressure surfaced through the mouse-event force channel — only
  // trust a sample from the last 250ms so a stale end-of-stroke value never
  // bleeds into the next stroke
  if(_stylus.force>0&&Date.now()-_stylus.forceT<250)return _stylus.force;
  var raw=event.event&&event.event.pressure;
  if(typeof raw==='number'&&raw>0&&raw!==0.5)return Math.min(1,raw);
  var now=Date.now();var dt=Math.max(8,now-(_vb.lastT||now));_vb.lastT=now;
  var dist=_vb.lastPt?event.point.getDistance(_vb.lastPt):0;_vb.lastPt=event.point.clone();
  var speed=dist/dt;
  return Math.max(0.15,1-Math.min(1,speed/2.2));
}
// Maps a normalized pressure sample [0..1] to an actual stroke width using
// the user's pressure range (min% at zero pressure → max% at full), with an
// optional inversion (hard press = thin line — Callipeg/Sketchbook both
// offer this for inking styles where light touches carry the weight).
function vbWidthFor(p){
  if(state.pressureInvert)p=1-p;
  var lo=state.pressureMin/100,hi=state.pressureMax/100;
  return state.brushSize*(lo+(hi-lo)*p);
}
function vbRebuildPreview(){
  if(_vb.pts.length<2)return;
  var widths=_vb.pts.map(function(p,i){return vbWidthFor(_vb.widths[i]);});
  if(state.taperEnds)widths=combineTaper(_vb.pts,widths,0.15);
  var outline=buildVariableWidthPath(_vb.pts,widths);
  if(outline){
    currentPath.segments=outline.segments;currentPath.closed=true;outline.remove();
  }
}

// ---- PEN TOOL (Bezier anchor points) ----
// Click = corner anchor. Click+drag = smooth anchor, drag sets the tangent
// handle (mirrored in/out, Illustrator/Animate-style). Click near the first
// point, or a quick second click in place, finalizes the path.
var _pen={path:null,previewLine:null,draggingHandle:false,lastClickTime:0,lastClickPt:null};
function finalizePen(){
  if(_pen.previewLine){_pen.previewLine.remove();_pen.previewLine=null;}
  if(!_pen.path)return;
  if(_pen.path.segments.length<2){_pen.path.remove();if(state.undoStack.length)state.undoStack.pop();}
  else if(state.taperEnds){
    var p=_pen.path;
    var cs=buildCenterSegmentsFromPath(p,function(frac){return taperWidthAtFrac(frac,state.brushSize,0.18);});
    var outline=new Path({insert:false});
    outline.data.isVectorBrush=true;outline.data.centerSegments=cs;
    outline.fillColor=state.strokeColor;outline.strokeColor=null;outline.opacity=state.opacity/100;
    rebuildVectorBrushOutline(outline);
    outline.insertAbove(p);
    p.remove();
    if(state.shadowMode)outline.data.channelTag='shadow';
    tagOwner(outline);
  }else{
    if(state.shadowMode)_pen.path.data.channelTag='shadow';
    tagOwner(_pen.path);
  }
  _pen.path=null;_pen.draggingHandle=false;
  saveActiveLayerFrame();updateUI();
}

function onMouseDown(event){
  if(state.playing){stopPlay();return;}
  if(state.tool==='hand'||state.spaceDown){state.isPanning=true;return;}
  if(state.tool==='zoom'){if(event.event.altKey)view.zoom=Math.max(.05,view.zoom*.8);else view.zoom=Math.min(20,view.zoom*1.25);updZoom();renderArcs();return;}
  var layer=userLayers[state.activeLayerIdx];
  if(state.tool==='draw'){
    if(state.layers[state.activeLayerIdx].locked)return;ensureKeyframe();pushUndo();layer.activate();
    if(state.vectorBrush){
      _vbLastPenPressure=null;_vbSmoothedPressure=null;stabQueue=[event.point.clone()];_vb.pts=[event.point.clone()];_vb.widths=[vbPressureOf(event)];_vb.lastT=Date.now();_vb.lastPt=event.point.clone();
      currentPath=new Path();currentPath.fillColor=state.strokeColor;currentPath.strokeColor=null;currentPath.opacity=state.opacity/100;
      currentPath.data.isVectorBrush=true;
    }else{
      currentPath=new Path();currentPath.strokeColor=state.strokeColor;currentPath.strokeWidth=state.brushSize;
      currentPath.strokeCap=state.strokeCap;currentPath.strokeJoin=state.strokeJoin;currentPath.fillColor=state.fillEnabled?state.fillColor:null;currentPath.opacity=state.opacity/100;
      applyStrokeStyle(currentPath);
    }
    currentPath.add(event.point);stabQueue=[event.point.clone()];
  }else if(state.tool==='pen'){
    if(state.layers[state.activeLayerIdx].locked)return;
    var now=Date.now();
    var isDoubleClick=_pen.path&&(now-_pen.lastClickTime<350)&&_pen.lastClickPt&&event.point.getDistance(_pen.lastClickPt)<10/view.zoom;
    _pen.lastClickTime=now;_pen.lastClickPt=event.point.clone();
    if(isDoubleClick){finalizePen();return;}
    if(!_pen.path){
      ensureKeyframe();pushUndo();layer.activate();
      _pen.path=new Path();_pen.path.strokeColor=state.strokeColor;_pen.path.strokeWidth=state.brushSize;
      _pen.path.strokeCap=state.strokeCap;_pen.path.strokeJoin=state.strokeJoin;_pen.path.fillColor=null;_pen.path.opacity=state.opacity/100;
      applyStrokeStyle(_pen.path);
      _pen.path.add(event.point);
    }else{
      var first=_pen.path.firstSegment.point;var tol=10/view.zoom;
      if(_pen.path.segments.length>1&&event.point.getDistance(first)<tol){
        _pen.path.closed=true;finalizePen();return;
      }
      _pen.path.add(event.point);
    }
    _pen.draggingHandle=true;
  }else if(state.tool==='line'||state.tool==='rect'||state.tool==='ellipse'){
    if(state.layers[state.activeLayerIdx].locked)return;ensureKeyframe();pushUndo();layer.activate();shapeStart=event.point.clone();
    if(state.tool==='line')currentPath=new Path.Line({from:event.point,to:event.point,strokeColor:state.strokeEnabled?state.strokeColor:null,strokeWidth:state.brushSize,strokeCap:state.strokeCap,fillColor:null,opacity:state.opacity/100});
    else if(state.tool==='rect')currentPath=new Path.Rectangle({from:event.point,to:event.point.add(new Point(1,1)),strokeColor:state.strokeEnabled?state.strokeColor:null,strokeWidth:state.brushSize,fillColor:state.fillEnabled?state.fillColor:null,opacity:state.opacity/100});
    else currentPath=new Path.Ellipse({rectangle:new Rectangle(event.point,new Size(1,1)),strokeColor:state.strokeEnabled?state.strokeColor:null,strokeWidth:state.brushSize,fillColor:state.fillEnabled?state.fillColor:null,opacity:state.opacity/100});
  }else if(state.tool==='subselect'){
    var bestNh=null,bestNd=8/view.zoom;
    for(var ni=0;ni<nodeHandles.length;ni++){var nh=nodeHandles[ni];var nd=event.point.getDistance(nh.pos);if(nd<bestNd){bestNd=nd;bestNh=nh;}}
    if(bestNh){
      pushUndo();
      _nodeDrag.active=true;_nodeDrag.path=selectedPaths[0];_nodeDrag.segIndex=bestNh.segIndex;
      // grabbing one of several marquee-selected anchors drags them all
      if(bestNh.type==='point'&&_nodeSel.indexOf(bestNh.segIndex)>=0&&_nodeSel.length>1){_nodeDrag.type='group';}
      else{
        _nodeDrag.type=bestNh.type;
        if(bestNh.type==='point'){_nodeSel=event.modifiers.shift?_nodeSel.concat([bestNh.segIndex]):[bestNh.segIndex];renderNodeHandles();}
      }
      return;
    }
    var subHit=layer.hitTest(event.point,{stroke:true,fill:true,tolerance:8/view.zoom});
    if(subHit&&subHit.item instanceof Path){
      clearSel();
      var subTarget=resolveBrushAnchor(subHit.item,layer);
      selectedPaths.push(subTarget);state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
      // nodeHandles (hit-test array) is stale until rebuilt — see the same
      // fix's comment in subselect-bridge.js for the full story.
      renderNodeHandles();
    }else if(selectedPaths.length===1){
      // empty-space drag with a path selected: marquee over its anchors
      _nmq.active=true;_nmq.start=event.point.clone();_nmq.rect=null;
      return;
    }else{clearSel();}
    renderArcs();updateUI();
  }else if(state.tool==='comment'){
    // Existing pin nearby -> reopen it for editing; otherwise start a new
    // one anchored at the click. Hit radius is in world units scaled by
    // zoom so it feels consistent regardless of how far zoomed in you are.
    var hitRadius=14/view.zoom;
    var existing=(state.comments||[]).filter(function(cm){return cm.frame===state.currentFrame;})
      .find(function(cm){return event.point.getDistance(new Point(cm.x,cm.y))<hitRadius;});
    openCommentPopover(event.point,existing);
  }else if(state.tool==='fsselect'){
    _fsSel=fsHitTest(event.point,layer);
    updateUI();
    // A plain click-to-select mutates no layer content, so it never bumps
    // window._sceneVersion (saveActiveLayerFrame/loadFrame's job) — without
    // this the highlight overlay wouldn't actually appear until some
    // unrelated action happened to trigger the next render tick.
    if(window.SMEngineBridge&&window.SMEngineBridge.renderNow)window.SMEngineBridge.renderNow();
  }else if(state.tool==='select'){
    for(var i=0;i<arcHandles.length;i++){var ah=arcHandles[i];if(event.point.getDistance(ah.handle.position)<14/view.zoom){draggingArc=ah;return;}}draggingArc=null;
    var bestXh=null,bestXd=9/view.zoom;
    for(var xi=0;xi<xformHandles.length;xi++){var xh=xformHandles[xi];var xd=event.point.getDistance(xh.pos);if(xd<bestXd){bestXd=xd;bestXh=xh;}}
    if(bestXh){
      pushUndo();
      var xb=xformSelBounds();
      if(bestXh.type==='rotate'){
        _xform.active=true;_xform.type='rotate';_xform.center=xb.center.clone();
        _xform.startAngle=Math.atan2(event.point.y-_xform.center.y,event.point.x-_xform.center.x)*180/Math.PI;_xform.lastAngle=0;
      }else{
        var anchorMap={nw:'se',ne:'sw',sw:'ne',se:'nw',n:'s',s:'n',e:'w',w:'e'};
        var anchors={nw:xb.topLeft,ne:xb.topRight,sw:xb.bottomLeft,se:xb.bottomRight,n:xb.topCenter,s:xb.bottomCenter,e:xb.rightCenter,w:xb.leftCenter};
        _xform.active=true;_xform.type='scale';_xform.dir=bestXh.dir;
        _xform.anchor=anchors[anchorMap[bestXh.dir]].clone();_xform.origHandlePos=bestXh.pos.clone();
        _xform.lastSx=1;_xform.lastSy=1;
      }
      return;
    }
    var hit=layer.hitTest(event.point,{stroke:true,fill:true,tolerance:8/view.zoom});
    var hitOtherLayerIdx=-1;
    // Nothing on the active layer — check every OTHER normal layer too
    // (clicking a stroke drawn on layer 1 while layer 2 is active must
    // switch to layer 1), same courtesy the component fallback below
    // already gives symbol layers. Topmost-drawn first.
    if(!hit){
      for(var pli=project.layers.length-1;pli>=0;pli--){
        var pl=project.layers[pli];var oli=userLayers.indexOf(pl);
        if(oli<0||oli===state.activeLayerIdx)continue;
        var ld2=state.layers[oli];
        if(!ld2||ld2.locked||!ld2.visible||ld2.symbolId)continue;
        var oh=pl.hitTest(event.point,{stroke:true,fill:true,tolerance:8/view.zoom});
        if(oh){hit=oh;hitOtherLayerIdx=oli;break;}
      }
    }
    if(!hit){
      // Nothing on the active layer — but a component instance sitting on
      // a DIFFERENT layer should still act like a clickable object (Animate
      // graphic-symbol behavior), so check component layers as a fallback
      // rather than requiring the user to make that layer active first.
      var compHit=hitTestComponentLayers(event.point);
      if(compHit){
        var now2=Date.now();
        var isDbl=_compClick.layerIdx===compHit.layerIdx&&(now2-_compClick.time<350);
        _compClick.layerIdx=compHit.layerIdx;_compClick.time=now2;
        if(!event.modifiers.shift)clearSel();
        state.activeLayerIdx=compHit.layerIdx;activateUL(compHit.layerIdx);
        selectedPaths=userLayers[compHit.layerIdx].children.filter(function(c){return (c instanceof Path||c instanceof Raster)&&!(c.data&&(c.data.isLinkedFillCompanion||c.data.isBrushTextureCopy));});
        state.selectedStrokeIndices=[];
        renderArcs();updateUI();
        if(isDbl)window.SM.enterSymbol(state.layers[compHit.layerIdx].symbolId);
        return;
      }
    }
    if(hit&&(hit.item instanceof Path||hit.item instanceof Raster)){
      if(hitOtherLayerIdx>=0){state.activeLayerIdx=hitOtherLayerIdx;activateUL(hitOtherLayerIdx);}
      // Same "act as one rigid group" requirement as the compHit fallback
      // above, for the case where the component is ALREADY the active
      // layer (hitTestComponentLayers is only ever consulted when the
      // active layer's own hitTest misses, so this plain branch used to be
      // reachable for a component too, selecting just the one clicked
      // child instead of the whole instance).
      var activeLd2=state.layers[state.activeLayerIdx];
      if(activeLd2&&activeLd2.symbolId){
        var now3=Date.now();
        var isDbl2=_compClick.layerIdx===state.activeLayerIdx&&(now3-_compClick.time<350);
        _compClick.layerIdx=state.activeLayerIdx;_compClick.time=now3;
        if(!event.modifiers.shift)clearSel();
        selectedPaths=userLayers[state.activeLayerIdx].children.filter(function(c){return (c instanceof Path||c instanceof Raster)&&!(c.data&&(c.data.isLinkedFillCompanion||c.data.isBrushTextureCopy));});
        state.selectedStrokeIndices=[];
        renderArcs();updateUI();
        if(isDbl2)window.SM.enterSymbol(activeLd2.symbolId);
        return;
      }
      var p=hit.item;var idx2=selectedPaths.indexOf(p);
      if(event.modifiers.shift){
        if(idx2>=0)selectedPaths.splice(idx2,1);else selectedPaths.push(p);
      }else if(idx2<0){
        // Clicking a NEW item without shift replaces the selection — but
        // clicking one that's ALREADY part of a multi-selection must NOT
        // clear the rest of it first, or dragging the group by its body
        // (as opposed to grabbing a transform handle, which is checked
        // above and returns before reaching here) collapses the selection
        // down to just the one clicked item before the move-drag even
        // starts, so only that one element ends up moving — exactly the
        // reported "transform works but moving a multi-selection doesn't".
        clearSel();
        selectedPaths.push(p);
      }
      state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
    }else{
      if(!event.modifiers.shift)clearSel();
      // clicked empty canvas: start a rubber-band marquee selection instead
      // of just clearing — lets you drag-select multiple strokes/fills at
      // once (stroke and fill shapes are both plain Paths here, so one
      // marquee naturally picks up both kinds together).
      _marquee.active=true;_marquee.start=event.point.clone();_marquee.rect=null;
    }
    renderArcs();updateUI();
  }else if(state.tool==='eraser'){
    if(state.layers[state.activeLayerIdx].locked)return;ensureKeyframe();
    // instanceof Path AND CompoundPath, not just Path: eraseAtPoint (below)
    // turns a shape into a CompoundPath the moment a bite creates a hole
    // (a donut shape can't be represented as a single Path) — if the guard
    // only accepted `instanceof Path`, erasing again into that same shape
    // silently no-opped (hit-test still found it, but the type check
    // rejected it), matching the reported "sometimes erases, sometimes
    // not" — it worked until the first bite, then stopped for that shape.
    var hit2=layer.hitTest(event.point,{stroke:true,fill:true,tolerance:Math.max(8,state.eraserSize/2)/view.zoom});
    if(hit2&&(hit2.item instanceof Path||hit2.item instanceof CompoundPath)&&(hit2.item.strokeColor||hit2.item.fillColor||(hit2.item.data&&hit2.item.data.isVectorBrush))){
      pushUndo();_eraseDragActive=true;
      var erasedItem2=hit2.item;
      eraseAtPoint(erasedItem2,event.point,state.eraserSize/2);
      _eraseLastPt=event.point.clone();
      fillRegenerateLinked(layer,erasedItem2);saveActiveLayerFrame();updateUI();
    }
  }else if(state.tool==='fill'){
    if(state.layers[state.activeLayerIdx].locked)return;ensureKeyframe();
    if(event.modifiers.shift){
      var hitRm=layer.hitTest(event.point,{fill:true,tolerance:12/view.zoom});
      if(hitRm&&hitRm.item instanceof Path&&hitRm.item.fillColor){pushUndo();hitRm.item.fillColor=null;saveActiveLayerFrame();updateUI();showToast('Fill supprimé');}
      return;
    }
    var res=fillVectorFind(event.point,layer,null);
    if(!res){showToast('Aucune zone fermée ici');return;}
    pushUndo();
    layer.insertChild(0,res.path);
    res.path.fillColor=state.fillColor;res.path.strokeColor=null;res.path.opacity=state.opacity/100;
    // remembers where/how this fill was made so subselection edits to the
    // strokes around it can regenerate it in place (see fillRegenerateLinked)
    res.path.data.fillSeed=[event.point.x,event.point.y];
    res.path.data.fillGapPx=res.gapPx;
    if(res.wallIds&&res.wallIds.length)res.path.data.fillWalls=res.wallIds;
    saveActiveLayerFrame();updateUI();showToast('Fill appliqué');
  }else if(state.tool==='fillbrush'){
    if(state.layers[state.activeLayerIdx].locked)return;ensureKeyframe();pushUndo();layer.activate();
    _vbLastPenPressure=null;_vb.pts=[event.point.clone()];_vb.widths=[vbPressureOf(event)];_vb.lastT=Date.now();_vb.lastPt=event.point.clone();
    currentPath=new Path();currentPath.fillColor=state.fillColor;currentPath.strokeColor=null;currentPath.opacity=state.opacity/100;
    currentPath.data.isVectorBrush=true;currentPath.data.isFillShape=true;
    currentPath.add(event.point);stabQueue=[event.point.clone()];
  }else if(state.tool==='eyedropper'){
    var hit4=layer.hitTest(event.point,{stroke:true,fill:true,tolerance:8/view.zoom});
    if(hit4&&hit4.item instanceof Path){var ep=hit4.item;
      var isVB=!!(ep.data&&ep.data.isVectorBrush);
      // colorHex8(), not .toCSS(true) — Paper's own toCSS(true) always
      // forces alpha to 1 (see colorHex8's comment in app.js), which threw
      // away a picked color's real transparency; .dataset.hex8 alongside
      // .value for the same reason as everywhere else this pattern appears
      // — the native <input type=color> truncates alpha out of .value.
      if(isVB&&ep.fillColor){state.strokeColor=colorHex8(ep.fillColor);document.getElementById('color-stroke').value=state.strokeColor;document.getElementById('color-stroke').dataset.hex8=state.strokeColor;document.getElementById('pm-stroke-c').value=state.strokeColor;document.getElementById('pm-stroke-c').dataset.hex8=state.strokeColor;document.getElementById('stroke-well').style.background=state.strokeColor;document.getElementById('pm-stroke').style.background=state.strokeColor;}
      else{
        if(ep.strokeColor){state.strokeColor=colorHex8(ep.strokeColor);document.getElementById('color-stroke').value=state.strokeColor;document.getElementById('color-stroke').dataset.hex8=state.strokeColor;document.getElementById('pm-stroke-c').value=state.strokeColor;document.getElementById('pm-stroke-c').dataset.hex8=state.strokeColor;document.getElementById('stroke-well').style.background=state.strokeColor;document.getElementById('pm-stroke').style.background=state.strokeColor;}
        if(ep.fillColor){state.fillColor=colorHex8(ep.fillColor);state.fillEnabled=true;document.getElementById('color-fill').value=state.fillColor;document.getElementById('color-fill').dataset.hex8=state.fillColor;document.getElementById('pm-fill-c').value=state.fillColor;document.getElementById('pm-fill-c').dataset.hex8=state.fillColor;document.getElementById('pm-fill').style.background=state.fillColor;document.getElementById('fill-well').style.background=state.fillColor;document.getElementById('fill-well').classList.remove('none');document.getElementById('p-fill-on').checked=true;}
      }
      if(ep.strokeWidth){state.brushSize=ep.strokeWidth;document.getElementById('p-sw').value=Math.round(ep.strokeWidth);}
      showToast('Color picked');}
  }
}
function onMouseDrag(event){
  if(state.playing)return;
  if(state.isPanning||state.spaceDown){var dx=event.event.movementX||0;var dy=event.event.movementY||0;view.center=view.center.subtract(new Point(dx,dy).divide(view.zoom));return;}
  if(state.tool==='draw'){
    if(!currentPath)return;
    if(state.vectorBrush){
      var vbP=vbSmoothPressure(vbPressureOf(event));
      _vb.pts.push(vbStabilizePoint(event.point.clone()));_vb.widths.push(vbP);
      vbRebuildPreview();
      return;
    }
    var stab=state.stabilizer;
    if(stab===0)currentPath.add(event.point);
    else{stabQueue.push(event.point.clone());var maxQ=stab===1?3:stab===2?6:10;while(stabQueue.length>maxQ)stabQueue.shift();var avg=new Point(0,0);stabQueue.forEach(function(p){avg=avg.add(p);});avg=avg.divide(stabQueue.length);currentPath.add(avg);}
  }else if(state.tool==='fillbrush'){
    if(!currentPath)return;
    var fbP=vbSmoothPressure(vbPressureOf(event));
    _vb.pts.push(vbStabilizePoint(event.point.clone()));_vb.widths.push(fbP);
    vbRebuildPreview();
  }else if(state.tool==='pen'){
    if(!_pen.path||!_pen.draggingHandle)return;
    var seg=_pen.path.lastSegment;var delta=event.point.subtract(seg.point);
    seg.handleOut=delta;seg.handleIn=delta.multiply(-1);
  }else if((state.tool==='line'||state.tool==='rect'||state.tool==='ellipse')&&currentPath&&shapeStart){
    currentPath.remove();
    if(state.tool==='line'){currentPath=new Path.Line({from:shapeStart,to:event.point,strokeColor:state.strokeEnabled?state.strokeColor:null,strokeWidth:state.brushSize,strokeCap:state.strokeCap,fillColor:null,opacity:state.opacity/100});applyStrokeStyle(currentPath);}
    else if(state.tool==='rect')currentPath=new Path.Rectangle({from:shapeStart,to:event.point,strokeColor:state.strokeEnabled?state.strokeColor:null,strokeWidth:state.brushSize,fillColor:state.fillEnabled?state.fillColor:null,opacity:state.opacity/100});
    else{currentPath=new Path.Ellipse({rectangle:new Rectangle(shapeStart,event.point),strokeColor:state.strokeEnabled?state.strokeColor:null,strokeWidth:state.brushSize,fillColor:state.fillEnabled?state.fillColor:null,opacity:state.opacity/100});}
  }else if(state.tool==='subselect'){
    if(_nmq.active){
      var nx1=Math.min(_nmq.start.x,event.point.x),ny1=Math.min(_nmq.start.y,event.point.y);
      var nx2=Math.max(_nmq.start.x,event.point.x),ny2=Math.max(_nmq.start.y,event.point.y);
      if(_nmq.rect)_nmq.rect.remove();
      var prevA2=project.activeLayer;marqueeLayer.activate();
      _nmq.rect=new Path.Rectangle({from:new Point(nx1,ny1),to:new Point(nx2,ny2),strokeColor:'rgba(255,184,108,.9)',strokeWidth:1/view.zoom,dashArray:[4/view.zoom,3/view.zoom],fillColor:new Color(1,0.72,0.42,0.08),insert:true});
      prevA2.activate();
    }else if(_nodeDrag.active&&_nodeDrag.type==='group'){
      var gp=_nodeDrag.path;
      if(gp.data&&gp.data.isVectorBrush&&gp.data.centerSegments){
        _nodeSel.forEach(function(si){var cs3=gp.data.centerSegments[si];if(cs3)cs3.point=[cs3.point[0]+event.delta.x,cs3.point[1]+event.delta.y];});
        rebuildVectorBrushOutline(gp);
      }else{
        _nodeSel.forEach(function(si){var sg=gp.segments[si];if(sg)sg.point=sg.point.add(event.delta);});
      }
      renderNodeHandles();
      // Live fill follow: without this, a fill linked to this stroke only
      // regenerated at mouseup, so the fill visibly lagged behind the
      // stroke for the whole drag instead of tracking it in real time.
      fillRegenerateLinked(userLayers[state.activeLayerIdx],gp);
    }else if(_nodeDrag.active){
      var sdp=_nodeDrag.path;
      if(sdp.data&&sdp.data.isVectorBrush&&sdp.data.centerSegments){
        var scs=sdp.data.centerSegments[_nodeDrag.segIndex];
        if(_nodeDrag.type==='point'){scs.point=[scs.point[0]+event.delta.x,scs.point[1]+event.delta.y];}
        else if(_nodeDrag.type==='handleOut'){var sno=[event.point.x-scs.point[0],event.point.y-scs.point[1]];scs.handleOut=sno;if(!event.modifiers.alt)scs.handleIn=[-sno[0],-sno[1]];}
        else if(_nodeDrag.type==='handleIn'){var sni=[event.point.x-scs.point[0],event.point.y-scs.point[1]];scs.handleIn=sni;if(!event.modifiers.alt)scs.handleOut=[-sni[0],-sni[1]];}
        rebuildVectorBrushOutline(sdp);
      }else{
        var sseg=sdp.segments[_nodeDrag.segIndex];
        if(_nodeDrag.type==='point')sseg.point=sseg.point.add(event.delta);
        else if(_nodeDrag.type==='handleOut'){sseg.handleOut=event.point.subtract(sseg.point);if(!event.modifiers.alt)sseg.handleIn=sseg.handleOut.multiply(-1);}
        else if(_nodeDrag.type==='handleIn'){sseg.handleIn=event.point.subtract(sseg.point);if(!event.modifiers.alt)sseg.handleOut=sseg.handleIn.multiply(-1);}
      }
      renderNodeHandles();
      fillRegenerateLinked(userLayers[state.activeLayerIdx],sdp);
    }
  }else if(state.tool==='select'){
    if(_xform.active){
      if(_xform.type==='rotate'){
        var curAngle=Math.atan2(event.point.y-_xform.center.y,event.point.x-_xform.center.x)*180/Math.PI;
        var deltaFromStart=curAngle-_xform.startAngle;
        var stepAngle=deltaFromStart-_xform.lastAngle;
        selectedPaths.forEach(function(p){
          p.rotate(stepAngle,_xform.center);
          if(p.data&&p.data.isVectorBrush&&p.data.centerSegments){rotateCenterSegments(p.data.centerSegments,stepAngle,_xform.center.x,_xform.center.y);rebuildVectorBrushOutline(p);}
        });
        _xform.lastAngle=deltaFromStart;
        symGestureAccumulate(new Matrix().rotate(stepAngle,_xform.center));
      }else{
        var anchor=_xform.anchor,dir=_xform.dir,sx=1,sy=1;
        if(dir==='nw'||dir==='ne'||dir==='sw'||dir==='se'){
          var origDX=_xform.origHandlePos.x-anchor.x,origDY=_xform.origHandlePos.y-anchor.y;
          var curDX=event.point.x-anchor.x,curDY=event.point.y-anchor.y;
          sx=origDX!==0?curDX/origDX:1;sy=origDY!==0?curDY/origDY:1;
        }else if(dir==='n'||dir==='s'){
          var origDY2=_xform.origHandlePos.y-anchor.y,curDY2=event.point.y-anchor.y;
          sy=origDY2!==0?curDY2/origDY2:1;
        }else{
          var origDX2=_xform.origHandlePos.x-anchor.x,curDX2=event.point.x-anchor.x;
          sx=origDX2!==0?curDX2/origDX2:1;
        }
        if(Math.abs(sx)<0.05)sx=sx<0?-0.05:0.05;
        if(Math.abs(sy)<0.05)sy=sy<0?-0.05:0.05;
        var stepSx=sx/_xform.lastSx,stepSy=sy/_xform.lastSy;
        selectedPaths.forEach(function(p){
          p.scale(stepSx,stepSy,anchor);
          if(p.data&&p.data.isVectorBrush&&p.data.centerSegments){scaleCenterSegments(p.data.centerSegments,stepSx,stepSy,anchor.x,anchor.y);rebuildVectorBrushOutline(p);}
        });
        _xform.lastSx=sx;_xform.lastSy=sy;
        symGestureAccumulate(new Matrix().scale(stepSx,stepSy,anchor));
      }
      renderTransformHandles();
    }else if(_marquee.active){
      var mx1=Math.min(_marquee.start.x,event.point.x),my1=Math.min(_marquee.start.y,event.point.y);
      var mx2=Math.max(_marquee.start.x,event.point.x),my2=Math.max(_marquee.start.y,event.point.y);
      if(_marquee.rect)_marquee.rect.remove();
      var prevA=project.activeLayer;marqueeLayer.activate();
      _marquee.rect=new Path.Rectangle({from:new Point(mx1,my1),to:new Point(mx2,my2),strokeColor:'rgba(74,158,255,.9)',strokeWidth:1/view.zoom,dashArray:[4/view.zoom,3/view.zoom],fillColor:new Color(0.29,0.62,1,0.08),insert:true});
      prevA.activate();
    }else if(draggingArc){setArcCtrl(draggingArc.fA,draggingArc.fB,draggingArc.matchIdx,draggingArc.ptA,draggingArc.ptB,event.point.x,event.point.y);renderArcs();}
    else if(selectedPaths.length>0){
      if(!_moveDragStarted){pushUndo();_moveDragStarted=true;}
      selectedPaths.forEach(function(p){
      p.position=p.position.add(event.delta);
      if(p.data&&p.data.isVectorBrush&&p.data.centerSegments){p.data.centerSegments.forEach(function(s){s.point=[s.point[0]+event.delta.x,s.point[1]+event.delta.y];});}
    });renderNodeHandles();renderTransformHandles();
    symGestureAccumulate(new Matrix().translate(event.delta));}
  }else if(state.tool==='eraser'){
    eraseUpdateCursor(event);
    if(state.layers[state.activeLayerIdx].locked)return;
    var layer2e=userLayers[state.activeLayerIdx];
    var hit=layer2e.hitTest(event.point,{stroke:true,fill:true,tolerance:Math.max(8,state.eraserSize/2)/view.zoom});
    if(hit&&(hit.item instanceof Path||hit.item instanceof CompoundPath)&&(hit.item.strokeColor||hit.item.fillColor||(hit.item.data&&hit.item.data.isVectorBrush))){
      if(!_eraseDragActive){pushUndo();_eraseDragActive=true;}
      eraseAtPoint(hit.item,event.point,state.eraserSize/2,_eraseLastPt);
      _eraseLastPt=event.point.clone();
      fillRegenerateLinked(layer2e,null);saveActiveLayerFrame();updateUI();
    }
  }
}
function onMouseUp(event){
  if(state.isPanning){state.isPanning=false;return;}if(state.playing)return;
  _eraseDragActive=false;_eraseLastPt=null;
  if(state.tool==='draw'&&currentPath){
    if(state.vectorBrush){
      // Catch-up point, mirrors draw-bridge.js's onUp — the stabilizer's
      // trailing average otherwise leaves the stroke stopping short of
      // wherever the pen actually lifted.
      _vb.pts.push(event.point.clone());_vb.widths.push(vbSmoothPressure(vbPressureOf(event)));
      if(_vb.pts.length<2){currentPath.remove();if(state.undoStack.length)state.undoStack.pop();}
      else{
        var rawWidths=_vb.pts.map(function(p,i){return vbWidthFor(_vb.widths[i]);});
        var cs=buildCenterSegmentsFromRawStroke(_vb.pts,rawWidths,state.smoothing);
        if(state.taperEnds)applyTaperToCenterSegments(cs,0.15);
        currentPath.data.centerSegments=cs;
        currentPath.data.widthProfile=cs.widthProfile;
        rebuildVectorBrushOutline(currentPath);
      }
      _vb.pts=[];_vb.widths=[];
    }else if(currentPath.segments.length<2){
      currentPath.remove();if(state.undoStack.length)state.undoStack.pop();
    }else{
      currentPath.simplify(state.smoothing);
      if(state.taperEnds){
        var cs2=buildCenterSegmentsFromPath(currentPath,function(frac){return taperWidthAtFrac(frac,state.brushSize,0.18);});
        var outline=new Path({insert:false});
        outline.data.isVectorBrush=true;outline.data.centerSegments=cs2;
        outline.fillColor=state.strokeColor;outline.strokeColor=null;outline.opacity=state.opacity/100;
        rebuildVectorBrushOutline(outline);
        userLayers[state.activeLayerIdx].activate();outline.insertAbove(currentPath);
        currentPath.remove();currentPath=outline;
      }
    }
    // Animate's "Paint behind": the finished stroke slips under everything
    // already on the layer instead of stacking on top.
    if(currentPath&&state.drawMode==='behind')userLayers[state.activeLayerIdx].insertChild(0,currentPath);
    if(currentPath&&state.shadowMode)currentPath.data.channelTag='shadow';
    if(currentPath)tagOwner(currentPath);
    currentPath=null;stabQueue=[];saveActiveLayerFrame();updateUI();
  }else if(state.tool==='fillbrush'&&currentPath){
    _vb.pts.push(event.point.clone());_vb.widths.push(vbSmoothPressure(vbPressureOf(event)));
    if(_vb.pts.length<2){currentPath.remove();if(state.undoStack.length)state.undoStack.pop();}
    else{
      var fbWidths=_vb.pts.map(function(p,i){return vbWidthFor(_vb.widths[i]);});
      var fbCs=buildCenterSegmentsFromRawStroke(_vb.pts,fbWidths,state.smoothing);
      currentPath.data.centerSegments=fbCs;
      currentPath.data.widthProfile=fbCs.widthProfile;
      rebuildVectorBrushOutline(currentPath);
      // Placement (Above/Below/Merge) — see applyFillBrushPlacement's own
      // comment; replaces the old unconditional "always at the back".
      currentPath=applyFillBrushPlacement(currentPath,userLayers[state.activeLayerIdx]);
      if(currentPath&&state.shadowMode)currentPath.data.channelTag='shadow';
      if(currentPath)tagOwner(currentPath);
    }
    _vb.pts=[];_vb.widths=[];currentPath=null;stabQueue=[];saveActiveLayerFrame();updateUI();
  }else if(state.tool==='pen'){
    _pen.draggingHandle=false;
  }else if((state.tool==='line'||state.tool==='rect'||state.tool==='ellipse')&&currentPath){
    if(shapeStart&&event.point.getDistance(shapeStart)<2){currentPath.remove();if(state.undoStack.length)state.undoStack.pop();}
    else{
      if(state.shadowMode)currentPath.data.channelTag='shadow';
      tagOwner(currentPath);
    }
    currentPath=null;shapeStart=null;saveActiveLayerFrame();updateUI();
  }else if(state.tool==='select'){
    if(_xform.active){
      _xform.active=false;
      var xLd2=state.layers[state.activeLayerIdx];
      if(xLd2&&xLd2.symbolId){
        loadFrame(state.currentFrame);
        selectedPaths=userLayers[state.activeLayerIdx].children.filter(function(c){return (c instanceof Path||c instanceof Raster)&&!(c.data&&(c.data.isLinkedFillCompanion||c.data.isBrushTextureCopy));});
        state.selectedStrokeIndices=[];
      }else{
        fillRegenerateLinked(userLayers[state.activeLayerIdx],null);saveActiveLayerFrame();
      }
      renderTransformHandles();renderNodeHandles();updateUI();
    }
    else if(_marquee.active){
      if(_marquee.rect){
        var mb=_marquee.rect.bounds;
        var layer2=userLayers[state.activeLayerIdx];
        layer2.children.forEach(function(c){
          if(((c instanceof Path&&c.segments.length>0&&(c.strokeColor||c.fillColor))||c instanceof Raster)&&mb.intersects(c.bounds)){
            if(selectedPaths.indexOf(c)<0)selectedPaths.push(c);
          }
        });
        state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
        _marquee.rect.remove();_marquee.rect=null;
      }
      _marquee.active=false;renderArcs();updateUI();
    }
    else if(draggingArc){draggingArc=null;generateTweens();}else if(selectedPaths.length>0){
      _moveDragStarted=false;
      var mLd2=state.layers[state.activeLayerIdx];
      if(mLd2&&mLd2.symbolId){
        loadFrame(state.currentFrame);
        selectedPaths=userLayers[state.activeLayerIdx].children.filter(function(c){return (c instanceof Path||c instanceof Raster)&&!(c.data&&(c.data.isLinkedFillCompanion||c.data.isBrushTextureCopy));});
        state.selectedStrokeIndices=[];
      }else{
        fillRegenerateLinked(userLayers[state.activeLayerIdx],null);saveActiveLayerFrame();
      }
    }
  }else if(state.tool==='subselect'){
    if(_nmq.active){
      if(_nmq.rect){
        var nmb=_nmq.rect.bounds;
        var npath=nodeEditTargetPath();
        _nodeSel=[];
        if(npath){
          var nsegs=nodeEditSegmentsData(npath);
          nsegs.forEach(function(s,i){if(nmb.contains(new Point(s.point[0],s.point[1])))_nodeSel.push(i);});
        }
        _nmq.rect.remove();_nmq.rect=null;
        if(!_nodeSel.length)clearSel();
      }else{clearSel();}
      _nmq.active=false;renderArcs();updateUI();
    }
    else if(_nodeDrag.active){var editedPath=_nodeDrag.path;_nodeDrag.active=false;_nodeDrag.path=null;
      fillRegenerateLinked(userLayers[state.activeLayerIdx],editedPath);
      regenerateBrushTexture(editedPath,userLayers[state.activeLayerIdx]);
      saveActiveLayerFrame();renderNodeHandles();updateUI();}
  }
}
var _eraserCursor=null;
function eraseUpdateCursor(event){
  if(_eraserCursor){_eraserCursor.remove();_eraserCursor=null;}
  if(state.tool!=='eraser'||!event)return;
  var prevA=project.activeLayer;marqueeLayer.activate();
  _eraserCursor=new Path.Circle({center:event.point,radius:state.eraserSize/2,strokeColor:'rgba(255,255,255,.9)',strokeWidth:1/view.zoom,fillColor:new Color(1,1,1,0.12),insert:true});
  _eraserCursor.guide=true;
  prevA.activate();
}
function onMouseMoveTool(event){
  if(state.tool==='eraser'){eraseUpdateCursor(event);return;}
  if(_eraserCursor){_eraserCursor.remove();_eraserCursor=null;}
  if(state.tool!=='pen'||!_pen.path)return;
  if(_pen.previewLine){_pen.previewLine.remove();_pen.previewLine=null;}
  var seg=_pen.path.lastSegment;
  _pen.previewLine=new Path.Line({from:seg.point,to:event.point,strokeColor:'rgba(120,170,255,.6)',strokeWidth:1/view.zoom});
  _pen.previewLine.dashArray=[4/view.zoom,3/view.zoom];
  _pen.previewLine.guide=true;
}
// Double-click a filled shape to also select the stroke(s) that bound it —
// matches Animate's "double-click a fill selects its surrounding stroke"
// convention. There's no stored link between a fill (built by the Fill
// tool) and the strokes that formed it, so this uses a bounds-overlap
// heuristic: any stroke-only path whose bounds intersect the fill's bounds
// is assumed to be part of its outline.
function onViewDoubleClick(event){
  if(state.tool!=='select')return;
  var layer=userLayers[state.activeLayerIdx];
  var hit=layer.hitTest(event.point,{fill:true,tolerance:4/view.zoom});
  if(!hit||!(hit.item instanceof Path)||!hit.item.fillColor)return;
  var fillPath=hit.item;
  if(selectedPaths.indexOf(fillPath)<0)selectedPaths.push(fillPath);
  // strokeBounds (not plain bounds) includes stroke-width padding — a
  // perfectly axis-aligned line has zero-height/width *geometric* bounds,
  // so a straight stroke lying exactly on the fill's edge would otherwise
  // count as merely touching (not intersecting) and get missed. Padding by
  // a couple of pixels on top of that covers any remaining tolerance gap.
  var fb=(fillPath.strokeBounds||fillPath.bounds).expand(4/view.zoom);
  layer.children.forEach(function(c){
    if(c instanceof Path&&c!==fillPath&&c.strokeColor){
      var cb=(c.strokeBounds||c.bounds);
      if(cb.intersects(fb)&&selectedPaths.indexOf(c)<0)selectedPaths.push(c);
    }
  });
  state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
  renderArcs();updateUI();
}
view.onMouseDown=onMouseDown;view.onMouseDrag=onMouseDrag;view.onMouseUp=onMouseUp;view.onMouseMove=onMouseMoveTool;view.onDoubleClick=onViewDoubleClick;
// Zoom-to-pointer: the fixed ±8%-per-event step (old code) felt identical
// for a single notchy mouse-wheel click and a fast trackpad fling — every
// event was the same size regardless of how hard/fast the gesture was.
// Scaling the exponent by the actual delta magnitude (clamped, so one wild
// event can't jump too far) makes gentle scrolls fine-grained and fast
// scrolls ramp up, which is what reads as "fluid" rather than "steppy".
// Trackpad pinch gestures land here too (browsers report them as wheel
// events with ctrlKey=true) — boosted sensitivity for those specifically,
// since they typically report much smaller deltaY per event than a mouse
// wheel notch for the same perceived gesture size.
canvasEl.addEventListener('wheel',function(e){
  e.preventDefault();
  var mag=Math.min(1,Math.abs(e.deltaY)/(e.ctrlKey?40:100));
  var pct=(e.ctrlKey?0.06:0.10)*mag;
  var f=e.deltaY>0?(1-pct):(1+pct);
  var nz=Math.max(.05,Math.min(20,view.zoom*f));
  var mp=view.viewToProject(new Point(e.offsetX,e.offsetY));
  view.zoom=nz;
  var nm=view.viewToProject(new Point(e.offsetX,e.offsetY));
  view.center=view.center.add(mp.subtract(nm));
  updZoom();renderArcs();
  if(window.SMEngineBridge&&window.SMEngineBridge.renderNow)window.SMEngineBridge.renderNow();
},{passive:false});
