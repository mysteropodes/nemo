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
// hasRealStroke is AUTHORITATIVE over strokeColor (app.js's desP already
// treats it this way — see its own header comment: a path with NO real
// stroke keeps serP's historical '#ffffff'/'#fff' phantom fallback sitting
// in the stored field, purely so old data/other code paths that expect a
// string don't choke, but it must never be read as an actual color).
// strokeType/strokeFeat used to read sd.strokeColor raw — bug found while
// stress-testing (2026-07-17): saveAllLayerFrames runs at the START of
// every generateTweens call, round-tripping whichever keyframe sits at
// state.currentFrame through desP/serP — a fill-only shape (hasRealStroke:
// false) that happened to be the CURRENT frame when Tween was pressed came
// back out with a real-looking '#ffffff' in strokeColor, misclassifying it
// as type 'both' (typePenalty 0.5, one of the largest single terms) against
// its own un-round-tripped partner keyframe still correctly typed 'fill' —
// a pure serialization artifact, not a real difference between the two
// drawings, silently pushing an otherwise-perfect match toward rejection.
function realStrokeColor(sd){return sd.hasRealStroke===false?null:sd.strokeColor;}
function strokeType(sd){if(sd.isVectorBrush)return'vb';var hasS=!!realStrokeColor(sd),hasF=!!sd.fillColor;if(hasS&&hasF)return'both';if(hasF)return'fill';return'stroke';}
// Pressure-brush strokes are stored as their filled OUTLINE (a closed
// sausage around the drawn line) — comparing outlines wrecks proximity,
// curvature and open/closed detection. All geometric features are computed
// on the actual drawn centerline instead whenever it's available.
function buildTPFeat(sd){
  var usingCenter=sd.isVectorBrush&&sd.centerSegments&&sd.centerSegments.length>1;
  var segs=usingCenter?sd.centerSegments:sd.segments;
  var p=new Path({insert:false});
  segs.forEach(function(s){p.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
  // Bug found by stress-testing (2026-07-17): never setting `.closed` here
  // meant every downstream feature (strokeFeat's 16-point Chamfer samples,
  // turning-angle profile, centroid, Fourier descriptor) measured a CLOSED
  // shape's own length short by its implicit closing segment — the same
  // root cause fixed in resamplePJS/resamplePairFeatureAware, but here it
  // corrupts the MATCHING COST itself, not just the resample stage. A
  // vector-brush centerline stays open regardless (it's the drawn stroke,
  // never a closed loop even when its rendered ribbon outline is).
  p.closed=!usingCenter&&!!sd.closed;
  return p;
}
// Fourier magnitude descriptor of a boundary point sequence — captures
// the overall SILHOUETTE of a stroke independent of art style. It's
// translation-invariant by construction (points are already centroid-
// relative), rotation-invariant because rotating the shape by θ multiplies
// every DFT bin by e^{iθ} which leaves its MAGNITUDE unchanged (only phase
// is discarded here, deliberately), and scale-invariant because the
// magnitude vector is normalized by its own L2 norm. Only the first few
// low-frequency bins are kept: those encode the coarse shape (an "eye" vs
// an "eyebrow" vs "a fold of cloth"), which is what should read as the
// same across a bold cartoon stroke and a detailed realistic one of the
// same subject — the fine wobble/hachure-density differences between art
// styles live almost entirely in the higher frequencies this deliberately
// excludes, instead of polluting the comparison. Existing per-stroke cost
// terms (Chamfer proximity, turning-angle curvature) already do a good job
// on strokes drawn in a consistent style; this adds a second, independent
// style-agnostic signal matchSc can fall back on when those disagree.
var FOURIER_BINS=6;
function fourierDescriptor(pts,cx,cy){
  var n=pts.length;if(n<3)return null;
  var re=new Array(FOURIER_BINS+1).fill(0),im=new Array(FOURIER_BINS+1).fill(0);
  for(var k=0;k<n;k++){
    var x=pts[k][0]-cx,y=pts[k][1]-cy;
    for(var m=1;m<=FOURIER_BINS;m++){
      var ang=-2*Math.PI*m*k/n,c=Math.cos(ang),s=Math.sin(ang);
      re[m]+=x*c-y*s;im[m]+=x*s+y*c;
    }
  }
  var mags=[];for(var m2=1;m2<=FOURIER_BINS;m2++)mags.push(Math.sqrt(re[m2]*re[m2]+im[m2]*im[m2]));
  var norm=Math.sqrt(mags.reduce(function(a,b){return a+b*b;},0))||1;
  return mags.map(function(v){return v/norm;});
}
function fourierDist(a,b){
  if(!a||!b)return 0;
  var n=Math.min(a.length,b.length),s=0;
  for(var i=0;i<n;i++){var d=a[i]-b[i];s+=d*d;}
  return Math.min(1,Math.sqrt(s));
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
  var closedHeuristic=p.segments.length>3&&dl<Math.max(4,diag*0.08)&&len>diag*1.2;
  // Real topology beats guessed topology: sd.closed is the actual
  // open/closed flag Paper.js stored on the path (ground truth), but a
  // pressure-brush stroke compared here is its DRAWN CENTERLINE
  // (buildTPFeat above swaps in centerSegments), which is never a closed
  // Path even though its rendered ribbon OUTLINE (sd.closed) always is —
  // for that case the geometric heuristic below is the only signal that
  // means anything, so it's kept as the sole source. For every other
  // stroke type, trusting sd.closed instead of re-guessing from where the
  // endpoints happen to land avoids exactly the "visually plausible but
  // topologically wrong" mismatch this exists to prevent (e.g. two nearly-
  // touching endpoints on a genuinely open stroke previously read as
  // "closed", silently matching it against real closed loops).
  var usingCenterline=sd.isVectorBrush&&sd.centerSegments&&sd.centerSegments.length>1;
  var isClosed=(!usingCenterline&&typeof sd.closed==='boolean')?sd.closed:closedHeuristic;
  var fourier=fourierDescriptor(pts,cx,cy);
  p.remove();return{cx:cx,cy:cy,length:len,dirX:dx,dirY:dy,bounds:{x:b.x,y:b.y,w:b.width,h:b.height},shape:shape,pts:pts,turn:turn,closed:isClosed,strokeCol:parseHexColor(realStrokeColor(sd)),fillCol:parseHexColor(sd.fillColor),type:strokeType(sd),fourier:fourier};}
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
  // 3b. SILHOUETTE: style-agnostic Fourier magnitude descriptor distance —
  // see fourierDescriptor's own comment. Independent of curveT (which
  // compares LOCAL turning at each sample) in that this compares the
  // GLOBAL coarse outline, so it can still agree two shapes are "the same
  // drawing" even when local stylistic detail (hachures, a wobblier line,
  // extra tiny curvature) makes curveT noisy — the actual goal behind
  // adding it: matching should hold up across cartoon/simple/realistic
  // linework of the same underlying shape, not just within one style.
  var fourD=fourierDist(fA.fourier,fB.fourier);
  // 4. secondary cues & hard penalties
  var rdx=fA.relX-fB.relX,rdy=fA.relY-fB.relY;var rel=Math.min(1,Math.sqrt(rdx*rdx+rdy*rdy));
  var lenRatio=Math.max(fA.length,fB.length)/Math.max(1,Math.min(fA.length,fB.length));
  var ratioPen=lenRatio>2?Math.min(0.7,(lenRatio-2)*0.35):0;
  var closedPen=fA.closed!==fB.closed?0.35:0;
  var aArea=fA.bounds.w*fA.bounds.h,bArea=fB.bounds.w*fB.bounds.h;
  var szD=Math.abs(aArea-bArea)/Math.max(1,Math.max(aArea,bArea));
  var colD=(colorDist(fA.strokeCol,fB.strokeCol)+colorDist(fA.fillCol,fB.fillCol))/2;
  var typePenalty=fA.type!==fB.type?0.5:0;
  // HARD color-identity penalty (2026-07-17, production stress-test) —
  // the file's own header comment already states "a stroke's color is what
  // a viewer actually reads as its identity", but colD's soft 0.15 weight
  // couldn't enforce it: two same-shape balls of DIFFERENT colors crossing
  // paths (a hand passing in front of a face — everyday cel animation)
  // matched by POSITION instead (proximity's 0.48 dwarfs a full-scale
  // color clash at 0.604*0.15≈0.09), so both balls stood perfectly still
  // and hard-swapped colors at the tween midpoint instead of crossing.
  // A clearly-different hue on the same channel gets the same flat-penalty
  // treatment a type mismatch already has — 0.4, big enough to make any
  // plausible-motion pairing win, while small hue drift (shading tweaks
  // between keys, <0.35 normalized) stays penalty-free.
  var fillClash=fA.fillCol&&fB.fillCol&&colorDist(fA.fillCol,fB.fillCol)>0.35;
  var strokeClash=fA.strokeCol&&fB.strokeCol&&colorDist(fA.strokeCol,fB.strokeCol)>0.35;
  var colorPenalty=(fillClash||strokeClash)?0.4:0;
  var idxBonus=sameIndex?-0.03:0;
  return proxT*.48+alignT*.15+curveT*.12+fourD*.10+rel*.10+szD*.06+colD*.15+typePenalty+colorPenalty+ratioPen+closedPen+idxBonus;
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
  // realStrokeColor (not raw sd.strokeColor) so the WASM path sees the same
  // phantom-free color a fill-only shape's OWN JS-side strokeFeat/
  // strokeType do — otherwise WASM's own stroke_feat (tweenmatch.rs, which
  // has no concept of hasRealStroke at all) would still see serP's
  // '#ffffff' fallback and disagree with the JS fallback path on the exact
  // same input, purely depending on which one happened to run.
  return{segments:sd.segments||[],centerSegments:sd.centerSegments,strokeColor:realStrokeColor(sd)||null,fillColor:sd.fillColor||null,isVectorBrush:!!sd.isVectorBrush,closed:!!sd.closed};
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
  if(!transform)return uncrossMatches(matches,fA,fB);
  // the fitted motion (force line) is applied to every A stroke's actual
  // sample points, so pass 2's proximity measures "how far is this line
  // from where the drawing's motion says it should be" — not raw distance
  var ptsT=fA.map(function(f){return f.pts.map(function(pt){var q=applySimilarityTransform(transform,pt[0],pt[1]);return[q.x,q.y];});});
  var cost2=buildCost(ptsT);
  var assign2=hungarian(cost2);
  var matches2=[];
  for(var a4=0;a4<n;a4++){var b4=assign2[a4];if(b4!==undefined&&b4>=0&&b4<m)matches2.push({a:a4,b:b4,score:cost2[a4][b4]});}
  return uncrossMatches(matches2,fA,fB);
}
// ---- trajectory uncrossing (2026-07-17, "les yeux s'inversent") ----
// Found on a real hand-drawn animation: two nearly-identical eye strokes
// ~40px apart, whole face shifting diagonally between keys — the crossed
// assignment's total point distance (161px) was within 3% of the correct
// one (157px), so neither the Hungarian nor the force-line pass could
// tell them apart, and the eyes traded places mid-tween. The signal a
// human inbetweener uses here isn't proximity at all: two motion
// trajectories that CROSS each other, for strokes this similar, are
// almost always a matching error — cel features keep their spatial
// arrangement. Post-pass: for every pair of matches whose straight-line
// centroid trajectories intersect, try swapping the B partners; accept
// the swap when it doesn't cost meaningfully more than the crossed
// version (small tolerance in favor of uncrossing — near-ties are
// exactly the ambiguous case this exists for). A GENUINE crossing (the
// red/blue balls passing each other) survives: swapping there hits the
// color-clash/type penalties, far above the tolerance. Sweeps until
// stable (bounded) since one swap can uncross/cross a third trajectory.
function _segsIntersect(p1,p2,p3,p4){
  function ccw(a,b,c){return (c.y-a.y)*(b.x-a.x)>(b.y-a.y)*(c.x-a.x);}
  return ccw(p1,p3,p4)!==ccw(p2,p3,p4)&&ccw(p1,p2,p3)!==ccw(p1,p2,p4);
}
var UNCROSS_TOL=0.08;
function uncrossMatches(ms,fA,fB){
  if(ms.length<2)return ms;
  for(var sweep=0;sweep<4;sweep++){
    var swapped=false;
    for(var i=0;i<ms.length;i++)for(var j=i+1;j<ms.length;j++){
      var m1=ms[i],m2=ms[j];
      var a1={x:fA[m1.a].cx,y:fA[m1.a].cy},b1={x:fB[m1.b].cx,y:fB[m1.b].cy};
      var a2={x:fA[m2.a].cx,y:fA[m2.a].cy},b2={x:fB[m2.b].cx,y:fB[m2.b].cy};
      if(!_segsIntersect(a1,b1,a2,b2))continue;
      var cur=matchSc(fA[m1.a],fB[m1.b],m1.a===m1.b)+matchSc(fA[m2.a],fB[m2.b],m2.a===m2.b);
      var swp=matchSc(fA[m1.a],fB[m2.b],m1.a===m2.b)+matchSc(fA[m2.a],fB[m1.b],m2.a===m1.b);
      if(swp<=cur+UNCROSS_TOL){
        var tmp=m1.b;m1.b=m2.b;m2.b=tmp;
        m1.score=matchSc(fA[m1.a],fB[m1.b],m1.a===m1.b);
        m2.score=matchSc(fA[m2.a],fB[m2.b],m2.a===m2.b);
        swapped=true;
      }
    }
    if(!swapped)break;
  }
  return ms;
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
    out.closed=!!sd.closed;
    return out;
  }catch(e){console.warn('[geometry-wasm] resample_stroke failed, falling back to JS',e);return null;}
}
function resampleP(sd,n){var w=_resampleStrokeWasm(sd,n);if(w)return w;return resamplePJS(sd,n);}
// Bug found by stress-testing (2026-07-17): this temp Path used to never
// get `.closed` set before reading `.length` — for ANY closed shape (the
// overwhelming majority of drawn fills), Paper.js's own length therefore
// measured only the OPEN span (last drawn point back to the first), never
// the implicit closing segment, so every resample fell short of a true
// full loop by exactly that missing segment's length. Harmless-looking on
// its own (still n evenly-spaced points), but it silently shifts what each
// output index actually corresponds to around the loop — and compounds
// with alignResampledPair's own rotation search (same missing `.closed`
// bug there, fixed alongside this) to leave a residual few-degrees-to-one-
// vertex misalignment that showed up as visible self-intersection at the
// tween's midpoint on shapes needing any rotation correction.
function resamplePJS(sd,n){var p=new Path({insert:false});sd.segments.forEach(function(s){p.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});p.closed=!!sd.closed;var len=p.length;if(len<1||n<2){p.remove();return sd;}var segs=[];for(var i=0;i<n;i++){var t=i/(n-1);var off=t*len;var pt=p.getPointAt(off);if(!pt)pt=p.getPointAt(len);var tan=p.getTangentAt(off);if(!tan)tan=new Point(1,0);var hl=len/(n-1)/3;segs.push({point:[pt.x,pt.y],handleIn:i===0?[0,0]:[-tan.x*hl,-tan.y*hl],handleOut:i===n-1?[0,0]:[tan.x*hl,tan.y*hl]});}p.remove();return{segments:segs,closed:!!sd.closed,strokeColor:sd.strokeColor,strokeWidth:sd.strokeWidth,strokeCap:sd.strokeCap,strokeJoin:sd.strokeJoin,fillColor:sd.fillColor||null,opacity:sd.opacity!==undefined?sd.opacity:1};}
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
// ---- FEATURE-AWARE RETOPOLOGY ----
// Plain uniform arc-length resampling (resamplePJS/resampleCenterlineJS
// above) spreads n points evenly by DISTANCE, with zero regard for where
// each shape's actual corners/cusps land — a sharp corner on A can end up
// several samples away from where B's corresponding corner falls, and the
// interpolation (straight per-index lerp in interpStroke) then smears that
// corner into a soft curve for the whole tween instead of holding a crisp
// point, or times it wrong relative to the rest of the shape. This doesn't
// change the point BUDGET (still exactly `n` samples, so interpStroke's
// index-paired lerp needs no changes) — it changes WHERE within that budget
// the samples land: any point close to a detected corner on EITHER shape
// gets snapped exactly onto it, so both A and B keep a real sample sitting
// on (or very near) their own landmarks, and — since both sides use the
// SAME shared fraction list — roughly at the same relative position along
// the curve, which is the closest approximation of "corresponding points"
// achievable without solving actual point-to-point correspondence.
//
// Curvature is estimated by dense fixed-step sampling (independent of the
// path's own segment count — matters for the ribbon-shaped outlines
// pressure-brush strokes are stored as, which have far more segments than
// meaningful corners) rather than reading Paper's authored segments, so it
// also works on paths with no "real" authored corners at all (resampled/
// erased/boolean-op output).
function detectFeatureFractions(paperPath,maxFeatures){
  var len=paperPath.length;
  if(!(len>0))return[];
  var STEPS=96;
  var angles=[];
  for(var i=0;i<=STEPS;i++){
    var tan=paperPath.getTangentAt(Math.min(len,len*i/STEPS));
    angles.push(tan?Math.atan2(tan.y,tan.x):0);
  }
  var turn=[];
  for(var k=1;k<angles.length;k++){
    var d=angles[k]-angles[k-1];
    while(d>Math.PI)d-=Math.PI*2;while(d<-Math.PI)d+=Math.PI*2;
    turn.push(Math.abs(d));
  }
  // local maxima of |turning angle| above a modest threshold — corners and
  // cusps stand out as sharp spikes in this signal; smooth curves stay low.
  var TH=0.28;
  var feats=[];
  for(var m=0;m<turn.length;m++){
    if(turn[m]<TH)continue;
    var prev=turn[m-1]!==undefined?turn[m-1]:-1;
    var next=turn[m+1]!==undefined?turn[m+1]:-1;
    if(turn[m]>=prev&&turn[m]>=next)feats.push({t:(m+1)/STEPS,mag:turn[m]});
  }
  feats.sort(function(a,b){return b.mag-a.mag;});
  return feats.slice(0,maxFeatures).map(function(f){return f.t;}).sort(function(a,b){return a-b;});
}
// Builds ONE shared t-fraction list (length n) from the union of A's and
// B's detected feature fractions, snapping the nearest uniform grid index
// onto each feature instead of inserting extra points — keeps the point
// count exactly n (interpStroke needs equal-length arrays) while biasing
// WHERE those n samples fall toward both shapes' actual landmarks. Falls
// back to a plain uniform grid if there are no notable features (e.g. a
// circle) — same result as the old behavior in that case.
function buildSharedFractions(pathA,pathB,n){
  var grid=[];for(var i=0;i<n;i++)grid.push(i/(n-1));
  if(n<4)return grid;
  var maxFeat=Math.max(2,Math.min(10,Math.floor(n/4)));
  var feats=detectFeatureFractions(pathA,maxFeat).concat(detectFeatureFractions(pathB,maxFeat));
  if(!feats.length)return grid;
  var minGap=1/(n*2); // don't let two features collapse onto the same grid slot
  var used={};
  feats.forEach(function(f){
    if(f<=0||f>=1)return; // endpoints are pinned — snapping them risks breaking the start/end continuity resampleP's endpoint handling relies on
    var bestI=-1,bestD=Infinity;
    for(var gi=1;gi<n-1;gi++){
      if(used[gi])continue;
      var d=Math.abs(grid[gi]-f);
      if(d<bestD){bestD=d;bestI=gi;}
    }
    if(bestI>=0&&bestD<0.5/n*4){grid[bestI]=f;used[bestI]=true;}
  });
  grid.sort(function(a,b){return a-b;});
  grid[0]=0;grid[n-1]=1;
  // Re-enforce a minimum gap after sorting — two features snapped to
  // adjacent grid slots can end up closer than minGap, which would starve
  // interpStroke's per-vertex lerp of any real spacing between them.
  for(var j=1;j<n;j++)if(grid[j]-grid[j-1]<minGap)grid[j]=grid[j-1]+minGap;
  if(grid[n-1]>1){var over=grid[n-1]-1;for(var j2=0;j2<n;j2++)grid[j2]=Math.max(0,grid[j2]-over*(j2/(n-1)));}
  return grid;
}
function _sampleAtFractions(p,len,fractions){
  var segs=[];
  for(var i=0;i<fractions.length;i++){
    var off=fractions[i]*len;
    var pt=p.getPointAt(off);if(!pt)pt=p.getPointAt(len);
    var tan=p.getTangentAt(off);if(!tan)tan=new Point(1,0);
    var hl=len/Math.max(1,fractions.length-1)/3;
    segs.push({point:[pt.x,pt.y],handleIn:i===0?[0,0]:[-tan.x*hl,-tan.y*hl],handleOut:i===fractions.length-1?[0,0]:[tan.x*hl,tan.y*hl]});
  }
  return segs;
}
// Feature-aware replacement for the plain `rfn(spec.aData,resN)` /
// `rfn(spec.bData,resN)` independent-resample pair used to build each
// matched pair's interpolation input (generateTweens) — same output shape
// as resampleP/resampleCenterline, just sampled at a shared, landmark-
// biased fraction list instead of two independently-uniform ones.
function resamplePairFeatureAware(aData,bData,n,isVB){
  if(n<4){var rfn0=isVB?resampleCenterline:resampleP;return[rfn0(aData,n),rfn0(bData,n)];}
  var segKeyA=isVB?'centerSegments':'segments',segKeyB=segKeyA;
  var usingCenterA=isVB&&aData.centerSegments&&aData.centerSegments.length>1;
  var usingCenterB=isVB&&bData.centerSegments&&bData.centerSegments.length>1;
  var srcA=usingCenterA?aData.centerSegments:aData.segments;
  var srcB=usingCenterB?bData.centerSegments:bData.segments;
  if(!srcA||!srcB||srcA.length<2||srcB.length<2){var rfn1=isVB?resampleCenterline:resampleP;return[rfn1(aData,n),rfn1(bData,n)];}
  var pA=new Path({insert:false});srcA.forEach(function(s){pA.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
  var pB=new Path({insert:false});srcB.forEach(function(s){pB.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
  // Same missing-`.closed` bug as resamplePJS (see its comment) — a
  // vector-brush CENTERLINE stays open regardless (it's the drawn stroke
  // path, never a closed loop even when its rendered ribbon outline is),
  // matching strokeFeat's own usingCenterline special-case.
  pA.closed=!usingCenterA&&!!aData.closed;pB.closed=!usingCenterB&&!!bData.closed;
  var lenA=pA.length,lenB=pB.length;
  if(!(lenA>0)||!(lenB>0)){pA.remove();pB.remove();var rfn2=isVB?resampleCenterline:resampleP;return[rfn2(aData,n),rfn2(bData,n)];}
  var fractions=buildSharedFractions(pA,pB,n);
  var segsA=_sampleAtFractions(pA,lenA,fractions);
  var segsB=_sampleAtFractions(pB,lenB,fractions);
  var ra,rb;
  if(isVB){
    function widthsAt(srcSegs,total){
      var segLens=[0];for(var i=1;i<srcSegs.length;i++)segLens.push(segLens[i-1]+new Point(srcSegs[i].point[0],srcSegs[i].point[1]).getDistance(new Point(srcSegs[i-1].point[0],srcSegs[i-1].point[1])));
      var tot=segLens[segLens.length-1]||1;
      return fractions.map(function(t){
        var targetLen=t*tot;
        var wi=0;while(wi<segLens.length-2&&segLens[wi+1]<targetLen)wi++;
        var span=Math.max(0.0001,segLens[wi+1]-segLens[wi]);
        var lt=Math.min(1,Math.max(0,(targetLen-segLens[wi])/span));
        return srcSegs[wi].width+(srcSegs[wi+1].width-srcSegs[wi].width)*lt;
      });
    }
    ra={segments:segsA,widths:widthsAt(srcA,lenA),isVectorBrush:true,strokeColor:null,fillColor:aData.fillColor||null,opacity:aData.opacity!==undefined?aData.opacity:1};
    rb={segments:segsB,widths:widthsAt(srcB,lenB),isVectorBrush:true,strokeColor:null,fillColor:bData.fillColor||null,opacity:bData.opacity!==undefined?bData.opacity:1};
  }else{
    ra={segments:segsA,closed:!!aData.closed,strokeColor:aData.strokeColor,strokeWidth:aData.strokeWidth,strokeCap:aData.strokeCap,strokeJoin:aData.strokeJoin,fillColor:aData.fillColor||null,opacity:aData.opacity!==undefined?aData.opacity:1};
    rb={segments:segsB,closed:!!bData.closed,strokeColor:bData.strokeColor,strokeWidth:bData.strokeWidth,strokeCap:bData.strokeCap,strokeJoin:bData.strokeJoin,fillColor:bData.fillColor||null,opacity:bData.opacity!==undefined?bData.opacity:1};
  }
  pA.remove();pB.remove();
  return[ra,rb];
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
// Per-keyframe-pair override (state.tweenEasing), falling back to the
// global curve (getEasing()) for any pair that never got its own —
// COMPLEMENTS the global curve, doesn't replace it. evalPointsCurve is the
// pure evaluator (ui.js) — independent of whatever the curve widget
// happens to be showing right now, unlike evalCurve/activePoints which
// only reflect the currently-open view.
function getEasingForPair(li,fA,fB){
  var key=li+':'+fA+'-'+fB;
  var custom=state.tweenEasing&&state.tweenEasing[key];
  if(custom&&custom.points&&custom.points.length&&window._curveEditor&&window._curveEditor.evalPointsCurve){
    var pts=custom.points;
    return function(t){return window._curveEditor.evalPointsCurve(pts,t);};
  }
  return getEasing();
}
function lerp(a,b,t){return a+(b-a)*t;}function lerpV(a,b,t){return[lerp(a[0],b[0],t),lerp(a[1],b[1],t)];}
function arcKey(fA,fB,i){return fA+'-'+fB+'-'+i;}
// Cubic bezier through ptA/ptB with independent OUT (leaving A) and IN
// (arriving at B) handles — upgraded 2026-07 from a single shared quadratic
// control point, live feedback: "le motion path de caméra a des poignées
// pour le in et out alors que le motion path de tween c'est juste une
// poignée pour les 2 donc moins réglable". camera.js/motion.js already use
// this exact two-handle rig for their own motion paths; this brings the
// REAL tween-arc system (state.motionArcs — genuinely read by interpStroke
// below to bend the tween's spatial path, not a decorative overlay) to
// parity with them instead of being a separate, disconnected visualization.
//
// Default (untouched) offsets sit at exactly 1/3 and 2/3 along the A->B
// segment — the standard exact cubic representation of a straight line.
// Verified by construction, not just "looks straight": at every t this
// reproduces the OLD quadratic-at-exact-midpoint system's parametrization
// exactly (same position at every t, not merely the same endpoints), so
// every arc nobody has touched yet tweens identically to before. A literal
// [0,0] offset default was tried first and rejected — the resulting handle
// sits exactly ON its own endpoint, indistinguishable from the endpoint dot
// itself rather than merely subtle (the mistake the retired tween-motion-
// path Labs prototype made and had to fix); 1/3 and 2/3 are already
// visibly separate from both endpoints with no extra display-only fallback
// needed.
//
// Old single-handle `{cx,cy}` entries (pre-2026-07) are NOT migrated —
// state.motionArcs is a young (v16), rarely-touched feature; an old custom
// arc just reads back as this new straight-line default instead of being
// silently reinterpreted. Simpler and safer than guessing an equivalent
// two-handle shape from one point, and this is still ui-ux-experimental.
function getArcHandles(fA,fB,i,ptA,ptB){
  var k=arcKey(fA,fB,i);var a=state.motionArcs[k];
  var dx=ptB[0]-ptA[0],dy=ptB[1]-ptA[1];
  // Field names match setArcHandle's `which` values ('out'/'in') exactly —
  // confirmed live the hard way: an earlier version read a.hOut/a.hIn here
  // while setArcHandle wrote cur[which] (i.e. cur.out/cur['in']), so a
  // dragged handle's offset was stored correctly but this always fell
  // through to the untouched default regardless, silently ignoring every
  // drag (state.motionArcs held the right data; nothing ever read it back).
  var hOut=(a&&a.out)||[dx/3,dy/3];
  var hIn=(a&&a['in'])||[-dx/3,-dy/3];
  return { out:[ptA[0]+hOut[0],ptA[1]+hOut[1]], in:[ptB[0]+hIn[0],ptB[1]+hIn[1]] };
}
function setArcHandle(fA,fB,i,which,ptA,ptB,x,y){
  var k=arcKey(fA,fB,i);
  var cur=state.motionArcs[k]||{};
  var anchor=which==='out'?ptA:ptB;
  cur[which]=[x-anchor[0],y-anchor[1]];
  state.motionArcs[k]=cur;
}
function cubicBez(a,c1,c2,b,t){var u=1-t;return u*u*u*a+3*u*u*t*c1+3*u*t*t*c2+t*t*t*b;}
// v16: motion-arc handles are keyed by literal frame numbers (arcKey above)
// with no separate "belongs to this keyframe pair" identity — retiming a
// keyframe (dragging it to a new frame index, timeline.js moveFrames)
// used to leave the old fA-fB-i entries stranded forever (nothing ever
// deleted them) while the new fA-fB pair silently started blank, i.e. a
// custom arc "reset" on retime and leaked a stale orphaned entry. The cx/cy
// offset itself is still valid after a retime (it's relative to the
// matched shapes' own positions, which a pure timing change doesn't
// touch) — so this MOVES the entry to the new key instead of dropping it,
// called by moveFrames for every keyframe pair whose fA and/or fB frame
// index changed in that move.
function rekeyTweenPairData(fA,fB,newFA,newFB){
  if(fA===newFA&&fB===newFB)return;
  var oldPrefix=fA+'-'+fB+'-';
  Object.keys(state.motionArcs).forEach(function(k){
    if(k.indexOf(oldPrefix)!==0)return;
    var idx=k.slice(oldPrefix.length);
    var val=state.motionArcs[k];
    delete state.motionArcs[k];
    state.motionArcs[newFA+'-'+newFB+'-'+idx]=val;
  });
}
// Rotates+scales a vector (px,py) by angle ang (radians) and uniform factor
// scale — the rigid part of a similarity transform, used below to give
// tweens real rotation instead of every point sliding along a straight
// line toward its counterpart (which, for a stroke that's simply turned
// between its two keyframes — a swinging arm, a clock hand, a turning
// wheel spoke — makes the shape visibly warp/flatten through its own
// center instead of appearing to rotate).
function rotScalePt(px,py,ang,scale){var c=Math.cos(ang),s=Math.sin(ang);return[scale*(px*c-py*s),scale*(px*s+py*c)];}
function interpStroke(rA,rB,t,easFn,fA,fB,mIdx){
  var et=easFn(t);var n=Math.min(rA.segments.length,rB.segments.length);var segs=[];
  var cxA=0,cyA=0,cxB=0,cyB=0;for(var i=0;i<n;i++){cxA+=rA.segments[i].point[0];cyA+=rA.segments[i].point[1];cxB+=rB.segments[i].point[0];cyB+=rB.segments[i].point[1];}cxA/=n;cyA/=n;cxB/=n;cyB/=n;
  var ah=getArcHandles(fA,fB,mIdx,[cxA,cyA],[cxB,cyB]);var cx2=cubicBez(cxA,ah.out[0],ah.in[0],cxB,et);var cy2=cubicBez(cyA,ah.out[1],ah.in[1],cyB,et);
  // Fit the rigid rotation+scale that best explains A's centroid-relative
  // points turning into B's (same similarity-transform math the "force
  // line" matching pass already uses, just fit per-pair here instead of
  // across the whole frame). thetaT/scaleT ramp this transform from
  // identity (t=0, exactly reproduces A) to the full fit (t=1, exactly
  // reproduces B) applied FORWARD from A; thetaB/scaleB ramp the inverse
  // transform from full (t=0, approximates A) to identity (t=1, exactly
  // reproduces B) applied BACKWARD from B — blending the two keeps both
  // keyframe endpoints exact by construction (each formula evaluates to
  // identity at its own boundary regardless of the fitted values), so this
  // can never make a tween's start/end frames deviate from the keyframes
  // themselves, only reshape the path between them. A small dead-zone on
  // theta avoids introducing visible spurious spin on ordinary shape
  // morphs where the least-squares fit finds SOME best-fit rotation just
  // by chance even though the two shapes aren't really related by a turn.
  var theta=0,scaleF=1;
  if(n>=2){
    var loA=[],loB=[];
    for(var li=0;li<n;li++){loA.push({x:rA.segments[li].point[0]-cxA,y:rA.segments[li].point[1]-cyA});loB.push({x:rB.segments[li].point[0]-cxB,y:rB.segments[li].point[1]-cyB});}
    var simT=fitSimilarityTransform(loA,loB);
    if(simT){
      var mag=Math.sqrt(simT.wRe*simT.wRe+simT.wIm*simT.wIm);
      if(mag>0.15&&mag<8){
        var th=Math.atan2(simT.wIm,simT.wRe);
        // A large fitted rotation (>90°) can be a mathematical mirage on a
        // simple shape (a real hand-drawn "bouche qui tourne" bug, mouth
        // going from a wide flat arc to a tall narrow one): a similarity
        // transform only has rotation+UNIFORM scale, no independent x/y
        // stretch, so a pure aspect-ratio change (wide↔tall, no real turn)
        // gets forced through the rotation term since that's the only knob
        // available to explain it — verified on the reported case: even the
        // best-fitting index correspondence found ~159° "rotation" for every
        // candidate, yet it only cut residual ~12x vs the best NON-rotating
        // fit (closed-form optimal scale, no theta) — while a genuine
        // rigid turn (the 90°-rectangle regression test) cuts residual by
        // ~200x. Below that gap, the "rotation" is more likely an artifact
        // of the shape's own change than real turning motion, so it's
        // dropped in favor of the non-rotating fit (mouth still resizes,
        // just doesn't spin to get there).
        if(Math.abs(th)>Math.PI/2){
          var dot=0,aa=0;
          for(var qi=0;qi<n;qi++){dot+=loA[qi].x*loB[qi].x+loA[qi].y*loB[qi].y;aa+=loA[qi].x*loA[qi].x+loA[qi].y*loA[qi].y;}
          var s0=aa>1e-9?dot/aa:mag;
          var resFit=0,resNoRot=0;
          for(var qi2=0;qi2<n;qi2++){
            var x=loA[qi2].x,y=loA[qi2].y;
            var rx=mag*(Math.cos(th)*x-Math.sin(th)*y),ry=mag*(Math.sin(th)*x+Math.cos(th)*y);
            var dxf=rx-loB[qi2].x,dyf=ry-loB[qi2].y;resFit+=dxf*dxf+dyf*dyf;
            var dxn=s0*x-loB[qi2].x,dyn=s0*y-loB[qi2].y;resNoRot+=dxn*dxn+dyn*dyn;
          }
          var ROTATION_TRUST_RATIO=20;
          if(resFit*ROTATION_TRUST_RATIO>=resNoRot){th=0;mag=Math.min(3,Math.max(0.33,s0));}
        }
        if(Math.abs(th)>=0.06)theta=th; // ~3.4° dead-zone
        scaleF=Math.min(3,Math.max(0.33,mag));
      }
    }
  }
  var thetaT=theta*et,scaleT=lerp(1,scaleF,et);
  var thetaB=thetaT-theta,scaleB=lerp(scaleF>1e-6?1/scaleF:1,1,et);
  for(var i2=0;i2<n;i2++){var sA=rA.segments[i2],sB=rB.segments[i2];
    var fwd=rotScalePt(sA.point[0]-cxA,sA.point[1]-cyA,thetaT,scaleT);
    var bwd=rotScalePt(sB.point[0]-cxB,sB.point[1]-cyB,thetaB,scaleB);
    var hiF=rotScalePt(sA.handleIn[0],sA.handleIn[1],thetaT,scaleT),hiB=rotScalePt(sB.handleIn[0],sB.handleIn[1],thetaB,scaleB);
    var hoF=rotScalePt(sA.handleOut[0],sA.handleOut[1],thetaT,scaleT),hoB=rotScalePt(sB.handleOut[0],sB.handleOut[1],thetaB,scaleB);
    segs.push({point:[cx2+lerp(fwd[0],bwd[0],et),cy2+lerp(fwd[1],bwd[1],et)],handleIn:[lerp(hiF[0],hiB[0],et),lerp(hiF[1],hiB[1],et)],handleOut:[lerp(hoF[0],hoB[0],et),lerp(hoF[1],hoB[1],et)]});}
  if(rA.isVectorBrush&&rB.isVectorBrush){
    var widths=[];for(var w=0;w<n;w++)widths.push(lerp(rA.widths[w]||1,rB.widths[w]||1,et));
    var centerSegs=segs.map(function(s,idx){return{point:s.point,handleIn:s.handleIn,handleOut:s.handleOut,width:widths[idx]};});
    return{segments:outlineFromCenterSegs(centerSegs),closed:true,strokeColor:null,strokeWidth:lerp(rA.strokeWidth||3,rB.strokeWidth||3,et),strokeCap:rA.strokeCap||'round',strokeJoin:rA.strokeJoin||'round',fillColor:et<.5?(rA.fillColor||null):(rB.fillColor||null),opacity:lerp(rA.opacity!==undefined?rA.opacity:1,rB.opacity!==undefined?rB.opacity:1,et),isVectorBrush:true,centerSegments:centerSegs};
  }
  // A closed shape tweening into another closed shape should stay closed
  // throughout — matches the same "switch at the halfway point" convention
  // already used for strokeColor/fillColor a few lines up, rather than
  // trying to blend "closedness" itself (not a numeric quantity).
  return{segments:segs,closed:et<.5?!!rA.closed:!!rB.closed,strokeColor:et<.5?rA.strokeColor:rB.strokeColor,strokeWidth:lerp(rA.strokeWidth||3,rB.strokeWidth||3,et),strokeCap:rA.strokeCap||'round',strokeJoin:rA.strokeJoin||'round',fillColor:et<.5?(rA.fillColor||null):(rB.fillColor||null),opacity:lerp(rA.opacity!==undefined?rA.opacity:1,rB.opacity!==undefined?rB.opacity:1,et)};
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
// Gates whether alignResampledPair's cyclic-rotation search runs.
// Loosened 2026-07-17 (bug found on a REAL hand-drawn animation, not a
// synthetic test): a hand-drawn head outline is a nearly-closed LOOP left
// open by a small pen gap (~7% of its own diagonal here), and typically
// drawn starting at a DIFFERENT point around the loop in each keyframe.
// The old 5%-of-diagonal gap test said "open" → only reversal was ever
// tried → index 0 lerped to index 0 with a large angular offset around the
// loop → interpStroke's rigid fit read that offset as a spurious ~80°
// whole-head ROTATION, visibly knotting/spinning the outline mid-tween.
// New test: near-closed = endpoint gap under 15% of the diagonal AND total
// polyline length over 1.5x the diagonal (a genuine loop wraps around —
// its arc length is well above its own bbox diagonal; an open arc like a
// mouth line ~1.4x, a straight stroke ~1.0x, so those keep reversal-only
// alignment). Rotating a near-closed OPEN stroke's seam around the loop
// makes the small pen gap travel to a different spot on the inbetweens —
// a far smaller artifact than the spin it prevents, and the keyframes
// themselves are never touched.
function resampledIsClosed(r){
  var s=r.segments;if(s.length<4)return false;
  var minx=Infinity,miny=Infinity,maxx=-Infinity,maxy=-Infinity;
  s.forEach(function(sg){minx=Math.min(minx,sg.point[0]);miny=Math.min(miny,sg.point[1]);maxx=Math.max(maxx,sg.point[0]);maxy=Math.max(maxy,sg.point[1]);});
  var diag2=(maxx-minx)*(maxx-minx)+(maxy-miny)*(maxy-miny);
  var dx=s[0].point[0]-s[s.length-1].point[0],dy=s[0].point[1]-s[s.length-1].point[1];
  var gap2=dx*dx+dy*dy;
  if(gap2>=Math.max(1,diag2)*0.0225)return false; // 0.15^2 of the diagonal
  var polyLen=0;
  for(var i=1;i<s.length;i++){var ddx=s[i].point[0]-s[i-1].point[0],ddy=s[i].point[1]-s[i-1].point[1];polyLen+=Math.sqrt(ddx*ddx+ddy*ddy);}
  return polyLen*polyLen>diag2*2.25; // length > 1.5x diagonal — real loops only
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
      // Rust's ResampledJsonIn requires isVectorBrush (plain `bool`, no
      // serde default) but the JS resampled dicts only carry the flag when
      // it's TRUE — JSON.stringify drops the undefined, so EVERY non-brush
      // pair failed deserialization and silently fell back to the slower
      // JS alignment (the exact fallback-masks-a-Rust-bug trap CLAUDE.md
      // §3 warns about — only visible as a console.warn flood). Serialize
      // through a shallow wrapper that always materializes the flag.
      var wrap=function(r){return JSON.stringify({segments:r.segments,widths:r.widths!==undefined?r.widths:null,isVectorBrush:!!r.isVectorBrush,strokeColor:r.strokeColor!==undefined?r.strokeColor:null,fillColor:r.fillColor!==undefined?r.fillColor:null});};
      var out=JSON.parse(window.GeometryWasm.align_pair(wrap(a),wrap(b)));
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
// Residual after fitting the best RIGID rotation+scale for a candidate B
// ordering, instead of alignCost's raw centroid-relative point distance.
// Found by stress-testing (2026-07-17): a 300x40 rectangle rotated a
// clean 90° picked the mathematically-optimal-by-alignCost correspondence
// (confirmed by exhaustive brute-force search — not an alignment-search
// bug), yet interpStroke's OWN fitSimilarityTransform on that exact
// correspondence came back mag≈0.34/theta≈-10° instead of the expected
// mag≈1/theta≈90° — visibly non-rigid, "melting" instead of turning.
// Root cause: raw point distance has no notion of "does this
// correspondence read as one coherent rigid motion" — only "are the
// points numerically close after centering" — and a shape with some
// rotational/reflective symmetry (any non-square rectangle included) can
// have MULTIPLE orderings that tie or nearly tie on that measure while
// only one of them is the "clean turn". Scoring candidates by how well
// they fit a SINGLE rigid rotation+scale (what interpStroke will actually
// apply) picks the one that turns instead of the one that merely
// minimizes raw distance — the traditional "line of force" idea applied
// to the alignment decision itself, not just the final interpolation.
function rotationFitResidual(a,b){
  var n=Math.min(a.segments.length,b.segments.length);
  var ptsA=[],ptsB=[];
  for(var i=0;i<n;i++){ptsA.push({x:a.segments[i].point[0],y:a.segments[i].point[1]});ptsB.push({x:b.segments[i].point[0],y:b.segments[i].point[1]});}
  var t=fitSimilarityTransform(ptsA,ptsB);
  if(!t)return alignCost(a,b); // degenerate (coincident points) — fall back to plain distance
  var s=0;
  for(var i2=0;i2<n;i2++){
    var q=applySimilarityTransform(t,ptsA[i2].x,ptsA[i2].y);
    var dx=q.x-ptsB[i2].x,dy=q.y-ptsB[i2].y;
    s+=dx*dx+dy*dy;
  }
  return s;
}
// The rotation-fit criterion is CLOSED-shapes-only (2026-07-17, found on
// a real hand-drawn face): for a near-straight OPEN stroke (eyebrow,
// eyelid), reversing the point order is geometrically indistinguishable
// from a ~180° rotation — so judging candidates by residual-after-fitting-
// a-rotation "explains" the reversed ordering with a perfect half-spin
// (residual ≈0) and prefers it over the honest as-is ordering (whose small
// residual is just hand-drawn wobble). interpStroke then faithfully plays
// that fitted spin: eyebrows visibly twirled -145°..-165° for no reason.
// Open strokes go back to the plain raw-distance test (reversal must be
// justified by actual geometry); closed loops keep the rotation-fit
// criterion — there the cyclic candidates are all the SAME loop retraced,
// spin-vs-shift ambiguity is real, and it's what fixed the 90°-rectangle
// and rotated-start-star cases.
function alignResampledPairJS(a,b){
  var closed=resampledIsClosed(a)&&resampledIsClosed(b);
  var costFn=closed?rotationFitResidual:alignCost;
  var best=b,bestC=costFn(a,b);
  var rev=reverseResampled(b);
  var rc=costFn(a,rev);if(rc<bestC){bestC=rc;best=rev;}
  if(closed){
    [b,rev].forEach(function(base){
      var n=base.segments.length;
      for(var k=1;k<n;k++){
        var cand=rotateResampled(base,k);
        var c=costFn(a,cand);
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
// Manual counterpart to resolveSplitMatches' own tryDirection('B') piece-
// cutting (same extractStrokePiece-at-cumulative-length-fractions recipe),
// but for a USER-CONFIRMED split rather than a heuristic guess — no score
// gating needed, the user already knows these parts belong together (the
// "Réparer le tween" tool, below). Splits `mergedData` into
// partsData.length pieces, ordered by where each part projects onto
// mergedData's own path (not partsData's array order — the user may click
// the pieces in either order), each piece's length proportional to its
// corresponding part's own length.
function splitMergedIntoOrderedPieces(mergedData,partsData){
  var feats=partsData.map(strokeFeat);
  var sumLen=0;feats.forEach(function(f){sumLen+=f.length;});
  var mp=buildTPFeat(mergedData);
  var withLoc=partsData.map(function(pd,i){
    var f=feats[i];
    var loc=mp.getNearestLocation(new Point(f.cx,f.cy));
    return{part:pd,len:f.length,off:loc?loc.offset:i};
  }).sort(function(x,y){return x.off-y.off;});
  mp.remove();
  var fr=[],acc=0;
  for(var i=0;i<withLoc.length-1;i++){acc+=withLoc[i].len;fr.push(Math.min(0.95,Math.max(0.05,acc/Math.max(1,sumLen))));}
  var pieces=[],prevF=0;
  for(var i2=0;i2<withLoc.length;i2++){
    var f1=i2<fr.length?fr[i2]:1;
    pieces.push({part:withLoc[i2].part,piece:extractStrokePiece(mergedData,prevF,f1)});
    prevF=f1;
  }
  return pieces;
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
// Brush-texture dabs are RENDERING artifacts of their anchor stroke, not
// drawings in their own right — letting them into the matcher (as happened
// the moment presets landed) exploded the Hungarian matrix to hundreds of
// entries (O(n³)!), had dabs of one feature "matching" dabs of a completely
// different feature (the reported mouth-inbetweens-with-nose class of
// garbage), and interpolated each little ellipse independently, shredding
// the texture. Matching/interpolation now runs on ANCHORS ONLY; the dabs of
// a generated inbetween are re-stamped fresh along the interpolated
// centerline (dabRecordsForTween below) with a per-pair SEEDED rng so the
// texture sticks to the morphing stroke instead of re-rolling (boiling)
// on every frame.
// manualMode (2026-07, "tween seulement des éléments select... avec le clic
// droit sur les éléments select" — the RIGHT direction after an inverted
// first attempt): a keyframe pair is normally fully-automatic (every
// non-dab stroke participates in autoMatch, matching this function's
// behavior for years). manualMode flips that for ONE specific pair
// (ld.frames[fA].tweenManualMode, set by select-bridge.js's
// toggleTweenOnForSelection the first time the artist opts an element IN)
// — only strokes flagged data.tweenOn/sd.tweenOn via that right-click
// action participate; everything else on that pair is held static instead
// of auto-matching, letting several elements of the same keyframe be
// tweened or not independently. manualMode is per-pair and self-reverting
// (toggleTweenOnForSelection turns it back off once no stroke in the frame
// is flagged anymore) — every OTHER keyframe pair in the project, and this
// function's behavior for anyone who never uses the feature, is completely
// unchanged.
function splitTweenables(strokes,manualMode){
  var list=[],orig=[],dabsByGroup={},held=[];
  strokes.forEach(function(sd,i){
    if(sd.isBrushTextureCopy){
      if(sd.brushGroupId)(dabsByGroup[sd.brushGroupId]=dabsByGroup[sd.brushGroupId]||[]).push(sd);
      return;
    }
    // A held stroke is copied UNCHANGED into every generated inbetween
    // (generateTweens' emit loop) instead of participating in
    // autoMatch/interpStroke at all — dabs above are re-stamped fresh each
    // frame instead, a different mechanism for a different reason.
    if(manualMode&&!sd.tweenOn){held.push(sd);return;}
    list.push(sd);orig.push(i);
  });
  return{list:list,orig:orig,dabsByGroup:dabsByGroup,held:held};
}
function dabRecordsForTween(rec,presetKey,colorHexStr,baseWidth,seed,opacityMul){
  var preset=resolveBrushPreset(presetKey);
  if(!preset)return[];
  var p=new Path({insert:false});
  rec.segments.forEach(function(s){p.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
  if(rec.closed)p.closed=true;
  var dabs=buildBrushDabs(p,preset,baseWidth||3,seededRng(seed));
  p.remove();
  return dabs.map(function(dab){
    var segs=dab.segments.map(function(s){return{point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};});
    var op=dab.data.dabOpacity*(opacityMul!==undefined?opacityMul:1);
    dab.remove();
    return{segments:segs,closed:true,strokeColor:null,fillColor:colorHexStr,opacity:op,isBrushTextureCopy:true,brushGroupId:rec.brushGroupId};
  });
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
  var resN=state.resamplePts;var step=state.tweenStep;var total=0;
  for(var ki=0;ki<keys.length-1;ki++){
    var fA=keys[ki],fB=keys[ki+1];
    if(restrictTo&&!restrictTo[fA])continue;
    // Per-pair override (complements the global curve, see getEasingForPair)
    var easFn=getEasingForPair(li,fA,fB);
    var manualMode=!!ld.frames[fA].tweenManualMode;
    var sAsplit=splitTweenables(ld.frames[fA].strokes,manualMode),sBsplit=splitTweenables(ld.frames[fB].strokes,manualMode);
    var sA=sAsplit.list,sB=sBsplit.list;
    // v16: manual pairing overrides (state.tweenOverrides) take priority
    // over autoMatch for this specific keyframe pair — resolved here by
    // stable strokeId, since sA/sB index order isn't stable across edits.
    // A stroke removed since the override was made just makes that one
    // override silently inert (falls through to auto-matching again),
    // rather than erroring the whole tween generation.
    var ovKey=li+':'+fA+'-'+fB;
    var overrides=(state.tweenOverrides&&state.tweenOverrides[ovKey])||[];
    var forcedAIdx={},forcedBIdx={},forcedPairs=[];
    overrides.forEach(function(ov){
      // Multi-source override (Réparer le tween, tw-reassign — a single
      // artist-intended line saved as 2+ separate strokes, e.g. "trait uni
      // qui se mélange avec les cheveux"): ov.aIds is an array instead of
      // the plain ov.aId. Split the ONE target B stroke into that many
      // pieces (splitMergedIntoOrderedPieces — same recipe as
      // resolveSplitMatches' own automatic piece-cutting, just user-
      // confirmed instead of heuristically detected) so each selected A
      // stroke gets its own correctly-ordered slice of B to morph into,
      // instead of every A candidate competing for the whole B stroke.
      if(ov.aIds){
        var aIdxs=[];
        ov.aIds.forEach(function(id){for(var ii=0;ii<sA.length;ii++)if(sA[ii].strokeId===id&&aIdxs.indexOf(ii)<0){aIdxs.push(ii);break;}});
        var bIdx0=-1;for(var jj=0;jj<sB.length;jj++)if(sB[jj].strokeId===ov.bId){bIdx0=jj;break;}
        if(!aIdxs.length||bIdx0<0||forcedBIdx[bIdx0])return;
        aIdxs=aIdxs.filter(function(ai){return!forcedAIdx[ai];});
        if(!aIdxs.length)return;
        forcedBIdx[bIdx0]=1;
        if(aIdxs.length===1){
          forcedAIdx[aIdxs[0]]=1;
          forcedPairs.push({aIdx:aIdxs[0],bIdx:bIdx0,aData:sA[aIdxs[0]],bData:sB[bIdx0],mi:-1-forcedPairs.length,score:0,forced:true});
          return;
        }
        var pieces=splitMergedIntoOrderedPieces(sB[bIdx0],aIdxs.map(function(ai){return sA[ai];}));
        pieces.forEach(function(pc){
          var ai=sA.indexOf(pc.part);
          forcedAIdx[ai]=1;
          forcedPairs.push({aIdx:ai,bIdx:bIdx0,aData:sA[ai],bData:pc.piece,mi:-1-forcedPairs.length,score:0,forced:true,isPiece:true});
        });
        return;
      }
      var aIdx=-1,bIdx=-1;
      // Check BOTH stored ids on BOTH sides, not just ov.aId on A / ov.bId
      // on B — found live testing tween-arc handles (any drag re-triggers
      // generateTweens): the very first successful resolution of this
      // override already stamps the SAME shared pairId onto both
      // spec.aData AND spec.bData a few lines below (splitTweenables
      // doesn't clone, so this mutates the real keyframe stroke data) —
      // meaning ov.bId (B's ORIGINAL id) no longer exists anywhere the
      // instant after the first call. Every regeneration after that first
      // one silently failed this lookup, fell through to autoMatch, and —
      // for exactly the pairs that needed a forced override because they
      // don't auto-match well (dissimilar/far apart) — got treated as a
      // fade-out+fade-in instead, ADDING that content into frames that
      // already held the correctly-tweened result from the first pass
      // (confirmed: strokes.length 1->2 on a plain second call, no other
      // change). After a merge, A and B share one identical id, so
      // matching either stored id against either side is safe pre- and
      // post-merge: pre-merge it degrades to exactly the old aId-on-A/
      // bId-on-B check (the ids differ, so the OR's second half never
      // matches); post-merge both sides already carry the same value, so
      // either id resolves both.
      for(var ii=0;ii<sA.length;ii++)if(sA[ii].strokeId===ov.aId||sA[ii].strokeId===ov.bId){aIdx=ii;break;}
      for(var jj=0;jj<sB.length;jj++)if(sB[jj].strokeId===ov.bId||sB[jj].strokeId===ov.aId){bIdx=jj;break;}
      if(aIdx<0||bIdx<0||forcedAIdx[aIdx]||forcedBIdx[bIdx])return;
      forcedAIdx[aIdx]=1;forcedBIdx[bIdx]=1;
      forcedPairs.push({aIdx:aIdx,bIdx:bIdx,aData:sA[aIdx],bData:sB[bIdx],mi:-1-forcedPairs.length,score:0,forced:true});
    });
    // Pre-existing bug found by stress-testing (2026-07-17): this used to
    // `continue` whenever autoMatch returned ZERO pairs (and no forced
    // overrides) — i.e. exactly when the two keyframes' drawings are so
    // different that the augmented Hungarian sent EVERY stroke to a fade
    // dummy (a full cut-away: small shape top-left key A, unrelated big
    // shape bottom-right key B). Skipping the span meant NO inbetweens at
    // all were generated — not even the cross-fade that unmatched strokes
    // are supposed to get — silently leaving whatever frames were there
    // before. Only skip when there is genuinely nothing to tween on either
    // side; zero matches with real strokes still flows through so the
    // fade-out/fade-in machinery below does its job.
    var matches=autoMatch(sA,sB);if(!sA.length&&!sB.length&&!forcedPairs.length)continue;
    // Only morph plausible pairs. A stroke whose best assignment still
    // scores badly (no real counterpart in the other key — count mismatch,
    // or a shape that genuinely appears/disappears) cross-fades in place
    // instead of scaling/warping toward an unrelated stroke.
    var MATCH_TH=0.48;
    // Bug found by stress-testing (2026-07-17): matchSc's dominant terms
    // (proxT 0.48 + alignT 0.15) are ABSOLUTE-position Chamfer/ordered
    // distance, normalized by the strokes' own size — so a single shape
    // that simply moves far between two keys (a thrown ball, a fast pan,
    // any large but perfectly ordinary motion) climbs past MATCH_TH purely
    // from distance, with NOTHING else about it changed, and gets treated
    // as an "appear/disappear" cross-fade instead of an interpolated move.
    // Confirmed empirically: identical circles moving ~4.5x their own
    // diameter already exceed 0.48, scale-invariantly (same ratio at every
    // tested radius) — a very ordinary distance for anime action, not an
    // edge case. Generalized (same session, character-pose test): a raised
    // ARM — one long stroke pivoting ~90° around its shoulder with some
    // foreshortening, THE textbook limb motion — scored 0.739 while the
    // character's other 5 strokes all matched confidently, so the arm
    // cross-faded while the rest of the body moved. The Hungarian had
    // paired arm↔arm; only the threshold vetoed it. Whenever a rejected
    // Hungarian pair's two members are BOTH still unmatched after the
    // threshold pass (in the 1-stroke-per-frame case that's trivially
    // true — the original soleCandidates form of this fix), there is no
    // competing candidate the position score could be protecting: fading
    // is strictly worse than following the assignment the global optimum
    // already chose — traditional inbetweening prefers motion over a
    // dissolve whenever the strokes are plausibly the same object. "Same
    // object" is gated on the identity signals (type, open/closed, and no
    // hard color clash — same test as matchSc's colorPenalty), NOT on
    // distance. Position keeps its full weight wherever 2+ candidates
    // actually compete.
    var pairSpecs=[],aMatched={},bMatched={};
    forcedPairs.forEach(function(fp){pairSpecs.push(fp);aMatched[fp.aIdx]=1;bMatched[fp.bIdx]=1;});
    matches.forEach(function(m){
      if(forcedAIdx[m.a]||forcedBIdx[m.b])return; // conflicts with a manual override — drop the auto guess
      if(m.score<=MATCH_TH){pairSpecs.push({aIdx:m.a,bIdx:m.b,aData:sA[m.a],bData:sB[m.b],mi:matches.indexOf(m),score:m.score});aMatched[m.a]=1;bMatched[m.b]=1;}
    });
    // Second chance for mutually-leftover Hungarian pairs (see comment above).
    matches.forEach(function(m){
      if(m.score<=MATCH_TH)return; // already handled by the first pass
      if(forcedAIdx[m.a]||forcedBIdx[m.b])return;
      if(aMatched[m.a]||bMatched[m.b])return; // one side already claimed — real ambiguity, let it fade
      var fta=strokeFeat(sA[m.a]),ftb=strokeFeat(sB[m.b]);
      var clash=(fta.fillCol&&ftb.fillCol&&colorDist(fta.fillCol,ftb.fillCol)>0.35)||(fta.strokeCol&&ftb.strokeCol&&colorDist(fta.strokeCol,ftb.strokeCol)>0.35);
      if(fta.type===ftb.type&&fta.closed===ftb.closed&&!clash){
        pairSpecs.push({aIdx:m.a,bIdx:m.b,aData:sA[m.a],bData:sB[m.b],mi:matches.indexOf(m),score:m.score});aMatched[m.a]=1;bMatched[m.b]=1;
      }
    });
    var unA=[],unB=[];
    for(var ai=0;ai<sA.length;ai++)if(!aMatched[ai])unA.push(ai);
    for(var bi2=0;bi2<sB.length;bi2++)if(!bMatched[bi2])unB.push(bi2);
    // N:1 rescue pass — may convert fades + a mediocre pair into clean
    // piece-wise morphs by splitting a merged stroke (see resolveSplitMatches)
    if(unA.length||unB.length)resolveSplitMatches(sA,sB,pairSpecs,unA,unB);
    var fadeOutA=unA.map(function(i){return sA[i];}),fadeInB=unB.map(function(i){return sB[i];});
    // Same identity-continuity fix as the matched pairs above (see that
    // comment): a solo fading stroke's id-less keyframe would otherwise
    // be a different identity from its own generated fade frames.
    fadeOutA.forEach(function(sd,i2){if(!sd.strokeId)sd.strokeId='twf_'+fA+'_a'+i2;});
    fadeInB.forEach(function(sd,i2){if(!sd.strokeId)sd.strokeId='twf_'+fB+'_b'+i2;});
    if(!pairSpecs.length&&!fadeOutA.length&&!fadeInB.length)continue;
    // ---- OCCLUSION: stacking order interpolated from real authored data ----
    // The z-order (draw/stack order, front-to-back) of a generated inbetween
    // used to be frozen on whatever order pairSpecs happened to iterate in —
    // effectively frame A's own stacking for the WHOLE span. If the artist
    // deliberately restacked two elements between the keyframes (an arm
    // drawn BEHIND the torso in A, but drawn IN FRONT of it in B — a common,
    // intentional way to indicate the arm swinging to the near side), the
    // frozen order meant the whole tween stayed on A's stacking then POPPED
    // to B's at the very last frame, instead of crossing at a sensible point.
    // A true depth-aware solution (recomputing which surface should occlude
    // which via boolean geometry every generated frame) has no principled
    // answer for 2D vector art — flat strokes carry no inherent depth, only
    // the artist's own draw order does, so there's nothing for booleans to
    // resolve that isn't already expressed by that order. This uses exactly
    // that real, authored signal instead: each matched pair's stacking RANK
    // (its position within A's/B's own stroke array, 0=bottom..1=top) is
    // interpolated the same way its shape and easing already are, and the
    // whole `tw` array is re-sorted by that interpolated rank every
    // generated frame — so a restack between keyframes crosses smoothly
    // in-between (typically right around the shape's own halfway point)
    // instead of freezing on A then popping to B on the last frame.
    var pairs=pairSpecs.map(function(spec){
      var isVB=!!(spec.aData.isVectorBrush&&spec.bData.isVectorBrush);
      // Feature-aware (corner/cusp-biased) shared resampling — see
      // resamplePairFeatureAware's own comment. Falls back internally to
      // the old independent uniform resampleP/resampleCenterline for
      // degenerate inputs (very low resN, missing centerline data, etc.).
      var rpair=resamplePairFeatureAware(spec.aData,spec.bData,resN,isVB);
      var ra=rpair[0],rb=rpair[1];
      // Brush-texture metadata for re-stamping the dabs on every generated
      // frame (see splitTweenables' comment). texSide tracks WHICH keyframe
      // is textured so a textured→plain morph fades the texture out rather
      // than double-drawing it over the fading-in plain stroke.
      var texA=spec.aData.brushTexturePreset,texB=spec.bData.brushTexturePreset;
      var tex=null;
      if(texA||texB){
        tex={
          preset:texA||texB,
          side:texA&&texB?'both':(texA?'a':'b'),
          color:spec.aData.preTextureStroke||spec.aData.strokeColor||spec.bData.preTextureStroke||spec.bData.strokeColor||'#000000',
          groupId:spec.aData.brushGroupId||spec.bData.brushGroupId,
          seed:(fA*7919+spec.mi*131+1)>>>0,
        };
      }
      // Bitmap Brush's own tween record (bitmap-brush.js's recordForTween,
      // mirroring dabRecordsForTween just above) — a bitmap-brush anchor
      // stores its resolved spec (tip/size/spacing/scatter/opacity/color,
      // already a concrete color string, not the camouflaged null
      // strokeColor) directly on .data.bitmapBrushSpec at creation, so no
      // preTextureStroke lookup needed here, unlike the vector case.
      var bmpA=spec.aData.bitmapBrushSpec,bmpB=spec.bData.bitmapBrushSpec;
      var bmpTex=null;
      if(bmpA||bmpB){
        bmpTex={
          spec:bmpA||bmpB,
          side:bmpA&&bmpB?'both':(bmpA?'a':'b'),
          groupId:spec.aData.brushGroupId||spec.bData.brushGroupId,
          seed:(fA*7919+spec.mi*131+1)>>>0,
        };
      }
      // Stable identity for every frame this pair generates. Ordinary
      // hand-drawn strokes usually have NO strokeId at all — it's
      // assigned lazily only for fill-wall/review purposes (tools.js'
      // ensureStrokeId) — so a naive "read spec.aData.strokeId, else
      // synthesize one for the interpolated frames only" left the id-less
      // KEYFRAME endpoints in a different identity than the interpolated
      // middle, breaking continuity exactly at the tween boundary (each
      // keyframe becomes an isolated 1-frame shape, the interpolated span
      // a separate disconnected one). Live-caught 2026-07 twice: first
      // "les inbetween sont mal timés" (position-based matching), then
      // after fixing that, "je ne vois que 3 images" (id-based matching,
      // but ids didn't reach the keyframes). Fixed by STAMPING the
      // resolved id back onto spec.aData/bData — splitTweenables doesn't
      // clone, so these are the live objects sitting in
      // ld.frames[fA/fB].strokes, and the assignment persists into the
      // keyframe's own stored data, giving keyframe and every
      // interpolated frame in between the exact same identity.
      var pairId=spec.aData.strokeId||spec.bData.strokeId||('tw_'+fA+'_'+spec.mi);
      spec.aData.strokeId=pairId;spec.bData.strokeId=pairId;
      return{a:ra,b:alignResampledPair(ra,rb),mi:spec.mi,tex:tex,bmpTex:bmpTex,id:pairId,
        aRank:spec.aIdx/Math.max(1,sA.length-1),bRank:spec.bIdx/Math.max(1,sB.length-1)};
    });
    var gap=fB-fA;
    for(var fi=fA+1;fi<fB;fi++){
      // A frame flagged isManualEdit was hand-corrected after a previous
      // tween — leave it exactly as the artist left it unless they've
      // explicitly unchecked "skip manually-edited frames".
      if(state.tweenSkipManual&&ld.frames[fi].isManualEdit)continue;
      if(step>1&&(fi-fA)%step!==0){ld.frames[fi]={strokes:[],isInterpolated:false,isKeyframe:false};continue;}
      var t=(fi-fA)/gap;var et2=easFn(t);
      var tw=[];
      pairs.forEach(function(pr){
        var sdOut=interpStroke(pr.a,pr.b,t,easFn,fA,fB,pr.mi);
        sdOut.strokeId=pr.id; // stable per-pair identity (see pairs construction)
        sdOut.__zKey=lerp(pr.aRank,pr.bRank,et2);
        if(pr.tex){
          // carry the anchor's texture identity so a later manual edit of
          // this frame keeps behaving as a textured group
          sdOut.brushTexturePreset=pr.tex.preset;
          if(pr.tex.groupId)sdOut.brushGroupId=pr.tex.groupId;
          var mul=pr.tex.side==='a'?(1-et2):pr.tex.side==='b'?et2:1;
          if(mul>0.02){
            // +1e-4 (not -1e-4): the live/non-tween equivalent always
            // inserts the texture ABOVE its anchor (dab.insertAbove(basePath),
            // tools.js) so it draws OVER the anchor's own fill — a lower
            // __zKey here sorted the dabs BEHIND the anchor instead, so a
            // generated tween in-between showed the anchor's fill painted
            // over its own brush texture. Found live (2026-07, screenshot:
            // "lors des tween le fill passe devant le stroke avec les brush
            // bitmap") — same bug affected the vector-dab path here, the
            // bitmap-raster path just below, and the fading-dabs path
            // further down (pushFade) — all three fixed together.
            dabRecordsForTween(sdOut,pr.tex.preset,pr.tex.color,sdOut.strokeWidth||3,pr.tex.seed,mul)
              .forEach(function(dr){dr.__zKey=sdOut.__zKey+1e-4;tw.push(dr);});
          }
        }
        if(pr.bmpTex&&window.SMBitmapBrush){
          // carry the identity so a later node edit on this generated
          // frame still resolves through regenerateBrushTexture's bitmap
          // branch (tools.js) instead of falling through as an untextured
          // plain anchor.
          sdOut.bitmapBrushSpec=pr.bmpTex.spec;
          if(pr.bmpTex.groupId)sdOut.brushGroupId=pr.bmpTex.groupId;
          var bmul=pr.bmpTex.side==='a'?(1-et2):pr.bmpTex.side==='b'?et2:1;
          if(bmul>0.02){
            var specForFrame={tip:pr.bmpTex.spec.tip,size:pr.bmpTex.spec.size,spacing:pr.bmpTex.spec.spacing,scatter:pr.bmpTex.spec.scatter,opacity:pr.bmpTex.spec.opacity,color:pr.bmpTex.spec.color,seed:pr.bmpTex.seed};
            var brec=SMBitmapBrush.recordForTween(sdOut.segments,sdOut.closed,specForFrame,bmul,pr.bmpTex.groupId);
            if(brec){brec.__zKey=sdOut.__zKey+1e-4;tw.push(brec);} // see +1e-4 comment above (vector-dab branch)
          }
        }
        tw.push(sdOut);
      });
      // Fading strokes have no counterpart to interpolate a rank toward —
      // they keep their own frame's rank fixed for the whole span (a
      // disappearing element stays wherever it was stacked in A; an
      // appearing one stays wherever it'll be stacked in B). A textured
      // fader's own record may be invisible (opacity-0 anchor) — its dabs
      // (cloned from the source frame, opacity-scaled) are what fades.
      function pushFade(sd,rank,mul,dabsByGroup){
        var c=JSON.parse(JSON.stringify(sd));c.opacity=(c.opacity!==undefined?c.opacity:1)*mul;c.__zKey=rank;
        if(c.opacity>0.02)tw.push(c);
        var grp=sd.brushTexturePreset&&sd.brushGroupId&&dabsByGroup[sd.brushGroupId];
        if(grp)grp.forEach(function(d){
          var dc=JSON.parse(JSON.stringify(d));dc.opacity=(dc.opacity!==undefined?dc.opacity:1)*mul;dc.__zKey=rank+1e-4; // see +1e-4 comment above (vector-dab branch)
          if(dc.opacity>0.02)tw.push(dc);
        });
      }
      fadeOutA.forEach(function(sd,fi2){pushFade(sd,unA[fi2]/Math.max(1,sA.length-1),1-et2,sAsplit.dabsByGroup);});
      fadeInB.forEach(function(sd,fi2){pushFade(sd,unB[fi2]/Math.max(1,sB.length-1),et2,sBsplit.dabsByGroup);});
      // Per-element manual tween mode (2026-07): when this pair is in
      // manual mode (ld.frames[fA].tweenManualMode), any stroke NOT
      // flagged data.tweenOn is held — copied UNCHANGED into every
      // interpolated frame, at full opacity, instead of being matched/
      // interpolated. Only sAsplit.held is used, not sBsplit.held —
      // toggleTweenOnForSelection (select-bridge.js) propagates the SAME
      // flag to both keyframe A and B's copy of a strokeId specifically so
      // sBsplit already excludes an unflagged stroke from B's OWN matcher
      // (no bogus fade-in), NOT so both sides get emitted here — emitting
      // both would draw two overlapping copies of the same shape.
      sAsplit.held.forEach(function(sd,hi){
        var c=JSON.parse(JSON.stringify(sd));c.__zKey=hi/Math.max(1,sAsplit.held.length-1);
        tw.push(c);
      });
      tw.sort(function(x,y){return x.__zKey-y.__zKey;});
      tw.forEach(function(s){delete s.__zKey;});
      ld.frames[fi]={strokes:tw,isInterpolated:true,isKeyframe:false};total++;
    }
  }
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();showToast(total+' inbetween(s) générés');
}

// ---- ARCS ----
var arcHandles=[],draggingArc=null;
// Split out of renderArcs so a caller dragging a single arc handle can
// compute this ONCE at drag-start and reuse it on every pointermove instead
// of re-running splitTweenables+autoMatch (documented O(n³) Hungarian) on
// every mouse pixel — perf bug found 2026-07: dragging a handle called the
// full renderArcs() (recompute + rebuild) per move event, unlike every
// other continuous-drag path in this app, which defers expensive work.
// Only the dragged handle's OWN position changes during a drag; which
// strokes match which never does (that's re-decided by generateTweens()
// on drag-end, mode==='arc' in select-bridge.js/tools.js).
function computeArcMatchState(){
  if(state.tool!=='select'||!state.selectedStrokeIndices.length)return null;
  var li=state.activeLayerIdx;var ld=state.layers[li];
  var keys=[];for(var i=0;i<state.totalFrames;i++){if(ld.frames[i].isKeyframe&&ld.frames[i].strokes.length>0)keys.push(i);}
  if(keys.length<2)return null;
  var fA=-1,fB=-1;for(var i2=0;i2<keys.length-1;i2++){if(state.currentFrame>=keys[i2]&&state.currentFrame<=keys[i2+1]){fA=keys[i2];fB=keys[i2+1];break;}}
  if(fA<0)return null;
  var manualMode=!!ld.frames[fA].tweenManualMode;
  var spA=splitTweenables(ld.frames[fA].strokes,manualMode),spB=splitTweenables(ld.frames[fB].strokes,manualMode);
  var sA=spA.list,sB=spB.list;
  // v16: replicate generateTweens' forced-pair resolution (state.tweenOverrides)
  // here so a forced pair gets the SAME negative matchIdx in both places —
  // otherwise the handle rendered/dragged here writes to a different
  // state.motionArcs key (arcKey()) than interpStroke() reads at generation
  // time, so dragging it has zero effect on the actual tween. Found live
  // 2026-07: dragged both handles of a forced pair substantially, motionArcs
  // held real offsets, but the in-between frame's content never moved.
  var ovKey=li+':'+fA+'-'+fB;
  var overrides=(state.tweenOverrides&&state.tweenOverrides[ovKey])||[];
  var forcedAIdx={},forcedBIdx={},forcedPairs=[];
  overrides.forEach(function(ov){
    var aIdx=-1,bIdx=-1;
    for(var ii=0;ii<sA.length;ii++)if(sA[ii].strokeId===ov.aId||sA[ii].strokeId===ov.bId){aIdx=ii;break;}
    for(var jj=0;jj<sB.length;jj++)if(sB[jj].strokeId===ov.bId||sB[jj].strokeId===ov.aId){bIdx=jj;break;}
    if(aIdx<0||bIdx<0||forcedAIdx[aIdx]||forcedBIdx[bIdx])return;
    forcedAIdx[aIdx]=1;forcedBIdx[bIdx]=1;
    forcedPairs.push({a:aIdx,b:bIdx,mi:-1-forcedPairs.length});
  });
  var matches=autoMatch(sA,sB);if(!matches.length&&!forcedPairs.length)return null;
  var sel=state.selectedStrokeIndices;var fm=[];
  forcedPairs.forEach(function(fp){if(sel.indexOf(spA.orig[fp.a])>=0)fm.push(fp);});
  matches.forEach(function(m,idx){
    if(forcedAIdx[m.a]||forcedBIdx[m.b])return; // superseded by a forced override, same as generateTweens
    if(sel.indexOf(spA.orig[m.a])>=0)fm.push({a:m.a,b:m.b,mi:idx});
  });
  if(!fm.length)return null;
  return {fA:fA,fB:fB,sA:sA,sB:sB,matches:matches,fm:fm};
}
function renderArcs(cached){
  updateReassignBadge();
  arcLayer.removeChildren();arcHandles=[];
  renderNodeHandles();
  renderTransformHandles();
  // Same anchors-only filtering as generateTweens (see splitTweenables'
  // comment) — this runs on every selection render, so dab pollution here
  // was ALSO an O(n³) Hungarian on hundreds of entries per click. m.a
  // indexes the filtered list; selectedStrokeIndices index the raw frame
  // array — map back through .orig for the selection check.
  var st=cached||computeArcMatchState();
  if(!st)return;
  var fA=st.fA,fB=st.fB,sA=st.sA,sB=st.sB,matches=st.matches,fm=st.fm;
  arcLayer.activate();var cols=['#ff6b6b','#4ecdc4','#ffe66d','#a29bfe','#fd79a8','#00cec9'];var easFn=getEasingForPair(state.activeLayerIdx,fA,fB);
  // A multi-element selection used to draw every arc at full brightness/
  // width in its own cycling color — with more than a handful selected the
  // dashed lines and endpoint dots pile on top of each other into an
  // unreadable tangle ("plusieurs lignes de trajectoire qui se superposent,
  // c'est illisible"). Past one element, fall back to a single neutral,
  // faint style for all of them: still one real arc + one real draggable
  // handle per matched pair (each tween keeps its own independently
  // adjustable easing — collapsing them into one path would lose that), just
  // rendered so the OVERALL motion reads as one soft bundle instead of a
  // rainbow pileup. Single-element selections (by far the most common case
  // for fine-tuning one curve) keep the original full-contrast styling.
  var multi=fm.length>1;
  fm.forEach(function(m,di){
    var pA=buildTP(sA[m.a]),pB=buildTP(sB[m.b]);var cA=pA.bounds.center,cB=pB.bounds.center;pA.remove();pB.remove();
    var mIdx=m.mi;var ah=getArcHandles(fA,fB,mIdx,[cA.x,cA.y],[cB.x,cB.y]);var col=multi?'#ffffff':cols[di%cols.length];var zs=1/view.zoom;
    var ap=new Path({insert:true});ap.strokeColor=new Color(col);ap.strokeColor.alpha=multi?.3:.6;ap.strokeWidth=(multi?1:2)*zs;ap.dashArray=[6*zs,4*zs];
    for(var s=0;s<=24;s++){var t=s/24;var x=cubicBez(cA.x,ah.out[0],ah.in[0],cB.x,t);var y=cubicBez(cA.y,ah.out[1],ah.in[1],cB.y,t);if(s===0)ap.moveTo(new Point(x,y));else ap.lineTo(new Point(x,y));}
    new Path.Circle({center:cA,radius:(multi?2.5:4)*zs,insert:true,fillColor:col,opacity:multi?.45:.8});
    new Path.Circle({center:cB,radius:(multi?2.5:4)*zs,insert:true,fillColor:col,opacity:multi?.45:.8});
    // Independent OUT (from A) / IN (to B) handles — camera.js's exact rig,
    // not a single shared knob (live feedback 2026-07). Each gets its own
    // thin connector line (same convention camera.js/motion.js use) so
    // it's visually obvious which endpoint a given handle belongs to.
    var outLine=new Path({insert:true,segments:[cA,new Point(ah.out[0],ah.out[1])]});outLine.strokeColor=new Color(col);outLine.strokeColor.alpha=multi?.25:.5;outLine.strokeWidth=(multi?.8:1)*zs;
    var hOut=new Path.Circle({center:new Point(ah.out[0],ah.out[1]),radius:(multi?3:6)*zs,insert:true});hOut.fillColor=new Color(1,1,1,multi?.5:.95);hOut.strokeColor=col;hOut.strokeWidth=(multi?1:2)*zs;
    var inLine=new Path({insert:true,segments:[cB,new Point(ah.in[0],ah.in[1])]});inLine.strokeColor=new Color(col);inLine.strokeColor.alpha=multi?.25:.5;inLine.strokeWidth=(multi?.8:1)*zs;
    var hIn=new Path.Circle({center:new Point(ah.in[0],ah.in[1]),radius:(multi?3:6)*zs,insert:true});hIn.fillColor=new Color(1,1,1,multi?.5:.95);hIn.strokeColor=col;hIn.strokeWidth=(multi?1:2)*zs;
    arcHandles.push({handle:hOut,which:'out',fA:fA,fB:fB,matchIdx:mIdx,ptA:[cA.x,cA.y],ptB:[cB.x,cB.y]});
    arcHandles.push({handle:hIn,which:'in',fA:fA,fB:fB,matchIdx:mIdx,ptA:[cA.x,cA.y],ptB:[cB.x,cB.y]});
  });
  userLayers[state.activeLayerIdx].activate();
}

// ---- GHOST ALL KEYFRAMES ----
// Onion skin only shows the neighboring frames either side of the playhead
// — this shows EVERY keyframe of the active layer at once (a purple-tinted
// ghost per keyframe, current frame excluded since its real content is
// already on screen), a quick way to see the whole layer's timing/spacing
// in one view. Purely visual; selectGhostAll() below is the separate step
// that turns these into real, editable, per-frame-tagged proxy objects.
function renderGhostAll(){
  ghostAllLayer.removeChildren();
  if(!state.ghostAllFrames)return;
  var li=state.activeLayerIdx,cf=state.currentFrame;
  var ld=state.layers[li];if(!ld||ld.symbolId||ld.nativeVideo||ld.montageId)return;
  for(var fi=0;fi<ld.frames.length;fi++){
    if(fi===cf)continue;
    var fr=ld.frames[fi];if(!fr.isKeyframe||!fr.strokes.length)continue;
    var dist=Math.abs(fi-cf);
    var op=Math.max(.12,.4-dist*.03);
    fr.strokes.forEach(function(sd){
      if(sd.isRaster)return;
      var p=desP(sd,ghostAllLayer,op);
      p.strokeColor=new Color(.68,.6,1,op*1.6);
      p.fillColor=null;
      p.data.ghostFrame=fi;
    });
  }
  userLayers[li]&&userLayers[li].activate?userLayers[li].activate():null;
}

// ---- ONION ----
// Folded into the same function (rather than adding renderGhostAll() to
// every one of renderOS()'s many call sites across app.js/timeline.js) so
// every existing "state changed, recompute the ghost overlays" call site
// keeps working unmodified and picks up Ghost All for free.
function renderOS(){
  // toggleOnion()/setOnionMode()/setOnionRange()/toggleGhostAll() etc. all
  // just mutate state then call renderOS() — none of them ever bumped
  // window._sceneVersion, so engine-bridge.js's tick() (which skips
  // rebuilding the scene JSON entirely unless that counter or the viewport
  // changed) kept painting the PREVIOUS onion/ghost picture until some
  // unrelated action — typically the next frame navigation — happened to
  // bump the version for its own reasons. Toggling onion/ghost state IS a
  // real scene change and belongs in the same "things that dirty the
  // picture" bucket as everything else that already increments this here.
  window._sceneVersion++;
  renderGhostAll();
  onionPrevLayer.removeChildren();onionNextLayer.removeChildren();
  if(!state.onionSkin)return;var li=state.activeLayerIdx;var cf=state.currentFrame;
  // isRaster entries (imported image/video frames) go through desR, not
  // desP — a Raster has no fillColor/strokeColor, so tinted/outline modes
  // (which recolor the stroke) fall back to a plain opacity fade for it,
  // same as its normal on-canvas rendering just dimmer.
  // Brush-texture scaffolding (the invisible anchor behind the dabs,
  // isBrushTextureCopy dab stamps themselves) is skipped entirely here —
  // 'tinted'/'outline' onion modes unconditionally FORCE a visible
  // strokeColor onto every ghosted item, which bypassed the anchor's
  // opacity:0/strokeColor:null invisibility convention (applyBrushTexture,
  // tools.js) and showed it as a stray thin blue/red line running the
  // length of any textured stroke on an adjacent frame (reported: "filet
  // bleu derrière les brush avec preset de texture") — same "new tag
  // handled in one consumer (buildSceneJson/live render) but not another
  // (onion)" bug family documented in this repo's CLAUDE.md. The dabs
  // themselves are cheap, regenerated-per-frame stamps (not meaningful as a
  // traced reference), so onion just omits texture-preset strokes
  // altogether rather than trying to half-render them.
  // Falloff denominator scales with the ACTUAL configured onion range
  // (cf-onionIn / onionOut-cf), not a fixed ~5-frame distance — the old
  // `1-dist*.2` floored out (Math.max(.15,...)) by dist=4.25 regardless of
  // how far the user dragged the onion markers, so widening the marker to
  // reach a keyframe 11+ frames away kept adding real scene items (verified:
  // they DID reach the Rust engine, correctly matched frame content) but at
  // a flat ~4.5% final opacity (.15 floor × 30% base) — technically present,
  // practically invisible, reported as "le fantôme n'apparaît/disparaît pas
  // en conséquence" of resizing the markers. Now the .15 floor is reached
  // exactly at the FAR EDGE of whatever range the user picked: the farthest
  // frame in view stays visibly present, nearer frames are progressively
  // more opaque, and widening the range genuinely changes what's legible
  // instead of just adding more scene items nobody can see.
  var prevRangeSpan=Math.max(1,cf-state.onionIn);
  var nextRangeSpan=Math.max(1,state.onionOut-cf);
  // sd.hasRealStroke (serP, app.js) gates the manufactured tint/outline
  // stroke below — found live (2026-07-17, "il me met un trait bleu
  // derrière" on a selected Bitmap Brush stroke): a texture-camouflaged
  // anchor (Bitmap Brush or vector-preset) serializes strokeColor:null on
  // purpose (the texture companion IS its visible edge, itself correctly
  // excluded from onion ghosting via isBrushTextureCopy above), but this
  // code used to force `p.strokeColor = tint` onto EVERY ghost regardless,
  // resurrecting a solid colored outline nothing in the real artwork has.
  // Normally invisible (perfectly eclipsed by the live stroke sitting on
  // top of it), it becomes a visible halo the instant the live stroke's
  // geometry differs even slightly from the frozen ghost (e.g. mid-drag
  // width scrub, or simply a keyframe boundary the ghost doesn't share).
  // Falls back to the plain opacity-only treatment ('default' mode's own
  // branch) for these — there's no real vector stroke to tint or outline.
  // 2026-07 feedback ("l'onion skin outline only n'a pas l'air de marcher
  // pour toutes les frames"): the tinted/outline branches below were gated
  // on sd.hasRealStroke ALONE, so a genuine fill-only shape (Fill Brush
  // result, or a Draw-tool stroke with Stroke off) always fell to the
  // opacity-only 'else' — shown as a normal filled ghost even in "outline
  // only" mode, while plain-stroke items in the SAME frame correctly
  // outlined; across a scene with a mix of item types this reads as
  // "doesn't work for some frames". Widened to `sd.hasRealStroke||
  // sd.fillColor` — safe here specifically because brush-textured anchors
  // (the ONE case the halo-bug comment above warns about: fillColor set,
  // hasRealStroke false, same shape) are already filtered out by the
  // isBrushTextureCopy/brushTexturePreset return above, so anything
  // reaching this line with a fillColor and no real stroke is a genuine
  // plain fill shape, not a texture-camouflaged one.
  for(var fi=cf-1;fi>=state.onionIn&&fi>=0;fi--){var strokes=getEffectiveStrokes(li,fi);if(!strokes.length)continue;var dist=cf-fi;var op=(state.onionPrevOpacity/100)*Math.max(.15,1-(dist/prevRangeSpan)*.85);strokes.forEach(function(sd){if(sd.isBrushTextureCopy||sd.brushTexturePreset)return;if(sd.isRaster){var pr=desR(sd,onionPrevLayer);pr.opacity=op;return;}var p=desP(sd,onionPrevLayer,op);var canTint=sd.hasRealStroke||sd.fillColor;if(state.onionMode==='tinted'&&canTint)p.strokeColor=new Color(1,.3,.3,op);else if(state.onionMode==='outline'&&canTint){p.fillColor=null;p.strokeColor=new Color(1,.3,.3,op*.8);p.strokeWidth=1;}else p.opacity=op;});}
  for(var fi2=cf+1;fi2<=state.onionOut&&fi2<state.totalFrames;fi2++){var strokes2=getEffectiveStrokes(li,fi2);if(!strokes2.length)continue;var dist2=fi2-cf;var op2=(state.onionNextOpacity/100)*Math.max(.15,1-(dist2/nextRangeSpan)*.85);strokes2.forEach(function(sd){if(sd.isBrushTextureCopy||sd.brushTexturePreset)return;if(sd.isRaster){var nr=desR(sd,onionNextLayer);nr.opacity=op2;return;}var p=desP(sd,onionNextLayer,op2);var canTint2=sd.hasRealStroke||sd.fillColor;if(state.onionMode==='tinted'&&canTint2)p.strokeColor=new Color(.3,.55,1,op2);else if(state.onionMode==='outline'&&canTint2){p.fillColor=null;p.strokeColor=new Color(.3,.55,1,op2*.8);p.strokeWidth=1;}else p.opacity=op2;});}
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
function layersSnapshotNow(){return{type:'layers',layers:JSON.parse(JSON.stringify(state.layers)),active:state.activeLayerIdx,totalFrames:state.totalFrames,cameraKeys:JSON.parse(JSON.stringify(state.cameraKeys||[]))};}
// window._scrubLiveActive (ui.js, live drag-scrub des champs numériques) :
// pendant un drag de valeur, les handlers 'change' tournent à CHAQUE tick
// (reflet temps réel au canvas) et la plupart commencent par pushUndo — un
// snapshot par tick aurait pollué la pile pour un seul geste. ui.js pousse
// UN snapshot pré-geste au premier mouvement puis lève ce flag ; ici on
// no-op tant qu'il est levé (y compris le 'change' final du release).
function pushUndoLayers(){if(window._scrubLiveActive)return;saveAllLayerFrames();state.undoStack.push(layersSnapshotNow());if(state.undoStack.length>state.maxUndo)state.undoStack.shift();state.redoStack=[];if(window.SMFeedback)SMFeedback.logAction();}
function restoreLayersSnapshot(s){
  while(userLayers.length>0)userLayers.pop().remove();
  state.layers=[];
  s.layers.forEach(function(ld){var idx=createUserLayer(ld.name);state.layers[idx]=ld;});
  state.totalFrames=s.totalFrames;window._totalF=s.totalFrames;
  if(state.waOut>=s.totalFrames)state.waOut=s.totalFrames-1;window._waOut=state.waOut;
  if(state.currentFrame>=s.totalFrames)state.currentFrame=s.totalFrames-1;
  state.activeLayerIdx=Math.max(0,Math.min(s.active,state.layers.length-1));
  // Caméra (v19) : les clés caméra font partie du snapshot complet — un
  // undo après un drag de cadrage restaure le cadrage d'avant, pas
  // seulement les traits. Les snapshots antérieurs à v19 n'ont pas le
  // champ : on laisse alors les clés actuelles intactes (undefined check).
  if(s.cameraKeys!==undefined)state.cameraKeys=JSON.parse(JSON.stringify(s.cameraKeys));
  activateUL(state.activeLayerIdx);
  loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();
  if(window.SMCamera&&window.updateCameraPanel){updateCameraPanel();}
}
function undo(){if(!state.undoStack.length){showToast('Rien à annuler');return;}var s=state.undoStack.pop();
if(s.type==='layers'){state.redoStack.push(layersSnapshotNow());restoreLayersSnapshot(s);return;}
var cur={frame:state.currentFrame,layers:[]};for(var i=0;i<state.layers.length;i++){var f=state.layers[i].frames[state.currentFrame];cur.layers.push({strokes:JSON.parse(JSON.stringify(f.strokes)),isKeyframe:f.isKeyframe,isInterpolated:f.isInterpolated});}state.redoStack.push(cur);for(var i2=0;i2<s.layers.length&&i2<state.layers.length;i2++){var tf=state.layers[i2].frames[s.frame];tf.strokes=s.layers[i2].strokes;tf.isKeyframe=s.layers[i2].isKeyframe;tf.isInterpolated=s.layers[i2].isInterpolated;}if(s.frame!==state.currentFrame)state.currentFrame=s.frame;loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();}
function redo(){if(!state.redoStack.length){showToast('Rien à refaire');return;}var s=state.redoStack.pop();
if(s.type==='layers'){state.undoStack.push(layersSnapshotNow());restoreLayersSnapshot(s);return;}
var cur={frame:state.currentFrame,layers:[]};for(var i=0;i<state.layers.length;i++){var f=state.layers[i].frames[state.currentFrame];cur.layers.push({strokes:JSON.parse(JSON.stringify(f.strokes)),isKeyframe:f.isKeyframe,isInterpolated:f.isInterpolated});}state.undoStack.push(cur);for(var i2=0;i2<s.layers.length&&i2<state.layers.length;i2++){var tf=state.layers[i2].frames[s.frame];tf.strokes=s.layers[i2].strokes;tf.isKeyframe=s.layers[i2].isKeyframe;tf.isInterpolated=s.layers[i2].isInterpolated;}if(s.frame!==state.currentFrame)state.currentFrame=s.frame;loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();}

// ---- MANUAL INBETWEEN REASSIGNMENT (v16) ----
// autoMatch (top of this file) sometimes misidentifies correspondence when
// a shape changes too much between keyframes (size, stroke/fill change) —
// this lets the artist force ONE pairing by clicking the element on
// keyframe A then the corresponding element on keyframe B, persisted via
// stable data.strokeId into state.tweenOverrides and consumed by
// generateTweens() above. A 2-click guided flow, not a from-scratch cross-
// frame multi-select UI (nothing like that exists anywhere in the app yet
// per this session's own architecture research) — reuses ensureStrokeId
// (tools.js) and a simple capture-phase click intercept registered on
// `document`, same pattern ui.js already uses to steal a click before any
// per-tool bridge (draw/select/etc, all on #canvas-area) sees it.
// aIds (2026-07-18): a stroke drawn as one continuous line but saved as
// 2+ separate objects (pen lifted mid-line, or split by an earlier edit —
// "trait uni qui se mélange avec les cheveux") had no way to be pointed
// out as a UNIT: step 1 used to force exactly one A element. Now every
// click on keyframe A during step 1 ADDS to the set (instead of
// immediately advancing) — Enter confirms the set and moves to step 2,
// so a single click + Enter reproduces the old one-element flow exactly,
// and clicking 2-3 pieces first lets `generateTweens` split the ONE
// target B stroke to match, via splitMergedIntoOrderedPieces above.
var _reassign={active:false,step:0,layer:-1,frameA:-1,frameB:-1,aIds:[]};
function reassignStatusEl(){return document.getElementById('tw-reassign-status');}
function reassignSetStatus(msg){
  var el=reassignStatusEl();if(!el)return;
  if(msg){el.textContent=msg;el.style.display='block';}else el.style.display='none';
}
function cancelReassign(silent){
  if(!_reassign.active)return;
  _reassign.active=false;_reassign.step=0;_reassign.aIds=[];
  reassignSetStatus(null);
  hideReassignBadge();
  if(!silent)showToast('Réattribution annulée');
}
function startReassign(){
  if(_reassign.active){cancelReassign();return;}
  saveAllLayerFrames();
  var li=state.activeLayerIdx,ld=state.layers[li],cf=state.currentFrame;
  var fA=-1;
  for(var i=cf;i>=0;i--){if(ld.frames[i].isKeyframe){fA=i;break;}}
  if(fA<0){showToast('Placez le playhead sur une keyframe (ou après une keyframe)');return;}
  var fB=-1;
  for(var j=fA+1;j<state.totalFrames;j++){if(ld.frames[j].isKeyframe){fB=j;break;}}
  if(fB<0){showToast('Cette keyframe n\'a pas de keyframe suivante à réattribuer');return;}
  _reassign.active=true;_reassign.step=1;_reassign.layer=li;_reassign.frameA=fA;_reassign.frameB=fB;_reassign.aIds=[];
  reassignSetStatus('1/2 — Cliquez l\'élément de départ (plusieurs si un même trait a été séparé), puis Entrée');
  showToast('Cliquez l\'élément de départ sur le canvas');
}
function confirmReassignStep1(){
  if(!_reassign.active||_reassign.step!==1)return;
  if(!_reassign.aIds.length){showToast('Cliquez au moins un élément avant de valider');return;}
  saveActiveLayerFrame();
  _reassign.step=2;
  goToFrame(_reassign.frameB);
  reassignSetStatus('2/2 — Cliquez l\'élément correspondant sur la keyframe '+(_reassign.frameB+1));
  showToast('Cliquez l\'élément correspondant sur la keyframe suivante');
}
function reassignHandleClick(pt){
  if(!_reassign.active)return false;
  var li=_reassign.layer;
  if(li!==state.activeLayerIdx||state.currentFrame!==(_reassign.step===1?_reassign.frameA:_reassign.frameB)){
    // playhead/layer changed mid-flow — the click can't be trusted to be on
    // the intended keyframe's own content, safer to just cancel than guess
    cancelReassign();return false;
  }
  var layer=userLayers[li];
  var hit=layer.hitTest(pt,{stroke:true,fill:true,tolerance:8/view.zoom});
  if(!hit||!(hit.item instanceof Path)){showToast('Aucun élément à cet endroit');return true;}
  var target=resolveBrushAnchor(hit.item,layer);
  var sid=ensureStrokeId(target);
  if(_reassign.step===1){
    if(_reassign.aIds.indexOf(sid)<0)_reassign.aIds.push(sid);
    reassignSetStatus('1/2 — '+_reassign.aIds.length+' élément(s) sélectionné(s). Cliquez-en d\'autres si ce trait a été séparé, ou appuyez sur Entrée');
    return true;
  }
  // step 2
  saveActiveLayerFrame();
  var key=li+':'+_reassign.frameA+'-'+_reassign.frameB;
  var list=state.tweenOverrides[key]=state.tweenOverrides[key]||[];
  // replace any earlier override touching either side of this pair — one
  // strokeId can't sensibly be forced into two different correspondences
  // for the same keyframe pair
  state.tweenOverrides[key]=list.filter(function(ov){
    var ovA=ov.aIds||[ov.aId];
    return ovA.indexOf(_reassign.aIds[0])<0&&_reassign.aIds.every(function(id){return ovA.indexOf(id)<0;})&&ov.bId!==sid;
  });
  state.tweenOverrides[key].push(_reassign.aIds.length===1?{aId:_reassign.aIds[0],bId:sid}:{aIds:_reassign.aIds.slice(),bId:sid});
  var doneA=_reassign.frameA;
  cancelReassign(true);
  // regenerate just this span: select the origin keyframe so generateTweens'
  // own restrictTo logic (tweens.js generateTweens, selOnLayer) narrows to
  // it instead of redoing the whole layer.
  selClear();selAdd(li,doneA);
  generateTweens();
  selClear();
  showToast('Inbetween réattribué');
  return true;
}
document.addEventListener('pointerdown',function(e){
  if(!_reassign.active)return;
  var area=document.getElementById('canvas-area');
  if(!area||!area.contains(e.target))return;
  var w=window.SMEngineBridge?window.SMEngineBridge.screenToWorld(e.clientX,e.clientY):null;
  var pt=w?new Point(w[0],w[1]):null;
  if(!pt)return;
  if(reassignHandleClick(pt)){e.stopImmediatePropagation();e.preventDefault();}
},true);
document.addEventListener('keydown',function(e){
  if(!_reassign.active)return;
  if(e.key==='Escape'){cancelReassign();return;}
  // stopPropagation, not just preventDefault: timeline.js's own global
  // keydown handler (document.addEventListener('keydown',onKeyDown) with
  // no capture flag, i.e. bubble phase — this listener runs first since
  // it's capture:true) binds bare Enter to togglePlay(). Found live: a
  // synthetic Enter to confirm step 1 also started playback, because
  // preventDefault alone only blocks the browser's own default action, not
  // sibling listeners further down the same dispatch.
  if(e.key==='Enter'&&_reassign.step===1){e.preventDefault();e.stopPropagation();confirmReassignStep1();}
},true);
function initReassignUI(){
  var btn=document.getElementById('btn-tw-reassign');
  if(btn)btn.addEventListener('click',startReassign);
  var badge=document.getElementById('tween-reassign-badge');
  if(badge)badge.addEventListener('click',function(e){e.stopPropagation();e.preventDefault();if(_reassignBadgeAction)_reassignBadgeAction();});
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initReassignUI);else initReassignUI();

// ---- Reassign badge (2026-07, "un bouton pour réassigné le tween à un
// autre ID sur la frame suivante") — a discoverable, bounding-box-anchored
// alternative to the #btn-tw-reassign + raw-canvas-click flow above (which
// still works unchanged; this is just a second entry point onto the SAME
// _reassign state machine/state.tweenOverrides data). Green: the current
// single selection has an active tween pair to the next keyframe — click
// jumps there and arms step 2. Yellow: step 2 is pending — click confirms
// whatever's currently selected as the new target, same effect as clicking
// the shape directly (reassignHandleClick's step-2 branch), just a more
// discoverable explicit action instead of an invisible click-anywhere.
var _reassignBadgeAction=null;
function reassignBadgeEl(){return document.getElementById('tween-reassign-badge');}
function hideReassignBadge(){var el=reassignBadgeEl();if(el)el.style.display='none';_reassignBadgeAction=null;}
// Purely cosmetic: strokeId is a long opaque string, not meaningful shown
// verbatim on a small pill — hashed down to a short 3-digit label so the
// SAME element keeps the SAME label across renders/frames.
function shortIdLabel(strokeId){
  var h=0;for(var i=0;i<strokeId.length;i++){h=((h<<5)-h+strokeId.charCodeAt(i))|0;}
  return String(Math.abs(h)%900+100);
}
function positionReassignBadge(worldBounds,label,color){
  var el=reassignBadgeEl();if(!el)return;
  var canvas=document.getElementById('drawing-canvas');if(!canvas)return;
  var rect=canvas.getBoundingClientRect();
  var tr=view.projectToView(new Point(worldBounds.right,worldBounds.top));
  el.textContent='Id:'+label;
  el.className=color;
  el.style.display='block';
  el.style.left=(rect.left+tr.x+8)+'px';
  el.style.top=(rect.top+tr.y-10)+'px';
}
function updateReassignBadge(){
  var el=reassignBadgeEl();if(!el)return;
  if(_reassign.active&&_reassign.step===2){
    // Guards against a STALE selectedPaths[0] left over from before the
    // jump to frameB — loadFrame(fB) rebuilds the layer's children from
    // scratch, so the old (pre-jump) Path is still `instanceof Path` but no
    // longer actually IN the layer; without this check the badge could
    // render using a detached object's frozen bounds instead of staying
    // hidden until the artist picks a real target on the new frame.
    var liveLayer=userLayers[state.activeLayerIdx];
    if(state.activeLayerIdx!==_reassign.layer||state.currentFrame!==_reassign.frameB||selectedPaths.length!==1||!(selectedPaths[0]instanceof Path)||liveLayer.children.indexOf(selectedPaths[0])<0){hideReassignBadge();return;}
    var p=selectedPaths[0];
    positionReassignBadge(p.bounds,shortIdLabel(_reassign.aIds[0]||''),'yellow');
    _reassignBadgeAction=function(){
      var target=selectedPaths[0];if(!target)return;
      var sid=ensureStrokeId(target);
      saveActiveLayerFrame();
      var li=_reassign.layer;
      var key=li+':'+_reassign.frameA+'-'+_reassign.frameB;
      var list=state.tweenOverrides[key]=state.tweenOverrides[key]||[];
      state.tweenOverrides[key]=list.filter(function(ov){
        var ovA=ov.aIds||[ov.aId];
        return _reassign.aIds.every(function(id){return ovA.indexOf(id)<0;})&&ov.bId!==sid;
      });
      state.tweenOverrides[key].push(_reassign.aIds.length===1?{aId:_reassign.aIds[0],bId:sid}:{aIds:_reassign.aIds.slice(),bId:sid});
      var doneA=_reassign.frameA;
      cancelReassign(true);
      selClear();selAdd(li,doneA);
      generateTweens();
      selClear();
      showToast('Inbetween réattribué');
    };
    return;
  }
  if(_reassign.active){hideReassignBadge();return;} // mid legacy step-1 multi-click flow -- avoid a stale green badge underneath
  var st=computeArcMatchState();
  if(!st||st.fm.length!==1||selectedPaths.length!==1||!(selectedPaths[0]instanceof Path)||userLayers[state.activeLayerIdx].children.indexOf(selectedPaths[0])<0){hideReassignBadge();return;}
  var m=st.fm[0],sd=st.sA[m.a],p2=selectedPaths[0];
  var aStrokeId=sd.strokeId||ensureStrokeId(p2);
  var fA=st.fA,fB=st.fB;
  positionReassignBadge(p2.bounds,shortIdLabel(aStrokeId),'green');
  _reassignBadgeAction=function(){
    saveAllLayerFrames();
    _reassign.active=true;_reassign.step=2;_reassign.layer=state.activeLayerIdx;_reassign.frameA=fA;_reassign.frameB=fB;_reassign.aIds=[aStrokeId];
    goToFrame(fB);
    reassignSetStatus('2/2 — Cliquez l\'élément correspondant sur la keyframe '+(fB+1)+' (ou sélectionnez-le puis cliquez le bouton jaune)');
    showToast('Sélectionnez l\'élément correspondant, puis cliquez le bouton jaune');
  };
}

