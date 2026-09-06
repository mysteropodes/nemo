// ---- Tween matcher bench (2026-09-04, audit autotween) ----
// Dev-only console API: window.__twBench. Nothing here runs on its own.
// Purpose: make every matcher change MEASURABLE from the browser preview,
// on the exact production path (_spanPairSpecs, tweens.js), without
// generating frames or touching the document.
//
//   __twBench.load('/__tmp_x.json')       load a project file served from src/
//   __twBench.keys(li)                    drawn keyframes of a layer
//   __twBench.span(li,fA,fB)              production assignment on a COPY of the two keys
//   __twBench.corpus(li)                  every span of a layer, summary + signature
//   __twBench.diff(li,{TW_MOTION_FIELD:false})  spans whose pairs change under other flags
//   __twBench.synthetic(li,fA,{level:'medium',seeds:5})  ground-truth bench on key fA
//   __twBench.suite(li,fA)                the three levels, flags on vs off
//   __twBench.overlay(li,fA,fB)           draw A (red), B (blue), pairs (green) in a temp layer
//   __twBench.clearOverlay()
//   __twBench.flags() / setFlags({...})   read / set the TW_* switches (tweens.js globals)
//
// The overlay lives in the Paper canvas: visible only while the Rust engine
// is OFF (a fresh preview tab), see CLAUDE.md §4.
(function(){
  if(typeof window==='undefined')return;
  var B={};
  var FLAG_NAMES=['TW_MATCH_RELATIONAL','TW_MATCH_MULTI_MOTION','TW_REL_2OPT','TW_MATCH_TRACKING','TW_PIECE_COMPLETION','TW_MATCH_AXIS','TW_MATCH_TURNING','TW_MATCH_GAP','TW_REL_REVERSE','TW_OC_SIDE_GUARD','TW_ID_PINS','TW_ARC_FROM_CHAIN','TW_TWIN_GROUPS','TW_TEMPORAL_PRIOR',
    'TW_MOTION_FIELD','TW_CHIRALITY','TW_MATCH_WIDTH','TW_MATCH_TOPOLOGY','TW_MATCH_REGIONS','TW_PROVENANCE_PINS','TW_PIN_IN_SOLVER','TW_ORPHAN_FOLLOW'];
  var NEW_FLAGS=['TW_MOTION_FIELD','TW_CHIRALITY','TW_MATCH_WIDTH','TW_MATCH_TOPOLOGY','TW_MATCH_REGIONS','TW_PROVENANCE_PINS','TW_PIN_IN_SOLVER','TW_ORPHAN_FOLLOW','TW_MATCH_AXIS','TW_MATCH_TURNING','TW_MATCH_GAP','TW_REL_REVERSE','TW_OC_SIDE_GUARD','TW_ID_PINS','TW_ARC_FROM_CHAIN','TW_TWIN_GROUPS','TW_TEMPORAL_PRIOR'];
  B.NEW_FLAGS=NEW_FLAGS;
  B.flags=function(){var o={};FLAG_NAMES.forEach(function(k){o[k]=window[k];});return o;};
  B.setFlags=function(obj){var prev={};Object.keys(obj||{}).forEach(function(k){prev[k]=window[k];window[k]=obj[k];});return prev;};
  B.withFlags=function(obj,fn){var prev=B.setFlags(obj);try{return fn();}finally{B.setFlags(prev);}};

  function keysOf(li){var ld=state.layers[li];var ks=[];for(var i=0;i<state.totalFrames;i++)if(ld.frames[i]&&ld.frames[i].isKeyframe&&ld.frames[i].strokes.length)ks.push(i);return ks;}
  B.keys=function(li){li=li===undefined?state.activeLayerIdx:li;return keysOf(li);};
  B.layers=function(){return state.layers.map(function(l,li){return{li:li,name:l.name,keys:keysOf(li)};});};
  B.load=async function(url){
    var r=await fetch(url,{cache:'no-store'});if(!r.ok)throw new Error('fetch '+url+' → '+r.status);
    var txt=await r.text();window.SM.importJSON(txt,true);
    return B.layers();
  };

  // ---- production assignment on a copy ----
  function runSpan(ldCopy,li,fA,fB,prev){
    var t0=performance.now();
    var res=_spanPairSpecs(ldCopy,li,fA,fB,prev);
    var ms=performance.now()-t0;
    if(!res)return{empty:true,ms:ms};
    var pairs=res.pairSpecs.map(function(ps){return{a:ps.aIdx,b:ps.bIdx,score:+ps.score.toFixed(3),forced:!!ps.forced,provenance:!!ps.provenance,piece:!!ps.isPiece,completion:!!ps.completion,identity:!!ps.identity,twin:!!ps.twin};});
    pairs.sort(function(p,q){return p.a-q.a||p.b-q.b;});
    var sig=pairs.filter(function(p){return!p.piece;}).map(function(p){return p.a+'>'+p.b;}).join(' ');
    return{ms:ms,nA:res.sA.length,nB:res.sB.length,pairs:pairs,unA:res.unA.slice(),unB:res.unB.slice(),sig:sig,
      rescued:pairs.filter(function(p){return p.score>0.48&&!p.forced;}).length,
      pieces:pairs.filter(function(p){return p.piece;}).length,
      sA:res.sA,sB:res.sB};
  }
  function copyLd(ld,fA,fB){var c={frames:{}};c.frames[fA]=JSON.parse(JSON.stringify(ld.frames[fA]));c.frames[fB]=JSON.parse(JSON.stringify(ld.frames[fB]));return c;}
  B.span=function(li,fA,fB){
    li=li===undefined?state.activeLayerIdx:li;
    var ld=state.layers[li],keys=keysOf(li);
    if(fA===undefined){fA=keys[0];fB=keys[1];}
    var ki=keys.indexOf(fA);
    var prev=(ki>0)?JSON.parse(JSON.stringify(ld.frames[keys[ki-1]].strokes)):null;
    return runSpan(copyLd(ld,fA,fB),li,fA,fB,prev);
  };
  B.corpus=function(li){
    li=li===undefined?state.activeLayerIdx:li;
    var keys=keysOf(li),out=[];
    for(var k=0;k<keys.length-1;k++){
      var r=B.span(li,keys[k],keys[k+1]);
      out.push({fA:keys[k],fB:keys[k+1],nA:r.nA,nB:r.nB,pairs:r.pairs?r.pairs.length:0,fadeA:r.unA?r.unA.length:0,fadeB:r.unB?r.unB.length:0,rescued:r.rescued,pieces:r.pieces,ms:+r.ms.toFixed(1),sig:r.sig});
    }
    return out;
  };
  B.diff=function(li,flagsB){
    var a=B.corpus(li),b=B.withFlags(flagsB,function(){return B.corpus(li);});
    var out=[];
    a.forEach(function(sa,i){
      var sb=b[i];if(sa.sig===sb.sig)return;
      var A=sa.sig.split(' '),Bs=sb.sig.split(' ');
      out.push({fA:sa.fA,fB:sa.fB,onlyCurrent:A.filter(function(x){return Bs.indexOf(x)<0;}),onlyOther:Bs.filter(function(x){return A.indexOf(x)<0;}),fadesCurrent:sa.fadeA+'/'+sa.fadeB,fadesOther:sb.fadeA+'/'+sb.fadeB});
    });
    return{spans:a.length,changed:out.length,details:out};
  };

  // ---- synthetic ground truth ----
  function seededRng(seed){var s=(seed>>>0)||1;return function(){s=(s*1664525+1013904223)>>>0;return s/4294967296;};}
  function ptsOf(sd){var segs=(sd.isVectorBrush&&sd.centerSegments&&sd.centerSegments.length>1)?sd.centerSegments:sd.segments;return segs?segs.map(function(s){return s.point;}):[];}
  function bboxOf(list){var x1=Infinity,y1=Infinity,x2=-Infinity,y2=-Infinity;list.forEach(function(sd){ptsOf(sd).forEach(function(p){if(p[0]<x1)x1=p[0];if(p[0]>x2)x2=p[0];if(p[1]<y1)y1=p[1];if(p[1]>y2)y2=p[1];});});return{x1:x1,y1:y1,x2:x2,y2:y2,w:x2-x1,h:y2-y1,diag:Math.hypot(x2-x1,y2-y1)};}
  var LEVELS={
    light:{jitter:0.10,redraw:false,drop:0.0,rot:12,scl:[0.92,1.10],tr:0.10},
    medium:{jitter:0.25,redraw:false,drop:0.10,rot:20,scl:[0.85,1.20],tr:0.15},
    redraw:{jitter:0.40,redraw:true,drop:0.20,rot:25,scl:[0.80,1.25],tr:0.18}
  };
  // Smooth articulated deformation: 3 anchors, each a similarity, blended
  // by Gaussians — the same family the #739 bench used (a HARD region split
  // produced fake errors between two ticks 19 px apart; deformation must
  // be smooth).
  function makeField(bb,lv,rng){
    var anchors=[];
    for(var k=0;k<3;k++){
      var ax=bb.x1+rng()*bb.w,ay=bb.y1+rng()*bb.h;
      var th=(rng()*2-1)*lv.rot*Math.PI/180,sc=lv.scl[0]+rng()*(lv.scl[1]-lv.scl[0]);
      var tx=(rng()*2-1)*lv.tr*bb.diag,ty=(rng()*2-1)*lv.tr*bb.diag;
      anchors.push({ax:ax,ay:ay,c:Math.cos(th)*sc,s:Math.sin(th)*sc,tx:tx,ty:ty});
    }
    var sig2=2*Math.pow(0.5*bb.diag,2);
    return function(x,y){
      var sx=0,sy=0,sw=0;
      anchors.forEach(function(a){
        var w=Math.exp(-((x-a.ax)*(x-a.ax)+(y-a.ay)*(y-a.ay))/sig2)+1e-6;
        var dx=x-a.ax,dy=y-a.ay;
        sx+=w*(a.ax+a.c*dx-a.s*dy+a.tx);sy+=w*(a.ay+a.s*dx+a.c*dy+a.ty);sw+=w;
      });
      return{x:sx/sw,y:sy/sw};
    };
  }
  function redrawStroke(sd,rng){
    var c=JSON.parse(JSON.stringify(sd));
    function thin(segs){if(segs.length<=3)return segs;var out=[];for(var i=0;i<segs.length;i++){if(i===0||i===segs.length-1||i%2===0)out.push(segs[i]);}return out;}
    function noise(segs){segs.forEach(function(s){var f=1+(rng()*2-1)*0.2;s.handleIn=[s.handleIn[0]*f,s.handleIn[1]*f];s.handleOut=[s.handleOut[0]*f,s.handleOut[1]*f];});}
    if(c.isVectorBrush&&c.centerSegments&&c.centerSegments.length>1){c.centerSegments=thin(c.centerSegments);noise(c.centerSegments);try{c.segments=outlineFromCenterSegs(c.centerSegments);c.closed=true;}catch(e){}}
    else if(c.segments){c.segments=thin(c.segments);noise(c.segments);}
    return c;
  }
  B.makeSynthetic=function(li,fA,opts){
    opts=opts||{};var lv=LEVELS[opts.level||'medium'];if(!lv)throw new Error('level: light|medium|redraw');
    var rng=seededRng(opts.seed||1);
    var ld=state.layers[li];
    var A=splitTweenables(JSON.parse(JSON.stringify(ld.frames[fA].strokes)),false).list;
    A.forEach(function(sd){delete sd.strokeId;delete sd.dupOf;});
    var bb=bboxOf(A);if(!(bb.diag>0))throw new Error('empty key');
    var field=makeField(bb,lv,rng);
    var Bs=[],gt=[];
    A.forEach(function(sd,i){
      if(rng()<lv.drop)return;
      var pts=ptsOf(sd),sb=bboxOf([sd]);var size=Math.max(1,sb.diag);
      var jx=(rng()*2-1)*lv.jitter*size,jy=(rng()*2-1)*lv.jitter*size;
      var src=lv.redraw?redrawStroke(sd,rng):sd;
      var moved=_transportStroke(src,function(x,y){var q=field(x,y);return{x:q.x+jx,y:q.y+jy};});
      Bs.push(moved);gt.push(i);
    });
    // shuffle B
    for(var i2=Bs.length-1;i2>0;i2--){var j=Math.floor(rng()*(i2+1));var t=Bs[i2];Bs[i2]=Bs[j];Bs[j]=t;var g=gt[i2];gt[i2]=gt[j];gt[j]=g;}
    return{A:A,B:Bs,gt:gt,level:opts.level||'medium',seed:opts.seed||1};
  };
  B.scoreSynthetic=function(syn,li){
    var ldc={frames:{}};ldc.frames[0]={strokes:syn.A,isKeyframe:true};ldc.frames[1]={strokes:syn.B,isKeyframe:true};
    var r=runSpan(ldc,li===undefined?state.activeLayerIdx:li,0,1,null);
    if(r.empty)return{errors:0,n:0};
    var gtOfB=syn.gt;var present={};syn.gt.forEach(function(a){present[a]=1;});
    var correct=0,wrong=0,falsePair=0,pieces=0,matchedA={};
    r.pairs.forEach(function(p){
      if(p.piece){pieces++;return;}
      matchedA[p.a]=1;
      if(gtOfB[p.b]===p.a)correct++;else if(present[p.a])wrong++;else falsePair++;
    });
    var miss=0;syn.A.forEach(function(sd,i){if(present[i]&&!matchedA[i])miss++;});
    return{n:syn.gt.length,correct:correct,wrong:wrong,miss:miss,falsePair:falsePair,pieces:pieces,errors:wrong+miss+falsePair,ms:+r.ms.toFixed(1)};
  };
  B.synthetic=function(li,fA,opts){
    opts=opts||{};li=li===undefined?state.activeLayerIdx:li;if(fA===undefined)fA=keysOf(li)[0];
    var seeds=opts.seeds||5,tot={n:0,correct:0,wrong:0,miss:0,falsePair:0,pieces:0,errors:0,ms:0},per=[];
    for(var s=1;s<=seeds;s++){
      var syn=B.makeSynthetic(li,fA,{level:opts.level||'medium',seed:s});
      var sc=B.scoreSynthetic(syn,li);per.push(sc);
      Object.keys(tot).forEach(function(k){tot[k]+=sc[k]||0;});
    }
    tot.ms=+(tot.ms/seeds).toFixed(1);tot.level=opts.level||'medium';tot.seeds=seeds;tot.per=per;
    return tot;
  };
  // The three levels, current flags vs all NEW flags off (baseline = what
  // main + #740 do today).
  B.suite=function(li,fA,seeds){
    li=li===undefined?state.activeLayerIdx:li;if(fA===undefined)fA=keysOf(li)[0];
    var off={};NEW_FLAGS.forEach(function(k){off[k]=false;});
    var rows=[];
    ['light','medium','redraw'].forEach(function(lv){
      var cur=B.synthetic(li,fA,{level:lv,seeds:seeds||5});
      var base=B.withFlags(off,function(){return B.synthetic(li,fA,{level:lv,seeds:seeds||5});});
      rows.push({level:lv,n:cur.n,errors_new:cur.errors,errors_base:base.errors,wrong:cur.wrong+'/'+base.wrong,miss:cur.miss+'/'+base.miss,falsePair:cur.falsePair+'/'+base.falsePair,ms:cur.ms+'/'+base.ms});
    });
    return rows;
  };
  // One flag at a time: each NEW flag off individually, others as current.
  B.ablation=function(li,fA,level,seeds){
    li=li===undefined?state.activeLayerIdx:li;if(fA===undefined)fA=keysOf(li)[0];
    var cur=B.synthetic(li,fA,{level:level||'medium',seeds:seeds||5}).errors,rows=[{flag:'(all on)',errors:cur}];
    NEW_FLAGS.forEach(function(k){var o={};o[k]=false;rows.push({flag:k+' off',errors:B.withFlags(o,function(){return B.synthetic(li,fA,{level:level||'medium',seeds:seeds||5});}).errors});});
    return rows;
  };

  // ---- overlay ----
  B._ov=null;
  B.clearOverlay=function(){if(B._ov){B._ov.remove();B._ov=null;if(window.view)view.update();}};
  function strokePath(sd,color,width){
    var segs=(sd.isVectorBrush&&sd.centerSegments&&sd.centerSegments.length>1)?sd.centerSegments:sd.segments;
    var p=new Path({strokeColor:color,strokeWidth:width||1.5,insert:true});
    segs.forEach(function(s){p.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
    return p;
  }
  B.overlay=function(li,fA,fB,opts){
    B.clearOverlay();
    li=li===undefined?state.activeLayerIdx:li;
    var r=B.span(li,fA,fB);if(r.empty)return r;
    var prev=project.activeLayer;
    var L=new Layer();L.name='__twBenchOverlay';B._ov=L;
    r.sA.forEach(function(sd){strokePath(sd,'#e84a5f',1.5);});
    r.sB.forEach(function(sd){strokePath(sd,'#3a8ee6',1.5);});
    r.pairs.forEach(function(p){
      var ca=_quickCentroid(r.sA[p.a]),cb=_quickCentroid(p.piece?r.sB[p.b]:r.sB[p.b]);if(!ca||!cb)return;
      var ln=new Path.Line(new Point(ca[0],ca[1]),new Point(cb[0],cb[1]));
      ln.strokeColor=p.forced?'#f2a541':(p.score>0.48?'#c07be0':'#2ecc71');ln.strokeWidth=p.forced?2.5:1.5;
      var tx=new PointText(new Point((ca[0]+cb[0])/2,(ca[1]+cb[1])/2));tx.content=p.a+'>'+p.b;tx.fontSize=10;tx.fillColor='#111';
    });
    r.unA.forEach(function(i){var c=_quickCentroid(r.sA[i]);if(c){var ci=new Path.Circle(new Point(c[0],c[1]),5);ci.strokeColor='#e84a5f';ci.strokeWidth=2;}});
    r.unB.forEach(function(i){var c=_quickCentroid(r.sB[i]);if(c){var ci=new Path.Circle(new Point(c[0],c[1]),5);ci.strokeColor='#3a8ee6';ci.strokeWidth=2;}});
    prev.activate();
    if(window.view)view.update();
    return{pairs:r.pairs,unA:r.unA,unB:r.unB,ms:r.ms};
  };
  // ---- FOLDS : auto-intersections inventées par l'interpolation ----
  // Les clés font foi : si un trait ne se croise pas dans les deux clés et
  // se croise dans une image générée, l'interpolation a replié le trait.
  // C'est LA métrique qui isole le défaut « la shape se retourne ».
  function _selfX(sd){
    var s=(sd.centerSegments&&sd.centerSegments.length>1)?sd.centerSegments:sd.segments;
    if(!s||s.length<4)return 0;
    var P=s.map(function(q){return q.point;}),n=0;
    function cr(o,p,q){return (p[0]-o[0])*(q[1]-o[1])-(p[1]-o[1])*(q[0]-o[0]);}
    function it(a,b,c,d){var d1=cr(c,d,a),d2=cr(c,d,b),d3=cr(a,b,c),d4=cr(a,b,d);
      return((d1>0&&d2<0)||(d1<0&&d2>0))&&((d3>0&&d4<0)||(d3<0&&d4>0));}
    for(var i=0;i+1<P.length;i++)for(var j=i+2;j+1<P.length;j++){
      if(i===0&&j+1===P.length-1)continue;
      if(it(P[i],P[i+1],P[j],P[j+1]))n++;}
    return n;
  }
  B.folds=function(li){
    li=li===undefined?state.activeLayerIdx:li;
    state.activeLayerIdx=li; generateTweens();
    var ld=state.layers[li],keys=keysOf(li),out=[],total=0,keyX=0;
    keys.forEach(function(k){ (ld.frames[k].strokes||[]).forEach(function(sd){keyX+=_selfX(sd);}); });
    for(var f=0;f<state.totalFrames;f++){
      var fr=ld.frames[f]; if(!fr||!fr.isInterpolated)continue;
      var x=0;(fr.strokes||[]).forEach(function(sd){x+=_selfX(sd);});
      if(x){out.push({frame:f,crossings:x});total+=x;}
    }
    return{keyCrossings:keyX, generatedFramesWithCrossings:out.length, totalCrossings:total, frames:out};
  };
  // ---- AUDIT MULTI-FICHIERS (2026-09-05) ----------------------------
  // Cyril : « as-tu testé sur mes autres anims ? ». Métriques SANS vérité
  // terrain, donc applicables à n'importe quel fichier : elles ne disent
  // pas « c'est faux », elles DÉSIGNENT LES SUSPECTS à regarder à l'œil.
  //  - travel   : un trait dont le centroïde parcourt beaucoup plus que la
  //               médiane de la portée ET une part notable du dessin ;
  //               c'est la signature d'un trait qui traverse le dessin.
  //  - lenRatio : une paire dont les longueurs sont dans un rapport > 2.
  //  - folds    : traits (> 25 px) qui s'auto-intersectent en Bézier dans
  //               une image générée alors que les clés ne le font pas.
  //  - fades    : traits laissés en fondu de chaque côté.
  function _flatten(S,k){
    var P=[],i,s2;
    for(i=0;i+1<S.length;i++){
      var a=S[i],b=S[i+1],p0=a.point,p3=b.point;
      var p1=[p0[0]+(a.handleOut?a.handleOut[0]:0),p0[1]+(a.handleOut?a.handleOut[1]:0)];
      var p2=[p3[0]+(b.handleIn?b.handleIn[0]:0),p3[1]+(b.handleIn?b.handleIn[1]:0)];
      for(s2=0;s2<k;s2++){var t=s2/k,u=1-t;
        P.push([u*u*u*p0[0]+3*u*u*t*p1[0]+3*u*t*t*p2[0]+t*t*t*p3[0],
                u*u*u*p0[1]+3*u*u*t*p1[1]+3*u*t*t*p2[1]+t*t*t*p3[1]]);}
    }
    P.push(S[S.length-1].point);return P;
  }
  function _xings(P){
    function cr(o,p,q){return (p[0]-o[0])*(q[1]-o[1])-(p[1]-o[1])*(q[0]-o[0]);}
    function it(a,b,c,d){var d1=cr(c,d,a),d2=cr(c,d,b),d3=cr(a,b,c),d4=cr(a,b,d);
      return((d1>0&&d2<0)||(d1<0&&d2>0))&&((d3>0&&d4<0)||(d3<0&&d4>0));}
    var n=0,i,j;
    for(i=0;i+1<P.length;i++)for(j=i+2;j+1<P.length;j++)if(it(P[i],P[i+1],P[j],P[j+1]))n++;
    return n;
  }
  function _segsOf(sd){return (sd.centerSegments&&sd.centerSegments.length>1)?sd.centerSegments:sd.segments;}
  function _span2(S){var x1=1/0,y1=1/0,x2=-1/0,y2=-1/0;S.forEach(function(s){
    if(s.point[0]<x1)x1=s.point[0];if(s.point[0]>x2)x2=s.point[0];
    if(s.point[1]<y1)y1=s.point[1];if(s.point[1]>y2)y2=s.point[1];});
    return Math.max(x2-x1,y2-y1);}
  B.suspects=function(li){
    li=li===undefined?state.activeLayerIdx:li;
    state.activeLayerIdx=li;
    var ld=state.layers[li],keys=keysOf(li);
    // --- appariement, portée par portée
    var travel=[],lenR=[],fades=0,pairs=0;
    for(var k=0;k<keys.length-1;k++){
      var r=B.span(li,keys[k],keys[k+1]);
      if(!r||r.empty)continue;
      var sA=ld.frames[keys[k]].strokes||[],sB=ld.frames[keys[k+1]].strokes||[];
      var bx1=1/0,by1=1/0,bx2=-1/0,by2=-1/0;
      [sA,sB].forEach(function(L){L.forEach(function(sd){var S=_segsOf(sd);if(!S)return;
        S.forEach(function(s){if(s.point[0]<bx1)bx1=s.point[0];if(s.point[0]>bx2)bx2=s.point[0];
          if(s.point[1]<by1)by1=s.point[1];if(s.point[1]>by2)by2=s.point[1];});});});
      var diag=Math.hypot(bx2-bx1,by2-by1)||1;
      var ds=[],specs=[];
      r.pairs.forEach(function(p){
        var a=sA[p.a],b=sB[p.b];if(!a||!b)return;
        var fa=strokeFeat(a),fb=strokeFeat(b);
        var d=Math.hypot(fb.cx-fa.cx,fb.cy-fa.cy);
        ds.push(d);specs.push({a:p.a,b:p.b,d:d,la:fa.length,lb:fb.length});
      });
      pairs+=specs.length;fades+=(r.unA?r.unA.length:0)+(r.unB?r.unB.length:0);
      var med=ds.slice().sort(function(x,y){return x-y;})[Math.floor(ds.length/2)]||1;
      specs.forEach(function(sp){
        if(sp.d>Math.max(3*med,0.12*diag)&&sp.d>30)
          travel.push({span:keys[k]+'>'+keys[k+1],a:sp.a,b:sp.b,px:Math.round(sp.d),medPx:Math.round(med)});
        var lr=Math.max(sp.la,sp.lb)/Math.max(1,Math.min(sp.la,sp.lb));
        if(lr>2&&Math.min(sp.la,sp.lb)>20)
          lenR.push({span:keys[k]+'>'+keys[k+1],a:sp.a,b:sp.b,ratio:+lr.toFixed(1)});
      });
    }
    // --- replis dans les images générées
    generateTweens();
    var keyX=0,folds=[];
    keys.forEach(function(kf){(ld.frames[kf].strokes||[]).forEach(function(sd){
      var S=_segsOf(sd);if(S&&S.length>3&&_span2(S)>25&&_xings(_flatten(S,6)))keyX++;});});
    for(var f=0;f<state.totalFrames;f++){
      var fr=ld.frames[f];if(!fr||!fr.isInterpolated)continue;
      var c=0;(fr.strokes||[]).forEach(function(sd){
        var S=_segsOf(sd);if(S&&S.length>3&&_span2(S)>25&&_xings(_flatten(S,6)))c++;});
      if(c)folds.push({frame:f,n:c});
    }
    return{pairs:pairs,fades:fades,keyCrossings:keyX,
      folds:folds.reduce(function(a,x){return a+x.n;},0),foldFrames:folds.length,
      travel:travel,lenRatio:lenR};
  };
  // Compare la config courante à une config de référence sur le MÊME fichier.
  B.auditFile=function(li,baseFlags){
    var cur=B.suspects(li);
    var base=B.withFlags(baseFlags,function(){return B.suspects(li);});
    function sum(x){return{pairs:x.pairs,fades:x.fades,folds:x.folds,
      travel:x.travel.length,lenRatio:x.lenRatio.length};}
    return{courant:sum(cur),reference:sum(base),
      keyCrossings:cur.keyCrossings,
      travelDetail:cur.travel.slice(0,8),lenDetail:cur.lenRatio.slice(0,8),
      travelRef:base.travel.slice(0,8)};
  };
  // ---- MAINTIENS (2026-09-05) ----------------------------------------
  // Un maintien = deux clés consécutives identiques. Tout trait généré qui
  // s'en éloigne est une erreur certaine, sans vérité terrain. C'est ce
  // test qui a révélé la régression des épingles lisant des identifiants
  // réécrits par la génération en cours — invisible pour `span()`, qui
  // apparie sur des données fraîches. À passer sur la SORTIE de
  // generateTweens, jamais sur une copie.
  function _pathCentroid(sd){var f=strokeFeat(sd);return[f.cx,f.cy];}
  B.holds=function(li){
    li=li===undefined?state.activeLayerIdx:li;state.activeLayerIdx=li;
    generateTweens();
    var ld=state.layers[li],keys=keysOf(li),out=[];
    for(var k=0;k+1<keys.length;k++){
      var A=ld.frames[keys[k]].strokes||[],Bs=ld.frames[keys[k+1]].strokes||[];
      if(A.length!==Bs.length||!A.length)continue;
      var same=A.every(function(s,i){var b=Bs[i];if(!b)return false;var p=_pathCentroid(s),q=_pathCentroid(b);return Math.abs(p[0]-q[0])<0.5&&Math.abs(p[1]-q[1])<0.5;});
      if(!same)continue;
      var byId={};A.forEach(function(s){var id=(s.strokeId||'').split('#')[0];if(id&&!byId[id])byId[id]=_pathCentroid(s);});
      var mx=0,n=0;
      for(var f=keys[k]+1;f<keys[k+1];f++)(ld.frames[f].strokes||[]).forEach(function(sd){
        var c=byId[(sd.strokeId||'').split('#')[0]];if(!c)return;
        var q=_pathCentroid(sd),d=Math.hypot(q[0]-c[0],q[1]-c[1]);if(d>5)n++;if(d>mx)mx=d;});
      out.push({span:keys[k]+'>'+keys[k+1],deplaces:n,maxPx:Math.round(mx)});
    }
    return out;
  };
  // ---- VÉRIFICATION COMPLÈTE EN UNE COMMANDE (2026-09-05) --------------
  // Cyril : « vérifie à chaque fois sur plusieurs anim différentes ».
  // Tout ce qui a servi de garde-fou dans la session, sur les six fichiers :
  //  - accord avec l'identité des traits (copies fraîches, repère biaisé sur
  //    un fichier déjà tweené : les ids y portent d'anciennes décisions),
  //  - trajectoires croisées, sur le même sous-ensemble de paires,
  //  - replis inventés et maintiens, sur la SORTIE réelle de generateTweens,
  //  - les cas validés à l'œil sur cats (patte, moustaches, deux bras),
  //  - le banc synthétique.
  // `flags` : configuration à comparer à la courante (ex. {TW_X:false}).
  function _truthOf(r){var cA={},cB={};
    r.sA.forEach(function(sd){if(sd.strokeId)cA[sd.strokeId]=(cA[sd.strokeId]||0)+1;});
    r.sB.forEach(function(sd){if(sd.strokeId)cB[sd.strokeId]=(cB[sd.strokeId]||0)+1;});
    var t={};r.sA.forEach(function(sd,ai){var id=sd.strokeId;if(!id||cA[id]!==1||cB[id]!==1)return;
      for(var bi=0;bi<r.sB.length;bi++)if(r.sB[bi].strokeId===id){t[ai]=bi;break;}});return t;}
  function _segX(a,b,c,d){function cr(o,p,q){return (p[0]-o[0])*(q[1]-o[1])-(p[1]-o[1])*(q[0]-o[0]);}
    var d1=cr(c,d,a),d2=cr(c,d,b),d3=cr(a,b,c),d4=cr(a,b,d);
    return((d1>0&&d2<0)||(d1<0&&d2>0))&&((d3>0&&d4<0)||(d3<0&&d4>0));}
  function _crossOf(r,as){var P=Object.keys(as).map(function(a){return{a:+a,b:as[a]};}),FA={},FB={};
    P.forEach(function(p){FA[p.a]=FA[p.a]||strokeFeat(r.sA[p.a]);FB[p.b]=FB[p.b]||strokeFeat(r.sB[p.b]);});
    var n=0;for(var i=0;i<P.length;i++)for(var j=i+1;j<P.length;j++){var a1=FA[P[i].a],b1=FB[P[i].b],a2=FA[P[j].a],b2=FB[P[j].b];
      if(_segX([a1.cx,a1.cy],[b1.cx,b1.cy],[a2.cx,a2.cy],[b2.cx,b2.cy]))n++;}return n;}
  function _foldsReal(li){var ld=state.layers[li],n=0;
    for(var f=0;f<state.totalFrames;f++){var fr=ld.frames[f];if(!fr||!fr.isInterpolated)continue;
      (fr.strokes||[]).forEach(function(sd){var S=_segsOf(sd);if(S&&S.length>3&&_span2(S)>25&&_xings(_flatten(S,6)))n++;});}
    return n;}
  B.FILES=['__tmp_cats.json','__tmp_untitled3.json','__tmp_untitled4.json','__tmp_totale.json','__tmp_traits.json','__tmp_b.json','__tmp_testanim.json'];
  B.auditAll=async function(flags,opts){
    flags=flags||{};opts=opts||{};var li=0,out={};
    for(var fi=0;fi<B.FILES.length;fi++){
      var file=B.FILES[fi],name=file.replace('__tmp_','').replace('.json','');
      await B.load('/'+file);state.activeLayerIdx=li;
      var keys=keysOf(li),cross=0,ag=0,tot=0,pins=0;
      B.withFlags(flags,function(){for(var k=0;k+1<keys.length;k++){var r=B.span(li,keys[k],keys[k+1]);if(!r||r.empty)continue;
        var t=_truthOf(r),eng={};r.pairs.forEach(function(p){if(p.piece)return;eng[p.a]=p.b;if(p.identity)pins++;});
        var sub={};Object.keys(t).forEach(function(a){if(eng[a]!==undefined){sub[a]=eng[a];tot++;if(eng[a]===t[a])ag++;}});
        cross+=_crossOf(r,sub);}});
      await B.load('/'+file);state.activeLayerIdx=li;
      var h=B.withFlags(flags,function(){return B.holds(li);});   // holds() appelle generateTweens
      var folds=_foldsReal(li);
      var hd=h.reduce(function(a,x){return a+x.deplaces;},0);
      out[name]={accord:ag+'/'+tot,crois:cross,replis:folds,maintiensDeplaces:h.length?hd:'-'};
    }
    // cas validés à l'œil, cats
    await B.load('/__tmp_cats.json');state.activeLayerIdx=li;
    var chk=function(fA,fB,idx){var r=B.span(li,fA,fB);return idx.map(function(a){var p=null;for(var i=0;i<r.pairs.length;i++)if(r.pairs[i].a===a){p=r.pairs[i];break;}
      if(!p)return a+'>fade';return a+'>'+p.b+(p.identity?'*':'')+(p.twin?'†':'');}).join(' ');};
    out.controles=B.withFlags(flags,function(){return{patte_7_17:chk(6,16,[20]),moustaches_7_17:chk(6,16,[7,8,9]),moustaches_35_47:chk(34,46,[7,8,9,10,11,12]),bras_26_35:chk(25,34,[15,16]),bras_35_47:chk(34,46,[15,18,25])};});
    if(!opts.noBench){var b0=B.withFlags(flags,function(){return B.suite(li,0,5);}),b6=B.withFlags(flags,function(){return B.suite(li,6,5);});
      out.banc=b0.concat(b6).reduce(function(a,x){return a+x.errors_new;},0);}
    return out;
  };
  // ---- journal d'appariement (chantier 0.3) : lecture humaine ----
  // window.__TW_DEBUG_MATCH=true ; générer ; B.journal() = dernière portée,
  // B.journal(k) = k-ième, B.journal(fA,fB) = la portée fA→fB. Renvoie un
  // texte et l'affiche en console.
  B.journal=function(a,b){
    var L=window.__twMatchLog||[];if(!L.length)return 'journal vide (mettre window.__TW_DEBUG_MATCH=true avant de générer)';
    var J=null;
    if(b!==undefined){for(var i=0;i<L.length;i++)if(L[i].fA===a&&L[i].fB===b)J=L[i];}
    else J=(a===undefined)?L[L.length-1]:L[a];
    if(!J)return 'portée introuvable';
    var out=[];
    out.push('PORTÉE '+J.fA+'→'+J.fB+' : '+J.n+' traits en A, '+J.m+' en B');
    if(J.epingles.length){out.push('ÉPINGLES');J.epingles.forEach(function(e){out.push('  '+e.type+' : '+e.idA+' → '+e.idB);});}
    J.passes.forEach(function(p){
      out.push('PASSE '+p.passe);
      p.paires.forEach(function(e){
        if(e.b<0){out.push('  '+e.idA+' → fondu'+(e.meilleur?' (meilleur candidat '+e.meilleur.idB+' à '+e.meilleur.cout+')':''));return;}
        var flags=[];if(e.mutuel)flags.push('mutuel');if(e.marge!==null&&e.marge<0.05)flags.push('MARGE FAIBLE');
        out.push('  '+e.idA+' → '+e.idB+'  coût '+e.cout+'  alt ligne '+e.altLigne+' / col '+e.altCol+'  marge '+e.marge+'  fondu '+(e.margeFondu===null?'?':(e.margeFondu>=0?'+':'')+e.margeFondu)+(e.canal?'  ['+e.canal+']':'')+(flags.length?'  '+flags.join(', '):''));
      });
    });
    J.etapes.forEach(function(e){
      if(e.changements){
        if(!e.changements.length){out.push('ÉTAPE '+e.etape+' : aucun changement');return;}
        out.push('ÉTAPE '+e.etape+(e.raison?' ('+e.raison+')':''));
        e.changements.forEach(function(c){out.push('  '+c.idA+' : '+c.idDe+' → '+c.idVers);});
        return;
      }
      var parts=[];for(var k in e){if(k==='etape'||k==='paires')continue;var v=e[k];if(v&&typeof v==='object')v=JSON.stringify(v);parts.push(k+'='+v);}
      if(e.paires){parts.push('paires='+e.paires.map(function(q){return q.idA+'→'+q.idB+(q.score!==undefined?'('+q.score+')':'');}).join(' '));}
      out.push('NOTE '+e.etape+' : '+parts.join('  '));
    });
    if(J.final){
      out.push('BILAN');
      J.final.forEach(function(f){out.push('  '+f.idA+' → '+f.idB+'  ['+f.source+']'+(f.score!==null?'  '+f.score:''));});
      if(J.fondusA&&J.fondusA.length)out.push('  fondus A : '+J.fondusA.map(function(x){return x.idA;}).join(' '));
      if(J.fondusB&&J.fondusB.length)out.push('  fondus B : '+J.fondusB.map(function(x){return x.idB;}).join(' '));
    }
    var txt=out.join('\n');console.log(txt);return txt;
  };
  B.snapshot=function(){var c=document.querySelector('#canvas')||document.querySelector('canvas');return c?c.toDataURL('image/png'):null;};
  window.__twBench=B;
})();
