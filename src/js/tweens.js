// ---- MATCHING ----
function buildTP(sd){var p=new Path({insert:false});sd.segments.forEach(function(s){p.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});return p;}
// A stroke's color and fill/stroke "type" are what a viewer actually reads
// as its identity — a black outline and a red fill can end up at a similar
// position/shape by pure coincidence (hatching, overlapping outlines), and
// without this the geometry-only cost below would happily match them,
// which reads as strokes "swapping" colors/types mid-tween.
function parseHexColor(css){
  if(!css)return null;
  var h=String(css).replace('#','');
  if(h.length===3)h=h.split('').map(function(c){return c+c;}).join('');
  if(h.length!==6&&h.length!==8)return null;
  var r=parseInt(h.substr(0,2),16),g=parseInt(h.substr(2,2),16),b=parseInt(h.substr(4,2),16);
  if(isNaN(r)||isNaN(g)||isNaN(b))return null;
  return{r:r,g:g,b:b};
}
function colorDist(c1,c2){
  if(!c1&&!c2)return 0;
  if(!c1||!c2)return 1;
  var dr=c1.r-c2.r,dg=c1.g-c2.g,db=c1.b-c2.b;
  return Math.sqrt(dr*dr+dg*dg+db*db)/441.6729559300637;
}
function strokeType(sd){if(sd.isVectorBrush)return'vb';var hasS=!!sd.strokeColor,hasF=!!sd.fillColor;if(hasS&&hasF)return'both';if(hasF)return'fill';return'stroke';}
// Pressure-brush strokes are stored as their filled OUTLINE (a closed
// sausage around the drawn line) — comparing outlines wrecks proximity,
// curvature and open/closed detection. All geometric features are computed
// on the actual drawn centerline instead whenever it's available.
function buildTPFeat(sd){
  var segs=(sd.isVectorBrush&&sd.centerSegments&&sd.centerSegments.length>1)?sd.centerSegments:sd.segments;
  var p=new Path({insert:false});
  segs.forEach(function(s){p.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
  return p;
}
function strokeFeat(sd){var p=buildTPFeat(sd);var b=p.bounds,len=p.length;var cx=0,cy=0,nS=12;for(var i=0;i<nS;i++){var pt=p.getPointAt(i/(nS-1)*len);if(pt){cx+=pt.x;cy+=pt.y;}}cx/=nS;cy/=nS;var f2=p.firstSegment.point,l=p.lastSegment.point;var dx=l.x-f2.x,dy=l.y-f2.y,dl=Math.sqrt(dx*dx+dy*dy);if(dl>0){dx/=dl;dy/=dl;}var shape=[];for(var i2=0;i2<8;i2++){var pt2=p.getPointAt(i2/7*len);if(pt2)shape.push([(pt2.x-cx)/Math.max(b.width,1),(pt2.y-cy)/Math.max(b.height,1)]);}
  // dense absolute samples + turning-angle profile: the raw material the
  // geometric cost below is computed from (real line-to-line proximity and
  // real curve comparison, instead of coarse centroid/bbox summaries)
  var K=16,pts=[];
  for(var ki=0;ki<K;ki++){var kp=p.getPointAt(ki/(K-1)*len);pts.push(kp?[kp.x,kp.y]:[cx,cy]);}
  var turn=[];
  for(var ti=1;ti<K-1;ti++){
    var v1x=pts[ti][0]-pts[ti-1][0],v1y=pts[ti][1]-pts[ti-1][1];
    var v2x=pts[ti+1][0]-pts[ti][0],v2y=pts[ti+1][1]-pts[ti][1];
    turn.push(Math.atan2(v1x*v2y-v1y*v2x,v1x*v2x+v1y*v2y||1e-9));
  }
  // closed-ness: a loop (head outline, iris, closed fill boundary) is a
  // fundamentally different animal from an open stroke of similar bulk
  var diag=Math.sqrt(b.width*b.width+b.height*b.height);
  var isClosed=p.segments.length>3&&dl<Math.max(4,diag*0.08)&&len>diag*1.2;
  p.remove();return{cx:cx,cy:cy,length:len,dirX:dx,dirY:dy,bounds:{x:b.x,y:b.y,w:b.width,h:b.height},shape:shape,pts:pts,turn:turn,closed:isClosed,strokeCol:parseHexColor(sd.strokeColor),fillCol:parseHexColor(sd.fillColor),type:strokeType(sd)};}
// Relative position (within the whole frame's own composition bbox) is what
// actually distinguishes "left eye" from "right eye" — raw absolute centroid
// distance breaks down whenever the whole drawing translates/scales between
// keyframes (e.g. a head moving), since an eye can end up numerically closer
// to its mirror counterpart in the other frame than to itself.
function unionBounds(feats){
  var x1=Infinity,y1=Infinity,x2=-Infinity,y2=-Infinity;
  feats.forEach(function(f){x1=Math.min(x1,f.bounds.x);y1=Math.min(y1,f.bounds.y);x2=Math.max(x2,f.bounds.x+f.bounds.w);y2=Math.max(y2,f.bounds.y+f.bounds.h);});
  if(x1>x2)return{x:0,y:0,w:1,h:1};
  return{x:x1,y:y1,w:Math.max(1,x2-x1),h:Math.max(1,y2-y1)};
}
var _matchNorm=0;
function matchSc(fA,fB,sameIndex,aPtsOverride){
  var pts=aPtsOverride||fA.pts;var K=Math.min(pts.length,fB.pts.length);
  // 1. PROXIMITY (dominant): symmetric Chamfer distance — for each sample
  // of one line, distance to the nearest sample of the other. Start-point
  // and direction invariant; directly answers "do these two lines lie in
  // the same place", normalized by the strokes' own size so 'near' means
  // near relative to how big the strokes are, not to the whole drawing.
  var sumAB=0,sumBA=0;
  for(var i=0;i<K;i++){
    var best=1e18;
    for(var j=0;j<K;j++){var dx=pts[i][0]-fB.pts[j][0],dy=pts[i][1]-fB.pts[j][1];var d=dx*dx+dy*dy;if(d<best)best=d;}
    sumAB+=Math.sqrt(best);
  }
  for(var j2=0;j2<K;j2++){
    var best2=1e18;
    for(var i2=0;i2<K;i2++){var dx2=pts[i2][0]-fB.pts[j2][0],dy2=pts[i2][1]-fB.pts[j2][1];var d2=dx2*dx2+dy2*dy2;if(d2<best2)best2=d2;}
    sumBA+=Math.sqrt(best2);
  }
  var cham=(sumAB+sumBA)/(2*K);
  // 2. LAYOUT: ordered point-to-point distance (better of both directions)
  // — two lines can overlap as point sets yet trace different paths; this
  // keeps e.g. a zigzag from matching a straight line through its middle.
  var fwd=0,rev=0;
  for(var k=0;k<K;k++){
    var dxf=pts[k][0]-fB.pts[k][0],dyf=pts[k][1]-fB.pts[k][1];fwd+=Math.sqrt(dxf*dxf+dyf*dyf);
    var dxr=pts[k][0]-fB.pts[K-1-k][0],dyr=pts[k][1]-fB.pts[K-1-k][1];rev+=Math.sqrt(dxr*dxr+dyr*dyr);
  }
  var alg=Math.min(fwd,rev)/K;
  var da=Math.sqrt(fA.bounds.w*fA.bounds.w+fA.bounds.h*fA.bounds.h);
  var db=Math.sqrt(fB.bounds.w*fB.bounds.w+fB.bounds.h*fB.bounds.h);
  var scaleAB=(da+db)/2+(typeof _matchNorm==='number'?_matchNorm*0.04:0)+1;
  var proxT=cham/(cham+scaleAB*0.5);
  var alignT=alg/(alg+scaleAB);
  // 3. CURVES: turning-angle signature — compares how the two lines bend
  // along their length (scale & position invariant, direction-normalized)
  var TA=fA.turn,TB=fB.turn;var nT=Math.min(TA.length,TB.length);
  var cf=0,cr=0;
  for(var q=0;q<nT;q++){cf+=Math.abs(TA[q]-TB[q]);cr+=Math.abs(TA[q]+TB[nT-1-q]);}
  var curveT=nT?Math.min(1,Math.min(cf,cr)/nT/Math.PI):0;
  // 4. secondary cues & hard penalties
  var rdx=fA.relX-fB.relX,rdy=fA.relY-fB.relY;var rel=Math.min(1,Math.sqrt(rdx*rdx+rdy*rdy));
  var lenRatio=Math.max(fA.length,fB.length)/Math.max(1,Math.min(fA.length,fB.length));
  var ratioPen=lenRatio>2?Math.min(0.7,(lenRatio-2)*0.35):0;
  var closedPen=fA.closed!==fB.closed?0.35:0;
  var aArea=fA.bounds.w*fA.bounds.h,bArea=fB.bounds.w*fB.bounds.h;
  var szD=Math.abs(aArea-bArea)/Math.max(1,Math.max(aArea,bArea));
  var colD=(colorDist(fA.strokeCol,fB.strokeCol)+colorDist(fA.fillCol,fB.fillCol))/2;
  var typePenalty=fA.type!==fB.type?0.5:0;
  var idxBonus=sameIndex?-0.03:0;
  return proxT*.50+alignT*.15+curveT*.18+rel*.10+szD*.06+colD*.15+typePenalty+ratioPen+closedPen+idxBonus;
}
// "Force line" motion model: eyes, chin, and other small close-together
// features are exactly where independent per-stroke shape/position matching
// breaks down — two candidates can look equally plausible in isolation. The
// fix borrowed from traditional (Japanese-style) inbetweening is to first
// read the *overall* motion of the drawing (the line of force the whole
// pose is moving along) from whichever strokes matched unambiguously, then
// use that predicted motion to resolve the ambiguous ones — a feature
// should land where the drawing's motion says it lands, not just wherever
// looks locally similar. This fits a similarity transform (rotation +
// uniform scale + translation, no shear/reflection) via least squares in
// complex-number form, which has a direct closed-form solution in 2D.
function fitSimilarityTransform(ptsA,ptsB){
  var n=ptsA.length;if(n<2)return null;
  var ca={x:0,y:0},cb={x:0,y:0};
  for(var i=0;i<n;i++){ca.x+=ptsA[i].x;ca.y+=ptsA[i].y;cb.x+=ptsB[i].x;cb.y+=ptsB[i].y;}
  ca.x/=n;ca.y/=n;cb.x/=n;cb.y/=n;
  var numRe=0,numIm=0,den=0;
  for(var i2=0;i2<n;i2++){
    var ax=ptsA[i2].x-ca.x,ay=ptsA[i2].y-ca.y;
    var bx=ptsB[i2].x-cb.x,by=ptsB[i2].y-cb.y;
    numRe+=ax*bx+ay*by;numIm+=ax*by-ay*bx;den+=ax*ax+ay*ay;
  }
  if(den<1e-6)return null;
  return{wRe:numRe/den,wIm:numIm/den,ca:ca,cb:cb};
}
function applySimilarityTransform(t,x,y){
  var dx=x-t.ca.x,dy=y-t.ca.y;
  var rx=t.wRe*dx-t.wIm*dy,ry=t.wIm*dx+t.wRe*dy;
  return{x:rx+t.cb.x,y:ry+t.cb.y};
}
// Hungarian algorithm (Kuhn-Munkres, O(n^3)) — true minimum-cost perfect
// assignment on a square cost matrix. Replaces the previous greedy
// "cheapest pair first" assignment, which could lock in a locally-cheap
// swap (e.g. eye A->eye B) before a better global pairing was considered.
function hungarian(cost){
  var n=cost.length;var INF=1e9;
  var u=new Array(n+1).fill(0),v=new Array(n+1).fill(0);
  var p=new Array(n+1).fill(0),way=new Array(n+1).fill(0);
  for(var i=1;i<=n;i++){
    p[0]=i;var j0=0;
    var minv=new Array(n+1).fill(INF);
    var used=new Array(n+1).fill(false);
    do{
      used[j0]=true;
      var i0=p[j0],delta=INF,j1=-1;
      for(var j=1;j<=n;j++){
        if(!used[j]){
          var cur=cost[i0-1][j-1]-u[i0]-v[j];
          if(cur<minv[j]){minv[j]=cur;way[j]=j0;}
          if(minv[j]<delta){delta=minv[j];j1=j;}
        }
      }
      for(var j2=0;j2<=n;j2++){
        if(used[j2]){u[p[j2]]+=delta;v[j2]-=delta;}
        else minv[j2]-=delta;
      }
      j0=j1;
    }while(p[j0]!==0);
    do{var j1b=way[j0];p[j0]=p[j1b];j0=j1b;}while(j0);
  }
  var assign=new Array(n).fill(-1);
  for(var j=1;j<=n;j++){if(p[j]>0)assign[p[j]-1]=j-1;}
  return assign;
}
// geometry-wasm's tweenmatch.rs ported this exact algorithm (same tuned
// constants, same two-pass Hungarian + similarity-transform seeding) — see
// autoMatchJS below for the reference implementation, kept as the fallback
// on any WASM failure/absence, same pattern as fill/erase/boolean/shapes.
function _strokeInJson(sd){
  return{segments:sd.segments||[],centerSegments:sd.centerSegments,strokeColor:sd.strokeColor||null,fillColor:sd.fillColor||null,isVectorBrush:!!sd.isVectorBrush};
}
function autoMatch(sA,sB){
  if(!sA.length||!sB.length)return[];
  if(window.GeometryWasm&&window.GeometryWasm.ready){
    try{
      var json=window.GeometryWasm.auto_match(JSON.stringify(sA.map(_strokeInJson)),JSON.stringify(sB.map(_strokeInJson)));
      return JSON.parse(json);
    }catch(e){console.warn('[geometry-wasm] auto_match failed, falling back to JS',e);}
  }
  return autoMatchJS(sA,sB);
}
function autoMatchJS(sA,sB){
  if(!sA.length||!sB.length)return[];
  var fA=sA.map(strokeFeat),fB=sB.map(strokeFeat);
  var bA=unionBounds(fA),bB=unionBounds(fB);
  fA.forEach(function(f){f.relX=(f.cx-bA.x)/bA.w;f.relY=(f.cy-bA.y)/bA.h;});
  fB.forEach(function(f){f.relX=(f.cx-bB.x)/bB.w;f.relY=(f.cy-bB.y)/bB.h;});
  _matchNorm=Math.sqrt(Math.pow(Math.max(bA.x+bA.w,bB.x+bB.w)-Math.min(bA.x,bB.x),2)+Math.pow(Math.max(bA.y+bA.h,bB.y+bB.h)-Math.min(bA.y,bB.y),2));
  var md=1;fA.forEach(function(a){fB.forEach(function(b){var d=Math.sqrt((a.cx-b.cx)*(a.cx-b.cx)+(a.cy-b.cy)*(a.cy-b.cy));if(d>md)md=d;});});
  // Augmented assignment: the matrix is padded so EVERY stroke can opt out
  // into a dummy (= cross-fade) at a fixed cost, instead of only fading via
  // a post-threshold. The Hungarian optimum then decides globally whether a
  // chain of mediocre forced matches is worth more than letting one stroke
  // fade — which resolves conflict chains where a stroke's true partner
  // was stolen by a neighbor and it got stuck with garbage.
  var FADE_COST=0.6;
  var n=sA.length,m=sB.length,N=n+m;
  function buildCost(ptsT){
    var c=[];
    for(var a=0;a<N;a++){
      var row=[];
      for(var b=0;b<N;b++){
        if(a<n&&b<m)row.push(matchSc(fA[a],fB[b],a===b,ptsT?ptsT[a]:undefined));
        else if(a>=n&&b>=m)row.push(0);
        else row.push(FADE_COST);
      }
      c.push(row);
    }
    return c;
  }
  var cost=buildCost(null);
  var assign=hungarian(cost);
  var matches=[];
  for(var a2=0;a2<n;a2++){var b2=assign[a2];if(b2!==undefined&&b2>=0&&b2<m)matches.push({a:a2,b:b2,score:cost[a2][b2]});}
  if(matches.length<2)return matches;
  // Pass 2: seed the motion model from the best-scoring (least ambiguous)
  // half of pass 1's matches, then re-resolve every pair using how well it
  // agrees with that predicted motion — this is what untangles close/
  // similar features (eyes, chin, parallel hatching on an arm) that pass 1
  // alone can flip.
  var seeds=matches.slice().sort(function(x,y){return x.score-y.score;});
  var seedCount=Math.max(2,Math.ceil(seeds.length*0.5));
  seeds=seeds.slice(0,seedCount);
  var ptsA=seeds.map(function(s){return{x:fA[s.a].cx,y:fA[s.a].cy};});
  var ptsB=seeds.map(function(s){return{x:fB[s.b].cx,y:fB[s.b].cy};});
  var transform=fitSimilarityTransform(ptsA,ptsB);
  if(!transform)return matches;
  // the fitted motion (force line) is applied to every A stroke's actual
  // sample points, so pass 2's proximity measures "how far is this line
  // from where the drawing's motion says it should be" — not raw distance
  var ptsT=fA.map(function(f){return f.pts.map(function(pt){var q=applySimilarityTransform(transform,pt[0],pt[1]);return[q.x,q.y];});});
  var cost2=buildCost(ptsT);
  var assign2=hungarian(cost2);
  var matches2=[];
  for(var a4=0;a4<n;a4++){var b4=assign2[a4];if(b4!==undefined&&b4>=0&&b4<m)matches2.push({a:a4,b:b4,score:cost2[a4][b4]});}
  return matches2;
}

// ---- INTERPOLATION ----
// resample_stroke unifies both the plain-outline and vector-brush-
// centerline branches internally (keyed off the same isVectorBrush &&
// centerSegments.length>1 condition JS itself dispatches on), so both
// resampleP and resampleCenterline below share this one wasm call — see
// each function's own JS fallback (resamplePJS/resampleCenterlineJS) for
// the reference implementation. ResampledOut doesn't carry strokeWidth/
// strokeCap/strokeJoin/opacity (not needed for the resample math itself),
// so those are re-attached from the source stroke record after the call.
function _resampleStrokeWasm(sd,n){
  if(!(window.GeometryWasm&&window.GeometryWasm.ready))return null;
  try{
    var out=JSON.parse(window.GeometryWasm.resample_stroke(JSON.stringify(_strokeInJson(sd)),n));
    out.strokeWidth=sd.strokeWidth;out.strokeCap=sd.strokeCap;out.strokeJoin=sd.strokeJoin;
    out.opacity=sd.opacity!==undefined?sd.opacity:1;
    if(!out.isVectorBrush)out.strokeColor=sd.strokeColor;
    return out;
  }catch(e){console.warn('[geometry-wasm] resample_stroke failed, falling back to JS',e);return null;}
}
function resampleP(sd,n){var w=_resampleStrokeWasm(sd,n);if(w)return w;return resamplePJS(sd,n);}
function resamplePJS(sd,n){var p=new Path({insert:false});sd.segments.forEach(function(s){p.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});var len=p.length;if(len<1||n<2){p.remove();return sd;}var segs=[];for(var i=0;i<n;i++){var t=i/(n-1);var off=t*len;var pt=p.getPointAt(off);if(!pt)pt=p.getPointAt(len);var tan=p.getTangentAt(off);if(!tan)tan=new Point(1,0);var hl=len/(n-1)/3;segs.push({point:[pt.x,pt.y],handleIn:i===0?[0,0]:[-tan.x*hl,-tan.y*hl],handleOut:i===n-1?[0,0]:[tan.x*hl,tan.y*hl]});}p.remove();return{segments:segs,strokeColor:sd.strokeColor,strokeWidth:sd.strokeWidth,strokeCap:sd.strokeCap,strokeJoin:sd.strokeJoin,fillColor:sd.fillColor||null,opacity:sd.opacity!==undefined?sd.opacity:1};}
// Centerline-driven variable-width strokes (vector brush / taper) are
// matched and interpolated via their lean editable centerline + per-anchor
// widths (data.centerSegments) instead of the dense filled outline that's
// actually stored in sd.segments — far more stable to resample/lerp, and
// keeps the result node-editable. The visible outline is regenerated from
// the interpolated centerline afterward (see interpStroke).
function resampleCenterline(sd,n){
  var cs=sd.centerSegments;
  if(!cs||cs.length<2)return resampleP(sd,n);
  var w=_resampleStrokeWasm(sd,n);if(w)return w;
  return resampleCenterlineJS(sd,n);
}
function resampleCenterlineJS(sd,n){
  var cs=sd.centerSegments;
  var p=new Path({insert:false});cs.forEach(function(s){p.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
  var len=p.length;
  var segLens=[0];for(var i=1;i<cs.length;i++)segLens.push(segLens[i-1]+new Point(cs[i].point[0],cs[i].point[1]).getDistance(new Point(cs[i-1].point[0],cs[i-1].point[1])));
  var total=segLens[segLens.length-1]||1;
  if(len<1||n<2){p.remove();return{segments:cs.map(function(s){return{point:s.point,handleIn:s.handleIn,handleOut:s.handleOut};}),widths:cs.map(function(s){return s.width;}),isVectorBrush:true,strokeColor:null,fillColor:sd.fillColor||null,opacity:sd.opacity!==undefined?sd.opacity:1};}
  var segs=[],widths=[];
  for(var i2=0;i2<n;i2++){
    var t=i2/(n-1);var off=t*len;
    var pt=p.getPointAt(off);if(!pt)pt=p.getPointAt(len);
    var tan=p.getTangentAt(off);if(!tan)tan=new Point(1,0);
    var hl=len/(n-1)/3;
    segs.push({point:[pt.x,pt.y],handleIn:i2===0?[0,0]:[-tan.x*hl,-tan.y*hl],handleOut:i2===n-1?[0,0]:[tan.x*hl,tan.y*hl]});
    var targetLen=t*total;
    var wi=0;while(wi<segLens.length-2&&segLens[wi+1]<targetLen)wi++;
    var span=Math.max(0.0001,segLens[wi+1]-segLens[wi]);
    var lt=Math.min(1,Math.max(0,(targetLen-segLens[wi])/span));
    widths.push(cs[wi].width+(cs[wi+1].width-cs[wi].width)*lt);
  }
  p.remove();
  return{segments:segs,widths:widths,isVectorBrush:true,strokeColor:null,fillColor:sd.fillColor||null,opacity:sd.opacity!==undefined?sd.opacity:1};
}
// Rebuilds a filled outline (plain serializable segments) from a centerline
// control-segments array + per-anchor widths. Mirrors tools.js's
// rebuildVectorBrushOutline but works on plain data (no live Path/data
// needed), since tween generation produces fresh frame records, not items.
function outlineFromCenterSegs(segs){
  var center=new Path({insert:false});
  segs.forEach(function(s){center.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
  var len=center.length;var steps=Math.max(8,Math.round(len/6));
  var segLens=[0];for(var i=1;i<segs.length;i++)segLens.push(segLens[i-1]+new Point(segs[i].point[0],segs[i].point[1]).getDistance(new Point(segs[i-1].point[0],segs[i-1].point[1])));
  var total=segLens[segLens.length-1]||1;
  var pts=[],widths=[];
  for(var k=0;k<=steps;k++){
    var d=len*k/steps;
    var pt=center.getPointAt(d);if(!pt)pt=center.getPointAt(len)||new Point(segs[segs.length-1].point[0],segs[segs.length-1].point[1]);
    pts.push(pt);
    var targetLen=(d/Math.max(len,0.0001))*total;
    var wi=0;while(wi<segLens.length-2&&segLens[wi+1]<targetLen)wi++;
    var span=Math.max(0.0001,segLens[wi+1]-segLens[wi]);
    var lt=Math.min(1,Math.max(0,(targetLen-segLens[wi])/span));
    widths.push(segs[wi].width+(segs[wi+1].width-segs[wi].width)*lt);
  }
  center.remove();
  var outline=buildVariableWidthPath(pts,widths);
  var outSegs=outline?outline.segments.map(function(s){return{point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};}):[];
  if(outline)outline.remove();
  return outSegs;
}
function getEasing(){if(window._curveEditor)return window._curveEditor.evalCurve;return function(t){return t;};}
function lerp(a,b,t){return a+(b-a)*t;}function lerpV(a,b,t){return[lerp(a[0],b[0],t),lerp(a[1],b[1],t)];}
function arcKey(fA,fB,i){return fA+'-'+fB+'-'+i;}
function getArcCtrl(fA,fB,i,ptA,ptB){var k=arcKey(fA,fB,i);var a=state.motionArcs[k];var mx=(ptA[0]+ptB[0])/2,my=(ptA[1]+ptB[1])/2;if(a)return{x:mx+a.cx,y:my+a.cy};return{x:mx,y:my};}
function setArcCtrl(fA,fB,i,ptA,ptB,cx,cy){var mx=(ptA[0]+ptB[0])/2,my=(ptA[1]+ptB[1])/2;state.motionArcs[arcKey(fA,fB,i)]={cx:cx-mx,cy:cy-my};}
function qBez(a,c,b,t){var u=1-t;return u*u*a+2*u*t*c+t*t*b;}
function interpStroke(rA,rB,t,easFn,fA,fB,mIdx){
  var et=easFn(t);var n=Math.min(rA.segments.length,rB.segments.length);var segs=[];
  var cxA=0,cyA=0,cxB=0,cyB=0;for(var i=0;i<n;i++){cxA+=rA.segments[i].point[0];cyA+=rA.segments[i].point[1];cxB+=rB.segments[i].point[0];cyB+=rB.segments[i].point[1];}cxA/=n;cyA/=n;cxB/=n;cyB/=n;
  var ac=getArcCtrl(fA,fB,mIdx,[cxA,cyA],[cxB,cyB]);var cx2=qBez(cxA,ac.x,cxB,et);var cy2=qBez(cyA,ac.y,cyB,et);
  for(var i2=0;i2<n;i2++){var sA=rA.segments[i2],sB=rB.segments[i2];segs.push({point:[cx2+lerp(sA.point[0]-cxA,sB.point[0]-cxB,et),cy2+lerp(sA.point[1]-cyA,sB.point[1]-cyB,et)],handleIn:lerpV(sA.handleIn,sB.handleIn,et),handleOut:lerpV(sA.handleOut,sB.handleOut,et)});}
  if(rA.isVectorBrush&&rB.isVectorBrush){
    var widths=[];for(var w=0;w<n;w++)widths.push(lerp(rA.widths[w]||1,rB.widths[w]||1,et));
    var centerSegs=segs.map(function(s,idx){return{point:s.point,handleIn:s.handleIn,handleOut:s.handleOut,width:widths[idx]};});
    return{segments:outlineFromCenterSegs(centerSegs),strokeColor:null,strokeWidth:lerp(rA.strokeWidth||3,rB.strokeWidth||3,et),strokeCap:rA.strokeCap||'round',strokeJoin:rA.strokeJoin||'round',fillColor:et<.5?(rA.fillColor||null):(rB.fillColor||null),opacity:lerp(rA.opacity!==undefined?rA.opacity:1,rB.opacity!==undefined?rB.opacity:1,et),isVectorBrush:true,centerSegments:centerSegs};
  }
  return{segments:segs,strokeColor:et<.5?rA.strokeColor:rB.strokeColor,strokeWidth:lerp(rA.strokeWidth||3,rB.strokeWidth||3,et),strokeCap:rA.strokeCap||'round',strokeJoin:rA.strokeJoin||'round',fillColor:et<.5?(rA.fillColor||null):(rB.fillColor||null),opacity:lerp(rA.opacity!==undefined?rA.opacity:1,rB.opacity!==undefined?rB.opacity:1,et)};
}
// ---- RESAMPLED-PAIR ALIGNMENT ----
// Even with a correct match, interpolating point i of A toward point i of B
// twists the inbetween whenever the two strokes were drawn from opposite
// ends (the shape flips through itself) or, for closed shapes, start at a
// different point along the loop (the fill "swirls"). Before interpolation
// the B side is re-oriented — reversed and/or cyclically rotated — to the
// candidate whose points sit closest to A's, translation-independently.
function reverseResampled(r){
  var out={};for(var k in r)if(r.hasOwnProperty(k)&&k!=='segments'&&k!=='widths')out[k]=r[k];
  out.segments=r.segments.slice().reverse().map(function(s){return{point:s.point,handleIn:s.handleOut,handleOut:s.handleIn};});
  if(r.widths)out.widths=r.widths.slice().reverse();
  return out;
}
function rotateResampled(r,k){
  var out={};for(var kk in r)if(r.hasOwnProperty(kk)&&kk!=='segments'&&kk!=='widths')out[kk]=r[kk];
  out.segments=r.segments.slice(k).concat(r.segments.slice(0,k));
  if(r.widths)out.widths=r.widths.slice(k).concat(r.widths.slice(0,k));
  return out;
}
function resampledIsClosed(r){
  var s=r.segments;if(s.length<4)return false;
  var minx=Infinity,miny=Infinity,maxx=-Infinity,maxy=-Infinity;
  s.forEach(function(sg){minx=Math.min(minx,sg.point[0]);miny=Math.min(miny,sg.point[1]);maxx=Math.max(maxx,sg.point[0]);maxy=Math.max(maxy,sg.point[1]);});
  var diag2=(maxx-minx)*(maxx-minx)+(maxy-miny)*(maxy-miny);
  var dx=s[0].point[0]-s[s.length-1].point[0],dy=s[0].point[1]-s[s.length-1].point[1];
  return dx*dx+dy*dy<Math.max(1,diag2)*0.0025;
}
function alignCost(a,b){
  var n=Math.min(a.segments.length,b.segments.length);
  var cax=0,cay=0,cbx=0,cby=0;
  for(var i=0;i<n;i++){cax+=a.segments[i].point[0];cay+=a.segments[i].point[1];cbx+=b.segments[i].point[0];cby+=b.segments[i].point[1];}
  cax/=n;cay/=n;cbx/=n;cby/=n;
  var s=0;
  for(var i2=0;i2<n;i2++){
    var dx=(a.segments[i2].point[0]-cax)-(b.segments[i2].point[0]-cbx);
    var dy=(a.segments[i2].point[1]-cay)-(b.segments[i2].point[1]-cby);
    s+=dx*dx+dy*dy;
  }
  return s;
}
function alignResampledPair(a,b){
  if(window.GeometryWasm&&window.GeometryWasm.ready){
    try{
      var out=JSON.parse(window.GeometryWasm.align_pair(JSON.stringify(a),JSON.stringify(b)));
      // align_pair's ResampledOut doesn't carry strokeWidth/strokeCap/
      // strokeJoin/opacity (unused by the alignment cost) — re-attach from
      // the original b, whose fields the returned segments/widths are just
      // a reordering/reversal of.
      out.strokeWidth=b.strokeWidth;out.strokeCap=b.strokeCap;out.strokeJoin=b.strokeJoin;
      out.opacity=b.opacity;
      if(!out.isVectorBrush)out.strokeColor=b.strokeColor;
      return out;
    }catch(e){console.warn('[geometry-wasm] align_pair failed, falling back to JS',e);}
  }
  return alignResampledPairJS(a,b);
}
function alignResampledPairJS(a,b){
  var best=b,bestC=alignCost(a,b);
  var rev=reverseResampled(b);
  var rc=alignCost(a,rev);if(rc<bestC){bestC=rc;best=rev;}
  if(resampledIsClosed(a)&&resampledIsClosed(b)){
    [b,rev].forEach(function(base){
      var n=base.segments.length;
      for(var k=1;k<n;k++){
        var cand=rotateResampled(base,k);
        var c=alignCost(a,cand);
        if(c<bestC){bestC=c;best=cand;}
      }
    });
  }
  return best;
}
// ---- N:1 SPLIT MATCHING ----
// An artist often draws in ONE stroke what the other keyframe holds as
// several (shoulder+arm as two strokes in key A, a single line in key B).
// A strict 1:1 assignment can only mismatch or fade those. This pass finds
// groups of 2-3 strokes on one side that together retrace a single longer
// stroke on the other side, splits that long stroke at matching arc-length
// fractions, and pairs each piece with its stroke. Only strokes that
// actually need it are deconstructed, and only inside the generated
// inbetweens — the keyframes themselves are never modified.
function extractStrokePiece(sd,f0,f1){
  var isVB=!!(sd.isVectorBrush&&sd.centerSegments&&sd.centerSegments.length>1);
  var srcSegs=isVB?sd.centerSegments:sd.segments;
  var p=new Path({insert:false});
  srcSegs.forEach(function(sg){p.add(new Segment(new Point(sg.point[0],sg.point[1]),new Point(sg.handleIn[0],sg.handleIn[1]),new Point(sg.handleOut[0],sg.handleOut[1])));});
  var len=p.length,n=24;
  var segLens=[0];
  if(isVB)for(var wi2=1;wi2<srcSegs.length;wi2++)segLens.push(segLens[wi2-1]+Math.sqrt(Math.pow(srcSegs[wi2].point[0]-srcSegs[wi2-1].point[0],2)+Math.pow(srcSegs[wi2].point[1]-srcSegs[wi2-1].point[1],2)));
  var totalW=segLens[segLens.length-1]||1;
  var segs=[];
  for(var i=0;i<n;i++){
    var t=f0+(f1-f0)*i/(n-1);var off=Math.max(0,Math.min(len,t*len));
    var pt=p.getPointAt(off)||p.getPointAt(len)||p.lastSegment.point;
    var tan=p.getTangentAt(off)||new Point(1,0);
    var hl=(f1-f0)*len/(n-1)/3;
    var seg={point:[pt.x,pt.y],handleIn:i===0?[0,0]:[-tan.x*hl,-tan.y*hl],handleOut:i===n-1?[0,0]:[tan.x*hl,tan.y*hl]};
    if(isVB){
      var tl=t*totalW,wj=0;
      while(wj<segLens.length-2&&segLens[wj+1]<tl)wj++;
      var span=Math.max(0.0001,segLens[wj+1]-segLens[wj]);
      var lt=Math.min(1,Math.max(0,(tl-segLens[wj])/span));
      seg.width=(srcSegs[wj].width||3)+((srcSegs[wj+1].width||3)-(srcSegs[wj].width||3))*lt;
    }
    segs.push(seg);
  }
  p.remove();
  if(isVB)return{segments:[],isVectorBrush:true,centerSegments:segs,strokeColor:null,fillColor:sd.fillColor||null,opacity:sd.opacity!==undefined?sd.opacity:1};
  return{segments:segs,strokeColor:sd.strokeColor,strokeWidth:sd.strokeWidth,strokeCap:sd.strokeCap,strokeJoin:sd.strokeJoin,fillColor:null,opacity:sd.opacity!==undefined?sd.opacity:1};
}
function boundsOverlapLoose(b1,b2){
  var m=Math.max(20,Math.min(Math.max(b1.w,b1.h),Math.max(b2.w,b2.h))*0.3);
  return b1.x-m<b2.x+b2.w&&b2.x-m<b1.x+b1.w&&b1.y-m<b2.y+b2.h&&b2.y-m<b1.y+b1.h;
}
function resolveSplitMatches(sA,sB,pairSpecs,unA,unB){
  var featA=sA.map(strokeFeat),featB=sB.map(strokeFeat);
  var bA=unionBounds(featA),bB=unionBounds(featB);
  featA.forEach(function(f){f.relX=(f.cx-bA.x)/bA.w;f.relY=(f.cy-bA.y)/bA.h;});
  featB.forEach(function(f){f.relX=(f.cx-bB.x)/bB.w;f.relY=(f.cy-bB.y)/bB.h;});
  _matchNorm=Math.sqrt(Math.pow(Math.max(bA.x+bA.w,bB.x+bB.w)-Math.min(bA.x,bB.x),2)+Math.pow(Math.max(bA.y+bA.h,bB.y+bB.h)-Math.min(bA.y,bB.y),2));
  function tryDirection(mergedSide){
    var featMerged=mergedSide==='B'?featB:featA;
    var featParts=mergedSide==='B'?featA:featB;
    var sMerged=mergedSide==='B'?sB:sA;
    var sParts=mergedSide==='B'?sA:sB;
    var unParts=mergedSide==='B'?unA:unB;
    var unMerged=mergedSide==='B'?unB:unA;
    var mergedBounds=mergedSide==='B'?bB:bA;
    for(var mi2=0;mi2<sMerged.length;mi2++){
      var fm=featMerged[mi2];
      if(fm.type==='fill'||fm.closed)continue;
      var curPair=null;
      for(var pi=0;pi<pairSpecs.length;pi++){var ps=pairSpecs[pi];if((mergedSide==='B'?ps.bIdx:ps.aIdx)===mi2&&!ps.isPiece){curPair=ps;break;}}
      var cand=unParts.filter(function(i){var f=featParts[i];return f.type!=='fill'&&boundsOverlapLoose(f.bounds,fm.bounds);});
      if(curPair)cand=cand.concat([mergedSide==='B'?curPair.aIdx:curPair.bIdx]);
      if(cand.length<2||cand.length>3)continue;
      var sumLen=0;cand.forEach(function(i){sumLen+=featParts[i].length;});
      if(sumLen<fm.length*0.55||sumLen>fm.length*1.7)continue;
      // order the part strokes by where they attach along the merged stroke
      var mp=buildTPFeat(sMerged[mi2]);
      var ordered=cand.map(function(i){
        var loc=mp.getNearestLocation(new Point(featParts[i].cx,featParts[i].cy));
        return{idx:i,off:loc?loc.offset:0};
      }).sort(function(x,y){return x.off-y.off;});
      mp.remove();
      // cut fractions = cumulative length share of the ordered parts
      var fr=[],acc=0;
      for(var oi=0;oi<ordered.length-1;oi++){acc+=featParts[ordered[oi].idx].length;fr.push(Math.min(0.95,Math.max(0.05,acc/sumLen)));}
      var pieces=[],scores=[],prevF=0,ok=true;
      for(var oi2=0;oi2<ordered.length;oi2++){
        var f1=oi2<fr.length?fr[oi2]:1;
        var piece=extractStrokePiece(sMerged[mi2],prevF,f1);prevF=f1;
        var pf=strokeFeat(piece);
        pf.relX=(pf.cx-mergedBounds.x)/mergedBounds.w;pf.relY=(pf.cy-mergedBounds.y)/mergedBounds.h;
        var sc=mergedSide==='B'?matchSc(featParts[ordered[oi2].idx],pf,false):matchSc(pf,featParts[ordered[oi2].idx],false);
        pieces.push(piece);scores.push(sc);
        if(sc>0.48)ok=false;
      }
      if(!ok)continue;
      var avg=scores.reduce(function(a,b){return a+b;},0)/scores.length;
      var baseline=cand.map(function(i){return(curPair&&(mergedSide==='B'?curPair.aIdx:curPair.bIdx)===i)?curPair.score:0.95;});
      var baseAvg=baseline.reduce(function(a,b){return a+b;},0)/baseline.length;
      if(avg>=baseAvg-0.03)continue;
      // accept the split
      if(curPair)pairSpecs.splice(pairSpecs.indexOf(curPair),1);
      ordered.forEach(function(o,k){
        var iu=unParts.indexOf(o.idx);if(iu>=0)unParts.splice(iu,1);
        if(mergedSide==='B')pairSpecs.push({aIdx:o.idx,bIdx:mi2,aData:sA[o.idx],bData:pieces[k],mi:9000+mi2*10+k,score:scores[k],isPiece:true});
        else pairSpecs.push({aIdx:mi2,bIdx:o.idx,aData:pieces[k],bData:sB[o.idx],mi:9500+mi2*10+k,score:scores[k],isPiece:true});
      });
      var um=unMerged.indexOf(mi2);if(um>=0)unMerged.splice(um,1);
    }
  }
  tryDirection('B');
  tryDirection('A');
}
function generateTweens(){
  saveAllLayerFrames();var li=state.activeLayerIdx;var ld=state.layers[li];
  var keys=[];for(var i=0;i<state.totalFrames;i++){if(ld.frames[i].isKeyframe&&ld.frames[i].strokes.length>0)keys.push(i);}
  if(keys.length<2){showToast('Il faut au moins 2 keyframes dessinées');return;}
  // A frame selection on this layer restricts regeneration to just those
  // keyframes' own span (start keyframe -> its next keyframe), instead of
  // silently redoing the whole layer — select the frame to fix, hit Tween,
  // and nothing outside that span is touched. Selecting any held/tween
  // frame within a span counts as selecting its origin keyframe, matching
  // the double-click-selects-span behavior.
  var selOnLayer=_sel.frames.filter(function(s){return s.layer===li;}).map(function(s){return s.frame;});
  var restrictTo=null;
  if(selOnLayer.length){
    restrictTo={};
    selOnLayer.forEach(function(fi0){for(var pi=fi0;pi>=0;pi--){if(ld.frames[pi].isKeyframe){restrictTo[pi]=true;break;}}});
  }
  pushUndoLayers();
  var easFn=getEasing();var resN=state.resamplePts;var step=state.tweenStep;var total=0;
  for(var ki=0;ki<keys.length-1;ki++){
    var fA=keys[ki],fB=keys[ki+1];
    if(restrictTo&&!restrictTo[fA])continue;
    var sA=ld.frames[fA].strokes,sB=ld.frames[fB].strokes;
    var matches=autoMatch(sA,sB);if(!matches.length)continue;
    // Only morph plausible pairs. A stroke whose best assignment still
    // scores badly (no real counterpart in the other key — count mismatch,
    // or a shape that genuinely appears/disappears) cross-fades in place
    // instead of scaling/warping toward an unrelated stroke.
    var MATCH_TH=0.48;
    var pairSpecs=[],aMatched={},bMatched={};
    matches.forEach(function(m){if(m.score<=MATCH_TH){pairSpecs.push({aIdx:m.a,bIdx:m.b,aData:sA[m.a],bData:sB[m.b],mi:matches.indexOf(m),score:m.score});aMatched[m.a]=1;bMatched[m.b]=1;}});
    var unA=[],unB=[];
    for(var ai=0;ai<sA.length;ai++)if(!aMatched[ai])unA.push(ai);
    for(var bi2=0;bi2<sB.length;bi2++)if(!bMatched[bi2])unB.push(bi2);
    // N:1 rescue pass — may convert fades + a mediocre pair into clean
    // piece-wise morphs by splitting a merged stroke (see resolveSplitMatches)
    if(unA.length||unB.length)resolveSplitMatches(sA,sB,pairSpecs,unA,unB);
    var fadeOutA=unA.map(function(i){return sA[i];}),fadeInB=unB.map(function(i){return sB[i];});
    if(!pairSpecs.length&&!fadeOutA.length&&!fadeInB.length)continue;
    var pairs=pairSpecs.map(function(spec){
      var isVB=!!(spec.aData.isVectorBrush&&spec.bData.isVectorBrush);
      var rfn=isVB?resampleCenterline:resampleP;
      var ra=rfn(spec.aData,resN),rb=rfn(spec.bData,resN);
      return{a:ra,b:alignResampledPair(ra,rb),mi:spec.mi};
    });
    var gap=fB-fA;
    for(var fi=fA+1;fi<fB;fi++){
      // A frame flagged isManualEdit was hand-corrected after a previous
      // tween — leave it exactly as the artist left it unless they've
      // explicitly unchecked "skip manually-edited frames".
      if(state.tweenSkipManual&&ld.frames[fi].isManualEdit)continue;
      if(step>1&&(fi-fA)%step!==0){ld.frames[fi]={strokes:[],isInterpolated:false,isKeyframe:false};continue;}
      var t=(fi-fA)/gap;var tw=pairs.map(function(pr){return interpStroke(pr.a,pr.b,t,easFn,fA,fB,pr.mi);});
      var et2=easFn(t);
      fadeOutA.forEach(function(sd){var c=JSON.parse(JSON.stringify(sd));c.opacity=(c.opacity!==undefined?c.opacity:1)*(1-et2);if(c.opacity>0.02)tw.push(c);});
      fadeInB.forEach(function(sd){var c=JSON.parse(JSON.stringify(sd));c.opacity=(c.opacity!==undefined?c.opacity:1)*et2;if(c.opacity>0.02)tw.push(c);});
      ld.frames[fi]={strokes:tw,isInterpolated:true,isKeyframe:false};total++;
    }
  }
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast(total+' inbetween(s) générés');
}

// ---- ARCS ----
var arcHandles=[],draggingArc=null;
function renderArcs(){
  arcLayer.removeChildren();arcHandles=[];
  renderNodeHandles();
  renderTransformHandles();
  if(state.tool!=='select'||!state.selectedStrokeIndices.length)return;
  var li=state.activeLayerIdx;var ld=state.layers[li];
  var keys=[];for(var i=0;i<state.totalFrames;i++){if(ld.frames[i].isKeyframe&&ld.frames[i].strokes.length>0)keys.push(i);}
  if(keys.length<2)return;
  var fA=-1,fB=-1;for(var i2=0;i2<keys.length-1;i2++){if(state.currentFrame>=keys[i2]&&state.currentFrame<=keys[i2+1]){fA=keys[i2];fB=keys[i2+1];break;}}
  if(fA<0)return;
  var sA=ld.frames[fA].strokes,sB=ld.frames[fB].strokes;var matches=autoMatch(sA,sB);if(!matches.length)return;
  var sel=state.selectedStrokeIndices;var fm=matches.filter(function(m){return sel.indexOf(m.a)>=0;});if(!fm.length)return;
  arcLayer.activate();var cols=['#ff6b6b','#4ecdc4','#ffe66d','#a29bfe','#fd79a8','#00cec9'];var easFn=getEasing();
  fm.forEach(function(m,di){
    var pA=buildTP(sA[m.a]),pB=buildTP(sB[m.b]);var cA=pA.bounds.center,cB=pB.bounds.center;pA.remove();pB.remove();
    var mIdx=matches.indexOf(m);var ac=getArcCtrl(fA,fB,mIdx,[cA.x,cA.y],[cB.x,cB.y]);var col=cols[di%cols.length];var zs=1/view.zoom;
    var ap=new Path({insert:true});ap.strokeColor=new Color(col);ap.strokeColor.alpha=.6;ap.strokeWidth=2*zs;ap.dashArray=[6*zs,4*zs];
    for(var s=0;s<=24;s++){var t=s/24;var x=qBez(cA.x,ac.x,cB.x,t);var y=qBez(cA.y,ac.y,cB.y,t);if(s===0)ap.moveTo(new Point(x,y));else ap.lineTo(new Point(x,y));}
    new Path.Circle({center:cA,radius:4*zs,insert:true,fillColor:col,opacity:.8});
    new Path.Circle({center:cB,radius:4*zs,insert:true,fillColor:col,opacity:.8});
    var h=new Path.Circle({center:new Point(ac.x,ac.y),radius:7*zs,insert:true});h.fillColor=new Color(1,1,1,.95);h.strokeColor=col;h.strokeWidth=2*zs;
    arcHandles.push({handle:h,fA:fA,fB:fB,matchIdx:mIdx,ptA:[cA.x,cA.y],ptB:[cB.x,cB.y]});
  });
  userLayers[state.activeLayerIdx].activate();
}

// ---- ONION ----
function renderOS(){
  onionPrevLayer.removeChildren();onionNextLayer.removeChildren();
  if(!state.onionSkin)return;var li=state.activeLayerIdx;var cf=state.currentFrame;
  for(var fi=cf-1;fi>=state.onionIn&&fi>=0;fi--){var strokes=getEffectiveStrokes(li,fi);if(!strokes.length)continue;var dist=cf-fi;var op=(state.onionPrevOpacity/100)*Math.max(.15,1-dist*.2);strokes.forEach(function(sd){var p=desP(sd,onionPrevLayer,op);if(state.onionMode==='tinted')p.strokeColor=new Color(1,.3,.3,op);else if(state.onionMode==='outline'){p.strokeColor=new Color(1,.3,.3,op*.8);p.strokeWidth=1;}else p.opacity=op;});}
  for(var fi2=cf+1;fi2<=state.onionOut&&fi2<state.totalFrames;fi2++){var strokes2=getEffectiveStrokes(li,fi2);if(!strokes2.length)continue;var dist2=fi2-cf;var op2=(state.onionNextOpacity/100)*Math.max(.15,1-dist2*.2);strokes2.forEach(function(sd){var p=desP(sd,onionNextLayer,op2);if(state.onionMode==='tinted')p.strokeColor=new Color(.3,.55,1,op2);else if(state.onionMode==='outline'){p.strokeColor=new Color(.3,.55,1,op2*.8);p.strokeWidth=1;}else p.opacity=op2;});}
  userLayers[state.activeLayerIdx].activate();
}

// ---- UNDO ----
// App-wide undo: every pushUndo snapshots the entire layers tree (all
// frames, all layers, frame count). The old per-current-frame snapshot
// couldn't restore anything that touched other frames or the arrays'
// shape — moving/pasting frames, tweens, layer ops — which made ⌘Z feel
// like it only worked for strokes. A full snapshot is ~a few hundred KB
// per entry (JSON) which at maxUndo=60 stays well within budget.
function pushUndo(){pushUndoLayers();}
function layersSnapshotNow(){return{type:'layers',layers:JSON.parse(JSON.stringify(state.layers)),active:state.activeLayerIdx,totalFrames:state.totalFrames};}
function pushUndoLayers(){saveAllLayerFrames();state.undoStack.push(layersSnapshotNow());if(state.undoStack.length>state.maxUndo)state.undoStack.shift();state.redoStack=[];}
function restoreLayersSnapshot(s){
  while(userLayers.length>0)userLayers.pop().remove();
  state.layers=[];
  s.layers.forEach(function(ld){var idx=createUserLayer(ld.name);state.layers[idx]=ld;});
  state.totalFrames=s.totalFrames;window._totalF=s.totalFrames;
  if(state.waOut>=s.totalFrames)state.waOut=s.totalFrames-1;window._waOut=state.waOut;
  if(state.currentFrame>=s.totalFrames)state.currentFrame=s.totalFrames-1;
  state.activeLayerIdx=Math.max(0,Math.min(s.active,state.layers.length-1));
  activateUL(state.activeLayerIdx);
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
}
function undo(){if(!state.undoStack.length){showToast('Rien à annuler');return;}var s=state.undoStack.pop();
if(s.type==='layers'){state.redoStack.push(layersSnapshotNow());restoreLayersSnapshot(s);return;}
var cur={frame:state.currentFrame,layers:[]};for(var i=0;i<state.layers.length;i++){var f=state.layers[i].frames[state.currentFrame];cur.layers.push({strokes:JSON.parse(JSON.stringify(f.strokes)),isKeyframe:f.isKeyframe,isInterpolated:f.isInterpolated});}state.redoStack.push(cur);for(var i2=0;i2<s.layers.length&&i2<state.layers.length;i2++){var tf=state.layers[i2].frames[s.frame];tf.strokes=s.layers[i2].strokes;tf.isKeyframe=s.layers[i2].isKeyframe;tf.isInterpolated=s.layers[i2].isInterpolated;}if(s.frame!==state.currentFrame)state.currentFrame=s.frame;loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();}
function redo(){if(!state.redoStack.length){showToast('Rien à refaire');return;}var s=state.redoStack.pop();
if(s.type==='layers'){state.undoStack.push(layersSnapshotNow());restoreLayersSnapshot(s);return;}
var cur={frame:state.currentFrame,layers:[]};for(var i=0;i<state.layers.length;i++){var f=state.layers[i].frames[state.currentFrame];cur.layers.push({strokes:JSON.parse(JSON.stringify(f.strokes)),isKeyframe:f.isKeyframe,isInterpolated:f.isInterpolated});}state.undoStack.push(cur);for(var i2=0;i2<s.layers.length&&i2<state.layers.length;i2++){var tf=state.layers[i2].frames[s.frame];tf.strokes=s.layers[i2].strokes;tf.isKeyframe=s.layers[i2].isKeyframe;tf.isInterpolated=s.layers[i2].isInterpolated;}if(s.frame!==state.currentFrame)state.currentFrame=s.frame;loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();}

