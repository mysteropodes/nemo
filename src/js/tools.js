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

function ensureKeyframe(){var curF=state.layers[state.activeLayerIdx].frames[state.currentFrame];if(!curF.isKeyframe&&!curF.isInterpolated){curF.isKeyframe=true;curF.strokes=JSON.parse(JSON.stringify(getEffectiveStrokes(state.activeLayerIdx,state.currentFrame)));loadFrame(state.currentFrame);}}

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
  fills.forEach(function(f){
    var seed=new Point(f.data.fillSeed[0],f.data.fillSeed[1]);
    var col=f.fillColor,op=f.opacity;
    var onlyIds=(f.data.fillWalls&&f.data.fillWalls.length)?f.data.fillWalls:undefined;
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
      united.fillColor=path.fillColor;united.strokeColor=null;united.opacity=path.opacity;
      var idx=layer.children.indexOf(target);
      target.remove();path.remove();
      layer.insertChild(Math.min(idx,layer.children.length),united);
      return united;
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
  if(outline){path.segments=outline.segments;path.closed=true;outline.remove();}
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
  userLayers[state.activeLayerIdx].addChild(result);
  paths.forEach(function(p){p.remove();});
  selectedPaths=[result];state.selectedStrokeIndices=[];
  fillRegenerateLinked(userLayers[state.activeLayerIdx],result);
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
  if(!isVB&&path.strokeColor&&!path.fillColor){
    var expanded=eraseExpandStrokeToFill(path);
    if(!expanded)return;
    layer.insertChild(idx,expanded);
    path.remove();
    target=expanded;
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
    layer.insertChild(Math.min(tIdx,layer.children.length),result);
  }else if(result)result.remove();
}

// ---- PRESSURE-SIMULATED VECTOR BRUSH ----
var _vb={pts:[],widths:[],lastT:0,lastPt:null};
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
      _vbLastPenPressure=null;_vb.pts=[event.point.clone()];_vb.widths=[vbPressureOf(event)];_vb.lastT=Date.now();_vb.lastPt=event.point.clone();
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
    if(state.tool==='line')currentPath=new Path.Line({from:event.point,to:event.point,strokeColor:state.strokeColor,strokeWidth:state.brushSize,strokeCap:state.strokeCap,fillColor:null,opacity:state.opacity/100});
    else if(state.tool==='rect')currentPath=new Path.Rectangle({from:event.point,to:event.point.add(new Point(1,1)),strokeColor:state.strokeColor,strokeWidth:state.brushSize,fillColor:state.fillEnabled?state.fillColor:null,opacity:state.opacity/100});
    else currentPath=new Path.Ellipse({rectangle:new Rectangle(event.point,new Size(1,1)),strokeColor:state.strokeColor,strokeWidth:state.brushSize,fillColor:state.fillEnabled?state.fillColor:null,opacity:state.opacity/100});
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
      selectedPaths.push(subHit.item);state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
    }else if(selectedPaths.length===1){
      // empty-space drag with a path selected: marquee over its anchors
      _nmq.active=true;_nmq.start=event.point.clone();_nmq.rect=null;
      return;
    }else{clearSel();}
    renderArcs();updateUI();
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
        selectedPaths=userLayers[compHit.layerIdx].children.filter(function(c){return c instanceof Path;});
        state.selectedStrokeIndices=[];
        renderArcs();updateUI();
        if(isDbl)window.SM.enterSymbol(state.layers[compHit.layerIdx].symbolId);
        return;
      }
    }
    if(hit&&hit.item instanceof Path){
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
      eraseAtPoint(hit2.item,event.point,state.eraserSize/2);
      _eraseLastPt=event.point.clone();
      fillRegenerateLinked(layer,null);saveActiveLayerFrame();updateUI();
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
      if(isVB&&ep.fillColor){state.strokeColor=ep.fillColor.toCSS(true);document.getElementById('color-stroke').value=state.strokeColor;document.getElementById('pm-stroke-c').value=state.strokeColor;document.getElementById('stroke-well').style.background=state.strokeColor;document.getElementById('pm-stroke').style.background=state.strokeColor;}
      else{
        if(ep.strokeColor){state.strokeColor=ep.strokeColor.toCSS(true);document.getElementById('color-stroke').value=state.strokeColor;document.getElementById('pm-stroke-c').value=state.strokeColor;document.getElementById('stroke-well').style.background=state.strokeColor;document.getElementById('pm-stroke').style.background=state.strokeColor;}
        if(ep.fillColor){state.fillColor=ep.fillColor.toCSS(true);state.fillEnabled=true;document.getElementById('color-fill').value=state.fillColor;document.getElementById('pm-fill-c').value=state.fillColor;document.getElementById('pm-fill').style.background=state.fillColor;document.getElementById('fill-well').style.background=state.fillColor;document.getElementById('fill-well').classList.remove('none');document.getElementById('p-fill-on').checked=true;}
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
      _vb.pts.push(event.point.clone());_vb.widths.push(vbPressureOf(event));
      vbRebuildPreview();
      return;
    }
    var stab=state.stabilizer;
    if(stab===0)currentPath.add(event.point);
    else{stabQueue.push(event.point.clone());var maxQ=stab===1?3:stab===2?6:10;while(stabQueue.length>maxQ)stabQueue.shift();var avg=new Point(0,0);stabQueue.forEach(function(p){avg=avg.add(p);});avg=avg.divide(stabQueue.length);currentPath.add(avg);}
  }else if(state.tool==='fillbrush'){
    if(!currentPath)return;
    _vb.pts.push(event.point.clone());_vb.widths.push(vbPressureOf(event));
    vbRebuildPreview();
  }else if(state.tool==='pen'){
    if(!_pen.path||!_pen.draggingHandle)return;
    var seg=_pen.path.lastSegment;var delta=event.point.subtract(seg.point);
    seg.handleOut=delta;seg.handleIn=delta.multiply(-1);
  }else if((state.tool==='line'||state.tool==='rect'||state.tool==='ellipse')&&currentPath&&shapeStart){
    currentPath.remove();
    if(state.tool==='line'){currentPath=new Path.Line({from:shapeStart,to:event.point,strokeColor:state.strokeColor,strokeWidth:state.brushSize,strokeCap:state.strokeCap,fillColor:null,opacity:state.opacity/100});applyStrokeStyle(currentPath);}
    else if(state.tool==='rect')currentPath=new Path.Rectangle({from:shapeStart,to:event.point,strokeColor:state.strokeColor,strokeWidth:state.brushSize,fillColor:state.fillEnabled?state.fillColor:null,opacity:state.opacity/100});
    else{currentPath=new Path.Ellipse({rectangle:new Rectangle(shapeStart,event.point),strokeColor:state.strokeColor,strokeWidth:state.brushSize,fillColor:state.fillEnabled?state.fillColor:null,opacity:state.opacity/100});}
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
    });renderNodeHandles();renderTransformHandles();}
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
    currentPath=null;stabQueue=[];saveActiveLayerFrame();updateUI();
  }else if(state.tool==='fillbrush'&&currentPath){
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
    }
    _vb.pts=[];_vb.widths=[];currentPath=null;stabQueue=[];saveActiveLayerFrame();updateUI();
  }else if(state.tool==='pen'){
    _pen.draggingHandle=false;
  }else if((state.tool==='line'||state.tool==='rect'||state.tool==='ellipse')&&currentPath){
    if(shapeStart&&event.point.getDistance(shapeStart)<2){currentPath.remove();if(state.undoStack.length)state.undoStack.pop();}
    currentPath=null;shapeStart=null;saveActiveLayerFrame();updateUI();
  }else if(state.tool==='select'){
    if(_xform.active){
      _xform.active=false;fillRegenerateLinked(userLayers[state.activeLayerIdx],null);saveActiveLayerFrame();renderTransformHandles();renderNodeHandles();updateUI();
    }
    else if(_marquee.active){
      if(_marquee.rect){
        var mb=_marquee.rect.bounds;
        var layer2=userLayers[state.activeLayerIdx];
        layer2.children.forEach(function(c){
          if(c instanceof Path&&c.segments.length>0&&(c.strokeColor||c.fillColor)&&mb.intersects(c.bounds)){
            if(selectedPaths.indexOf(c)<0)selectedPaths.push(c);
          }
        });
        state.selectedStrokeIndices=selectedPaths.map(getSI).filter(function(i2){return i2>=0;});
        _marquee.rect.remove();_marquee.rect=null;
      }
      _marquee.active=false;renderArcs();updateUI();
    }
    else if(draggingArc){draggingArc=null;generateTweens();}else if(selectedPaths.length>0){_moveDragStarted=false;fillRegenerateLinked(userLayers[state.activeLayerIdx],null);saveActiveLayerFrame();}
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
canvasEl.addEventListener('wheel',function(e){e.preventDefault();var f=e.deltaY>0?.92:1.08;var nz=Math.max(.05,Math.min(20,view.zoom*f));var mp=view.viewToProject(new Point(e.offsetX,e.offsetY));view.zoom=nz;var nm=view.viewToProject(new Point(e.offsetX,e.offsetY));view.center=view.center.add(mp.subtract(nm));updZoom();renderArcs();},{passive:false});
