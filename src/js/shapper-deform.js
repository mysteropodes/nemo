// Shapper Intelligence — bind weights + SSD-style deformation (M2)
//
// Builds a "bone graph" out of a skeleton (window.SMSkeleton.extractSkeleton,
// skeleton-extract.js): every point of every branch's fitted curve becomes a
// draggable control point ("bone"), shared/deduped at junctions across
// branches. Each vertex of the original shape binds to its nearest bone
// SEGMENT (not nearest point) with a linear-projection weight split between
// that segment's two endpoints — same model as the real Shapper AE extension
// (calculateShapeInfluence/calculateProjectionFactor in its main.tsx).
//
// Deformation is real 2D skeletal skinning (SSD), not pure translation: each
// vertex stores its bind-time LOCAL OFFSET relative to each of its two bound
// bones, plus each bone's bind-time "chord direction" (average angle to its
// neighbors in the bone graph). On every drag, each bone's CURRENT chord
// direction is recomputed, the vertex's local offset is rotated by that
// bone's angle delta and translated to the bone's current position — the two
// bones' proposals are blended by the stored weight. Ported from Shapper's
// computeDeformTS/generateShapeExpressionB.
//
// Session-only for M2 (like src/js/labs/rig-deform.js) — _rig holds a live
// Paper.js Path reference; M3 persists the bone graph + binds as plain JSON
// (data.skeleton/data.riggedBinds) and adds a relink step, avoiding the
// live-reference trap documented in CLAUDE.md §1.
(function(){
  'use strict';
  var _rig=null; // {paths, skeleton, boneGraph, bindsByPath, curPos, bindAngle}
  var _dragId=null;

  function dist2(ax,ay,bx,by){var dx=ax-bx,dy=ay-by;return dx*dx+dy*dy;}

  // Union every fitted-curve point across all branches into one shared point
  // list (deduped by rounded position so branches meeting at a junction node
  // share the exact same bone id), plus the edge list connecting consecutive
  // points within each branch.
  function buildBoneGraph(skeleton){
    var pts=[];var idOf={};
    function keyFor(x,y){return Math.round(x*100)+','+Math.round(y*100);}
    function getId(x,y){
      var k=keyFor(x,y);
      if(idOf[k]!==undefined)return idOf[k];
      var id=pts.length;pts.push({id:id,x:x,y:y});idOf[k]=id;return id;
    }
    var edges=[];var neighbors={};
    // Ordered chains — one per branch, mirroring Shapper's masks: its bone
    // angle formula reads the PREVIOUS/NEXT vertex of the ordered mask
    // chain (getBoneAngle_B, main.tsx), so each bone needs a canonical
    // (chain, index) home. A junction bone shared by several branches
    // keeps the first chain that contains it.
    var chains=[];var chainRef={};
    skeleton.branches.forEach(function(b){
      var ids=b.segments.map(function(s){return getId(s.point[0],s.point[1]);});
      var chainIdx=chains.length;
      chains.push(ids);
      ids.forEach(function(id,ii){
        if(chainRef[id]===undefined)chainRef[id]={chain:chainIdx,idx:ii};
      });
      for(var i=0;i<ids.length-1;i++){
        var a=ids[i],c=ids[i+1];
        if(a===c)continue;
        edges.push([a,c]);
        (neighbors[a]=neighbors[a]||[]).push(c);
        (neighbors[c]=neighbors[c]||[]).push(a);
      }
    });
    return {points:pts,edges:edges,neighbors:neighbors,chains:chains,chainRef:chainRef};
  }

  // Shapper's bone angle, "Formule B" (getBoneAngle_B / the AE expression
  // generateShapeExpressionB in the real Shapper source): the direction of
  // the CHORD through the bone's ordered-chain neighbors —
  // atan2(next - prev), with prev/next clamped at the chain's ends. Read
  // against whatever positions function is passed (bind positions at bind
  // time, curPos during a drag). Ordered-chain chords are what makes the
  // rotation stable: unlike an unordered graph-neighbor average (tried,
  // flipped geometry inside out), the chord direction moves continuously
  // with the points and only rotates as much as the chain actually bends.
  function boneChordAngle(boneGraph,boneId,posOf){
    var ref=boneGraph.chainRef[boneId];
    if(!ref)return 0;
    var chain=boneGraph.chains[ref.chain];
    if(chain.length<2)return 0;
    var prev=posOf(chain[Math.max(ref.idx-1,0)]);
    var next=posOf(chain[Math.min(ref.idx+1,chain.length-1)]);
    return Math.atan2(next.y-prev.y,next.x-prev.x);
  }

  function normalizeAngle(a){return ((a+Math.PI)%(2*Math.PI)+2*Math.PI)%(2*Math.PI)-Math.PI;}
  function rotateVec(v,a){var c=Math.cos(a),s=Math.sin(a);return{x:v.x*c-v.y*s,y:v.x*s+v.y*c};}

  function pointToSeg(px,py,ax,ay,bx,by){
    var dx=bx-ax,dy=by-ay;
    var len2=dx*dx+dy*dy;
    var t=len2>1e-9?((px-ax)*dx+(py-ay)*dy)/len2:0;
    t=Math.max(0,Math.min(1,t));
    var cx=ax+t*dx,cy=ay+t*dy;
    var ddx=px-cx,ddy=py-cy;
    return {dist2:ddx*ddx+ddy*ddy,t:t};
  }

  function bindPathToBones(path,boneGraph){
    var pts=boneGraph.points;
    var segs=path.segments;
    var binds=new Array(segs.length);
    for(var i=0;i<segs.length;i++){
      var v=segs[i].point;
      var best=null;
      for(var e=0;e<boneGraph.edges.length;e++){
        var edge=boneGraph.edges[e];
        var A=pts[edge[0]],B=pts[edge[1]];
        var r=pointToSeg(v.x,v.y,A.x,A.y,B.x,B.y);
        if(!best||r.dist2<best.dist2)best={dist2:r.dist2,t:r.t,a:edge[0],b:edge[1]};
      }
      if(!best){binds[i]=null;continue;}
      var A2=pts[best.a],B2=pts[best.b];
      binds[i]={
        a:best.a,b:best.b,wA:1-best.t,wB:best.t,
        offA:{x:v.x-A2.x,y:v.y-A2.y},
        offB:{x:v.x-B2.x,y:v.y-B2.y},
        handleIn:{x:segs[i].handleIn.x,y:segs[i].handleIn.y},
        handleOut:{x:segs[i].handleOut.x,y:segs[i].handleOut.y},
      };
    }
    return binds;
  }

  // Deforms EVERY bound path (a multi-shape rig binds each selected shape's
  // vertices against the same shared bone graph — see generateForSelection).
  //
  // Split model, settled over three feedback rounds:
  //   POSITIONS — pure translation blend: vertex = w-blended (bone position
  //   + CONSTANT bind offset). Strictly "un drive de vertices": dragging a
  //   bone moves only the vertices bound to it, exactly by the blend of its
  //   displacement — nothing can flip, and nothing far from the drag moves.
  //   (Rotating offsets — even with Shapper's own ordered-chain chord — let
  //   one dragged bone swing the chord of its NEIGHBORS too, orbiting
  //   distant vertices around bones that never moved: "la déformation
  //   déforme aussi le trait pas juste les vertices".)
  //   TANGENTS — rotated by the local chord delta (Shapper's Formule B,
  //   atan2(next-prev) over the ordered chain, blended per influence): the
  //   bezier handles turn with the chain's local bend, so ink CURVES around
  //   a fold instead of staying frozen at bind orientation — the part of
  //   Shapper's rotation system that acts on the stroke's direction without
  //   displacing any vertex.
  function deformAll(){
    if(!_rig)return;
    var curPos=_rig.curPos;
    var posOf=function(id){return curPos[id];};
    var curAngle={};
    _rig.boneGraph.points.forEach(function(p){curAngle[p.id]=boneChordAngle(_rig.boneGraph,p.id,posOf);});
    _rig.paths.forEach(function(path,pi){
      var binds=_rig.bindsByPath[pi];
      // Path shape changed elsewhere (another tool edited it) since bind —
      // skip THIS path rather than bail out of the whole rig.
      if(!path||path.segments.length!==binds.length)return;
      var segs=path.segments;
      for(var vi=0;vi<segs.length;vi++){
        var bd=binds[vi];
        if(!bd)continue;
        var A=curPos[bd.a],B=curPos[bd.b];
        var nx=bd.wA*(A.x+bd.offA.x)+bd.wB*(B.x+bd.offB.x);
        var ny=bd.wA*(A.y+bd.offA.y)+bd.wB*(B.y+bd.offB.y);
        var raA=normalizeAngle(curAngle[bd.a]-_rig.bindAngle[bd.a]);
        var raB=normalizeAngle(curAngle[bd.b]-_rig.bindAngle[bd.b]);
        var hiA=rotateVec(bd.handleIn,raA),hiB=rotateVec(bd.handleIn,raB);
        var hoA=rotateVec(bd.handleOut,raA),hoB=rotateVec(bd.handleOut,raB);
        segs[vi].point=new Point(nx,ny);
        segs[vi].handleIn=new Point(bd.wA*hiA.x+bd.wB*hiB.x,bd.wA*hiA.y+bd.wB*hiB.y);
        segs[vi].handleOut=new Point(bd.wA*hoA.x+bd.wB*hoB.x,bd.wA*hoA.y+bd.wB*hoB.y);
      }
    });
  }

  // Companion expansion (2026-07 feedback: "le fill non attaché à la
  // stroke" — clicking a drawing picks ONE path, typically the outline
  // stroke on top, but the artist means the whole drawing: the paint-
  // bucket fill underneath must deform with it, and its solid interior is
  // also what makes the skeleton run through the middle instead of along
  // the contour band). Pull in same-layer companions of the selection:
  //   - exact links both ways: a fill's data.fillWalls lists the
  //     data.strokeId of the wall strokes it was traced against
  //     (fillVectorFindRaster, tools.js) — selecting either side rigs
  //     both;
  //   - geometric overlap fallback (fills made without wall links): a
  //     path whose bbox intersection covers >=80% of the smaller of the
  //     two bboxes is the same drawing (a fill under its outline), while
  //     unrelated art standing nearby overlaps far less.
  function expandForRig(paths){
    if(typeof userLayers==='undefined'||typeof state==='undefined')return paths;
    var layer=userLayers[state.activeLayerIdx];
    if(!layer)return paths;
    var out=paths.slice();
    var wallIds={};var strokeIds={};
    paths.forEach(function(p){
      if(!p.data)return;
      if(p.data.strokeId)strokeIds[p.data.strokeId]=true;
      if(p.data.fillWalls)p.data.fillWalls.forEach(function(w){wallIds[w]=true;});
    });
    layer.children.forEach(function(c){
      if(out.indexOf(c)>=0)return;
      if(!(c instanceof Path)||c.segments.length<3)return;
      var linked=false;
      if(c.data){
        if(c.data.fillWalls&&c.data.fillWalls.some(function(w){return strokeIds[w];}))linked=true;
        if(c.data.strokeId&&wallIds[c.data.strokeId])linked=true;
      }
      if(!linked){
        linked=paths.some(function(p){
          var ib=c.bounds.intersect(p.bounds);
          if(ib.width<=0||ib.height<=0)return false;
          var ia=ib.width*ib.height;
          var minA=Math.min(c.bounds.width*c.bounds.height,p.bounds.width*p.bounds.height);
          return minA>0&&ia>=minA*0.8;
        });
      }
      if(linked)out.push(c);
    });
    return out;
  }

  // Accepts 1+ selected shapes (2026-07 fix: "je n'arrive pas à select un
  // ensemble... ni un seul élément" — the tool previously hard-required
  // exactly one path, and offered no way to pick a shape while already
  // inside the tool, see the onDown click/marquee handling below). Multiple
  // shapes bind against ONE shared skeleton extracted from their combined
  // silhouette (skeleton-extract.js's extractSkeleton now accepts an array),
  // so a multi-part character can be posed as a single rig — and the
  // selection is auto-expanded with linked/overlapping companions (fill
  // under outline), see expandForRig above.
  function generateForSelection(){
    if(typeof selectedPaths==='undefined'||!selectedPaths.length){
      if(window.showToast)showToast('Sélectionnez au moins une forme pour générer son squelette');
      return;
    }
    var paths=selectedPaths.filter(function(p){return p instanceof Path&&p.segments.length>=3;});
    if(!paths.length){
      if(window.showToast)showToast('Sélectionnez au moins une forme (3 points ou plus) pour générer son squelette');
      return;
    }
    paths=expandForRig(paths);
    var t0=performance.now();
    // Tool Options' "Poignées" field (index.html) — lower tolerance keeps
    // the fitted curve closer to the raw pixel chain (more control points),
    // higher decimates more aggressively (fewer, sparser points). Read at
    // generation time, like every other Tool Options field in this app.
    var tolerance=(typeof state!=='undefined'&&state.shapperTolerance)?state.shapperTolerance:2;
    // Tool Options' "Fusion" field — manual closing radius in DOCUMENT px
    // (0 = the automatic first-area-jump heuristic). skeleton-extract
    // converts to raster px via its own scale.
    var fusion=(typeof state!=='undefined'&&state.shapperFusion)?state.shapperFusion:0;
    var skeleton;
    try{skeleton=window.SMSkeleton.extractSkeleton(paths,{maxDim:512,tolerance:tolerance,closeRadius:fusion});}
    catch(e){console.error('Shapper extractSkeleton failed',e);if(window.showToast)showToast('Échec de l\'extraction du squelette');return;}
    if(!skeleton||!skeleton.nodes.length){if(window.showToast)showToast('Squelette introuvable pour cette sélection');return;}
    var boneGraph=buildBoneGraph(skeleton);
    if(boneGraph.points.length<2){if(window.showToast)showToast('Squelette trop simple pour être riggé');return;}
    var bindsByPath=paths.map(function(p){return bindPathToBones(p,boneGraph);});
    var curPos={};
    boneGraph.points.forEach(function(p){curPos[p.id]={x:p.x,y:p.y};});
    // Bind-time chord angle per bone (Shapper's o.rotationAngle) — the
    // deform rotates each bind offset/tangent by (current - bind).
    var posOf=function(id){return curPos[id];};
    var bindAngle={};
    boneGraph.points.forEach(function(p){bindAngle[p.id]=boneChordAngle(boneGraph,p.id,posOf);});
    _rig={paths:paths,skeleton:skeleton,boneGraph:boneGraph,bindsByPath:bindsByPath,curPos:curPos,bindAngle:bindAngle};
    _dragId=null;
    if(window.SMEngineBridge)window.SMEngineBridge.renderNow();
    if(window.showToast)showToast('Squelette généré — '+paths.length+' forme(s), '+boneGraph.points.length+' points, '+((performance.now()-t0)|0)+'ms');
  }

  function nearestBoneAt(pt,radius){
    if(!_rig)return null;
    var best=null,bestD=radius*radius;
    for(var i=0;i<_rig.boneGraph.points.length;i++){
      var p=_rig.boneGraph.points[i];
      var cp=_rig.curPos[p.id];
      var d=dist2(pt.x,pt.y,cp.x,cp.y);
      if(d<bestD){bestD=d;best=p.id;}
    }
    return best;
  }

  // Multi-point selection (2026-07 feedback: "on peut select à la manière
  // du lasso plusieurs points et les bouger ensemble") — freehand lasso
  // over empty canvas selects every bone inside the drawn polygon;
  // grabbing any SELECTED bone then drags the whole set rigidly (each
  // bone keeps its offset from the grab point), the SSD deform following.
  var _selIds={};      // boneId -> true
  var _lasso=null;     // {pts:[{x,y},...]} while a lasso drag is running
  var _groupStart=null;// boneId -> {x,y} at drag start (group drag only)
  var _dragStartPt=null;

  function selectionCount(){var n=0;for(var k in _selIds)n++;return n;}

  function pointInPolygon(px,py,pts){
    var inside=false;
    for(var i=0,j=pts.length-1;i<pts.length;j=i++){
      var xi=pts[i].x,yi=pts[i].y,xj=pts[j].x,yj=pts[j].y;
      if(((yi>py)!==(yj>py))&&(px<(xj-xi)*(py-yi)/(yj-yi)+xi))inside=!inside;
    }
    return inside;
  }

  function onDown(event){
    if(!_rig)return false;
    var radius=10/view.zoom;
    var id=nearestBoneAt(event.point,radius);
    if(id!==null){
      if(typeof pushUndo==='function')pushUndo();
      _dragId=id;
      _dragStartPt={x:event.point.x,y:event.point.y};
      if(_selIds[id]&&selectionCount()>1){
        _groupStart={};
        for(var k in _selIds)_groupStart[k]={x:_rig.curPos[k].x,y:_rig.curPos[k].y};
      }else{
        // Grabbing an unselected bone drops any prior lasso selection —
        // same "click replaces selection" convention as the Select tool.
        _selIds={};_selIds[id]=true;_groupStart=null;
      }
      return true;
    }
    return false;
  }
  // Called by tools.js's shapper branch AFTER its own shape hit-test came
  // up empty — clicking directly ON a shape must still re-pick it (and
  // rebuild the rig), so the lasso only owns genuinely empty canvas.
  function startLasso(pt){
    if(!_rig)return false;
    _lasso={pts:[{x:pt.x,y:pt.y}]};
    return true;
  }
  function onDrag(event){
    if(_lasso){
      _lasso.pts.push({x:event.point.x,y:event.point.y});
      if(window.SMEngineBridge)window.SMEngineBridge.renderNow();
      return true;
    }
    if(_dragId===null||!_rig)return false;
    if(_groupStart){
      var dx=event.point.x-_dragStartPt.x,dy=event.point.y-_dragStartPt.y;
      for(var k in _groupStart)_rig.curPos[k]={x:_groupStart[k].x+dx,y:_groupStart[k].y+dy};
    }else{
      _rig.curPos[_dragId]={x:event.point.x,y:event.point.y};
    }
    deformAll();
    if(window.SMEngineBridge)window.SMEngineBridge.renderNow();
    return true;
  }
  function onUp(){
    if(_lasso){
      if(_lasso.pts.length>=3&&_rig){
        _selIds={};
        _rig.boneGraph.points.forEach(function(p){
          var cp=_rig.curPos[p.id];
          if(pointInPolygon(cp.x,cp.y,_lasso.pts))_selIds[p.id]=true;
        });
        var n=selectionCount();
        if(window.showToast&&n)showToast(n+' point(s) sélectionné(s)');
      }
      _lasso=null;
      if(window.SMEngineBridge)window.SMEngineBridge.renderNow();
      return true;
    }
    if(_dragId===null)return false;
    _dragId=null;_groupStart=null;_dragStartPt=null;
    if(window.SMEngineBridge)window.SMEngineBridge.renderNow();
    return true;
  }

  function commitPose(){
    if(!_rig){if(window.showToast)showToast('Aucun rig actif — générez un squelette d\'abord');return;}
    if(typeof ensureKeyframe==='function')ensureKeyframe();
    if(typeof saveActiveLayerFrame==='function')saveActiveLayerFrame();
    if(window.showToast)showToast('Pose commitée sur cette frame');
  }

  function clearRig(){_rig=null;_dragId=null;_selIds={};_lasso=null;_groupStart=null;_dragStartPt=null;}
  function isActive(){return !!_rig;}

  function octagon(cx,cy,r,fillColor,strokeColor,strokeWidth){
    var segs=[];
    for(var i=0;i<8;i++){
      var a=i/8*Math.PI*2;
      segs.push({point:[cx+Math.cos(a)*r,cy+Math.sin(a)*r]});
    }
    return{segments:segs,closed:true,fillColor:fillColor,strokeColor:strokeColor,strokeWidth:strokeWidth};
  }

  // Consumed by engine-bridge.js's buildSceneJson (same pattern as
  // SMCamera.buildOverlayItems) so the bone handles render through the Rust
  // engine's JSON overlay pipeline — a live Paper.js Group never renders
  // there (view.autoUpdate is false whenever the engine is active, see
  // CLAUDE.md §5), which is why an earlier debug-only attempt during M1
  // never showed up on screen.
  function buildOverlayItems(){
    if(!_rig||typeof view==='undefined')return[];
    // Rig data survives tool switches (2026-07: "le skeleton est enregistré
    // ainsi que ses positions si on revient sur l'outil") — but its handles
    // only SHOW while the Shapper tool is active, like every other tool's
    // own gizmos.
    if(typeof state!=='undefined'&&state.tool!=='shapper')return[];
    var zs=1/Math.max(0.0001,view.zoom);
    var items=[];
    _rig.boneGraph.edges.forEach(function(e){
      var A=_rig.curPos[e[0]],B=_rig.curPos[e[1]];
      items.push({segments:[{point:[A.x,A.y]},{point:[B.x,B.y]}],closed:false,fillColor:null,strokeColor:[0,220,255,220],strokeWidth:1.6*zs});
    });
    _rig.boneGraph.points.forEach(function(p){
      var cp=_rig.curPos[p.id];
      var r=4.5*zs;
      var isDragging=(_dragId===p.id);
      var col=isDragging?[255,210,0,255]:(_selIds[p.id]?[255,150,40,255]:[80,220,120,240]);
      items.push(octagon(cp.x,cp.y,r,col,[20,20,20,220],1.2*zs));
    });
    if(_lasso&&_lasso.pts.length>1){
      items.push({segments:_lasso.pts.map(function(q){return{point:[q.x,q.y]};}),closed:false,fillColor:[255,150,40,18],strokeColor:[255,150,40,230],strokeWidth:1.2*zs,dashPattern:[5*zs,4*zs]});
    }
    return items;
  }

  window.SMShapper={
    generateForSelection:generateForSelection,
    onDown:onDown,onDrag:onDrag,onUp:onUp,
    startLasso:startLasso,
    commitPose:commitPose,
    clearRig:clearRig,
    isActive:isActive,
    buildOverlayItems:buildOverlayItems,
  };
})();
