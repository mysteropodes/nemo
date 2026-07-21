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
  var _rig=null; // {path, skeleton, boneGraph, binds, curPos, bindDir}
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
    skeleton.branches.forEach(function(b){
      var ids=b.segments.map(function(s){return getId(s.point[0],s.point[1]);});
      for(var i=0;i<ids.length-1;i++){
        var a=ids[i],c=ids[i+1];
        if(a===c)continue;
        edges.push([a,c]);
        (neighbors[a]=neighbors[a]||[]).push(c);
        (neighbors[c]=neighbors[c]||[]).push(a);
      }
    });
    return {points:pts,edges:edges,neighbors:neighbors};
  }

  // A bone's orientation = the average direction toward its graph neighbors,
  // evaluated against whatever position function is passed in (bind-time
  // positions at bind, current positions during deform) — this is what makes
  // rotation propagate through a junction with 3+ neighbors, not just a
  // simple 2-neighbor chain.
  function boneDirection(boneGraph,boneId,posOf){
    var nbrs=boneGraph.neighbors[boneId]||[];
    if(!nbrs.length)return 0;
    var p=posOf(boneId);
    var sx=0,sy=0;
    for(var i=0;i<nbrs.length;i++){
      var q=posOf(nbrs[i]);
      var d=Math.atan2(q.y-p.y,q.x-p.x);
      sx+=Math.cos(d);sy+=Math.sin(d);
    }
    return Math.atan2(sy,sx);
  }

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

  function rotateVec(v,a){var c=Math.cos(a),s=Math.sin(a);return{x:v.x*c-v.y*s,y:v.x*s+v.y*c};}

  // Deforms EVERY bound path (a multi-shape rig binds each selected shape's
  // vertices against the same shared bone graph — see generateForSelection).
  function deformAll(){
    if(!_rig)return;
    var boneGraph=_rig.boneGraph;
    var curPos=_rig.curPos;
    var posOf=function(id){return curPos[id];};
    var curDir={};
    for(var i=0;i<boneGraph.points.length;i++)curDir[i]=boneDirection(boneGraph,i,posOf);
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
        var dA=curDir[bd.a]-_rig.bindDir[bd.a];
        var dB=curDir[bd.b]-_rig.bindDir[bd.b];
        var posA=rotateVec(bd.offA,dA);posA.x+=A.x;posA.y+=A.y;
        var posB=rotateVec(bd.offB,dB);posB.x+=B.x;posB.y+=B.y;
        var nx=bd.wA*posA.x+bd.wB*posB.x,ny=bd.wA*posA.y+bd.wB*posB.y;
        var hAngle=Math.atan2(bd.wA*Math.sin(dA)+bd.wB*Math.sin(dB),bd.wA*Math.cos(dA)+bd.wB*Math.cos(dB));
        var hin=rotateVec(bd.handleIn,hAngle),hout=rotateVec(bd.handleOut,hAngle);
        segs[vi].point=new Point(nx,ny);
        segs[vi].handleIn=new Point(hin.x,hin.y);
        segs[vi].handleOut=new Point(hout.x,hout.y);
      }
    });
  }

  // Accepts 1+ selected shapes (2026-07 fix: "je n'arrive pas à select un
  // ensemble... ni un seul élément" — the tool previously hard-required
  // exactly one path, and offered no way to pick a shape while already
  // inside the tool, see the onDown click/marquee handling below). Multiple
  // shapes bind against ONE shared skeleton extracted from their combined
  // silhouette (skeleton-extract.js's extractSkeleton now accepts an array),
  // so a multi-part character can be posed as a single rig.
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
    var t0=performance.now();
    var skeleton;
    try{skeleton=window.SMSkeleton.extractSkeleton(paths,{maxDim:512,tolerance:2});}
    catch(e){console.error('Shapper extractSkeleton failed',e);if(window.showToast)showToast('Échec de l\'extraction du squelette');return;}
    if(!skeleton||!skeleton.nodes.length){if(window.showToast)showToast('Squelette introuvable pour cette sélection');return;}
    var boneGraph=buildBoneGraph(skeleton);
    if(boneGraph.points.length<2){if(window.showToast)showToast('Squelette trop simple pour être riggé');return;}
    var bindsByPath=paths.map(function(p){return bindPathToBones(p,boneGraph);});
    var curPos={};
    boneGraph.points.forEach(function(p){curPos[p.id]={x:p.x,y:p.y};});
    var posOf=function(id){return curPos[id];};
    var bindDir={};
    boneGraph.points.forEach(function(p){bindDir[p.id]=boneDirection(boneGraph,p.id,posOf);});
    _rig={paths:paths,skeleton:skeleton,boneGraph:boneGraph,bindsByPath:bindsByPath,curPos:curPos,bindDir:bindDir};
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

  function onDown(event){
    if(!_rig)return false;
    var radius=10/view.zoom;
    var id=nearestBoneAt(event.point,radius);
    if(id===null)return false;
    if(typeof pushUndo==='function')pushUndo();
    _dragId=id;
    return true;
  }
  function onDrag(event){
    if(_dragId===null||!_rig)return false;
    _rig.curPos[_dragId]={x:event.point.x,y:event.point.y};
    deformAll();
    if(window.SMEngineBridge)window.SMEngineBridge.renderNow();
    return true;
  }
  function onUp(){
    if(_dragId===null)return false;
    _dragId=null;
    if(window.SMEngineBridge)window.SMEngineBridge.renderNow();
    return true;
  }

  function commitPose(){
    if(!_rig){if(window.showToast)showToast('Aucun rig actif — générez un squelette d\'abord');return;}
    if(typeof ensureKeyframe==='function')ensureKeyframe();
    if(typeof saveActiveLayerFrame==='function')saveActiveLayerFrame();
    if(window.showToast)showToast('Pose commitée sur cette frame');
  }

  function clearRig(){_rig=null;_dragId=null;}
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
      var col=isDragging?[255,210,0,255]:[80,220,120,240];
      items.push(octagon(cp.x,cp.y,r,col,[20,20,20,220],1.2*zs));
    });
    return items;
  }

  window.SMShapper={
    generateForSelection:generateForSelection,
    onDown:onDown,onDrag:onDrag,onUp:onUp,
    commitPose:commitPose,
    clearRig:clearRig,
    isActive:isActive,
    buildOverlayItems:buildOverlayItems,
  };
})();
