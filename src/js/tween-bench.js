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
  var FLAG_NAMES=['TW_MATCH_RELATIONAL','TW_MATCH_MULTI_MOTION','TW_REL_2OPT','TW_MATCH_TRACKING','TW_PIECE_COMPLETION',
    'TW_MOTION_FIELD','TW_CHIRALITY','TW_MATCH_WIDTH','TW_MATCH_TOPOLOGY','TW_MATCH_REGIONS','TW_PROVENANCE_PINS','TW_PIN_IN_SOLVER','TW_ORPHAN_FOLLOW'];
  var NEW_FLAGS=['TW_MOTION_FIELD','TW_CHIRALITY','TW_MATCH_WIDTH','TW_MATCH_TOPOLOGY','TW_MATCH_REGIONS','TW_PROVENANCE_PINS','TW_PIN_IN_SOLVER','TW_ORPHAN_FOLLOW'];
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
    var pairs=res.pairSpecs.map(function(ps){return{a:ps.aIdx,b:ps.bIdx,score:+ps.score.toFixed(3),forced:!!ps.forced,provenance:!!ps.provenance,piece:!!ps.isPiece,completion:!!ps.completion};});
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
  B.snapshot=function(){var c=document.querySelector('#canvas')||document.querySelector('canvas');return c?c.toDataURL('image/png'):null;};
  window.__twBench=B;
})();
