// ---- TOOLS ----
var currentPath=null,selectedPaths=[],stabQueue=[],shapeStart=null;
var _textDragStart=null,_textDragRect=null;
// Shift-constrain helpers for Rectangle/Ellipse/Line (see their onMouseDrag
// handler) — kept standalone rather than inlined since both the shape and
// its live drag-preview need the identical constrained endpoint.
function constrainSquare(start,pt){
  var dx=pt.x-start.x,dy=pt.y-start.y;var d=Math.max(Math.abs(dx),Math.abs(dy));
  return new Point(start.x+(dx<0?-d:d),start.y+(dy<0?-d:d));
}
function constrainAngle45(start,pt){
  var dx=pt.x-start.x,dy=pt.y-start.y;var dist=Math.sqrt(dx*dx+dy*dy);
  var step=Math.PI/4,angle=Math.round(Math.atan2(dy,dx)/step)*step;
  return new Point(start.x+Math.cos(angle)*dist,start.y+Math.sin(angle)*dist);
}
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
// Multi-select array (2026-07, "impossible de faire de la multiselection
// avec fill/stroke select + shift... lasso et rectangle") — each entry is
// {path, kind:'fill'|'stroke'|'fillregion', segStart, segEnd, closed}, same
// shape fsHitTest always returned; single-select is just the length-1 case.
var _fsSel=[];
function fsClearSel(){_fsSel=[];}
var _fsIsolation=null; // {groupId,path}; entered by Select double-click
// Last-touched entry — every place that used to read _fsSel.kind/.path
// directly for a SINGLE-value display (panel color swatch, header text)
// keeps doing that against this one, while actions (recolor/delete/opacity)
// loop over the whole _fsSel array.
function fsPrimarySel(){return _fsSel.length?_fsSel[_fsSel.length-1]:null;}
function fsSelectionAtPoint(pt){
  for(var i=_fsSel.length-1;i>=0;i--){
    var hi=fsHighlightPath(_fsSel[i]);
    if(!hi)continue;
    var hit=false;
    try{
      hit=(_fsSel[i].kind==='stroke')
        ? !!hi.hitTest(pt,{stroke:true,tolerance:8/view.zoom})
        : (hi.contains(pt)||!!hi.hitTest(pt,{fill:true,tolerance:2/view.zoom}));
    }catch(e){hit=false;}
    hi.remove();
    if(hit)return _fsSel[i];
  }
  return null;
}
// Promote the selected fill/stroke fragments to ordinary standalone paths.
// Once promoted, the regular Select tool supplies the complete move/scale/
// rotate UI instead of maintaining a second, subtly different transform
// implementation for aspect selections.
function fsPromoteSelectionForTransform(layer){
  if(!_fsSel.length)return[];
  var promoted=[];
  // Fill cuts change the source geometry, so realize them first.
  _fsSel.filter(function(s){return s.kind==='fillregion';}).forEach(function(s){
    var r=fsRealizeFillRegion(s,layer);
    if(r&&r.path&&promoted.indexOf(r.path)<0)promoted.push(r.path);
  });
  _fsSel.filter(function(s){return s.kind==='fill';}).forEach(function(s){
    if(!s.path||s.path.removed)return;
    // Selecting only the fill of a combined fill+stroke shape must not drag
    // its outline along. Split the visual aspects into two real paths.
    if(s.path.strokeColor){
      var fillOnly=s.path.clone({insert:false,deep:true});
      fillOnly.strokeColor=null;
      s.path.fillColor=null;
      if(s.path.data)delete s.path.data.fillGradient;
      fsUnlinkFillRegen(fillOnly);fsUnlinkFillRegen(s.path);
      layer.insertChild(layer.children.indexOf(s.path),fillOnly);
      promoted.push(fillOnly);
    }else if(promoted.indexOf(s.path)<0)promoted.push(s.path);
  });
  // Descending offsets keep earlier offsets valid while an open path is
  // split repeatedly into selected arcs.
  _fsSel.filter(function(s){return s.kind==='stroke'&&s.path&&!s.path.removed;})
    .sort(function(a,b){return b.segStart-a.segStart;})
    .forEach(function(s){
      var p=s.path;
      var whole=s.segStart===0&&Math.abs(s.segEnd-p.length)<0.001;
      if(whole&&p.fillColor){
        var strokeOnly=p.clone({insert:false,deep:true});
        strokeOnly.fillColor=null;
        if(strokeOnly.data)delete strokeOnly.data.fillGradient;
        p.strokeColor=null;
        layer.insertChild(layer.children.indexOf(p)+1,strokeOnly);
        if(promoted.indexOf(strokeOnly)<0)promoted.push(strokeOnly);
      }else{
        var arc=fsRealizeStrokeSegment(s,layer);
        if(arc&&promoted.indexOf(arc)<0)promoted.push(arc);
      }
    });
  return promoted.filter(function(p){return p&&!p.removed;});
}
var _fsPromoteDrag=null;
document.addEventListener('pointerdown',function(e){
  var onStage=e.target===canvasEl||(e.target&&e.target.id==='rust-canvas');
  if(e.button!==0||state.tool!=='fsselect'||!window.SMEngineBridge||!SMEngineBridge.isEnabled()||!onStage)return;
  var w=SMEngineBridge.screenToWorld(e.clientX,e.clientY),pt=new Point(w[0],w[1]);
  var selectedAtPointer=fsSelectionAtPoint(pt);
  if(!selectedAtPointer||e.shiftKey)return;
  pushUndo();
  selectedPaths=fsPromoteSelectionForTransform(userLayers[state.activeLayerIdx]);
  fsClearSel();_fsIsolation=null;
  state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i){return i>=0;});
  window.SM.setTool('select');
  _fsPromoteDrag={last:pt};
  renderTransformHandles();renderNodeHandles();updateUI();
  e.preventDefault();e.stopImmediatePropagation();
},{capture:true});
// Raw pointermove/pointerup listeners for the fsselect marquee/lasso
// (2026-07) — NOT Paper.js Tool's own onMouseDrag/onMouseUp (the branches
// below still handle those, for when the Rust engine is off and Paper's
// own Tool event loop runs normally). Confirmed live: with the engine on
// (the default), Paper's Tool onMouseDrag/onMouseUp never fire at all for
// a real drag gesture on this tool (view.autoUpdate=false likely stops
// Paper's own queued-event dispatch loop) — onMouseDown still fires fine
// (that's what starts the marquee/lasso below), but nothing ever grew or
// resolved it, so a drag-select silently did nothing. select-bridge.js's
// OWN marquee for the Select tool sidesteps this entirely by never
// depending on Paper's Tool system in the first place (raw DOM listeners
// throughout) — mirrored here rather than fighting Paper's dead event
// loop. Harmless alongside the Paper-Tool branches below when the engine
// IS off: whichever resolves first clears _marquee.active, making the
// other a no-op.
document.addEventListener('pointermove',function(e){
  if(_fsPromoteDrag){
    var wm=SMEngineBridge.screenToWorld(e.clientX,e.clientY),pm=new Point(wm[0],wm[1]);
    var delta=pm.subtract(_fsPromoteDrag.last);_fsPromoteDrag.last=pm;
    selectedPaths.forEach(function(p){
      p.position=p.position.add(delta);
      if(p.data&&p.data.isVectorBrush&&p.data.centerSegments)p.data.centerSegments.forEach(function(s){s.point=[s.point[0]+delta.x,s.point[1]+delta.y];});
    });
    renderTransformHandles();renderNodeHandles();SMEngineBridge.renderNow();
    e.preventDefault();e.stopImmediatePropagation();return;
  }
  if(!(_marquee.active&&_marquee.mode==='fsselect')||!window.SMEngineBridge)return;
  var w=window.SMEngineBridge.screenToWorld(e.clientX,e.clientY);
  var pt=new Point(w[0],w[1]);
  var prevA=project.activeLayer;marqueeLayer.activate();
  if(_marquee.lasso){
    if(!_marquee.rect)_marquee.rect=new Path({segments:[_marquee.start],closed:false,strokeColor:'rgba(74,158,255,.9)',strokeWidth:1/view.zoom,dashArray:[4/view.zoom,3/view.zoom],fillColor:new Color(0.29,0.62,1,0.08),insert:true});
    _marquee.rect.add(pt);
  }else{
    var mx1=Math.min(_marquee.start.x,pt.x),my1=Math.min(_marquee.start.y,pt.y);
    var mx2=Math.max(_marquee.start.x,pt.x),my2=Math.max(_marquee.start.y,pt.y);
    if(_marquee.rect)_marquee.rect.remove();
    _marquee.rect=new Path.Rectangle({from:new Point(mx1,my1),to:new Point(mx2,my2),strokeColor:'rgba(74,158,255,.9)',strokeWidth:1/view.zoom,dashArray:[4/view.zoom,3/view.zoom],fillColor:new Color(0.29,0.62,1,0.08),insert:true});
  }
  prevA.activate();
  if(window.SMEngineBridge.renderNow)window.SMEngineBridge.renderNow();
},{capture:true});
function fsStrokeRangesInRegion(path,region){
  var out=[],len=path.length||0;if(len<0.001)return out;
  var steps=Math.max(24,Math.min(600,Math.ceil(len/(3/view.zoom))));
  var runStart=null;
  for(var i=0;i<=steps;i++){
    var off=len*i/steps,pt=path.getPointAt(Math.min(len,off));
    var inside=!!(pt&&region.contains(pt));
    if(inside&&runStart===null)runStart=off;
    if((!inside||i===steps)&&runStart!==null){
      var end=inside?off:Math.max(runStart,len*(i-1)/steps);
      if(end-runStart>0.5/view.zoom)out.push({path:path,kind:'stroke',segStart:runStart,segEnd:end,closed:path.closed});
      runStart=null;
    }
  }
  return out;
}
function fsResolveRegionSelection(region,layer){
  var rb=region.bounds;
  layer.children.forEach(function(c){
    if(!(c instanceof Path)||c.segments.length===0||!(c.strokeColor||c.fillColor))return;
    if(!rb.intersects(c.bounds))return;
    var intersects=false;
    try{intersects=region.intersects(c);}catch(e){}
    var fullyInside=region.contains(c.bounds.topLeft)&&region.contains(c.bounds.topRight)&&
      region.contains(c.bounds.bottomLeft)&&region.contains(c.bounds.bottomRight);
    if(!fullyInside&&!intersects&&!region.contains(c.position))return;
    if(c.fillColor){
      var fillKind=fullyInside?{path:c,kind:'fill'}:{path:c,kind:'fillregion',boolCut:true,cutter:region.clone({insert:false}),inside:true};
      if(!_fsSel.some(function(s){return s.path===c&&s.kind===fillKind.kind;}))_fsSel.push(fillKind);
    }
    if(c.strokeColor){
      var ranges=fullyInside?[{path:c,kind:'stroke',segStart:0,segEnd:c.length,closed:c.closed}]:fsStrokeRangesInRegion(c,region);
      ranges.forEach(function(r){
        if(!_fsSel.some(function(s){return s.path===c&&s.kind==='stroke'&&Math.abs(s.segStart-r.segStart)<0.01&&Math.abs(s.segEnd-r.segEnd)<0.01;}))_fsSel.push(r);
      });
    }
  });
}
document.addEventListener('pointerup',function(e){
  if(_fsPromoteDrag){
    _fsPromoteDrag=null;saveActiveLayerFrame();renderArcs();updateUI();
    if(window.SMEngineBridge)SMEngineBridge.renderNow();
    e.preventDefault();e.stopImmediatePropagation();return;
  }
  if(!(_marquee.active&&_marquee.mode==='fsselect'))return;
  if(_marquee.rect){
    var mbf=_marquee.rect.bounds;
    var lassoF=null;
    if(_marquee.lasso&&_marquee.rect.segments.length>2){_marquee.rect.closePath();lassoF=_marquee.rect;}
    var layerF=userLayers[state.activeLayerIdx];
    fsResolveRegionSelection(lassoF||_marquee.rect,layerF);
    _marquee.rect.remove();_marquee.rect=null;_marquee.mode=null;
  }
  _marquee.active=false;renderArcs();updateUI();
  if(window.SMEngineBridge&&window.SMEngineBridge.renderNow)window.SMEngineBridge.renderNow();
},{capture:true});
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
    if(c===fillPath||!(c instanceof Path)||!c.strokeColor||c.segments.length<2)continue;
    // A CLOSED cutter (e.g. a circle stroked over a filled shape) used to be
    // excluded here entirely — clicking either side of it just selected the
    // WHOLE fill, no split at all, exactly the "fill selection cut by a
    // stroke doesn't select the right zone" complaint. The arc-join trick
    // below (fsBuildFillRegion) only works for an OPEN cutter: it has a
    // single unambiguous connecting arc between the two crossing points. A
    // CLOSED cutter has TWO complementary arcs of its own, same ambiguity as
    // the fill — tried reusing the same join, and it silently built a region
    // using an arbitrary one of those two arcs, which can poke outside the
    // fill's real area entirely (confirmed live: a circle straddling a
    // rectangle's edge produced a "region" bounded by the circle's OUTSIDE
    // half). Paper's own boolean ops have no such ambiguity — intersect() is
    // exactly "the fill piece inside the cutter", subtract() is exactly
    // "the piece outside it" — so a closed cutter gets its own branch here
    // instead of forcing it through the open-cutter arc machinery.
    if(c.closed){
      var ixc;
      try{ixc=fillPath.getIntersections(c);}catch(e){continue;}
      if(!ixc||ixc.length<2)continue; // touches/tangent/fully inside or outside — no real split
      var inside,outside;
      try{inside=fillPath.intersect(c,{insert:false});outside=fillPath.subtract(c,{insert:false});}
      catch(e){continue;}
      // Multi-island result (concave overlap, cutter weaves in/out more than
      // once) is out of scope here — same "real planar-subdivision engine
      // out of scope" boundary the open-cutter path already draws elsewhere
      // in this function; fall back to whole-fill selection for that case.
      var okIn=inside&&inside.segments&&inside.segments.length&&!(inside instanceof CompoundPath);
      var okOut=outside&&outside.segments&&outside.segments.length&&!(outside instanceof CompoundPath);
      if(!okIn||!okOut){if(inside)inside.remove();if(outside)outside.remove();continue;}
      var chosenB=inside.contains(pt)?inside:(outside.contains(pt)?outside:null);
      if(!chosenB){inside.remove();outside.remove();continue;}
      var otherB=chosenB===inside?outside:inside;
      otherB.remove();
      return{regionPath:chosenB,cutter:c,boolCut:true,inside:chosenB===inside};
    }
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
  // Fallback: a fill whose own OUTLINE self-intersects, with no separate
  // stroke object anywhere nearby to act as a cutter ("une ouverture sans
  // stroke et des intersection" — 2026-07 feedback: "il ne sélectionne pas
  // la partie fill avec intersection mais l'ensemble"). The loop above only
  // ever looks for a DIFFERENT path to split against; it never asked
  // fillVectorFind (the paint-bucket tool's own wall/self-crossing tracer,
  // which already explicitly excludes a self-intersecting whole shape from
  // winning as its own candidate — see fill.rs's poly_self_intersects) to
  // isolate just the clicked lobe. Reusing that shared engine instead of a
  // second region algorithm, per this repo's own duplication-hazard
  // convention.
  //
  // Only attempted when fillPath actually self-intersects — Path#area is a
  // plain shoelace sum, which nets out near ZERO for a self-intersecting
  // outline (opposite-winding lobes cancel each other out), so comparing it
  // against the traced lobe's area to detect "did this really split
  // something" is unusable here (tried that first, it broke — a real bowtie
  // fill's own .area came back ~0, making even a correct ~half-size lobe
  // look "not smaller"). Trusting fillVectorFind unconditionally once
  // self-intersection is confirmed is safe: the whole self-intersecting
  // shape is disqualified from ever winning as its own candidate inside
  // fill.rs, so any result it returns here is necessarily a genuine
  // sub-lobe, never a same-size re-find of fillPath itself.
  var selfIx=null;
  try{selfIx=fillPath.getIntersections(fillPath);}catch(e){selfIx=null;}
  if(selfIx&&selfIx.length&&typeof fillVectorFind==='function'){
    var selfRes=null;
    try{selfRes=fillVectorFind(pt,layer,null,undefined,null);}catch(e){selfRes=null;}
    if(selfRes&&selfRes.path)return{regionPath:selfRes.path,selfTrace:true,clickPt:[pt.x,pt.y]};
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
  if(sel.selfTrace){
    var traced=null;
    try{traced=fillVectorFind(new Point(sel.clickPt[0],sel.clickPt[1]),layer,null,undefined,null);}catch(e){traced=null;}
    var lobe=traced&&traced.path,rest=null;
    if(lobe){try{rest=fillPath.subtract(lobe,{insert:false});}catch(e){rest=null;}}
    if(!lobe||!rest||!((rest instanceof CompoundPath&&rest.children.length)||(rest.segments&&rest.segments.length))){
      // Geometry changed since selection — same staleness bail every other
      // branch here already uses rather than leave a half-edit.
      if(lobe)lobe.remove();if(rest)rest.remove();
      return{path:fillPath,kind:'fill'};
    }
    lobe.fillColor=fillPath.fillColor;lobe.strokeWidth=fillPath.strokeWidth;
    lobe.strokeColor=null; // matches this branch's own precondition (fillPath has no stroke — see this function's header comment)
    fsUnlinkFillRegen(lobe);
    layer.insertChild(idx,lobe);
    // rest can legitimately come back as a CompoundPath (the self-crossing
    // outline's OTHER half, subtracted against a self-intersecting operand
    // — Paper.js's own boolean ops have no obligation to return a single
    // simple contour there) — insertBooleanResult (this file, used
    // everywhere else a boolean op's result needs inserting) already knows
    // how to explode that into independent flat Paths rather than this
    // branch needing its own second copy of that logic.
    insertBooleanResult(layer,idx,rest,fillPath.fillColor,fillPath.opacity,null,fillPath.data);
    fillPath.remove();
    return{path:lobe,kind:'fill'};
  }
  if(sel.boolCut){
    var inside,outside;
    try{inside=fillPath.intersect(sel.cutter,{insert:false});outside=fillPath.subtract(sel.cutter,{insert:false});}
    catch(e){inside=null;outside=null;}
    if(!inside||!outside||inside instanceof CompoundPath||outside instanceof CompoundPath){
      // Cutter moved/geometry changed since selection (same staleness case
      // fsHighlightPath already falls back on) — bail without touching the
      // layer rather than leave it half-edited.
      if(inside)inside.remove();if(outside)outside.remove();
      return{path:fillPath,kind:'fill'};
    }
    inside.fillColor=fillPath.fillColor;inside.strokeColor=fillPath.strokeColor;inside.strokeWidth=fillPath.strokeWidth;
    outside.fillColor=fillPath.fillColor;outside.strokeColor=fillPath.strokeColor;outside.strokeWidth=fillPath.strokeWidth;
    fsUnlinkFillRegen(inside);fsUnlinkFillRegen(outside);
    layer.insertChild(idx,inside);layer.insertChild(idx,outside);
    fillPath.remove();
    return{path:sel.inside?inside:outside,kind:'fill'};
  }
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
      if(region){
        region.regionPath.remove();
        if(region.selfTrace)return{path:fp,kind:'fillregion',selfTrace:true,clickPt:region.clickPt};
        if(region.boolCut)return{path:fp,kind:'fillregion',boolCut:true,cutter:region.cutter,inside:region.inside};
        return{path:fp,kind:'fillregion',boundaryStart:region.boundaryStart,boundaryEnd:region.boundaryEnd,cutter:region.cutter,cutterA:region.cutterA,cutterB:region.cutterB};
      }
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
    if(sel.selfTrace){
      var selfHi=null;
      try{selfHi=fillVectorFind(new Point(sel.clickPt[0],sel.clickPt[1]),sel.path.parent,null,undefined,null);}catch(e){selfHi=null;}
      if(!selfHi||!selfHi.path)return sel.path.clone({insert:false,deep:false}); // geometry changed since selection
      var out2=selfHi.path.clone({insert:false,deep:false});
      selfHi.path.remove();
      return out2;
    }
    if(sel.boolCut){
      var boolRegion;
      try{boolRegion=sel.inside?sel.path.intersect(sel.cutter,{insert:false}):sel.path.subtract(sel.cutter,{insert:false});}
      catch(e){boolRegion=null;}
      if(!boolRegion||!boolRegion.segments||!boolRegion.segments.length||boolRegion instanceof CompoundPath){
        if(boolRegion)boolRegion.remove();
        return sel.path.clone({insert:false,deep:false}); // cutter moved/geometry changed since selection — fall back to whole fill
      }
      return boolRegion;
    }
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
  if(!_fsSel.length)return;
  pushUndo();
  var layer=userLayers[state.activeLayerIdx];
  _fsSel.slice().forEach(function(sel){
    if(sel.kind==='fillregion')sel=fsRealizeFillRegion(sel,layer);
    if(sel.kind==='fill'){
      var p=sel.path;
      fsUnlinkFillRegen(p);
      if(!markDeleteAsRevision(p)){ // foreign-owned fill: ghosted in place, keep its color so the ghost still reads correctly
        p.fillColor=null;
        if(!p.strokeColor)p.remove();
      }
      saveActiveLayerFrame();
    }else{
      fsDeleteSegment(sel,layer);
    }
  });
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

// Splits sel.path at segStart/segEnd exactly like fsDeleteSegment, but
// instead of discarding the selected arc it KEEPS both pieces — the arc as
// an independent Path (returned, so the caller can restyle just that piece)
// and the remainder left in the layer with the original path's style
// untouched. Mirrors fsRealizeFillRegion's role for the fill/fillregion
// case: materializes a SECTION selection into a real standalone Path
// before the caller mutates its style, so e.g. a stroke-color change never
// bleeds onto the rest of the stroke (the bug reported in issue #12).
function fsRealizeStrokeSegment(sel,layer){
  var path=sel.path;
  if(sel.segStart===0&&sel.segEnd===path.length&&!(path.closed&&sel.segEnd<sel.segStart)){
    return path; // whole stroke, no crossings -- nothing to split off
  }
  if(path.closed){
    var loopLen=path.length;
    path.splitAt(path.getLocationAt(sel.segStart)); // re-bases the seam to segStart; still closed, still one object
    var newEndOffset=((sel.segEnd-sel.segStart)+loopLen)%loopLen;
    if(newEndOffset<=0.001)newEndOffset=loopLen; // selected arc is the WHOLE (now-open) loop
    path.splitAt(path.getLocationAt(newEndOffset)); // now open (re-based+split) -> real split: path=[segStart..segEnd] (the arc), remainder=[segEnd..segStart] (kept tail, stays in the layer with its original style)
    return path; // `path` is now just the selected arc, open
  }
  var tail=path.splitAt(path.getLocationAt(sel.segEnd)); // path=[start..segEnd] (kept head + arc), tail=[segEnd..originalEnd] (kept, stays in the layer)
  if(tail&&tail.length<0.001)tail.remove(); // segEnd was the original end -> degenerate tail, drop it
  var arc=path.splitAt(path.getLocationAt(sel.segStart)); // path=[start..segStart] (kept head), arc=[segStart..segEnd] (the selected arc, RETURNED)
  if(arc){
    if(path.length<0.001)path.remove(); // segStart was 0 -> "head" is degenerate, drop it
    return arc;
  }
  return path; // segStart was 0: Paper's splitAt no-ops at the exact start, so `path` itself IS the arc (no separate head ever existed)
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
// Fill tool Alt+drag "closing stroke": a temporary, visual-only guide line
// the user draws to bridge a region the fill engine's own crossing/gap
// detection can't close on its own — never added to the document, just
// queued here until a fill click consumes (or a tool change discards) it.
// Each queued stroke carries a stable id (assigned as its data.strokeId
// when materialized — see fillMaterializeTempCloseStrokes) so a fill click
// can tell, via the winning result's wallIds, exactly WHICH queued
// strokes it actually used and only clear those — drawing several closing
// strokes for different regions and filling them one at a time keeps the
// others queued instead of discarding everything on the first click.
// See onMouseDown's 'fill' branch, fillMaterializeTempCloseStrokes, and
// fillCloseOverlayItems.
var _fillCloseDrag=null; // {points:[Point,...]} while an alt-drag is in progress
var _fillCloseIdCounter=0;
var _fillCloseStrokes=[]; // [{id, points:[[x,y],...]}, ...] queued completed strokes, world coords
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
// ---- Subselect Position/Size/Rotation panel (2026-07, "la position du
// panneau doivent driver les vertices... pareil pour size... et rotation
// aussi") — the right panel's transform fields always read/wrote the
// WHOLE selected path's bounds (xformSelBounds/selPropsApplyMove/Scale/
// Rotate below), even under the Subselect tool with individual vertices
// picked out of _nodeSel. These are the vertex-scoped equivalents,
// dispatched to from timeline.js's wireLiveXformField wrappers whenever
// state.tool==='subselect'&&_nodeSel.length — same real Rectangle return
// shape as xformSelBounds() (so xformAnchorPoint/.topLeft etc. all still
// work unchanged) and the same commit tail (saveActiveLayerFrame/
// renderNodeHandles/renderArcs/updateUI) subselect-bridge.js's own drag
// handlers already use at gesture end.
function nodeSelBounds(){
  var path=nodeEditTargetPath();
  if(!path||!_nodeSel.length)return null;
  var segs=nodeEditSegmentsData(path);
  var minX=Infinity,minY=Infinity,maxX=-Infinity,maxY=-Infinity;
  _nodeSel.forEach(function(i){
    var s=segs[i];if(!s)return;
    var x=s.point[0],y=s.point[1];
    if(x<minX)minX=x;if(x>maxX)maxX=x;
    if(y<minY)minY=y;if(y>maxY)maxY=y;
  });
  if(minX===Infinity)return null;
  return new Rectangle(minX,minY,maxX-minX,maxY-minY);
}
function nodeSelCommitTail(path){
  // Same commit sequence as subselect-bridge.js's own node-edit tail (the
  // sibling gesture for the same kind of vertex edit) — forking/texture
  // regen/onion-refresh were missing here, so a vertex drag through THIS
  // path silently skipped all three while the other path handled them.
  forkIfForeignOwner(path);
  fillRegenerateLinked(userLayers[state.activeLayerIdx],path);
  if(window.regenerateBrushTexture)regenerateBrushTexture(path,userLayers[state.activeLayerIdx]);
  if(path.data&&path.data.bitmapBrushSpec&&window.SMBitmapBrush)SMBitmapBrush.regenerate(path,userLayers[state.activeLayerIdx]);
  saveActiveLayerFrame();
  renderOS();
  if(window.renderNodeHandles)renderNodeHandles();
  renderArcs();updateUI();
  if(window.SMEngineBridge)SMEngineBridge.renderNow();
}
function nodeSelApplyMove(dx,dy,skipUndo){
  var path=nodeEditTargetPath();
  if((!dx&&!dy)||!path||!_nodeSel.length)return;
  if(!skipUndo)pushUndo();
  if(path.data&&path.data.isVectorBrush&&path.data.centerSegments){
    _nodeSel.forEach(function(i){var cs=path.data.centerSegments[i];if(cs)cs.point=[cs.point[0]+dx,cs.point[1]+dy];});
    rebuildVectorBrushOutline(path);
  }else{
    _nodeSel.forEach(function(i){var sg=path.segments[i];if(sg)sg.point=sg.point.add(new Point(dx,dy));});
  }
  nodeSelCommitTail(path);
}
function nodeSelApplyScale(sx,sy,anchor,skipUndo){
  var path=nodeEditTargetPath();
  if((sx===1&&sy===1)||!path||!_nodeSel.length)return;
  if(!skipUndo)pushUndo();
  if(path.data&&path.data.isVectorBrush&&path.data.centerSegments){
    _nodeSel.forEach(function(i){
      var cs=path.data.centerSegments[i];if(!cs)return;
      cs.point=[anchor.x+(cs.point[0]-anchor.x)*sx,anchor.y+(cs.point[1]-anchor.y)*sy];
    });
    rebuildVectorBrushOutline(path);
  }else{
    _nodeSel.forEach(function(i){
      var sg=path.segments[i];if(!sg)return;
      var p=sg.point;
      sg.point=new Point(anchor.x+(p.x-anchor.x)*sx,anchor.y+(p.y-anchor.y)*sy);
      // Handles are RELATIVE offset vectors (Paper.js convention) — a
      // non-uniform scale scales each axis of the vector directly, no
      // anchor math needed since they're already relative to the point.
      if(sg.handleIn)sg.handleIn=new Point(sg.handleIn.x*sx,sg.handleIn.y*sy);
      if(sg.handleOut)sg.handleOut=new Point(sg.handleOut.x*sx,sg.handleOut.y*sy);
    });
  }
  nodeSelCommitTail(path);
}
function nodeSelApplyRotate(deltaDeg,center,skipUndo){
  var path=nodeEditTargetPath();
  if(!deltaDeg||!path||!_nodeSel.length)return;
  if(!skipUndo)pushUndo();
  var rad=deltaDeg*Math.PI/180,cos=Math.cos(rad),sin=Math.sin(rad);
  function rotPt(x,y){var dx=x-center.x,dy=y-center.y;return[center.x+dx*cos-dy*sin,center.y+dx*sin+dy*cos];}
  function rotVec(x,y){return[x*cos-y*sin,x*sin+y*cos];}
  if(path.data&&path.data.isVectorBrush&&path.data.centerSegments){
    _nodeSel.forEach(function(i){var cs=path.data.centerSegments[i];if(cs)cs.point=rotPt(cs.point[0],cs.point[1]);});
    rebuildVectorBrushOutline(path);
  }else{
    _nodeSel.forEach(function(i){
      var sg=path.segments[i];if(!sg)return;
      var np=rotPt(sg.point.x,sg.point.y);
      sg.point=new Point(np[0],np[1]);
      // Handles are relative vectors too — rotate the vector itself, no
      // center needed (same reasoning as the scale branch above).
      if(sg.handleIn){var nh=rotVec(sg.handleIn.x,sg.handleIn.y);sg.handleIn=new Point(nh[0],nh[1]);}
      if(sg.handleOut){var nh2=rotVec(sg.handleOut.x,sg.handleOut.y);sg.handleOut=new Point(nh2[0],nh2[1]);}
    });
  }
  nodeSelCommitTail(path);
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
  // entree falsy/retiree tolerée : une seule entree morte dans selectedPaths
  // faisait crasher buildSceneJson -> tick() desactivait TOUT le moteur.
  selectedPaths.forEach(function(p){if(!p||!p.bounds)return;b=b?b.unite(p.bounds):p.bounds.clone();});
  return b;
}
// ---- Oriented selection box (2026-07) ----
// "les boîtes de transformation ne tournent pas avec l'objet quand on
// rotate" — the transform box was always the axis-aligned union of
// bounds, so rotating a selection left the box flat (and growing) around
// the tilted object. The box now carries a persistent angle, accumulated
// by every rotate gesture (both the engine select-bridge and the Paper-
// native mirror below) and reset when the selection changes (clearSel).
// orientedSelBox() computes the TIGHT box in de-rotated space: each
// selected path is cloned, counter-rotated, and its exact (curve-aware)
// bounds united — the pivot choice is irrelevant to the box's shape
// (rigid rotation: any pivot only translates the de-rotated cloud), it
// just has to be the SAME one selBoxPt uses to map corners back to world.
//
// The angle lives ON EACH STROKE (data.boxAngle, persisted via serP/desP)
// — not in selection-session state. First version kept it in a global
// reset on deselection, which meant deselect+reselect snapped the box
// straight again ("une fois que l'on tourne la box on désélectionne, on
// retrouve la box droite"). Now a rotated object carries its orientation:
// reselecting shows the tilted box, across saves/reloads too. A MIXED
// selection (strokes rotated by different amounts) falls back to the
// axis-aligned box — no single angle is honest there.
function selBoxAngleOf(){
  if(!selectedPaths.length)return 0;
  var a=null;
  for(var i=0;i<selectedPaths.length;i++){
    var p=selectedPaths[i];if(!p)continue;
    var pa=(p.data&&p.data.boxAngle)||0;
    if(a===null)a=pa;else if(Math.abs(a-pa)>0.01)return 0;
  }
  return a||0;
}
function orientedSelBox(){
  if(!selectedPaths.length)return null;
  var b0=xformSelBounds();if(!b0)return null;
  var ang=selBoxAngleOf();
  if(!ang)return{b:b0,angle:0,pivot:b0.center};
  var pivot=b0.center,b=null;
  selectedPaths.forEach(function(p){
    if(!p||!p.bounds)return;
    var c=p.clone({insert:false});
    c.rotate(-ang,pivot);
    b=b?b.unite(c.bounds):c.bounds.clone();
    c.remove();
  });
  return b?{b:b,angle:ang,pivot:pivot}:null;
}
// De-rotated-space point -> world (rotate by the box angle around its pivot).
function selBoxPt(x,y,box){
  if(!box.angle)return new Point(x,y);
  var r=box.angle*Math.PI/180,c=Math.cos(r),s=Math.sin(r);
  var dx=x-box.pivot.x,dy=y-box.pivot.y;
  return new Point(box.pivot.x+dx*c-dy*s,box.pivot.y+dx*s+dy*c);
}
// Gradient anchors are document geometry just like path vertices. Keep them
// in the same transform stream so a gradient never appears pinned to the
// canvas while its owning shape moves, scales or rotates.
function transformFillGradient(path, pointTransform){
  var g=path&&path.data&&path.data.fillGradient;
  if(!g||!g.from||!g.to)return;
  var a=pointTransform(new Point(g.from[0],g.from[1]));
  var b=pointTransform(new Point(g.to[0],g.to[1]));
  g.from=[a.x,a.y];g.to=[b.x,b.y];
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
    transformFillGradient(p,function(pt){return pt.add(d);});
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
// A freely-placed anchor (Alt+click with the Select tool, see select-
// bridge.js) overrides the 9-dot preset grid entirely — set by world-space
// coordinates (state.xformAnchorCustom, [x,y]) rather than a bounds-relative
// key, since the whole point is letting it sit somewhere the preset grid
// can't reach. Cleared by clearSel() (new selection = stale anchor is
// meaningless) and by explicitly picking a preset dot again (timeline.js).
function xformAnchorPoint(b){
  if(state.xformAnchorCustom)return new Point(state.xformAnchorCustom[0],state.xformAnchorCustom[1]);
  return b[XFORM_ANCHOR_PROP[state.xformAnchorKey]||'center'];
}
function renderTransformHandles(){
  xformLayer.removeChildren();xformHandles=[];
  if(state.tool!=='select'||!selectedPaths.length)return;
  var box=orientedSelBox();if(!box)return;
  var b=box.b;
  xformLayer.activate();var zs=1/view.zoom;
  // Live corner-pin distort (2026-07 feedback: "la bounding box ne reflete
  // pas cette transformation") — same live-quad treatment as
  // engine-bridge.js's buildTransformBoxItems, mirrored here for the
  // Paper-native fallback path (see that function's own parity comment).
  var liveDistort=window.SMSelectBridge&&SMSelectBridge.getDistortState();
  if(liveDistort&&liveDistort.quad){
    var dq=liveDistort.quad;
    var dPath=new Path({segments:[dq.nw,dq.ne,dq.se,dq.sw],closed:true,strokeColor:'rgba(74,158,255,.8)',strokeWidth:1*zs,dashArray:[4*zs,3*zs],insert:true});
    ['nw','ne','se','sw'].forEach(function(k){
      var pos=dq[k],isActive=k===liveDistort.dir;
      new Path.Rectangle({center:pos,size:[(isActive?9:7)*zs,(isActive?9:7)*zs],fillColor:'#ffffff',strokeColor:isActive?'#ff9f0a':'#4a9eff',strokeWidth:1.2*zs,insert:true});
      xformHandles.push({type:'scale',dir:k,pos:pos});
    });
    return;
  }
  var boxRect=new Path.Rectangle({rectangle:b,strokeColor:'rgba(74,158,255,.8)',strokeWidth:1*zs,dashArray:[4*zs,3*zs],insert:true});
  if(box.angle)boxRect.rotate(box.angle,box.pivot);
  var corners={nw:selBoxPt(b.left,b.top,box),ne:selBoxPt(b.right,b.top,box),sw:selBoxPt(b.left,b.bottom,box),se:selBoxPt(b.right,b.bottom,box),n:selBoxPt(b.center.x,b.top,box),s:selBoxPt(b.center.x,b.bottom,box),e:selBoxPt(b.right,b.center.y,box),w:selBoxPt(b.left,b.center.y,box)};
  Object.keys(corners).forEach(function(k){
    var pos=corners[k];
    // Ctrl-hover corner-pin affordance — same accent as the active-drag
    // highlight above, shown before any drag starts (see select-bridge.js's
    // onMove hover pass for state.xformDistortHoverDir).
    var isDistortHover=state.xformDistortHoverDir===k;
    new Path.Rectangle({center:pos,size:[(isDistortHover?9:7)*zs,(isDistortHover?9:7)*zs],fillColor:'#ffffff',strokeColor:isDistortHover?'#ff9f0a':'#4a9eff',strokeWidth:1.2*zs,insert:true});
    xformHandles.push({type:'scale',dir:k,pos:pos});
  });
  // Rotate RING (2026-07, replaces the old tiny offset stem+dot handle —
  // same formula as select-bridge.js's computeHandles/engine-bridge.js's
  // buildTransformBoxItems, mirrored here for the Paper-native fallback
  // path so both renderers stay visually identical).
  var apPt=(typeof xformAnchorPoint==='function')?xformAnchorPoint(b):b.center;
  // A custom pivot (Alt+click) is ALREADY a world point (xformAnchorPoint's
  // own early-return) — only the bounds-derived preset anchors live in the
  // box's de-rotated space and need selBoxPt's mapping.
  var ringCenter=state.xformAnchorCustom?apPt.clone():selBoxPt(apPt.x,apPt.y,box);
  // Small and mostly size-independent (per user mockup), not scaled to the
  // selection's own bounds — "le cercle de rotation devrait être plus petit".
  var ringRadius=Math.min(36*zs,Math.max(b.width,b.height)*0.3);
  new Path.Circle({center:ringCenter,radius:ringRadius,strokeColor:'rgba(74,158,255,.63)',strokeWidth:1*zs,insert:true});
  xformHandles.push({type:'rotate',center:ringCenter,radius:ringRadius});
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
// Bug found live (2026-07, while testing the new subselect marquee-without-
// pre-selection feature): clearSel() never cleared `nodeHandles` (the
// module-level hit-test array renderNodeHandles() populates) — a stale
// entry from whatever was selected BEFORE this clearSel() call could then
// spuriously match a later click at roughly the same screen position (the
// subselect onDown hit-test loop trusts nodeHandles unconditionally,
// with no check that it still corresponds to the CURRENT selection),
// silently "grabbing" a phantom handle instead of starting a fresh
// marquee/selection. nodeHandles conceptually can't outlive the
// selection it was built from, so it's cleared here too — nodeLayer
// (the Paper-fallback's actual visual dots) already gets wiped by the
// next real renderNodeHandles() call, no need to touch it here.
function clearSel(){selectedPaths=[];state.selectedStrokeIndices=[];_nodeSel=[];state.xformAnchorCustom=null;state.xformAnchorHovered=false;state.xformRingHovered=false;if(typeof nodeHandles!=='undefined')nodeHandles=[];
  // 2026-07 ("si on clic dans le canvas sans rien sélectionner il ne faut
  // pas afficher les options de calque, ce n'est que si on sélectionne des
  // calques dans la timeline"): drop the "an explicit timeline layer click
  // is why layer-sec is showing" flag every time selection resets — clicking
  // empty canvas (which routes through here, select-bridge.js) falls back
  // to the fallback 'document' context in updatePropsContext(), which
  // should then hide layer-sec instead of defaulting to the last-active
  // layer. setActiveLayer (timeline.js) sets the flag back to true right
  // after ITS OWN clearSel() call, so clicking a layer row still works.
  if(typeof window!=='undefined')window._layerActiveExplicit=false;
}
function getSI(path){var ch=userLayers[state.activeLayerIdx].children;for(var i=0;i<ch.length;i++){if(ch[i]===path)return i;}return -1;}
// UI/UX audit (2026-07): the statusbar footer has claimed "⌘/Ctrl+D
// Dupliquer" for as long as an object selection exists, but NOTHING wired
// to that combo ever duplicated the selected shape. Ctrl+D had no handler
// at all (grepped); plain 'd' (no modifier) is duplicateKeyframe — clones
// the WHOLE layer's current-frame content into a NEW frame and jumps the
// playhead there, ignoring selectedPaths entirely. Neither is what a user
// with 1-2 shapes selected reasonably expects from "Dupliquer" in that
// context. This is the real thing: an in-place copy of exactly the
// selected object(s), offset slightly so it's visibly distinct from the
// source, left as the new active selection (every editor's convention).
// Built on serP/desP (the save/load round-trip) rather than Path.clone()
// by hand — it's the one code path already proven to correctly carry
// every field (fill, vector-brush centerline, brush-texture flags...)
// through a rebuild, so this doesn't need to re-solve that.
// Snapshot a LIVE path into plain data a clone can be rebuilt from later —
// possibly much later, on a different frame/layer than the source ever
// existed on (copySelection/pasteSelection below). Captures the two things
// serP() alone can't round-trip because they're live object references
// (data.linkedFill, data.brushCompanions), same reasoning as app.js's own
// linkedFillId/brushGroupId comments. Returns null for a dab (isBrushTextureCopy)
// — it rides along with its anchor via the anchor's own snapshot, never a
// primary target itself.
function _snapshotForClone(p){
  if(p.data&&p.data.isBrushTextureCopy)return null;
  var snap={d:serP(p)};
  if(p.data&&p.data.isVectorBrush&&p.data.linkedFill&&!p.data.linkedFill.removed){
    snap.vbLinkedFill={fillColor:p.data.linkedFill.fillColor,opacity:p.data.linkedFill.opacity};
  }
  if(p.data&&p.data.brushGroupId){
    snap.dabs=(p.data.brushCompanions||[]).filter(function(dab){return dab&&!dab.removed;}).map(serP);
  }
  return snap;
}
// Rebuild live Path clones from _snapshotForClone() output into `layer`,
// offset by `offset` px on both axes (0 = paste exactly in place). Shared by
// duplicateSelection (snapshots taken and materialized in the same call) and
// copySelection/pasteSelection (snapshots persisted in between, possibly
// across a frame/layer change) — a single code path so brush-texture dabs
// and vector-brush fill backdrops can't drift out of sync between the two
// features (see CLAUDE.md §1's "family of bug #1").
function _materializeClones(snaps,layer,offset){
  var clones=[];
  // Brush-texture anchors share a brushGroupId with their dab companions
  // (tools.js applyBrushTexture / app.js relinkBrushCompanions) — every
  // cloned member of a group must land on the SAME fresh id, or
  // relinkBrushCompanions can't tell the new anchor and its new dabs
  // apart from the originals.
  var groupIdMap={};
  function freshGroupId(oldGid){
    if(!groupIdMap[oldGid])groupIdMap[oldGid]='bg_'+Date.now()+'_'+Math.floor(Math.random()*1e6)+'_'+Object.keys(groupIdMap).length;
    return groupIdMap[oldGid];
  }
  snaps.forEach(function(snap){
    var d=JSON.parse(JSON.stringify(snap.d));
    d.strokeId=undefined; // fresh identity below — must NOT alias the source's
    if(d.brushGroupId)d.brushGroupId=freshGroupId(d.brushGroupId);
    var clone=desP(d,layer,d.opacity);
    if(offset)clone.translate(new Point(offset,offset));
    ensureStrokeId(clone);
    if(d.isVectorBrush&&d.centerSegments){
      clone.data.centerSegments=JSON.parse(JSON.stringify(d.centerSegments)).map(function(s){if(offset){s.point[0]+=offset;s.point[1]+=offset;}return s;});
      // Vector-brush fill backdrop (draw-bridge.js) — regenerated from the
      // (now-offset) centerline via the same rebuild every other edit to
      // this stroke type already goes through, rather than hand-cloning
      // its geometry.
      if(snap.vbLinkedFill){
        var fillClone=new Path();fillClone.fillColor=snap.vbLinkedFill.fillColor;fillClone.strokeColor=null;fillClone.opacity=snap.vbLinkedFill.opacity;
        fillClone.insertBelow(clone);
        clone.data.linkedFill=fillClone;
        // Fresh stable id pair (see draw-bridge.js's own comment) — must NOT
        // reuse the source's linkedFillId, same reasoning as strokeId a few
        // lines up (freshly-cloned content is a distinct stroke).
        clone.data.linkedFillId=ensureStrokeId({data:{}});
        fillClone.data.isLinkedFillCompanion=true;
        fillClone.data.linkedFillId=clone.data.linkedFillId;
        if(typeof rebuildVectorBrushOutline==='function')rebuildVectorBrushOutline(clone);
      }
    }
    clones.push(clone);
  });
  // Dabs of any cloned brush-texture anchor — cloned the same way, tagged
  // with the anchor's fresh group id, so relinkBrushCompanions can regroup
  // them below exactly like it does after a normal frame load.
  snaps.forEach(function(snap){
    if(!snap.dabs||!snap.dabs.length)return;
    var newGid=groupIdMap[snap.d.brushGroupId];if(!newGid)return;
    snap.dabs.forEach(function(dabD){
      var dd=JSON.parse(JSON.stringify(dabD));
      dd.brushGroupId=newGid;
      var dabClone=desP(dd,layer,dd.opacity);
      if(offset)dabClone.translate(new Point(offset,offset));
      clones.push(dabClone);
    });
  });
  if(typeof relinkBrushCompanions==='function')relinkBrushCompanions(layer);
  if(typeof fillRegenerateLinked==='function')fillRegenerateLinked(layer,null);
  return clones;
}
function duplicateSelection(){
  if(!selectedPaths.length)return;
  pushUndo();
  var layer=userLayers[state.activeLayerIdx];
  var snaps=selectedPaths.map(_snapshotForClone).filter(Boolean);
  var clones=_materializeClones(snaps,layer,12); // px — small, deliberate nudge so the copy doesn't sit invisibly on top of the source
  clearSel();
  selectedPaths=clones.filter(isSelectablePathChild);
  state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
  saveActiveLayerFrame();renderArcs();updateUI();
  if(window.SMEngineBridge)SMEngineBridge.renderNow();
}
// ---- CANVAS ELEMENT COPY / CUT / PASTE (2026-07) ----
// UI/UX audit: ⌘C/⌘X/⌘V existed ONLY for timeline keyframes (copyFrames/
// cutFrames/pasteFrames, timeline.js) — there was no way to copy a shape
// selection at all, only duplicateSelection's immediate in-place clone
// (⌘D). Built on the same _snapshotForClone/_materializeClones pair so a
// brush-textured or linked-fill stroke behaves identically whether it's
// duplicated, copied, or cut — one clone path, not two that can drift
// apart (see CLAUDE.md §1). The clipboard remembers WHERE it was copied
// from so pasteSelection can tell "same frame" (offset, so the paste is
// visibly distinct from the source, matching duplicateSelection) from
// "different frame/layer" (paste exactly in place — there's no source
// directly underneath to be confused with).
var _canvasClip=null; // {snaps, layerIdx, frameIdx}
function copySelection(){
  if(!selectedPaths.length)return;
  var snaps=selectedPaths.map(_snapshotForClone).filter(Boolean);
  if(!snaps.length)return;
  _canvasClip={snaps:snaps,layerIdx:state.activeLayerIdx,frameIdx:state.currentFrame};
  if(typeof window!=='undefined')window._lastClipKind='canvas';
  showToast('Copié ('+snaps.length+')');
}
function cutSelection(){
  if(!selectedPaths.length)return;
  copySelection();
  window.SM.deleteSelStrokes(); // pushes its own undo entry
  showToast('Coupé ('+_canvasClip.snaps.length+')');
}
function pasteSelection(){
  if(!_canvasClip||!_canvasClip.snaps.length){showToast('Rien à coller');return;}
  pushUndo();
  var layer=userLayers[state.activeLayerIdx];
  var samePlace=(_canvasClip.layerIdx===state.activeLayerIdx&&_canvasClip.frameIdx===state.currentFrame);
  var clones=_materializeClones(_canvasClip.snaps,layer,samePlace?12:0);
  clearSel();
  selectedPaths=clones.filter(isSelectablePathChild);
  state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
  saveActiveLayerFrame();renderArcs();updateUI();
  if(window.SMEngineBridge)SMEngineBridge.renderNow();
  showToast('Collé ('+clones.filter(isSelectablePathChild).length+')');
}
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
// EXPERIMENTAL (native-video-decode): drawing on a native video layer is
// refused up-front — saveActiveLayerFrame deliberately skips such layers
// (their picture is a decoder-fed engine texture, not frame strokes), so a
// stroke drawn here would VANISH on the next frame navigation. A visible
// refusal beats silent data loss; draw on a normal layer above the video.
// Order bug (found 2026-07 auditing select-bridge.js's transform-box drag,
// live-reported: "si on est sur une frame d'une keyframe prolongée et que
// l'on déplace un objet celui-ci revient en place"): getEffectiveStrokes
// (app.js) short-circuits to `f.strokes` the instant f.isKeyframe is true —
// it only walks BACK to find the inherited keyframe while isKeyframe is
// still false. This function used to set curF.isKeyframe=true BEFORE
// calling getEffectiveStrokes, so the call read curF.strokes back off
// itself — still the held frame's original EMPTY array, not yet
// reassigned — instead of the real inherited content, silently freezing an
// EMPTY keyframe. Confirmed live: a held frame inheriting 1 stroke read
// getEffectiveStrokes()===1 right up until this function ran, which then
// promoted it to isKeyframe:true, strokes:[] — the stroke wasn't moved,
// it was deleted. Must compute the effective content FIRST, while the
// frame still reads as "not yet a keyframe".
// Why the active layer can't take a drawing edit right now, or null if it
// can. ensureKeyframe already refused nativeVideo/montageId WITH a message;
// `locked` and `symbolId` were refused SILENTLY (found 2026-07-26 by a
// tool x condition sweep: all five drawing tools on a locked layer, and all
// five on a component layer, produced zero children, zero saved strokes and
// zero toasts — you draw and nothing whatsoever happens or explains why).
// The component case is the nastier of the two: nothing on the canvas says
// the layer is an instance, so it just reads as the app being broken.
var _editRefusalAt=0;
function editRefusalReason(){
  var ld=state.layers[state.activeLayerIdx];
  if(!ld)return null;
  // symbolId BEFORE locked, deliberately: convertActiveLayerToComponent sets
  // locked=true on the instance, so a component layer is ALWAYS both — and
  // "unlock it with the padlock" is the useless half of the truth. Following
  // that advice unlocks the layer and you still can't draw, you just finally
  // get told why. The component nature is the actual reason.
  if(ld.symbolId)return 'Calque composant — double-clique dessus pour entrer dedans et dessiner';
  if(ld.locked)return 'Calque verrouillé — déverrouille-le (cadenas) pour dessiner dessus';
  if(ld.nativeVideo)return 'Calque vidéo — dessine sur un calque normal au-dessus';
  if(ld.montageId)return 'Calque montage — son contenu s\u2019édite dans le StoryBoard';
  return null;
}
// True when the edit may proceed; otherwise explains ONCE and returns false.
// Throttled: a locked-layer drag re-enters the mousemove guards many times
// per gesture, and one toast per frame would be its own bug.
function canEditActiveLayer(){
  var why=editRefusalReason();
  if(!why)return true;
  var now=Date.now();
  if(now-_editRefusalAt>1500){_editRefusalAt=now;if(window.showToast)showToast(why);}
  return false;
}
window.canEditActiveLayer=canEditActiveLayer;
window.editRefusalReason=editRefusalReason;
function ensureKeyframe(){var ldk=state.layers[state.activeLayerIdx];if(!canEditActiveLayer())return;var curF=ldk.frames[state.currentFrame];if(!curF.isKeyframe&&!curF.isInterpolated){var effStrokes=JSON.parse(JSON.stringify(getEffectiveStrokes(state.activeLayerIdx,state.currentFrame)));curF.isKeyframe=true;curF.strokes=effStrokes;loadFrame(state.currentFrame);syncLinkedKeyframeFolder(state.activeLayerIdx,state.currentFrame);}}

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
// Shadow Brush (2026-07, shadow-brush-bridge.js) — every state.shadowMode
// commit site used to just stamp data.channelTag='shadow' and otherwise
// draw with whatever the normal Stroke/Fill color happened to be. Now that
// there's a dedicated small palette for this (state.shadowPalette, each
// swatch with a stable id), the same tag also needs the ACTIVE swatch's
// color (not the global strokeColor) and its id (data.shadowSwatchId, so a
// future step can group/recolor "all guide lines for this shadow bucket"
// even after the swatch's own color is edited). Pure guide lines have no
// fill — delimiting the eventual fill is the whole point (see the paint-
// bucket workflow the bridge's header comment describes), so fillColor is
// explicitly cleared here too.
function applyShadowBrushTag(p,preferFill){
  if(!p)return;
  p.data=p.data||{};
  p.data.channelTag='shadow';
  if(window.SMShadowBrush){
    var sw=window.SMShadowBrush.activeSwatch();
    // Shadow Brush is construction ink for the fill solver, never a painted
    // ribbon. Convert pressure/Fill-Brush outlines back to their centreline
    // while that centreline still exists, then enforce a visible stroke and
    // no fill for every producer (Draw, Fill Brush, Pen and Shape).
    if(p.data.isVectorBrush&&p.data.centerSegments&&p.data.centerSegments.length){
      var shadowCenter=p.data.centerSegments.map(function(s){return{
        point:[s.point[0],s.point[1]],
        handleIn:s.handleIn?[s.handleIn[0],s.handleIn[1]]:[0,0],
        handleOut:s.handleOut?[s.handleOut[0],s.handleOut[1]]:[0,0],
      };});
      p.removeSegments();
      shadowCenter.forEach(function(s){
        p.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));
      });
      p.closed=false;
      delete p.data.isVectorBrush;delete p.data.centerSegments;delete p.data.widthProfile;
    }
    p.fillColor=null;
    p.strokeColor=sw.color;
    p.strokeWidth=Math.max(1,p.strokeWidth||state.brushSize||3);
    p.strokeCap='round';p.strokeJoin='round';
    p.data.shadowSwatchId=sw.id;
  }
}
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
// Team review only protects against real collaborators — gated on this
// project actually having a shared sync folder configured (Réglages ›
// Collab). Without it, a mismatched ownerId is just local identity drift
// (localStorage cleared, preview reopened on a fresh origin/port — see
// nemo/CLAUDE.md §4 — or the project file opened on another machine solo)
// with nobody else's work to protect. Reported bug: every select-drag of a
// pre-existing object was silently spawning a permanent 35%-opacity ghost
// at its old position ("trace fantôme") purely from this kind of drift,
// since ownerId mismatches were treated as a foreign edit unconditionally.
function isTeamCollabActive(){
  return !!(window.SMProject&&window.SMProject.getSyncFolder&&window.SMProject.getSyncFolder());
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
  if(!isTeamCollabActive()){
    // No shared folder for this project — silently reclaim ownership
    // instead of forking, same treatment as the supervisor branch below.
    path.data.ownerId=state.userProfile.id;
    path.data.ownerName=state.userProfile.name;
    path.data.ownerColor=state.userProfile.color;
    return path;
  }
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
  if(!isTeamCollabActive())return false; // no shared folder — delete outright, same as forkIfForeignOwner's reclaim
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
// Alt+drag closing-stroke overlay (see onMouseDown/onMouseDrag's 'fill'
// branches) — a dashed guide line rendered ONLY through the Rust-engine
// JSON overlay (never a real Paper.js item), same pattern as shape-bridge's
// in-progress shape preview. Cleared automatically once the drag ends.
function fillCloseOverlayItems(){
  if(!_fillCloseDrag||_fillCloseDrag.points.length<2)return[];
  var zs=1/Math.max(0.0001,view.zoom);
  return[{
    segments:_fillCloseDrag.points.map(function(p){return{point:[p.x,p.y]};}),
    closed:false,fillColor:null,
    strokeColor:[255,152,0,230],strokeWidth:2*zs,dashPattern:[6*zs,4*zs],
  }];
}
// Every ALREADY-QUEUED closing stroke (drag finished, not yet consumed by a
// fill click) — unlike fillCloseOverlayItems above (only the drag currently
// in progress, pushed via renderWithOverlayItem's rAF-coalesced overlay
// path during onMove), this is called from buildSceneJson so the dashed
// guide keeps showing on every ORDINARY render too — reported: the line
// was vanishing right after being drawn instead of staying until either a
// fill click uses it or a tool switch discards it (see window.SM.setTool,
// timeline.js, which clears _fillCloseStrokes on leaving 'fill').
function fillCloseStrokesOverlayItems(){
  if(!_fillCloseStrokes.length)return[];
  var zs=1/Math.max(0.0001,view.zoom);
  return _fillCloseStrokes.map(function(entry){
    return{
      segments:entry.points.map(function(p){return{point:p};}),
      closed:false,fillColor:null,
      strokeColor:[255,152,0,230],strokeWidth:2*zs,dashPattern:[6*zs,4*zs],
    };
  });
}
// Discards only the queued closing strokes whose id appears in `wallIds`
// (a just-completed fillVectorFind result's own wallIds — the exact set of
// stroke ids that bounded the winning region) — drawing several closing
// strokes for different zones and filling them one at a time keeps every
// OTHER queued stroke around for its own later click instead of wiping
// all of them the moment any one fill succeeds.
function fillConsumeCloseStrokes(wallIds){
  if(!wallIds||!wallIds.length||!_fillCloseStrokes.length)return;
  var used={};wallIds.forEach(function(id){used[id]=true;});
  _fillCloseStrokes=_fillCloseStrokes.filter(function(entry){return!used[entry.id];});
}
// Turns every queued Alt-drawn closing stroke into a REAL (but disposable)
// Path, inserted into `layer` so fillCollectWalls picks it up as a wall for
// the very next fillVectorFind call — then the caller removes it via
// fillRemoveTempCloseStrokes immediately after, synchronously, before any
// render/save/selection pass ever runs. Because insert-and-remove happens
// within one unbroken synchronous click handler, these paths never reach
// ANY of the layer.children consumers CLAUDE.md warns about (buildSceneJson,
// saveActiveLayerFrame, selectedPaths, serP/desP, tween matching) — no
// special-casing needed there, unlike a tag meant to persist. Each temp
// Path's data.strokeId is pre-set to the queued entry's own stable id
// (ensureStrokeId, tools.js, is a no-op once data.strokeId already exists)
// so the result's wallIds can be traced back to exactly which queued
// closing stroke(s) were actually used — see fillConsumeCloseStrokes.
// Snap each point of the hand-drawn Alt+drag closing line onto nearby REAL
// ink (any existing stroke/fill boundary, never another temp-close line)
// when it runs close enough — the closing gesture is meant to bridge a
// genuine GAP, not to redraw the shape's own edge, but a freehand Alt+drag
// can easily wander well away from the real curve it's running alongside
// (2026-07 feedback: "il n'est pas attaché au stroke alors que celui-ci le
// touche" — confirmed live, a fill boundary sitting up to 228px off the
// real ink at one point because the closing line itself drifted that far).
// Pulling each sample onto the nearest real wall point within SNAP_TOL
// keeps the closing line hugging visible ink wherever it passes near some,
// while leaving it untouched wherever it's genuinely crossing open space
// (nothing within tolerance there, so it stays exactly as drawn).
var FILL_CLOSE_SNAP_TOL=18;
function fillMaterializeTempCloseStrokes(layer){
  var realWalls=layer.children.filter(function(c){
    return c instanceof Path&&(c.strokeColor||c.fillColor||(c.data&&c.data.isVectorBrush))&&!(c.data&&c.data.isFillTempClose)&&c.segments.length>=2;
  });
  return _fillCloseStrokes.map(function(entry){
    var p=new Path({strokeColor:'#000000',strokeWidth:1,fillColor:null});
    entry.points.forEach(function(pt){
      var pos=new Point(pt[0],pt[1]);
      var best=null,bestD=FILL_CLOSE_SNAP_TOL;
      realWalls.forEach(function(w){
        var np=w.getNearestPoint(pos);
        if(!np)return;
        var d=pos.getDistance(np);
        if(d<bestD){bestD=d;best=np;}
      });
      p.add(best||pos);
    });
    p.data.strokeId=entry.id;
    p.data.isFillTempClose=true;
    layer.addChild(p);
    return p;
  });
}
function fillRemoveTempCloseStrokes(paths){
  paths.forEach(function(p){p.remove();});
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
    if(w.closed||w.firstSegment.point.getDistance(w.lastSegment.point)<0.5){
      w.closed=true;closed.push(w);closedSrc.push(sid);
      // ALSO feed this closed outline into the open-wall trace graph, as a
      // ring polyline (opened + explicitly re-closed by appending a copy of
      // its first segment, so its two endpoints coincide and join into one
      // graph node). Closed walls used to exist ONLY as whole-loop
      // candidates — a region bounded partly by a closed shape's outline
      // and partly by open strokes crossing it (the everyday "paint bucket
      // inside a zone traced over an existing filled shape" case) was
      // structurally unfindable, so the bucket silently fell back to the
      // WHOLE closed shape. In the graph, crossings split this ring into
      // arcs exactly like any open wall (build_graph/fill.rs cuts at every
      // crossing), letting hybrid loops (stroke arc + outline arc) win by
      // the existing smallest-area rule. A crossing-less ring degrades to
      // the same whole-loop candidate it already was — no behavior change.
      var ring=w.clone({insert:false});
      ring.closed=false;
      var rf=ring.firstSegment;
      ring.add(new Segment(rf.point.clone(),rf.handleIn.clone(),new Point(0,0)));
      // The first anchor's handleIn belonged to the (now removed) closing
      // curve — the appended seam segment above already took a copy as ITS
      // incoming tangent, so on the first anchor it's stale. Leaving it
      // made any face whose boundary passes THROUGH the seam node carry a
      // zero-length curve with a wild backward handle right at the seam — a
      // spike that flagged the whole face as self-intersecting (confirmed
      // live: a chord-split circle's left half, whose boundary crosses the
      // ring seam, was rejected every time while the seam-free right half
      // worked).
      rf.handleIn=new Point(0,0);
      open.push(ring);openSrc.push(sid);
    }
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
    // Self-loop guard (mirrors build_graph/fill.rs's own a==b `continue`,
    // which the JS side never had — found live on a dense arrangement):
    // a micro sub-wall left by two near-coincident crossing cuts has both
    // endpoints inside one joinEps cluster; as an a===b edge it's always
    // "available" at that node without ever LEAVING it, trapping the
    // sharpest-turn walk until maxSteps kills the trace — 358 of 763 seed
    // walks returned null and no real face survived on the reported scene.
    if(a===b)return;
    // Coincident-duplicate guard (2026-07, found live, kept in sync with
    // build_graph/fill.rs): duplicated wall geometry is STRUCTURAL in this
    // app — a paint-bucket fill's outline literally retraces the stroke
    // centerlines it was traced against (fillVectorFind), so two adjacent
    // fills plus the original stroke put 2-3 identical copies of the same
    // boundary arc into the wall set. Two same-geometry edges between the
    // same node pair form a degenerate two-edge "lens" face the
    // sharpest-turn walk falls into and orbits forever (confirmed live: a
    // 4-hop rogue cycle of two duplicated arc pairs absorbed EVERY trace on
    // the reported scene). Midpoint comparison keeps genuine two-arc
    // lenses (different arcs between the same nodes — the diamond/losange
    // case, which must stay traceable) while dropping exact copies.
    var sm=s.getPointAt(s.length/2);
    for(var d0=0;d0<edges.length;d0++){
      var pe=edges[d0];
      if(pe.type!=='stroke')continue;
      if(!((pe.a===a&&pe.b===b)||(pe.a===b&&pe.b===a)))continue;
      var pm=opens[pe.strokeIdx].getPointAt(opens[pe.strokeIdx].length/2);
      if(pm&&sm&&pm.getDistance(sm)<=Math.max(1.5,joinEps))return;
    }
    edges.push({type:'stroke',strokeIdx:i,a:a,b:b,length:s.length});
  });
  for(var i=0;i<nodes.length;i++){
    for(var j=i+1;j<nodes.length;j++){
      var d=nodes[i].pt.getDistance(nodes[j].pt);
      if(d>joinEps&&d<=gapThr)edges.push({type:'gap',a:i,b:j,length:d});
    }
  }
  // Spur pruning (2026-07, Umoupen-parity — kept in sync with
  // build_graph/fill.rs): iteratively drop every edge with a degree-1
  // endpoint until none remain. A dangling stroke tail — the natural
  // overshoot past the last crossing in ANY hand-drawn intersection, plus
  // whole strokes that connect to nothing — can never bound a face, but
  // the sharpest-turn walk doesn't know that: it happily walks INTO the
  // spur and dies at its tip (`if(!bestEdge)return null`), killing the
  // whole face trace. Confirmed live on a dense real drawing: every trace
  // seeded from a genuine interior face hit some spur within 5-16 hops and
  // returned null — NO real face was ever found, the paint bucket fell
  // back to whatever whole closed wall contained the click. Umoupen's
  // triangulation sidesteps this by construction (dangling segments don't
  // close triangles); pruning is the graph-world equivalent.
  var pruned=true;
  while(pruned){
    pruned=false;
    var degree={};
    edges.forEach(function(e){degree[e.a]=(degree[e.a]||0)+1;degree[e.b]=(degree[e.b]||0)+1;});
    for(var k=edges.length-1;k>=0;k--){
      if(degree[edges[k].a]<2||degree[edges[k].b]<2){edges.splice(k,1);pruned=true;}
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
  // Rho-orbit abort (2026-07, kept in sync with trace_loop/fill.rs): with
  // perfectly consistent per-node angular ordering the sharpest-turn
  // successor is a permutation and every orbit returns to its start — but
  // real node-clustered graphs (joinEps merging, near-parallel edges at
  // shallow-angle crossings) can make two different arrivals pick the same
  // exit, so a walk can merge into a cycle that never includes startNode
  // and spin until maxSteps. Detecting a repeated DIRECTED half-edge
  // proves the orbit is closed without the start — abort immediately
  // instead of burning the remaining steps.
  var seenHE={};seenHE[curEdge.idx+'_'+curNode]=true;
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
    var heKey=bestEdge.idx+'_'+toNode;
    if(seenHE[heKey])return null; // merged into a foreign orbit — see comment above
    seenHE[heKey]=true;
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
// Animate-style bucket stacking: a paint-bucket click inside a region that
// sits ON TOP of an existing opaque fill must produce a fill you can
// actually SEE — the historical insertChild(0, ...) (bottom of the layer,
// so strokes always stay above) painted the new region BEHIND that
// existing fill, i.e. invisibly, whenever the clicked region overlapped
// one ("ça fill derrière, pas devant"). Animate resolves this because its
// merge-drawing model carves the underlying fill; without that rework
// (explicitly out of scope — Sujet #100), the visually-equivalent behavior
// is to insert the new region just ABOVE the topmost fill that covers the
// click point: the region then reads as replaced/painted-over, while
// every stroke and fill stacked higher than that covering fill (the
// boundary strokes the user traced the region with, drawn later) keeps
// its place on top. No covering fill -> index 0, the classic behavior.
function fillInsertIndexFor(layer,pt,excludePath){
  var insertAt=0;
  for(var i=layer.children.length-1;i>=0;i--){
    var c=layer.children[i];
    if(c===excludePath)continue;
    var covers=false;
    if((c instanceof Path||c instanceof CompoundPath)&&c.fillColor){
      try{covers=c.contains(pt);}catch(e){covers=false;}
    }else if(c instanceof Raster){
      covers=c.bounds.contains(pt);
    }
    if(covers){insertAt=i+1;break;}
  }
  return insertAt;
}
// Animate merge-drawing (the fill half of it): a fresh bucket fill that
// touches or overlaps an EXISTING fill of the same color fuses with it into
// one shape — no hairline seam where the two regions abut (two separate
// paths sharing a boundary always show an antialiasing seam, "j'ai une
// séparation"). Only pure fills merge (closed, no strokeColor, none of the
// special-role tags) — a hand-drawn filled+stroked shape keeps its own
// identity, matching Animate, where strokes always survive as dividers.
// The merged result keeps auto-regen tracking, but as data.fillSeeds (an
// ARRAY, one entry per merged participant) rather than a single fillSeed —
// regenerating from just ONE seed would silently shrink the merge back to
// that one participant's own region the next time a bounding stroke moves
// (reported: "je bouge un tracé dont le fill est complètement collé, celui-
// ci ne suit pas"), so fillRegenerateLinked instead re-traces EVERY stored
// seed and re-unions whichever still resolve, keeping the merge alive
// without that shrink risk.
// True if a real, currently-visible stroke (strokeColor item) in the layer
// runs between p1 and p2 — used to tell "these two same-color fills only
// touch because of gap-bridging/rounding" (no real line between them, safe
// to fuse) apart from "a hand-drawn divider sits right between them" (issue
// #10: bucket-filling both sides of a drawn line the same color must NOT
// silently erase that division). Deliberately geometric (a probe segment
// crossing test) rather than comparing the two fills' recorded fillWalls
// ids: when a fill itself becomes a tracing wall for a later click right
// next to it (the "fill inside an already-filled zone" case,
// fillCollectWalls), the trace can end up recording that NEIGHBORING FILL's
// own edge as the wall instead of the real coincident stroke underneath it
// — an id-based check silently misses the divider whenever that happens,
// which is exactly the failure mode a first attempt at this fix hit.
function _strokeBetween(layer,p1,p2){
  if(!p1||!p2)return false;
  var probe=new Path({segments:[p1,p2],insert:false});
  for(var i=0;i<layer.children.length;i++){
    var s=layer.children[i];
    if(!(s instanceof Path)||!s.strokeColor||s.segments.length<2)continue;
    var hit=false;
    try{hit=probe.getIntersections(s).length>0;}catch(e){}
    if(hit)return true;
  }
  return false;
}
// Recolor-in-place (2026-07, Umoupen-parity pass — "le remplacement de fill
// existant par une autre couleur avec le pot de peinture crée un nouveau
// fill alors qu'il faut juste remplacer la couleur parfois"). fillCollectWalls
// deliberately re-opens every closed fill's own outline as a wall (its own
// long comment) specifically so a click bounded partly by an existing
// shape's outline can still retrace a hybrid region — but the everyday
// "just recolor what I clicked on" case (no new subdividing strokes drawn
// at all) also goes through that same path: fillVectorFind re-derives that
// exact shape's own boundary, and the caller then stacked a brand-new Path
// with identical geometry ON TOP of the original instead of recognizing
// "this IS that shape". Detected here cheaply (matching area + bounds
// within a tight tolerance) rather than an expensive boolean-op comparison
// per candidate — a genuinely different, smaller sub-region (new strokes
// actually subdividing the area) will never match and still gets its own
// new shape exactly as before; only a full, unmodified re-trace of an
// existing shape's own outline is redirected to a plain recolor.
function fillFindExistingMatch(layer,path){
  var area=Math.abs(path.area),b=path.bounds;
  if(!(area>0))return null;
  var best=null,bestDiff=Infinity;
  for(var i=0;i<layer.children.length;i++){
    var c=layer.children[i];
    if(!(c instanceof Path||c instanceof CompoundPath)||!c.fillColor)continue;
    if(c.data&&(c.data.isLinkedFillCompanion||c.data.isBrushTextureCopy))continue;
    var cb=c.bounds;
    // Tolerance loosened 1px/1% -> 4px/4% (2026-07, "impossible de mettre
    // un fill... ça déforme complètement la forme"): a re-trace of the
    // SAME shape can drift more than 1% from a prior simplify()/smooth()
    // pass (this exact fill's own _eraseDegenerateSelfLoops cleanup, or an
    // earlier commit's Fill-Brush simplify), narrowly missing this match
    // and falling through to fillMergeSameColor's `.unite()` — which is
    // numerically unsafe for two near-identical overlapping polygons
    // (confirmed directly: a shape unioned with its own clone produced 23
    // garbage fragments). Catching the near-identical case here, before
    // any union is ever attempted, is the safest place to stop it.
    if(Math.abs(cb.x-b.x)>4||Math.abs(cb.y-b.y)>4||Math.abs(cb.width-b.width)>4||Math.abs(cb.height-b.height)>4)continue;
    var diff=Math.abs(Math.abs(c.area)-area);
    if(diff>Math.max(4,area*0.04))continue; // >4% area difference -> a genuinely different region
    if(diff<bestDiff){bestDiff=diff;best=c;}
  }
  return best;
}
// Shared "is this basically the same shape" test (bounds+area proxy, no
// boolean op involved) — used both by fillMergeSameColor's own defense-in-
// depth check (see its call site's comment) and available for any future
// caller needing the same near-identical guard.
function _isNearIdenticalArea(a,b){
  var areaA=Math.abs(a.area),areaB=Math.abs(b.area);
  if(!(areaA>0)||!(areaB>0))return false;
  var maxArea=Math.max(areaA,areaB);
  if(Math.abs(areaA-areaB)>Math.max(4,maxArea*0.04))return false;
  var ba=a.bounds,bb=b.bounds;
  var tol=Math.max(4,Math.max(ba.width,ba.height)*0.04);
  return Math.abs(ba.x-bb.x)<=tol&&Math.abs(ba.y-bb.y)<=tol&&Math.abs(ba.width-bb.width)<=tol&&Math.abs(ba.height-bb.height)<=tol;
}
// preserveFillShapes (2026-07, "les bords extérieurs du fill brush une fois
// le pot de peinture appliqué change de forme comme si des vecteurs
// avaient bougé"): Paper.js's .unite() always re-emits a BRAND NEW point
// set for the WHOLE resulting boundary, never just the touched region —
// there's no way to union two shapes and keep one side's anchors byte-
// identical. That's an acceptable, even wanted, trade-off when the Fill
// Brush tool merges a fresh stroke into the PREVIOUS one it's still
// actively drawing (both are transient, about-to-settle geometry) — but
// NOT when the paint bucket, well after the fact, absorbs an
// already-finished, hand-authored Fill Brush shape it merely happens to
// border: the artist doesn't expect that shape's far, untouched outer
// edge to visibly reshape just because they filled next to it. Pass
// true from the Fill Brush's own commit call sites (unchanged behavior);
// pass false (or omit) from the paint bucket's call site.
function fillMergeSameColor(layer,newFill,allowFillShapeAbsorb){
  if(!newFill||!newFill.fillColor)return newFill;
  var col=colorHex8(newFill.fillColor);
  var absorbed=[];
  layer.children.forEach(function(c){
    if(c===newFill||!(c instanceof Path)||!c.closed||!c.fillColor||c.strokeColor)return;
    if(c.data&&(c.data.isVectorBrush||c.data.isLinkedFillCompanion||c.data.isBrushTextureCopy||c.data.isRevisionGhost||c.data.ghostFrame!==undefined))return;
    if(c.data&&c.data.isFillShape){
      // Still excluded by default (preserveFillShapes, above) — UNLESS this
      // exact Fill Brush shape's own boundary was one of the walls the
      // paint bucket traced against to build newFill (data.fillWalls,
      // stamped on newFill just before this call). That's the "patching a
      // gap in THIS shape's own unclosed loop" case (2026-07 feedback:
      // "le fill ne se merge pas complet avec le fill brush" — a Fill
      // Brush ring gesture that didn't quite close, painted-bucket-filled
      // via Alt+drag) — genuinely the same intended shape, not just an
      // adjacent neighbor it happens to border, so absorbing it here is
      // completing the shape rather than reshaping an unrelated one.
      var tracedAgainstThis=newFill.data&&newFill.data.fillWalls&&c.data.strokeId&&newFill.data.fillWalls.indexOf(c.data.strokeId)>=0;
      if(!allowFillShapeAbsorb&&!tracedAgainstThis)return;
    }
    if(colorHex8(c.fillColor)!==col)return;
    if(!c.bounds.intersects(newFill.bounds))return;
    if(_strokeBetween(layer,newFill.interiorPoint,c.interiorPoint))return;
    var touches=false;
    try{
      touches=newFill.intersects(c)
        ||(c.interiorPoint&&newFill.contains(c.interiorPoint))
        ||(newFill.interiorPoint&&c.contains(newFill.interiorPoint));
    }catch(e){}
    // Near-miss tolerance (2026-07, "je remplis la forme [avec le pot de
    // peinture], il ne l'a remplis pas complétement et n'en fait pas une
    // seule forme"): fillVectorFind's traced boundary (paint bucket) can
    // sit a hairline inside an existing shape's true edge — invisible on
    // screen (graph-tracing flatten/refine tolerance, tools.js's own
    // fillVectorFind comments) but enough to fail Paper.js's strict
    // curve-intersection/contains test above, silently leaving two
    // same-color fills unmerged with a technically-there seam. Falls back
    // to a real nearest-point distance check before giving up — samples
    // points along newFill's own curves (cheap, no dense flatten) and
    // checks how close the OTHER shape's boundary gets to each.
    if(!touches){
      try{
        var TOL=3,curves=newFill.curves;
        for(var ci=0;ci<curves.length&&!touches;ci++){
          var sp=curves[ci].getPointAt(0,true);
          if(!sp)continue;
          var np=c.getNearestPoint(sp);
          if(np&&np.getDistance(sp)<TOL)touches=true;
        }
      }catch(e){}
    }
    if(touches)absorbed.push(c);
  });
  if(!absorbed.length)return newFill;
  // Gather every participant's own regen tracking BEFORE the union — reused
  // below to tag the merged result(s) so the whole merge stays live (see
  // this function's own header comment for why fillSeeds is an array).
  var participants=[newFill].concat(absorbed);
  var mergedSeeds=[],mergedWallIds=[],mergedGapPx=0;
  participants.forEach(function(p){
    if(!p.data)return;
    var ps=p.data.fillSeeds||(p.data.fillSeed?[p.data.fillSeed]:[]);
    ps.forEach(function(s){mergedSeeds.push(s);});
    (p.data.fillWalls||[]).forEach(function(id){if(mergedWallIds.indexOf(id)<0)mergedWallIds.push(id);});
    if(p.data.fillGapPx>mergedGapPx)mergedGapPx=p.data.fillGapPx;
  });
  var acc=newFill,nearIdentical=null;
  absorbed.forEach(function(c){
    // Near-identical to the ORIGINAL newFill (not the evolving acc — a
    // candidate can be near-identical to newFill itself even after acc has
    // already grown from an earlier union in this same loop): re-clicking
    // fill on an already-filled area, when fillFindExistingMatch's tighter
    // upstream tolerance narrowly misses (a hairline of drift from an
    // earlier simplify/smooth pass), reaches here with newFill essentially
    // duplicating an existing same-color shape. Uniting two NEAR-IDENTICAL
    // overlapping polygons is numerically unsafe in Paper.js's clipper —
    // confirmed directly this session: unioning a shape with its own exact
    // clone produced 23 garbage fragments, not one clean shape. `c` already
    // covers essentially the same area newFill does, so it contributes
    // nothing new to union anyway — just drop the redundant duplicate
    // instead of ever feeding this pair through unite() (2026-07: "impossible
    // de mettre un fill... ça déforme complètement la forme").
    if(_isNearIdenticalArea(newFill,c)){nearIdentical=nearIdentical||c;return;}
    var u=null;
    try{u=acc.unite(c,{insert:false});}catch(e){}
    if(!u)return; // degenerate geometry — leave this one unmerged rather than fail the whole fill
    if(acc!==newFill)acc.remove();
    acc=u;
  });
  if(acc===newFill){
    // Every real candidate was either a failed unite or a near-identical
    // duplicate — if it was the latter, the existing shape already covers
    // this area; discard the fragile new trace and keep it, rather than
    // leaving two redundant overlapping copies in the layer.
    if(nearIdentical){newFill.remove();return nearIdentical;}
    return newFill; // every unite failed — nothing changed
  }
  // Strip clipper-noise self-touching revisits before this goes anywhere
  // else (insertBooleanResult below, or a FUTURE merge/regeneration pass
  // that would otherwise unite() this already-messy result again and
  // compound the degeneracy) — see _eraseDegenerateSelfLoops' own comment.
  if(acc instanceof CompoundPath)acc.children.forEach(function(ch){_eraseDegenerateSelfLoops(ch);});
  else _eraseDegenerateSelfLoops(acc);
  var op=newFill.opacity;
  // Highest stacking position among the participants, adjusted for the
  // removals below it — the union visually replaces ALL of them, so it
  // takes the topmost one's place relative to everything else.
  var idxs=[layer.children.indexOf(newFill)].concat(absorbed.map(function(c){return layer.children.indexOf(c);}));
  var topIdx=Math.max.apply(null,idxs);
  var removedBelow=idxs.filter(function(i){return i<topIdx;}).length;
  newFill.remove();absorbed.forEach(function(c){c.remove();});
  var parts=insertBooleanResult(layer,Math.min(topIdx-removedBelow,layer.children.length),acc,newFill.fillColor,op,null,newFill.data);
  if(mergedSeeds.length){
    parts.forEach(function(part){
      part.data.fillSeeds=mergedSeeds;
      if(mergedWallIds.length)part.data.fillWalls=mergedWallIds;
      part.data.fillGapPx=mergedGapPx||24;
    });
  }
  return parts[0]||null;
}
// Regenerates a MULTI-seed (merged/"collé") fill: re-traces every stored
// seed independently and re-unions whichever still resolve, instead of
// collapsing to a single region. A seed that no longer finds anything
// (e.g. the stroke move actually detached that one sub-region) is simply
// dropped rather than aborting the whole merge — same "leave what's still
// valid alone" spirit as the single-seed path below.
function _fillRegenerateMulti(layer,f,onlyIds,gapCap){
  var found=[];
  (f.data.fillSeeds||[]).forEach(function(s){
    var seed=new Point(s[0],s[1]);
    var res=fillVectorFind(seed,layer,f,gapCap,onlyIds);
    if(res)found.push(res);
  });
  if(!found.length)return null;
  if(found.length===1)return{path:found[0].path,wallIds:found[0].wallIds,seeds:[[found[0].path.interiorPoint.x,found[0].path.interiorPoint.y]]};
  var acc=found[0].path;
  var seeds=[];
  found.forEach(function(r){
    if(r!==found[0]){
      var u=null;
      try{u=acc.unite(r.path,{insert:false});}catch(e){}
      if(u){if(acc!==found[0].path)acc.remove();acc=u;r.path.remove();}
      // degenerate unite: keep both un-unioned rather than lose one — acc
      // stays the last good accumulator, r.path is simply left out of it.
    }
  });
  // A CompoundPath here means the sub-regions no longer touch after the
  // edit (unite() couldn't merge them into one seamless outline) — inserting
  // that raw would violate the CompoundPath rule (CLAUDE.md §1: never insert
  // a boolean result directly, always split via insertBooleanResult). Rather
  // than wire full island-splitting through this return path for a rare
  // edge case, fall back to just the largest surviving sub-region: no worse
  // than the legacy single-seed fallback's own "shrinks to one seed" case,
  // and the OTHER seeds are still recorded (see the caller) so a further
  // edit that reconnects them will pick them back up.
  if(acc instanceof CompoundPath){
    var largest=found[0];
    found.forEach(function(r){if(!r.path.removed&&Math.abs(r.path.area)>Math.abs(largest.path.area))largest=r;});
    acc.remove();
    return{path:largest.path,seeds:[[largest.path.interiorPoint.x,largest.path.interiorPoint.y]]};
  }
  found.forEach(function(r){var ip=r.path.interiorPoint;if(ip)seeds.push([ip.x,ip.y]);});
  return{path:acc,seeds:seeds};
}
function fillRegenerateLinked(layer,touchedPath){
  if(!layer)return;
  var fills=layer.children.filter(function(c){return c instanceof Path&&c.data&&(c.data.fillSeed||(c.data.fillSeeds&&c.data.fillSeeds.length));});
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
    var col=f.fillColor,op=f.opacity;
    // With the wall restriction in place the gap cap is unnecessary (see
    // block comment above); without it (legacy fill), the cap remains the
    // only guard against grabbing unrelated strokes, so keep it.
    var gapCap=onlyIds?undefined:f.data.fillGapPx;
    var isMulti=f.data.fillSeeds&&f.data.fillSeeds.length;
    var newSeeds=null;
    var res;
    if(isMulti){
      var mres=_fillRegenerateMulti(layer,f,onlyIds,gapCap);
      if(!mres)return; // none of the merge's seeds still resolve — leave the old fill as-is
      res={path:mres.path,wallIds:mres.wallIds,gapPx:gapCap};
      newSeeds=mres.seeds;
    }else{
      var seed=new Point(f.data.fillSeed[0],f.data.fillSeed[1]);
      res=fillVectorFind(seed,layer,f,gapCap,onlyIds);
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
    }
    var idx=layer.children.indexOf(f);
    f.remove();
    res.path.fillColor=col;res.path.strokeColor=null;res.path.opacity=op;
    if(isMulti){
      // Re-anchor to the union's own sub-region interior points — same
      // self-correcting principle as the single-seed case below, just one
      // per surviving sub-region instead of one overall.
      res.path.data.fillSeeds=newSeeds&&newSeeds.length?newSeeds:[[res.path.interiorPoint.x,res.path.interiorPoint.y]];
    }else{
      // Re-anchor the seed to the NEW region's own interior point rather than
      // keeping the original click position — self-correcting, so the next
      // regeneration starts from wherever the fill actually lives now instead
      // of a possibly-stale original click point that keeps drifting further
      // outside the shape with each subsequent edit.
      var newSeed=res.path.interiorPoint||seed;
      res.path.data.fillSeed=[newSeed.x,newSeed.y];
    }
    res.path.data.fillGapPx=res.gapPx;
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
function _computeExactCrossings(opens,tJunctionTol){
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
    // Self-crossings (i===i): a single freehand stroke that loops back over
    // itself — a hand silhouette's fingers, a cursive/spiral doodle, any
    // shape where the SAME wall crosses its own path — never got a graph
    // node at that junction, since the loop above only ever compares two
    // DIFFERENT walls (j starts at i+1). Without a node there, build_graph
    // (fill.rs) sees one continuous polyline that geometrically crosses
    // itself, traces the only loop available through the click point, and
    // poly_self_intersects correctly (but unhelpfully) rejects it as an
    // invalid bow-tie — indistinguishable from a genuine mistraced loop at a
    // near-tangential junction. Reported: a clearly closed, hand-drawn
    // "hand" outline (fingers necessarily cross the palm's own line)
    // returning "Aucune zone fermée ici" at every gap threshold, even 280px
    // — for a ~108px gap that should close trivially. Splitting the wall at
    // its own self-intersection(s) here, same shape as the cross-wall case,
    // gives Rust the node it needs to route through the junction instead of
    // rejecting the whole loop.
    var selfIx=opens[i].getIntersections(opens[i]);
    selfIx.forEach(function(loc){
      if(!loc.intersection)return;
      var pt=[loc.point.x,loc.point.y];
      crossings.push({wall:i,frac:loc.offset/opens[i].length,pt:pt});
      crossings.push({wall:i,frac:loc.intersection.offset/opens[i].length,pt:pt});
    });
  }
  // T-junction welds (2026-07, Umoupen-parity — "les intersections faites
  // avec l'outil shadow brush ne sont pas prises en compte par le pot de
  // peinture"): a stroke whose ENDPOINT lands on (or a hair short of) the
  // MIDDLE of another wall — the classic way to subdivide a region with a
  // partition line, and exactly what Shadow Brush guide lines do — never
  // produced a graph junction: getIntersections above only reports true
  // transversal crossings, and the graph's endpoint-join/gap-edge machinery
  // only ever connects endpoints to OTHER ENDPOINTS, never to the middle of
  // a wall. Confirmed live: a partition line ending ~1px inside a closed
  // region's boundary left the face unsplit — both sides of the line filled
  // as one identical region. Fix: cut the OTHER wall at the endpoint's
  // nearest-point projection whenever it's within tJunctionTol. That cut
  // becomes a real graph NODE, and the EXISTING machinery does the rest —
  // an exact touch merges with the endpoint via node clustering (join_eps),
  // a small gap connects via a normal gap edge at whatever gapThr
  // escalation step covers it. No new edge type, no Rust change (crossings
  // are computed here and handed to build_graph/fill.rs as-is; a degree-2
  // cut node on an unrelated wall is traced straight through, harmless).
  if(tJunctionTol>0){
    for(var wi=0;wi<opens.length;wi++){
      [opens[wi].firstSegment.point,opens[wi].lastSegment.point].forEach(function(ep){
        for(var wj=0;wj<opens.length;wj++){
          if(wj===wi)continue;
          var loc=opens[wj].getNearestLocation(ep);
          if(!loc)continue;
          var d=loc.point.getDistance(ep);
          if(d>tJunctionTol)continue;
          var frac=loc.offset/opens[wj].length;
          // Projections landing at the other wall's own ends are already
          // endpoint-to-endpoint cases the existing join/gap logic covers.
          if(frac<0.001||frac>0.999)continue;
          crossings.push({wall:wj,frac:frac,pt:[loc.point.x,loc.point.y]});
        }
      });
    }
  }
  // Collapse near-duplicate cuts on the same wall (2026-07, found live on a
  // dense ~185-crossing arrangement): three strokes crossing pairwise near
  // one point report 2-3 crossings a fraction of a px apart on the same
  // wall (fracs like 0.557 vs 0.558) — each becomes its own cut, producing
  // micro sub-edges whose two endpoints then cluster into the SAME graph
  // node downstream. In fill.rs that degenerate edge is dropped by its
  // a==b guard; the JS fillBuildGraph gained the same guard alongside this
  // fix — but dropping the noise ONCE here at the shared source keeps both
  // consumers' graphs clean AND identical, instead of each side sanitizing
  // (or forgetting to sanitize) independently. 0.75px: well under joinEps'
  // 1.5 floor, so two cuts this close were always going to be one node.
  var byWallDedup={};
  crossings.forEach(function(c){(byWallDedup[c.wall]=byWallDedup[c.wall]||[]).push(c);});
  var deduped=[];
  Object.keys(byWallDedup).forEach(function(wk){
    var list=byWallDedup[wk],len=opens[wk].length;
    list.sort(function(a,b){return a.frac-b.frac;});
    var kept=[];
    list.forEach(function(c){
      if(kept.length&&(c.frac-kept[kept.length-1].frac)*len<0.75)return;
      kept.push(c);
    });
    kept.forEach(function(c){deduped.push(c);});
  });
  return deduped;
}
function fillVectorFindWasm(clickPt,gapThr,layer,excludePath,onlyIds){
  var walls=fillCollectWalls(layer,excludePath,onlyIds);
  var input={
    openWalls:walls.open.map(function(w){return{segments:_wallSegments(w)};}),
    closedWalls:walls.closed.map(function(w){return{segments:_wallSegments(w)};}),
    gapThr:gapThr,click:[clickPt.x,clickPt.y],
    // T-junction tolerance floors at 6px so an exact/near touch welds even
    // on the very first gapThr=0 pass (via node clustering) instead of
    // waiting for escalation; beyond that it tracks gapThr so a farther
    // endpoint-to-mid-wall gap bridges at the same escalation step an
    // endpoint-to-endpoint gap of the same size would.
    crossings:_computeExactCrossings(walls.open,Math.max(6,gapThr)),
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
// Floor below which a candidate is pure numerical noise (a near-tangent
// crossing sampled to a ~0-area sliver), never a real region a user could
// have meant to click. Confirmed live (fb_mrj7byvu_913273): a click near a
// small closed detail (an ear/curl bump on a long hand-drawn outline)
// produced a real, on-canvas but nearly-invisible fill with area -95 —
// the "smallest area wins" comparison below has no lower bound, so a
// degenerate candidate this small can beat the correct, larger region the
// user actually meant. NOTE: this floor alone does not filter that exact
// -95px² case (95 > 4) — it only guards the true sub-pixel noise floor.
// The -95 sliver itself needs a proper root-cause fix (likely in how
// build_graph/fill.rs represents a near-tangent self-crossing), not yet
// found — see the commit message for what was and wasn't verified.
var FILL_MIN_AREA=4;
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
    // fillVectorFindJS (fillBuildGraph specifically) is a STALE peer of
    // build_graph/fill.rs: it never learned the wall-splitting-at-crossings
    // logic Rust got (see fill.rs's header comment + _computeExactCrossings
    // above, which ONLY the WASM path consults below) — it still treats
    // each wall as a single whole edge between its two endpoints, so it
    // structurally cannot find a region bounded purely by two strokes
    // CROSSING each other with no shared/nearby endpoints involved — e.g.
    // the everyday "diamond/losange formed by two crossing strokes" shape
    // (reported: "je clic sur le losange centrale et il ne detecte pas ça
    // comme une forme fermée"). Confirmed live: fillVectorFindJS returns
    // not-found on such a shape at every gapThr, while fillVectorFindWasm
    // finds it correctly at gapThr=0.
    // Used to ALSO run this stale JS path whenever WASM merely returned
    // null (a normal "nothing closes at this gapThr yet" answer, not a
    // failure) — its wrong/crossing-blind candidate could then win
    // fillVectorFind's cross-step area comparison over the correct WASM
    // candidate found at a later, larger gapThr step, or trip the early
    // usedGap===false break on a wrong answer before the real one was ever
    // tried (reported: "encore des problème de détection et de fill dans ce
    // cas de figure" — inconsistent results needing several clicks). Only
    // fall back to JS when WASM is genuinely unavailable or threw — never
    // to "double-check" a clean WASM null, which IS the correct answer at
    // that gapThr.
    var wasmAvailable=window.GeometryWasm&&window.GeometryWasm.ready;
    var wasmFailed=false;
    if(wasmAvailable){
      try{res=fillVectorFindWasm(clickPt,gapThr,layer,excludePath,onlyIds);}
      catch(e){wasmFailed=true;console.warn('[geometry-wasm] fill_find failed, falling back to JS',e);}
    }
    if(!res&&(!wasmAvailable||wasmFailed))res=fillVectorFindJS(clickPt,gapThr,layer,excludePath,onlyIds);
    if(res&&Math.abs(res.path.area)<FILL_MIN_AREA){
      // Degenerate sliver (self-crossing noise) — not a real candidate.
      // Discard and keep escalating instead of letting it win by default.
      res.path.remove();res=null;
    }
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
  if(!best){
    // The graph/wall approach above found NOTHING at any gapThr — before
    // giving up ("aucune zone fermée"), try the fundamentally different
    // raster fallback (see fillVectorFindRaster below): rasterize every
    // wall AND every existing fill as solid barriers and flood-fill from
    // the click pixel. A bitmap has no concept of "wall endpoint", "graph
    // edge", or "self-crossing" — it only sees painted vs. empty pixels —
    // so it can succeed on exactly the shapes that keep defeating the
    // graph tracer (a self-tangent near-touch, a wrong-pairing gap bridge,
    // a face the exhaustive enumeration still somehow missed). Only tried
    // as a last resort (not raced at every gapThr step) since it's far
    // more expensive per call than a graph lookup.
    try{
      var rres=fillVectorFindRaster(clickPt,layer,excludePath,24,onlyIds);
      if(rres){
        if(Math.abs(rres.path.area)>=FILL_MIN_AREA){rres.gapPx=rres.gapPx;best=rres;}
        else rres.path.remove();
      }
    }catch(e){console.warn('[fill] raster fallback failed',e);}
  }
  return best;
}
// ---- Raster fallback (2026-07) ----------------------------------------
// A second, independent fill strategy alongside the vector wall/graph
// approach above: rasterize the walls (open + closed strokes AND every
// existing fill shape — reported request: "les stroke, les fermetures de
// gap faites à la main, les fill existants forment une unique forme" —
// exactly like a bitmap "magic wand" tool sees a drawing) to an offscreen
// <canvas>, flood-fill the connected background region from the click
// pixel, then trace that region's boundary back into a vector Path. Pixel
// flood fill has no notion of "wall", "graph edge", or "self-crossing" —
// only painted vs. empty — so it's naturally immune to the whole class of
// topology bugs the graph tracer keeps hitting (near-tangent slivers,
// wrong-pairing gap bridges, missed faces). Traded off against: raster
// resolution caps how small a real gap it can bridge, and it can't (yet)
// return a genuine multi-loop/holed region — only the single largest
// boundary loop. Good enough as a last-resort fallback, not a full
// replacement for the graph approach's precision.
function fillTraceRasterContour(visited,rw,rh,minX,minY,maxX,maxY){
  function isFilled(x,y){if(x<0||y<0||x>=rw||y>=rh)return false;return!!visited[y*rw+x];}
  var edgeMap={};
  function key(x,y){return x+','+y;}
  function addEdge(x1,y1,x2,y2){
    var k1=key(x1,y1),k2=key(x2,y2);
    (edgeMap[k1]=edgeMap[k1]||[]).push(k2);
    (edgeMap[k2]=edgeMap[k2]||[]).push(k1);
  }
  // Walk every filled pixel's 4 edges — an edge belongs to the boundary
  // whenever the pixel across it is NOT part of the filled region (blocked
  // by a wall, or off the raster entirely).
  for(var y=minY;y<=maxY;y++){
    for(var x=minX;x<=maxX;x++){
      if(!isFilled(x,y))continue;
      if(!isFilled(x-1,y))addEdge(x,y,x,y+1);
      if(!isFilled(x+1,y))addEdge(x+1,y,x+1,y+1);
      if(!isFilled(x,y-1))addEdge(x,y,x+1,y);
      if(!isFilled(x,y+1))addEdge(x,y+1,x+1,y+1);
    }
  }
  var usedEdge={};
  function edgeKey(a,b){return a<b?a+'|'+b:b+'|'+a;}
  var loops=[];
  Object.keys(edgeMap).forEach(function(startK){
    var neighbors=edgeMap[startK];
    for(var ni=0;ni<neighbors.length;ni++){
      var firstNext=neighbors[ni];
      if(usedEdge[edgeKey(startK,firstNext)])continue;
      var loop=[startK];
      var prev=startK,cur=firstNext;
      usedEdge[edgeKey(prev,cur)]=true;
      var guard=0,maxGuard=(rw+1)*(rh+1)*4+16;
      while(cur!==startK&&guard++<maxGuard){
        loop.push(cur);
        var opts=edgeMap[cur]||[];
        var candidates=opts.filter(function(o){return!usedEdge[edgeKey(cur,o)];});
        if(!candidates.length)break;
        var pick=null;
        for(var ci=0;ci<candidates.length;ci++){if(candidates[ci]!==prev){pick=candidates[ci];break;}}
        if(!pick)pick=candidates[0];
        usedEdge[edgeKey(cur,pick)]=true;
        prev=cur;cur=pick;
      }
      if(cur===startK)loops.push(loop);
    }
  });
  if(!loops.length)return null;
  // Several disjoint loops can appear (holes, or unrelated blobs elsewhere
  // in the raster) — the largest by point count is, in practice, always
  // the outer boundary of the region the flood fill actually grew from.
  loops.sort(function(a,b){return b.length-a.length;});
  return loops[0].map(function(k){var p=k.split(',');return[+p[0],+p[1]];});
}
function fillVectorFindRaster(clickPt,layer,excludePath,gapThr,onlyIds){
  var walls=fillCollectWalls(layer,excludePath,onlyIds);
  var allWalls=walls.open.concat(walls.closed);
  function cleanupWalls(){walls.open.forEach(function(w){w.remove();});walls.closed.forEach(function(w){w.remove();});}
  if(!allWalls.length){cleanupWalls();return null;}
  var bounds=allWalls[0].bounds.clone();
  for(var bi=1;bi<allWalls.length;bi++)bounds=bounds.unite(allWalls[bi].bounds);
  bounds=bounds.expand(gapThr*2+24);
  var maxDim=1400;
  var scale=Math.min(1,maxDim/Math.max(bounds.width,bounds.height,1));
  var rw=Math.max(1,Math.round(bounds.width*scale)),rh=Math.max(1,Math.round(bounds.height*scale));
  if(rw*rh>maxDim*maxDim*1.2){cleanupWalls();return null;}
  var canvas=document.createElement('canvas');canvas.width=rw;canvas.height=rh;
  var ctx=canvas.getContext('2d');
  ctx.lineJoin='round';ctx.lineCap='round';ctx.fillStyle='#000';ctx.strokeStyle='#000';
  allWalls.forEach(function(w){
    var len=w.length;if(!(len>0))return;
    var n=Math.max(4,Math.min(600,Math.ceil(len/3)));
    ctx.beginPath();
    for(var i=0;i<=n;i++){
      var pt=w.getPointAt(Math.min(len,i/n*len));
      var sx=(pt.x-bounds.x)*scale,sy=(pt.y-bounds.y)*scale;
      if(i===0)ctx.moveTo(sx,sy);else ctx.lineTo(sx,sy);
    }
    if(w.closed)ctx.closePath();
    // Inflate by 2×gapThr (radius each side of the centerline) so a real
    // hand-drawn pen-lift gap up to ~gapThr px visually closes shut —
    // this IS this engine's equivalent of the graph approach's gap-bridge
    // edges, just expressed as ink thickness instead of a virtual edge.
    ctx.lineWidth=Math.max(1.5,(w.strokeWidth||2))+gapThr*2;
    ctx.stroke();
    // An existing fill shape's INTERIOR is also a solid barrier — the
    // reported request: strokes + hand-closed gaps + existing fills should
    // all read as one combined obstacle map, not just their outlines.
    if(w.closed&&w.fillColor)ctx.fill();
  });
  var img=ctx.getImageData(0,0,rw,rh),data=img.data;
  var cx=Math.round((clickPt.x-bounds.x)*scale),cy=Math.round((clickPt.y-bounds.y)*scale);
  if(cx<0||cy<0||cx>=rw||cy>=rh){cleanupWalls();return null;}
  if(data[(cy*rw+cx)*4+3]>10){cleanupWalls();return null;} // clicked directly on ink/an existing fill
  var visited=new Uint8Array(rw*rh);
  var stack=[cy*rw+cx];visited[cy*rw+cx]=1;
  var minX=cx,maxX=cx,minY=cy,maxY=cy,count=0,touchedEdge=false;
  var guardMax=rw*rh;
  while(stack.length&&count<guardMax){
    var idx=stack.pop();
    var y=(idx/rw)|0,x=idx-y*rw;
    count++;
    if(x<minX)minX=x;if(x>maxX)maxX=x;if(y<minY)minY=y;if(y>maxY)maxY=y;
    if(x===0||y===0||x===rw-1||y===rh-1)touchedEdge=true;
    if(x>0){var l=idx-1;if(!visited[l]&&data[l*4+3]<=10){visited[l]=1;stack.push(l);}}
    if(x<rw-1){var r=idx+1;if(!visited[r]&&data[r*4+3]<=10){visited[r]=1;stack.push(r);}}
    if(y>0){var t=idx-rw;if(!visited[t]&&data[t*4+3]<=10){visited[t]=1;stack.push(t);}}
    if(y<rh-1){var bt=idx+rw;if(!visited[bt]&&data[bt*4+3]<=10){visited[bt]=1;stack.push(bt);}}
  }
  // The flood fill reached the raster's own edge — the click point sits in
  // background/open space, not a genuinely enclosed region (the same
  // "aucune zone fermée" case, just detected differently than the graph
  // approach's null return).
  if(touchedEdge||count<4){cleanupWalls();return null;}
  var contourPx=fillTraceRasterContour(visited,rw,rh,minX,minY,maxX,maxY);
  if(!contourPx||contourPx.length<3){cleanupWalls();return null;}
  var rawPath=new Path({insert:true});
  contourPx.forEach(function(p,i){
    var doc=new Point(bounds.x+p[0]/scale,bounds.y+p[1]/scale);
    if(i===0)rawPath.moveTo(doc);else rawPath.lineTo(doc);
  });
  rawPath.closed=true;
  // Blocky pixel-grid outline -> smooth curve, tolerance scaled back to
  // document units (same Paper.js simplify() already used everywhere else
  // in this codebase for stroke smoothing).
  rawPath.simplify(Math.max(1,1.5/scale));
  if(!rawPath.contains(clickPt)){rawPath.remove();cleanupWalls();return null;}
  var wallIds=walls.openSrc.concat(walls.closedSrc);
  cleanupWalls();
  return{path:rawPath,wallIds:wallIds,usedGap:gapThr>0,gapPx:gapThr,raster:true};
}
// fillTraceLoop picks, at each graph node, whichever outgoing edge has the
// sharpest turn from the arrival direction — correct for a simple planar
// face, but at a node where several curves cross near-tangentially (e.g. 2
// open curves crossing 2 closed quad-like shapes close together), two
// candidate turn angles can be near-indistinguishable and the trace can
// walk onto an edge belonging to a DIFFERENT, non-adjacent face. The loop
// still closes (nothing checks for that), producing a self-crossing
// "bow-tie" polygon that legitimately winds through several disconnected
// regions of the canvas — and because its shoelace sum can come out
// SMALLER than the true tight region's, it can win the area-only
// candidate comparison (fillVectorFindJS below), which is exactly the
// reported "fills the clicked corner but also spawns polygons elsewhere"
// symptom. A genuine planar face can never self-intersect, so rejecting
// any self-crossing candidate outright — rather than just deprioritizing
// it by area — is the correct fix: fillVectorFind's gap-escalation loop
// then keeps widening gapThr until it finds an actually-simple loop (or
// gives up and fills nothing), never a wrong one.
function _fillSegXing(p1,p2,p3,p4){
  function ccw(a,b,c){return (c.y-a.y)*(b.x-a.x)-(b.y-a.y)*(c.x-a.x);}
  var d1=ccw(p3,p4,p1),d2=ccw(p3,p4,p2),d3=ccw(p1,p2,p3),d4=ccw(p1,p2,p4);
  return ((d1>0&&d2<0)||(d1<0&&d2>0))&&((d3>0&&d4<0)||(d3<0&&d4>0));
}
function fillPolySelfIntersects(pts){
  var n=pts.length;
  if(n<4)return false;
  for(var i=0;i<n;i++){
    var a1=pts[i],a2=pts[(i+1)%n];
    for(var j=i+1;j<n;j++){
      // Skip edge j when it shares a vertex with edge i — either the NEXT
      // edge (j===i+1, sharing a2/pts[i+1]) or, via wraparound, the
      // PREVIOUS edge ((j+1)%n===i, sharing a1/pts[i]). The original
      // version only checked the second case: the missing j===i+1 check
      // let every pair of adjacent edges get tested against each other,
      // and a segment-crossing test right at a shared endpoint is exactly
      // where floating-point noise flips sign — with hundreds of closely-
      // spaced points from dense curve sampling (fillLoopSelfIntersects),
      // this false-flagged nearly EVERY legitimate simple loop as
      // self-intersecting, silently discarding every fill candidate
      // (reported: "il ne détecte aucune zone fermée").
      if(j===i||j===i+1||(j+1)%n===i)continue;
      if(_fillSegXing(a1,a2,pts[j],pts[(j+1)%n]))return true;
    }
  }
  return false;
}
// fillPolySelfIntersects operates on straight edges between successive
// points — fine for the closed-wall case (a user-drawn closed stroke's own
// segments), but a TRACED loop's edges are real bezier arcs cloned from
// hand-drawn strokes (fillBuildPathFromSeq), which routinely bulge well
// away from the straight chord between their anchor points. Two organic
// curves whose ANCHOR points form a perfectly simple, non-crossing polygon
// can still cross each other mid-arc — exactly what a hand-drawn character
// silhouette (arms/head/torso, all long sweeping curves) produces, and
// exactly why fillPolySelfIntersects(chainPath.segments) alone missed the
// reported "star-burst" fill spanning a whole character (fb_mrgedu37):
// confirmed live — a 4-anchor square path with handles bulging two
// opposite edges into each other tests as NOT self-intersecting via
// anchor points alone, but IS once the actual curve is sampled. Densely
// sampling the real path before the crossing test catches both cases.
function fillLoopSelfIntersects(chainPath){
  var len=chainPath.length;
  if(!(len>0))return false;
  var n=Math.min(300,Math.max(24,Math.round(len/4)));
  var pts=[];
  for(var i=0;i<n;i++)pts.push(chainPath.getPointAt((i/n)*len));
  return fillPolySelfIntersects(pts);
}
// Physically cuts each open wall at every exact crossing point
// (_computeExactCrossings — the SAME Paper.js getIntersections() math the
// WASM path already feeds to Rust's find_crossings/build_graph) via
// Path#splitAt, turning "one long wall that happens to cross another" into
// several shorter walls with REAL endpoints sitting at the crossing. This
// is the JS-side fix for fillVectorFindJS being a "stale peer" of
// build_graph/fill.rs (see fillVectorFind's own comment on the diamond/
// losange bug): fillBuildGraph only ever merges wall ENDPOINTS into graph
// nodes (never mid-curve crossings), so a region bounded purely by two
// strokes crossing each other with no shared/nearby endpoints was
// structurally unfindable in JS even though the WASM path already handled
// it correctly. Splitting here needs NO change to fillBuildGraph/
// fillTraceLoop at all — the two walls' independently-computed crossing
// points coincide (same real intersection, just computed once per wall by
// _computeExactCrossings), well within fillBuildGraph's own joinEps
// endpoint-merge tolerance, so they naturally fold into one shared node.
function _splitWallsAtCrossings(opens,tJunctionTol){
  var crossings=_computeExactCrossings(opens,tJunctionTol);
  var byWall={};
  crossings.forEach(function(c){(byWall[c.wall]=byWall[c.wall]||[]).push(c.frac);});
  var result=[];
  opens.forEach(function(w,i){
    var fracs=byWall[i];
    if(!fracs||!fracs.length){result.push(w);return;}
    var totalLen=w.length;
    var offsets=fracs.map(function(f){return f*totalLen;}).filter(function(o){return o>0.05&&o<totalLen-0.05;});
    offsets=Array.from(new Set(offsets.map(function(o){return Math.round(o*1000)/1000;}))).sort(function(a,b){return a-b;});
    if(!offsets.length){result.push(w);return;}
    var remaining=w,consumed=0;
    offsets.forEach(function(absOffset){
      var localOffset=absOffset-consumed;
      if(localOffset<=0.05||localOffset>=remaining.length-0.05)return;
      var piece=remaining.splitAt(localOffset);
      if(!piece)return;
      // splitAt's new cut anchor keeps a SYMMETRIC handle pair on both
      // sides (as if the original curve still continued straight through)
      // — correct for what splitAt is usually used for (re-joining the
      // pieces back into the same curve later), but wrong here: `remaining`
      // truly ENDS at this cut and `piece` truly STARTS here, nothing
      // continues past either tip. Left alone, fillBuildPathFromSeq's
      // addSegments later stitches these two pieces' cut-anchors together
      // as two back-to-back duplicate-point segments, one with a stray
      // handleOut and the other a stray handleIn both still pointing PAST
      // the shared point — a small spike/self-crossing right at the seam
      // (confirmed live: a clean two-arc lens between two crossings traced
      // as self-intersecting until this was flattened). Zeroing the
      // dangling handle on each side removes the phantom continuation
      // while leaving each piece's OWN real curve shape untouched.
      remaining.lastSegment.handleOut=new Point(0,0);
      piece.firstSegment.handleIn=new Point(0,0);
      result.push(remaining);
      remaining=piece;consumed=absOffset;
    });
    result.push(remaining);
  });
  return result;
}
function fillVectorFindJS(clickPt,gapThr,layer,excludePath,onlyIds){
  var walls=fillCollectWalls(layer,excludePath,onlyIds);
  var candidates=[]; // {chainPath, area, fromClosedWall}
  walls.closed.forEach(function(w){
    // Real curve area/containment (Paper's own bezier math), NOT the
    // anchor-point polygon approximations (fillPolyArea/fillPointInPoly) —
    // a 4-anchor Path.Circle's anchor polygon is a SQUARE with ~2/3 of the
    // circle's true area, which made a genuine sub-face (chord + half-arc)
    // TIE with the whole circle on approximated area and lose to it on
    // sort order (confirmed live: a chord-split circle recolored the whole
    // circle on one side and correctly half-filled the other). The Rust
    // path never had this bug: it measures flattened polylines at 0.75px
    // tolerance, which tracks the real curve closely.
    var area=Math.abs(w.area);
    // A hand-drawn "closed" wall isn't guaranteed to be a simple ring — an
    // organic stroke that loops back near its own earlier run can
    // self-cross exactly like a traced loop can (fillLoopSelfIntersects).
    // This is the mirror of the fill.rs fix for the same symmetric gap:
    // Best::Closed there had no guard either, and was confirmed (live,
    // against the reporter's real drawing) to be exactly what won the
    // reported diamond-artifact fill.
    if(area>=1&&w.contains(clickPt)&&!fillLoopSelfIntersects(w))candidates.push({chainPath:w,area:area,fromClosedWall:true});
  });
  var splitOpen=walls.open.length?_splitWallsAtCrossings(walls.open,Math.max(6,gapThr)):walls.open;
  if(splitOpen.length){
    var graph=fillBuildGraph(splitOpen,gapThr);
    var maxSteps=graph.edges.length*2+8;
    // Exhaustive planar-face enumeration, not a capped sample of arbitrary
    // seeds. fillTraceLoop's "always take the sharpest turn" rule is
    // actually the textbook-correct method for walking the boundary of
    // the SPECIFIC face touching a given directed half-edge — every
    // directed half-edge belongs to exactly one face under this rule. The
    // old code only ever tried up to 150 STROKE edges as seeds (missing
    // every edge past that cap on a busy scene, and never trying GAP
    // edges as seeds at all) — on a complex multi-overlap arrangement
    // (e.g. several existing fills' outlines crossing each other, ~168
    // crossings in the reported case) that silently left many real faces
    // completely undiscovered: not rejected by the area/self-intersection
    // filters below, just NEVER TRACED, so a click squarely inside one of
    // them found nothing at any gap threshold (confirmed live). Walking
    // EVERY edge from both endpoints in both turn directions, skipping a
    // (edge,node,turnSign) combo once its face has already been traced
    // from elsewhere on its own boundary, visits every face in the graph
    // exactly once — no cap, no missed faces, and actually cheaper than
    // the old approach for a busy scene since it stops re-tracing the
    // same face redundantly from every one of its boundary edges.
    var visited={};
    for(var ei=0;ei<graph.edges.length;ei++){
      var seedEdge=graph.edges[ei];
      [seedEdge.a,seedEdge.b].forEach(function(startNode){
        [1,-1].forEach(function(turnSign){
          var key=seedEdge.idx+':'+startNode+':'+turnSign;
          if(visited[key])return;
          visited[key]=true;
          var seq=fillTraceLoop(graph,splitOpen,startNode,seedEdge,turnSign,maxSteps);
          if(!seq||seq.length<2)return;
          seq.forEach(function(hop){visited[hop.edge.idx+':'+hop.from+':'+turnSign]=true;});
          var chainPath=fillBuildPathFromSeq(graph,splitOpen,seq);
          // Real curve area/containment — see the closed-wall loop's
          // comment above for why the anchor-polygon approximations are
          // not good enough to rank candidates against each other.
          var area=Math.abs(chainPath.area);
          if(area<1||!chainPath.contains(clickPt)||fillLoopSelfIntersects(chainPath)){chainPath.remove();return;}
          var usedGap=seq.some(function(h){return h.edge.type==='gap';});
          candidates.push({chainPath:chainPath,area:area,fromClosedWall:false,usedGap:usedGap});
        });
      });
    }
  }
  if(!candidates.length){
    splitOpen.forEach(function(w){w.remove();});walls.closed.forEach(function(w){w.remove();});
    return null;
  }
  // the smallest enclosing loop wins (the innermost face touching the
  // click), matching how Animate/TVPaint resolve nested regions
  candidates.sort(function(a,b){return a.area-b.area;});
  var winner=candidates[0];
  candidates.slice(1).forEach(function(c){if(!c.fromClosedWall)c.chainPath.remove();});
  splitOpen.forEach(function(w){w.remove();});
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
  // Closed-gesture detection: the artist's stroke came back near its own
  // start (a ring/loop, e.g. tracing a circle to paint-bucket the middle
  // later) rather than two genuinely separate open ends. Requires BOTH a
  // tight start/end gap AND enough total travel that this isn't just a
  // tiny jittery dot landing back on itself. See buildClosedRingOutline's
  // own comment for why this needs a dedicated code path at all.
  var avgW=0;for(var awi=0;awi<widths.length;awi++)avgW+=widths[awi];avgW/=widths.length;
  if(pts.length>=8){
    var travel=0;for(var tli=1;tli<pts.length;tli++)travel+=pts[tli].getDistance(pts[tli-1]);
    var closeThresh=Math.max(6,avgW*0.6);
    if(pts[0].getDistance(pts[pts.length-1])<closeThresh&&travel>avgW*3){
      var ring=buildClosedRingOutline(pts,widths,avgW);
      if(ring)return ring;
    }
  }
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
// Closed-gesture variant of the sweep above. The open-stroke sweep forces
// two round end-caps at the start/end points — fine for a real open stroke,
// but when those two points are actually the SAME spot (a ring/loop gesture,
// e.g. tracing a circle with the Fill/Pressure brush intending to
// paint-bucket the middle afterwards), the two caps bridge the outer and
// inner offsets with a half-stroke-width blob right at the closure. That
// seam is exactly what was forcing the paint bucket's escalating gap
// tolerance (FILL_GAP_STEPS) to kick in, producing a fill boundary visibly
// offset from the true inner edge (2026-07 feedback: red ring + hatched/
// gapped fill overlay screenshots).
// Fix: build the outer and inner offsets as their OWN closed loops (no caps
// needed at all — there's no open end) and derive the ribbon via a boolean
// subtract, same principle as Umoupen's mesh-silhouette fill approach (the
// dev's own publicly-released source, see conversation): don't stitch an
// offset polyline's own literal endpoints, let geometry produce the true
// annulus instead. The subtract's hole is then folded back into a SINGLE
// flat Path (same zero-width-slit technique insertBooleanResult already
// uses via _mergeHoleIntoExterior/_polyClosestPair for real boolean holes)
// so every existing caller of buildVariableWidthPath keeps working
// unchanged — this ribbon's edges are smoothed beziers though, not
// insertBooleanResult's straight WASM polygons, so handleIn/handleOut are
// preserved through the merge instead of being dropped.
function buildClosedRingOutline(pts,widths,avgW){
  var n0=pts.length;
  var loopPts=pts,loopWidths=widths;
  // Trim a near-duplicate closing point (pen came back within a fraction of
  // the average width of its own start) so the loop doesn't carry a
  // degenerate near-zero-length final segment.
  if(pts[0].getDistance(pts[n0-1])<Math.max(1,avgW*0.15)&&n0>3){
    loopPts=pts.slice(0,n0-1);loopWidths=widths.slice(0,n0-1);
  }
  var n=loopPts.length;
  if(n<4)return null;
  var left=[],right=[];
  for(var i=0;i<n;i++){
    var prevI=(i-1+n)%n,nextI=(i+1)%n;
    var tangent=loopPts[nextI].subtract(loopPts[prevI]);
    if(tangent.length<0.0001)tangent=new Point(1,0);
    tangent=tangent.normalize();
    var normal=new Point(-tangent.y,tangent.x);
    var hw=Math.max(0.3,loopWidths[i]/2);
    left.push(loopPts[i].add(normal.multiply(hw)));
    right.push(loopPts[i].subtract(normal.multiply(hw)));
  }
  var leftPath=new Path({insert:false,segments:left,closed:true});
  leftPath.smooth({type:'continuous'});
  var rightPath=new Path({insert:false,segments:right,closed:true});
  rightPath.smooth({type:'continuous'});
  var outer=Math.abs(leftPath.area)>=Math.abs(rightPath.area)?leftPath:rightPath;
  var inner=outer===leftPath?rightPath:leftPath;
  var boolResult=null;
  try{boolResult=outer.subtract(inner,{insert:false});}catch(e){boolResult=null;}
  if(!boolResult||!(boolResult instanceof CompoundPath)||boolResult.children.length<2){
    // Degenerate case (brush width comparable to the loop's own radius —
    // the inner offset self-crosses and collapses to nothing): no real
    // hole to cut, a plain filled blob from the outer loop alone is the
    // correct result.
    var fallback=outer.clone({insert:false});
    fallback.closed=true;
    leftPath.remove();rightPath.remove();
    if(boolResult&&!(boolResult instanceof CompoundPath))boolResult.remove();
    return fallback;
  }
  var extChild=boolResult.children.reduce(function(a,b){return Math.abs(a.area)>=Math.abs(b.area)?a:b;});
  var holeChild=boolResult.children.filter(function(c){return c!==extChild;})[0];
  var extSegs=extChild.segments,holeSegs=holeChild.segments;
  var pair=_polyClosestPair(extSegs.map(function(s){return s.point;}),holeSegs.map(function(s){return s.point;}));
  var merged=new Path({insert:false});
  for(var k=0;k<=pair.i;k++){var s=extSegs[k];merged.add(new Segment(s.point,s.handleIn,s.handleOut));}
  for(var h=0;h<=holeSegs.length;h++){var hs=holeSegs[(pair.j+h)%holeSegs.length];merged.add(new Segment(hs.point,hs.handleIn,hs.handleOut));}
  var backSeg=extSegs[pair.i];
  merged.add(new Segment(backSeg.point,backSeg.handleIn,backSeg.handleOut));
  for(var k2=pair.i+1;k2<extSegs.length;k2++){var s2=extSegs[k2];merged.add(new Segment(s2.point,s2.handleIn,s2.handleOut));}
  merged.closed=true;
  leftPath.remove();rightPath.remove();boolResult.remove();
  return merged;
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
  // Cross-width distribution presets (scatterDistribution, ported from
  // rnote's TexturedDotsDistribution — see sampleAcrossWidth above).
  // Existing presets keep their historical 'uniform' spread untouched.
  'airbrush-soft':   {nibSize:.9, roundness:1,  spacing:.3, spaceJitter:.3, rotationMode:'random',rotationJitter:180,sizeJitter:.4, opacity:.18,opacityJitter:.3, scatter:1.6,dashGap:0, scatterDistribution:'normal'},
  'marker-dry':      {nibSize:1.1,roundness:.6, spacing:.35,spaceJitter:.2, rotationMode:'tangent',rotationJitter:12, sizeJitter:.25,opacity:.5, opacityJitter:.25,scatter:.5, dashGap:0, scatterDistribution:'edge'},
  // Scribble-fill tip (tipShape:'scribble', buildBrushDabs above) — a woven
  // patch of short independently-angled marks instead of a line of blobs,
  // the graphite/charcoal "scribbled shading" look from the reference pack.
  'graphite-scribble':{nibSize:1.4,roundness:1,  spacing:.5, spaceJitter:.25,rotationMode:'tangent',rotationJitter:0, sizeJitter:0,   opacity:.5, opacityJitter:.3, scatter:0,   dashGap:0, tipShape:'scribble',scribbleCount:9,scribbleLen:1.5,scribbleLenJitter:.4,scribbleWidth:.1,scribbleSpread:.7,scribbleAngleSpread:75},
};
// Hard ceiling on dabs per stroke, regardless of preset/length — protects
// against a very long stroke with tight spacing multiplying scene-
// serialization cost unboundedly (each dab is a real, separately-
// serialized Path — see the perf audit note in strokemotion/CLAUDE.md).
// Spacing is widened (never narrowed below the preset's own value) just
// enough to land under this cap rather than silently truncating the tail
// of the stroke.
// 900 (was 450, was 180): the cap is a HARD ceiling on total dabs — once a
// stroke's natural density (len/minAllowedSpacing) exceeds it, spacing gets
// compressed to len/maxPositions to stay under budget, and that floor grows
// linearly with length forever. At 450, a fairly ordinary preset/width
// combo (chalk-blunt, 8px brush) started compressing past ~2570px of
// accumulated path length — well within a single meandering stroke on a
// normal canvas, not just a degenerate case — reported as "en fonction de
// la longueur... les éléments de texture s'écartent". Doubling to 900
// pushes that threshold to ~5150px (a single gesture would need to be
// genuinely very long/looping to still hit it) while commit cost (one-time,
// at stroke end, not per-frame) measured at 14.8ms worst case across
// presets — still comfortably inside the perf envelope the audit measured
// (60fps at ~2600 scene items); 1500 measured 58ms, a real risk of a
// felt commit hitch, so NOT raised further than this.
var BRUSH_MAX_DABS=900;
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
  }else if(shape==='custom'&&preset.customStamp&&preset.customStamp.segments&&preset.customStamp.segments.length){
    // User-drawn tip (captureBrushStamp below) — a normalized {point,
    // handleIn,handleOut}[] fit to a [-0.5,0.5] box, rescaled per-axis to
    // (w,h) here exactly like 'rect' stretches to its own w/h, so Roundness
    // still meaningfully squashes whatever the user drew. Real bezier
    // geometry (not a raster stamp) — an open captured squiggle fills via
    // its own implicit closing edge, same as every other filled open path
    // in this codebase; a closed one keeps its own silhouette.
    var st=preset.customStamp;
    path=new Path({insert:false,closed:!!st.closed});
    st.segments.forEach(function(s){
      path.add(new Segment(new Point(s.point[0]*w,s.point[1]*h),new Point(s.handleIn[0]*w,s.handleIn[1]*h),new Point(s.handleOut[0]*w,s.handleOut[1]*h)));
    });
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
// Captures a real, on-canvas path as a reusable brush-tip stamp (the
// "dessiner sa texture de brush" request) — normalizes it to a [-0.5,0.5]
// per-axis box so buildDabShape's 'custom' branch can rescale it to any
// (w,h) exactly like the built-in shapes. Geometry only (silhouette) — a
// dab is always solid-filled with the stroke's own ink color, so the
// source path's own fill/stroke never matters, only its outline.
// CompoundPath/Raster/Group are rejected with a clear reason rather than
// silently picking one child or flattening — same honest-scope precedent
// as svg-import.js's unsupported-element handling.
function captureBrushStamp(p){
  if(!p)return{ok:false,reason:'Rien à capturer — dessine une forme puis sélectionne-la.'};
  if(!(p instanceof Path))return{ok:false,reason:'Sélectionne un seul trait simple (pas une forme booléenne/composée, une image, ni un groupe).'};
  if(!p.segments||p.segments.length<2)return{ok:false,reason:'La forme sélectionnée est trop simple (au moins 2 points).'};
  var b=p.bounds;
  if(b.width<1e-6||b.height<1e-6)return{ok:false,reason:'La forme sélectionnée est dégénérée (largeur ou hauteur nulle).'};
  var cx=b.center.x,cy=b.center.y,sx=1/b.width,sy=1/b.height;
  var segments=p.segments.map(function(s){
    return{point:[(s.point.x-cx)*sx,(s.point.y-cy)*sy],handleIn:[s.handleIn.x*sx,s.handleIn.y*sy],handleOut:[s.handleOut.x*sx,s.handleOut.y*sy]};
  });
  return{ok:true,stamp:{segments:segments,closed:!!p.closed,pointCount:segments.length}};
}
window.captureBrushStamp=captureBrushStamp;
// widthProfile (optional): a buildWidthProfile()-shaped {t,width}[] array,
// t in [0,1] as a fraction of PATHLIKE'S OWN length (not the raw stroke's
// original arc length — applyBrushTexture is responsible for re-basing a
// pressure-brush's centerline-length-fractioned profile onto whatever
// pathLike it actually passes in here). When given, dab size/spacing at
// each stamp position tracks the LOCAL width there instead of one fixed
// baseWidth for the whole stroke — this is what lets a texture preset
// actually taper with a pressure stroke's own pressure curve, instead of
// silently never being offered on vectorBrush strokes at all (previously
// the only caller of this function was the plain-constant-width commit
// branch in draw-bridge.js). Omitting widthProfile reproduces the exact
// prior constant-width behavior bit-for-bit — every existing (non-
// pressure) caller is unaffected.
// Cross-width placement distribution (ported from rnote's
// TexturedDotsDistribution — style/textured/textureddotsdistribution.rs):
// where a dab lands ACROSS the stroke, as a factor in [-1,1] of the scatter
// range. This is what separates "confetti" from "matter": 'uniform' (the
// historical behavior) spreads evenly; 'normal' concentrates toward the
// centerline (felt-tip / airbrush core); 'edge' pushes density toward the
// two borders (dry marker whose ink pools at the edges); 'center' is a
// sharper exponential falloff from the centerline than 'normal' (soft
// airbrush halo). Out-of-range samples re-roll as uniform, same clipping
// strategy rnote uses.
function sampleAcrossWidth(rand,distribution){
  var u=rand()*2-1; // uniform in [-1,1]
  switch(distribution){
    case 'normal':{
      // Box-Muller; std-dev 1/3 of the half-range so ±3σ spans the range,
      // same σ choice as rnote's Normal branch.
      var u1=Math.max(1e-12,rand()),u2=rand();
      var n=Math.sqrt(-2*Math.log(u1))*Math.cos(2*Math.PI*u2)/3;
      return (n>=-1&&n<=1)?n:u;
    }
    case 'center':{
      var e=-Math.log(Math.max(1e-12,rand()))/4; // Exp(λ=1), scaled by the same 1/4 width factor as rnote
      var s=(rand()<.5?-1:1)*e;
      return (s>=-1&&s<=1)?s:u;
    }
    case 'edge':{
      var e2=-Math.log(Math.max(1e-12,rand()))/4;
      var s2=rand()<.5?(-1+e2):(1-e2); // start AT an edge, decay inward
      return (s2>=-1&&s2<=1)?s2:u;
    }
    default:return u; // 'uniform' — exact historical behavior
  }
}
function buildBrushDabs(pathLike,preset,baseWidth,rng,widthProfile){
  var rand=rng||Math.random;
  var len=pathLike.length;
  if(!(len>0))return[];
  var nibScale=preset.nibSize!==undefined?preset.nibSize:1;
  var spacingFrac=preset.spacing!==undefined?preset.spacing:.35;
  function localWidth(at){
    if(!widthProfile||!widthProfile.length)return baseWidth;
    return widthAtFrac(widthProfile,len>0?at/len:0);
  }
  // Cap-safety spacing floor: derived from the SMALLEST width along the
  // profile (or baseWidth when constant) so the total-dab-count bound
  // below is always a safe worst-case, regardless of where along the
  // stroke the thin/thick parts fall — a thinner point wants smaller,
  // more closely-spaced dabs, so sizing the floor off the thinnest point
  // never under-estimates how many positions a real walk could need.
  var minWidth=baseWidth;
  if(widthProfile&&widthProfile.length)minWidth=widthProfile.reduce(function(m,p){return Math.min(m,p.width);},baseWidth);
  var minNibDiam=Math.max(.5,minWidth*nibScale);
  var minAllowedSpacing=Math.max(.4,minNibDiam*spacingFrac);
  // Bristle/scribble tips emit several sub-marks PER stamp position (see
  // below) — divide the position budget by that count up front so total
  // emitted dabs still respects BRUSH_MAX_DABS regardless of tip shape.
  var bristleCount=preset.tipShape==='bristle'?Math.max(1,preset.bristleCount||5):1;
  var scribbleCount=preset.tipShape==='scribble'?Math.max(1,preset.scribbleCount||8):1;
  var perPositionCount=Math.max(bristleCount,scribbleCount);
  var maxPositions=Math.max(1,Math.floor(BRUSH_MAX_DABS/perPositionCount));
  // Cap-safety compression (task #106: a long stroke at a tiny fixed pitch
  // blows through BRUSH_MAX_DABS) — ONLY kicks in when the stroke would
  // actually need more than maxPositions dabs at its own natural spacing
  // (len/minAllowedSpacing, the worst case across the whole width profile).
  // The previous version set capFloorSpacing=len/maxPositions
  // UNCONDITIONALLY — a plain linear function of length alone, with no
  // regard for whether the budget was ever actually at risk. Since it
  // grows with length forever, ANY preset's natural spacing eventually
  // fell below it, silently thinning the texture density on ordinary long
  // strokes (reported: "en fonction de la longueur... les éléments de
  // texture s'écartent") — measured live: chalk-blunt held ~5.7-5.85px
  // spacing from 200-2000px (correct, uniform), then stretched to 8.9px at
  // 4000px, 17.8px at 8000px, 35px at 16000px, none of which were even
  // close to the 450-dab ceiling yet. Gating it behind the real risk check
  // keeps every preset's density constant regardless of stroke length
  // right up until the budget is genuinely the limiting factor.
  var worstCasePositions=len/minAllowedSpacing;
  var capFloorSpacing=worstCasePositions>maxPositions?len/maxPositions:0;
  var roundness=preset.roundness!==undefined?preset.roundness:1;
  var dabs=[],d=0,guard=0;
  while(d<=len&&guard++<maxPositions+2&&dabs.length<BRUSH_MAX_DABS){
    var at=Math.min(len,d);
    // Spacing is a PERCENTAGE OF THE DAB'S OWN DIAMETER (Photoshop's model —
    // its Spacing slider is literally % of brush diameter): dab size AND gap
    // both scale with the LOCAL width here, so the texture's visual density
    // stays consistent as a pressure stroke tapers, same as it already did
    // across different fixed stroke widths.
    var localW=localWidth(at);
    var nibDiam=Math.max(.5,localW*nibScale);
    var spacing=Math.max(minAllowedSpacing,Math.max(.4,nibDiam*spacingFrac));
    if(spacing<capFloorSpacing)spacing=capFloorSpacing;
    if(!(preset.dashGap&&rand()<preset.dashGap)){
      var pt=pathLike.getPointAt(at);
      var tan=pathLike.getTangentAt(at)||new Point(1,0);
      var normal=new Point(-tan.y,tan.x);
      var angleBase;
      if(preset.rotationMode==='random')angleBase=rand()*360;
      else if(preset.rotationMode==='fixed')angleBase=preset.fixedAngle||0;
      else angleBase=tan.angle; // 'tangent' (default): dab follows stroke direction, like a real angled nib
      if(preset.tipShape==='scribble'){
        // Graphite/charcoal scribble-fill — the reference-image texture
        // that's a woven PATCH of short, independently-angled hatching
        // marks, not a line of discrete blobs/strands like every other tip
        // shape here. Structurally different from 'bristle' (which offsets
        // PARALLEL strands across the width, all at the same angle): each
        // scribble mark gets its OWN random angle within a wide spread AND
        // its own 2D jitter (along AND across the path, not just across),
        // so marks genuinely cross over each other the way a hand-scribbled
        // patch of shading does. Built as real vector capsules (a fully-
        // rounded thin rectangle), never a raster texture.
        var scrCount=Math.max(1,preset.scribbleCount||8);
        var scrLen=(preset.scribbleLen!==undefined?preset.scribbleLen:1.4)*nibDiam;
        var scrLenJit=preset.scribbleLenJitter!==undefined?preset.scribbleLenJitter:.4;
        var scrW=Math.max(.25,(preset.scribbleWidth!==undefined?preset.scribbleWidth:.12)*nibDiam);
        var scrSpread=(preset.scribbleSpread!==undefined?preset.scribbleSpread:.6)*nibDiam;
        var scrAngleSpread=preset.scribbleAngleSpread!==undefined?preset.scribbleAngleSpread:70;
        for(var sc=0;sc<scrCount&&dabs.length<BRUSH_MAX_DABS;sc++){
          var jAlong=(rand()*2-1)*scrSpread,jAcross=(rand()*2-1)*scrSpread;
          var markCenter=pt.add(tan.multiply(jAlong)).add(normal.multiply(jAcross));
          var markLen=Math.max(.4,scrLen*(1+(rand()*2-1)*scrLenJit));
          var markAngle=angleBase+(rand()*2-1)*scrAngleSpread;
          var mark=new Path.Rectangle({point:[-markLen/2,-scrW/2],size:[markLen,scrW],radius:scrW/2,insert:false});
          mark.rotate(markAngle);
          mark.position=markCenter;
          // *.6 — many overlapping marks build tone through overlap (how
          // real scribbling gets darker), not through each mark being
          // opaque on its own; a full-opacity mark here would read as one
          // solid stripe instead of a woven texture.
          mark.data={dabOpacity:Math.max(0,Math.min(1,(preset.opacity!==undefined?preset.opacity:.5)*(1+(rand()*2-1)*(preset.opacityJitter||0))*.6))};
          dabs.push(mark);
        }
      }else if(preset.tipShape==='bristle'){
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
        var scatterAmt=sampleAcrossWidth(rand,preset.scatterDistribution)*nibDiam*(preset.scatter||0);
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
  // Pressure (vectorBrush) ribbons are a FILLED SHAPE whose width varies
  // continuously — buildBrushDabs stamps along the ribbon's own centerline
  // (rebuilt here from data.centerSegments, {insert:false} — a disposable
  // measuring aid, never added to the layer) rather than the visible outline
  // path, both because it's the geometrically correct "brush path" and
  // because data.widthProfile's `t` fractions are centerline-arc-length-
  // based (buildWidthProfile, tools.js) — walking the outline instead would
  // misalign the profile lookup against a totally different arc length.
  var isPressure=!!(basePath.data&&basePath.data.isVectorBrush&&basePath.data.centerSegments&&basePath.data.centerSegments.length>1&&basePath.data.widthProfile);
  var pathLike=basePath,widthProfile=undefined,baseWidth=basePath.strokeWidth;
  if(isPressure){
    var cs=basePath.data.centerSegments;
    var centerline=new Path({insert:false});
    for(var ci=0;ci<cs.length;ci++){
      var s=cs[ci];
      var seg=new Segment(new Point(s.point[0],s.point[1]),s.handleIn?new Point(s.handleIn[0],s.handleIn[1]):undefined,s.handleOut?new Point(s.handleOut[0],s.handleOut[1]):undefined);
      centerline.add(seg);
    }
    pathLike=centerline;
    widthProfile=basePath.data.widthProfile;
    baseWidth=widthProfile.reduce(function(sum,p){return sum+p.width;},0)/widthProfile.length;
  }
  // Dab size derives from strokeWidth (or, for a pressure ribbon, the local
  // width along its profile) and NOTHING else — explicitly per user
  // requirement ("il faut que la taille corresponde à la taille que l'on a
  // définie dans width stroke"). An earlier heuristic bumped baseWidth to
  // |area|/length for filled paths (trying to make texture visible against
  // a wide fill): correct for a thin ribbon, but for a CLOSED filled shape —
  // the completely ordinary draw-an-outline-and-fill-it case — area/length
  // is the average width of the WHOLE shape, producing monstrous dabs
  // (reported with a screenshot: ~150px blobs ringing a potato drawn at
  // Width 3). The texture replaces the STROKE's look only; the fill stays
  // flat by design.
  // On RE-apply (switching preset on an already-textured stroke) the live
  // strokeColor may already be nulled by the fill-visible branch below —
  // fall back to the remembered original so the new dabs don't silently
  // come out colorless.
  var baseColor=basePath.strokeColor||(basePath.data&&basePath.data.preTextureStroke?new Color(basePath.data.preTextureStroke):null)||(isPressure?basePath.fillColor:null);
  var groupId='bg'+Date.now().toString(36)+'_'+Math.floor(Math.random()*1e6);
  var dabs=buildBrushDabs(pathLike,preset,baseWidth,null,widthProfile);
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
  if(isPressure){
    // A pressure ribbon's fillColor IS its own visible ink (unlike a plain
    // stroke's fill, which is separate flat content the texture must leave
    // alone) — the dabs are meant to be the ENTIRE visible texture here, so
    // the ribbon must go fully invisible same as the no-fill case below,
    // regardless of basePath.fillColor being truthy.
    if(basePath.data.preTextureOpacity===undefined)basePath.data.preTextureOpacity=basePath.opacity!==undefined?basePath.opacity:1;
    basePath.opacity=0;
  }else if(basePath.fillColor){
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
  // Bitmap Brush anchors re-stamp through their own module (one Raster
  // companion instead of vector dabs, same stored-seed determinism) — both
  // existing call sites (subselect drag-end here and in subselect-bridge.js)
  // funnel through this one function, so branching HERE covers both paths
  // without either caller knowing bitmap anchors exist.
  if(basePath.data&&basePath.data.bitmapBrushSpec&&basePath.data.brushGroupId){
    if(window.SMBitmapBrush)SMBitmapBrush.regenerate(basePath,layer);
    return;
  }
  if(!basePath.data||!basePath.data.brushTexturePreset||!basePath.data.brushGroupId)return;
  var gid=basePath.data.brushGroupId;
  for(var i=layer.children.length-1;i>=0;i--){
    var c=layer.children[i];
    if(c.data&&c.data.isBrushTextureCopy&&c.data.brushGroupId===gid)c.remove();
  }
  applyBrushTexture(basePath,basePath.data.brushTexturePreset);
}
// Applies a brush-texture preset to an ALREADY-DRAWN, already-committed
// stroke (feedback: "impossible d'appliquer une brush preset à
// postériori") — previously applyBrushTexture only ever ran once, inline,
// at draw-commit time. First tears down whatever texture the path already
// had (same dab-removal as regenerateBrushTexture, plus restoring the
// anchor's pre-texture opacity/strokeColor so switching presets — or back
// to "None" — never leaves it invisible/strokeless), then applies the new
// one. presetKey 'none' just removes texture, leaving a plain stroke.
function applyOrChangeBrushTexture(basePath,layer,presetKey){
  if(basePath.data&&basePath.data.brushGroupId){
    var gid=basePath.data.brushGroupId;
    for(var i=layer.children.length-1;i>=0;i--){
      var c=layer.children[i];
      if(c.data&&c.data.isBrushTextureCopy&&c.data.brushGroupId===gid)c.remove();
    }
    if(basePath.data.preTextureOpacity!==undefined){basePath.opacity=basePath.data.preTextureOpacity;delete basePath.data.preTextureOpacity;}
    if(basePath.data.preTextureStroke!==undefined){basePath.strokeColor=basePath.data.preTextureStroke?new Color(basePath.data.preTextureStroke):null;delete basePath.data.preTextureStroke;}
    delete basePath.data.brushCompanions;delete basePath.data.brushTexturePreset;delete basePath.data.brushGroupId;
  }
  if(presetKey&&presetKey!=='none')applyBrushTexture(basePath,presetKey);
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
      var islands=insertBooleanResult(layer,Math.min(idx,layer.children.length),united,path.fillColor,path.opacity,null,target.data);
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
// Paper.js's own Path.simplify(t) does `t||2.5` internally — passing
// exactly 0 is indistinguishable from passing nothing at all, so it
// silently re-fits with the DEFAULT tolerance (2.5) instead of truly
// zero. Every call site that forwards the user's "Smooth" slider must
// skip the call entirely at 0 rather than pass 0 through — there's no
// tolerance value that makes Paper.js's fitter a true no-op. Bug found
// live: "à 0 il n'y a aucun smooth" — confirmed it wasn't the case before.
function simplifyIfNeeded(path,amount){if(amount>0)path.simplify(amount);}
function buildCenterSegmentsFromRawStroke(rawPts,rawWidths,smoothingAmt){
  var raw=new Path({insert:false});
  rawPts.forEach(function(p){raw.add(p);});
  simplifyIfNeeded(raw,smoothingAmt!==undefined?smoothingAmt:state.smoothing);
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
// ---- STROKE PROFILES (Sander van Dijk 6.2 "Taper Shape Layer Strokes",
// 6.3 "Stroke Gradients", 2026-07-26) ----------------------------------
// AE can only stroke a path at ONE width, so he asks for tapering and for
// gradients that run ALONG a stroke rather than across it. Nemo already has
// both capabilities — they are what the pressure brush is made of: a
// centreline (data.centerSegments, each anchor carrying its own width) that
// rebuildVectorBrushOutline turns into a FILLED ribbon. What was missing is
// a way to put an existing path into that machinery after the fact.
//
// So 6.2 is a conversion, and 6.3 comes free with it: once the stroke is a
// filled outline, the gradient system that already paints fills paints it
// along the ribbon, with no renderer change at all.
function strokeProfileWidthFn(kind, base) {
  var floor = taperFloorWidth();
  var span = Math.max(0, base - floor);
  switch (kind) {
    // Ramp lengths differ on purpose: a single-ended taper reads better
    // over a longer run, a double-ended one has to leave a body in between.
    case 'taper-in':   return function (t) { return floor + span * Math.min(1, t / 0.35); };
    case 'taper-out':  return function (t) { return floor + span * Math.min(1, (1 - t) / 0.35); };
    case 'taper-both': return function (t) { return taperWidthAtFrac(t, base, 0.18); };
    case 'bulge':      return function (t) { return floor + span * Math.sin(Math.PI * Math.min(1, Math.max(0, t))); };
    case 'even':       return function () { return base; };
    default:           return function () { return base; };
  }
}
// Applies a width profile to ONE path, in place. In place matters: the path
// keeps its identity, so strokeId (tween matching), groupId, channelTag,
// ownerId, its z-order and every other data.* tag survive untouched — the
// exact "new item type handled in one reader but not the others" risk
// CLAUDE.md §1 warns about, avoided by never creating a new item.
function applyStrokeProfileToPath(path, kind, baseOverride) {
  if (!path || !(path instanceof Path)) return false; // CompoundPath: no single centreline
  if (!path.segments || path.segments.length < 2) return false;
  var already = !!(path.data && path.data.isVectorBrush && path.data.centerSegments && path.data.centerSegments.length > 1);
  var base = baseOverride || (already ? (path.data.profileBase || state.brushSize) : (path.strokeWidth || state.brushSize));
  var fn = strokeProfileWidthFn(kind, base);
  // The profile has to go into the DENSE widthProfile, not only into the
  // per-anchor widths. rebuildVectorBrushOutline interpolates linearly
  // BETWEEN anchors, so on a 2-anchor path (a straight line drawn with the
  // Line tool — the most obvious thing to taper) a taper-both would set both
  // ends to the floor width and the interpolation would make the whole
  // stroke a hairline: the taper is not just invisible, it eats the stroke.
  // Measured before this fix: widths [0.6, 0.6] for a base of 3, and `bulge`
  // came out identical to `taper-both`.
  function denseProfile(f) {
    var out = [], N = 64;
    for (var i = 0; i <= N; i++) { var t = i / N; out.push({ t: t, width: f(t) }); }
    return out;
  }
  if (already) {
    // Re-profile the EXISTING centreline. Re-deriving it from path.segments
    // would be wrong: those are the outline by now, not the centreline.
    var cs = path.data.centerSegments;
    var lens = [0];
    for (var i = 1; i < cs.length; i++) lens.push(lens[i - 1] + new Point(cs[i].point).getDistance(new Point(cs[i - 1].point)));
    var total = lens[lens.length - 1] || 1;
    cs.forEach(function (sg, i2) { sg.width = fn(lens[i2] / total); });
    path.data.widthProfile = denseProfile(fn);
  } else {
    var ink = path.strokeColor || path.fillColor;
    if (!ink) return false; // nothing to paint the ribbon with
    // buildCenterSegmentsFromPath always writes a numeric .width — several
    // readers (scaleCenterSegments, setBrushSize, resample) index .width
    // with no default and would produce NaN geometry without it.
    path.data.centerSegments = buildCenterSegmentsFromPath(path, fn);
    path.data.widthProfile = denseProfile(fn);
    path.data.isVectorBrush = true;
    // The ribbon is FILLED. serP forces strokeColor to null for any
    // isVectorBrush item and desP nulls the fill when it is falsy, so an
    // ink left in strokeColor would come back invisible after one
    // save/reload — the failure is delayed, which is what makes it nasty.
    path.fillColor = ink;
    path.strokeColor = null;
  }
  path.data.profileBase = base;
  path.data.strokeProfile = kind;
  rebuildVectorBrushOutline(path); // synchronous: nothing else ever calls it
  return true;
}
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
    applyBrushKeyline(path); // re-derive from current fillColor/width — see app.js
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
    // simplifyIfNeeded (not a bare path.simplify(2.5)) — 2026-07 feedback:
    // "quand je mets stabilizer à 0 et smooth à 0, j'ai l'impression qu'il y
    // a encore une passe de smooth". This hardcoded 2.5 tolerance ran
    // UNCONDITIONALLY regardless of state.smoothing, so a Fill Brush shape
    // always got re-fit even with Smooth explicitly set to 0 — same
    // Paper.js `t||2.5` footgun simplifyIfNeeded's own comment already
    // documents (passing 0 silently falls back to the default tolerance,
    // so the guard has to skip the call entirely, not pass 0 through).
    if(path.data&&path.data.isFillShape)simplifyIfNeeded(path,state.smoothing);
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
  // Same source as the style (the last-selected path — the one whose look the
  // union takes on), so the merged shape keeps ONE coherent identity rather
  // than an arbitrary mix of the operands'.
  var islands=insertBooleanResult(boolLayer,boolLayer.children.length,result,style.fillColor,style.opacity,null,style.data);
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
  // Captured BEFORE any ribbon-expansion/replace below mutates or discards
  // `path` — this is what redraws a visible border around the eroded
  // result instead of silently dropping it (see insertBooleanResult's own
  // strokeInfo comment). Null for a genuine fill-only path (nothing to
  // restore, correctly stays borderless) and for vector-brush ink (its
  // fillColor already IS the visible ink, strokeColor is never the real
  // border there).
  var origStrokeInfo=(!isVB&&path.strokeColor)?{color:path.strokeColor,width:path.strokeWidth,cap:path.strokeCap,join:path.strokeJoin}:null;
  // Captured here for the same reason as origStrokeInfo: the ribbon-expansion
  // branch below can replace `path` with a `combined` clone that carries no
  // .data at all. See insertBooleanResult's BOOL_KEEP_DATA_* comment for what
  // survives an erase and what deliberately doesn't.
  var origData=path.data;
  // eraseExpandStrokeToFill/buildVariableWidthPath builds a ribbon by
  // flattening the path into a plain point SEQUENCE and sweeping a width
  // along it — built for an OPEN stroke's centerline, with no concept of
  // wrapping back around a closed loop. 2026-07 bug, found live: routing a
  // CLOSED path (the default for almost anything drawn — fillEnabled
  // defaults true, a closed shape's stroke traces the exact same contour
  // as its own fill) through this ribbon-builder exploded a shape with any
  // real curvature/concavity into a self-intersecting chaotic mess instead
  // of a clean notch (screenshot: comma-shaped green fill → tangle of
  // stray lines radiating from the erase point). A CLOSED path never
  // needs this at all — Paper.js already gives it well-defined area, so
  // erasing directly against its own polygon (whatever fillColor is set,
  // borrowing strokeColor as a stand-in on a fill-less closed loop) is
  // both correct AND simpler; the stroke that was ON it (origStrokeInfo,
  // captured above) gets redrawn around the resulting notched boundary by
  // insertBooleanResult below, same as ever.
  //
  // The ribbon/closed-clone machinery below is ONLY for an OPEN path,
  // which genuinely has no area of its own to subtract against directly:
  // its stroke (if any) needs real ink geometry, and Paper.js implicitly
  // closes an open path with a straight line to render its fillColor (that
  // implicit closure is what a closed=true CLONE reproduces — NOT the
  // synthetic ribbon, which was the earlier, now-fixed version of this
  // same bug: "sur une forme non fermé... si j'utilise la gomme sur fill
  // celui-ci est supprimé totalement").
  if(!isVB&&!path.closed){
    var strokeGeom=path.strokeColor?eraseExpandStrokeToFill(path):null;
    var fillGeom=null;
    if(path.fillColor){
      fillGeom=path.clone({insert:false});
      fillGeom.closed=true;
      fillGeom.strokeColor=null;
    }
    var combined=null;
    if(strokeGeom&&fillGeom){
      combined=fillGeom.unite(strokeGeom,{insert:false});
      fillGeom.remove();strokeGeom.remove();
    }else{
      combined=strokeGeom||fillGeom;
    }
    if(combined){
      combined.fillColor=path.fillColor||path.strokeColor;
      combined.opacity=path.opacity;
      layer.insertChild(idx,combined);
      path.remove();
      target=combined;
    }else{
      return;
    }
  }else if(!isVB&&path.closed&&!path.fillColor&&path.strokeColor){
    // Closed but fill-less (a decorative stroke-only loop, Fill off): no
    // fill polygon to erase against — borrow strokeColor as a temporary
    // fillColor so the subtract below has real area to work with. The
    // path is about to be removed and replaced by the subtract result
    // either way (see target.remove() further down), so mutating it here
    // directly is safe.
    target.fillColor=path.strokeColor;
  }
  if(!target.fillColor)return;
  var col=target.fillColor,op=target.opacity;
  var tIdx=layer.children.indexOf(target);
  // Built unconditionally (not just as the JS-fallback shape) so its own
  // area is available as a sanity reference for the WASM result below —
  // a single erase bite should never remove drastically more area than the
  // eraser's own footprint, no matter how thin/concave the target is.
  var eraserShape=(fromPt&&buildEraserCapsule(fromPt,worldPt,radius))||new Path.Circle({center:worldPt,radius:radius,insert:false});
  var eraserArea=Math.abs(eraserShape.area);
  var wasmRes=null;
  if(window.GeometryWasm&&window.GeometryWasm.ready){
    try{wasmRes=eraseAtPointWasm(target,worldPt,radius,fromPt);}
    catch(e){console.warn('[geometry-wasm] erase_at_point failed, falling back to Paper.js',e);wasmRes=null;}
  }
  if(wasmRes&&wasmRes.ok){
    // geo_booleanop (the WASM boolean-clipping engine) is numerically
    // fragile right at/near an existing vertex of the input polygons —
    // exactly the case an erase click near a shape's own corner or a
    // previous bite's edge can hit — and can return a near-empty/degenerate
    // difference for that one click location while an identical click 20px
    // away (landing in ordinary fill interior) works fine. Reported: "en
    // fonction de là où je gomme ça efface tout ou érase bien." Reject any
    // result that removed far more area than the eraser shape itself could
    // plausibly account for and fall back to Paper.js's mature, more
    // numerically robust boolean clipper instead of trusting it.
    var targetArea=Math.abs(target.area),wasmArea=Math.abs(wasmRes.item.area||0);
    var removedArea=targetArea-wasmArea;
    if(targetArea>eraserArea*4&&removedArea>eraserArea*6){
      console.warn('[geometry-wasm] erase_at_point result looked implausible (removed '+Math.round(removedArea)+' vs eraser footprint '+Math.round(eraserArea)+'), falling back to Paper.js');
      wasmRes.item.remove();
      wasmRes=null;
    }
  }
  var result;
  if(wasmRes&&wasmRes.ok){
    result=wasmRes.item;
    eraserShape.remove();
    target.remove();
  }else{
    result=target.subtract(eraserShape,{insert:false});
    eraserShape.remove();
    target.remove();
  }
  var hasArea=result&&((result.children&&result.children.length)||(result.segments&&result.segments.length));
  if(hasArea){
    result.fillColor=col;result.opacity=op;
    // A notch cut through a pressure-brush stroke's middle no longer has a
    // single consistent centerline (still fully colorable/selectable/
    // erasable, just not re-editable as a tapered centerline) — but it DOES
    // keep a plain outline at the original color/width around whatever
    // silhouette is left (origStrokeInfo, captured above before any
    // ribbon-expansion), rather than silently losing its border on the
    // first touch ("l'eraser supprime le stroke entier", 2026-07).
    insertBooleanResult(layer,Math.min(tIdx,layer.children.length),result,col,op,origStrokeInfo,origData);
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
// A CompoundPath child that's a genuine HOLE (fully enclosed inside a
// sibling, e.g. the eraser biting the MIDDLE of a shape without touching
// its outer edge — the single most common real erase gesture) used to be
// exploded into an independent, uniformly-fillColor'd Path by the loop
// below exactly like any other island — which turns a transparent cutout
// into a solid same-color PATCH sitting right on top of the hole, i.e. the
// erase bite visually does nothing at all ("l'eraser ... s'arrête alors
// que je drag encore", "supprime complètement" — a nearby unrelated
// island's hole-turned-patch masks whatever the eraser actually touched).
// serP()/desP() (app.js) have no CompoundPath support at all (they read
// `.segments` directly, which a CompoundPath doesn't have) — every frame
// save would corrupt/drop a raw hole-carrying CompoundPath left in the
// layer, so genuinely keeping the hole as nested Paper.js structure is not
// an option here without a much larger persistence-layer change. Instead:
// merge each hole into its parent exterior via a "keyhole" — slit from the
// hole's closest point out to the exterior's closest point and back,
// collapsing the two-contour (exterior+hole) shape into ONE ordinary,
// hole-free contour that every existing flat-Path consumer already
// understands, with the bite now visibly cut out of the silhouette.
function _polyClosestPair(a,b){
  var bestD=Infinity,bi=0,bj=0;
  for(var i=0;i<a.length;i++){
    for(var j=0;j<b.length;j++){
      var dx=a[i].x-b[j].x,dy=a[i].y-b[j].y,d=dx*dx+dy*dy;
      if(d<bestD){bestD=d;bi=i;bj=j;}
    }
  }
  return{i:bi,j:bj};
}
function _mergeHoleIntoExterior(extSegs,holeSegs){
  var pair=_polyClosestPair(extSegs.map(function(s){return s.point;}),holeSegs.map(function(s){return s.point;}));
  var out=[];
  for(var k=0;k<=pair.i;k++)out.push(extSegs[k]);
  for(var h=0;h<=holeSegs.length;h++)out.push(holeSegs[(pair.j+h)%holeSegs.length]);
  out.push(extSegs[pair.i]); // back out through the same slit point (zero-width seam)
  for(var k2=pair.i+1;k2<extSegs.length;k2++)out.push(extSegs[k2]);
  return out;
}
// Strips spurious zero-area "detour" loops from a closed path that revisits
// the exact same point more than once (2026-07, "encore des points de
// vecteur à l'intérieur" — a Fill Brush ring closed via buildClosedRingOutline's
// own zero-width-slit merge, later paint-bucket-filled and unite()'d back
// together via fillMergeSameColor). A single deliberate slit revisits its
// junction point exactly ONCE by design (see _mergeHoleIntoExterior above)
// — but feeding a path that already has one of these self-touching
// junctions through Paper.js's OWN `.unite()` isn't something its boolean
// clipper handles gracefully: confirmed live, the union came back with the
// same coordinate revisited 40 TIMES (292 points total, only 187 actually
// unique) — the clipper's internal sweep re-processing that touch point as
// several distinct intersection events. Every extra revisit brackets a
// sub-loop that starts and ends at the same coordinate — by construction
// that sub-loop's own closed shape is degenerate/near-zero-area clipper
// noise, never real geometry the artist drew, so splicing it out (keep the
// FIRST visit, remove everything through the next visit to the same point,
// repeat until no repeats remain) recovers the true minimal boundary.
// Verified live: 292 points -> 48, zero repeats left, shape area unchanged
// to within 0.3% (the removed loops really were negligible slivers).
function _eraseDegenerateSelfLoops(path){
  if(!path||!path.segments)return;
  // Distance-based (not rounded-key) revisit detection: two of Paper's own
  // union() outputs from the SAME real junction routinely land a fraction
  // of a px apart (confirmed live: 0.28px — computed by two independent
  // sources, a wraparound seam and a fresh anchor), which a naive
  // round-to-0.1px hash key can put in two DIFFERENT buckets and miss
  // entirely. 1.5px is well under anything a real drawn detail would ever
  // need two distinct anchors that close together for.
  var REVISIT_TOL=1.5;
  // AREA GUARD (2026-07-25). The splice below rests on the assumption stated
  // in the header comment — "by construction that sub-loop's own closed shape
  // is degenerate/near-zero-area clipper noise, never real geometry". That
  // holds for unite()'s re-processed touch points, but NOT for the one
  // deliberate revisit _mergeHoleIntoExterior creates: a hole merged into its
  // exterior through a zero-width slit revisits the slit anchor on purpose,
  // and the sub-loop bracketed by that revisit IS the hole.
  //
  // Without this guard the two passes fought each other, and the hole always
  // lost. Measured on a 200x200 square minus a 50px circle, straight through
  // insertBooleanResult:
  //
  //                        segments   area    bounds
  //   after hole merge        10      35000   100,100 200x200   (correct)
  //   after this function      3      61980    56, 56 279x279   (destroyed)
  //
  // The hole was spliced out entirely, and simplify() below then refit curves
  // through the three survivors, ballooning the outline past the original
  // shape. Every path that reaches insertBooleanResult with a hole was
  // affected: the boolean subtract/exclude tool, fill merge, and the eraser
  // piercing the middle of a filled shape.
  //
  // Scale-invariant threshold rather than an absolute one, so it behaves the
  // same on a thumbnail and a full-canvas shape. The two populations are ~3
  // orders of magnitude apart — the real hole above is 12.5% of its bounding
  // box, while the clipper slivers this function was written for measured
  // ~0.3% of shape area TOTAL across ~40 loops — so 0.2% sits far from either
  // edge rather than between them.
  var MIN_LOOP_FRAC=0.002;
  function _subLoopArea(segs,from,to){
    // Shoelace over the anchor ring segs[from..to]; handles ignored on
    // purpose, since this only has to separate "essentially nothing" from
    // "a real enclosed region".
    var a=0;
    for(var k=from;k<=to;k++){
      var p=segs[k].point,q=segs[k===to?from:k+1].point;
      a+=p.x*q.y-q.x*p.y;
    }
    return Math.abs(a)/2;
  }
  var bb=path.bounds, bbArea=Math.max(1,bb.width*bb.height);
  var minLoopArea=Math.max(1,bbArea*MIN_LOOP_FRAC);
  var changed=true,guard=0,keptRevisit=false;
  while(changed&&guard<5000){
    changed=false;guard++;
    var segs=path.segments,n=segs.length;
    for(var i=0;i<n;i++){
      for(var j=0;j<i;j++){
        if(segs[i].point.getDistance(segs[j].point)<=REVISIT_TOL){
          if(_subLoopArea(segs,j,i)>minLoopArea){keptRevisit=true;continue;} // real geometry (a hole), not clipper noise
          path.removeSegments(j+1,i+1);
          changed=true;
          break;
        }
      }
      if(changed)break;
    }
  }
  // Even with every exact/near revisit spliced out, Paper's boolean unite()
  // still routinely leaves the surviving anchors themselves oddly spaced
  // (a coarse, partly-flattened polygon rather than a fair curve) — a
  // plain `.smooth()` refit only straightens handles from whatever point
  // positions are already there, so unevenly-spaced anchors can still fit
  // into visibly kinked tangents (confirmed live: 2026-07, "la vertice
  // extérieur... perdent leur tangent" persisted on a real merged shape
  // even after the splice+smooth pass, with NO exact revisit for this
  // dedup to catch at all). `.simplify()` — a proper Douglas-Peucker +
  // least-squares bezier refit, same tolerance already used for Fill Brush
  // shapes elsewhere (rebuildVectorBrushOutline) — both re-fits smooth
  // tangents AND collapses the unevenly-spaced anchors that cause them,
  // unconditionally (safe/idempotent on an already-clean shape too, not
  // gated on whether a revisit was actually spliced like the old
  // smooth()-only pass was).
  // ...but NOT on a path that legitimately returns along its own slit
  // (2026-07-25). Douglas-Peucker + a least-squares bezier refit both assume a
  // simple, non-self-touching curve. Fed a hole merged through a zero-width
  // seam, the refit pulls the two coincident anchors apart into a bulge:
  // measured on a square-minus-circle donut, area 32972 (ideal 32146) and
  // bounds 100,100 200x200 going in, area 54286 and bounds 68,65 267x265
  // coming out. The area guard above having preserved a revisit is exactly the
  // signal that this path is one of those, so the refit is skipped for it —
  // and only for it, leaving the ordinary unite()-output case this pass was
  // written for untouched.
  if(!keptRevisit)path.simplify(2.5);
}
// strokeInfo (optional, 2026-07 — "l'eraser supprime le stroke entier"):
// every branch below used to hardcode strokeColor=null on its island(s),
// on the assumption a boolean-op result never has a meaningful outline to
// keep (true for a plain fill union/intersect). eraseAtPoint's own erase
// result is the one caller where that's wrong: the source path DID have a
// real visible strokeColor before erasing, and losing it entirely on the
// very first touch (instead of just notching the fill) reads as "the
// eraser deletes the whole border". Pass {color,width,cap,join} to redraw
// a stroke of the ORIGINAL color/width around whatever silhouette is left
// (can't preserve a tapered centerline through an arbitrary bite, but a
// plain outline at the same width/color is the closest visual match) —
// every other caller omits it and keeps the old strokeColor=null default.
// Identity/ownership metadata a boolean op must NOT silently drop (2026-07-26,
// found by erasing a corner off a grouped square: the bitten square came back
// with data === {} and had quietly left its own group). A boolean op replaces
// the source Path object entirely, and nothing here used to carry ANY of its
// .data across — so one eraser touch reset the shape's identity:
//   strokeId    → per-element Motion animation keyed to it is orphaned
//   groupId     → the shape drops out of its group, half the group left behind
//   owner*      → collaboration attribution stripped
//   linkedFillId→ the companion fill keeps pointing at an id no path carries
//   effects     → per-stroke effects list lost
// GEOMETRY tags are deliberately NOT in this list — isVectorBrush /
// centerSegments / widthProfile / strokeProfile / fillSeed* / brushTexture*
// all describe how to REBUILD the outline, and a notched outline no longer
// has a valid centerline to rebuild from (see eraseAtPoint's own comment on
// origStrokeInfo — that tradeoff is intentional and predates this).
var BOOL_KEEP_DATA_ALL=['groupId','ownerId','ownerName','ownerColor','channelTag','shadowSwatchId','tweenOn','boxAngle','xformAnchorKey','xformAnchorCustom','effects','fillGradient'];
// Keys that must stay UNIQUE across the layer: both are lookup map keys
// (motion.js:1413 scans for the first data.strokeId match, app.js:740 builds
// byId[linkedFillId]), so copying them onto every island of a split would
// make all but one unreachable. The first island inherits the identity; the
// extra islands are genuinely new shapes and get their OWN fresh id, so they
// stay individually addressable by per-element Motion instead of persisting
// with strokeId:null (nothing else mints one on save — app.js:1479 only does
// so inside mergeLayersIntoOne).
var BOOL_KEEP_DATA_FIRST=['strokeId','linkedFillId','isLinkedFillCompanion'];
function carryBooleanData(isl,srcData,isFirst){
  if(!srcData)return;
  BOOL_KEEP_DATA_ALL.forEach(function(k){if(srcData[k]!==undefined)isl.data[k]=srcData[k];});
  if(isFirst)BOOL_KEEP_DATA_FIRST.forEach(function(k){if(srcData[k]!==undefined)isl.data[k]=srcData[k];});
  else if(srcData.strokeId)ensureStrokeId(isl);
}
function insertBooleanResult(layer,insertAt,result,fillColor,opacity,strokeInfo,srcData){
  function applyStroke(isl){
    if(strokeInfo&&strokeInfo.color){isl.strokeColor=strokeInfo.color;isl.strokeWidth=strokeInfo.width;isl.strokeCap=strokeInfo.cap;isl.strokeJoin=strokeInfo.join;}
    else isl.strokeColor=null;
  }
  if(!(result instanceof CompoundPath)){
    // Every OTHER branch below applies fillColor/opacity/strokeColor=null
    // to its island(s) — this simplest one (the op produced one seamless
    // Path, no holes or islands at all, e.g. two same-color fills merging
    // into a single continuous outline) forgot to, silently inserting
    // `result` with whatever style it happened to inherit from Paper's own
    // unite()/etc (typically nothing — confirmed live: fillColor came back
    // null even though a color was explicitly passed in).
    _eraseDegenerateSelfLoops(result);
    if(fillColor!==undefined)result.fillColor=fillColor;
    applyStroke(result);
    if(opacity!==undefined)result.opacity=opacity;
    carryBooleanData(result,srcData,true);
    layer.insertChild(insertAt,result);
    return[result];
  }
  var children=result.children.slice();
  // clockwise===true is an exterior, false is a hole — exactly the
  // convention _polygonsToPaperItem (tools.js) already establishes when
  // building these from WASM boolean-op output.
  var exteriors=children.filter(function(c){return c.clockwise;});
  var holes=children.filter(function(c){return !c.clockwise;});
  // No holes at all (plain multi-island result, e.g. an eraser stroke that
  // split a shape in two) — unchanged prior behavior, one flat Path per
  // island, nothing to merge.
  if(!holes.length){
    var flat=exteriors.length?exteriors:children;
    flat.forEach(function(isl){isl.remove();});
    flat.forEach(function(isl,k){
      if(fillColor!==undefined)isl.fillColor=fillColor;
      applyStroke(isl);
      if(opacity!==undefined)isl.opacity=opacity;
      carryBooleanData(isl,srcData,k===0);
      layer.insertChild(insertAt+k,isl);
    });
    result.remove();
    return flat;
  }
  var islands=exteriors.map(function(ext){
    // Handles carried through, not zeroed (2026-07-25). This merge was written
    // for insertBooleanResult's straight WASM polygons, where every handle is
    // [0,0] anyway — but Paper's own subtract()/exclude() feed the SAME
    // function, and their output is curved. Zeroing turned a circular hole cut
    // by the boolean tool into a 4-corner diamond, and any curved exterior
    // into a polygon. Measured on a 50px-radius circular hole: area 5000
    // (diamond) instead of 7854 (circle).
    var extSegs=ext.segments.map(function(s){return{point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};});
    // Bounds-containment pairs each hole with the exterior it sits inside
    // — good enough since a hole can only ever nest inside the ONE
    // exterior it was cut from (_polygonsToPaperItem builds one hole list
    // per source polygon, never shared across exteriors).
    var myHoles=holes.filter(function(h){return ext.bounds.contains(h.bounds);});
    myHoles.forEach(function(h){
      var holeSegs=h.segments.map(function(s){return{point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};});
      extSegs=_mergeHoleIntoExterior(extSegs,holeSegs);
    });
    var merged=new Path({insert:false});
    extSegs.forEach(function(s){merged.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
    merged.closed=true;
    // Multiple holes merged into the same exterior can each independently
    // pick the SAME closest exterior point (_polyClosestPair) as their own
    // slit anchor — every extra hole compounds another revisit of that one
    // coordinate, same degenerate-loop shape _eraseDegenerateSelfLoops
    // already fixes after Paper's own unite() (see that function's comment).
    _eraseDegenerateSelfLoops(merged);
    return merged;
  });
  islands.forEach(function(isl,k){
    if(fillColor!==undefined)isl.fillColor=fillColor;
    applyStroke(isl);
    if(opacity!==undefined)isl.opacity=opacity;
    carryBooleanData(isl,srcData,k===0);
    layer.insertChild(insertAt+k,isl);
  });
  result.remove();
  return islands;
}

// ---- PRESSURE-SIMULATED VECTOR BRUSH ----
var _vb={pts:[],widths:[],lastT:0,lastPt:null};
// Mirrors draw-bridge.js's smoothPressure()/stabilizePoint() — this Paper-
// native path is only a fallback (the engine is on by default), but the two
// must stay in phase per this file's own duplication-hazard convention, or
// falling back here mid-session would visibly change how a stroke feels.
// One Euro Filter (2026-07, replaces a fixed 0.45-alpha exponential moving
// average — see draw-bridge.js's own copy of this exact comment for the
// full "moins naturel qu'un logiciel pro" rationale and tuning caveat).
function _vbMakeOneEuroFilter(mincutoff,beta,dcutoff){
  var xPrev=null,dxPrev=0,tPrev=null;
  function alpha(cutoff,dt){var tau=1/(2*Math.PI*cutoff);return 1/(1+tau/dt);}
  return function(x,tMs){
    if(xPrev==null){xPrev=x;tPrev=tMs;dxPrev=0;return x;}
    var dt=Math.max(0.001,(tMs-tPrev)/1000);
    tPrev=tMs;
    var dx=(x-xPrev)/dt;
    dxPrev=dxPrev+alpha(dcutoff,dt)*(dx-dxPrev);
    var cutoff=mincutoff+beta*Math.abs(dxPrev);
    var result=xPrev+alpha(cutoff,dt)*(x-xPrev);
    xPrev=result;
    return result;
  };
}
// Tuned constants MUST match draw-bridge.js's own copy exactly (see that
// file's comment for the empirical tuning rationale) — this is the
// engine-off fallback, per this file's own duplication-hazard convention.
var _VB_PRESSURE_MINCUTOFF=1.0,_VB_PRESSURE_BETA=20,_VB_PRESSURE_DCUTOFF=1.0;
var _vbPressureFilter=_vbMakeOneEuroFilter(_VB_PRESSURE_MINCUTOFF,_VB_PRESSURE_BETA,_VB_PRESSURE_DCUTOFF);
function _vbResetPressureFilter(){_vbPressureFilter=_vbMakeOneEuroFilter(_VB_PRESSURE_MINCUTOFF,_VB_PRESSURE_BETA,_VB_PRESSURE_DCUTOFF);}
function vbSmoothPressure(p){return _vbPressureFilter(p,Date.now());}
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
// Pressure response curve (ported from rnote's PressureCurve — style/mod.rs):
// remaps raw pressure BEFORE the min/max range mapping. sqrt/cbrt make a
// light touch already read wide ("soft" felt-tip feel), pow2/pow3 demand a
// firm press before the line thickens ("hard" pencil feel). 'linear' is
// exactly the historical behavior. Shared by BOTH width pipelines —
// widthFor (draw-bridge.js, Rust-engine path) and vbWidthFor below (legacy
// Paper.js path) — per this file's own duplication-hazard convention.
function applyPressureCurve(p){
  // Custom curve (2026-07, ui.js's editPressureCurve/pressureCurve) takes
  // priority the moment it exists — set only once the user actually opens
  // the dedicated curve editor and drags a point; the formula presets
  // below stay the default/fallback and remain fully functional on their
  // own (picking one from the dropdown clears pressureCurvePoints, see
  // its own change handler, timeline.js).
  if(state.pressureCurvePoints&&window.evalEasingPoints)return window.evalEasingPoints(state.pressureCurvePoints,p);
  switch(state.pressureCurve){
    case 'sqrt':return Math.sqrt(p);
    case 'cbrt':return Math.cbrt(p);
    case 'pow2':return p*p;
    case 'pow3':return p*p*p;
    default:return p; // 'linear'
  }
}
window.applyPressureCurve=applyPressureCurve;
function vbWidthFor(p){
  if(state.pressureInvert)p=1-p;
  p=applyPressureCurve(p);
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
    if(state.shadowMode)applyShadowBrushTag(outline);
    tagOwner(outline);
    if(window.SMSymmetry&&window.SMSymmetry.onStrokeCommitted)window.SMSymmetry.onStrokeCommitted(outline,userLayers[state.activeLayerIdx]);
  }else{
    if(state.shadowMode)applyShadowBrushTag(_pen.path);
    tagOwner(_pen.path);
    // Symmetry guide (symmetry-bridge.js, 2026-07): promoted from brush-only
    // to also cover the Pen tool — same single guarded call as
    // draw-bridge.js's commitStroke, right before the frame is saved so the
    // mirrored/rotated copy shares the same undo snapshot.
    if(window.SMSymmetry&&window.SMSymmetry.onStrokeCommitted)window.SMSymmetry.onStrokeCommitted(_pen.path,userLayers[state.activeLayerIdx]);
  }
  _pen.path=null;_pen.draggingHandle=false;
  saveActiveLayerFrame();updateUI();
}

function onMouseDown(event){
  if(state.playing){stopPlay();return;}
  if(state.tool==='hand'||state.spaceDown){state.isPanning=true;return;}
  if(state.tool==='zoom'){if(event.event.altKey)view.zoom=Math.max(.05,view.zoom*.8);else view.zoom=Math.min(20,view.zoom*1.25);updZoom();renderArcs();return;}
  // Motion mode (motion.js) is a persistent app-mode, not a tool value —
  // unlike the camera row below (a dedicated 'camera' tool that REPLACES
  // Select/Draw/etc.), its on-canvas position-handle dragging must coexist
  // with whatever tool is active. So it's an early intercept here, not
  // another branch of this tool chain: only consumes the event (returns
  // true) when the click actually lands on a motion handle/keyframe dot;
  // otherwise falls through unchanged into Select/Draw/etc. below.
  if(state.appMode==='motion'&&window.SMMotion&&SMMotion.onDown(event))return;
  var layer=userLayers[state.activeLayerIdx];
  if(state.tool==='draw'){
    if(!canEditActiveLayer())return;
    // Same both-eyes-off guard as draw-bridge.js's commitStroke — never
    // commit fully invisible ink.
    if(!state.strokeEnabled&&!state.fillEnabled){showToast('Stroke et Fill désactivés — rien à dessiner');return;}
    pushUndo();ensureKeyframe();layer.activate();
    if(state.vectorBrush){
      _vbLastPenPressure=null;_vbResetPressureFilter();stabQueue=[event.point.clone()];_vb.pts=[event.point.clone()];_vb.widths=[vbPressureOf(event)];_vb.lastT=Date.now();_vb.lastPt=event.point.clone();
      currentPath=new Path();currentPath.fillColor=state.strokeEnabled?state.strokeColor:state.fillColor;currentPath.strokeColor=null;currentPath.opacity=state.opacity/100;
      currentPath.data.isVectorBrush=true;
      applyBrushKeyline(currentPath); // visual separation from any nearby stroke — see app.js
    }else{
      // Stroke eye honored for the brush (was shape-tools-only) — stroke
      // OFF + fill ON draws a fill-only path, mirroring draw-bridge.js.
      currentPath=new Path();currentPath.strokeColor=state.strokeEnabled?state.strokeColor:null;currentPath.strokeWidth=state.brushSize;
      currentPath.strokeCap=state.strokeCap;currentPath.strokeJoin=state.strokeJoin;currentPath.fillColor=state.fillEnabled?state.fillColor:null;currentPath.opacity=state.opacity/100;
      applyStrokeStyle(currentPath);
    }
    currentPath.add(event.point);stabQueue=[event.point.clone()];
  }else if(state.tool==='pen'){
    if(!canEditActiveLayer())return;
    var now=Date.now();
    var isDoubleClick=_pen.path&&(now-_pen.lastClickTime<350)&&_pen.lastClickPt&&event.point.getDistance(_pen.lastClickPt)<10/view.zoom;
    _pen.lastClickTime=now;_pen.lastClickPt=event.point.clone();
    if(isDoubleClick){finalizePen();return;}
    if(!_pen.path){
      pushUndo();ensureKeyframe();layer.activate();
      _pen.path=new Path();_pen.path.strokeColor=state.strokeColor;_pen.path.strokeWidth=state.brushSize;
      _pen.path.strokeCap=state.strokeCap;_pen.path.strokeJoin=state.strokeJoin;_pen.path.fillColor=null;_pen.path.opacity=state.opacity/100;
      applyStrokeStyle(_pen.path);
      _pen.path.add(event.point);
    }else{
      var first=_pen.path.firstSegment.point;var tol=10/view.zoom;
      if(_pen.path.segments.length>1&&event.point.getDistance(first)<tol){
        _pen.path.closed=true;finalizePen();return;
      }
      // Shift-constrain (UI/UX audit, 2026-07) — Illustrator/Figma pen tool
      // convention: hold Shift while placing the NEXT anchor to snap its
      // angle from the PREVIOUS one to the nearest 45°, for clean
      // horizontal/vertical/diagonal construction lines. Deliberately
      // computed AFTER the close-loop hit-test above (against the real,
      // unconstrained event.point) so holding Shift can never make closing
      // the path near its own start point harder to hit.
      var placePt=event.point;
      // event.event.shiftKey (native browser event), not event.modifiers.shift
      // (Paper.js's own tracking) — live-caught 2026-07, "marche pas": this
      // app's own global keydown handlers (timeline.js onKeyDown et al.)
      // apparently intercept the Shift keydown before Paper.js's internal
      // Key-state listener ever sees it, so event.modifiers.shift silently
      // never reads true. event.event.shiftKey is the proven-reliable
      // pattern already used elsewhere in this exact file (the Zoom tool's
      // event.event.altKey, tools.js:3412).
      if(event.event.shiftKey)placePt=constrainAngle45(_pen.path.lastSegment.point,event.point);
      _pen.path.add(placePt);
    }
    _pen.draggingHandle=true;
  }else if(state.tool==='line'||state.tool==='rect'||state.tool==='ellipse'){
    if(!canEditActiveLayer())return;pushUndo();ensureKeyframe();layer.activate();shapeStart=event.point.clone();
    if(state.tool==='line')currentPath=new Path.Line({from:event.point,to:event.point,strokeColor:state.strokeEnabled?state.strokeColor:null,strokeWidth:state.brushSize,strokeCap:state.strokeCap,fillColor:null,opacity:state.opacity/100});
    else if(state.tool==='rect')currentPath=new Path.Rectangle({from:event.point,to:event.point.add(new Point(1,1)),strokeColor:state.strokeEnabled?state.strokeColor:null,strokeWidth:state.brushSize,fillColor:state.fillEnabled?state.fillColor:null,opacity:state.opacity/100});
    else currentPath=new Path.Ellipse({rectangle:new Rectangle(event.point,new Size(1,1)),strokeColor:state.strokeEnabled?state.strokeColor:null,strokeWidth:state.brushSize,fillColor:state.fillEnabled?state.fillColor:null,opacity:state.opacity/100});
  }else if(state.tool==='subselect'){
    var bestNh=null,bestNd=8/view.zoom;
    for(var ni=0;ni<nodeHandles.length;ni++){var nh=nodeHandles[ni];var nd=event.point.getDistance(nh.pos);if(nd<bestNd){bestNd=nd;bestNh=nh;}}
    if(bestNh){
      pushUndo();
      _nodeDrag.active=true;_nodeDrag.path=selectedPaths[0];_nodeDrag.segIndex=bestNh.segIndex;
      _nodeDrag.dragStartPointer=event.point.clone();_nodeDrag.appliedDelta=new Point(0,0);
      // grabbing one of several marquee-selected anchors drags them all
      if(bestNh.type==='point'&&_nodeSel.indexOf(bestNh.segIndex)>=0&&_nodeSel.length>1){_nodeDrag.type='group';}
      else{
        _nodeDrag.type=bestNh.type;
        if(bestNh.type==='point'){_nodeSel=event.modifiers.shift?_nodeSel.concat([bestNh.segIndex]):[bestNh.segIndex];renderNodeHandles();}
      }
      return;
    }
    var subHit=layer.hitTest(event.point,{stroke:true,fill:true,pixel:true,tolerance:8/view.zoom});
    // Isolation entered by a Select double-click: only the isolated shape
    // (or its group) is reachable, and clicking anywhere else leaves rather
    // than selecting something new — same contract fsselect had, kept so
    // the gesture behaves identically now that it enters subselect instead.
    if(_fsIsolation&&subHit){
      var subAllowed=_fsIsolation.groupId
        ? !!(subHit.item.data&&subHit.item.data.groupId===_fsIsolation.groupId)
        : subHit.item===_fsIsolation.path;
      if(!subAllowed)subHit=null;
    }
    if(!subHit&&_fsIsolation){
      _fsIsolation=null;clearSel();window.SM.setTool('select');
      renderArcs();updateUI();
      if(window.SMEngineBridge)SMEngineBridge.renderNow();
      return;
    }
    // Raster companions resolve to their anchor too (Bitmap Brush v2) —
    // see the same fix's full comment in subselect-bridge.js.
    if(subHit&&(subHit.item instanceof Path||(subHit.item instanceof Raster&&subHit.item.data&&subHit.item.data.isBrushTextureCopy))){
      clearSel();
      var subTarget=resolveBrushAnchor(subHit.item,layer);
      if(!(subTarget instanceof Path)){renderArcs();updateUI();return;}
      selectedPaths.push(subTarget);state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
      // nodeHandles (hit-test array) is stale until rebuilt — see the same
      // fix's comment in subselect-bridge.js for the full story.
      renderNodeHandles();
    }else{
      // Marquee now always starts here regardless of prior selection — see
      // subselect-bridge.js's identical fix comment for the full story.
      _nmq.active=true;_nmq.start=event.point.clone();_nmq.rect=null;
      return;
    }
    renderArcs();updateUI();
  }else if(state.tool==='comment'){
    // Existing pin nearby -> reopen it for editing; otherwise start a new
    // one anchored at the click. Hit radius is in world units scaled by
    // zoom so it feels consistent regardless of how far zoomed in you are.
    var hitRadius=14/view.zoom;
    var existing=(state.comments||[]).filter(function(cm){return cm.frame===state.currentFrame;})
      .find(function(cm){return event.point.getDistance(new Point(cm.x,cm.y))<hitRadius;});
    openCommentPopover(event.point,existing);
  }else if(state.tool==='camera'){
    if(window.SMCamera)SMCamera.onDown(event);
  }else if(state.tool==='text'){
    // A plain click still opens the point-text popover immediately (see
    // onMouseUp's companion branch — a click-with-no-drag never enters the
    // preview-rectangle path below, matching pre-2026-07 behavior exactly).
    // A drag defines an area-text box instead (Illustrator "type in a box"),
    // finalized in onMouseUp once the drag distance is known.
    _textDragStart=event.point.clone();
  }else if(state.tool==='fsselect'){
    var selectedFragment=fsSelectionAtPoint(event.point);
    if(selectedFragment&&!event.event.shiftKey){
      pushUndo();
      var promoted=fsPromoteSelectionForTransform(layer);
      fsClearSel();_fsIsolation=null;
      selectedPaths=promoted;
      state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
      window.SM.setTool('select');
      _moveDragStarted=true;
      renderTransformHandles();renderNodeHandles();updateUI();
      return;
    }
    var fsHit=fsHitTest(event.point,layer);
    if(_fsIsolation&&fsHit){
      var allowed=_fsIsolation.groupId
        ? !!(fsHit.path.data&&fsHit.path.data.groupId===_fsIsolation.groupId)
        : fsHit.path===_fsIsolation.path;
      if(!allowed)fsHit=null;
    }
    // event.event.shiftKey/altKey (native event), not event.modifiers.shift/alt
    // (Paper.js's own tracking) -- same proven-reliable pattern the Pen tool
    // and Zoom tool already use in this exact file: this app's own global
    // keydown handlers intercept the key before Paper's internal listener
    // ever sees it, so event.modifiers silently never reads true here.
    if(fsHit){
      // Shift toggles this hit in/out of the multi-selection (2026-07,
      // "impossible de faire de la multiselection avec fill/stroke select
      // + shift") — a plain click still replaces the whole selection with
      // just this one hit, same as before.
      if(event.event.shiftKey){
        var fsExistIdx=_fsSel.findIndex(function(s){return s.path===fsHit.path&&s.kind===fsHit.kind;});
        if(fsExistIdx>=0)_fsSel.splice(fsExistIdx,1);else _fsSel.push(fsHit);
      }else{
        _fsSel=[fsHit];
      }
    }else{
      if(_fsIsolation){
        _fsIsolation=null;fsClearSel();window.SM.setTool('select');updateUI();
        if(window.SMEngineBridge)SMEngineBridge.renderNow();
        return;
      }
      if(!event.event.shiftKey)fsClearSel();
      // Clicked empty canvas: start a rubber-band marquee (Alt held =
      // temporarily flips to/from lasso) — same feedback, "il faudrait les
      // outils de lasso et rectangle de selection pour cet outil". The
      // default mode (rect vs lasso) is set via the two dedicated buttons
      // in the Labs floating panel (labs-float-panel.js, state.fsSelectMode,
      // 2026-07: original ask was for VISIBLE buttons, not just a hidden
      // Alt-modifier convention) — Alt still works as a one-off override of
      // whichever mode is currently selected. Reuses the Select tool's own
      // _marquee state/rendering below (tagged with .mode so onMouseUp
      // knows to resolve it into _fsSel instead of selectedPaths), rather
      // than a second marquee implementation.
      var fsWantLasso=state.fsSelectMode==='lasso';
      _marquee.active=true;_marquee.start=event.point.clone();_marquee.rect=null;_marquee.lasso=event.event.altKey?!fsWantLasso:fsWantLasso;_marquee.mode='fsselect';
    }
    updateUI();
    // A plain click-to-select mutates no layer content, so it never bumps
    // window._sceneVersion (saveActiveLayerFrame/loadFrame's job) — without
    // this the highlight overlay wouldn't actually appear until some
    // unrelated action happened to trigger the next render tick.
    if(window.SMEngineBridge&&window.SMEngineBridge.renderNow)window.SMEngineBridge.renderNow();
  }else if(state.tool==='select'){
    for(var i=0;i<arcHandles.length;i++){var ah=arcHandles[i];if(event.point.getDistance(ah.handle.position)<14/view.zoom){draggingArc=ah;arcDragCache=computeArcMatchState();return;}}draggingArc=null;
    var bestXh=null,bestXd=9/view.zoom;
    // Ring band check first (2026-07): a 'rotate' entry now stores
    // {center,radius} instead of a single pos — anywhere within ~7px of
    // the circumference counts, matching select-bridge.js's ring hit-test.
    var ringTol=7/view.zoom;
    for(var xi=0;xi<xformHandles.length;xi++){
      var xh=xformHandles[xi];
      if(xh.type==='rotate'){
        if(Math.abs(event.point.getDistance(xh.center)-xh.radius)<ringTol){bestXh=xh;break;}
        continue;
      }
      var xd=event.point.getDistance(xh.pos);if(xd<bestXd){bestXd=xd;bestXh=xh;}
    }
    if(bestXh){
      pushUndo();
      var xb=xformSelBounds();
      if(bestXh.type==='rotate'){
        _xform.active=true;_xform.type='rotate';_xform.center=xb.center.clone();
        _xform.startAngle=Math.atan2(event.point.y-_xform.center.y,event.point.x-_xform.center.x)*180/Math.PI;_xform.lastAngle=0;
      }else{
        var anchorMap={nw:'se',ne:'sw',sw:'ne',se:'nw',n:'s',s:'n',e:'w',w:'e'};
        var xbx=orientedSelBox()||{b:xb,angle:0,pivot:xb.center};
        var anchors={nw:selBoxPt(xbx.b.left,xbx.b.top,xbx),ne:selBoxPt(xbx.b.right,xbx.b.top,xbx),sw:selBoxPt(xbx.b.left,xbx.b.bottom,xbx),se:selBoxPt(xbx.b.right,xbx.b.bottom,xbx),n:selBoxPt(xbx.b.center.x,xbx.b.top,xbx),s:selBoxPt(xbx.b.center.x,xbx.b.bottom,xbx),e:selBoxPt(xbx.b.right,xbx.b.center.y,xbx),w:selBoxPt(xbx.b.left,xbx.b.center.y,xbx)};
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
        selectedPaths=userLayers[compHit.layerIdx].children.filter(function(c){return (c instanceof Path||c instanceof Raster)&&isSelectablePathChild(c);});
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
        selectedPaths=userLayers[state.activeLayerIdx].children.filter(function(c){return (c instanceof Path||c instanceof Raster)&&isSelectablePathChild(c);});
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
    if(!canEditActiveLayer())return;
    // pushUndo() BEFORE ensureKeyframe(), and unconditionally — see
    // draw-bridge.js's commitStroke comment: one undo must revert both the
    // frame's auto-promotion to keyframe and whatever this gesture erases.
    pushUndo();ensureKeyframe();
    // instanceof Path AND CompoundPath, not just Path: eraseAtPoint (below)
    // turns a shape into a CompoundPath the moment a bite creates a hole
    // (a donut shape can't be represented as a single Path) — if the guard
    // only accepted `instanceof Path`, erasing again into that same shape
    // silently no-opped (hit-test still found it, but the type check
    // rejected it), matching the reported "sometimes erases, sometimes
    // not" — it worked until the first bite, then stopped for that shape.
    // Exact (zero-tolerance) hit wins first, same fix as eraser-bridge.js's
    // eraseAt() — this native Paper.js path is the fallback used whenever
    // the Rust engine isn't enabled, and never got that fix (2026-07
    // feedback: "l'outil eraser... s'arrête alors que je drag encore" — a
    // stray leftover fragment well within the radius-derived tolerance but
    // outside the visible cursor circle can steal the hit from the shape
    // actually under the cursor, reading as "erases the whole fill in one
    // go" then "stops responding" once nearby fragments run out).
    var hit2=layer.hitTest(event.point,{stroke:true,fill:true,tolerance:0});
    if(!hit2)hit2=layer.hitTest(event.point,{stroke:true,fill:true,tolerance:Math.max(8,state.eraserSize/2)/view.zoom});
    if(hit2&&(hit2.item instanceof Path||hit2.item instanceof CompoundPath)&&(hit2.item.strokeColor||hit2.item.fillColor||(hit2.item.data&&hit2.item.data.isVectorBrush))){
      _eraseDragActive=true;
      var erasedItem2=hit2.item;
      eraseAtPoint(erasedItem2,event.point,state.eraserSize/2);
      _eraseLastPt=event.point.clone();
      fillRegenerateLinked(layer,erasedItem2);saveActiveLayerFrame();updateUI();
    }
  }else if(state.tool==='fill'){
    if(!canEditActiveLayer())return;
    if(event.event.altKey){
      // Alt+drag: draw a TEMPORARY closing stroke to help the fill engine
      // bridge a region its own crossing/gap detection can't close on its
      // own (e.g. a busy junction where the wall-follower can't find a
      // simple loop) — visual-only dashed guide, never added to the real
      // document. Materialized as a real (but disposable) wall for exactly
      // the next non-alt fill click, then discarded whether that click
      // used it or not (see the non-alt branch below). Purely visual — no
      // pushUndo()/ensureKeyframe() here, nothing is mutated yet.
      _fillCloseDrag={points:[event.point.clone()]};
      window.SMEngineBridge.suspend();
      window.SMEngineBridge.renderWithOverlayItem(fillCloseOverlayItems());
      return;
    }
    // pushUndo() BEFORE ensureKeyframe(), and unconditionally — see
    // draw-bridge.js's commitStroke comment: one undo must revert both the
    // frame's auto-promotion to keyframe and whatever this click does.
    pushUndo();ensureKeyframe();
    if(event.modifiers.shift){
      var hitRm=layer.hitTest(event.point,{fill:true,tolerance:12/view.zoom});
      if(hitRm&&hitRm.item instanceof Path&&hitRm.item.fillColor){hitRm.item.fillColor=null;saveActiveLayerFrame();updateUI();showToast('Fill supprimé');}
      return;
    }
    // Any queued Alt-drawn closing strokes become real (but disposable)
    // walls for THIS click only — inserted and removed synchronously, so
    // they never reach any layer.children consumer (buildSceneJson, save,
    // selection, tween matching…) and need no special-casing there. Gone
    // after this click regardless of whether the fill succeeded, matching
    // "une fois le pot de peinture sans alt est fait dedans celle-ci
    // s'efface et disparaît complètement".
    var _tempCloseWalls=fillMaterializeTempCloseStrokes(layer);
    var res=fillVectorFind(event.point,layer,null);
    fillRemoveTempCloseStrokes(_tempCloseWalls);
    if(res)fillConsumeCloseStrokes(res.wallIds);
    if(!res){
      // No traceable closed region from the surrounding walls — but if the
      // click landed directly inside an already-filled shape, recolor it in
      // place instead of rejecting. Matches Animate's paint bucket: clicking
      // inside an existing fill always recolors it, even when there's no
      // fresh wall geometry around it to retrace a brand-new region from.
      var hitFill=layer.hitTest(event.point,{fill:true,tolerance:1/view.zoom});
      if(hitFill&&(hitFill.item instanceof Path||hitFill.item instanceof CompoundPath)&&hitFill.item.fillColor){
        hitFill.item.fillColor=state.fillColor;hitFill.item.opacity=state.opacity/100;
        saveActiveLayerFrame();updateUI();showToast('Couleur remplacée');return;
      }
      showToast('Aucune zone fermée ici');return;
    }
    var existingMatch=fillFindExistingMatch(layer,res.path);
    if(existingMatch){
      res.path.remove();
      existingMatch.fillColor=state.fillColor;existingMatch.opacity=state.opacity/100;
      saveActiveLayerFrame();updateUI();showToast('Couleur remplacée');return;
    }
    layer.insertChild(fillInsertIndexFor(layer,event.point,res.path),res.path);
    res.path.fillColor=state.fillColor;res.path.strokeColor=null;res.path.opacity=state.opacity/100;
    // remembers where/how this fill was made so subselection edits to the
    // strokes around it can regenerate it in place (see fillRegenerateLinked)
    res.path.data.fillSeed=[event.point.x,event.point.y];
    res.path.data.fillGapPx=res.gapPx;
    if(res.wallIds&&res.wallIds.length)res.path.data.fillWalls=res.wallIds;
    // allowFillShapeAbsorb omitted (falsy) — the paint bucket should never
    // reshape an already-finished, hand-authored Fill Brush shape it
    // merely borders (see fillMergeSameColor's own comment).
    fillMergeSameColor(layer,res.path);
    saveActiveLayerFrame();updateUI();showToast('Fill appliqué');
  }else if(state.tool==='fillbrush'){
    if(!canEditActiveLayer())return;pushUndo();ensureKeyframe();layer.activate();
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
  if(state.tool==='camera'){if(window.SMCamera)SMCamera.onDrag(event);return;}
  if(state.appMode==='motion'&&window.SMMotion&&SMMotion.onDrag(event))return;
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
  }else if(state.tool==='fill'&&_fillCloseDrag){
    _fillCloseDrag.points.push(event.point.clone());
    window.SMEngineBridge.renderWithOverlayItem(fillCloseOverlayItems());
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
    // Shift-constrain (UI/UX audit, 2026-07): Illustrator/Figma/Photoshop
    // convention, absent here entirely before this — Rectangle/Ellipse had
    // no way to draw a perfect square/circle, Line no way to snap to a
    // clean 0/45/90° angle, both routine, high-frequency needs when
    // roughing out a layout or a straight construction line.
    var endPt=event.point;
    // event.event.shiftKey, not event.modifiers.shift — see the Pen tool's
    // identical fix a few lines up for the full explanation.
    if(event.event.shiftKey){
      if(state.tool==='line')endPt=constrainAngle45(shapeStart,event.point);
      else endPt=constrainSquare(shapeStart,event.point);
    }
    if(state.tool==='line'){currentPath=new Path.Line({from:shapeStart,to:endPt,strokeColor:state.strokeEnabled?state.strokeColor:null,strokeWidth:state.brushSize,strokeCap:state.strokeCap,fillColor:null,opacity:state.opacity/100});applyStrokeStyle(currentPath);}
    else if(state.tool==='rect')currentPath=new Path.Rectangle({from:shapeStart,to:endPt,strokeColor:state.strokeEnabled?state.strokeColor:null,strokeWidth:state.brushSize,fillColor:state.fillEnabled?state.fillColor:null,opacity:state.opacity/100});
    else{currentPath=new Path.Ellipse({rectangle:new Rectangle(shapeStart,endPt),strokeColor:state.strokeEnabled?state.strokeColor:null,strokeWidth:state.brushSize,fillColor:state.fillEnabled?state.fillColor:null,opacity:state.opacity/100});}
  }else if(state.tool==='text'&&_textDragStart){
    // Drag guide rectangle — purely visual (marqueeLayer, never inserted
    // into real content), same pattern as the subselect marquee just below.
    // Only x matters for the actual box (wrapping is width-only, no fixed-
    // height clipping in this implementation); a nominal height is drawn
    // just so the drag reads as "defining a box" rather than a stray line.
    if(_textDragRect){_textDragRect.remove();_textDragRect=null;}
    var twv=Math.abs(event.point.x-_textDragStart.x);
    if(twv>4/view.zoom){
      var tx1=Math.min(_textDragStart.x,event.point.x),ty1=Math.min(_textDragStart.y,event.point.y);
      var tx2=Math.max(_textDragStart.x,event.point.x);
      var prevTxt=project.activeLayer;marqueeLayer.activate();
      _textDragRect=new Path.Rectangle({from:new Point(tx1,ty1),to:new Point(tx2,ty1+60/view.zoom),strokeColor:'rgba(255,184,108,.9)',strokeWidth:1/view.zoom,dashArray:[4/view.zoom,3/view.zoom],fillColor:new Color(1,0.72,0.42,0.06),insert:true});
      prevTxt.activate();
    }
  }else if(state.tool==='subselect'){
    var nodeEventPoint=event.point,nodeEventDelta=event.delta;
    if(_nodeDrag.active&&(_nodeDrag.type==='point'||_nodeDrag.type==='group')){
      var nodeDesired=nodeEventPoint.subtract(_nodeDrag.dragStartPointer||nodeEventPoint.subtract(event.delta));
      if(event.event.shiftKey){
        var nodeSnap=constrainAngle45(_nodeDrag.dragStartPointer,nodeEventPoint);
        nodeDesired=nodeSnap.subtract(_nodeDrag.dragStartPointer);
      }
      nodeEventDelta=nodeDesired.subtract(_nodeDrag.appliedDelta||new Point(0,0));
      _nodeDrag.appliedDelta=nodeDesired;
    }else if(_nodeDrag.active&&event.event.shiftKey&&(_nodeDrag.type==='handleIn'||_nodeDrag.type==='handleOut')){
      var nodeHs=nodeEditSegmentsData(_nodeDrag.path)[_nodeDrag.segIndex];
      if(nodeHs)nodeEventPoint=constrainAngle45(new Point(nodeHs.point[0],nodeHs.point[1]),nodeEventPoint);
    }
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
        _nodeSel.forEach(function(si){var cs3=gp.data.centerSegments[si];if(cs3)cs3.point=[cs3.point[0]+nodeEventDelta.x,cs3.point[1]+nodeEventDelta.y];});
        rebuildVectorBrushOutline(gp);
      }else{
        _nodeSel.forEach(function(si){var sg=gp.segments[si];if(sg)sg.point=sg.point.add(nodeEventDelta);});
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
        if(_nodeDrag.type==='point'){scs.point=[scs.point[0]+nodeEventDelta.x,scs.point[1]+nodeEventDelta.y];}
        else if(_nodeDrag.type==='handleOut'){var sno=[nodeEventPoint.x-scs.point[0],nodeEventPoint.y-scs.point[1]];scs.handleOut=sno;if(!event.modifiers.alt)scs.handleIn=[-sno[0],-sno[1]];}
        else if(_nodeDrag.type==='handleIn'){var sni=[nodeEventPoint.x-scs.point[0],nodeEventPoint.y-scs.point[1]];scs.handleIn=sni;if(!event.modifiers.alt)scs.handleOut=[-sni[0],-sni[1]];}
        rebuildVectorBrushOutline(sdp);
      }else{
        var sseg=sdp.segments[_nodeDrag.segIndex];
        if(_nodeDrag.type==='point')sseg.point=sseg.point.add(nodeEventDelta);
        else if(_nodeDrag.type==='handleOut'){sseg.handleOut=nodeEventPoint.subtract(sseg.point);if(!event.modifiers.alt)sseg.handleIn=sseg.handleOut.multiply(-1);}
        else if(_nodeDrag.type==='handleIn'){sseg.handleIn=nodeEventPoint.subtract(sseg.point);if(!event.modifiers.alt)sseg.handleOut=sseg.handleIn.multiply(-1);}
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
          if(p.data&&p.data.brushCompanions)p.data.brushCompanions.forEach(function(c){if(!c.removed)c.rotate(stepAngle,_xform.center);});
        });
        _xform.lastAngle=deltaFromStart;
        selectedPaths.forEach(function(p){if(p)p.data.boxAngle=(((p.data&&p.data.boxAngle)||0)+stepAngle)%360;}); // orientation lives on the stroke
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
          if(p.data&&p.data.brushCompanions)p.data.brushCompanions.forEach(function(c){if(!c.removed)c.scale(stepSx,stepSy,anchor);});
        });
        _xform.lastSx=sx;_xform.lastSy=sy;
        symGestureAccumulate(new Matrix().scale(stepSx,stepSy,anchor));
      }
      renderTransformHandles();
    }else if(_marquee.active){
      var prevA=project.activeLayer;marqueeLayer.activate();
      if(_marquee.lasso){
        // Freehand lasso (Alt-held drag) — grows an open Path point-by-point,
        // same convention select-bridge.js's own lasso already uses.
        if(!_marquee.rect)_marquee.rect=new Path({segments:[_marquee.start],closed:false,strokeColor:'rgba(74,158,255,.9)',strokeWidth:1/view.zoom,dashArray:[4/view.zoom,3/view.zoom],fillColor:new Color(0.29,0.62,1,0.08),insert:true});
        _marquee.rect.add(event.point);
      }else{
        var mx1=Math.min(_marquee.start.x,event.point.x),my1=Math.min(_marquee.start.y,event.point.y);
        var mx2=Math.max(_marquee.start.x,event.point.x),my2=Math.max(_marquee.start.y,event.point.y);
        if(_marquee.rect)_marquee.rect.remove();
        _marquee.rect=new Path.Rectangle({from:new Point(mx1,my1),to:new Point(mx2,my2),strokeColor:'rgba(74,158,255,.9)',strokeWidth:1/view.zoom,dashArray:[4/view.zoom,3/view.zoom],fillColor:new Color(0.29,0.62,1,0.08),insert:true});
      }
      prevA.activate();
    }else if(draggingArc){setArcHandle(draggingArc.fA,draggingArc.fB,draggingArc.matchIdx,draggingArc.which,draggingArc.ptA,draggingArc.ptB,event.point.x,event.point.y);renderArcs(arcDragCache);}
    else if(selectedPaths.length>0){
      if(!_moveDragStarted){pushUndo();_moveDragStarted=true;}
      selectedPaths.forEach(function(p){
      p.position=p.position.add(event.delta);
      if(p.data&&p.data.isVectorBrush&&p.data.centerSegments){p.data.centerSegments.forEach(function(s){s.point=[s.point[0]+event.delta.x,s.point[1]+event.delta.y];});}
    });renderNodeHandles();renderTransformHandles();
    symGestureAccumulate(new Matrix().translate(event.delta));}
  }else if(state.tool==='eraser'){
    eraseUpdateCursor(event);
    if(!canEditActiveLayer())return;
    var layer2e=userLayers[state.activeLayerIdx];
    // A fast mouse/tablet sweep can jump the cursor several eraser-widths
    // between two consecutive drag events — hit-testing only the NEW point
    // then missed shapes the sweep actually crossed, so the erased trail got
    // gappier the faster you moved (stutter/inconsistent flow). A real brush
    // doesn't have this problem because its dabs are spaced by arc length,
    // not by however many move events the OS happened to deliver. Resample
    // the segment since the last point at a fixed max step (a fraction of
    // the eraser's own radius) so the bite trail stays dense and continuous
    // regardless of input sampling rate.
    var fromP=_eraseLastPt||event.point;
    var segDist=fromP.getDistance(event.point);
    var step=Math.max(1,state.eraserSize/4);
    // Cap the per-event substep count: each substep that hits pays a real
    // boolean subtract, and eraseAtPoint's capsule (fromPt->subPt) already
    // guarantees the BITE itself is continuous whatever the spacing — the
    // resampling only exists so the hit-TEST can't jump clean over an item
    // on a fast sweep. Past ~40 samples per event, coarser spacing loses
    // nothing except the pathological worst-case cost spike.
    var steps=Math.min(40,Math.max(1,Math.ceil(segDist/step)));
    var erasedAny=false;
    for(var esi=1;esi<=steps;esi++){
      var subPt=fromP.add(event.point.subtract(fromP).multiply(esi/steps));
      // Exact-hit-first — same fix as onMouseDown's own eraser hitTest just
      // above (see its comment).
      var hit=layer2e.hitTest(subPt,{stroke:true,fill:true,tolerance:0});
      if(!hit)hit=layer2e.hitTest(subPt,{stroke:true,fill:true,tolerance:Math.max(8,state.eraserSize/2)/view.zoom});
      if(hit&&(hit.item instanceof Path||hit.item instanceof CompoundPath)&&(hit.item.strokeColor||hit.item.fillColor||(hit.item.data&&hit.item.data.isVectorBrush))){
        if(!_eraseDragActive){pushUndo();_eraseDragActive=true;}
        eraseAtPoint(hit.item,subPt,state.eraserSize/2,_eraseLastPt);
        erasedAny=true;
      }
      _eraseLastPt=subPt.clone();
    }
    if(erasedAny){fillRegenerateLinked(layer2e,null);saveActiveLayerFrame();updateUI();}
  }
}
function onMouseUp(event){
  if(state.isPanning){state.isPanning=false;return;}if(state.playing)return;
  if(state.tool==='camera'){if(window.SMCamera)SMCamera.onUp(event);return;}
  if(state.appMode==='motion'&&window.SMMotion&&SMMotion.onUp(event))return;
  _eraseDragActive=false;_eraseLastPt=null;
  if(state.tool==='fill'&&_fillCloseDrag){
    if(_fillCloseDrag.points.length>=2)_fillCloseStrokes.push({id:'fc'+Date.now().toString(36)+'_'+(++_fillCloseIdCounter),points:_fillCloseDrag.points.map(function(p){return[p.x,p.y];})});
    _fillCloseDrag=null;
    window.SMEngineBridge.resume();
    window.SMEngineBridge.renderNow();
    showToast(_fillCloseStrokes.length+' trait(s) de fermeture en attente — clic sans Alt pour remplir');
    return;
  }
  if(state.tool==='draw'&&currentPath){
    if(state.vectorBrush){
      // Catch-up point, mirrors draw-bridge.js's onUp — the stabilizer's
      // trailing average otherwise leaves the stroke stopping short of
      // wherever the pen actually lifted.
      _vb.pts.push(event.point.clone());_vb.widths.push(vbSmoothPressure(vbPressureOf(event)));
      if(_vb.pts.length<2){currentPath.remove();if(state.undoStack.length)state.undoStack.pop();}
      else if(!state.strokeEnabled){
        // Stroke eye OFF: fill-only — commit the enclosed region as a plain
        // closed filled shape instead of the pressure ribbon (mirrors
        // draw-bridge.js's own fill-only branch in commitStroke).
        var rawWidthsF=_vb.pts.map(function(p,i){return vbWidthFor(_vb.widths[i]);});
        var csF=buildCenterSegmentsFromRawStroke(_vb.pts,rawWidthsF,state.smoothing);
        currentPath.removeSegments();
        csF.forEach(function(s){currentPath.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
        currentPath.closed=true;
        currentPath.fillColor=state.fillColor;currentPath.strokeColor=null;
        if(state.shadowMode){
          currentPath.data.isVectorBrush=true;
          currentPath.data.centerSegments=csF;
        }else delete currentPath.data.isVectorBrush;
      }
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
    }else if(!state.shadowMode&&state.bitmapBrushOn&&state.strokeEnabled&&window.SMBitmapBrush){
      // Bitmap Brush's tools.js mirror (v2, 2026-07) — same anchor+
      // companion call as draw-bridge.js's commitStroke: the plain path
      // Paper built during the drag stays as the real, subselect-editable
      // anchor (fill kept, stroke camouflaged inside applyToPath), the
      // baked texture is its Raster companion. This fallback never called
      // applyBrushTexture for the EXISTING vector presets (grepped: zero
      // call sites), so no live preview here matches that precedent too.
      simplifyIfNeeded(currentPath,state.smoothing);
      // No per-point pressure available to pass here (unlike draw-bridge.js's
      // samples array) — this plain constant-width branch never captured it
      // in this fallback path (only the separate vectorBrush/_vb.widths
      // path does). Same "dead code when the Rust engine is on, the
      // default" acceptable-gap precedent as this file's other Bitmap
      // Brush mirror comments — flat size only here.
      window.SMBitmapBrush.applyToPath(currentPath);
    }else{
      simplifyIfNeeded(currentPath,state.smoothing);
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
    if(currentPath&&state.shadowMode)applyShadowBrushTag(currentPath);
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
      // Drop the centerline/width-profile scaffolding once the outline is
      // built — mirrors draw-bridge.js's own commit (its comment: "once
      // built, drop the isVectorBrush/centerSegments/widthProfile linkage
      // entirely"). Missing here (this Paper-Tool fallback path never had
      // it) left a COMMITTED Fill Brush shape still tagged as if it were a
      // live centerline+width ribbon — fillWallPath (paint bucket's wall
      // tracer) treats that tag as "use the CENTERLINE, not the true
      // outline", so filling next to one of these shapes traced a
      // boundary offset half a stroke-width INSIDE the real ink edge,
      // leaving a visible gap and never touching the ring closely enough
      // for fillMergeSameColor to fuse them into one shape. isFillShape
      // stays (still needed by pen-bridge.js's close-path/endpoint-snap
      // exclusion guard).
      if(state.shadowMode){
        // A shadow guide stroke must never auto-merge/unite with whatever
        // real artwork happens to sit underneath it — both
        // applyFillBrushPlacement's 'merge' mode and fillMergeSameColor
        // below unite with an overlapping fill REGARDLESS of color, which
        // would silently fuse a guide-only shape into real content the
        // moment it's tagged/recolored. Tag+recolor happens BEFORE either,
        // and both are skipped entirely for this stroke.
        applyShadowBrushTag(currentPath);
      }else{
        delete currentPath.data.isVectorBrush;
        delete currentPath.data.centerSegments;
        delete currentPath.data.widthProfile;
        // Placement (Above/Below/Merge) — see applyFillBrushPlacement's own
        // comment; replaces the old unconditional "always at the back".
        currentPath=applyFillBrushPlacement(currentPath,userLayers[state.activeLayerIdx]);
        // 2026-07 feedback ("plusieurs coup de pinceau avec la même couleur
        // doivent merger automatiquement") — Placement's own 'merge' option
        // unions with whatever fill it happens to overlap regardless of
        // color (see applyFillBrushPlacement); genuine same-color fusion
        // already exists (fillMergeSameColor, used by the paint bucket) but
        // was never called from the Fill Brush's own commit. Wired in here
        // unconditionally so consecutive same-color strokes merge into one
        // shape no matter which Placement mode is active.
        if(currentPath)currentPath=fillMergeSameColor(userLayers[state.activeLayerIdx],currentPath,true)||currentPath;
      }
      if(currentPath)tagOwner(currentPath);
    }
    _vb.pts=[];_vb.widths=[];currentPath=null;stabQueue=[];saveActiveLayerFrame();updateUI();
  }else if(state.tool==='pen'){
    _pen.draggingHandle=false;
  }else if(state.tool==='text'&&_textDragStart){
    if(_textDragRect){_textDragRect.remove();_textDragRect=null;}
    var textDragWidth=Math.abs(event.point.x-_textDragStart.x);
    if(textDragWidth>20/view.zoom){
      var textTopLeft=new Point(Math.min(_textDragStart.x,event.point.x),Math.min(_textDragStart.y,event.point.y));
      if(window.openTextPopoverForBox)openTextPopoverForBox(textTopLeft,textDragWidth);
    }else if(window.openTextPopover)openTextPopover(_textDragStart);
    _textDragStart=null;
  }else if((state.tool==='line'||state.tool==='rect'||state.tool==='ellipse')&&currentPath){
    if(shapeStart&&event.point.getDistance(shapeStart)<2){currentPath.remove();if(state.undoStack.length)state.undoStack.pop();}
    else{
      if(state.shadowMode)applyShadowBrushTag(currentPath);
      tagOwner(currentPath);
      if(window.SMSymmetry&&window.SMSymmetry.onStrokeCommitted)window.SMSymmetry.onStrokeCommitted(currentPath,userLayers[state.activeLayerIdx]);
    }
    currentPath=null;shapeStart=null;saveActiveLayerFrame();updateUI();
  }else if(state.tool==='select'){
    if(_xform.active){
      _xform.active=false;
      var xLd2=state.layers[state.activeLayerIdx];
      if(xLd2&&xLd2.symbolId){
        loadFrame(state.currentFrame);
        selectedPaths=userLayers[state.activeLayerIdx].children.filter(function(c){return (c instanceof Path||c instanceof Raster)&&isSelectablePathChild(c);});
        state.selectedStrokeIndices=[];
      }else{
        fillRegenerateLinked(userLayers[state.activeLayerIdx],null);
        if(window.SMBitmapBrush)selectedPaths.forEach(function(p){if(p.data&&p.data.bitmapBrushSpec)SMBitmapBrush.regenerate(p,userLayers[state.activeLayerIdx]);});
        saveActiveLayerFrame();
      }
      renderTransformHandles();renderNodeHandles();updateUI();
    }
    else if(_marquee.active&&_marquee.mode==='fsselect'){
      if(_marquee.rect){
        var mbf=_marquee.rect.bounds;
        var lassoF=null;
        if(_marquee.lasso&&_marquee.rect.segments.length>2){_marquee.rect.closePath();lassoF=_marquee.rect;}
        var layerF=userLayers[state.activeLayerIdx];
        fsResolveRegionSelection(lassoF||_marquee.rect,layerF);
        _marquee.rect.remove();_marquee.rect=null;_marquee.mode=null;
      }
      _marquee.active=false;renderArcs();updateUI();
      if(window.SMEngineBridge&&window.SMEngineBridge.renderNow)window.SMEngineBridge.renderNow();
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
    else if(draggingArc){draggingArc=null;arcDragCache=null;generateTweens();}else if(selectedPaths.length>0){
      _moveDragStarted=false;
      var mLd2=state.layers[state.activeLayerIdx];
      if(mLd2&&mLd2.symbolId){
        loadFrame(state.currentFrame);
        selectedPaths=userLayers[state.activeLayerIdx].children.filter(function(c){return (c instanceof Path||c instanceof Raster)&&isSelectablePathChild(c);});
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
        // See subselect-bridge.js's identical fix comment: pick a target
        // path from what the box overlaps when none was pre-selected.
        if(!npath){
          var layerForPick=userLayers[state.activeLayerIdx];
          var candidates=layerForPick?layerForPick.children.filter(function(c){return c instanceof Path&&c.segments&&c.segments.length&&isSelectablePathChild(c)&&c.bounds.intersects(nmb);}):[];
          if(candidates.length){
            var picked=candidates[candidates.length-1];
            clearSel();
            selectedPaths.push(picked);
            state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
            npath=nodeEditTargetPath();
          }
        }
        _nodeSel=[];
        if(npath){
          var nsegs=nodeEditSegmentsData(npath);
          nsegs.forEach(function(s,i){if(nmb.contains(new Point(s.point[0],s.point[1])))_nodeSel.push(i);});
        }
        _nmq.rect.remove();_nmq.rect=null;
        if(!_nodeSel.length)clearSel();
      }else{clearSel();}
      _nmq.active=false;renderNodeHandles();renderArcs();updateUI();
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
  // Re-edit a placed text block in place (2026-07 rework) — checked before
  // the select-only guard below since double-clicking with the Text tool
  // itself active must also work, not just Select.
  if(state.tool==='select'||state.tool==='text'){
    var textLayer=userLayers[state.activeLayerIdx];
    var textHit=textLayer.hitTest(event.point,{fill:true,stroke:true,tolerance:4/view.zoom});
    if(textHit&&textHit.item instanceof Raster&&textHit.item.data&&textHit.item.data.isText&&!textHit.item.data.isTextChar){
      if(window.openTextPopoverForEdit)openTextPopoverForEdit(textHit.item);
      return;
    }
    // Vector text (2026-07) — any glyph Path of a placed block carries
    // isVectorText+groupId, but only the group's FIRST Path carries the
    // full metadata (isTextRoot) openTextPopoverForEdit needs; a click can
    // land on any glyph, so find its root sibling before re-opening.
    if(textHit&&textHit.item instanceof Path&&textHit.item.data&&textHit.item.data.isVectorText){
      var vtGid=textHit.item.data.groupId;
      var vtRoot=textLayer.children.filter(function(c){return c.data&&c.data.groupId===vtGid&&c.data.isTextRoot;})[0];
      if(vtRoot&&window.openTextPopoverForEdit)openTextPopoverForEdit(vtRoot);
      return;
    }
  }
  if(state.tool!=='select')return;
  var layer=userLayers[state.activeLayerIdx];
  var hit=layer.hitTest(event.point,{fill:true,tolerance:4/view.zoom});
  if(!hit||!(hit.item instanceof Path)||!hit.item.fillColor)return;
  var fillPath=hit.item;
  clearSel();
  fsClearSel();
  _fsIsolation={groupId:fillPath.data&&fillPath.data.groupId,path:fillPath};
  // strokeBounds (not plain bounds) includes stroke-width padding — a
  // perfectly axis-aligned line has zero-height/width *geometric* bounds,
  // so a straight stroke lying exactly on the fill's edge would otherwise
  // count as merely touching (not intersecting) and get missed. Padding by
  // a couple of pixels on top of that covers any remaining tolerance gap.
  // Double-click enters SUBSELECT on the clicked shape (2026-07-27: "au
  // double clic avec select j'aimerais plutôt l'outil subselect avec même
  // fonctionnement mais capable de modifier les points de vecteurs"). It
  // used to enter fsselect, which isolates the same way but can only pick
  // whole fill/stroke fragments — never the anchors. Isolation semantics are
  // unchanged (_fsIsolation still scopes what is reachable, and clicking
  // outside it drops back to Select); only the tool differs, so the shape's
  // vertices and tangents are editable straight away.
  selectedPaths=[fillPath];
  state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
  window.SM.setTool('subselect');
  // nodeHandles is the array the NEXT click hit-tests against, and only this
  // call populates it — without it the first click on an anchor silently
  // misses (same staleness trap subselect-bridge.js documents at length).
  renderNodeHandles();
  renderArcs();updateUI();
  if(window.SMEngineBridge)SMEngineBridge.renderNow();
}
view.onMouseDown=onMouseDown;view.onMouseDrag=onMouseDrag;view.onMouseUp=onMouseUp;view.onMouseMove=onMouseMoveTool;view.onDoubleClick=onViewDoubleClick;
canvasEl.addEventListener('dblclick',function(e){
  if(!(window.SMEngineBridge&&SMEngineBridge.isEnabled())||state.tool!=='select')return;
  var w=SMEngineBridge.screenToWorld(e.clientX,e.clientY);
  onViewDoubleClick({point:new Point(w[0],w[1])});
  e.preventDefault();e.stopImmediatePropagation();
},{capture:true});
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
