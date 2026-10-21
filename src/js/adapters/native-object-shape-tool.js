/* Ordinary native shape and bounded freehand gestures. Only pointer-local data and SVG preview live here;
 * the existing vector client and Project resource queue own every committed command. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectShapeTool=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var tools=['line','rect','ellipse'],shapeTools=tools.concat(['star','speechbubble']);
  var pinKeys=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  function same(a,b){return !!a&&!!b&&pinKeys.every(function(k){return a[k]===b[k];});}
  function fail(message){throw new Error(message);}
  function consume(e){if(e&&e.stopImmediatePropagation)e.stopImmediatePropagation();if(e&&e.preventDefault)e.preventDefault();}
  function create(root,ports){
    var Mapping=ports.mapping;
    var admitted=ports.freehand?['draw']:tools,handled=ports.freehand?['draw','fillbrush']:shapeTools;
    var drag=null,pending=null,uncertain=null,overlay=null,retired=false,settled=Promise.resolve({status:'idle'});
    function session(){return root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root);}
    function nativeMode(){try{var s=session();return !!s&&s.blocksLegacy();}catch(_){return true;}}
    function notice(message){if(typeof root.showToast==='function')root.showToast(message);}
    function stageOrigin(e){var d=root.document;return e.target===d.getElementById('drawing-canvas')||e.target===d.getElementById('canvas-area');}
    function bounds(info){
      var d=root.document,el=d.getElementById('drawing-canvas'),r=el&&el.getBoundingClientRect(),html=d.documentElement;
      if(!r||![r.left,r.top,r.width,r.height,info.canvasW,info.canvasH].every(Number.isFinite)||r.width<=0||r.height<=0||info.canvasW<=0||info.canvasH<=0||
          r.left<0||r.top<0||r.left+r.width>html.clientWidth||r.top+r.height>html.clientHeight||!html.classList.contains('nemo-native-underlay'))fail('Native canvas presentation is unavailable.');
      var edges=[r.left,r.top,r.left+r.width,r.top+r.height];
      if(!['left','top','right','bottom'].every(function(k,i){return parseFloat(html.style.getPropertyValue('--nemo-native-'+k))===edges[i];}))fail('Native canvas presentation changed.');
      return {left:r.left,top:r.top,width:r.width,height:r.height};
    }
    // Confirmed CSS -> native composition pixels -> the painted receipt's inverse.
    // Canvas uses identity; Paper pan/zoom never supplies a native coordinate proof.
    function point(e,t){if(!Number.isFinite(e.clientX)||!Number.isFinite(e.clientY))fail('Pointer position is unavailable.');
      var p=Mapping.point(e,{bounds:t.bounds,canvasW:t.pin.canvasW,canvasH:t.pin.canvasH,presentation:t.presentation});return [p.x,p.y];}
    function style(tool,proof){
      var s=root.state,current=session(),ownerInfo=current&&current.info(),port=root.NemoNativePerspectiveController;
      var perspective=port&&(proof?typeof port.readForCommand==='function'&&port.readForCommand(proof):port.read());
      if(s.appMode!=='anim2d'||s.playing||s.spaceDown||s.isPanning||s.shadowMode||s.maskMode||s.symmetryEnabled||!perspective||!same(perspective.info,ownerInfo)||perspective.perspective.settings.perspectiveEnabled||s.activeSymbolId)fail('This drawing mode is unavailable in native shape tools.');
      if(ports.freehand)return ports.style();
      if(typeof s.strokeEnabled!=='boolean'||typeof s.fillEnabled!=='boolean'||(!s.strokeEnabled&&(tool==='line'||!s.fillEnabled)))fail('Enable a supported stroke or fill to draw.');
      if(!Number.isFinite(s.brushSize)||s.brushSize<=0||!Number.isFinite(s.opacity)||s.opacity<0||s.opacity>100)fail('Native shape style is unavailable.');
      var hex=/^#(?:[a-fA-F0-9]{3}|[a-fA-F0-9]{6}|[a-fA-F0-9]{8})$/;
      if(s.strokeEnabled&&!hex.test(s.strokeColor)||tool!=='line'&&s.fillEnabled&&!hex.test(s.fillColor))fail('Native shapes require solid colours.');
      if(!['solid','dashed','dotted'].includes(s.strokeStyle)||tool!=='line'&&s.strokeStyle!=='solid')fail('This shape stroke style is unavailable.');
      var out={strokeColor:s.strokeEnabled?s.strokeColor:null,hasRealStroke:s.strokeEnabled,strokeWidth:s.brushSize,
        fillColor:tool!=='line'&&s.fillEnabled?s.fillColor:null,opacity:s.opacity/100,
        strokeCap:tool==='line'?s.strokeCap:'butt',strokeJoin:'miter',miterLimit:10,dashOffset:0};
      // Reuse applyStrokeStyle on a detached plain style, never a Paper item.
      if(tool==='line')Object.assign(out,ports.lineStyle());
      if(!['butt','round','square'].includes(out.strokeCap))fail('Native stroke cap is unavailable.');
      return out;
    }
    function selection(info){
      var s=root.state,index=s.activeLayerIdx,row=s.layers&&s.layers[index],selected=root._layerSel;
      if(!Number.isSafeInteger(index)||!row||!Array.isArray(selected)||selected.length!==1||selected[0]!==index)fail('Select one native Drawing layer.');
      var layer=info.layers.filter(function(l){return l.layerUid===row.layerUid;});
      if(layer.length!==1||layer[0].locked===true||layer[0].visible===false||layer[0].parentLayerUid||row.threeD||row.motion&&Object.keys(row.motion).length)fail('The selected native layer is unavailable for this gesture.');
      return row.layerUid;
    }
    function capture(e){
      if(!Mapping||['point','transform','matchesView'].some(function(key){return typeof Mapping[key]!=='function';}))fail('Native canvas mapping is unavailable.');
      var s=session(),pin=s&&s.info(),state=root.state,frame=state.currentFrame,shown=s&&s.readPresentedObjectSession();
      if(!s||!s.isActive()||!pin||typeof s.runResourceCommand!=='function'||!shown||!same(pin,shown)||!same(pin,shown.presentation)||shown.presentation.owner!=='native'||shown.presentation.status!=='presented'||
          !Mapping.matchesView(root,shown.presentation)||shown.presentation.contextId!=='scene-root'||shown.presentation.frame!==frame||!Number.isSafeInteger(frame)||frame<0||frame>=pin.totalFrames||
          !root.NemoNativeObjectProjectFile||root.NemoNativeObjectProjectFile.projectionReadyForUI(root,pin)!==true||!root.NemoNativeObjectVector)fail('Native Drawing is not ready.');
      var perspective=root.NemoNativePerspectiveController&&root.NemoNativePerspectiveController.read();
      var t={perspective:perspective,session:s,pin:pin,presentation:shown.presentation,frame:frame,tool:state.tool,layerUid:selection(pin),style:style(state.tool,perspective),bounds:bounds(pin),pointer:e.pointerId,cancelled:false};
      t.start=point(e,t);if(ports.freehand)t.samples=ports.begin(t.start,e,t);return t;
    }
    function current(t,requirePresentation){
      try{var s=session(),pin=s&&s.info();
        if(retired||t.cancelled||!Mapping.matchesView(root,t.presentation)||s!==t.session||!s.isActive()||!s.blocksLegacy()||!same(pin,t.pin)||root.state.currentFrame!==t.frame||root.state.tool!==t.tool||selection(pin)!==t.layerUid||
            root.NemoNativeObjectProjectFile.projectionReadyForUI(root,pin)!==true||JSON.stringify(style(t.tool,requirePresentation?undefined:t.perspective))!==JSON.stringify(t.style)||JSON.stringify(bounds(pin))!==JSON.stringify(t.bounds))return false;
        var shown=requirePresentation&&s.readPresentedObjectSession();
        return !requirePresentation||!!shown&&same(shown,t.pin)&&shown.presentation===t.presentation;
      }catch(_){return false;}
    }
    function clear(){if(overlay){overlay.remove();overlay=null;}}
    function release(t){try{var el=root.document.getElementById('canvas-area')||root.document.getElementById('drawing-canvas');if(el.releasePointerCapture)el.releasePointerCapture(t.pointer);}catch(_){} }
    function cancel(){if(drag){var t=drag;drag=null;t.cancelled=true;clear();release(t);}return {status:'cancelled'};}
    function endpoint(t,e){var end=point(e,t);return !ports.freehand&&e.shiftKey?ports.constrain(t.tool,t.start,end):end;}
    function dictionary(t,end){
      if(ports.freehand)return ports.dictionary(t.samples,t.style);
      var g=ports.geometry(t.tool,t.start[0],t.start[1],end[0],end[1]);
      var out=Object.assign({},t.style,{closed:g.closed,segments:g.segments.map(function(s){return {point:s.point.slice(),handleIn:(s.handleIn||[0,0]).slice(),handleOut:(s.handleOut||[0,0]).slice()};})});
      if(t.tool==='rect')out.paramShape={kind:'rect',tl:0,tr:0,br:0,bl:0,box:{x1:Math.min(t.start[0],end[0]),y1:Math.min(t.start[1],end[1]),x2:Math.max(t.start[0],end[0]),y2:Math.max(t.start[1],end[1])}};
      return out;
    }
    function preview(t,end){
      var d=root.document,dict=dictionary(t,end);if(!dict)return;var segments=dict.segments;
      if(!overlay){overlay=d.createElementNS('http://www.w3.org/2000/svg','svg');overlay.setAttribute('aria-hidden','true');
        Object.assign(overlay.style,{position:'fixed',pointerEvents:'none',zIndex:'20',left:t.bounds.left+'px',top:t.bounds.top+'px',width:t.bounds.width+'px',height:t.bounds.height+'px'});
        overlay.setAttribute('viewBox','0 0 '+t.pin.canvasW+' '+t.pin.canvasH);overlay.setAttribute('preserveAspectRatio','none');
        overlay.appendChild(d.createElementNS('http://www.w3.org/2000/svg','path'));d.body.appendChild(overlay);}
      var path=overlay.firstChild;path.setAttribute('transform','matrix('+Mapping.transform({presentation:t.presentation},false).join(' ')+')');var start=segments[0].point,text='M '+start[0]+' '+start[1],count=segments.length+(dict.closed?1:0);
      for(var i=1;i<count;i++){var a=segments[i-1],b=segments[i%segments.length];text+=' C '+(a.point[0]+a.handleOut[0])+' '+(a.point[1]+a.handleOut[1])+' '+(b.point[0]+b.handleIn[0])+' '+(b.point[1]+b.handleIn[1])+' '+b.point[0]+' '+b.point[1];}
      if(dict.closed)text+=' Z';path.setAttribute('d',text);path.setAttribute('fill',dict.fillColor||'none');path.setAttribute('stroke',dict.strokeColor||'none');
      path.setAttribute('stroke-width',dict.strokeWidth);path.setAttribute('stroke-linecap',dict.strokeCap);path.setAttribute('stroke-linejoin',dict.strokeJoin);path.setAttribute('opacity',dict.opacity);
      if(dict.dashArray)path.setAttribute('stroke-dasharray',dict.dashArray.join(' '));path.setAttribute('stroke-dashoffset',dict.dashOffset);path.setAttribute('stroke-miterlimit',dict.miterLimit);
      path.setAttribute('paint-order',dict.paintOrder==='strokeFirst'?'stroke fill':'fill stroke');
    }
    function commit(t,end){
      if(!current(t,true))return Promise.resolve({status:'cancelled'});
      if(!ports.freehand&&Math.hypot(end[0]-t.start[0],end[1]-t.start[1])<2)return Promise.resolve({status:'cancelled'});
      var dict=dictionary(t,end),s=t.session;
      // No second queue: only add a current-intent check to the real Project
      // resource action. While that action runs, its own pending count hides presentation.
      var guarded={info:s.info,isActive:s.isActive,requestId:s.requestId,runResourceCommand:function(action){return s.runResourceCommand(function(ctx){
        if(!current(t,false))throw Object.assign(new Error('Native shape intent changed'),{code:'unavailable',reason:'intent_changed'});
        return action({reconcile:ctx.reconcile,dispatch:function(q){if(!current(t,false))throw Object.assign(new Error('Native shape intent changed'),{code:'unavailable',reason:'intent_changed'});return ctx.dispatch(q);}});
      });}};
      return root.NemoNativeObjectVector.create(guarded).createStroke(t.layerUid,t.frame,dict,{origin:t.pin});
    }
    // Geometry/mapping can refuse synchronously before the existing Project
    // queue starts. Always return a promise so release finalization clears
    // pending, without classifying a pre-dispatch refusal as owner uncertainty.
    function commitAttempt(t,end){
      try{return Promise.resolve(commit(t,end));}
      catch(_){return Promise.resolve({status:'unavailable',reason:'pre_dispatch_failed'});}
    }
    function handle(kind,e){
      if(retired)return false;
      // Chrome controls share the capture ancestor. Handle their event without
      // consuming it or falling through to the legacy shape handler.
      if(kind==='down'&&!stageOrigin(e))return !!drag||nativeMode()&&handled.includes(root.state.tool);
      if(drag){consume(e);if(e.pointerId!==drag.pointer)return true;
        if(kind==='cancel'){cancel();return true;}
        if(!current(drag,true)){cancel();return true;}
        var t=drag,end;
        try{end=endpoint(t,e);if(ports.freehand&&(kind==='move'||kind==='up')){
            var events=kind==='move'&&typeof e.getCoalescedEvents==='function'?e.getCoalescedEvents():null;
            (events&&events.length?events:[e]).forEach(function(sample){
              if(sample.pointerId!==undefined&&sample.pointerId!==t.pointer)fail('Brush sample belongs to another pointer.');
              ports.sample(t.samples,point(sample,t),sample,kind,t);});
          }
          if(kind==='move'){preview(t,end);return true;}
          if(kind==='up'){drag=null;clear();pending=t;settled=commitAttempt(t,end).catch(function(){return {status:'unconfirmed'};}).then(function(result){
            if(result.status==='indeterminate'||result.status==='unconfirmed')uncertain=t;
            if(!retired&&result.status!=='applied'&&result.status!=='noop'&&result.status!=='cancelled')notice('Native shape drawing was not confirmed.');return result;
          }).finally(function(){if(pending===t)pending=null;});release(t);return true;}
        }catch(_){cancel();notice('Native shape drawing is unavailable.');return true;}
        return true;
      }
      if(!nativeMode()||!handled.includes(root.state.tool))return false;
      if(!stageOrigin(e))return true;
      consume(e);if(kind!=='down'||pending)return true;
      if(uncertain){var owner=session(),pin=owner&&owner.info();
        if(owner===uncertain.session&&pin&&['instanceId','documentId','lifecycleGeneration'].every(function(k){return pin[k]===uncertain.pin[k];})){notice('Reopen the native project before drawing again; the previous result is unconfirmed.');return true;}
        uncertain=null;
      }
      if(!admitted.includes(root.state.tool)){notice('This shape tool is not available in native Drawing.');return true;}
      if(e.button!==0||e.isPrimary===false||!Number.isSafeInteger(e.pointerId))return true;
      try{drag=capture(e);var el=root.document.getElementById('canvas-area')||root.document.getElementById('drawing-canvas');if(el.setPointerCapture)el.setPointerCapture(e.pointerId);preview(drag,drag.start);}
      catch(error){cancel();notice(error.message||'Native Drawing is unavailable.');}return true;
    }
    function key(e){if(e.key==='Escape'&&drag){consume(e);cancel();}}
    root.document.addEventListener('keydown',key,{capture:true});
    return Object.freeze({handle:handle,cancel:cancel,whenSettled:function(){return settled;},isDragging:function(){return !!drag;},
      dispose:function(){retired=true;cancel();if(pending)pending.cancelled=true;root.document.removeEventListener('keydown',key,{capture:true});}});
  }
  // Composition of existing brush filters and detached geometry; no Paper
  // insertion, document writer or second queue. Shared by the real draw route.
  function freehandPorts(root,filters) {
    var Point=root.Point,Path=root.Path;
    var buildCenterSegmentsFromRawStroke=root.buildCenterSegmentsFromRawStroke,applyTaperToCenterSegments=root.applyTaperToCenterSegments;
    var sampleVectorBrushCenterline=root.sampleVectorBrushCenterline,buildVariableWidthPath=root.buildVariableWidthPath;
    var simplifyIfNeeded=root.simplifyIfNeeded,applyStrokeStyle=root.applyStrokeStyle;
    function nativeDrawStyle() {
      var state=root.state;
      if (state.tool !== 'draw' || state.bitmapBrushOn || state.trimStrokeEnds || state.drawMode !== 'front' ||
          state.brushPreset && state.brushPreset !== 'none' || root.SMLabs && root.SMLabs.list().some(function(p){return p.on;}))
        throw new Error('This brush combination is unavailable in native Drawing.');
      if (typeof state.vectorBrush !== 'boolean' || typeof state.strokeEnabled !== 'boolean' || typeof state.fillEnabled !== 'boolean' ||
          !state.strokeEnabled && !state.fillEnabled || state.vectorBrush && (!state.strokeEnabled || state.fillEnabled))
        throw new Error('Native pressure Drawing requires one stroke ribbon without a linked fill.');
      var keys = ['vectorBrush','strokeEnabled','fillEnabled','strokeColor','fillColor','brushSize','opacity','strokeCap','strokeJoin',
        'strokeStyle','miterLimit','dashOffset','paintOrder','smoothing','stabilizer','taperEnds','pressureMin','pressureMax',
        'pressureInvert','pressureCurve','pressureCurvePoints','brushAngle','brushAngleFactor'];
      var out = {}; keys.forEach(function(k){if(state[k] !== undefined)out[k]=state[k];});
      var hex = /^#(?:[a-fA-F0-9]{3}|[a-fA-F0-9]{6}|[a-fA-F0-9]{8})$/;
      if (state.strokeEnabled && !hex.test(state.strokeColor) || state.fillEnabled && !hex.test(state.fillColor) ||
          !Number.isFinite(state.brushSize) || state.brushSize <= 0 || !Number.isFinite(state.opacity) || state.opacity < 0 || state.opacity > 100 ||
          !Number.isFinite(state.smoothing) || state.smoothing < 0 || !Number.isInteger(state.stabilizer) || state.stabilizer < 0 || state.stabilizer > 6 ||
          !['butt','round','square'].includes(state.strokeCap) || !['miter','round','bevel'].includes(state.strokeJoin) ||
          !['solid','dashed','dotted'].includes(state.strokeStyle) || state.vectorBrush && state.strokeStyle !== 'solid')
        throw new Error('Native brush style is unavailable.');
      if (!Number.isFinite(state.brushAngle) || !Number.isFinite(state.brushAngleFactor) || state.brushAngleFactor < 0 || state.brushAngleFactor > 100 ||
          state.miterLimit !== undefined && (!Number.isFinite(state.miterLimit) || state.miterLimit < 0) ||
          state.dashOffset !== undefined && !Number.isFinite(state.dashOffset) || state.paintOrder !== undefined && !['fillFirst','strokeFirst'].includes(state.paintOrder))
        throw new Error('Native brush style values are invalid.');
      if (state.vectorBrush && (!Number.isFinite(state.pressureMin) || !Number.isFinite(state.pressureMax) || state.pressureMin < 0 ||
          state.pressureMax < state.pressureMin || typeof state.pressureInvert !== 'boolean' ||
          !['linear','sqrt','cbrt','pow2','pow3'].includes(state.pressureCurve) || state.pressureCurvePoints && typeof root.evalEasingPoints !== 'function'))
        throw new Error('Native brush pressure settings are unavailable.');
      if (state.stabilizer >= 4 && (!root.SMStrokeModeler || typeof root.SMStrokeModeler.create !== 'function'))
        throw new Error('Native brush stabilizer is not ready.');
      return JSON.parse(JSON.stringify(out));
    }
    function validPressure(e){if(e.pressure!==undefined&&(!Number.isFinite(e.pressure)||e.pressure<0||e.pressure>1))throw new Error('Brush pressure is invalid.');}
    function nativeBegin(w, e, intent) {
      var state=root.state;
      validPressure(e);
      if (e.altKey) throw new Error('Native brush resize is unavailable.');
      filters.reset();
      var pressure = filters.pressure(e,w);
      if ((state.stabilizer>=4?state.stabilizer-3:0)) {
        var sx=intent.bounds.width/intent.pin.canvasW,sy=intent.bounds.height/intent.pin.canvasH;
        if (sx !== sy) throw new Error('Native brush stabilizer needs a uniform presented scale.');
        intent.modeler=root.SMStrokeModeler.create((state.stabilizer>=4?state.stabilizer-3:0),sx);
        intent.modeler.down(w[0],w[1],root.performance.now()/1000,pressure);
      }
      return [[w[0],w[1],filters.width(pressure)]];
    }
    function nativeSample(samples,w,e,kind,intent) {
      validPressure(e);
      var pressure=filters.pressure(e,w);
      if (intent.modeler) {
        var outs=intent.modeler[kind==='up'?'up':'move'](w[0],w[1],root.performance.now()/1000,pressure);
        outs.forEach(function(p){samples.push([p.x,p.y,filters.width(p.p)]);});
      } else {
        if(kind==='move')w=filters.stabilize(w);
        samples.push([w[0],w[1],filters.width(pressure)]);
      }
    }
    function nativeDictionary(raw,style) {
      if(raw.length<2)return null;
      if(raw.some(function(s){return !s.every(Number.isFinite);}))throw new Error('Brush samples must stay finite.');
      var smp=filters.angle(raw),pts=smp.map(function(s){return new Point(s[0],s[1]);}),widths=smp.map(function(s){return s[2];}),path;
      var out={strokeColor:style.strokeEnabled?style.strokeColor:null,hasRealStroke:style.strokeEnabled,strokeWidth:style.brushSize,
        strokeCap:style.strokeCap,strokeJoin:style.strokeJoin,fillColor:style.fillEnabled?style.fillColor:null,opacity:style.opacity/100};
      function segments(item){return item.segments.map(function(s){return {point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};});}
      if(style.vectorBrush){
        var cs=buildCenterSegmentsFromRawStroke(pts,widths,style.smoothing);
        if(style.taperEnds)applyTaperToCenterSegments(cs,0.15);
        var sampled=sampleVectorBrushCenterline(cs,cs.widthProfile);
        path=buildVariableWidthPath(sampled.pts,sampled.widths);
        if(!path)throw new Error('Native brush outline is unavailable.');
        out.isVectorBrush=true;out.centerSegments=cs;out.widthProfile=cs.widthProfile;
        out.strokeColor=null;out.hasRealStroke=false;out.fillColor=style.strokeColor;
        // Matches a new baseline pressure Path: no actual stroke, Paper default
        // stroke width/cap/join, and applyBrushKeyline has no width to paint.
        out.strokeWidth=1;out.strokeCap='butt';out.strokeJoin='miter';out.miterLimit=10;out.dashOffset=0;
      }else{
        path=new Path({insert:false});pts.forEach(function(p){path.add(p);});simplifyIfNeeded(path,style.smoothing);
        var stroke={strokeWidth:style.brushSize,data:{}};applyStrokeStyle(stroke);
        out.miterLimit=stroke.miterLimit;out.dashOffset=stroke.dashOffset;
        if(stroke.dashArray.length)out.dashArray=stroke.dashArray.slice();
        if(stroke.data.paintOrder!==undefined)out.paintOrder=stroke.data.paintOrder;
      }
      try{out.segments=segments(path);out.closed=path.closed;
        if(out.segments.some(function(s){return !['point','handleIn','handleOut'].every(function(k){return s[k].every(Number.isFinite);});}))throw new Error('Brush geometry must stay finite.');
        return out;}finally{path.remove();}
    }
    return {freehand:true,style:nativeDrawStyle,begin:nativeBegin,sample:nativeSample,dictionary:nativeDictionary};
  }
  // Raw per-sample pressure (especially the speed-fallback formula, and
  // some stylus hardware) is noisy enough to visibly bump the variable-
  // width outline's edge — needs smoothing, but a single FIXED-factor
  // exponential moving average (the previous 0.45-alpha version) forces
  // one compromise value for every drawing speed: tuned to kill jitter
  // during a slow, careful line, it visibly LAGS behind a fast, deliberate
  // press/release (exactly the "moins naturel qu'un vrai logiciel pro
  // comme Moho" feedback — a real pen's weight should register close to
  // instantly, not smear across several samples). 2026-07: replaced with a
  // One Euro Filter (Casiez/Roussel/Vogel, https://gery.casiez.net/1euro/
  // — the industry-standard answer to exactly this "filter now, tau
  // later" tradeoff, used across stylus/motion-tracking software): cutoff
  // frequency adapts to how FAST pressure is currently changing, so a slow
  // stroke gets heavy smoothing (kills jitter) while a fast press/release
  // gets a high cutoff (near-zero lag). mincutoff/beta below are a
  // reasonable starting point for a 0..1 pressure signal (as opposed to
  // pixel-position tuning, where beta is usually two orders of magnitude
  // smaller) — real tuning needs a physical stylus to feel right, adjust
  // here if it still reads too smoothed/too jittery.

  // Tuned empirically (no physical stylus available in this environment —
  // simulated jitter [0.18-0.25] + press-to-0.9 sample sequences at
  // realistic ~10-16ms event spacing): beta needs to be ~30x larger than
  // typical 2D-position One Euro presets, since a 0..1 pressure signal's
  // derivative is numerically tiny compared to pixel-position deltas.
  // Result: reaches target within 2 samples on a real press (was 6+ samples
  // with the old fixed-0.45 filter) while holding a TIGHTER jitter band
  // than that old filter too — verify/retune live with a real tablet if it
  // ever reads too twitchy or too smoothed.
  function makeOneEuroFilter(mincutoff, beta, dcutoff) {
    var xPrev = null, dxPrev = 0, tPrev = null;
    function alpha(cutoff, dt) { var tau = 1 / (2 * Math.PI * cutoff); return 1 / (1 + tau / dt); }
    return function (x, tMs) {
      if (xPrev == null) { xPrev = x; tPrev = tMs; dxPrev = 0; return x; }
      var dt = Math.max(0.001, (tMs - tPrev) / 1000); // seconds; floored so a duplicate-timestamp sample can't divide by zero
      tPrev = tMs;
      var dx = (x - xPrev) / dt;
      dxPrev = dxPrev + alpha(dcutoff, dt) * (dx - dxPrev);
      var cutoff = mincutoff + beta * Math.abs(dxPrev);
      var result = xPrev + alpha(cutoff, dt) * (x - xPrev);
      xPrev = result;
      return result;
    };
  }
  function createPressureFilter(){return makeOneEuroFilter(1.0,20,1.0);}
  return Object.freeze({create:create,freehandPorts:freehandPorts,createPressureFilter:createPressureFilter});
}));
