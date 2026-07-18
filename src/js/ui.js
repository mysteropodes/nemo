// Multi-point easing curve editor. Was a fixed CSS-cubic-bezier(p1,p2) pair
// (2 OFF-curve tangent handles, standard CSS easing model) — replaced with
// an arbitrary number of ON-curve waypoints (After Effects/Blender graph-
// editor style) so points can be freely added/removed, per request. Each
// consecutive pair of points forms its own cubic Bezier segment with
// Catmull-Rom-derived tangents at the shared knots, giving a smooth curve
// through every point with zero manual handle-dragging. tweens.js only ever
// consumes `evalCurve(x)` as a pure function (verified: getEasing() in
// tweens.js just returns window._curveEditor.evalCurve and calls it with a
// plain [0,1] fraction) — completely decoupled from how many points back it,
// so this rewrite needed no changes on the tween-generation side.
(function(){
  var cvs=document.getElementById('curve-canvas'),ctx=cvs.getContext('2d');
  var W=cvs.width,H=cvs.height,pad=30;
  // tx/ty preserved when present: a point's MANUAL tangent override
  // (draggable Alt-handles, 2026-07-17 — "impossible de contrôler les
  // tangentes de point clé dans la courbe d'accélération"). Stripping
  // them here silently reset every hand-tuned tangent on the very next
  // preset-match check / setCurve round-trip.
  function clonePts(pts){return pts.map(function(p){var o={x:p.x,y:p.y};if(typeof p.tx==='number'){o.tx=p.tx;o.ty=p.ty||0;}return o;});}
  var cs={points:[{x:0,y:0},{x:.42,y:0},{x:.58,y:1},{x:1,y:1}]};
  var dragging=null,selected=null,hovering=false,rect=null;
  // Alt/Option reveals the selected point's tangents (feedback #22). The
  // curve model stores no handles — tangents are DERIVED (Catmull-Rom, see
  // segCtrl) — so this is a read-only visualization of the actual tangent
  // the interpolation uses at that knot, drawn as two orange handle stubs.
  // Sticky: shown by Alt+click (or pressing Alt with a point selected),
  // hidden again by the next plain click.
  var showTangents=false;
  // Camera-segment ease mode (feedback #5pi90): the camera panel's own tiny
  // 2-handle editor is gone — right-clicking the camera timeline row now
  // opens THIS shared widget instead, pointed at that segment's ease
  // instead of state.easingCurve. Storage stays the classic 2-off-curve-
  // control-point cubic bezier (camera.js's cameraAtFrame/bezierEase are
  // untouched) — only the editing surface moves, so no data migration.
  var camEaseSeg=null,camEaseLabel='';
  function isCamMode(){return !!camEaseSeg;}
  function camEase(){return camEaseSeg.ease||(camEaseSeg.ease=[.42,0,.58,1]);}
  // Motion-segment points-based ease mode (2026-07, explicit request: reuse
  // the SAME on-curve-waypoint model as the tween's own curve, not camera's
  // simpler 2-handle bezier) — a motion keyframe's own `curvePoints` array,
  // same shape as `cs.points` (the tween's global curve) but scoped to ONE
  // segment instead of applying uniformly everywhere. Mutually exclusive
  // with camera mode (editMotionSeg/editCameraSeg each clear the other).
  var motionEaseSeg=null,motionEaseLabel='';
  function isMotionMode(){return !!motionEaseSeg;}
  var MOTION_DEFAULT_CURVE=[{x:0,y:0},{x:.42,y:0},{x:.58,y:1},{x:1,y:1}];
  function motionCurve(){return motionEaseSeg.curvePoints||(motionEaseSeg.curvePoints=clonePts(MOTION_DEFAULT_CURVE));}
  // The points array actually being viewed/edited right now — motion-
  // segment mode if active, otherwise the tween's own global curve. Every
  // point-editing code path below (hit-test, drag, add/delete, presets)
  // reads/writes through this instead of `cs.points` directly, so camera
  // mode stays the only thing with genuinely separate rendering/
  // interaction. (Per-pair tween easing, state.tweenEasing, has its OWN
  // floating draggable-point editor — timeline.js's openTweenCurveInset —
  // rather than a 3rd mode here: it needs several instances open-able
  // independently of whatever this singleton widget happens to be
  // showing, which this widget's one-curve-at-a-time model can't do. Only
  // evalPointsCurve below is shared between the two.)
  function activePoints(){return isMotionMode()?motionCurve():cs.points;}
  function setActivePoints(pts){if(isMotionMode())motionEaseSeg.curvePoints=pts;else cs.points=pts;}
  // A small easing gallery (After Effects/GreenSock-style grid of named
  // curve families, each in/out/inout where that makes sense) — through-
  // point approximations of their usual off-curve-handle shapes, since the
  // underlying model here is on-curve waypoints, not tangent handles.
  // Rendered as actual curve-shape thumbnails (see buildThumbSvg), not text
  // labels — a text button gave zero indication of what a preset looked
  // like before clicking it.
  var presets={
    'linear':[{x:0,y:0},{x:1,y:1}],
    'sine-in':[{x:0,y:0},{x:.36,y:.1},{x:1,y:1}],
    'sine-out':[{x:0,y:0},{x:.64,y:.9},{x:1,y:1}],
    'sine-in-out':[{x:0,y:0},{x:.3,y:.1},{x:.7,y:.9},{x:1,y:1}],
    'quad-in':[{x:0,y:0},{x:.4,y:.08},{x:1,y:1}],
    'quad-out':[{x:0,y:0},{x:.6,y:.92},{x:1,y:1}],
    'quad-in-out':[{x:0,y:0},{x:.28,y:.06},{x:.72,y:.94},{x:1,y:1}],
    'cubic-in':[{x:0,y:0},{x:.5,y:.04},{x:1,y:1}],
    'cubic-out':[{x:0,y:0},{x:.5,y:.96},{x:1,y:1}],
    'cubic-in-out':[{x:0,y:0},{x:.25,y:.02},{x:.75,y:.98},{x:1,y:1}],
    'back-in':[{x:0,y:0},{x:.3,y:-.15},{x:1,y:1}],
    'back-out':[{x:0,y:0},{x:.7,y:1.15},{x:1,y:1}],
    'back-in-out':[{x:0,y:0},{x:.2,y:-.15},{x:.8,y:1.15},{x:1,y:1}],
    'elastic-out':[{x:0,y:0},{x:.35,y:1.3},{x:.55,y:.85},{x:.75,y:1.08},{x:1,y:1}],
    'bounce-out':[{x:0,y:0},{x:.36,y:.68},{x:.5,y:.4},{x:.72,y:.94},{x:.84,y:.78},{x:1,y:1}],
    'ease-in-out':[{x:0,y:0},{x:.3,y:.05},{x:.7,y:.95},{x:1,y:1}]
  };
  var PRESET_ORDER=['linear','sine-in','sine-out','sine-in-out','quad-in','quad-out','quad-in-out',
    'cubic-in','cubic-out','cubic-in-out','back-in','back-out','back-in-out','elastic-out','bounce-out','ease-in-out'];
  // Small inline SVG sparkline of a points array's actual evaluated curve —
  // shared by both the built-in and the user's saved custom presets, so a
  // hand-tuned custom curve gets the exact same visual treatment.
  function buildThumbSvg(pts){
    var w=44,h=30,pad=4,N=24;
    var lo=0,hi=1;
    pts.forEach(function(p){if(p.y<lo)lo=p.y;if(p.y>hi)hi=p.y;});
    var m=(hi-lo)*.12||.1;lo-=m;hi+=m;
    var d='';
    for(var i=0;i<=N;i++){
      var x=i/N,y=evalPointsCurve(pts,x);
      var px=pad+x*(w-2*pad),py=h-pad-((y-lo)/(hi-lo))*(h-2*pad);
      d+=(i===0?'M':'L')+px.toFixed(1)+','+py.toFixed(1)+' ';
    }
    return '<svg viewBox="0 0 '+w+' '+h+'" width="'+w+'" height="'+h+'"><path d="'+d+'" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>';
  }
  var CUSTOM_KEY='nemo_easing_presets';
  function loadCustomPresets(){try{return JSON.parse(localStorage.getItem(CUSTOM_KEY)||'[]');}catch(e){return[];}}
  function saveCustomPresets(list){try{localStorage.setItem(CUSTOM_KEY,JSON.stringify(list));}catch(e){}}

  // Y-axis auto-fits to whatever the points' range actually is (with a 10%
  // margin) instead of a fixed [0,1] — otherwise an overshoot curve (e.g.
  // "Over", peaking at y=1.15) would get silently clipped at the panel's
  // edge. X always stays [0,1] (that's the timing axis, points can't go
  // past the endpoints).
  function yRange(){
    var lo=0,hi=1;
    activePoints().forEach(function(p){if(p.y<lo)lo=p.y;if(p.y>hi)hi=p.y;});
    var m=(hi-lo)*.12||.1;
    return{lo:lo-m,hi:hi+m};
  }
  function tX(n){return pad+n*(W-2*pad);}
  function fX(c){return(c-pad)/(W-2*pad);}
  function tY(n,yr){return H-pad-((n-yr.lo)/(yr.hi-yr.lo))*(H-2*pad);}
  function fY(c,yr){return yr.lo+((H-pad-c)/(H-2*pad))*(yr.hi-yr.lo);}

  function cubicAt(t,a,b,c,d){var u=1-t;return u*u*u*a+3*u*u*t*b+3*u*t*t*c+t*t*t*d;}
  function cubicDerivAt(t,a,b,c,d){var u=1-t;return 3*u*u*(b-a)+6*u*t*(c-b)+3*t*t*(d-c);}
  // Effective tangent at pts[i]: the point's own MANUAL override (tx/ty,
  // set by dragging its Alt-handles — one symmetric tangent per point,
  // in/out aligned like a graph editor's "smooth" knot) when present,
  // otherwise the derived Catmull-Rom (next-prev)/2 the model always
  // used. Duplicated in motion.js (curveTangentAt) — CLAUDE.md §3 pure-
  // math pair, keep in sync.
  function tangentAt(pts,i){
    var p=pts[i];
    if(typeof p.tx==='number')return{x:p.tx,y:p.ty||0};
    var prev=pts[i-1]||p,next=pts[i+1]||p;
    return{x:(next.x-prev.x)/2,y:(next.y-prev.y)/2};
  }
  // Tangent at each knot (manual override or derived Catmull-Rom),
  // converted to the pair of cubic-Bezier control points for the segment
  // [pts[i],pts[i+1]].
  function segCtrl(pts,i){
    var p0=pts[i],p3=pts[i+1];
    var t1=tangentAt(pts,i),t2=tangentAt(pts,i+1);
    return{c1:{x:p0.x+t1.x/3,y:p0.y+t1.y/3},c2:{x:p3.x-t2.x/3,y:p3.y-t2.y/3}};
  }
  function segForPts(pts,x){
    var i=0;
    while(i<pts.length-2&&pts[i+1].x<x)i++;
    return i;
  }
  // Pure function of an explicit points array — pulled out of evalCurve so
  // the preset gallery's thumbnails can plot the exact same math the main
  // canvas uses, without touching the live `cs.points` state.
  function evalPointsCurve(pts,x){
    if(pts.length<2)return x;
    x=Math.max(0,Math.min(1,x));
    var i=segForPts(pts,x),p0=pts[i],p3=pts[i+1],ctrl=segCtrl(pts,i);
    var span=p3.x-p0.x,t=span>1e-6?(x-p0.x)/span:0;
    for(var k=0;k<8;k++){
      var ex=cubicAt(t,p0.x,ctrl.c1.x,ctrl.c2.x,p3.x)-x;
      var dx=cubicDerivAt(t,p0.x,ctrl.c1.x,ctrl.c2.x,p3.x);
      if(Math.abs(dx)<1e-6)break;
      t-=ex/dx;t=Math.max(0,Math.min(1,t));
    }
    return cubicAt(t,p0.y,ctrl.c1.y,ctrl.c2.y,p3.y);
  }
  function evalCurve(x){return evalPointsCurve(cs.points,x);}

  // Classic 2-off-curve-control-point cubic bezier (0,0)->(1,1) — the exact
  // model/rendering camera.js's own mini editor used to have, just drawn on
  // this shared big canvas with a FIXED [0,1] range (no auto-fit: camera
  // eases essentially never overshoot, and a fixed frame make the control
  // handles easier to place precisely by eye).
  function drawCamEase(){
    var yr={lo:0,hi:1};
    ctx.clearRect(0,0,W,H);ctx.fillStyle='#111';ctx.fillRect(0,0,W,H);
    var e=camEase();
    ctx.strokeStyle='#334155';ctx.setLineDash([3,3]);
    var p0=[tX(0),tY(0,yr)],p1=[tX(1),tY(1,yr)];
    ctx.strokeRect(p0[0],p1[1],p1[0]-p0[0],p0[1]-p1[1]);
    ctx.beginPath();ctx.moveTo(p0[0],p0[1]);ctx.lineTo(p1[0],p1[1]);ctx.stroke();
    ctx.setLineDash([]);
    ctx.strokeStyle='#ffaa28';ctx.lineWidth=2.5;
    ctx.beginPath();ctx.moveTo(p0[0],p0[1]);
    var c1=[tX(e[0]),tY(e[1],yr)],c2=[tX(e[2]),tY(e[3],yr)];
    ctx.bezierCurveTo(c1[0],c1[1],c2[0],c2[1],p1[0],p1[1]);
    ctx.stroke();
    ctx.strokeStyle='#556';ctx.lineWidth=1;
    ctx.beginPath();ctx.moveTo(p0[0],p0[1]);ctx.lineTo(c1[0],c1[1]);ctx.stroke();
    ctx.beginPath();ctx.moveTo(p1[0],p1[1]);ctx.lineTo(c2[0],c2[1]);ctx.stroke();
    [c1,c2].forEach(function(p){
      ctx.fillStyle='#ffaa28';ctx.strokeStyle='#fff';ctx.lineWidth=1.5;
      ctx.beginPath();ctx.arc(p[0],p[1],5,0,Math.PI*2);ctx.fill();ctx.stroke();
    });
    var coordsEl=document.getElementById('curve-coords');
    if(coordsEl)coordsEl.textContent=camEaseLabel;
  }
  function draw(){
    if(isCamMode()){drawCamEase();return;}
    var pts=activePoints();
    var yr=yRange();
    ctx.clearRect(0,0,W,H);ctx.fillStyle='#111';ctx.fillRect(0,0,W,H);
    ctx.strokeStyle='#1e293b';ctx.lineWidth=1;
    for(var i=0;i<=4;i++){var n=i/4;ctx.beginPath();ctx.moveTo(tX(n),tY(yr.lo,yr));ctx.lineTo(tX(n),tY(yr.hi,yr));ctx.stroke();
      var ny=yr.lo+n*(yr.hi-yr.lo);ctx.beginPath();ctx.moveTo(tX(0),tY(ny,yr));ctx.lineTo(tX(1),tY(ny,yr));ctx.stroke();}
    // baseline y=0 / y=1 guides (distinct from the generic grid) so the
    // "nominal" range is still visible even when the view is zoomed out to
    // fit an overshoot curve.
    ctx.strokeStyle='#334155';ctx.setLineDash([4,4]);
    ctx.beginPath();ctx.moveTo(tX(0),tY(0,yr));ctx.lineTo(tX(1),tY(0,yr));ctx.stroke();
    ctx.beginPath();ctx.moveTo(tX(0),tY(1,yr));ctx.lineTo(tX(1),tY(1,yr));ctx.stroke();
    ctx.setLineDash([]);
    // curve — motion-segment mode plots THIS segment's own points (not the
    // global evalCurve/cs.points), everything else about the rendering is
    // shared with the tween's own curve view.
    ctx.strokeStyle='#4a9eff';ctx.lineWidth=2.5;ctx.beginPath();
    var evalFn=isMotionMode()?function(x){return evalPointsCurve(pts,x);}:evalCurve;
    ctx.moveTo(tX(0),tY(evalFn(0),yr));
    var N=100;for(var s=1;s<=N;s++){var xx=s/N;ctx.lineTo(tX(xx),tY(evalFn(xx),yr));}
    ctx.stroke();
    // through-point handles + connecting guide lines between consecutive points
    ctx.strokeStyle='rgba(255,255,255,.15)';ctx.lineWidth=1;ctx.beginPath();
    pts.forEach(function(p,i2){var px=tX(p.x),py=tY(p.y,yr);if(i2===0)ctx.moveTo(px,py);else ctx.lineTo(px,py);});
    ctx.stroke();
    pts.forEach(function(p,i3){
      var isEnd=i3===0||i3===pts.length-1;
      var col=i3===selected?'#fff':(isEnd?'#bd93f9':'#4a9eff');
      drawH(p.x,p.y,col,yr,isEnd?7:8);
    });
    // Selected point's tangent handles (Alt/Option — see showTangents
    // above): the EFFECTIVE tangent segCtrl feeds the interpolation
    // (manual override if the point has one, derived Catmull-Rom
    // otherwise), split into its in/out thirds. No longer read-only
    // (2026-07-17, "impossible de contrôler les tangentes"): the two
    // stubs are DRAGGABLE — grabbing one sets the point's tx/ty
    // override (symmetric, in/out aligned), double-click on the point
    // clears it back to auto. Manual tangents draw green, derived ones
    // keep the original orange, so "this knot was hand-tuned" reads at
    // a glance.
    if(showTangents&&selected!=null&&pts[selected]){
      var sp=pts[selected];
      var tt=tangentAt(pts,selected);
      var manual=typeof sp.tx==='number';
      var hcol=manual?'#50fa7b':'#ffb86c';
      [{x:sp.x-tt.x/3,y:sp.y-tt.y/3},{x:sp.x+tt.x/3,y:sp.y+tt.y/3}].forEach(function(h){
        var hx=tX(h.x),hy=tY(h.y,yr);
        ctx.strokeStyle=hcol;ctx.lineWidth=1.2;
        ctx.beginPath();ctx.moveTo(tX(sp.x),tY(sp.y,yr));ctx.lineTo(hx,hy);ctx.stroke();
        ctx.fillStyle='#fff';ctx.fillRect(hx-4,hy-4,8,8);
        ctx.strokeStyle=hcol;ctx.strokeRect(hx-4,hy-4,8,8);
      });
    }
    var coordsEl=document.getElementById('curve-coords');
    if(coordsEl)coordsEl.textContent=isMotionMode()?motionEaseLabel:(pts.length+' points');
  }
  function drawH(nx,ny,c,yr,r){ctx.beginPath();ctx.arc(tX(nx),tY(ny,yr),r,0,Math.PI*2);ctx.fillStyle=c;ctx.fill();ctx.strokeStyle='#fff';ctx.lineWidth=1.5;ctx.stroke();}
  function hitT(mx,my){
    var yr=yRange(),best=-1,bestD=16;
    activePoints().forEach(function(p,i){var d=Math.hypot(mx-tX(p.x),my-tY(p.y,yr));if(d<bestD){bestD=d;best=i;}});
    return best;
  }
  // Only the tween's own global curve persists to state.easingCurve/save —
  // a motion segment's points live on the KEY itself (motion.js's own
  // save/load path), never here, so pushCurve must not clobber the global
  // tween curve while a motion segment happens to be open in this same
  // widget. renderNow()/renderTimeline() aren't called from here (this
  // module doesn't know about Motion mode's rows) — motion.js's own
  // onEaseSegChanged hook (if present) picks up the repaint instead.
  function pushCurve(){
    if(isMotionMode()){if(window.SMMotion&&window.SMMotion.onEaseSegChanged)window.SMMotion.onEaseSegChanged();return;}
    if(window.SM)window.SM.setCurve(clonePts(cs.points));upP();
  }

  // Camera-mode drag state — which control handle (0 or 1) is being moved.
  var camDragWhich=null;
  // Tangent-handle drag state — {idx, dir:+1|-1} while one of the selected
  // point's two Alt-handle stubs is being dragged, null otherwise.
  var dragTangent=null;
  cvs.addEventListener('mousedown',function(e){
    rect=cvs.getBoundingClientRect();
    var mx=(e.clientX-rect.left)*(W/rect.width),my=(e.clientY-rect.top)*(H/rect.height);
    if(isCamMode()){
      var e2=camEase();
      var yr={lo:0,hi:1};
      var c1=[tX(e2[0]),tY(e2[1],yr)],c2=[tX(e2[2]),tY(e2[3],yr)];
      var d1=Math.hypot(mx-c1[0],my-c1[1]),d2=Math.hypot(mx-c2[0],my-c2[1]);
      if(window.pushUndo)window.pushUndo(); // camera ease is part of the framing — Cmd+Z restores it too
      camDragWhich=d1<=d2?0:1;
      return;
    }
    // Visible tangent stubs get first grab priority — they sit close to
    // their own point, and hitT would otherwise always win and start a
    // point-drag instead ("impossible de contrôler les tangentes").
    if(showTangents&&selected!=null){
      var spts=activePoints(),sp0=spts[selected];
      if(sp0){
        var yrT=yRange(),ttg=tangentAt(spts,selected);
        var hs=[{dir:-1,x:sp0.x-ttg.x/3,y:sp0.y-ttg.y/3},{dir:1,x:sp0.x+ttg.x/3,y:sp0.y+ttg.y/3}];
        for(var hi2=0;hi2<hs.length;hi2++){
          if(Math.hypot(mx-tX(hs[hi2].x),my-tY(hs[hi2].y,yrT))<10){
            if(window.pushUndo)window.pushUndo(); // one undo per tangent gesture — same convention as the camera handles above
            dragTangent={idx:selected,dir:hs[hi2].dir};
            return;
          }
        }
      }
    }
    var hit=hitT(mx,my);
    dragging=hit>=0?hit:null;
    if(hit>=0){selected=hit;showTangents=e.altKey;}
    else showTangents=false; // plain click off any point clears the sticky reveal
    draw();
  });
  // Alt held while a point is already selected also reveals its tangents —
  // sticky like the click path (see header comment), not a hold-to-preview:
  // simpler mental model, and avoids fighting over a keyup that can't tell
  // "was this reveal from a click or from hovering+Alt" apart.
  document.addEventListener('keydown',function(e){
    if(e.key==='Alt'&&hovering&&selected!=null&&!showTangents){showTangents=true;draw();}
  });
  window.addEventListener('mousemove',function(e){
    if(isCamMode()){
      if(camDragWhich==null)return;
      if(!rect)rect=cvs.getBoundingClientRect();
      var cmx=(e.clientX-rect.left)*(W/rect.width),cmy=(e.clientY-rect.top)*(H/rect.height);
      var cnx=Math.max(0,Math.min(1,fX(cmx))),cny=fY(cmy,{lo:0,hi:1});
      var ce=camEase();
      if(camDragWhich===0){ce[0]=cnx;ce[1]=cny;}else{ce[2]=cnx;ce[3]=cny;}
      draw();
      if(window.SMCamera&&window.SMCamera.renderCameraRow)window.SMCamera.renderCameraRow();
      if(window.SMEngineBridge)window.SMEngineBridge.renderNow();
      return;
    }
    if(dragTangent){
      if(!rect)rect=cvs.getBoundingClientRect();
      var yrt=yRange();
      var tmx=(e.clientX-rect.left)*(W/rect.width),tmy=(e.clientY-rect.top)*(H/rect.height);
      var tpts=activePoints(),tp=tpts[dragTangent.idx];
      if(!tp){dragTangent=null;return;}
      // handle sits at p + dir*t/3 → t = dir*3*(handle-p). tx clamped
      // positive: the eval solves x(t)=x by Newton and assumes x is
      // monotonically increasing along the curve — a backward-pointing
      // tangent (negative x component) could fold the segment over
      // itself in x, which has no meaning on a timing axis.
      var ntx=dragTangent.dir*3*(fX(tmx)-tp.x),nty=dragTangent.dir*3*(fY(tmy,yrt)-tp.y);
      tp.tx=Math.max(0.001,ntx);tp.ty=nty;
      draw();pushCurve();
      return;
    }
    if(dragging==null)return;
    if(!rect)rect=cvs.getBoundingClientRect();
    var yr=yRange();
    var mx=(e.clientX-rect.left)*(W/rect.width),my=(e.clientY-rect.top)*(H/rect.height);
    var nx=fX(mx),ny=fY(my,yr);
    var pts=activePoints();
    var p=pts[dragging];
    if(dragging===0)p.x=0;
    else if(dragging===pts.length-1)p.x=1;
    else{
      var lo=pts[dragging-1].x+.01,hi=pts[dragging+1].x-.01;
      p.x=Math.max(lo,Math.min(hi,nx));
    }
    p.y=Math.max(-1,Math.min(2,ny));
    draw();pushCurve();
  });
  window.addEventListener('mouseup',function(){dragging=null;camDragWhich=null;dragTangent=null;});
  cvs.addEventListener('dblclick',function(e){
    if(isCamMode())return; // camera mode has exactly 2 fixed control points, nothing to add
    rect=cvs.getBoundingClientRect();
    var mx=(e.clientX-rect.left)*(W/rect.width),my=(e.clientY-rect.top)*(H/rect.height);
    var dblHit=hitT(mx,my);
    if(dblHit>=0){
      // Double-click ON a point: clear its manual tangent override back
      // to the derived (auto) Catmull-Rom — the escape hatch matching
      // the drag that set it (green handles turn orange again).
      var dpts=activePoints(),dp=dpts[dblHit];
      if(typeof dp.tx==='number'){delete dp.tx;delete dp.ty;selected=dblHit;draw();pushCurve();}
      return;
    }
    var yr=yRange();
    var nx=Math.max(.01,Math.min(.99,fX(mx))),ny=fY(my,yr);
    var pts=activePoints();
    var idx=pts.length;
    for(var i=1;i<pts.length;i++){if(pts[i].x>nx){idx=i;break;}}
    pts.splice(idx,0,{x:nx,y:ny});
    selected=idx;
    draw();pushCurve();
  });
  cvs.addEventListener('mouseenter',function(){hovering=true;});
  cvs.addEventListener('mouseleave',function(){hovering=false;});
  document.addEventListener('keydown',function(e){
    if(!hovering||selected==null||isCamMode())return;
    if(e.key!=='Delete'&&e.key!=='Backspace')return;
    var pts=activePoints();
    if(selected===0||selected===pts.length-1)return; // endpoints are permanent
    e.preventDefault();
    pts.splice(selected,1);
    selected=null;
    draw();pushCurve();
  });

  function isMatch(p){
    var pts=cs.points;
    return p&&p.length===pts.length&&p.every(function(pt,i){return Math.abs(pt.x-pts[i].x)<.03&&Math.abs(pt.y-pts[i].y)<.03;});
  }
  function upP(){
    document.querySelectorAll('#curve-presets button[data-preset], #curve-custom-presets button[data-preset]').forEach(function(b){
      var p=presets[b.dataset.preset]||_customByKey[b.dataset.preset];
      b.classList.toggle('active',!!isMatch(p));
    });
  }
  // Built-in gallery: rendered once, each button a real curve-shape
  // thumbnail (buildThumbSvg) plus a small caption underneath, laid out in
  // a CSS grid (see style.css #curve-presets) instead of a plain button row.
  function renderPresetGallery(){
    var wrap=document.getElementById('curve-presets');if(!wrap)return;
    wrap.innerHTML='';
    PRESET_ORDER.forEach(function(key){
      var pts=presets[key];
      var b=document.createElement('button');
      b.dataset.preset=key;
      b.title=key;
      b.innerHTML=buildThumbSvg(pts)+'<span>'+key+'</span>';
      b.addEventListener('click',function(){setActivePoints(clonePts(pts));selected=null;draw();upP();pushCurve();});
      wrap.appendChild(b);
    });
  }
  var _customByKey={};
  function renderCustomPresetButtons(){
    var wrap=document.getElementById('curve-custom-presets');if(!wrap)return;
    wrap.innerHTML='';
    _customByKey={};
    loadCustomPresets().forEach(function(cp,i){
      var key='custom:'+i;
      _customByKey[key]=cp.points;
      var b=document.createElement('button');
      b.dataset.preset=key;
      b.title=cp.name+' — clic : appliquer, clic droit : supprimer';
      b.innerHTML=buildThumbSvg(cp.points)+'<span>'+cp.name+'</span>';
      b.addEventListener('click',function(){setActivePoints(clonePts(cp.points));selected=null;draw();upP();pushCurve();});
      b.addEventListener('contextmenu',function(e){
        e.preventDefault();
        if(!window.confirm('Supprimer le preset "'+cp.name+'" ?'))return;
        var list=loadCustomPresets();list.splice(i,1);saveCustomPresets(list);renderCustomPresetButtons();
      });
      wrap.appendChild(b);
    });
  }
  renderPresetGallery();
  var savePresetBtn=document.getElementById('curve-save-preset');
  if(savePresetBtn)savePresetBtn.addEventListener('click',function(){
    var name=window.prompt('Nom du preset :');
    if(!name)return;
    var list=loadCustomPresets();
    list.push({name:name,points:clonePts(cs.points)});
    saveCustomPresets(list);
    renderCustomPresetButtons();
  });
  renderCustomPresetButtons();

  // Corner drag-resize — same thin-handle idiom as #tl-resize/#layer-panel-resize
  // elsewhere in this file, just diagonal. Only the CSS display size changes;
  // the coordinate math above already scales by (W/rect.width) etc, so it
  // stays correct at any display size without touching the internal buffer.
  var rh3=document.getElementById('curve-resize-handle');
  if(rh3){
    var rsx,rsy,rsw,rshh;
    rh3.addEventListener('mousedown',function(e){rsx=e.clientX;rsy=e.clientY;rsw=cvs.offsetWidth;rshh=cvs.offsetHeight;window._curveResize=true;e.preventDefault();e.stopPropagation();});
    window.addEventListener('mousemove',function(e){
      if(!window._curveResize)return;
      var nw=Math.max(140,Math.min(500,rsw+(e.clientX-rsx)));
      var nh=Math.max(140,Math.min(500,rshh+(e.clientY-rsy)));
      cvs.style.width=nw+'px';cvs.style.height=nh+'px';
    });
    window.addEventListener('mouseup',function(){window._curveResize=false;});
  }

  window._curveEditor={
    evalCurve:evalCurve,
    // Pure evaluator for an arbitrary points array — used at TWEEN
    // GENERATION time (tweens.js getEasingForPair), independent of whether
    // this widget is even open/editing that pair right now (evalCurve/
    // activePoints only reflect whatever's currently being VIEWED).
    evalPointsCurve:evalPointsCurve,
    getState:function(){return cs;},
    setState:function(s){
      // Backward-compat: projects saved before this rewrite stored
      // {p1x,p1y,p2x,p2y} (2 off-curve tangent handles) instead of
      // {points:[...]}—convert on load so old saves still open.
      if(s&&s.points)cs.points=clonePts(s.points);
      else if(s&&typeof s.p1x==='number')cs.points=[{x:0,y:0},{x:s.p1x,y:s.p1y},{x:s.p2x,y:s.p2y},{x:1,y:1}];
      selected=null;draw();upP();
    },
    draw:draw,
    // Camera-segment ease editing (feedback #5pi90) — see camEaseSeg above.
    // Clears motion mode too — only one "currently edited segment" at a
    // time on this one shared canvas.
    editCameraSeg:function(seg,label){
      motionEaseSeg=null;
      camEaseSeg=seg;camEaseLabel=label||'';
      if(window.openPropsSection)window.openPropsSection('easing-sec');
      draw();
    },
    exitCameraSeg:function(){
      if(!camEaseSeg)return;
      camEaseSeg=null;draw();
    },
    // Motion-segment points-based ease editing (2026-07) — see motionEaseSeg
    // above. `seg` is a motion key object (its own `.curvePoints` gets
    // lazily created/mutated in place, same live-reference contract
    // editCameraSeg already has for `.ease`).
    editMotionSeg:function(seg,label){
      camEaseSeg=null;
      motionEaseSeg=seg;motionEaseLabel=label||'';
      selected=null;
      if(window.openPropsSection)window.openPropsSection('easing-sec');
      draw();
    },
    exitMotionSeg:function(){
      if(!motionEaseSeg)return;
      motionEaseSeg=null;draw();
    },
    isCameraMode:isCamMode,
    isMotionMode:isMotionMode
  };
  setTimeout(draw,100);
})();

// Stroke/Fill swap button (Illustrator's "X") — was originally built as a
// left-panel light/dark theme toggle, corrected per explicit feedback: this
// button swaps the stroke and fill COLORS, not the toolbar's own UI colors.
(function(){
  var btn=document.getElementById('tools-invert-btn');
  if(!btn)return;
  btn.addEventListener('click',function(){window.SM.swapStrokeFill();});
})();

// ======== UI INTERACTIONS ========
(function(){
  document.querySelectorAll('.phdr').forEach(function(h){h.addEventListener('click',function(){if(window._secDragJustEnded){window._secDragJustEnded=false;return;}var b=this.nextElementSibling;if(!b||!b.classList.contains('pbdy'))return;b.classList.toggle('hid');this.classList.toggle('closed');});});

  // All right-panel sections start collapsed (Selection/Component Instance
  // are already display:none by default via their own conditional-show
  // logic — collapsing their body too is harmless, they just won't have
  // been expanded when they eventually become visible on selection).
  document.querySelectorAll('.psec .pbdy').forEach(function(b){b.classList.add('hid');});
  document.querySelectorAll('.psec .phdr').forEach(function(h){h.classList.add('closed');});

  // Panel-visibility-by-context is now owned by updatePropsContext() in
  // timeline.js (the unified Properties panel: Transform/Fill/Stroke/Tool
  // Options/Effects/Document, shown by priority — selection > active tool
  // > document), called from window.SM.setTool and updateUI(). This used
  // to be a standalone per-tool accordion-opener; superseded, not needed
  // here anymore.
  // (scrubbable number fields show their own value directly — no separate label span to sync anymore)
  var rh=document.getElementById('tl-resize'),ta=document.getElementById('timeline-area'),rsy,rsh;
  rh.addEventListener('mousedown',function(e){rsy=e.clientY;rsh=ta.offsetHeight;window._tlResize=true;e.preventDefault();});
  window.addEventListener('mousemove',function(e){if(!window._tlResize)return;ta.style.height=Math.max(80,Math.min(500,rsh+(rsy-e.clientY)))+'px';});
  window.addEventListener('mouseup',function(){window._tlResize=false;});

  // Layers panel horizontal resize — same drag-a-thin-bar pattern as the
  // timeline's own vertical #tl-resize above.
  var lpr=document.getElementById('layer-panel-resize'),lp=document.getElementById('layer-panel'),lprx,lprw;
  lpr.addEventListener('mousedown',function(e){lprx=e.clientX;lprw=lp.offsetWidth;window._lpResize=true;lpr.classList.add('active');e.preventDefault();});
  window.addEventListener('mousemove',function(e){if(!window._lpResize)return;lp.style.width=Math.max(90,Math.min(400,lprw+(e.clientX-lprx)))+'px';});
  window.addEventListener('mouseup',function(){window._lpResize=false;lpr.classList.remove('active');});

  // Left (tools) and right (props) panel horizontal resize — same
  // drag-a-thin-bar pattern as #layer-panel-resize above. Each move also
  // fires a window resize event: the drawing canvas's pixel size is set by
  // Paper.js's own window-resize listener, so without this the canvas kept
  // its old size while the panels moved, pushing the props panel clean off
  // the window edge (reported layout bug).
  var tpr=document.getElementById('tools-panel-resize'),tp=document.getElementById('tools-panel'),tprx,tprw;
  tpr.addEventListener('mousedown',function(e){tprx=e.clientX;tprw=tp.offsetWidth;window._tpResize=true;tpr.classList.add('active');e.preventDefault();});
  window.addEventListener('mousemove',function(e){if(!window._tpResize)return;tp.style.width=Math.max(50,Math.min(160,tprw+(e.clientX-tprx)))+'px';window.dispatchEvent(new Event('resize'));});
  window.addEventListener('mouseup',function(){if(!window._tpResize)return;window._tpResize=false;tpr.classList.remove('active');window.dispatchEvent(new Event('resize'));});

  var ppr=document.getElementById('props-panel-resize'),pp=document.getElementById('props-panel'),pprx,pprw;
  ppr.addEventListener('mousedown',function(e){pprx=e.clientX;pprw=pp.offsetWidth;window._ppResize=true;ppr.classList.add('active');e.preventDefault();});
  window.addEventListener('mousemove',function(e){if(!window._ppResize)return;pp.style.width=Math.max(240,Math.min(520,pprw-(e.clientX-pprx)))+'px';window.dispatchEvent(new Event('resize'));});
  window.addEventListener('mouseup',function(){if(!window._ppResize)return;window._ppResize=false;ppr.classList.remove('active');window.dispatchEvent(new Event('resize'));});

  // No local FC here (was a stale hardcoded 14, a duplicate of app.js's
  // global FC that drifted out of sync with it — real bug, caused the
  // work-area bar and onion markers to desync from the actual frame-cell
  // width). Every reference below resolves to app.js's global `FC` via the
  // normal scope chain instead, since none of these closures run until a
  // user actually drags — long after app.js (loaded after this file) has
  // set it.
  function initWaDrag(){
    var bar=document.getElementById('wa-bar'),hleft=bar.querySelector('.wa-handle.left'),hright=bar.querySelector('.wa-handle.right');
    var dragType=null,startX,origIn,origOut;
    function onDown(type,e){dragType=type;startX=e.clientX;origIn=window._waIn||0;origOut=window._waOut||23;e.stopPropagation();e.preventDefault();}
    hleft.addEventListener('mousedown',function(e){onDown('in',e);});
    hright.addEventListener('mousedown',function(e){onDown('out',e);});
    bar.addEventListener('mousedown',function(e){if(e.target===bar)onDown('both',e);});
    window.addEventListener('mousemove',function(e){
      if(!dragType)return;var dx=Math.round((e.clientX-startX)/FC);var total=window._totalF||24;
      if(dragType==='in')window._waIn=Math.max(0,Math.min(origIn+dx,(window._waOut||total-1)-1));
      else if(dragType==='out')window._waOut=Math.min(total-1,Math.max(origOut+dx,(window._waIn||0)+1));
      else{var w=origOut-origIn;var ni=Math.max(0,origIn+dx);if(ni+w>=total)ni=total-1-w;window._waIn=ni;window._waOut=ni+w;}
      // Commit to state FIRST, then read it back via updateWaBar() below —
      // see that function's own comment for why state is now the single
      // source of truth this bar renders from.
      if(window.SM)window.SM.setWorkArea(window._waIn,window._waOut);
      window.updateWaBar();
    });
    window.addEventListener('mouseup',function(){dragType=null;});
  }
  // Bug found 2026-07 ("le bleu de sélection... ne va pas jusqu'au bout"):
  // this used to read window._waIn/_waOut — a SEPARATE copy of the work
  // area range that initWaDrag keeps synced with state.waIn/waOut during
  // an active drag, but nothing guaranteed every OTHER code path that
  // mutates state.waOut (frame insert/clear, tween regen, project load —
  // timeline.js/tweens.js) also updated this window copy before the next
  // updateWaBar() call. Any one of those missed sync points left the bar
  // rendering a stale, shorter range than the project's real work area.
  // Reading state.waIn/waOut directly (the actual single source of truth
  // — see layerInPoint/layerOutPoint, app.js, for the same fix pattern)
  // makes that whole class of desync impossible; window._waIn/_waOut are
  // now only a live scratch value DURING a drag, in sync with state on
  // every mousemove (see above).
  window.updateWaBar=function(){
    var bar=document.getElementById('wa-bar');
    var inF=(window.state&&state.waIn!=null)?state.waIn:(window._waIn||0);
    var outF=(window.state&&state.waOut!=null)?state.waOut:(window._waOut||23);
    bar.style.left=(inF*FC)+'px';bar.style.width=((outF-inF+1)*FC)+'px';
    // Design audit 2026-07, round 2 (feedback: "un highlight dans la zone
    // de previz"): #wa-tint mirrors this same left/width down through the
    // actual scrub/preview area (#frame-grid), a second time — updateWaBar
    // was already the single choke point every code path (drag, project
    // load, frame insert/delete, zoom) runs through to keep #wa-bar synced,
    // so piggybacking here keeps the tint synced for free instead of
    // hunting down every call site a second time.
    var tint=document.getElementById('wa-tint');
    if(tint){
      tint.style.left=bar.style.left;tint.style.width=bar.style.width;
      var grid=document.getElementById('frame-grid'),wrap=document.getElementById('fg-wrap');
      if(grid&&wrap){
        tint.style.top=grid.offsetTop+'px';
        tint.style.height=Math.max(grid.scrollHeight,wrap.clientHeight-grid.offsetTop)+'px';
      }
    }
  };
  window._waIn=0;window._waOut=23;window._totalF=24;
  initWaDrag();

  function updateOnionBar(){
    var omIn=document.getElementById('om-in'),omOut=document.getElementById('om-out'),bar=document.getElementById('onion-bar');
    var inF=parseInt(omIn.dataset.frame||0),outF=parseInt(omOut.dataset.frame||23);
    // Design audit 2026-07 (feedback: "la barre gradient dépasse les in/out
    // point"): left/width used to be computed off the FRAME grid (inF*FC,
    // a full extra cell added via +1) with no relation to where the actual
    // .om-in/.om-out markers render (left:frame*FC+3, width:4px — see
    // updateOmMarkers below) — the bar's right edge landed a whole cell
    // past the out marker's own right edge. Now solved from the markers'
    // own geometry instead of re-deriving it: starts exactly at om-in's
    // left edge, ends exactly at om-out's right edge.
    bar.style.left=(inF*FC+3)+'px';bar.style.width=((outF-inF)*FC+4)+'px';
  }
  function initOmDrag(){
    var omIn=document.getElementById('om-in'),omOut=document.getElementById('om-out');
    var dragEl=null,startX2,origF;
    function onDown2(el,e){dragEl=el;startX2=e.clientX;origF=parseInt(el.dataset.frame||0);e.stopPropagation();e.preventDefault();}
    omIn.addEventListener('mousedown',function(e){onDown2(this,e);});
    omOut.addEventListener('mousedown',function(e){onDown2(this,e);});
    window.addEventListener('mousemove',function(e){
      if(!dragEl)return;var dx=Math.round((e.clientX-startX2)/FC);var nf=Math.max(0,Math.min((window._totalF||24)-1,origF+dx));
      dragEl.dataset.frame=nf;dragEl.style.left=(nf*FC+3)+'px';updateOnionBar();
      if(window.SM){var inF2=parseInt(document.getElementById('om-in').dataset.frame||0);var outF2=parseInt(document.getElementById('om-out').dataset.frame||23);window.SM.setOnionRange(inF2,outF2);
        // manual drag redefines the span the follow mode keeps around the playhead
        var cf2=window._curFrame||0;window._omSpan={prev:Math.max(0,cf2-inF2),next:Math.max(0,outF2-cf2)};
      }
    });
    window.addEventListener('mouseup',function(){dragEl=null;});
  }
  window._omFollow=true;window._omSpan={prev:2,next:2};
  window.updateOmMarkers=function(curF,totalF){
    var omIn=document.getElementById('om-in'),omOut=document.getElementById('om-out');
    // Bug found 2026-07 ("Quand l'onion skin est désactivé celles ci
    // devrait disparaitre"): the bar/markers used to render unconditionally
    // — no code path ever hid them when state.onionSkin was off, they just
    // sat there always-visible even though renderOS()'s own onion-drawing
    // is already gated by that same flag. #wa-bar (work area) is a
    // DIFFERENT feature living in the same #bars-row and stays visible
    // regardless — only the onion-specific elements toggle here.
    var onionOn=!!(window.state&&state.onionSkin);
    document.getElementById('onion-bar').style.display=onionOn?'':'none';
    omIn.style.display=onionOn?'':'none';
    omOut.style.display=onionOn?'':'none';
    if(!omIn.dataset.inited){omIn.dataset.frame=Math.max(0,curF-3);omOut.dataset.frame=Math.min(totalF-1,curF+3);omIn.dataset.inited='1';}
    if(window._omFollow){
      var span=window._omSpan||{prev:2,next:2};
      var inF=Math.max(0,curF-span.prev),outF=Math.min(totalF-1,curF+span.next);
      omIn.dataset.frame=inF;omOut.dataset.frame=outF;
      if(window.SM)window.SM.setOnionRange(inF,outF);
    }
    omIn.style.left=(parseInt(omIn.dataset.frame)*FC+3)+'px';omOut.style.left=(parseInt(omOut.dataset.frame)*FC+3)+'px';
    updateOnionBar();
  };
  initOmDrag();

  // Timeline scrubbing.
  // Coalesced on rAF: goToFrame is heavy synchronous work (layer save,
  // audio scrub, frame load kickoff, onion skins, full updateUI) and
  // mousemove fires several times per display refresh — calling it per
  // event saturates the main thread and the playhead visibly trails the
  // mouse (live-caught 2026-07, "le curseur de temps suit au ralenti la
  // souris"). One rAF applies only the LATEST mouse position, capping the
  // work at once per refresh while the cursor stays glued to the pointer.
  var scrubbing=false,_scrubPending=-1,_scrubRaf=0;
  function _scrubTo(frame){
    _scrubPending=frame;
    if(_scrubRaf)return;
    _scrubRaf=requestAnimationFrame(function(){
      _scrubRaf=0;
      var f=_scrubPending;_scrubPending=-1;
      if(f>=0&&window.SM)window.SM.goToFrame(f);
    });
  }
  document.getElementById('frame-hdr').addEventListener('mousedown',function(e){
    var wrap=document.getElementById('fg-wrap');var rect=wrap.getBoundingClientRect();
    var x=e.clientX-rect.left+wrap.scrollLeft;var frame=Math.floor(x/FC);
    if(frame>=0&&frame<(window._totalF||24)){scrubbing=true;if(window.SM){window.SM.stopPlay();window.SM.goToFrame(frame);}}
    e.preventDefault();
  });
  // Playhead flag handle (UI/UX audit, 2026-07 — see its own HTML/CSS
  // comments): grabbing it starts the exact same scrub as clicking the
  // ruler, just from a visible, always-grabbable marker instead of only
  // "click somewhere in this thin strip of frame cells".
  var playheadFlag=document.getElementById('playhead-flag');
  if(playheadFlag)playheadFlag.addEventListener('mousedown',function(e){
    scrubbing=true;if(window.SM)window.SM.stopPlay();
    e.preventDefault();e.stopPropagation();
  });
  window.addEventListener('mousemove',function(e){
    if(!scrubbing)return;var wrap=document.getElementById('fg-wrap');var rect=wrap.getBoundingClientRect();
    var x=e.clientX-rect.left+wrap.scrollLeft;var frame=Math.max(0,Math.min((window._totalF||24)-1,Math.floor(x/FC)));
    _scrubTo(frame);
  });
  window.addEventListener('mouseup',function(){scrubbing=false;});

  // Fill well toggle — same dblclick-to-toggle on the Properties panel's
  // swatch (#pm-fill), merging what used to be a separate "On" checkbox
  // into the swatch itself: click it to pick a color, double-click to
  // enable/disable fill. setFillEnabled() (timeline.js) keeps both swatches
  // in sync with each other.
  document.getElementById('fill-well').addEventListener('dblclick',function(e){
    e.preventDefault();e.stopPropagation();this.classList.toggle('none');
    if(window.SM)window.SM.setFillEnabled(!this.classList.contains('none'));
  });
  document.getElementById('pm-fill').addEventListener('dblclick',function(e){
    e.preventDefault();e.stopPropagation();this.classList.toggle('none');
    if(window.SM)window.SM.setFillEnabled(!this.classList.contains('none'));
  });

  // Generic right-click context menu (used by the frame grid and layer
  // list) — items: [{label, shortcut, action, disabled, sep}]. Positioned
  // at the click point, clamped to stay on-screen, dismissed on any outside
  // click/Escape/scroll so it never lingers.
  var ctxEl=null;
  function closeCtxMenu(){if(ctxEl){ctxEl.remove();ctxEl=null;}}
  window.showContextMenu=function(x,y,items){
    closeCtxMenu();
    var m=document.createElement('div');m.className='ctx-menu';
    items.forEach(function(it){
      if(it.sep){var s=document.createElement('div');s.className='ctx-sep';m.appendChild(s);return;}
      var row=document.createElement('div');row.className='ctx-item'+(it.disabled?' disabled':'');
      var lbl=document.createElement('span');lbl.textContent=it.label;row.appendChild(lbl);
      if(it.shortcut){var sc=document.createElement('span');sc.className='ctx-sc';sc.textContent=it.shortcut;row.appendChild(sc);}
      if(!it.disabled)row.addEventListener('click',function(e){e.stopPropagation();closeCtxMenu();it.action();});
      m.appendChild(row);
    });
    document.body.appendChild(m);
    var mw=m.offsetWidth,mh=m.offsetHeight;
    m.style.left=Math.min(x,window.innerWidth-mw-4)+'px';
    m.style.top=Math.min(y,window.innerHeight-mh-4)+'px';
    ctxEl=m;
  };
  window.addEventListener('mousedown',function(e){if(ctxEl&&!ctxEl.contains(e.target))closeCtxMenu();});
  // Canvas fix (2026-07, "le menu clic droit ne disparaît pas quand on
  // clique ailleurs"): the 'mousedown' listener above never fires when the
  // outside click lands on the canvas — every canvas tool bridge
  // (select-bridge.js etc.) intercepts 'pointerdown' at CAPTURE phase and
  // calls e.preventDefault(), which suppresses the browser's synthesized
  // legacy 'mousedown' compatibility event entirely. A capture-phase
  // listener on `document` for 'pointerdown' itself fires BEFORE any
  // capture listener on a descendant element (capture runs document→
  // target), so this always sees the click first regardless of what a
  // bridge does with it afterward — and it deliberately never calls
  // stopPropagation, so it's a passive dismiss-if-open check, not something
  // that could shadow any tool's own interception.
  document.addEventListener('pointerdown',function(e){if(ctxEl&&!ctxEl.contains(e.target))closeCtxMenu();},true);
  window.addEventListener('scroll',closeCtxMenu,true);
  window.addEventListener('keydown',function(e){if(e.key==='Escape')closeCtxMenu();});

  // Modern instant tooltips — native title tooltips are slow to appear and
  // unstyled; this swaps every title attribute for a shared styled tip on
  // first hover (delegated, so dynamically-created elements work too).
  var tip=document.createElement('div');tip.id='ui-tip';document.body.appendChild(tip);
  document.addEventListener('mouseover',function(e){
    var el=e.target.closest?e.target.closest('[title],[data-tip]'):null;
    if(!el){tip.classList.remove('show');return;}
    if(el.hasAttribute('title')){el.dataset.tip=el.getAttribute('title');el.removeAttribute('title');}
    var t=el.dataset.tip;if(!t){tip.classList.remove('show');return;}
    tip.textContent=t;tip.classList.add('show');
    var r=el.getBoundingClientRect();
    var tw=tip.offsetWidth,th=tip.offsetHeight;
    var x=Math.max(4,Math.min(window.innerWidth-tw-4,r.left+r.width/2-tw/2));
    var y=r.bottom+7;if(y+th>window.innerHeight-4)y=r.top-th-7;
    tip.style.left=x+'px';tip.style.top=y+'px';
  });
  document.addEventListener('mousedown',function(){tip.classList.remove('show');},true);

  // Detachable property panels — drag a section header to tear the section
  // off as a floating window that follows the cursor. Drop it back inside
  // the right panel to re-dock it (at the position under the cursor), drop
  // it anywhere else and it stays floating where you left it; its header
  // remains draggable to move it again. Docked order persists per label.
  var secDrag={el:null,startX:0,startY:0,started:false,offX:0,offY:0,wasFloating:false};
  function secKey(sec){var h=sec.querySelector('.phdr');return h?h.textContent.replace(/[^A-Za-z]/g,'').slice(0,20):'';}
  function saveSecOrder(){
    var pp=document.getElementById('props-panel');
    try{localStorage.setItem('nemo-panel-order',JSON.stringify(Array.prototype.slice.call(pp.querySelectorAll('.psec:not(.floating)')).map(secKey)));}catch(e){}
  }
  var savedOrder=null;
  try{savedOrder=JSON.parse(localStorage.getItem('nemo-panel-order')||'null');}catch(e){}
  if(savedOrder&&Array.isArray(savedOrder)){
    var pp0=document.getElementById('props-panel');
    savedOrder.forEach(function(k){
      var secs=Array.prototype.slice.call(pp0.querySelectorAll('.psec'));
      var m=secs.filter(function(s){return secKey(s)===k;})[0];
      if(m)pp0.appendChild(m);
    });
  }
  document.addEventListener('mousedown',function(e){
    var h=e.target.closest?e.target.closest('.phdr'):null;if(!h)return;
    var sec=h.parentElement;if(!sec.classList.contains('psec'))return;
    secDrag.el=sec;secDrag.startX=e.clientX;secDrag.startY=e.clientY;secDrag.started=false;
    secDrag.wasFloating=sec.classList.contains('floating');
    var r=sec.getBoundingClientRect();secDrag.offX=e.clientX-r.left;secDrag.offY=e.clientY-r.top;
  });
  window.addEventListener('mousemove',function(e){
    if(!secDrag.el)return;
    if(!secDrag.started){
      if(Math.abs(e.clientY-secDrag.startY)+Math.abs(e.clientX-secDrag.startX)<7)return;
      secDrag.started=true;
      if(!secDrag.el.classList.contains('floating')){
        var r2=secDrag.el.getBoundingClientRect();
        secDrag.el.classList.add('floating');
        secDrag.el.style.left=r2.left+'px';secDrag.el.style.top=r2.top+'px';
        document.body.appendChild(secDrag.el);
      }
      secDrag.el.classList.add('drag-sec');
    }
    // clamp so a floating panel can never be dropped outside the app window
    var fw=secDrag.el.offsetWidth,fh=secDrag.el.offsetHeight;
    secDrag.el.style.left=Math.max(0,Math.min(window.innerWidth-fw,e.clientX-secDrag.offX))+'px';
    secDrag.el.style.top=Math.max(0,Math.min(window.innerHeight-fh,e.clientY-secDrag.offY))+'px';
    document.querySelectorAll('.psec.drag-over-sec').forEach(function(s){s.classList.remove('drag-over-sec');});
    var ppr=document.getElementById('props-panel').getBoundingClientRect();
    if(e.clientX>=ppr.left&&e.clientX<=ppr.right&&e.clientY>=ppr.top&&e.clientY<=ppr.bottom){
      secDrag.el.style.pointerEvents='none';
      var under=document.elementFromPoint(e.clientX,e.clientY);
      secDrag.el.style.pointerEvents='';
      var over=under&&under.closest?under.closest('.psec'):null;
      if(over&&over!==secDrag.el)over.classList.add('drag-over-sec');
    }
  });
  window.addEventListener('mouseup',function(e){
    if(!secDrag.el)return;
    if(secDrag.started){
      var pp3=document.getElementById('props-panel');
      var ppr2=pp3.getBoundingClientRect();
      var inside=e.clientX>=ppr2.left&&e.clientX<=ppr2.right&&e.clientY>=ppr2.top&&e.clientY<=ppr2.bottom;
      if(inside){
        var over2=document.querySelector('.psec.drag-over-sec');
        secDrag.el.classList.remove('floating');
        secDrag.el.style.left='';secDrag.el.style.top='';
        if(over2)pp3.insertBefore(secDrag.el,over2);
        else pp3.appendChild(secDrag.el);
        saveSecOrder();
      }
      window._secDragJustEnded=true;
      document.querySelectorAll('.psec').forEach(function(s){s.classList.remove('drag-sec','drag-over-sec');});
    }
    secDrag.el=null;secDrag.started=false;
  });
  window.addEventListener('resize',function(){
    document.querySelectorAll('.psec.floating').forEach(function(s){
      s.style.left=Math.max(0,Math.min(window.innerWidth-s.offsetWidth,parseInt(s.style.left)||0))+'px';
      s.style.top=Math.max(0,Math.min(window.innerHeight-s.offsetHeight,parseInt(s.style.top)||0))+'px';
    });
  });

  // Rive-style scrubbable numeric fields, replacing every slider in the
  // right panel: drag left/right on the field to change its value (hold
  // Shift for fine 0.1-step adjustments, Alt for x10 coarse steps — same
  // modifier convention Rive/AE use), or just click to place a caret and
  // type a value directly. A plain click (no drag) never fires a change,
  // so tabbing/typing behaves exactly like a normal number input.
  // Pointer Events + explicit setPointerCapture, not raw mousedown/mousemove/
  // mouseup on window: the previous version tracked drag state in a plain
  // closure variable cleared on a *global* window 'mouseup' listener — if
  // that mouseup was ever missed (button released while the pointer had
  // drifted over another element/window that swallowed it, or any other
  // listener elsewhere calling stopPropagation/preventDefault on the
  // gesture) the state var was NEVER cleared, so every future mousemove
  // anywhere kept computing a new value from the stale startX/startVal —
  // exactly the reported "value keeps changing no matter what I do
  // afterward" bug. Pointer capture makes the target element itself the
  // guaranteed recipient of every subsequent pointer event up to and
  // including pointerup/pointercancel, regardless of where the cursor
  // physically ends up, which is the robust, standard fix for this whole
  // class of "drag state got stuck" bug.
  // v13: every drawing tool bridge (draw/pen/eraser/select/shape/subselect/
  // perspective/viewtools-bridge.js) registers its own pointerdown/move/up
  // with {capture:true} on #canvas-area AS A WHOLE (not just the actual
  // <canvas>), and none of them check the event's target before deciding
  // whether to intercept — so ANY click anywhere inside #canvas-area,
  // including UI chrome overlaid on top of it, gets swallowed by whichever
  // tool is currently active before it ever reaches the element the user
  // actually clicked. Confirmed real bug: the canvas zoom-scrub pill (a
  // child of #canvas-area) couldn't be dragged OR typed into — draw-bridge's
  // onDown ran first on every click and called stopImmediatePropagation.
  // Fixed centrally here rather than patching all 9 bridge files: a
  // document-level capture listener runs before #canvas-area's own capture
  // listener (capture order follows DOM position, root to target, not
  // registration order), so stopping propagation here keeps the event from
  // ever reaching those bridges at all when it started on real UI chrome.
  document.addEventListener('pointerdown',function(e){
    if(e.target.closest&&e.target.closest('#canvas-zoom-pills'))e.stopPropagation();
  },true);
  var scrubState=null;
  document.addEventListener('pointerdown',function(e){
    var el=e.target.closest&&e.target.closest('input.scrub');
    if(!el||document.activeElement===el)return;
    scrubState={el:el,pointerId:e.pointerId,startX:e.clientX,startVal:parseFloat(el.value)||0,moved:false};
    el.setPointerCapture(e.pointerId);
    e.preventDefault();
  });
  // Live scrub (2026-07-17, "les valeurs qui se changent en drag doivent
  // se refléter en temps réel dans le canvas, pas juste au relâchement") :
  // pendant le drag, on dispatch le VRAI événement 'change' à chaque tick
  // (coalescé sur rAF — les handlers lourds comme motion.js's scrubField
  // rebuildent des pans entiers d'UI, inutile de le faire au-delà de
  // 60Hz), pas seulement au release. Les handlers 'change' existants font
  // donc leur travail normal (setValue + renderNow + refresh) en continu —
  // zéro modification champ par champ, tout input.scrub de l'app devient
  // live d'un coup (panel droit, timeline Motion, partout).
  //
  // Contrepartie undo : la plupart de ces handlers commencent par
  // pushUndo() — un snapshot par tick de drag aurait pollué la pile (des
  // dizaines d'entrées pour UN geste) et, pire, le snapshot du release
  // aurait capturé l'état déjà-final (Ctrl+Z = no-op perçu). D'où
  // window._scrubLiveActive : UN pushUndo réel au premier mouvement du
  // drag (snapshot pré-geste), puis pushUndoLayers (tweens.js) NO-OP tant
  // que le flag est levé — y compris pendant le 'change' final du release.
  // Un geste = une entrée d'undo, qui restaure l'état d'avant le drag.
  var scrubChangeRaf=0;
  function dispatchLiveChange(){
    if(scrubChangeRaf)return;
    scrubChangeRaf=requestAnimationFrame(function(){
      scrubChangeRaf=0;
      if(scrubState&&scrubState.moved)scrubState.el.dispatchEvent(new Event('change',{bubbles:true}));
    });
  }
  document.addEventListener('pointermove',function(e){
    if(!scrubState||e.pointerId!==scrubState.pointerId)return;
    var dx=e.clientX-scrubState.startX;
    if(!scrubState.moved){
      if(Math.abs(dx)<3)return;
      scrubState.moved=true;scrubState.el.classList.add('scrubbing');
      if(window.pushUndo)window.pushUndo(); // snapshot pré-geste, AVANT de lever le flag
      window._scrubLiveActive=true;
    }
    var step=parseFloat(scrubState.el.dataset.step)||1;
    if(e.shiftKey)step*=0.1;else if(e.altKey)step*=10;
    var raw=scrubState.startVal+Math.round(dx/4)*step;
    var min=scrubState.el.min!==''?parseFloat(scrubState.el.min):null;
    var max=scrubState.el.max!==''?parseFloat(scrubState.el.max):null;
    if(min!==null)raw=Math.max(min,raw);
    if(max!==null)raw=Math.min(max,raw);
    var decimals=(String(step).split('.')[1]||'').length;
    scrubState.el.value=decimals?raw.toFixed(decimals):Math.round(raw);
    scrubState.el.dispatchEvent(new Event('input',{bubbles:true}));
    dispatchLiveChange();
  });
  function endScrub(e){
    if(!scrubState||(e&&e.pointerId!==undefined&&e.pointerId!==scrubState.pointerId))return;
    if(scrubChangeRaf){cancelAnimationFrame(scrubChangeRaf);scrubChangeRaf=0;}
    if(!scrubState.moved){scrubState.el.focus();scrubState.el.select();}
    else{
      scrubState.el.classList.remove('scrubbing');
      // 'change' final DANS la fenêtre du flag — son pushUndo interne
      // reste no-op, voir le commentaire de tête.
      scrubState.el.dispatchEvent(new Event('change',{bubbles:true}));
      window._scrubLiveActive=false;
    }
    scrubState=null;
  }
  document.addEventListener('pointerup',endScrub);
  document.addEventListener('pointercancel',endScrub);
  // Last-resort safety net: even with pointer capture this should never be
  // needed, but if the pointer capture itself is ever lost/released by the
  // browser without a matching pointerup (e.g. devtools interfering, or the
  // window losing focus mid-drag), don't leave a scrub stuck forever.
  window.addEventListener('blur',function(){scrubState=null;window._scrubLiveActive=false;});
})();
