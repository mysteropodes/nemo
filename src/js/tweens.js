// ---- TWEEN ENGINE FEATURE FLAGS (2026-07) ----
// Cyril: "possibilité de revenir sur ou/et l'autre" — each of the two
// 2026-07 additions below (curvature-aware DTW cost, fold-correction
// pass) can be independently switched off for A/B comparison or a quick
// rollback without touching the surrounding logic or reverting a commit.
// Curvature-DTW measured a real regression on testD's own motivating
// pointing-arm pair (crossings 6→9-12, MLS's win lost entirely) even
// after cutting its weight 0.2→0.08 — a coarse ≤90-point curvature
// estimate is noisy enough on a hand-drawn stroke's sharpest bend that it
// misdirects the warping path more often than it helps. Off by default,
// left in for Cyril to flip on and compare — see the flag comment below.
var TW_CURVATURE_DTW=false;
var TW_CORRECTION_PASS=true;
var TW_POINT_REDUCTION=true;
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
// 2026-07 audit: must match Rust's is_vb()/stroke_type() (tweenmatch.rs)
// EXACTLY — matchSc's type_penalty (one of the largest single cost terms)
// otherwise scores the identical stroke pair differently depending on
// whether the WASM auto_match path or this JS fallback happens to run,
// a silent divergence for any isVectorBrush:true stroke whose
// centerSegments is missing or ≤1 point (degenerate/partial recording).
function strokeType(sd){if(sd.isVectorBrush&&sd.centerSegments&&sd.centerSegments.length>1)return'vb';var hasS=!!realStrokeColor(sd),hasF=!!sd.fillColor;if(hasS&&hasF)return'both';if(hasF)return'fill';return'stroke';}
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
  // Whether isClosed came from the geometric GUESS above rather than the
  // stored ground-truth flag — a vector-brush centerline's closedness is
  // always heuristic, and two hand-drawn versions of the same limb can flip
  // it (measured live: the same arm redrawn between two keys read open in
  // one key, closed in the other). matchSc and generateTweens' rescue pass
  // both treat a guessed flag as soft evidence, not identity.
  var closedIsGuess=usingCenterline||typeof sd.closed!=='boolean';
  var fourier=fourierDescriptor(pts,cx,cy);
  p.remove();return{cx:cx,cy:cy,length:len,dirX:dx,dirY:dy,bounds:{x:b.x,y:b.y,w:b.width,h:b.height},shape:shape,pts:pts,turn:turn,closed:isClosed,closedIsGuess:closedIsGuess,strokeCol:parseHexColor(realStrokeColor(sd)),fillCol:parseHexColor(sd.fillColor),type:strokeType(sd),fourier:fourier};}
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
  // Arc-length identity penalty, recalibrated 2026-07 (testB, the mouth
  // "X" artifact): a 28px neck tick stole the 57px mouth's match (ratio
  // 2.05) from the true 52px mouth (ratio 1.10) on proximity alone — the
  // old ramp only started ABOVE 2.0 so 2.05 cost a meaningless 0.014.
  // Measured across every legitimate pair of the same file: all sit at
  // ratio ≤1.29, so a 1.6 threshold has wide margin. The absolute-
  // difference gate (>15px) keeps micro-strokes exempt: two hand-drawn
  // eye dots measured 7px vs 15px (ratio 2.04!) — at that size the ratio
  // is pure pen noise, and cel features that small legitimately jitter.
  var ratioPen=(lenRatio>1.6&&Math.abs(fA.length-fB.length)>15)?Math.min(0.7,(lenRatio-1.6)*0.5):0;
  // 0.35 only when BOTH closed flags are ground truth. When either side is
  // a heuristic guess (vector-brush centerline — see strokeFeat's
  // closedIsGuess), a disagreement is as likely a drawing accident as a
  // real topological difference — measured live: the SAME arm redrawn
  // between two keys scored 0.551 (rejected at the 0.48 threshold) purely
  // from this penalty, so the limb faded/trimmed as two unrelated strokes
  // instead of swinging. Mirrored in tweenmatch.rs (closed_pen).
  var closedPen=fA.closed!==fB.closed?((fA.closedIsGuess||fB.closedIsGuess)?0.12:0.35):0;
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
// Weighted variant (2026-07, MLS-style local rigid engine below) — same
// closed-form complex-number fit, generalized with a per-point weight so
// it reduces to fitSimilarityTransform exactly when every weight is 1.
function fitSimilarityTransformWeighted(ptsA,ptsB,weights){
  var n=ptsA.length;if(n<2)return null;
  var wsum=0,cax=0,cay=0,cbx=0,cby=0;
  for(var i=0;i<n;i++){var w=weights[i];wsum+=w;cax+=w*ptsA[i].x;cay+=w*ptsA[i].y;cbx+=w*ptsB[i].x;cby+=w*ptsB[i].y;}
  if(wsum<1e-9)return null;
  cax/=wsum;cay/=wsum;cbx/=wsum;cby/=wsum;
  var numRe=0,numIm=0,den=0;
  for(var i2=0;i2<n;i2++){
    var w2=weights[i2];
    var ax=ptsA[i2].x-cax,ay=ptsA[i2].y-cay;
    var bx=ptsB[i2].x-cbx,by=ptsB[i2].y-cby;
    numRe+=w2*(ax*bx+ay*by);numIm+=w2*(ax*by-ay*bx);den+=w2*(ax*ax+ay*ay);
  }
  if(den<1e-6)return null;
  return{wRe:numRe/den,wIm:numIm/den,ca:{x:cax,y:cay},cb:{x:cbx,y:cby}};
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
  // from where the drawing's motion says it should be" — not raw distance.
  //
  // LOCAL motion model (2026-07, "mauvaise reconnaissance de trait" on a
  // full character: arms swinging wide + head tilting back): ONE global
  // similarity transform is dominated by whichever strokes are biggest/
  // most confident (the arms/torso), so every region moving differently
  // (the face) gets mispredicted, and its small same-looking features
  // chain-mismatch (measured: hair→nose, nose→mouth, mouth→chin cascade,
  // each individual score mediocre-but-passing while the correct pairing
  // scored strictly better in raw cost — the bad global prediction is
  // what inverted the ranking). Character animation is articulated —
  // there is no single rigid motion. Fix: per-stroke local transform
  // fitted on the K nearest seed pairs (by centroid distance in A), so
  // an arm stroke is predicted by arm-region motion and a face stroke by
  // face-region motion. K=4 gives a well-determined similarity fit (2
  // points minimum, 4 gives redundancy against one bad seed) while
  // staying local; the global fit remains as fallback when a local one
  // is degenerate, and for tiny seed sets (<=K) where "local" would just
  // be the same as global anyway.
  var K_LOCAL=4;
  var ptsT=fA.map(function(f,ai){
    var tf=transform;
    if(seeds.length>K_LOCAL){
      var near=seeds.slice().sort(function(s1,s2){
        var d1=Math.pow(fA[s1.a].cx-f.cx,2)+Math.pow(fA[s1.a].cy-f.cy,2);
        var d2=Math.pow(fA[s2.a].cx-f.cx,2)+Math.pow(fA[s2.a].cy-f.cy,2);
        return d1-d2;
      }).slice(0,K_LOCAL);
      var lA=near.map(function(s){return{x:fA[s.a].cx,y:fA[s.a].cy};});
      var lB=near.map(function(s){return{x:fB[s.b].cx,y:fB[s.b].cy};});
      var lt=fitSimilarityTransform(lA,lB);
      if(lt){
        // A local fit from few points can go wild (huge scale/spin from
        // near-degenerate geometry) — sanity-bound it against the global
        // fit and fall back when implausible, same spirit as interpStroke's
        // own mag window.
        var lmag=Math.sqrt(lt.wRe*lt.wRe+lt.wIm*lt.wIm);
        if(lmag>0.15&&lmag<8)tf=lt;
      }
    }
    return f.pts.map(function(pt){var q=applySimilarityTransform(tf,pt[0],pt[1]);return[q.x,q.y];});
  });
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
      // NEAR-TWIN widened tolerance (2026-07, "un oeil est mal reconnu
      // par rapport à l'autre — identification par rapport au
      // placement"): two eyes on a head that shifted -70px matched
      // crossed (each eye to the OTHER's new position) and the swap
      // missed the 0.08 tolerance by 0.026 (swapped 0.866 vs crossed
      // 0.76+0.08) — absolute proximity outvoted left-stays-left. When
      // all four strokes are near-twins (same type both sides, length
      // ratio <1.5 within each side's pair), crossing trajectories are
      // near-certain matching error — cel features preserve their
      // spatial arrangement — so the uncross gets a much wider benefit
      // of the doubt. Genuinely-crossing distinct objects stay protected
      // by the color-clash/type penalties (0.4+, above even this).
      // Same micro-stroke exemption as matchSc's ratioPen: two hand-drawn
      // eye ticks measured 21px vs 13px (ratio 1.6 — pure pen noise at
      // that size), so a small ABSOLUTE difference also qualifies as twin.
      var lr1=Math.max(fA[m1.a].length,fA[m2.a].length)/Math.max(1,Math.min(fA[m1.a].length,fA[m2.a].length));
      var lr2=Math.max(fB[m1.b].length,fB[m2.b].length)/Math.max(1,Math.min(fB[m1.b].length,fB[m2.b].length));
      var twin1=lr1<1.5||Math.abs(fA[m1.a].length-fA[m2.a].length)<15;
      var twin2=lr2<1.5||Math.abs(fB[m1.b].length-fB[m2.b].length)<15;
      var twins=fA[m1.a].type===fA[m2.a].type&&fB[m1.b].type===fB[m2.b].type&&twin1&&twin2;
      var tol=twins?0.25:UNCROSS_TOL;
      if(swp<=cur+tol){
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
  // ARC-SPACED non-maximum suppression (2026-07, "un pli se forme qu'il
  // faut identifier et positionner tout le long de l'inter"): strongest-
  // first with no spacing rule, a scribbly HAND eats every landmark slot
  // (measured on the reported arm: all 8 landed within frac 0.57-0.76 —
  // the hand — while the ELBOW, the one corner that actually travels,
  // got none, so the fold could never be re-synchronized). Greedy
  // strongest-first with a minimum arc separation spreads the budget
  // along the whole stroke: the hand keeps its 2-3 sharpest corners and
  // the elbow finally gets a slot. Separation of half an even split
  // still lets two GENUINE nearby corners both in (a zigzag), it only
  // blocks piles.
  var minSep=0.5/Math.max(2,maxFeatures);
  var picked=[];
  for(var fi2=0;fi2<feats.length&&picked.length<maxFeatures;fi2++){
    var cand=feats[fi2];
    var ok=true;
    for(var pj=0;pj<picked.length;pj++)if(Math.abs(picked[pj].t-cand.t)<minSep){ok=false;break;}
    if(ok)picked.push(cand);
  }
  return picked.map(function(f){return f.t;}).sort(function(a,b){return a-b;});
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
    // LOCAL handle length (2026-07, "artefact de trait... bump" — visible
    // kink reported live on a DTW-correspondence stroke): the old formula
    // divided the WHOLE stroke's arc length evenly across every point
    // regardless of where fractions[i] actually sits relative to its
    // neighbors — only correct when `fractions` is uniform. DTW/landmark
    // correspondence deliberately ISN'T uniform (a fold earns more
    // samples than a straight run), so wherever local point density ran
    // denser than the stroke's global average, the old fixed handle
    // overshot and rendered as a visible bump. Handle length is now the
    // ACTUAL local gap to each neighbor (Catmull-Rom style — averaged
    // from both sides at an interior point), so it shrinks exactly where
    // points are packed together and grows where they're sparse, instead
    // of an average that's wrong almost everywhere fractions isn't
    // uniform. Reduces to the exact old formula when fractions IS
    // uniform (nextOff-prevOff = 2·len/(n-1) there, /6 undoes the ×2).
    var prevOff=i>0?fractions[i-1]*len:off;
    var nextOff=i<fractions.length-1?fractions[i+1]*len:off;
    var hl=Math.max(0.01,nextOff-prevOff)/6;
    segs.push({point:[pt.x,pt.y],handleIn:i===0?[0,0]:[-tan.x*hl,-tan.y*hl],handleOut:i===fractions.length-1?[0,0]:[tan.x*hl,tan.y*hl]});
  }
  return segs;
}
// ---- CHIKARA-SEN (line-of-force) LANDMARK CORRESPONDENCE (2026-07) ----
// User-proposed, from the traditional Japanese inbetweening technique: an
// animator identifies anatomically-equivalent points between two poses
// (a knuckle in key A, the SAME knuckle in key B) and draws a straight
// "force line" between them — the inbetween follows that line, not a
// generic nearest-point or same-arc-length-fraction guess. This is the
// missing piece behind a real failure mode found this session (a hand-
// drawn arm+hand silhouette ballooning into a "noodle" mid-tween, DTW-on-
// turning-angle didn't fix it either): buildSharedFractions/
// _sampleAtFractions apply the SAME fraction list to BOTH A and B, which
// silently assumes a landmark sits at the same arc-length % of the
// perimeter in both poses — false whenever a fold (the hand/finger detail
// here) eats a different SHARE of the stroke between poses. detectFeature-
// Fractions already finds each shape's OWN corners; what was missing is
// matching A's corners to B's corners the way an animator would — by which
// pairing reads as ONE COHERENT motion, not by raw distance — then letting
// A and B's fraction lists DIFFER at the matched landmarks so each anchored
// segment gets re-parametrized independently on each side.
//
// Same 2-pass "force line" motion-model recipe autoMatchJS already uses at
// the STROKE-matching level (fitSimilarityTransform seeded from a naive
// guess, then re-score every candidate pair by residual against the fitted
// transform, Hungarian-assign) — applied here one level down, to a single
// stroke's own landmarks instead of a frame's whole set of strokes.
function _matchLandmarks(pA,lenA,pB,lenB,maxFeatures){
  var featsA=detectFeatureFractions(pA,maxFeatures),featsB=detectFeatureFractions(pB,maxFeatures);
  if(featsA.length<2||featsB.length<2)return null;
  var ptsA=featsA.map(function(f){var p=pA.getPointAt(f*lenA);return p?{x:p.x,y:p.y}:null;});
  var ptsB=featsB.map(function(f){var p=pB.getPointAt(f*lenB);return p?{x:p.x,y:p.y}:null;});
  if(ptsA.indexOf(null)>=0||ptsB.indexOf(null)>=0)return null;
  // Pass 1 (seed): fit the stroke's OWN global motion from 32 uniform
  // arc-length probes of the two paths — not from the corners themselves.
  // The original rank-order corner seed (i-th A corner ↔ interpolated
  // i-th B corner) degenerates whenever the two sides detect different
  // corner sets: measured on the reported pointing arm (5 vs 4 corners,
  // sets only partially overlapping), the seed fit collapsed to scale
  // 0.15 / rotation 174° — under which EVERY pairing's residual passed
  // the gate, the Hungarian assigned corners arbitrarily, and the
  // monotonicity check then (rightly) bailed the whole landmark pass to
  // null, losing the elbow. Uniform whole-stroke probes always exist,
  // are already order-consistent (the early-orientation pass upstream
  // fixed the drawing direction), and capture the limb's real motion.
  var m=Math.min(ptsA.length,ptsB.length);
  var _gK=32,_gA=[],_gB=[];
  for(var gi=0;gi<_gK;gi++){
    var gpa=pA.getPointAt(lenA*gi/(_gK-1))||pA.lastSegment.point;
    var gpb=pB.getPointAt(lenB*gi/(_gK-1))||pB.lastSegment.point;
    _gA.push({x:gpa.x,y:gpa.y});_gB.push({x:gpb.x,y:gpb.y});
  }
  var transform=fitSimilarityTransform(_gA,_gB);
  if(!transform)return null;
  // Pass 2: re-score EVERY (i,j) by residual after the fitted transform —
  // the "does this pairing agree with the drawing's overall motion"
  // question — augmented with dummy rows/cols so a landmark with no honest
  // match (appeared/disappeared, e.g. a finger only visible in one pose)
  // can opt out instead of being forced onto the nearest wrong point.
  var n2=ptsA.length,o=ptsB.length,N=n2+o;
  var maxOkDist=Math.max(24,0.4*Math.max(lenA,lenB)/Math.max(1,m));
  var cost=[];
  for(var a=0;a<N;a++){
    var row=[];
    for(var b=0;b<N;b++){
      if(a<n2&&b<o){var q=applySimilarityTransform(transform,ptsA[a].x,ptsA[a].y);row.push(Math.hypot(q.x-ptsB[b].x,q.y-ptsB[b].y));}
      else if(a>=n2&&b>=o)row.push(0);
      else row.push(maxOkDist);
    }
    cost.push(row);
  }
  var assign=hungarian(cost);
  var pairs=[];
  for(var a2=0;a2<n2;a2++){
    var b2=assign[a2];
    if(b2===undefined||b2<0||b2>=o)continue;
    var q2=applySimilarityTransform(transform,ptsA[a2].x,ptsA[a2].y);
    if(Math.hypot(q2.x-ptsB[b2].x,q2.y-ptsB[b2].y)<=maxOkDist)pairs.push({fracA:featsA[a2],fracB:featsB[b2]});
  }
  // ONE matched landmark is enough (2026-07, "un pli qu'il faut
  // identifier et positionner tout le long de l'inter"): on the reported
  // pointing arm, the ELBOW was the only corner whose pairing survived
  // the residual gate (the hand's corners move too non-rigidly for the
  // global fit — residuals 125-174 vs the 66 ceiling) — and one elbow
  // pair is exactly the fold re-synchronization this exists for
  // (_landmarkFractions handles a single pair as two independent spans,
  // start→elbow and elbow→end). The old ≥2 gate threw away the elbow
  // with the rest.
  if(pairs.length<1)return null;
  pairs.sort(function(x,y){return x.fracA-y.fracA;});
  // A genuine cyclic reorder (not just a missing/extra landmark) breaks the
  // "one coherent motion" premise this whole approach rests on — bail
  // rather than build crossed, self-tangling segments.
  for(var k=1;k<pairs.length;k++)if(pairs[k].fracB<=pairs[k-1].fracB)return null;
  return pairs;
}
// ---- DTW (dynamic time warping) FULL-CURVE CORRESPONDENCE (2026-07) ----
// The keystone upgrade over chikara-sen: landmarks above match a SPARSE set
// of detected corners, then re-parametrize piecewise-linearly between them —
// every non-corner point still gets its correspondence by blind arc-length
// interpolation. Cyril's diagnosis, live: "le moteur se permet d'inverser
// le trait pendant l'intervalle... ça manque d'intelligence de distinction
// de forme, même de pli qu'on retrouve par la suite même si c'est pas le
// même nombre de vertices" — what's missing is a correspondence computed
// over EVERY point, not just corners, that can't invert by construction.
// DTW is exactly that: given two point sequences, its dynamic-programming
// warping path is monotonic by definition (i and j each only ever advance
// forward), so a direction flip mid-stroke is structurally impossible —
// and it naturally handles unequal vertex/feature counts (a plateau in the
// path lets several samples on the fold-heavy side map to one on the other).
//
// Historical note (see this section's header comment above): a turning-
// angle-ONLY DTW cost was tried earlier and did NOT fix the reported
// "noodle" ballooning — local curvature agreement alone still let far-apart
// regions match if their bend happened to look similar (a wrist curl
// matching a shoulder bend). The cost here is POSITION-dominant instead
// (mirrors matchSc's own proximity-is-dominant design, and _matchLandmarks'
// residual-against-fitted-motion test): each candidate pairing is scored by
// how far B's point lands from where the stroke's own fitted rigid motion
// predicts A's point should go, with local tangent-angle disagreement as a
// secondary tie-breaker — so the correspondence follows the drawing's real
// motion, with shape only refining ambiguous ties, not driving the match.
//
// Runs as the PRIMARY correspondence (tried before landmarks); landmarks
// and the plain shared-fraction grid remain as the fallback cascade for
// whatever DTW can't handle (too few samples, no coherent rigid motion,
// zero-length input) — Cyril: "garde quand même l'heuristique de côté ou
// même d'appoint".
function _dtwCorrespondence(pA,lenA,pB,lenB,n){
  if(!(lenA>0)||!(lenB>0))return null;
  var gK=Math.max(16,Math.min(90,n)); // DP grid resolution — independent of n, capped for O(gK²) cost
  var ptsA=[],ptsB=[],tanA=[],tanB=[];
  for(var i=0;i<gK;i++){
    var off=lenA*i/(gK-1);
    var p=pA.getPointAt(off)||pA.lastSegment.point;
    var t=pA.getTangentAt(off);
    ptsA.push({x:p.x,y:p.y});tanA.push(t?Math.atan2(t.y,t.x):0);
  }
  for(var j=0;j<gK;j++){
    var offB=lenB*j/(gK-1);
    var pb=pB.getPointAt(offB)||pB.lastSegment.point;
    var tb=pB.getTangentAt(offB);
    ptsB.push({x:pb.x,y:pb.y});tanB.push(tb?Math.atan2(tb.y,tb.x):0);
  }
  // Curvature at each grid sample (2026-07, Cyril: "reconnaissance de
  // point de courbure afin de mieux identifier les points qui doivent se
  // retrouver aux mêmes endroits de courbure"): arc-length-NORMALIZED
  // curvature (Δtangent/Δs), not the raw per-vertex turning angle used
  // elsewhere in this file (candScore's fold-angle term, MLS handle
  // placement) — those are fine as a local tie-breaker on an ALREADY
  // n-point-resampled stroke where spacing is roughly uniform, but here
  // the grid step ds is a genuine per-shape constant (lenA/(gK-1),
  // lenB/(gK-1)), so normalizing by it makes the signal comparable
  // between two shapes even if their raw sampling density differs.
  var dsA=lenA/(gK-1),dsB=lenB/(gK-1);
  var curvA=new Array(gK),curvB=new Array(gK);
  for(var ka=0;ka<gK;ka++){
    var prevA=tanA[ka>0?ka-1:ka],nextA=tanA[ka<gK-1?ka+1:ka];
    var spanA=(ka>0&&ka<gK-1)?2*dsA:dsA;
    curvA[ka]=_wrapPI(nextA-prevA)/Math.max(1e-6,spanA);
  }
  for(var kb=0;kb<gK;kb++){
    var prevB=tanB[kb>0?kb-1:kb],nextB=tanB[kb<gK-1?kb+1:kb];
    var spanB=(kb>0&&kb<gK-1)?2*dsB:dsB;
    curvB[kb]=_wrapPI(nextB-prevB)/Math.max(1e-6,spanB);
  }
  // Rigid motion the correspondence should agree with — same 32-probe
  // whole-stroke fit _matchLandmarks seeds from (already order-consistent:
  // resamplePairFeatureAware's early-orientation pass runs before this).
  var transform=fitSimilarityTransform(ptsA,ptsB);
  if(!transform)return null;
  var predA=ptsA.map(function(p){return applySimilarityTransform(transform,p.x,p.y);});
  var normScale=Math.max(1,(lenA+lenB)/2*0.25); // "far" ≈ a quarter of the stroke's own length
  function cost(i,j){
    var dx=predA[i].x-ptsB[j].x,dy=predA[i].y-ptsB[j].y;
    var posC=Math.hypot(dx,dy)/normScale;
    var ad=Math.abs(_wrapPI(tanA[i]-tanB[j]))/Math.PI; // 0..1
    // Curvature agreement — kept a MODEST tie-breaker (like the tangent
    // term above), never allowed to outweigh position: a wrist curl and a
    // shoulder bend can share similar curvature by coincidence, exactly
    // the "noodle ballooning" failure this file's own history note above
    // already warns a curvature-only cost falls into.
    var cc=TW_CURVATURE_DTW?Math.min(1,Math.abs(curvA[i]-curvB[j])*normScale):0;
    return posC+0.35*ad+0.08*cc;
  }
  // Classic DTW DP: monotonic path from (0,0) to (gK-1,gK-1), each step
  // advances i, j, or both — a plateau (several j's per i or vice versa)
  // is exactly a fold eating more of one side's perimeter than the other.
  var D=new Float64Array(gK*gK);var BP=new Uint8Array(gK*gK); // 0=diag,1=up(i-1),2=left(j-1)
  var IDX=function(i,j){return i*gK+j;};
  D[0]=cost(0,0);
  for(var i2=1;i2<gK;i2++){D[IDX(i2,0)]=D[IDX(i2-1,0)]+cost(i2,0);BP[IDX(i2,0)]=1;}
  for(var j2=1;j2<gK;j2++){D[IDX(0,j2)]=D[IDX(0,j2-1)]+cost(0,j2);BP[IDX(0,j2)]=2;}
  for(var i3=1;i3<gK;i3++){
    for(var j3=1;j3<gK;j3++){
      var c=cost(i3,j3);
      var dd=D[IDX(i3-1,j3-1)],du=D[IDX(i3-1,j3)],dl=D[IDX(i3,j3-1)];
      if(dd<=du&&dd<=dl){D[IDX(i3,j3)]=dd+c;BP[IDX(i3,j3)]=0;}
      else if(du<=dl){D[IDX(i3,j3)]=du+c;BP[IDX(i3,j3)]=1;}
      else{D[IDX(i3,j3)]=dl+c;BP[IDX(i3,j3)]=2;}
    }
  }
  // Backtrack to the path, then resample it to exactly n output points,
  // evenly spaced by average arc-length PROGRESS (fracA+fracB)/2 — same
  // distribution principle as _landmarkFractions, just driven by the full
  // per-vertex path instead of only the matched-landmark stops.
  var path=[];var pi=gK-1,pj=gK-1;
  path.push([pi,pj]);
  while(pi>0||pj>0){
    var mv=BP[IDX(pi,pj)];
    if(pi===0)mv=2;else if(pj===0)mv=1;
    if(mv===0){pi--;pj--;}else if(mv===1)pi--;else pj--;
    path.push([pi,pj]);
  }
  path.reverse();
  var prog=path.map(function(pr){return (pr[0]/(gK-1)+pr[1]/(gK-1))/2;});
  var fracA=[0],fracB=[0];
  var pk=0;
  for(var oi2=1;oi2<n-1;oi2++){
    var target=oi2/(n-1);
    while(pk<prog.length-2&&prog[pk+1]<target)pk++;
    var p0=prog[pk],p1=prog[pk+1]!==undefined?prog[pk+1]:p0;
    var lt=p1>p0?(target-p0)/(p1-p0):0;
    var a0=path[pk][0]/(gK-1),a1=(path[pk+1]||path[pk])[0]/(gK-1);
    var b0=path[pk][1]/(gK-1),b1=(path[pk+1]||path[pk])[1]/(gK-1);
    fracA.push(a0+(a1-a0)*lt);fracB.push(b0+(b1-b0)*lt);
  }
  fracA.push(1);fracB.push(1);
  // Monotonicity is guaranteed by construction, but guard against float
  // fuzz at a plateau boundary before handing this to _sampleAtFractions.
  for(var mi=1;mi<n;mi++){
    if(fracA[mi]<fracA[mi-1])fracA[mi]=fracA[mi-1];
    if(fracB[mi]<fracB[mi-1])fracB[mi]=fracB[mi-1];
  }
  return{fracA:fracA,fracB:fracB};
}
// Turns matched landmarks into TWO fraction lists (length n each, same
// LENGTH so interpStroke's index-paired lerp still works, but the VALUES
// at a given index can now differ between A and B) — piecewise-linear
// re-parametrization: between two consecutive matched landmarks (plus the
// implicit start/end pair), A's own span and B's own span are each split
// independently, so the fold that ate a different % of the perimeter in
// each pose no longer drags unrelated points into the same sample. Segment
// sample budget is proportional to the AVERAGE of A's and B's own span
// length, so a segment that's short on both sides doesn't hog points.
function _landmarkFractions(landmarks,n){
  var stops=[{a:0,b:0}].concat(landmarks).concat([{a:1,b:1}]).map(function(s){return{a:s.fracA!==undefined?s.fracA:s.a,b:s.fracB!==undefined?s.fracB:s.b};});
  stops=stops.filter(function(s,i){return i===0||i===stops.length-1||(s.a>0.02&&s.a<0.98);});
  if(stops.length<3)return null; // every landmark got filtered as too-close-to-an-endpoint
  var segLens=[];
  for(var i=1;i<stops.length;i++)segLens.push(Math.max(0.0001,((stops[i].a-stops[i-1].a)+(stops[i].b-stops[i-1].b))/2));
  var total=segLens.reduce(function(s,v){return s+v;},0);
  var raw=segLens.map(function(l){return l/total*(n-1);});
  var counts=raw.map(function(v){return Math.max(1,Math.round(v));});
  var sum=counts.reduce(function(s,v){return s+v;},0);
  counts[counts.length-1]+=(n-1-sum);
  if(counts[counts.length-1]<1)return null; // rounding drift ate the last segment — bail, caller falls back
  var fracA=[0],fracB=[0];
  for(var s=0;s<counts.length;s++){
    var a0=stops[s].a,a1=stops[s+1].a,b0=stops[s].b,b1=stops[s+1].b;
    for(var k=1;k<=counts[s];k++){
      var lt=k/counts[s];
      fracA.push(a0+(a1-a0)*lt);fracB.push(b0+(b1-b0)*lt);
    }
  }
  if(fracA.length!==n)return null;
  return{fracA:fracA,fracB:fracB};
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
  // EARLY ORIENTATION (2026-07, "bras à gauche qui change de sens pendant
  // l'inter" + "un pli qu'il faut positionner tout le long"): the drawing
  // direction of B was only ever resolved AFTER this function, by
  // alignResampledPair reversing the RESAMPLED points — which destroys
  // whatever correspondence the landmark pass below built (each fracB
  // then reads the geometry from the wrong end), and feeds _matchLandmarks
  // a B whose along-the-stroke rank order is backwards, so the elbow
  // corner can't pair with its counterpart in the first place (measured
  // on the reported pointing arm, drawn shoulder→hand in one key and
  // hand→shoulder in the other: landmarks degenerated to fracA===fracB,
  // midframe arc length collapsed 47%). Decide the reversal HERE, on the
  // raw geometry, before any fraction is placed: 32 uniform probes per
  // side, centroid-relative squared distance, direct vs reversed — the
  // exact test alignResampledPair applies later, which then simply
  // confirms the already-correct orientation for open strokes. Closed
  // loops keep their existing cyclic-rotation handling downstream.
  var bReversed=false;
  if(!pB.closed){
    var _oPA=[],_oPB=[],_oK=32;
    for(var oi=0;oi<_oK;oi++){
      var oa=pA.getPointAt(lenA*oi/(_oK-1))||pA.lastSegment.point;
      var ob=pB.getPointAt(lenB*oi/(_oK-1))||pB.lastSegment.point;
      _oPA.push(oa);_oPB.push(ob);
    }
    var _cax=0,_cay=0,_cbx=0,_cby=0;
    for(var oj=0;oj<_oK;oj++){_cax+=_oPA[oj].x;_cay+=_oPA[oj].y;_cbx+=_oPB[oj].x;_cby+=_oPB[oj].y;}
    _cax/=_oK;_cay/=_oK;_cbx/=_oK;_cby/=_oK;
    var _cd=0,_cr=0;
    for(var ok=0;ok<_oK;ok++){
      var ax=_oPA[ok].x-_cax,ay=_oPA[ok].y-_cay;
      var bxD=_oPB[ok].x-_cbx,byD=_oPB[ok].y-_cby;
      var bxR=_oPB[_oK-1-ok].x-_cbx,byR=_oPB[_oK-1-ok].y-_cby;
      _cd+=(ax-bxD)*(ax-bxD)+(ay-byD)*(ay-byD);
      _cr+=(ax-bxR)*(ax-bxR)+(ay-byR)*(ay-byR);
    }
    if(_cr<_cd){
      bReversed=true;
      srcB=srcB.slice().reverse().map(function(s){return{point:s.point,handleIn:s.handleOut,handleOut:s.handleIn,width:s.width};});
      pB.remove();
      pB=new Path({insert:false});srcB.forEach(function(s){pB.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
      pB.closed=false;
      lenB=pB.length;
    }
  }else{
    // CLOSED-LOOP early start alignment (2026-07, same "resolve orientation
    // before any correspondence is built" principle as the open-stroke
    // case above, extended to cyclic shapes): a closed vector-brush
    // centerline's index 0 is just wherever the artist happened to start
    // drawing — nothing guarantees it sits at the same anatomical point in
    // both keyframes. DTW below has no notion of "the loop wraps"; without
    // this it silently assumed pA's start already corresponds to pB's
    // start. Coarse search (12 rotation offsets ×
    // reversed/direct, 32-point probe each — cheap, same cost class as the
    // open-stroke test above) picks the (direction, start-offset) minimizing
    // centroid-relative squared distance, exactly alignResampledPairJS's
    // own rotationFitResidual search but run once here BEFORE sampling so
    // every downstream correspondence stage (DTW, landmarks, plain grid)
    // agrees on where B "starts" instead of only the final safety-net
    // alignment (which reorders already-sampled points and would desync
    // whatever correspondence was built against the old order).
    var _cK=32,_cRot=12;
    var _cPA=[];for(var ci=0;ci<_cK;ci++){var cpa=pA.getPointAt(lenA*ci/_cK)||pA.lastSegment.point;_cPA.push(cpa);}
    var _ccax=0,_ccay=0;for(var ci2=0;ci2<_cK;ci2++){_ccax+=_cPA[ci2].x;_ccay+=_cPA[ci2].y;}_ccax/=_cK;_ccay/=_cK;
    var _bestCost=Infinity,_bestRev=false,_bestOff=0;
    [false,true].forEach(function(rev){
      for(var ro=0;ro<_cRot;ro++){
        var offFrac=ro/_cRot;
        var cbx=0,cby=0,cpts=[];
        for(var ci3=0;ci3<_cK;ci3++){
          var tfrac=offFrac+(rev?-(ci3/_cK):(ci3/_cK));
          tfrac=((tfrac%1)+1)%1;
          var cpb=pB.getPointAt(lenB*tfrac)||pB.lastSegment.point;
          cpts.push(cpb);cbx+=cpb.x;cby+=cpb.y;
        }
        cbx/=_cK;cby/=_cK;
        var s=0;
        for(var ci4=0;ci4<_cK;ci4++){
          var dxA=_cPA[ci4].x-_ccax,dyA=_cPA[ci4].y-_ccay;
          var dxB=cpts[ci4].x-cbx,dyB=cpts[ci4].y-cby;
          s+=(dxA-dxB)*(dxA-dxB)+(dyA-dyB)*(dyA-dyB);
        }
        if(s<_bestCost){_bestCost=s;_bestRev=rev;_bestOff=offFrac;}
      }
    });
    if(_bestRev||_bestOff>0.001){
      bReversed=_bestRev;
      // Width lookup into the PRE-rebuild centerline (arc-length fraction,
      // same convention widthsAtSparse already uses) — the rebuilt array
      // below only carries fresh geometry; without this every rebuilt
      // point would silently lose its recorded brush width.
      var _origSrcB=srcB,_origLenB=0,_origSegLens=[0];
      for(var wi3=1;wi3<_origSrcB.length;wi3++){_origLenB+=Math.hypot(_origSrcB[wi3].point[0]-_origSrcB[wi3-1].point[0],_origSrcB[wi3].point[1]-_origSrcB[wi3-1].point[1]);_origSegLens.push(_origLenB);}
      function _widthAtOrigFrac(fr){
        var target=fr*(_origLenB||1);
        var wi4=0;while(wi4<_origSegLens.length-2&&_origSegLens[wi4+1]<target)wi4++;
        var span=Math.max(0.0001,_origSegLens[wi4+1]-_origSegLens[wi4]);
        var lt2=Math.min(1,Math.max(0,(target-_origSegLens[wi4])/span));
        var w0=_origSrcB[wi4].width!==undefined?_origSrcB[wi4].width:1;
        var w1=(_origSrcB[wi4+1]&&_origSrcB[wi4+1].width!==undefined)?_origSrcB[wi4+1].width:w0;
        return w0+(w1-w0)*lt2;
      }
      var _rebuilt=[];
      for(var ni=0;ni<_cK;ni++){
        var tf2=_bestOff+(_bestRev?-(ni/_cK):(ni/_cK));
        tf2=((tf2%1)+1)%1;
        var np=pB.getPointAt(lenB*tf2)||pB.lastSegment.point;
        var nt=pB.getTangentAt(lenB*tf2)||new Point(1,0);
        var nhl=lenB/_cK/3;
        _rebuilt.push({point:[np.x,np.y],handleIn:[-nt.x*nhl,-nt.y*nhl],handleOut:[nt.x*nhl,nt.y*nhl],width:_widthAtOrigFrac(tf2)});
      }
      srcB=_rebuilt;
      pB.remove();
      pB=new Path({insert:false});srcB.forEach(function(s){pB.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
      pB.closed=true;
      lenB=pB.length;
    }
  }
  var fractions=buildSharedFractions(pA,pB,n);
  var fractionsB=fractions; // default: identical to A's (today's behavior)
  // Correspondence cascade (2026-07, "je cherche le meilleur moteur pour
  // ça, c'est la clé de voûte de l'app"): DTW (full per-vertex, monotonic
  // by construction — see _dtwCorrespondence's own header) is now the
  // PRIMARY correspondence; chikara-sen landmarks and the plain shared-
  // fraction grid above remain as a heuristic fallback cascade for
  // whatever DTW can't handle. Every stage only replaces `fractions`/
  // `fractionsB` on success, so a shape none of this applies to (a circle,
  // a straight line, a rigid rotation already handled well) still falls
  // through to the untouched uniform grid.
  var _uniformFractions=fractions; // the plain grid, kept as an arbitration candidate below
  if(n>=10){
    var dtw=_dtwCorrespondence(pA,lenA,pB,lenB,n);
    if(dtw){
      fractions=dtw.fracA;fractionsB=dtw.fracB;
    }else{
      var maxFeat=Math.max(2,Math.min(8,Math.floor(n/8)));
      var landmarks=_matchLandmarks(pA,lenA,pB,lenB,maxFeat);
      if(landmarks){
        var lf=_landmarkFractions(landmarks,n);
        if(lf){fractions=lf.fracA;fractionsB=lf.fracB;}
      }
    }
  }
  var segsA=_sampleAtFractions(pA,lenA,fractions);
  var segsB=_sampleAtFractions(pB,lenB,fractionsB);
  // FINAL EMPIRICAL ARBITRATION (2026-07, "ça manque encore d'intelligence
  // de distinction de forme" — the visual check Cyril asked for): even a
  // monotonic-by-construction DTW path can pick a globally bad alignment
  // on a shape whose deformation is too large for any correspondence to
  // stay smooth (measured on a re-drawn closed hand: min per-vertex
  // motion 65px, 2208 pairwise crossings among the A[i]→B[i] motion
  // lines — de-rotating didn't help, so it's the correspondence itself,
  // not the rigid blend downstream). Direct, cheap measurement: count how
  // many of those motion lines cross EACH OTHER for the winning candidate
  // vs the plain uniform grid (always available, always monotonic in the
  // most boring possible way) — whichever tangles less wins. Ties (or
  // n too large to afford the O(n²) check — capped, this is a last-
  // resort safety net, not the primary decision) keep the richer
  // candidate, matching every other probe in this file.
  if(fractions!==_uniformFractions&&n>=5&&n<=200){
    function _motionLineX(ptsA2,ptsB2){
      var c=0;
      for(var mi2=0;mi2<n;mi2++){
        for(var mj2=mi2+3;mj2<n;mj2++){
          var a1=ptsA2[mi2],b1=ptsB2[mi2],a2=ptsA2[mj2],b2=ptsB2[mj2];
          function ccwM(a,b,cc){return (cc[1]-a[1])*(b[0]-a[0])>(b[1]-a[1])*(cc[0]-a[0]);}
          if(ccwM(a1,a2,b2)!==ccwM(b1,a2,b2)&&ccwM(a1,b1,a2)!==ccwM(a1,b1,b2))c++;
        }
      }
      return c;
    }
    var curA=segsA.map(function(s){return s.point;}),curB=segsB.map(function(s){return s.point;});
    var xCur=_motionLineX(curA,curB);
    if(xCur>0){
      var uniA=_sampleAtFractions(pA,lenA,_uniformFractions),uniB=_sampleAtFractions(pB,lenB,_uniformFractions);
      var xUni=_motionLineX(uniA.map(function(s){return s.point;}),uniB.map(function(s){return s.point;}));
      if(xUni<xCur*0.85){ // uniform must be MEANINGFULLY better, not just tied — keep the richer correspondence otherwise
        segsA=uniA;segsB=uniB;fractions=_uniformFractions;fractionsB=_uniformFractions;
      }
    }
  }
  var ra,rb;
  if(isVB){
    function widthsAtSparse(srcSegs,total,fracs){
      var segLens=[0];for(var i=1;i<srcSegs.length;i++)segLens.push(segLens[i-1]+new Point(srcSegs[i].point[0],srcSegs[i].point[1]).getDistance(new Point(srcSegs[i-1].point[0],srcSegs[i-1].point[1])));
      var tot=segLens[segLens.length-1]||1;
      return fracs.map(function(t){
        var targetLen=t*tot;
        var wi=0;while(wi<segLens.length-2&&segLens[wi+1]<targetLen)wi++;
        var span=Math.max(0.0001,segLens[wi+1]-segLens[wi]);
        var lt=Math.min(1,Math.max(0,(targetLen-segLens[wi])/span));
        return srcSegs[wi].width+(srcSegs[wi+1].width-srcSegs[wi].width)*lt;
      });
    }
    // Prefer the DENSE raw-pressure profile over sparse centerline-anchor
    // widths — same fix, same reasoning as rebuildVectorBrushOutline's own
    // (tools.js, buildWidthProfile's comment): Paper's simplify() can leave
    // a stroke's editable centerline with very few anchors, so interpolating
    // ONLY between them discards most of the recorded pressure curve. A
    // live-drawn keyframe already renders from this profile (widthAtFrac);
    // a generated inbetween never consulted it at all — found live
    // ("le bras est moins épais [aux clés]... l'intervalle" thicker than
    // both keyframes): measured via rendered outline area/length, a real
    // arm stroke's own keyframes render ~4.06-4.57px average width while
    // the sparse-only inbetween rendered ~4.66-4.77px — thicker than
    // EITHER keyframe, because the sparse anchor interpolation misses the
    // narrower dips the dense profile actually recorded. widthAtFrac's `t`
    // domain (raw-sample arc-length fraction) is already exactly the same
    // convention as `fractions` here, so no conversion is needed.
    function widthsAt(sdData,srcSegs,total,fracs,rev){
      // The dense pressure profile is indexed by the RAW drawing's own
      // arc-length fraction — when B was flipped by the early-orientation
      // pass above, a resample fraction t reads the profile at 1-t. The
      // sparse fallback needs no conversion: srcSegs is already the
      // flipped array, widths riding along with their anchors.
      if(sdData.widthProfile&&sdData.widthProfile.length>1)return fracs.map(function(t){return widthAtFrac(sdData.widthProfile,rev?1-t:t);});
      return widthsAtSparse(srcSegs,total,fracs);
    }
    ra={segments:segsA,widths:widthsAt(aData,srcA,lenA,fractions,false),isVectorBrush:true,strokeColor:null,fillColor:aData.fillColor||null,opacity:aData.opacity!==undefined?aData.opacity:1};
    rb={segments:segsB,widths:widthsAt(bData,srcB,lenB,fractionsB,bReversed),isVectorBrush:true,strokeColor:null,fillColor:bData.fillColor||null,opacity:bData.opacity!==undefined?bData.opacity:1};
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
// ---- INTRINSIC (edge-length + turning-angle) INTERPOLATION ----
// Sederberg-style "physically based" 2D shape blending: instead of lerping
// vertex POSITIONS (which cuts corners — a bending stroke's interpolated
// midframes are shorter than either keyframe, the classic elbow-shrink),
// interpolate the shape's INTRINSIC parameters: each edge's length and each
// vertex's turning angle, then reconstruct the polyline by integrating.
// A limb bending at a joint keeps its arc length through the whole tween
// (measured on the elbow test: linear dipped to 189.5/200 at midpoint; the
// worse the fold, the worse linear gets, up to passing through itself).
// Used as a CORRECTION, not a replacement — see interpStroke below: the
// linear+rigid-fit result stays authoritative whenever it preserves length
// (translations, rigid rotations, gentle morphs measure deficit≈0 and are
// bit-identical to before); the intrinsic reconstruction blends in only in
// proportion to the measured arc-length deficit, plus a hard override when
// the linear inbetween SELF-INTERSECTS while neither keyframe does (a
// shape "knotting" mid-tween is never right). Both gates go to zero at the
// endpoints by construction, so keyframes are never altered.
function _wrapPI(a){while(a>Math.PI)a-=2*Math.PI;while(a<-Math.PI)a+=2*Math.PI;return a;}
function _segPolyLen(segs){var L=0;for(var i=1;i<segs.length;i++){var dx=segs[i].point[0]-segs[i-1].point[0],dy=segs[i].point[1]-segs[i-1].point[1];L+=Math.sqrt(dx*dx+dy*dy);}return L;}
// Polyline self-intersection on the sample points (handles ignored — at
// resamplePts density the polyline is a faithful proxy). O(n²) segment
// pairs, adjacent pairs skipped. Pairs sharing an endpoint are skipped
// explicitly (_nearPt): a closed resample duplicates its first/last point,
// and the seam pair (segment 0 vs segment n-1) meets AT that shared point —
// _segsIntersect's strict ccw test is unreliable exactly there (float
// noise on near-collinear neighbors), measured as a systematic false
// positive on every closed circle, which would have permanently disarmed
// the self-crossing guard for closed shapes (both keyframes "self-
// intersect" → guard never fires).
function _nearPt(ax,ay,bx,by){var dx=ax-bx,dy=ay-by;return dx*dx+dy*dy<1e-9;}
// ---- STROKE-STYLE CARRY-THROUGH ("même style de trait sur chaque tween") ----
// The resample stage (resampleP/resamplePairFeatureAware) only copies the
// core fields (strokeColor/Width/Cap/Join, fillColor, opacity) — every
// OTHER style attribute serP persists and desP faithfully re-applies was
// silently dropped from generated inbetweens: a dashed/dotted stroke's
// tweens rendered SOLID, a gradient fill fell back to its flat fillColor,
// paintOrder flipped to default, miterLimit reset, per-element effects
// vanished, a Shadow-Brush guide lost its channelTag (turning into a
// permanent normal stroke on every inbetween), and hasRealStroke was lost
// (letting serP's phantom '#ffffff' render as a real white contour on
// fill-only shapes mid-tween). Numeric fields lerp (dashOffset,
// miterLimit; gradients lerp geometry+stops when both sides share the
// same structure); discrete fields switch at the halfway point — the same
// convention interpStroke already uses for strokeColor/fillColor.
function _lerpHexColor(a,b,t){
  var pa=parseHexColor(a),pb=parseHexColor(b);
  if(!pa||!pb)return t<.5?a:b;
  function aOf(h){h=String(h).replace('#','');return h.length===8?parseInt(h.substr(6,2),16):255;}
  var al=Math.round(lerp(aOf(a),aOf(b),t));
  function h2(v){v=Math.max(0,Math.min(255,Math.round(v)));var s=v.toString(16);return s.length<2?'0'+s:s;}
  return '#'+h2(lerp(pa.r,pb.r,t))+h2(lerp(pa.g,pb.g,t))+h2(lerp(pa.b,pb.b,t))+(al<255?h2(al):'');
}
function _carryTweenStyle(out,srcA,srcB,et){
  srcA=srcA||{};srcB=srcB||{};
  var near=et<.5?srcA:srcB;
  if(near.dashArray&&near.dashArray.length)out.dashArray=near.dashArray.slice();
  var doA=srcA.dashOffset,doB=srcB.dashOffset;
  if(doA!==undefined||doB!==undefined)out.dashOffset=(typeof doA==='number'&&typeof doB==='number')?lerp(doA,doB,et):(et<.5?doA:doB);
  var mlA=srcA.miterLimit,mlB=srcB.miterLimit;
  if(mlA!==undefined||mlB!==undefined)out.miterLimit=(typeof mlA==='number'&&typeof mlB==='number')?lerp(mlA,mlB,et):(et<.5?mlA:mlB);
  if(near.paintOrder)out.paintOrder=near.paintOrder;
  if(near.channelTag)out.channelTag=near.channelTag;
  if(near.groupId)out.groupId=near.groupId;
  if(near.effects&&near.effects.length)out.effects=JSON.parse(JSON.stringify(near.effects));
  if(near.hasRealStroke!==undefined)out.hasRealStroke=near.hasRealStroke;
  var gA=srcA.fillGradient,gB=srcB.fillGradient;
  if(gA&&gB&&gA.kind===gB.kind&&gA.stops&&gB.stops&&gA.stops.length===gB.stops.length){
    out.fillGradient={kind:gA.kind,
      from:[lerp(gA.from[0],gB.from[0],et),lerp(gA.from[1],gB.from[1],et)],
      to:[lerp(gA.to[0],gB.to[0],et),lerp(gA.to[1],gB.to[1],et)],
      stops:gA.stops.map(function(s,i){return{offset:lerp(s.offset,gB.stops[i].offset,et),color:_lerpHexColor(s.color,gB.stops[i].color,et)};})};
  }else if(gA||gB){var g=et<.5?gA:gB;if(g)out.fillGradient=JSON.parse(JSON.stringify(g));}
  return out;
}
// Polyline length straight off stroke DATA (centerline preferred, same
// dispatch as buildTPFeat) — cheap probe for the adaptive resample budget.
function _strokeDataPolyLen(sd){
  var segs=(sd.isVectorBrush&&sd.centerSegments&&sd.centerSegments.length>1)?sd.centerSegments:sd.segments;
  if(!segs||segs.length<2)return 0;
  var L=0;for(var i=1;i<segs.length;i++)L+=Math.hypot(segs[i].point[0]-segs[i-1].point[0],segs[i].point[1]-segs[i-1].point[1]);
  return L;
}
function _segsSelfIntersect(segs){
  var n=segs.length-1;
  if(n<3)return false;
  for(var i=0;i<n;i++){
    var p1={x:segs[i].point[0],y:segs[i].point[1]},p2={x:segs[i+1].point[0],y:segs[i+1].point[1]};
    for(var j=i+2;j<n;j++){
      var p3={x:segs[j].point[0],y:segs[j].point[1]},p4={x:segs[j+1].point[0],y:segs[j+1].point[1]};
      if(_nearPt(p1.x,p1.y,p3.x,p3.y)||_nearPt(p1.x,p1.y,p4.x,p4.y)||_nearPt(p2.x,p2.y,p3.x,p3.y)||_nearPt(p2.x,p2.y,p4.x,p4.y))continue;
      if(_segsIntersect(p1,p2,p3,p4))return true;
    }
  }
  return false;
}
// Full crossing COUNT (same adjacency/endpoint exclusions as the boolean
// test above) — the tangle-strategy probe needs "how badly does this
// candidate cross" relative to the keyframes' own legitimate crossings
// (a hand drawn as a loop over its arm crosses ONCE in the keys
// themselves), not just "does it cross at all".
function _segsSelfXCount(segs){
  var n=segs.length-1;
  if(n<3)return 0;
  var c=0;
  for(var i=0;i<n;i++){
    var p1={x:segs[i].point[0],y:segs[i].point[1]},p2={x:segs[i+1].point[0],y:segs[i+1].point[1]};
    for(var j=i+2;j<n;j++){
      var p3={x:segs[j].point[0],y:segs[j].point[1]},p4={x:segs[j+1].point[0],y:segs[j+1].point[1]};
      if(_nearPt(p1.x,p1.y,p3.x,p3.y)||_nearPt(p1.x,p1.y,p4.x,p4.y)||_nearPt(p2.x,p2.y,p3.x,p3.y)||_nearPt(p2.x,p2.y,p4.x,p4.y))continue;
      if(_segsIntersect(p1,p2,p3,p4))c++;
    }
  }
  return c;
}
function _intrinsicSegs(rA,rB,et,cx2,cy2,closed){
  var A=rA.segments,B=rB.segments,n=Math.min(A.length,B.length);
  if(n<3)return null;
  // Interpolate: first edge's absolute direction takes the shortest signed
  // way (this carries the shape's global rotation, replacing the rigid fit
  // for the blended-in portion); every subsequent edge interpolates its
  // TURNING relative to the previous edge, so each local bend opens/closes
  // independently of the global spin. Edge lengths lerp directly — that's
  // the whole length-preservation property.
  var lens=[],dirs=[],prevA=0,prevB=0,dir=0;
  for(var i=1;i<n;i++){
    var ax=A[i].point[0]-A[i-1].point[0],ay=A[i].point[1]-A[i-1].point[1];
    var bx=B[i].point[0]-B[i-1].point[0],by=B[i].point[1]-B[i-1].point[1];
    var aa=Math.atan2(ay,ax),ab=Math.atan2(by,bx);
    if(i===1)dir=aa+_wrapPI(ab-aa)*et;
    else dir+=lerp(_wrapPI(aa-prevA),_wrapPI(ab-prevB),et);
    prevA=aa;prevB=ab;
    lens.push(lerp(Math.sqrt(ax*ax+ay*ay),Math.sqrt(bx*bx+by*by),et));dirs.push(dir);
  }
  var m=lens.length;
  var pts=[[0,0]];
  for(var e=0;e<m;e++)pts.push([pts[e][0]+lens[e]*Math.cos(dirs[e]),pts[e][1]+lens[e]*Math.sin(dirs[e])]);
  // Closed loops: mid-tween the integrated walk doesn't land exactly back
  // on its start — distribute the closure error along arc length
  // (Sederberg's fix). Zero at t=0/1 since both keyframes close exactly.
  if(closed){
    var ex=pts[m][0]-pts[0][0],ey=pts[m][1]-pts[0][1];
    var total=0;for(var q=0;q<m;q++)total+=lens[q];
    if(total>1e-9){var acc=0;for(var p5=1;p5<=m;p5++){acc+=lens[p5-1];var f=acc/total;pts[p5][0]-=ex*f;pts[p5][1]-=ey*f;}}
  }
  // Center on centroid, then place on the same arc-path position (cx2,cy2)
  // the linear result uses — motion-arc handles keep working identically.
  var mx=0,my=0;for(var c=0;c<n;c++){mx+=pts[c][0];my+=pts[c][1];}mx/=n;my/=n;
  var out=[];
  for(var s2=0;s2<n;s2++){
    // handles from the incoming/outgoing edge vectors (independent in/out —
    // preserves corners instead of central-averaging them round)
    var hIn=s2>0?[-(pts[s2][0]-pts[s2-1][0])/3,-(pts[s2][1]-pts[s2-1][1])/3]:[0,0];
    var hOut=s2<m?[(pts[s2+1][0]-pts[s2][0])/3,(pts[s2+1][1]-pts[s2][1])/3]:[0,0];
    out.push({point:[pts[s2][0]-mx+cx2,pts[s2][1]-my+cy2],handleIn:hIn,handleOut:hOut});
  }
  return out;
}
// ---- MLS-STYLE LOCALLY-WEIGHTED RIGID DEFORMATION (2026-07) ----
// Third interpolation engine, alongside the whole-stroke rigid blend
// (buildLinearSegs, inside interpStroke) and the intrinsic arc-length/
// turning-angle reconstruction (_intrinsicSegs above) — kept as genuine
// alternative candidates in the tangle-strategy arbitration, not a
// replacement (Cyril: "garde les 2 autres moteurs... si on doit switch...
// coupler si besoin"). Where the whole-stroke fit forces ONE rotation+
// scale onto every vertex regardless of how far it sits from the fit's
// own best-explained region (Cyril's diagnosis, live: "des plis qui
// deviennent bizarres parce qu'éloignés" — a fold far from the dominant
// motion the global fit picked up has no local say in its own transform),
// this fits a SEPARATE rotation+scale per vertex from nearby correspondence
// pairs, weighted by distance — Moving Least Squares rigid deformation
// (Schaefer/McPhail/Warren 2006's image-deformation technique, applied
// here to a stroke's own DTW-corresponded points as handles instead of
// artist-placed control points). A shoulder's own local motion no longer
// has to explain a finger's.
//
// Handles are a SUBSET of the already-corresponded points (not all n —
// O(n·H) instead of O(n²), cheap: n≤150, H≤24 measured under 2ms): a
// uniform-by-index baseline for full coverage, TOPPED UP with extra
// handles at detected corners (2026-07) so a region with several tight
// folds close together isn't left as under-sampled as a straight run —
// see _mlsHandleIndices' own comment. Weight kernel is the standard MLS
// inverse-square falloff, distance measured along rA's own arc length
// (index-paired with rB, so the SAME metric weights both the forward-
// from-A and backward-from-B fits — deliberately, so the two stay
// consistent with each other instead of drifting apart under two
// unrelated distance metrics).
// Baseline: up to 24 handles spread uniformly by INDEX (full coverage, no
// gap can go handle-less even on a featureless curve) — SAME total budget
// as before, so this never loses resolution vs. the plain-uniform version.
function _mlsHandleIndices(rA,rB){
  var n=Math.min(rA.segments.length,rB.segments.length);
  var H=Math.min(24,n);
  var seen={},handleIdx=[];
  for(var h=0;h<H;h++){
    var ix=Math.round(h*(n-1)/Math.max(1,H-1));
    if(!seen[ix]){seen[ix]=1;handleIdx.push(ix);}
  }
  // Corner-anchored handles (2026-07, "poignées ancrées sur les points de
  // repère"): plain uniform spacing can land its nearest sample a few
  // vertices AWAY from an actual fold, blurring MLS's local fit exactly
  // where precision matters most. Rather than ADDING extra handles (tried
  // first, measured: it shrank the effective uniform coverage for the
  // same total budget and cost the confirmed testD win), each detected
  // corner SNAPS its single nearest existing uniform handle onto itself —
  // same total handle count, same coverage guarantee, just relocated to
  // sit exactly on the fold instead of near it. "Corner" = cross-keyframe
  // agreement (max turn at this vertex in EITHER rA or rB — the same
  // signal candScore's fold-angle term trusts), snapped strongest-first so
  // two nearby corners can't both claim the same handle.
  if(n>=5){
    var cum=[0];for(var ci=1;ci<n;ci++)cum.push(cum[ci-1]+Math.hypot(rA.segments[ci].point[0]-rA.segments[ci-1].point[0],rA.segments[ci].point[1]-rA.segments[ci-1].point[1]));
    function turnMag(pts,i){
      var p0=pts[i-1].point,p1=pts[i].point,p2=pts[i+1].point;
      var a1=Math.atan2(p1[1]-p0[1],p1[0]-p0[0]),a2=Math.atan2(p2[1]-p1[1],p2[0]-p1[0]);
      return Math.abs(_wrapPI(a2-a1));
    }
    var corners=[];
    for(var ii=1;ii<n-1;ii++){
      var m=Math.max(turnMag(rA.segments,ii),turnMag(rB.segments,ii));
      if(m>0.5)corners.push({i:ii,m:m});
    }
    corners.sort(function(a,b){return b.m-a.m;});
    var claimed={};
    for(var cj=0;cj<corners.length;cj++){
      var cand=corners[cj].i;
      if(seen[cand])continue; // already a handle
      var bestHk=-1,bestD=Infinity;
      for(var hk=0;hk<handleIdx.length;hk++){
        if(claimed[hk])continue; // this handle already snapped to a stronger corner
        var d=Math.abs(cum[cand]-cum[handleIdx[hk]]);
        if(d<bestD){bestD=d;bestHk=hk;}
      }
      if(bestHk<0)continue;
      claimed[bestHk]=1;
      delete seen[handleIdx[bestHk]];
      handleIdx[bestHk]=cand;
      seen[cand]=1;
    }
    handleIdx.sort(function(a,b){return a-b;});
  }
  return handleIdx;
}
// Per-vertex weighted similarity fit, meant to be cached ONCE per pair
// (like _twIwProbe) — not re-fit every frame, only re-EVALUATED at the
// current et via buildMLSSegs below.
function _fitMLSPerVertex(rA,rB){
  var n=Math.min(rA.segments.length,rB.segments.length);
  var handleIdx=_mlsHandleIndices(rA,rB);
  var H=handleIdx.length;
  if(H<3)return null;
  var cum=[0];for(var i=1;i<n;i++)cum.push(cum[i-1]+Math.hypot(rA.segments[i].point[0]-rA.segments[i-1].point[0],rA.segments[i].point[1]-rA.segments[i-1].point[1]));
  var totalLen=cum[n-1]||1;
  var sigma=Math.max(1,totalLen/H*0.75); // ~3/4 of the average handle gap
  var handlesA=handleIdx.map(function(ix){return {x:rA.segments[ix].point[0],y:rA.segments[ix].point[1]};});
  var handlesB=handleIdx.map(function(ix){return {x:rB.segments[ix].point[0],y:rB.segments[ix].point[1]};});
  var per=new Array(n);
  for(var qi=0;qi<n;qi++){
    var w=new Array(H);
    for(var h2=0;h2<H;h2++){
      var d=Math.abs(cum[qi]-cum[handleIdx[h2]]);
      w[h2]=1/((d*d)/(sigma*sigma)+0.01);
    }
    var t=fitSimilarityTransformWeighted(handlesA,handlesB,w);
    if(!t){
      // Degenerate (near-zero variance under these weights) — identity
      // fallback keeps this vertex on a straight lerp instead of blowing
      // up, same spirit as fitSimilarityTransform's own null-guard above.
      per[qi]={theta:0,scale:1,pcx:rA.segments[qi].point[0],pcy:rA.segments[qi].point[1],qcx:rB.segments[qi].point[0],qcy:rB.segments[qi].point[1]};
      continue;
    }
    var mag=Math.sqrt(t.wRe*t.wRe+t.wIm*t.wIm);
    var th=Math.atan2(t.wIm,t.wRe);
    per[qi]={theta:(mag>0.1&&mag<10)?th:0,scale:Math.min(3,Math.max(0.33,mag||1)),pcx:t.ca.x,pcy:t.ca.y,qcx:t.cb.x,qcy:t.cb.y};
  }
  return per;
}
// Evaluates the MLS candidate at a given et — same fwd(A)/bwd(B) blend
// SHAPE as buildLinearSegs (identity transform at each side's own
// boundary), so it's endpoint-exact by the identical construction, just
// with a PER-VERTEX theta/scale/pivot from _fitMLSPerVertex instead of
// one shared set for the whole stroke.
function buildMLSSegs(rA,rB,per,etv){
  var n=per.length,o=new Array(n);
  for(var i=0;i<n;i++){
    var pv=per[i];
    var invScale=pv.scale>1e-6?1/pv.scale:1;
    var thT=pv.theta*etv,scT=1+(pv.scale-1)*etv;
    var thB=thT-pv.theta,scB=invScale+(1-invScale)*etv;
    var sA=rA.segments[i],sB=rB.segments[i];
    var fwd=rotScalePt(sA.point[0]-pv.pcx,sA.point[1]-pv.pcy,thT,scT);
    var bwd=rotScalePt(sB.point[0]-pv.qcx,sB.point[1]-pv.qcy,thB,scB);
    var hiF=rotScalePt(sA.handleIn[0],sA.handleIn[1],thT,scT),hiB=rotScalePt(sB.handleIn[0],sB.handleIn[1],thB,scB);
    var hoF=rotScalePt(sA.handleOut[0],sA.handleOut[1],thT,scT),hoB=rotScalePt(sB.handleOut[0],sB.handleOut[1],thB,scB);
    o[i]={point:[lerp(pv.pcx+fwd[0],pv.qcx+bwd[0],etv),lerp(pv.pcy+fwd[1],pv.qcy+bwd[1],etv)],handleIn:[lerp(hiF[0],hiB[0],etv),lerp(hiF[1],hiB[1],etv)],handleOut:[lerp(hoF[0],hoB[0],etv),lerp(hoF[1],hoB[1],etv)]};
  }
  return o;
}
// ---- FOLD-CORRECTION PASS (2026-07) ----
// Cyril: "une passe de correction qui regarde si le tween respecte bien
// les écarts de formes faites et remodule aussi" — a genuine SECOND pass
// on the winning candidate, not just picking the best of several fixed
// ones (what the tangle-strategy arbitration in interpStroke already
// does). Classic ARAP alternates a LOCAL step (fit per-cell rotations)
// and a GLOBAL step (solve positions honoring those rotations) to
// convergence; this is a lightweight surrogate for that global step — a
// bounded, damped nudge of each genuine fold vertex (rA._twFoldW, the
// same signal the arbitration's own fold-angle score already trusts)
// toward the turning angle its OWN two keyframes agree on AT THIS et
// (rA._twFoldThA/_twFoldThB, cached once per pair — see where they're
// set, right after _foldW above), re-measuring self-crossings after
// every round and discarding the round outright if it made tangling
// WORSE — verify, then commit, never applied blind.
//
// Endpoint-exactness note: at et=0/et=1 the winning candidate already
// reproduces rA/rB exactly (by construction, verified across every
// canonical fixture), so the "actual" turn measured here already equals
// thetaA[i]/thetaB[i] to float precision — delta comes out ~0 and the
// 0.01rad skip threshold below no-ops the correction at both ends, same
// guarantee as every other candidate.
function _applyFoldCorrection(segs,rA,et){
  if(!TW_CORRECTION_PASS)return segs;
  var thA=rA._twFoldThA,thB=rA._twFoldThB,w=rA._twFoldW;
  if(!thA)return segs;
  var n2=segs.length;
  var baseX=_segsSelfXCount(segs);
  var cur=segs;
  var ITERS=2,DAMP=0.5,MAXROT=0.26; // ~15° cap — small deliberate nudges only
  for(var it=0;it<ITERS;it++){
    var raw=new Array(n2);
    for(var i=1;i<n2-1;i++){
      var wv=w[i];
      if(!wv||wv<0.35){raw[i]=0;continue;} // ~20°+ turn only — genuine corners, not noise (same bar as the scoring term)
      var expTheta=thA[i]+_wrapPI(thB[i]-thA[i])*et;
      var p0=cur[i-1].point,p1=cur[i].point,p2=cur[i+1].point;
      var a1=Math.atan2(p1[1]-p0[1],p1[0]-p0[0]),a2=Math.atan2(p2[1]-p1[1],p2[0]-p1[0]);
      var actual=_wrapPI(a2-a1);
      var d=_wrapPI(expTheta-actual)*DAMP;
      if(d>MAXROT)d=MAXROT;else if(d<-MAXROT)d=-MAXROT;
      raw[i]=d;
    }
    raw[0]=0;raw[n2-1]=0;
    // Smooth the raw per-vertex correction across its 2 immediate
    // neighbors (2026-07, live-reported "cassure" artifact): a single
    // vertex's turning angle is a noisy 3-point measurement on a hand-
    // drawn stroke — correcting it ALONE, with its neighbors left
    // untouched, reads as a sharp spike popped out of an otherwise smooth
    // curve. Tapering the same correction into 2 flanking vertices (a
    // 25%/50%/25% kernel) means a real, wide corner still gets corrected
    // (its own strong raw delta dominates the sum), but a single noisy
    // outlier gets diluted into an unnoticeable ripple instead of a kink.
    var sm=new Array(n2),changed=false;
    for(var i2=1;i2<n2-1;i2++){
      var v=raw[i2]*0.5+(i2>1?raw[i2-1]*0.25:0)+(i2<n2-2?raw[i2+1]*0.25:0);
      sm[i2]=v;
      if(Math.abs(v)>=0.01)changed=true;
    }
    if(!changed)break;
    var next=cur.map(function(s){return{point:s.point.slice(),handleIn:s.handleIn.slice(),handleOut:s.handleOut.slice()};});
    for(var i3=1;i3<n2-1;i3++){
      var d3=sm[i3];
      if(Math.abs(d3)<0.01)continue;
      var p0b=cur[i3-1].point,p1b=cur[i3].point,p2b=cur[i3+1].point;
      var mx=(p0b[0]+p2b[0])/2,my=(p0b[1]+p2b[1])/2;
      var dx=p1b[0]-mx,dy=p1b[1]-my;
      var cs=Math.cos(d3),sn=Math.sin(d3);
      next[i3].point=[mx+dx*cs-dy*sn,my+dx*sn+dy*cs];
    }
    var newX=_segsSelfXCount(next);
    if(newX>baseX)break; // this round tangled more than the uncorrected baseline — stop, keep the last accepted state
    cur=next;
  }
  return cur;
}
// ---- POINT-REDUCTION PASS (2026-07) ----
// Cyril: "j'ai encore beaucoup de point de vertex par rapport à mes
// dessins originaux... peut être avoir une réduction des points" — the
// correspondence stage deliberately resamples both keyframes to a FINE
// shared point count (up to 150, adaptive to arc length) so DTW/fold
// matching has enough resolution to work with; that resolution has no
// reason to survive into the rendered OUTPUT, where it reads as
// noticeably smoother/more "computed" than the artist's own hand-drawn
// density. Runs ONCE per pair (cached on rA._twKeepIdx, same pattern as
// every other per-pair decision in this file) — the SAME retained index
// set is reused for every frame in the span, so the point count never
// jitters frame-to-frame. A genuine fold/corner (rA._twFoldW, the same
// signal the correction pass and MLS handle-snapping already trust) is
// PROTECTED from removal, so simplification can never erase it.
//
// Vector-brush ribbons are excluded (see the call site) — their width
// channel is indexed 1:1 with the centerline, and reducing one without
// the other would desync stroke tapering; a separate pass if that's ever
// wanted.
//
// Algorithm: Visvalingam-Whyatt (repeatedly drop the point whose removal
// changes the polyline LEAST — twice the triangle area formed with its
// current neighbors, the standard measure of a point's visual
// contribution), scored against BOTH keyframes and taking the max, so a
// point insignificant in A's pose but a real corner in B's isn't dropped.
function _decimateKeepIndices(rA,rB,targetN){
  var n=Math.min(rA.segments.length,rB.segments.length);
  if(targetN>=n)return null;
  var w=rA._twFoldW;
  var prev=new Array(n),next=new Array(n),alive=new Array(n);
  for(var i=0;i<n;i++){prev[i]=i-1;next[i]=i+1;alive[i]=true;}
  var protect=new Array(n);
  for(var i2=0;i2<n;i2++)protect[i2]=(i2===0||i2===n-1)||(w&&w[i2]>=0.35);
  function area(pts,pi,ci,ni){
    var p0=pts[pi].point,p1=pts[ci].point,p2=pts[ni].point;
    return Math.abs((p1[0]-p0[0])*(p2[1]-p0[1])-(p2[0]-p0[0])*(p1[1]-p0[1]));
  }
  var remaining=n;
  while(remaining>targetN){
    var worstI=-1,worstScore=Infinity;
    for(var i3=1;i3<n-1;i3++){
      if(!alive[i3]||protect[i3])continue;
      var pi=prev[i3],ni=next[i3];
      var score=Math.max(area(rA.segments,pi,i3,ni),area(rB.segments,pi,i3,ni));
      if(score<worstScore){worstScore=score;worstI=i3;}
    }
    if(worstI<0)break; // nothing left to remove without touching a protected corner
    alive[worstI]=false;
    next[prev[worstI]]=next[worstI];
    prev[next[worstI]]=prev[worstI];
    remaining--;
  }
  var idx=[];for(var i4=0;i4<n;i4++)if(alive[i4])idx.push(i4);
  return idx;
}
// Rebuilds smooth Catmull-Rom-style handles for the reduced point set —
// the OLD handles were sized for the dense pre-reduction spacing and
// would badly overshoot the new, sparser gaps.
function _reduceSegs(segs,keepIdx){
  var m=keepIdx.length,out=new Array(m);
  for(var k=0;k<m;k++){
    var idx=keepIdx[k];
    var p1=segs[idx].point;
    var pPrev=k>0?segs[keepIdx[k-1]].point:p1;
    var pNext=k<m-1?segs[keepIdx[k+1]].point:p1;
    var tx=pNext[0]-pPrev[0],ty=pNext[1]-pPrev[1];
    var tlen=Math.hypot(tx,ty)||1;
    tx/=tlen;ty/=tlen;
    var distPrev=k>0?Math.hypot(p1[0]-pPrev[0],p1[1]-pPrev[1]):0;
    var distNext=k<m-1?Math.hypot(pNext[0]-p1[0],pNext[1]-p1[1]):0;
    out[k]={point:p1,handleIn:k===0?[0,0]:[-tx*distPrev/3,-ty*distPrev/3],handleOut:k===m-1?[0,0]:[tx*distNext/3,ty*distNext/3]};
  }
  return out;
}
// Safety check (2026-07, live-reported regression: reduction introduced
// self-crossings on a stroke that had NONE at full resolution): removing
// points can change a polyline's self-intersection topology either way —
// a locally-wiggly run that keeps two far-apart regions from touching
// can get straightened into a crossing. Since the kept-index SET is a
// single per-pair decision reused for the whole span (needed so point
// count doesn't jitter frame-to-frame), it can't be verified/rejected
// per-frame the way the correction pass verifies every round — instead,
// probe it eagerly with a cheap LINEAR proxy (plain lerp of rA/rB's own
// points, not the full MLS/blend pipeline) at a few et samples across the
// span; reject the whole reduction for this pair if it's worse anywhere.
function _reductionIsSafe(rA,rB,keepIdx,n0){
  var PROBES=[0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9]; // dense — a cheap linear proxy, one-time per pair
  for(var p=0;p<PROBES.length;p++){
    var et=PROBES[p],full=new Array(n0);
    for(var i=0;i<n0;i++){
      var pa=rA.segments[i].point,pb=rB.segments[i].point;
      full[i]={point:[lerp(pa[0],pb[0],et),lerp(pa[1],pb[1],et)]};
    }
    var red=new Array(keepIdx.length);
    for(var k=0;k<keepIdx.length;k++)red[k]=full[keepIdx[k]];
    if(_segsSelfXCount(red)>_segsSelfXCount(full))return false;
  }
  return true;
}
function _applyPointReduction(segs,rA,rB){
  if(!TW_POINT_REDUCTION)return segs;
  if(rA._twKeepIdx===undefined){
    rA._twKeepIdx=null;
    if(rA._src&&rB._src){
      var n0=Math.min(rA.segments.length,rB.segments.length);
      var origN=Math.round(((rA._src.segments||[]).length+(rB._src.segments||[]).length)/2);
      var cand=_decimateKeepIndices(rA,rB,Math.max(10,origN));
      if(cand&&_reductionIsSafe(rA,rB,cand,n0))rA._twKeepIdx=cand;
    }
  }
  if(!rA._twKeepIdx)return segs;
  return _reduceSegs(segs,rA._twKeepIdx);
}
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
        // Damp the rotation when the two centroids sit far apart relative
        // to the shape's own size (2026-07, "des lignes de force font des
        // aller-retour" — part of the same fix as the iw-probe stabilizer
        // below; that one alone wasn't enough, confirmed live: disabling
        // rotation ENTIRELY also independently removed a residual
        // reversal the intrinsic-only fix didn't reach). fwd rotates
        // around cxA, bwd rotates around cxB — two DIFFERENT pivots, only
        // ever recentered onto the same final point (cx2) AFTER the lerp.
        // When the pivots are close (a shape spinning near its own place —
        // the 90°-rectangle regression case measures ~0 here), fwd's and
        // bwd's arcs are nearly the same arc and blend cleanly. When far
        // apart (a limb SWINGING to a new position, not spinning in
        // place — measured on the reported case: centroids 146px apart,
        // avg point radius ~105px, ratio ~1.4), fwd's arc (around A's old
        // spot) and bwd's arc (around B's new spot) are geometrically
        // different curves; scalar-lerping two different curves can
        // produce a combined path that isn't itself monotonic in any
        // direction even though et and each individual arc are. Ramp:
        // full rotation kept below a 0.6x size:separation ratio, fully
        // suppressed above 2x (falls back to the straight lerp path,
        // still shape-correct via the intrinsic correction above).
        var avgR=0;for(var ri=0;ri<n;ri++)avgR+=(Math.hypot(loA[ri].x,loA[ri].y)+Math.hypot(loB[ri].x,loB[ri].y))/2;avgR/=n;
        var centroidDist=Math.hypot(cxB-cxA,cyB-cyA);
        var sepRatio=centroidDist/Math.max(1,avgR);
        var rotTrust=Math.max(0,Math.min(1,1-(sepRatio-0.6)/1.4));
        theta*=rotTrust;
        // EMPIRICAL rotation-crossing check (2026-07, closed-shape tangle
        // — a re-drawn hand pair measured 13 self-crossings across the
        // span with no safety net catching it): the geometric damping
        // above (centroid-separation ratio) doesn't measure actual self-
        // crossing — a closed shape whose vertices sit unevenly around
        // its own centroid can still tangle under a "trusted" rotation,
        // and this pair never even reaches the tangle-strategy probe
        // further down (that one's gated on intrinsic weight, which
        // CORRESPONDENCE-TRUST already zeroed here — turnTrust 0,
        // mean per-vertex disagreement too high to trust the fold
        // reconstruction at all, pure rigid+linear is genuinely the
        // best available candidate, it just still crosses). Cached once
        // per pair: build the SAME fwd/bwd rigid blend at 5 et samples
        // with vs without theta, count crossings above the keys' own
        // baseline, and fully zero theta if de-rotating measurably helps
        // — binary, not graduated, because this is a direct measurement
        // of the failure (unlike sepRatio's indirect proxy), not a
        // heuristic that needs a soft ramp to avoid false positives.
        if(rA._twRotXDamp===undefined){
          rA._twRotXDamp=1;
          if(theta!==0&&n>=5){
            (function(){
              function buildAt(etv,th2){
                var thT2=th2*etv,scT2=lerp(1,scaleF,etv),thB2=thT2-th2,scB2=lerp(scaleF>1e-6?1/scaleF:1,1,etv);
                var cxx=cxA+(cxB-cxA)*etv,cyy=cyA+(cyB-cyA)*etv;
                var o=[];
                for(var bi5=0;bi5<n;bi5++){
                  var fwd5=rotScalePt(rA.segments[bi5].point[0]-cxA,rA.segments[bi5].point[1]-cyA,thT2,scT2);
                  var bwd5=rotScalePt(rB.segments[bi5].point[0]-cxB,rB.segments[bi5].point[1]-cyB,thB2,scB2);
                  o.push({point:[cxx+lerp(fwd5[0],bwd5[0],etv),cyy+lerp(fwd5[1],bwd5[1],etv)]});
                }
                return o;
              }
              var baseX=Math.max(_segsSelfXCount(rA.segments),_segsSelfXCount(rB.segments));
              var withX=0,withoutX=0;
              [0.15,0.35,0.5,0.65,0.85].forEach(function(etp){
                withX+=Math.max(0,_segsSelfXCount(buildAt(etp,theta))-baseX);
                withoutX+=Math.max(0,_segsSelfXCount(buildAt(etp,0))-baseX);
              });
              if(withX>withoutX)rA._twRotXDamp=0;
            })();
          }
        }
        theta*=rA._twRotXDamp;
      }
    }
  }
  var thetaT=theta*et,scaleT=lerp(1,scaleF,et);
  var thetaB=thetaT-theta,scaleB=lerp(scaleF>1e-6?1/scaleF:1,1,et);
  // Factored out (2026-07, "des lignes de force font des aller-retour" —
  // see the iw-probe comment below for the full story) so the SAME blend
  // math can be evaluated at a fixed reference et, not just the real
  // per-frame one, without duplicating the loop body.
  function buildLinearSegs(etv,thT,scT,thB,scB,cxx,cyy){
    var o=[];
    for(var i2=0;i2<n;i2++){var sA=rA.segments[i2],sB=rB.segments[i2];
      var fwd=rotScalePt(sA.point[0]-cxA,sA.point[1]-cyA,thT,scT);
      var bwd=rotScalePt(sB.point[0]-cxB,sB.point[1]-cyB,thB,scB);
      var hiF=rotScalePt(sA.handleIn[0],sA.handleIn[1],thT,scT),hiB=rotScalePt(sB.handleIn[0],sB.handleIn[1],thB,scB);
      var hoF=rotScalePt(sA.handleOut[0],sA.handleOut[1],thT,scT),hoB=rotScalePt(sB.handleOut[0],sB.handleOut[1],thB,scB);
      o.push({point:[cxx+lerp(fwd[0],bwd[0],etv),cyy+lerp(fwd[1],bwd[1],etv)],handleIn:[lerp(hiF[0],hiB[0],etv),lerp(hiF[1],hiB[1],etv)],handleOut:[lerp(hoF[0],hoB[0],etv),lerp(hoF[1],hoB[1],etv)]});
    }
    return o;
  }
  segs=buildLinearSegs(et,thetaT,scaleT,thetaB,scaleB,cx2,cy2);
  // ---- intrinsic correction (see _intrinsicSegs) ----
  // Gate 1 (graduated): arc-length deficit of the linear result vs the
  // lerped keyframe lengths — 0 below 2% (translations, rigid rotations
  // and gentle morphs stay bit-identical to the historical output), full
  // weight at 10%+ (a real fold). Gate 2 (hard): the linear inbetween
  // self-intersects while neither keyframe does.
  //
  // Both gates are now measured ONCE per pair, at a FIXED et=0.5 probe,
  // not re-measured from THIS frame's own linear geometry on every call.
  // Found live ("des lignes de force font des aller-retour", traced to a
  // specific point reversing direction mid-span): fwd arcs around cxA,
  // bwd arcs around cxB — two DIFFERENT pivots — so segs' own arc length
  // is NOT guaranteed monotonic in et (scalar-lerping two differently-
  // pivoted arcs can shrink then grow then shrink again). Re-measuring the
  // deficit from that noisy length every frame let iw cross the activation
  // threshold in one direction then back within a couple of frames,
  // snapping the blend ~0→1→0 and dragging points toward the intrinsic
  // path then back — confirmed by direct trajectory tracing (the reversal
  // vanished with intrinsic correction disabled entirely) and by ruling
  // out the self-intersect override specifically (removing IT alone
  // changed nothing — the graduated deficit measure was noisy on its own).
  // A single et=0.5 probe is representative of the worst typical fold-
  // induced deficit for THIS pair and, being pair-constant, can only vary
  // per-frame through the smooth envelope below — never through re-
  // measured noise. (Translation doesn't affect arc length or self-
  // intersection, so the probe's own recentering point is irrelevant —
  // passed as 0,0.)
  if(n>=5){
    if(rA._twLen===undefined){rA._twLen=_segPolyLen(rA.segments);rA._twSelfX=_segsSelfIntersect(rA.segments);}
    if(rB._twLen===undefined){rB._twLen=_segPolyLen(rB.segments);rB._twSelfX=_segsSelfIntersect(rB.segments);}
    if(rA._twIwProbe===undefined){
      var thT5=theta*0.5,scT5=lerp(1,scaleF,0.5),thB5=thT5-theta,scB5=lerp(scaleF>1e-6?1/scaleF:1,1,0.5);
      var probeSegs=buildLinearSegs(0.5,thT5,scT5,thB5,scB5,0,0);
      var Lexp5=lerp(rA._twLen,rB._twLen,0.5);
      // |error|, not just shrink: a fold-onto-itself confuses the rigid
      // fit into OVERSHOOTING arc length (measured 215/200 on the 10°
      // V-fold — excess is exactly as wrong as deficit; an honest morph's
      // length lerps). Ramp: dead below 1.5% (hand wobble / resample
      // noise), full at 7% — the plain 90° elbow measures ~5.25%.
      var lenErr5=Lexp5>1e-6?Math.abs(Lexp5-_segPolyLen(probeSegs))/Lexp5:0;
      var iwP=Math.max(0,Math.min(1,(lenErr5-0.015)/0.055));
      if(iwP<1&&!rA._twSelfX&&!rB._twSelfX&&_segsSelfIntersect(probeSegs))iwP=1;
      // WINDOWED local deficit (2026-07, "un pli qu'on retrouve sur la clé
      // suivante" se perd pendant l'inter — a detailed fold on a LONG
      // stroke): the whole-stroke lenErr5 above dilutes a real local
      // collapse into insignificance — measured on a reported fist/finger
      // detail sitting on a ~1600px arm outline: whole-stroke lenErr5
      // only 1.2% (under the 1.5% dead zone, iwP stayed exactly 0, no
      // correction ever engaged), while a handful of ~15-vertex windows
      // covering just the fingers ran 4%+ short. A long straight run
      // elsewhere on the SAME stroke needs no correction and shouldn't
      // trigger one on its own account — this only fires from an actual
      // LOCAL deficit, and even then only opens the door: the existing
      // per-vertex local-trust/turn-trust gates below still decide which
      // vertices actually receive the correction, so this can't reproduce
      // the old whole-limb-ballooning failure the global gate was built
      // to prevent (2026-07, "encore un bras hyper déformé").
      if(iwP<1&&n>=20){
        var winW=Math.max(8,Math.round(n*0.1));
        var worstWinErr=0;
        for(var wi5=0;wi5+winW<n;wi5+=Math.max(2,Math.round(winW/3))){
          var wEnd=wi5+winW;
          var wLA=0,wLB=0,wLP=0;
          for(var wj=wi5+1;wj<=wEnd;wj++){
            wLA+=Math.hypot(rA.segments[wj].point[0]-rA.segments[wj-1].point[0],rA.segments[wj].point[1]-rA.segments[wj-1].point[1]);
            wLB+=Math.hypot(rB.segments[wj].point[0]-rB.segments[wj-1].point[0],rB.segments[wj].point[1]-rB.segments[wj-1].point[1]);
            wLP+=Math.hypot(probeSegs[wj].point[0]-probeSegs[wj-1].point[0],probeSegs[wj].point[1]-probeSegs[wj-1].point[1]);
          }
          var wExp=(wLA+wLB)/2;
          if(wExp<3)continue;
          var wErr=Math.abs(wExp-wLP)/wExp;
          if(wErr>worstWinErr)worstWinErr=wErr;
        }
        // Same dead-zone/full-weight shape as the whole-stroke ramp, just
        // a tighter window (local detail needs a smaller absolute % to
        // read as "collapsed" than the whole stroke does).
        var iwPWin=Math.max(0,Math.min(1,(worstWinErr-0.02)/0.05));
        if(iwPWin>iwP)iwP=iwPWin;
      }
      rA._twIwProbe=iwP;
      // PER-VERTEX local trust (2026-07, "encore aller retour" — the mean
      // turning-trust above missed this: a hand/limb-wide MEAN disagreement
      // can look fine — 150-vertex stroke, global mean well under 15° —
      // while a short run of vertices next to one bad correspondence
      // "kink" (measured: two edges at 44°/61° local disagreement,
      // isolated in an otherwise near-0° neighborhood) still integrates
      // into a real cumulative drift for THAT stretch. The tell isn't
      // disagreement in isolation, it's disagreement relative to how far
      // the vertex actually needs to go: a vertex whose own A→B net
      // displacement is small but whose intrinsic reconstruction excursion
      // (distance from the straight A↔B midpoint, probed at the same
      // fixed et=0.5 reference as iwP above) is LARGE in comparison is
      // being dragged through a wide loop only to return — and since iw is
      // a zero-at-both-ends envelope in et, "dragged out then back" is
      // exactly what reads as aller-retour. A genuine fold moves every
      // vertex a comparable amount (excursion≈netDisp, ratio~1); measured
      // on the reported case: the affected run sat at ratio 1.2-4.5×,
      // cleanly separated from the rest of the same stroke (≤1.0×).
      // Absolute-frame throughout (unlike the translation-invariant probes
      // above) since excursion-vs-netDisp is NOT translation-invariant —
      // recentered on the pair's own fixed midpoint, not the real per-
      // frame cx2/cy2, so this stays a once-per-pair constant like iwP.
      rA._twLocalTrust=null;
      if(iwP>0.001){
        var cx2m=(cxA+cxB)/2,cy2m=(cyA+cyB)/2;
        var probeI5=_intrinsicSegs(rA,rB,0.5,cx2m,cy2m,!!rA.closed);
        if(probeI5){
          var lt=new Array(n);
          for(var lv=0;lv<n;lv++){
            var pAv=rA.segments[lv].point,pBv=rB.segments[lv].point;
            var dxv=pBv[0]-pAv[0],dyv=pBv[1]-pAv[1];
            var netD=Math.hypot(dxv,dyv);
            var refMx=(pAv[0]+pBv[0])/2,refMy=(pAv[1]+pBv[1])/2;
            var exc=Math.hypot(probeI5[lv].point[0]-refMx,probeI5[lv].point[1]-refMy);
            var ratio=exc/Math.max(netD,2);
            var magTrust=Math.max(0,Math.min(1,(2.2-ratio)/(2.2-1.0)));
            // Second, complementary tell: a vertex with a SUBSTANTIAL net
            // A→B travel can still pass the magnitude check above (its
            // excursion is proportionally reasonable) yet still overshoot
            // PAST B or undershoot BEHIND A along its own direction of
            // travel — measured on a second reported vertex: netD 44px
            // (real, sizeable motion), excursion only 0.77x that (would
            // pass the ratio gate alone), but its progress fraction along
            // the A→B axis at the et=0.5 probe was 1.23-2.4 (i.e. 23-140%
            // PAST the destination) for several consecutive vertices — the
            // envelope's forced return to exactly B at t=1 then reads as a
            // rush-past-and-correct. Good (non-reversing) strokes measured
            // tightly within s∈[0.17,0.7] here; comfort zone [0,1] with a
            // 0.3 ramp gives real bends slack without missing this case.
            var axialTrust=1;
            if(netD>=2){
              var ux=dxv/netD,uy=dyv/netD;
              var s=((probeI5[lv].point[0]-pAv[0])*ux+(probeI5[lv].point[1]-pAv[1])*uy)/netD;
              var excess=Math.max(0,-s,s-1);
              axialTrust=Math.max(0,Math.min(1,1-excess/0.3));
            }
            lt[lv]=Math.min(magTrust,axialTrust);
          }
          // SPATIAL SMOOTHING (2026-07, "les bras sont complètement
          // déformés... beaucoup moins bien qu'hier" — regression caught
          // same-day): raw lt flips 1→0 within 2-3 vertices (measured
          // [1,1,.95,.58,.11,0,0...] on the reported stroke). Each vertex
          // then blends toward a DIFFERENT curve (intrinsic vs linear),
          // so the hard boundary physically tears the inbetween — the
          // trusted run follows the fold, its neighbor doesn't, and the
          // seam reads as a broken elbow (measured: max local turn angle
          // 62°→104° on the reported arm with the raw array; back to 62°
          // — the no-trust baseline — smoothed). Two passes: (1) min-
          // erode over ±1 so one lone trusted vertex inside a bad run
          // can't spike AND an isolated bad vertex keeps real suppression
          // after the blur dilutes it (measured: erode+blur kills all 3
          // residual reversing vertices on the reported file, blur alone
          // leaves 1), then (2) a triangular blur (radius 3 — swept 3/5/
          // n:16: radius 3 costs the least arc-length corner-cutting,
          // 7.7% vs 8.6% on the reported arm, with the kink equally
          // fixed) so the blend weight ramps over a real stretch of the
          // stroke instead of cliff-dropping between neighbors.
          var er=new Array(n);
          for(var ei=0;ei<n;ei++){
            var mn=lt[ei];
            if(ei>0&&lt[ei-1]<mn)mn=lt[ei-1];
            if(ei<n-1&&lt[ei+1]<mn)mn=lt[ei+1];
            er[ei]=mn;
          }
          var rad=3;
          var sm=new Array(n);
          for(var si=0;si<n;si++){
            var acc=0,wsum=0;
            for(var di=-rad;di<=rad;di++){
              var j=si+di;if(j<0||j>=n)continue;
              var w=rad+1-Math.abs(di);
              acc+=er[j]*w;wsum+=w;
            }
            sm[si]=acc/wsum;
          }
          rA._twLocalTrust=sm;
          // ---- TANGLE-STRATEGY PROBE (2026-07, "des formes qui se
          // tiennent pas dans leur forme" — arm-with-hand-loop stroke
          // tangling mid-tween): blending intrinsic and linear PER-VERTEX
          // mixes two geometrically incompatible curves, and a partial
          // mix can self-cross even where neither pure curve does
          // (measured on the reported stroke, crossings summed over 5
          // sample frames: per-vertex blend 17, uniform intrinsic 5, pure
          // linear 2 — keys themselves 0 and 1). So the per-vertex blend
          // is now a CANDIDATE, not a given: at this same et=0.5 probe,
          // count self-crossings of all three strategies and keep the
          // least-crossing one for the whole pair (ties keep the richer
          // strategy: blend over uniform over linear). Baseline is the
          // keys' own crossing count — a hand drawn as a loop over its
          // arm legitimately crosses once in the keys, and no strategy
          // should be punished for preserving it.
          if(rA._twTurnTrust===undefined){
            var sumTd0=0,cntTd0=0,pvA0=null,pvB0=null;
            for(var tp=1;tp<n;tp++){
              var taA0=Math.atan2(rA.segments[tp].point[1]-rA.segments[tp-1].point[1],rA.segments[tp].point[0]-rA.segments[tp-1].point[0]);
              var taB0=Math.atan2(rB.segments[tp].point[1]-rB.segments[tp-1].point[1],rB.segments[tp].point[0]-rB.segments[tp-1].point[0]);
              if(pvA0!==null){sumTd0+=Math.abs(_wrapPI(_wrapPI(taA0-pvA0)-_wrapPI(taB0-pvB0)));cntTd0++;}
              pvA0=taA0;pvB0=taB0;
            }
            var meanTd0=cntTd0?sumTd0/cntTd0:0;
            rA._twTurnTrust=Math.max(0,Math.min(1,(0.524-meanTd0)/(0.524-0.262)));
          }
          var iwPeak=iwP*rA._twTurnTrust;
          if(iwPeak>0.001){
            var base=Math.max(_segsSelfXCount(rA.segments),_segsSelfXCount(rB.segments));
            // Composite score, not crossings alone (2026-07, "des rotations
            // inattendues frame 21" — same-day follow-up): on an arm drawn
            // as an out-and-back contour (shoulder→hand→shoulder), the
            // uniform-intrinsic candidate crossed NOTHING at the probe yet
            // spun the whole stroke wildly off course — chord angle 9° at
            // the midframe where the keys' own chords (55°→92°) demand
            // ~74°, and 1082px wide vs 496/639 in the keys. Crossings
            // can't see that failure mode. Each candidate now also pays
            // for (a) chord-angle deviation from the keys' interpolated
            // chord — the direct measure of "unexpected rotation" — and
            // (b) arc-length error vs the lerped key lengths — the direct
            // measure of stretch/shrink (this is also exactly what the
            // intrinsic correction is FOR, so the blend keeps winning
            // wherever it genuinely preserves length: the folding-arm
            // case measures blend 7.7% vs linear 21% here and stays
            // blend). Weights, calibrated on the reported pair: chord
            // drift is perceptually the worst failure (a limb visibly
            // rotating the wrong way reads as broken even at correct
            // length), so 30° of drift ≡ 20% length error ≡ 1/10th of a
            // crossing — measured: uniform-intrinsic (65° drift, 0% len)
            // must lose to pure linear (0° drift, 15% len), while the
            // folding-arm blend (few-degree drift, 7.7% len) must still
            // beat ITS linear (0° drift, 21% len). Near-ties (<0.05) keep
            // the richer strategy so clean pairs stay bit-identical.
            function chord5(segs5){var a5=segs5[0].point,b5=segs5[segs5.length-1].point;return Math.atan2(b5[1]-a5[1],b5[0]-a5[0]);}
            var chordExp=(function(){
              var cA5=chord5(rA.segments),cB5=chord5(rB.segments);
              return cA5+_wrapPI(cB5-cA5)*0.5;
            })();
            // BACKTRACK term (2026-07, "sur le haut du bras à droite les
            // shapes font des aller-retour"): a candidate can be clean at
            // the midframe on every measure above yet still drag vertices
            // OUT and BACK over the span — the reported shoulder moved
            // 21px backward then 43px forward under uniform-intrinsic
            // while crossing nothing and holding chord/length perfectly
            // at et=0.5. That failure only shows in a TRAJECTORY, so each
            // candidate is now built at five et samples (real envelope,
            // real interpolated recenter — translation must be included
            // or purely-carried motion reads as false backtrack) plus the
            // two key endpoints, and pays for the total distance its
            // vertices travel in reverse. Sampling at the same five et
            // means crossings are also caught anywhere in the span now,
            // not only at the midframe.
            var ETS=[0.15,0.35,0.5,0.65,0.85];
            var candTraj={b:[rA.segments],u:[rA.segments],l:[rA.segments]};
            // MLS local-rigid (2026-07, "des plis qui deviennent bizarres
            // parce qu'éloignés" — see _fitMLSPerVertex's own header): a
            // FOURTH candidate, evaluated by the exact same scoring as the
            // three existing ones so the arbitration can pick whichever
            // actually performs best on THIS pair — "switch" per Cyril's
            // request, not a replacement (blend/uniform/linear are fully
            // intact above and still win whenever they score better).
            var mlsPerV=_fitMLSPerVertex(rA,rB);
            if(mlsPerV)candTraj.m=[rA.segments];
            var midIdx=2;
            var chordMid={},lenMid={};
            ETS.forEach(function(ets_,ei){
              var thTs=theta*ets_,scTs=lerp(1,scaleF,ets_),thBs=thTs-theta,scBs=lerp(scaleF>1e-6?1/scaleF:1,1,ets_);
              var cxs=cxA+(cxB-cxA)*ets_,cys=cyA+(cyB-cyA)*ets_;
              var linS=buildLinearSegs(ets_,thTs,scTs,thBs,scBs,cxs,cys);
              var intrS=_intrinsicSegs(rA,rB,ets_,cxs,cys,ets_<.5?!!rA.closed:!!rB.closed);
              var iwS=iwPeak*Math.sin(Math.PI*ets_);
              function mixS(w){
                var o=[];
                for(var mi5=0;mi5<n;mi5++){
                  var wv=(typeof w==='number'?w:w[mi5])*iwS;
                  o.push({point:intrS?[lerp(linS[mi5].point[0],intrS[mi5].point[0],wv),lerp(linS[mi5].point[1],intrS[mi5].point[1],wv)]:linS[mi5].point});
                }
                return o;
              }
              candTraj.b.push(mixS(sm));
              candTraj.u.push(mixS(1));
              candTraj.l.push(linS);
              if(mlsPerV)candTraj.m.push(buildMLSSegs(rA,rB,mlsPerV,ets_));
            });
            candTraj.b.push(rB.segments);candTraj.u.push(rB.segments);candTraj.l.push(rB.segments);
            if(mlsPerV)candTraj.m.push(rB.segments);
            // FOLD-CORRESPONDENCE term (2026-07, giving the MLS candidate a
            // fair trial without fighting the existing intrinsic
            // correction): chord/length above are inherently GLOBAL
            // metrics — a single whole-stroke rigid fit is, by definition,
            // the best possible explanation of the global chord and total
            // length, so it always looks best by those two measures alone,
            // even when a fold far from that fit's own dominant region is
            // locally wrong (exactly Cyril's "des plis qui deviennent
            // bizarres parce qu'éloignés"). A first attempt here scored
            // local segment-LENGTH deviation from the raw per-window
            // average of rA/rB — measured, it flipped the already-fixed
            // (PR167) arm/fold pair from blend to plain linear/rigid,
            // because a fold's length legitimately deviates from that
            // naive average AT the fold — that's what preserving it looks
            // like, not an error. TURNING ANGLE at each vertex doesn't
            // have that problem: rA.segments[i]/rB.segments[i] are already
            // the same vertex by DTW/landmark correspondence (Cyril's own
            // diagnosis — "peut être que les tangentes ne sont pas prises
            // en compte"), so wherever BOTH keyframes show a real
            // corner/fold at index i, the expected mid-turn is a direct
            // circular lerp of the two keyframes' own turn there — a
            // candidate that reproduces it is preserving the fold, one
            // that doesn't (flattens it, or turns the wrong way) is
            // exactly the visible artifact reported. Flat regions (no
            // corner in either keyframe) are excluded so noise/wobble
            // doesn't drown out genuine folds.
            function turnAngle(pts,i){
              var p0=pts[i-1].point,p1=pts[i].point,p2=pts[i+1].point;
              var a1=Math.atan2(p1[1]-p0[1],p1[0]-p0[0]),a2=Math.atan2(p2[1]-p1[1],p2[0]-p1[0]);
              return _wrapPI(a2-a1);
            }
            var _foldExp=null,_foldW=null;
            if(n>=5){
              _foldExp=new Array(n);_foldW=new Array(n);
              // Raw per-keyframe turn arrays, cached on rA (2026-07, fold-
              // correction pass): _foldExp above is the expected turn at a
              // FIXED et=0.5, only good for this scoring pass. The
              // correction pass runs at whatever et the CURRENT frame
              // actually needs, so it needs thetaA[i]/thetaB[i] themselves
              // to circular-lerp at arbitrary et — computed once here
              // (same loop, no extra cost) and cached because this whole
              // block only runs ONCE per pair (guarded by
              // rA._twIwProbe===undefined below), while the correction
              // pass runs on every frame this pair generates.
              rA._twFoldThA=new Array(n);rA._twFoldThB=new Array(n);
              for(var fi7=1;fi7<n-1;fi7++){
                var thA7=turnAngle(rA.segments,fi7),thB7=turnAngle(rB.segments,fi7);
                _foldExp[fi7]=thA7+_wrapPI(thB7-thA7)*0.5;
                _foldW[fi7]=Math.max(Math.abs(thA7),Math.abs(thB7));
                rA._twFoldThA[fi7]=thA7;rA._twFoldThB[fi7]=thB7;
              }
              rA._twFoldW=_foldW;
            }
            function foldAngleErr(mid){
              if(!_foldExp)return 0;
              var wsum=1e-6,esum=0;
              for(var fi8=1;fi8<n-1;fi8++){
                var w8=_foldW[fi8];
                if(w8<0.35)continue; // ~20°+ turn only — genuine corners, not noise
                var thM=turnAngle(mid,fi8);
                esum+=w8*Math.abs(_wrapPI(thM-_foldExp[fi8]));
                wsum+=w8;
              }
              return esum/wsum; // radians
            }
            function candScore(traj){
              var exX=0;
              for(var st=1;st<traj.length-1;st++)exX+=Math.max(0,_segsSelfXCount(traj[st])-base);
              var mid=traj[midIdx+1];
              var cd=Math.abs(_wrapPI(chord5(mid)-chordExp));
              var ld5=Lexp5>1e-6?Math.abs(_segPolyLen(mid)-Lexp5)/Lexp5:0;
              var back=0;
              for(var pi5=0;pi5<n;pi5++){
                for(var st2=1;st2<traj.length-1;st2++){
                  var p0=traj[st2-1][pi5].point,p1=traj[st2][pi5].point,p2=traj[st2+1][pi5].point;
                  var v1x=p1[0]-p0[0],v1y=p1[1]-p0[1],v2x=p2[0]-p1[0],v2y=p2[1]-p1[1];
                  var l1=Math.hypot(v1x,v1y),l2=Math.hypot(v2x,v2y);
                  if(l1<0.3||l2<0.3)continue;
                  if((v1x*v2x+v1y*v2y)/(l1*l2)<-0.3)back+=l2;
                }
              }
              var foldErr=foldAngleErr(mid);
              // Same units/weight as the chord-drift term above (30° of
              // fold-angle error ≡ 1 point, matching "30° of drift ≡ 20%
              // length error ≡ 1/10th of a crossing" calibration already
              // established for cd/ld5/back).
              return exX*10+cd/(Math.PI/6)+ld5*5+back/(n*0.8)+foldErr/(Math.PI/6);
            }
            var scB5c=candScore(candTraj.b),scU5=candScore(candTraj.u),scL5=candScore(candTraj.l);
            var scM5=mlsPerV?candScore(candTraj.m):Infinity;
            var best=Math.min(scB5c,scU5,scL5,scM5);
            rA._twUseMLS=null;
            if(scB5c-best<0.05){/* keep per-vertex blend */}
            else if(scU5-best<0.05)rA._twLocalTrust=null;  // uniform intrinsic
            else if(scM5-best<0.05){rA._twIwProbe=0;rA._twUseMLS=mlsPerV;} // MLS local-rigid (standalone, no intrinsic layered on top — see "coupler" note on buildMLSSegs' own candidate)
            else rA._twIwProbe=0;                          // pure linear
          }
        }
      }
    }
    // MLS local-rigid won the arbitration above (once-per-pair decision,
    // cached on rA._twUseMLS) — override the whole-stroke rigid `segs`
    // with the per-vertex version at the REAL current et. rA._twIwProbe
    // was set to 0 alongside the decision, so the intrinsic-blend block
    // right below is a no-op here (iw stays 0) — MLS stands on its own
    // when selected, per Cyril's "switch" framing; coupling it with the
    // intrinsic correction too is a possible follow-up, not built yet.
    if(rA._twUseMLS)segs=buildMLSSegs(rA,rB,rA._twUseMLS,et);
    // Smooth, zero-at-both-ends envelope in et (matches the endpoint-exact
    // guarantee even more strictly than the old per-frame measure did;
    // peaks where the probe above was measured).
    var iw=rA._twIwProbe*Math.sin(Math.PI*Math.max(0,Math.min(1,et)));
    // CORRESPONDENCE-TRUST factor (2026-07, "encore un bras hyper
    // déformé"): the intrinsic reconstruction integrates lerped turning
    // angles, so it's only as good as the index correspondence between the
    // two shapes. On a complex multi-lobed contour (a hand with spread
    // fingers ring-drawn in one stroke, waving between two poses) the
    // aligned correspondence maps finger to non-finger — measured 41° MEAN
    // per-vertex turning disagreement vs ≤11° on every clean case (elbow
    // 2°, V-fold 3°, S-flip 11°) — and integrating those mismatched
    // angles BALLOONS the inbetween (bbox wider than either keyframe,
    // 866px vs 768/760) even though its arc length is perfect. Linear, for
    // all its corner-cutting, stays bounded by construction. Trust ramps
    // 1→0 over 15°→30° mean disagreement, scaling BOTH gates (a garbage
    // correspondence can't credibly fix a self-crossing either), so
    // unreliable pairs simply keep the historical linear behavior.
    if(iw>0.001){
      if(rA._twTurnTrust===undefined){
        var sumTd=0,cntTd=0,pvA=null,pvB=null;
        for(var ti2=1;ti2<n;ti2++){
          var taA=Math.atan2(rA.segments[ti2].point[1]-rA.segments[ti2-1].point[1],rA.segments[ti2].point[0]-rA.segments[ti2-1].point[0]);
          var taB=Math.atan2(rB.segments[ti2].point[1]-rB.segments[ti2-1].point[1],rB.segments[ti2].point[0]-rB.segments[ti2-1].point[0]);
          if(pvA!==null){sumTd+=Math.abs(_wrapPI(_wrapPI(taA-pvA)-_wrapPI(taB-pvB)));cntTd++;}
          pvA=taA;pvB=taB;
        }
        var meanTd=cntTd?sumTd/cntTd:0; // radians; 15°=0.262, 30°=0.524
        rA._twTurnTrust=Math.max(0,Math.min(1,(0.524-meanTd)/(0.524-0.262)));
      }
      iw*=rA._twTurnTrust;
    }
    if(iw>0.001){
      var iSegs=_intrinsicSegs(rA,rB,et,cx2,cy2,et<.5?!!rA.closed:!!rB.closed);
      var lTrust=rA._twLocalTrust;
      if(iSegs)for(var wi3=0;wi3<n;wi3++){
        var iwv=lTrust?iw*lTrust[wi3]:iw;
        if(iwv<=0.001)continue;
        var LS=segs[wi3],IS=iSegs[wi3];
        LS.point=[lerp(LS.point[0],IS.point[0],iwv),lerp(LS.point[1],IS.point[1],iwv)];
        LS.handleIn=[lerp(LS.handleIn[0],IS.handleIn[0],iwv),lerp(LS.handleIn[1],IS.handleIn[1],iwv)];
        LS.handleOut=[lerp(LS.handleOut[0],IS.handleOut[0],iwv),lerp(LS.handleOut[1],IS.handleOut[1],iwv)];
      }
    }
  }
  segs=_applyFoldCorrection(segs,rA,et);
  if(!(rA.isVectorBrush&&rB.isVectorBrush))segs=_applyPointReduction(segs,rA,rB);
  if(rA.isVectorBrush&&rB.isVectorBrush){
    var widths=[];for(var w=0;w<n;w++)widths.push(lerp(rA.widths[w]||1,rB.widths[w]||1,et));
    var centerSegs=segs.map(function(s,idx){return{point:s.point,handleIn:s.handleIn,handleOut:s.handleOut,width:widths[idx]};});
    return _carryTweenStyle({segments:outlineFromCenterSegs(centerSegs),closed:true,strokeColor:null,strokeWidth:lerp(rA.strokeWidth||3,rB.strokeWidth||3,et),strokeCap:rA.strokeCap||'round',strokeJoin:rA.strokeJoin||'round',fillColor:et<.5?(rA.fillColor||null):(rB.fillColor||null),opacity:lerp(rA.opacity!==undefined?rA.opacity:1,rB.opacity!==undefined?rB.opacity:1,et),isVectorBrush:true,centerSegments:centerSegs},rA._src,rB._src,et);
  }
  // A closed shape tweening into another closed shape should stay closed
  // throughout — matches the same "switch at the halfway point" convention
  // already used for strokeColor/fillColor a few lines up, rather than
  // trying to blend "closedness" itself (not a numeric quantity).
  return _carryTweenStyle({segments:segs,closed:et<.5?!!rA.closed:!!rB.closed,strokeColor:et<.5?rA.strokeColor:rB.strokeColor,strokeWidth:lerp(rA.strokeWidth||3,rB.strokeWidth||3,et),strokeCap:rA.strokeCap||'round',strokeJoin:rA.strokeJoin||'round',fillColor:et<.5?(rA.fillColor||null):(rB.fillColor||null),opacity:lerp(rA.opacity!==undefined?rA.opacity:1,rB.opacity!==undefined?rB.opacity:1,et)},rA._src,rB._src,et);
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
function extractStrokePiece(sd,f0,f1,nOverride){
  var isVB=!!(sd.isVectorBrush&&sd.centerSegments&&sd.centerSegments.length>1);
  var srcSegs=isVB?sd.centerSegments:sd.segments;
  var p=new Path({insert:false});
  srcSegs.forEach(function(sg){p.add(new Segment(new Point(sg.point[0],sg.point[1]),new Point(sg.handleIn[0],sg.handleIn[1]),new Point(sg.handleOut[0],sg.handleOut[1])));});
  var len=p.length,n=nOverride||24;
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
      // curPair's own stroke needs the SAME type/closed filter as every
      // other candidate above (2026-07, live-reported hooked/pinched
      // artifact on a vector-brush fill shape): extractStrokePiece always
      // returns an OPEN arc (no `closed` field at all — a fraction of a
      // path has no natural closing edge), so splicing a CLOSED fill
      // shape in as one of the pieces forces interpStroke to blend a
      // looping shape against a linear one — the correspondence has no
      // sane way to reconcile "wraps around" with "has two free ends",
      // and the result is exactly this kind of self-tangling sliver. The
      // unParts-sourced candidates already reject fill/closed; curPair
      // was being concatenated in unconditionally, missing that same gate.
      if(curPair){
        var curIdx=mergedSide==='B'?curPair.aIdx:curPair.bIdx;
        var curFeat=featParts[curIdx];
        if(curFeat.type!=='fill'&&!curFeat.closed)cand=cand.concat([curIdx]);
      }
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
        // Geometric self-consistency (2026-07, live-reported hooked/
        // pinched artifact): a piece can score fine as a STATIC shape
        // match (matchSc only compares silhouettes) while still being a
        // malformed FRAGMENT — extractStrokePiece samples uniformly by
        // fraction of the MERGED stroke's total length, so if the merged
        // source has a sharp direction reversal (an artist's stroke that
        // doubles back on itself) landing inside this piece's own
        // fraction range, the piece inherits that reversal verbatim.
        // Interpolating TOWARD a piece whose own path already folds back
        // on itself is exactly what produced the hook (confirmed live:
        // the SAME pair matched as a single whole stroke, no split,
        // tweens perfectly cleanly — only the split piece is malformed).
        // Reject the whole split if any piece's own path backtracks more
        // than ~25% of its own start-to-end chord length.
        var pieceSegs=piece.centerSegments||piece.segments;
        if(pieceSegs&&pieceSegs.length>=3){
          var pFirst=pieceSegs[0].point,pLast=pieceSegs[pieceSegs.length-1].point;
          var chordLen=Math.hypot(pLast[0]-pFirst[0],pLast[1]-pFirst[1]);
          if(chordLen>1){
            var ux=(pLast[0]-pFirst[0])/chordLen,uy=(pLast[1]-pFirst[1])/chordLen;
            var maxProj=-Infinity,worstBack=0;
            for(var pj=0;pj<pieceSegs.length;pj++){
              var proj=(pieceSegs[pj].point[0]-pFirst[0])*ux+(pieceSegs[pj].point[1]-pFirst[1])*uy;
              if(proj>maxProj)maxProj=proj;else if(maxProj-proj>worstBack)worstBack=maxProj-proj;
            }
            if(worstBack>chordLen*0.25)ok=false;
          }
        }
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
  var list=[],orig=[],dabsByGroup={},held=[],backdropsById={};
  strokes.forEach(function(sd,i){
    if(sd.isBrushTextureCopy){
      if(sd.brushGroupId)(dabsByGroup[sd.brushGroupId]=dabsByGroup[sd.brushGroupId]||[]).push(sd);
      return;
    }
    // Vector-brush FILL BACKDROP (data.linkedFill/linkedFillId, draw-
    // bridge.js's Pressure-brush-with-Fill-enabled path) — excluded from
    // independent matching for the same reason dabs are: it's not
    // independent content, it's a rendering companion of its ribbon
    // (rebuildVectorBrushOutline keeps it curve-fitted to the LIVE
    // centerline on every edit). Letting autoMatch pair it on its own gave
    // it a completely separate resample/alignment/rotation search from its
    // ribbon — nothing then guaranteed the two stayed visually attached
    // mid-tween (found live: "le fill suit pas le stroke", confirmed the
    // backdrop and ribbon can drift apart by several px at the midframe
    // even on a simple synthetic bend, more on a real hand-drawn curve
    // whose smoothed backdrop and raw centerline aren't identical point
    // sets to begin with). Regenerated fresh from the ribbon's own
    // INTERPOLATED centerline every frame instead (generateTweens' pairs
    // loop + emit loop) — same "single source of truth" principle as the
    // live-edit path, just applied per generated frame.
    if(sd.isLinkedFillCompanion){
      if(sd.linkedFillId)backdropsById[sd.linkedFillId]=sd;
      return;
    }
    // Team review ghost (revision-bridge.js): a frozen "before" record
    // belonging to THIS keyframe only — never matched into a tween (would
    // morph a static historical snapshot into whatever the next keyframe
    // is) and never `held` either (held content repeats UNCHANGED into
    // every generated inbetween of the whole span, which would smear one
    // frame's frozen ghost across the entire span instead of leaving it
    // where the reviewer left it). 2026-07 audit: no exclusion existed here
    // at all, unlike the dab/backdrop cases just above.
    if(sd.isRevisionGhost)return;
    // A held stroke is copied UNCHANGED into every generated inbetween
    // (generateTweens' emit loop) instead of participating in
    // autoMatch/interpStroke at all — dabs above are re-stamped fresh each
    // frame instead, a different mechanism for a different reason.
    if(manualMode&&!sd.tweenOn){held.push(sd);return;}
    list.push(sd);orig.push(i);
  });
  return{list:list,orig:orig,dabsByGroup:dabsByGroup,held:held,backdropsById:backdropsById};
}
// Plain closed fill built directly from a ribbon's centerline anchors (no
// width offset) — mirrors rebuildVectorBrushOutline's own backdrop sync
// (tools.js: `linkedFill.segments = center.segments.map(...)`) so a
// generated inbetween's backdrop is geometrically identical in kind to
// what a live edit would produce, not an approximation.
function _backdropFromCenterSegs(centerSegs,fillColor,opacity){
  return{segments:centerSegs.map(function(s){return{point:s.point,handleIn:s.handleIn,handleOut:s.handleOut};}),closed:true,strokeColor:null,fillColor:fillColor,opacity:opacity,hasRealStroke:false};
}
// ---- APPEAR/DISAPPEAR: trim (retract / draw-on) instead of ghost-fade ----
// 2026-07 feedback ("les formes qui ne sont plus là d'une frame à une autre
// qu'il faut trim") — an unmatched stroke used to cross-fade in place,
// leaving ghostly semi-transparent linework across the whole span (cel
// animation never shows half-transparent ink). Open LINEWORK now trims
// instead: a disappearing stroke retracts progressively along its own arc
// length (un-draws itself), an appearing one draws on — at full opacity
// throughout, so no ghost ever shows. Junction-aware: if exactly one of the
// stroke's endpoints touches ink that PERSISTS through the span (a hair
// strand rooted on a persisting head, a fold line attached to a sleeve),
// the attached end is the anchor and the FREE end is what retracts/grows —
// matching how an artist would actually add/remove the line. Fade remains
// for everything trim can't express: closed loops, filled shapes, textured
// anchors (their dab machinery is fade-based), rasters.
function _vanishPlanFor(sd,others){
  if(sd.brushTexturePreset||sd.bitmapBrushSpec||sd.isRaster)return{mode:'fade'};
  var isVB=!!(sd.isVectorBrush&&sd.centerSegments&&sd.centerSegments.length>1);
  if(!isVB&&(sd.closed||sd.fillColor))return{mode:'fade'};
  var segs=isVB?sd.centerSegments:sd.segments;
  if(!segs||segs.length<2)return{mode:'fade'};
  var p0=segs[0].point,p1=segs[segs.length-1].point;
  var tol=Math.max(9,(sd.strokeWidth||3)*2);
  var a0=false,a1=false;
  for(var i=0;i<others.length&&!(a0&&a1);i++){
    var o=others[i];
    var osegs=(o.isVectorBrush&&o.centerSegments&&o.centerSegments.length>1)?o.centerSegments:o.segments;
    if(o.isRaster||!osegs||osegs.length<2)continue;
    var op=buildTPFeat(o);
    if(!a0){var l0=op.getNearestLocation(new Point(p0[0],p0[1]));if(l0&&l0.distance<=tol)a0=true;}
    if(!a1){var l1=op.getNearestLocation(new Point(p1[0],p1[1]));if(l1&&l1.distance<=tol)a1=true;}
    op.remove();
  }
  // Only-end-attached → anchor the end (start retracts). Everything else —
  // start-attached, both, or free-floating — anchors the start: for a free
  // stroke that's "un-draw from the last-drawn end", the natural default
  // since segment order IS drawing order for hand-drawn strokes.
  return{mode:'trim',anchor:(a1&&!a0)?'end':'start'};
}
// extractStrokePiece already cuts any stroke (plain or centerline) at two
// arc-length fractions — reused verbatim; this just restores the rendering
// fields a standalone frame record needs (the split-matching caller feeds
// its pieces through resample/interp instead, which re-derives them).
function _trimmedStroke(sd,f0,f1){
  // Adaptive budget, same rationale as the matched pairs' adaptN (see
  // generateTweens): a long stroke trimmed at 24 fixed samples loses its
  // hand-drawn character mid-retraction — ~one sample per 8px of the
  // piece actually kept, floor 16, capped for cost.
  var n=Math.max(16,Math.min(100,Math.round(_strokeDataPolyLen(sd)*Math.max(0,f1-f0)/8)));
  var piece=extractStrokePiece(sd,f0,f1,n);
  if(piece.isVectorBrush&&piece.centerSegments){
    piece.segments=outlineFromCenterSegs(piece.centerSegments);
    piece.closed=true;
    piece.fillColor=sd.fillColor||null;
    piece.strokeWidth=sd.strokeWidth;
  }else{
    piece.hasRealStroke=sd.hasRealStroke;
  }
  // Style carry-through — same fields _carryTweenStyle moves onto matched
  // pairs' inbetweens (dash/miter/paintOrder/channelTag/effects/gradient/
  // group), read here straight from the vanishing stroke itself.
  ['dashArray','dashOffset','miterLimit','paintOrder','channelTag','groupId','effects','fillGradient'].forEach(function(k){
    if(sd[k]!==undefined)piece[k]=JSON.parse(JSON.stringify(sd[k]));
  });
  piece.strokeId=sd.strokeId;
  return piece;
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
// Renames any stroke past the FIRST one in `strokes` that shares a
// strokeId already claimed earlier in the SAME array — see the call
// site's comment for why this can happen and why it matters.
function _dedupeFrameStrokeIds(strokes,frameIdx){
  var seen={};
  for(var i=0;i<strokes.length;i++){
    var sd=strokes[i];
    if(!sd.strokeId)continue;
    if(seen[sd.strokeId]){sd.strokeId='twdup_'+frameIdx+'_'+i;}
    else seen[sd.strokeId]=1;
  }
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
    // 2026-07 feedback ("si je clic droit sur ma sélection et que je tween
    // il ne va pas faire le tween avec les formes correspondantes de la
    // frame suivante... comme quand il détecte chaque forme"): manualMode
    // only restricts the A side's candidate pool (which strokes are even
    // ELIGIBLE to start a tween) — B always offers its FULL candidate set,
    // exactly like a normal full auto-tween, so autoMatch's own Hungarian
    // matching can still automatically find the right corresponding shape
    // at B. Requiring the SAME strokeId to already exist at B (this
    // function's first attempt) defeated the whole point of automatic
    // shape detection.
    var manualMode=!!ld.frames[fA].tweenManualMode;
    // Self-healing identity guard (2026-07, live-reported: a KEYFRAME's
    // own stored strokes already had the SAME strokeId duplicated across
    // 2 different strokes on a fresh reload, no override involved — a
    // corruption baked in by an EARLIER generateTweens() run, before the
    // fadeOutA/fadeInB collision guard below existed). Deduping here, on
    // the raw keyframe data, BEFORE it feeds sA/sB, both prevents THIS
    // span's matching from reading an already-ambiguous identity and
    // self-heals whatever an old run already corrupted — every id-keyed
    // lookup (continuity, the reassign tool, motion arcs) only ever sees
    // a strokeId that resolves to exactly one stroke in the frame.
    _dedupeFrameStrokeIds(ld.frames[fA].strokes,fA);
    _dedupeFrameStrokeIds(ld.frames[fB].strokes,fB);
    var sAsplit=splitTweenables(ld.frames[fA].strokes,manualMode),sBsplit=splitTweenables(ld.frames[fB].strokes,false);
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
      // Stale/ambiguous-override guard (2026-07, live-reported cascade: a
      // hooked shape at one stroke AND an unrelated stroke silently
      // falling back to fade at another, both traced to the SAME override).
      // ov.bId is meant to identify ONE specific B-side stroke, but ids get
      // reused across UNRELATED strokes over a file's history (every other
      // 2026-07 fix in this function exists because of exactly that) — if
      // ov.bId ALSO happens to be some OTHER A-side stroke's own natural,
      // unrelated identity, this override is stealing that other stroke's
      // rightful B-side partner out from under it, forcing it into a worse
      // fallback match (confirmed live: removing the stale override let
      // BOTH strokes auto-match cleanly with zero unmatched/fading strokes
      // and zero self-tangling). Skip the whole override in that case
      // rather than silently mis-resolving it — auto-match still runs.
      var bIdCollision=false;
      for(var ci=0;ci<sA.length;ci++)if(ci!==aIdx&&sA[ci].strokeId===ov.bId){bIdCollision=true;break;}
      if(!bIdCollision)for(var ci2=0;ci2<sB.length;ci2++)if(ci2!==bIdx&&sB[ci2].strokeId===ov.aId){bIdCollision=true;break;}
      if(bIdCollision)return;
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
    // BOUNDED (2026-07, "grosses déformations... j'ai fait différentes
    // animations sur la même timeline"): the rescue exists for ONE limb
    // whose big-but-ordinary motion pushed it past MATCH_TH (the raised
    // arm measured 0.739) — it must NOT fire across a hard CUT between two
    // unrelated drawings sharing a timeline. Measured on the reported
    // file's cut (13 strokes → 1): the leftover pair scored 0.85, and
    // rescuing it warped an arm into the next scene's first stroke while
    // 12 siblings faded around it. Two independent guards, both derived
    // from measured cases, either one blocks: (1) absolute ceiling 0.78
    // (legitimate rescued limb 0.739 < 0.78 < aberrant cut 0.85); (2) a
    // heavily-unbalanced stroke count (3x+) says "different drawing, most
    // of one side HAS to vanish" — there a rescue needs near-threshold
    // confidence (MATCH_TH+0.1), not benefit-of-the-doubt.
    var RESCUE_CEIL=0.78;
    var cntRatio=Math.max(sA.length,sB.length)/Math.max(1,Math.min(sA.length,sB.length));
    var rescueCeil=cntRatio>=3?MATCH_TH+0.1:RESCUE_CEIL;
    matches.forEach(function(m){
      if(m.score<=MATCH_TH)return; // already handled by the first pass
      if(m.score>rescueCeil)return; // beyond any plausible same-object motion — fade/trim instead
      if(forcedAIdx[m.a]||forcedBIdx[m.b])return;
      if(aMatched[m.a]||bMatched[m.b])return; // one side already claimed — real ambiguity, let it fade
      var fta=strokeFeat(sA[m.a]),ftb=strokeFeat(sB[m.b]);
      var clash=(fta.fillCol&&ftb.fillCol&&colorDist(fta.fillCol,ftb.fillCol)>0.35)||(fta.strokeCol&&ftb.strokeCol&&colorDist(fta.strokeCol,ftb.strokeCol)>0.35);
      // closed-flag agreement only counts as an identity veto when both
      // flags are ground truth — a guessed flag (VB centerline) flipping
      // between two drawings of the same limb must not block the rescue
      // (same rationale as matchSc's softened closedPen above).
      var closedOk=fta.closed===ftb.closed||fta.closedIsGuess||ftb.closedIsGuess;
      if(fta.type===ftb.type&&closedOk&&!clash){
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
    // Identity-COLLISION guard (2026-07, live-reported: 2 real frames
    // showed a duplicate strokeId on two visually unrelated strokes). A
    // manual reassign override (state.tweenOverrides) re-stamps its
    // B-side stroke with the A-side's id — but that exact id can ALREADY
    // belong to some OTHER, unrelated stroke that this span left
    // unmatched (fading in/out with its own pre-existing identity
    // untouched, since the id-less fallback below is a no-op on a stroke
    // that already has ONE — just not a UNIQUE one anymore). Nothing used
    // to check for that: two different strokes in the same rendered
    // frame silently ended up sharing one strokeId, and every id-keyed
    // lookup downstream (continuity, the reassign tool itself, motion
    // arcs) picks whichever one it finds first — exactly the "il se
    // trompe sur la version d'avant" symptom. Precompute the SET of ids
    // pairSpecs are about to stamp (same formula as the stamping loop
    // below) and rename any fading stroke that already holds one of them
    // — a collision is just as much an identity problem as having no id.
    var pendingPairIds={};
    pairSpecs.forEach(function(sp){
      pendingPairIds[sp.aData.strokeId||sp.bData.strokeId||('tw_'+fA+'_'+sp.mi)]=1;
    });
    // Same identity-continuity fix as the matched pairs above (see that
    // comment): a solo fading stroke's id-less keyframe would otherwise
    // be a different identity from its own generated fade frames.
    fadeOutA.forEach(function(sd,i2){
      if(sd.strokeId&&pendingPairIds[sd.strokeId])sd.strokeId='twc_'+fA+'_a'+i2;
      if(!sd.strokeId)sd.strokeId='twf_'+fA+'_a'+i2;
    });
    fadeInB.forEach(function(sd,i2){
      if(sd.strokeId&&pendingPairIds[sd.strokeId])sd.strokeId='twc_'+fB+'_b'+i2;
      if(!sd.strokeId)sd.strokeId='twf_'+fB+'_b'+i2;
    });
    if(!pairSpecs.length&&!fadeOutA.length&&!fadeInB.length)continue;
    // Trim-vs-fade plans, computed ONCE per span (see _vanishPlanFor). The
    // junction anchors are the strokes that persist through the span:
    // every matched pair's own keyframe stroke, plus held strokes.
    var persistA=pairSpecs.map(function(sp){return sp.aData;}).concat(sAsplit.held);
    var persistB=pairSpecs.map(function(sp){return sp.bData;}).concat(sBsplit.held);
    var outPlans=fadeOutA.map(function(sd){return _vanishPlanFor(sd,persistA);});
    var inPlans=fadeInB.map(function(sd){return _vanishPlanFor(sd,persistB);});
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
      //
      // ADAPTIVE point budget ("même style de trait" feedback): resN
      // (default 50) is a fine budget for a short stroke, but a long one —
      // a 1600px arm ring at 50 samples = one point every 32px — gets its
      // hand-drawn wobble/character silently splined away, so inbetweens
      // read noticeably SMOOTHER than the keyframes around them. Scale the
      // budget with the longer stroke's own arc length (~one sample per
      // 8px, capped at 150 for cost), never below the user's resN setting.
      var adaptN=Math.max(resN,Math.min(150,Math.round(Math.max(_strokeDataPolyLen(spec.aData),_strokeDataPolyLen(spec.bData))/8)));
      var rpair=resamplePairFeatureAware(spec.aData,spec.bData,adaptN,isVB);
      var ra=rpair[0],rb=rpair[1];
      // Original keyframe stroke records, for style carry-through
      // (_carryTweenStyle) — the resampled records only hold core fields.
      ra._src=spec.aData;
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
      // _src set AFTER alignment: the wasm align path rebuilds its output
      // object from JSON, so a field attached to rb before the call would
      // be silently dropped on exactly the (default) wasm path.
      var rbAligned=alignResampledPair(ra,rb);
      rbAligned._src=spec.bData;
      // Fill-backdrop companion (see splitTweenables' own comment) — when
      // either keyframe's ribbon carries a linkedFillId, regenerate the
      // backdrop from THIS pair's own interpolated centerline every frame
      // instead of matching/interpolating it independently. A side with no
      // backdrop (Fill wasn't enabled there) contributes opacity 0, so a
      // toggle mid-span fades the fill in/out naturally through the same
      // lerp rather than needing special-case handling.
      var aBd=spec.aData.linkedFillId?sAsplit.backdropsById[spec.aData.linkedFillId]:null;
      var bBd=spec.bData.linkedFillId?sBsplit.backdropsById[spec.bData.linkedFillId]:null;
      var backdrop=(aBd||bBd)?{
        aFill:(aBd&&aBd.fillColor)||(bBd&&bBd.fillColor)||null,
        bFill:(bBd&&bBd.fillColor)||(aBd&&aBd.fillColor)||null,
        aOp:aBd?(aBd.opacity!==undefined?aBd.opacity:1):0,
        bOp:bBd?(bBd.opacity!==undefined?bBd.opacity:1):0,
      }:null;
      return{a:ra,b:rbAligned,mi:spec.mi,tex:tex,bmpTex:bmpTex,id:pairId,backdrop:backdrop,
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
        // Fill backdrop, regenerated from THIS frame's own interpolated
        // centerline (see splitTweenables' comment + this pair's own
        // construction above) — always geometrically attached to its
        // ribbon by construction, never independently matched/aligned.
        // Below the ribbon (-1e-4), mirroring insertBelow at draw time.
        if(pr.backdrop&&sdOut.centerSegments){
          var bdOp=lerp(pr.backdrop.aOp,pr.backdrop.bOp,et2);
          if(bdOp>0.02){
            var bd=_backdropFromCenterSegs(sdOut.centerSegments,et2<.5?pr.backdrop.aFill:pr.backdrop.bFill,bdOp);
            bd.__zKey=sdOut.__zKey-1e-4;bd.strokeId=sdOut.strokeId+'_bd';
            tw.push(bd);
          }
        }
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
      // Fill-backdrop companion for a VANISHING ribbon (see splitTweenables'
      // comment + the matched-pair backdrop just above) — same "regenerate
      // from the anchor's own current centerline" principle, applied to
      // whatever geometry this fade/trim pass just produced for sd itself,
      // so a disappearing filled pressure-brush stroke retracts/fades with
      // its fill still attached instead of the fill going independently
      // ghostly or vanishing at a different rate.
      function pushBackdropFor(sd,geomSrc,rank,opacityMul,backdropsById){
        var bd0=sd.linkedFillId&&backdropsById[sd.linkedFillId];
        if(!bd0||!geomSrc||!geomSrc.centerSegments)return;
        var op=(bd0.opacity!==undefined?bd0.opacity:1)*opacityMul;
        if(op<=0.02)return;
        var bd=_backdropFromCenterSegs(geomSrc.centerSegments,bd0.fillColor,op);
        bd.__zKey=rank-1e-4;tw.push(bd);
      }
      function pushFade(sd,rank,mul,dabsByGroup,backdropsById){
        var c=JSON.parse(JSON.stringify(sd));c.opacity=(c.opacity!==undefined?c.opacity:1)*mul;c.__zKey=rank;
        if(c.opacity>0.02)tw.push(c);
        var grp=sd.brushTexturePreset&&sd.brushGroupId&&dabsByGroup[sd.brushGroupId];
        if(grp)grp.forEach(function(d){
          var dc=JSON.parse(JSON.stringify(d));dc.opacity=(dc.opacity!==undefined?dc.opacity:1)*mul;dc.__zKey=rank+1e-4; // see +1e-4 comment above (vector-dab branch)
          if(dc.opacity>0.02)tw.push(dc);
        });
        if(backdropsById)pushBackdropFor(sd,sd,rank,mul,backdropsById);
      }
      // keep = how much of the stroke is still present (1=full, 0=gone) —
      // trim shows the [anchored] sub-stroke at FULL opacity, fade is the
      // legacy opacity ramp (see _vanishPlanFor for which strokes get
      // which). Clamped: an overshooting easing curve (back/elastic) can
      // push et2 outside [0,1].
      function pushVanish(sd,plan,rank,keep,dabsByGroup,backdropsById){
        keep=Math.max(0,Math.min(1,keep));
        if(plan.mode==='trim'){
          if(keep<=0.02)return;
          if(keep>=0.999){var full=JSON.parse(JSON.stringify(sd));full.__zKey=rank;tw.push(full);if(backdropsById)pushBackdropFor(sd,full,rank,1,backdropsById);return;}
          var pc=_trimmedStroke(sd,plan.anchor==='end'?1-keep:0,plan.anchor==='end'?1:keep);
          pc.__zKey=rank;tw.push(pc);
          if(backdropsById)pushBackdropFor(sd,pc,rank,1,backdropsById);
          return;
        }
        pushFade(sd,rank,keep,dabsByGroup,backdropsById);
      }
      fadeOutA.forEach(function(sd,fi2){pushVanish(sd,outPlans[fi2],unA[fi2]/Math.max(1,sA.length-1),1-et2,sAsplit.dabsByGroup,sAsplit.backdropsById);});
      fadeInB.forEach(function(sd,fi2){pushVanish(sd,inPlans[fi2],unB[fi2]/Math.max(1,sB.length-1),et2,sBsplit.dabsByGroup,sBsplit.backdropsById);});
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
  // manualMode only restricts A's candidate pool — see generateTweens' own
  // comment on this exact split for why B always stays unrestricted.
  var manualMode=!!ld.frames[fA].tweenManualMode;
  var spA=splitTweenables(ld.frames[fA].strokes,manualMode),spB=splitTweenables(ld.frames[fB].strokes,false);
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
// Human-readable description of "what's about to happen", captured at the
// SAME moment as the snapshot (pushUndoLayers runs before the mutation it
// guards, so this reflects the active tool/frame/layer context of the
// upcoming action, not a parsed diff). Reuses TOOL_LABELS (timeline.js,
// global) rather than a second tool→name map. Feeds history-panel.js only —
// never persisted (see undoLabels' own comment in app.js).
function _actionLabelNow(){
  var tool=state.tool||'?';
  var ld=state.layers&&state.layers[state.activeLayerIdx];
  return{label:(window.TOOL_LABELS&&TOOL_LABELS[tool])||tool,tool:tool,frame:state.currentFrame||0,layer:ld?ld.name:'',t:Date.now()};
}
// window._scrubLiveActive (ui.js, live drag-scrub des champs numériques) :
// pendant un drag de valeur, les handlers 'change' tournent à CHAQUE tick
// (reflet temps réel au canvas) et la plupart commencent par pushUndo — un
// snapshot par tick aurait pollué la pile pour un seul geste. ui.js pousse
// UN snapshot pré-geste au premier mouvement puis lève ce flag ; ici on
// no-op tant qu'il est levé (y compris le 'change' final du release).
function pushUndoLayers(){if(window._scrubLiveActive)return;saveAllLayerFrames();state.undoStack.push(layersSnapshotNow());state.undoLabels.push(_actionLabelNow());if(state.undoStack.length>state.maxUndo){state.undoStack.shift();state.undoLabels.shift();}state.redoStack=[];state.redoLabels=[];if(window.SMFeedback)SMFeedback.logAction();if(window.renderHistoryPanelIfOpen)renderHistoryPanelIfOpen();
  // See _maybePromoteInterpolated's own comment (app.js) — this is the
  // SAME choke point SMFeedback.logAction() right above already trusts as
  // "a real content-mutating action happened" (its own doc comment: "only
  // fires for actual content mutations"), reused here so a scrub-only
  // save can never promote a tween frame no matter what future field
  // desP()/serP() disagree on. Cleared by loadFrame the moment ANY frame
  // finishes being left (this action's own save already ran above, before
  // this line, so setting it here — for the NEXT save — is correct).
  window._tweenFrameDirty=true;
}
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
function undo(){if(!state.undoStack.length){showToast('Rien à annuler');return;}var s=state.undoStack.pop();var sl=state.undoLabels.pop()||_actionLabelNow();
if(s.type==='layers'){state.redoStack.push(layersSnapshotNow());state.redoLabels.push(sl);restoreLayersSnapshot(s);if(window.renderHistoryPanelIfOpen)renderHistoryPanelIfOpen();return;}
var cur={frame:state.currentFrame,layers:[]};for(var i=0;i<state.layers.length;i++){var f=state.layers[i].frames[state.currentFrame];cur.layers.push({strokes:JSON.parse(JSON.stringify(f.strokes)),isKeyframe:f.isKeyframe,isInterpolated:f.isInterpolated});}state.redoStack.push(cur);state.redoLabels.push(sl);for(var i2=0;i2<s.layers.length&&i2<state.layers.length;i2++){var tf=state.layers[i2].frames[s.frame];tf.strokes=s.layers[i2].strokes;tf.isKeyframe=s.layers[i2].isKeyframe;tf.isInterpolated=s.layers[i2].isInterpolated;}if(s.frame!==state.currentFrame)state.currentFrame=s.frame;loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();if(window.renderHistoryPanelIfOpen)renderHistoryPanelIfOpen();}
function redo(){if(!state.redoStack.length){showToast('Rien à refaire');return;}var s=state.redoStack.pop();var sl=state.redoLabels.pop()||_actionLabelNow();
if(s.type==='layers'){state.undoStack.push(layersSnapshotNow());state.undoLabels.push(sl);restoreLayersSnapshot(s);if(window.renderHistoryPanelIfOpen)renderHistoryPanelIfOpen();return;}
var cur={frame:state.currentFrame,layers:[]};for(var i=0;i<state.layers.length;i++){var f=state.layers[i].frames[state.currentFrame];cur.layers.push({strokes:JSON.parse(JSON.stringify(f.strokes)),isKeyframe:f.isKeyframe,isInterpolated:f.isInterpolated});}state.undoStack.push(cur);state.undoLabels.push(sl);for(var i2=0;i2<s.layers.length&&i2<state.layers.length;i2++){var tf=state.layers[i2].frames[s.frame];tf.strokes=s.layers[i2].strokes;tf.isKeyframe=s.layers[i2].isKeyframe;tf.isInterpolated=s.layers[i2].isInterpolated;}if(s.frame!==state.currentFrame)state.currentFrame=s.frame;loadFrame(state.currentFrame);renderOS();renderArcs();updateUI();if(window.renderHistoryPanelIfOpen)renderHistoryPanelIfOpen();}

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

