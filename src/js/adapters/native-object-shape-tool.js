/* Ordinary native shape, Pen and bounded freehand gestures. Only pointer-local data and SVG preview live here;
 * the existing vector client and Project resource queue own every committed command. */
(function(root,factory){var node=typeof module==='object'&&module.exports;
  var api=factory(node?require('./native-object-draw-paint.js'):root.NemoNativeDrawPaint,node?require('./native-object-draw-symmetry.js'):root.NemoNativeDrawSymmetry,
    node?require('./native-object-draw-bitmap.js'):root.NemoNativeDrawBitmap);
  if(node)module.exports=api;else root.NemoNativeObjectShapeTool=api;
}(typeof globalThis==='object'?globalThis:this,function(Paint,Symmetry,Bitmap){
  'use strict';
  var tools=['line','rect','ellipse'],shapeTools=tools.concat(['star','speechbubble']);
  var pinKeys=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  function same(a,b){return !!a&&!!b&&pinKeys.every(function(k){return a[k]===b[k];});}
  function fail(message){throw new Error(message);}
  // A pressure ribbon drawn with Fill on carries its linked fill companion (draw-bridge.js @1065e9d8): the dictionary
  // builder records it here, against the ribbon's dictionary, and the commit creates both in one createMany.
  var linkedCompanions=new WeakMap();
  function consume(e){if(e&&e.stopImmediatePropagation)e.stopImmediatePropagation();if(e&&e.preventDefault)e.preventDefault();}
  function create(root,ports){
    var Mapping=ports.mapping;
    var admitted=ports.freehand?['draw','fillbrush']:ports.pen?['pen']:tools,handled=ports.freehand?['draw','fillbrush']:ports.pen?['pen']:shapeTools;
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
    // CSS pixels per document unit as presented: the painted document-to-composition transform composed with the
    // element box, the same affine point() inverts. A fitted view letterboxes the document, so the element's own
    // width/canvasW and height/canvasH differ while the presentation stays uniform (#2343 B1). The brush modeler takes
    // this one scale (the baseline's view.zoom); null when the presentation is not uniform beyond CSS layout rounding.
    function presentedScale(t){
      var m=Mapping.transform({presentation:t.presentation},false),kx=t.bounds.width/t.pin.canvasW,ky=t.bounds.height/t.pin.canvasH,
        a=m[0]*kx,b=m[1]*ky,c=m[2]*kx,d=m[3]*ky,det=a*d-b*c,scale=Math.sqrt(det),tolerance=scale/1024;
      return det>0&&Number.isFinite(scale)&&Math.abs(a-d)<=tolerance&&Math.abs(b+c)<=tolerance?scale:null;
    }
    function point(e,t){if(!Number.isFinite(e.clientX)||!Number.isFinite(e.clientY))fail('Pointer position is unavailable.');
      var p=Mapping.point(e,{bounds:t.bounds,canvasW:t.pin.canvasW,canvasH:t.pin.canvasH,presentation:t.presentation});return [p.x,p.y];}
    function style(tool,proof){
      var s=root.state,current=session(),ownerInfo=current&&current.info(),port=root.NemoNativePerspectiveController;
      var perspective=port&&(proof?typeof port.readForCommand==='function'&&port.readForCommand(proof):port.read());
      if(s.appMode!=='anim2d'||s.playing||s.spaceDown||s.isPanning||!perspective||!same(perspective.info,ownerInfo)||perspective.perspective.settings.perspectiveEnabled||s.activeSymbolId)fail('This drawing mode is unavailable in native shape tools.');
      if(ports.freehand)return ports.style();
      if(ports.pen)return Paint.shadowStyle(root,s,penStyle(s));
      // Shadow mode inks the shape in the swatch whatever Stroke/Fill say (applyShadowBrushTag).
      if(typeof s.strokeEnabled!=='boolean'||typeof s.fillEnabled!=='boolean'||!s.shadowMode&&!s.strokeEnabled&&(tool==='line'||!s.fillEnabled))fail('Enable a supported stroke or fill to draw.');
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
      // tools.js Pen/Shape commit @1065e9d8 runs applyShadowBrushTag on the committed path in Shadow mode.
      return Paint.shadowStyle(root,s,out);
    }
    // tools.js Pen @1065e9d8: strokeColor/brushSize/cap/join/opacity, no fill, applyStrokeStyle dashes.
    function penStyle(s){
      if(s.taperEnds)fail('Tapered Pen strokes are unavailable in native Drawing.');
      var hex=/^#(?:[a-fA-F0-9]{3}|[a-fA-F0-9]{6}|[a-fA-F0-9]{8})$/;
      if(!hex.test(s.strokeColor))fail('Native shapes require solid colours.');
      if(!Number.isFinite(s.brushSize)||s.brushSize<=0||!Number.isFinite(s.opacity)||s.opacity<0||s.opacity>100)fail('Native shape style is unavailable.');
      if(!['solid','dashed','dotted'].includes(s.strokeStyle))fail('This shape stroke style is unavailable.');
      var out={strokeColor:s.strokeColor,hasRealStroke:true,strokeWidth:s.brushSize,fillColor:null,opacity:s.opacity/100,
        strokeCap:s.strokeCap===undefined?'butt':s.strokeCap,strokeJoin:s.strokeJoin===undefined?'miter':s.strokeJoin,miterLimit:10,dashOffset:0}; // unset = Paper defaults
      Object.assign(out,ports.lineStyle());
      if(!['butt','round','square'].includes(out.strokeCap)||!['miter','round','bevel'].includes(out.strokeJoin))fail('Native stroke cap is unavailable.');
      return out;
    }
    function selection(info){
      var s=root.state,index=s.activeLayerIdx,row=s.layers&&s.layers[index],selected=root._layerSel;
      // The baseline draws on the ACTIVE layer; a canvas click on empty stage leaves _layerSel=[]. Only other/several layers are ambiguous.
      if(!Number.isSafeInteger(index)||!row||!Array.isArray(selected)||selected.length>1||selected.length===1&&selected[0]!==index)fail('Select one native Drawing layer.');
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
      var t={perspective:perspective,session:s,pin:pin,presentation:shown.presentation,frame:frame,tool:state.tool,layerUid:selection(pin),sel:JSON.stringify(root._layerSel),style:style(state.tool,perspective),bounds:bounds(pin),pointer:e.pointerId,cancelled:false};
      t.behind=root.state.drawMode==='behind';t.start=point(e,t);t.scale=presentedScale(t);if(ports.freehand)t.samples=ports.begin(t.start,e,t);return t;
    }
    function current(t,requirePresentation,lease){
      try{var s=session(),pin=s&&s.info();
        if(retired||t.cancelled||requirePresentation&&!Mapping.matchesView(root,t.presentation)||s!==t.session||!s.isActive()||!s.blocksLegacy()||!same(pin,t.pin)||root.state.currentFrame!==t.frame||!t.finished&&root.state.tool!==t.tool||selection(pin)!==t.layerUid||JSON.stringify(root._layerSel)!==t.sel||
            root.NemoNativeObjectProjectFile.projectionReadyForUI(root,pin)!==true||JSON.stringify(style(t.tool,requirePresentation?undefined:t.perspective))!==JSON.stringify(t.style)||JSON.stringify(bounds(pin))!==JSON.stringify(t.bounds))return false;
        // Public presentation is hidden by this action's own pending entry.
        // Its existing lease still verifies the exact origin, view and subscription.
        var shown=requirePresentation?s.readPresentedObjectSession():lease&&typeof lease.readPresentedObjectSession==='function'&&lease.readPresentedObjectSession();
        return !!shown&&same(shown,t.pin)&&shown.presentation===t.presentation;
      }catch(_){return false;}
    }
    function clear(){if(overlay){overlay.remove();overlay=null;}}
    function release(t){try{var el=root.document.getElementById('canvas-area')||root.document.getElementById('drawing-canvas');if(el.releasePointerCapture)el.releasePointerCapture(t.pointer);}catch(_){} }
    function cancel(){if(drag){var t=drag;drag=null;t.cancelled=true;clear();release(t);}return {status:'cancelled'};}
    function endpoint(t,e){var end=point(e,t);return !ports.freehand&&e.shiftKey?ports.constrain(t.tool,t.start,end):end;}
    function dictionary(t,end,final){
      if(ports.pen){
        var anchors=t.anchors.concat(end&&!t.handleDrag?[{point:end}]:[]);
        return Paint.shade(Object.assign({},t.style,{closed:!!t.closed,segments:anchors.map(function(a){
          return {point:a.point.slice(),handleIn:(a.handleIn||[0,0]).slice(),handleOut:(a.handleOut||[0,0]).slice()};})}),final);
      }
      if(ports.freehand)return ports.dictionary(t.samples,t.style);
      var g=ports.geometry(t.tool,t.start[0],t.start[1],end[0],end[1]);
      var out=Object.assign({},t.style,{closed:g.closed,segments:g.segments.map(function(s){return {point:s.point.slice(),handleIn:(s.handleIn||[0,0]).slice(),handleOut:(s.handleOut||[0,0]).slice()};})});
      if(t.tool==='rect')out.paramShape={kind:'rect',tl:0,tr:0,br:0,bl:0,box:{x1:Math.min(t.start[0],end[0]),y1:Math.min(t.start[1],end[1]),x2:Math.max(t.start[0],end[0]),y2:Math.max(t.start[1],end[1])}};
      return Paint.shade(out,final);
    }
    // The live preview shows the stroke and, with symmetry on, every copy the commit will create
    // (symmetry-bridge.js onPreview @1065e9d8); a refused symmetry previews only the stroke itself.
    function preview(t,end){
      var d=root.document,dict=dictionary(t,end);if(!dict)return;
      var dicts;try{dicts=Symmetry.copies(root,dict,t.pin,linkedCompanions.get(dict)||null);}catch(_){dicts=[dict];}
      if(!overlay){overlay=d.createElementNS('http://www.w3.org/2000/svg','svg');overlay.setAttribute('aria-hidden','true');
        Object.assign(overlay.style,{position:'fixed',pointerEvents:'none',zIndex:'20',left:t.bounds.left+'px',top:t.bounds.top+'px',width:t.bounds.width+'px',height:t.bounds.height+'px'});
        overlay.setAttribute('viewBox','0 0 '+t.pin.canvasW+' '+t.pin.canvasH);overlay.setAttribute('preserveAspectRatio','none');d.body.appendChild(overlay);}
      while(overlay.children.length<dicts.length)overlay.appendChild(d.createElementNS('http://www.w3.org/2000/svg','path'));
      while(overlay.children.length>dicts.length)overlay.children[overlay.children.length-1].remove();
      var matrix='matrix('+Mapping.transform({presentation:t.presentation},false).join(' ')+')';
      dicts.forEach(function(one,at){var path=overlay.children[at],segments=one.segments;if(!segments.length)return;
        path.setAttribute('transform',matrix);var start=segments[0].point,text='M '+start[0]+' '+start[1],count=segments.length+(one.closed?1:0);
        for(var i=1;i<count;i++){var a=segments[i-1],b=segments[i%segments.length];text+=' C '+(a.point[0]+a.handleOut[0])+' '+(a.point[1]+a.handleOut[1])+' '+(b.point[0]+b.handleIn[0])+' '+(b.point[1]+b.handleIn[1])+' '+b.point[0]+' '+b.point[1];}
        if(one.closed)text+=' Z';path.setAttribute('d',text);path.setAttribute('fill',one.fillColor||'none');path.setAttribute('stroke',one.strokeColor||'none');
        path.setAttribute('stroke-width',one.strokeWidth);path.setAttribute('stroke-linecap',one.strokeCap);path.setAttribute('stroke-linejoin',one.strokeJoin);path.setAttribute('opacity',one.opacity);
        if(one.dashArray)path.setAttribute('stroke-dasharray',one.dashArray.join(' '));path.setAttribute('stroke-dashoffset',one.dashOffset);path.setAttribute('stroke-miterlimit',one.miterLimit);
        path.setAttribute('paint-order',one.paintOrder==='strokeFirst'?'stroke fill':'fill stroke');});
    }
    function commit(t,end){
      if(!current(t,true))return Promise.resolve({status:'cancelled'});
      if(ports.pen&&t.anchors.length<2)return Promise.resolve({status:'cancelled'});
      if(!ports.freehand&&!ports.pen&&Math.hypot(end[0]-t.start[0],end[1]-t.start[1])<2)return Promise.resolve({status:'cancelled'});
      var dict=dictionary(t,end,true),s=t.session,companion=linkedCompanions.get(dict)||null;
      if(ports.freehand&&dict&&!dict.isFillShape&&!dict.isVectorBrush&&dict.segments&&dict.segments.length<2)return Promise.resolve({status:'cancelled'}); // tap: a one-anchor Path draws nothing
      // Labs predictive stroke (draw-bridge.js onStrokeCommitted @1065e9d8) reshapes a freehand stroke before it is committed.
      if(ports.freehand&&root.SMLabs&&typeof root.SMLabs.predictStrokeDict==='function')dict=root.SMLabs.predictStrokeDict(dict)||dict;
      // Mask mode (tools.js tagOwner @1065e9d8, on every Draw/Pen/shape commit): the shape becomes a mask of the layer
      // with the chosen combine mode; its symmetry copies and multi-frame stamps carry the same tags. tagOwner tags the
      // committed path only, so a pressure ribbon's linked fill stays an ordinary fill.
      if(root.state.maskMode)dict=Object.assign({},dict,{isMask:true,maskMode:root.state.maskModeType||'add'});
      // No second queue: only add a current-intent check to the real Project
      // resource action. While that action runs, its own pending count hides presentation.
      var guarded={info:s.info,isActive:s.isActive,requestId:s.requestId,runResourceCommand:function(action){return s.runResourceCommand(function(ctx){
        if(!current(t,false,ctx))throw Object.assign(new Error('Native shape intent changed'),{code:'unavailable',reason:'intent_changed'});
        return action({reconcile:ctx.reconcile,dispatch:function(q){if(!current(t,false,ctx))throw Object.assign(new Error('Native shape intent changed'),{code:'unavailable',reason:'intent_changed'});return ctx.dispatch(q);}});
      });}};
      // draw-bridge.js Fill Brush: one native command places the shape (fillBrushMode) and merges same-colour fills.
      // Labs multi-frame draw stamps the path the merge hands back on the other selected frames, in the same commit.
      var fbStamps=dict.isFillShape&&root.SMLabs&&typeof root.SMLabs.multiframeTargets==='function'?root.SMLabs.multiframeTargets(t.frame):[];
      if(dict.isFillShape)return root.NemoNativeFillDesktop.create({current:function(){return guarded;},ready:function(){return true;},
        target:function(){return {layerUid:t.layerUid,frame:t.frame,contextId:'scene-root'};}}).brush({outline:dict.segments,color:dict.fillColor,opacity:dict.opacity,placement:root.state.fillBrushMode,mask:root.state.maskMode?root.state.maskModeType||'add':false,copies:Symmetry.matrices(root,t.pin),stamps:fbStamps})
        .then(function(r){if(r&&r.applied&&fbStamps.length&&typeof root.SMLabs.multiframeStamped==='function')root.SMLabs.multiframeStamped(fbStamps.length);
          return {status:r&&r.applied?'applied':'noop'};},root.NemoNativeObjectVector.statusOf);
      // draw-bridge.js Bitmap Brush: the anchor, its baked companion and the PNG in one native command.
      if(t.style&&t.style.bitmap)return Bitmap.commit(root,guarded,t,dict,root.NemoNativeObjectVector.statusOf);
      // tools.js @1065e9d8 Draw commit: drawMode 'behind' slips the stroke under the layer's artwork.
      // G23 (symmetry-bridge.js onStrokeCommitted @1065e9d8): the stroke and its copies, each linked fill below its ribbon,
      // in one createMany (behind keeps the order at the bottom).
      var vector=root.NemoNativeObjectVector.create(guarded),dicts=Symmetry.copies(root,dict,t.pin,companion);
      var group=companion?[companion,dict]:[dict];
      var options={origin:t.pin,placement:ports.freehand&&t.behind?'behind':'front'};
      // draw-bridge.js @1065e9d8 Trim Stroke Ends: a Draw commit trims before its symmetry copies exist; native shares the cuts.
      if(ports.freehand&&root.state.trimStrokeEnds&&root.state.trimStrokeEndsMax>0)options.trimEnds=root.state.trimStrokeEndsMax;
      // Labs multi-frame draw (draw-bridge.js onStrokeCommitted @1065e9d8, before the symmetry copies): the stroke is also
      // stamped on every other selected frame, held ones keyed with what they showed, in the same single undo step.
      var stamps=ports.freehand&&root.SMLabs&&typeof root.SMLabs.multiframeTargets==='function'?root.SMLabs.multiframeTargets(t.frame):[];
      if(stamps.length){
        // Behind, the k-th record of one create lies k-th from the bottom; one create per step lands each at the bottom.
        var here=dicts.map(function(d){return {create:d,placement:options.placement};});if(options.placement==='behind')here.reverse();
        var steps=here.concat([].concat.apply([],stamps.map(function(frame){
          var cell=group.map(function(d){var i=here.findIndex(function(s){return s.create===d;});return Object.assign(i<0?{create:d}:{copy:i},{frame:frame,keyframe:'copy',placement:options.placement});});// a stamp copies its drawn step: sent once (64 KiB)
          return options.placement==='behind'?cell.reverse():cell;})));
        return vector.batchStrokes(t.layerUid,t.frame,steps,Object.assign({origin:t.pin},options.trimEnds?{trimEnds:options.trimEnds}:{})).then(function(result){
          if(result&&result.status==='applied'&&typeof root.SMLabs.multiframeStamped==='function')root.SMLabs.multiframeStamped(stamps.length);return result;});
      }
      return dicts.length>1?vector.createStrokes(t.layerUid,t.frame,dicts,options):vector.createStroke(t.layerUid,t.frame,dict,options);
    }
    // Geometry/mapping can refuse synchronously before the existing Project
    // queue starts. Always return a promise so release finalization clears
    // pending, without classifying a pre-dispatch refusal as owner uncertainty.
    function commitAttempt(t,end){
      try{return Promise.resolve(commit(t,end));}
      catch(error){t.refusal=error&&error.message;return Promise.resolve({status:'unavailable',reason:'pre_dispatch_failed'});}
    }
    function submit(t,end){
      pending=t;settled=commitAttempt(t,end).catch(function(){return {status:'unconfirmed'};}).then(function(result){
        if(result.status==='indeterminate'||result.status==='unconfirmed')uncertain=t;
        if(!retired&&result.status!=='applied'&&result.status!=='noop'&&result.status!=='cancelled')notice(t.refusal||'Native shape drawing was not confirmed.');return result;
      }).finally(function(){if(pending===t)pending=null;});
    }
    function grab(e){var el=root.document.getElementById('canvas-area')||root.document.getElementById('drawing-canvas');if(el.setPointerCapture)el.setPointerCapture(e.pointerId);}
    // Pen: one gesture spans several presses. A press adds an anchor (Shift: 45 degrees from the
    // previous one), a drag sets its symmetric handles (Alt keeps handleIn), a press near the
    // first anchor closes and commits, a double press commits the open path, Escape cancels.
    // A finished Pen intent is fixed: a later tool switch (the one that may have finished it) cannot void it.
    function finishPen(t,closed){drag=null;clear();release(t);t.closed=closed;t.finished=true;submit(t,null);}
    function penHandle(kind,e){
      if(kind==='down'&&!stageOrigin(e))return !!drag||nativeMode()&&root.state.tool==='pen';
      // However the tool changed, the open path commits (finalizePen), never silently drops.
      if(drag&&root.state.tool!=='pen'){var open=drag;open.finished=true;if(current(open,true))finishPen(open,false);else cancel();return false;}
      if(!nativeMode()||root.state.tool!=='pen')return false;
      if(!drag&&kind!=='down')return true; // hover belongs to the stage; consume nothing
      if(!stageOrigin(e)&&!drag)return true;
      if(drag&&kind!=='down'&&e.pointerId!==drag.pointer)return true;
      consume(e);if(pending)return true;
      if(kind==='cancel'){cancel();return true;}
      try{
        if(kind==='down'){
          if(e.button!==0||e.isPrimary===false||!Number.isSafeInteger(e.pointerId))return true;
          var now=Date.now(),client=[e.clientX,e.clientY];
          if(!drag){
            if(uncertain){var owner=session(),pin=owner&&owner.info();
              if(owner===uncertain.session&&pin&&['instanceId','documentId','lifecycleGeneration'].every(function(k){return pin[k]===uncertain.pin[k];})){notice('Reopen the native project before drawing again; the previous result is unconfirmed.');return true;}
              uncertain=null;}
            drag=capture(e);drag.pointerType=e.pointerType||'mouse';drag.anchors=[{point:drag.start,client:client}];drag.handleDrag=true;drag.last={time:now,client:client};
            grab(e);preview(drag,drag.start);return true;
          }
          var t=drag;if(!current(t,true)){cancel();return true;}
          if((e.pointerType||'mouse')!==t.pointerType)return true;
          release(t);t.pointer=e.pointerId;var last=t.last;t.last={time:now,client:client};
          // pen-bridge.js @1065e9d8: Alt+press on a placed anchor reshapes its outgoing handle.
          if(e.altKey){var hit=-1;t.anchors.forEach(function(a,i){if(Math.hypot(client[0]-a.client[0],client[1]-a.client[1])<10)hit=i;});
            if(hit>=0){t.reshape=hit;t.handleDrag=true;grab(e);return true;}}
          if(now-last.time<350&&Math.hypot(client[0]-last.client[0],client[1]-last.client[1])<10){finishPen(t,false);return true;}
          var first=t.anchors[0];
          if(t.anchors.length>1&&Math.hypot(client[0]-first.client[0],client[1]-first.client[1])<10){finishPen(t,true);return true;}
          var p=point(e,t);if(e.shiftKey)p=ports.constrain('line',t.anchors[t.anchors.length-1].point,p);
          t.anchors.push({point:p,client:client});t.handleDrag=true;grab(e);preview(t,p);return true;
        }
        if(!drag||e.pointerId!==drag.pointer)return true;
        if(!current(drag,true)){cancel();return true;}
        var q=point(e,drag);
        if(kind==='move'){
          if(drag.handleDrag){var a=drag.anchors[drag.reshape===undefined?drag.anchors.length-1:drag.reshape],d=[q[0]-a.point[0],q[1]-a.point[1]];
            a.handleOut=d;if(!e.altKey&&drag.reshape===undefined)a.handleIn=[-d[0],-d[1]];}
          preview(drag,q);return true;
        }
        if(kind==='up'){drag.handleDrag=false;drag.reshape=undefined;release(drag);preview(drag,q);return true;}
      }catch(error){cancel();notice(error.message||'Native Pen is unavailable.');return true;}
      return true;
    }
    function handle(kind,e){
      if(retired)return false;
      if(ports.pen)return penHandle(kind,e);
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
          if(kind==='up'){drag=null;clear();submit(t,end);release(t);return true;}
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
    function key(e){
      if(e.key==='Escape'&&drag){consume(e);cancel();}
      else if(ports.pen&&e.key==='Enter'&&drag&&!pending){consume(e);finish();}
      // timeline.js @1065e9d8: Backspace/Delete drops the last anchor; the last one ends the gesture.
      else if(ports.pen&&(e.key==='Backspace'||e.key==='Delete')&&drag&&!pending){consume(e);
        if(drag.anchors.length<=1)cancel();else{drag.anchors.pop();drag.handleDrag=false;preview(drag,drag.anchors[drag.anchors.length-1].point);}}
    }
    function finish(){if(ports.pen&&drag&&!pending){if(current(drag,true))finishPen(drag,false);else cancel();}}
    root.document.addEventListener('keydown',key,{capture:true});
    // Switching away from Pen commits the open path (timeline.js setTool @1065e9d8: finalizePen()).
    var setTool=ports.pen&&root.SM&&typeof root.SM.setTool==='function'?root.SM.setTool:null,boundSetTool=null;
    if(setTool){boundSetTool=function(next){if(next!=='pen')finish();return setTool.apply(this,arguments);};root.SM.setTool=boundSetTool;}
    return Object.freeze({handle:handle,cancel:cancel,finish:finish,whenSettled:function(){return settled;},isDragging:function(){return !!drag;},
      dispose:function(){retired=true;cancel();if(pending)pending.cancelled=true;root.document.removeEventListener('keydown',key,{capture:true});
        if(boundSetTool&&root.SM&&root.SM.setTool===boundSetTool)root.SM.setTool=setTool;}});
  }
  // Composition of existing brush filters and detached geometry; no Paper insertion, writer or second queue.
  function freehandPorts(root,filters) {
    var Point=root.Point,Path=root.Path;
    var buildCenterSegmentsFromRawStroke=root.buildCenterSegmentsFromRawStroke,applyTaperToCenterSegments=root.applyTaperToCenterSegments;
    var sampleVectorBrushCenterline=root.sampleVectorBrushCenterline,buildVariableWidthPath=root.buildVariableWidthPath;
    var simplifyIfNeeded=root.simplifyIfNeeded,applyStrokeStyle=root.applyStrokeStyle;
    function nativeDrawStyle() {
      var state=root.state,fb=state.tool==='fillbrush';
      // Fill Brush (draw-bridge's isFillBrush branch) draws no texture, bitmap or Trim and ignores drawMode. Labs runs its
      // hooks on the placed path: predictive skips a fill shape, multi-frame stamps the merged result (native, at commit).
      if (fb && (['above','below','merge'].indexOf(state.fillBrushMode) < 0 || root.SMLabs && (typeof root.SMLabs.unmigratedDrawHooks === 'function'
          ? root.SMLabs.unmigratedDrawHooks().length > 0 : root.SMLabs.list().some(function(p){return p.on;})))) throw new Error('This Fill Brush combination is unavailable in native Drawing.');
      // A key a confirmed, settled native catalog lacks draws plain, as the baseline's resolveBrushPreset null does (applyBrushTexture
      // returns the path untouched). An unknown or changing catalog cannot tell a deleted key from a new one: unavailable.
      var CB=root.NemoNativeCustomBrushUI,cbc=CB&&CB.controller(root),key=state.brushPreset,absent=!fb&&!state.shadowMode&&!!key&&key!=='none'&&
        !(root.BRUSH_PRESETS&&Object.prototype.hasOwnProperty.call(root.BRUSH_PRESETS,key))&&!!cbc&&!cbc.isPending()&&CB.exists(root,key)===false;
      // Trim measures the whole stroke before the stop-at-axis clip (draw-bridge then symmetry-bridge); native would trim the clipped one.
      // A pressure ribbon trims natively alone, with its linked fill, its symmetry copies and its multi-frame stamps. Only a
      // mirror clips at its axis (symmetry-bridge.js clipPathAtAxis); a radial copy never does.
      if (!fb && (state.tool !== 'draw' || state.trimStrokeEnds && state.symmetryEnabled && state.symmetryMode !== 'radial' && !state.symmetryExtend ||
          state.drawMode !== 'front' && state.drawMode !== 'behind' ||
          // G16 option C (#2361): one seeded anchor whose dabs native stamps; the key resolves as resolveBrushPreset (a built-in,
          // else a confirmed native catalog record). A key neither holds, and texture with symmetry or Trim, stay unavailable.
          !state.shadowMode && state.brushPreset && state.brushPreset !== 'none' && !absent && (!(root.BRUSH_PRESETS && Object.prototype.hasOwnProperty.call(root.BRUSH_PRESETS, state.brushPreset) ||
            root.NemoNativeCustomBrushUI && root.NemoNativeCustomBrushUI.record(root, state.brushPreset)) ||
            state.strokeEnabled && (state.symmetryEnabled || state.trimStrokeEnds)) ||
          root.SMLabs && (typeof root.SMLabs.unmigratedDrawHooks === 'function'
            ? root.SMLabs.unmigratedDrawHooks().length > 0 : root.SMLabs.list().some(function(p){return p.on;}))))
        throw new Error('This brush combination is unavailable in native Drawing.');
      if (typeof state.vectorBrush !== 'boolean' || typeof state.strokeEnabled !== 'boolean' || typeof state.fillEnabled !== 'boolean' ||
          !fb && !state.strokeEnabled && !state.fillEnabled)
        throw new Error('Enable a stroke or a fill to draw.');
      var keys = ['vectorBrush','strokeEnabled','fillEnabled','strokeColor','fillColor','brushSize','opacity','strokeCap','strokeJoin','brushPreset',
        'strokeStyle','miterLimit','dashOffset','paintOrder','smoothing','stabilizer','taperEnds','pressureMin','pressureMax',
        'pressureInvert','pressureCurve','pressureCurvePoints','brushAngle','brushAngleFactor','tool','fillBrushMode'];
      var out = {}; keys.forEach(function(k){if(state[k] !== undefined)out[k]=state[k];});if(absent)out.brushPreset='none';
      var hex = /^#(?:[a-fA-F0-9]{3}|[a-fA-F0-9]{6}|[a-fA-F0-9]{8})$/;Paint.shadowStyle(root, state, out);
      if (state.strokeEnabled && !hex.test(state.strokeColor) || (state.fillEnabled || fb) && !hex.test(state.fillColor) ||
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
      return JSON.parse(JSON.stringify(Bitmap.style(root, state, out)));
    }
    function validPressure(e){if(e.pressure!==undefined&&(!Number.isFinite(e.pressure)||e.pressure<0||e.pressure>1))throw new Error('Brush pressure is invalid.');}
    function nativeBegin(w, e, intent) {
      var state=root.state;
      validPressure(e);
      if (e.altKey) throw new Error('Native brush resize is unavailable.');
      filters.reset();
      var pressure = filters.pressure(e,w);
      if ((state.stabilizer>=4?state.stabilizer-3:0)) {
        if (!(intent.scale>0)) throw new Error('Native brush stabilizer needs a uniform presented scale.');
        intent.modeler=root.SMStrokeModeler.create((state.stabilizer>=4?state.stabilizer-3:0),intent.scale);
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
    // A native vector record holds 2..256 anchors (native-object-vector-stroke.js); the baseline Path has no cap, and a
    // pressure ribbon across 40% of an HD frame already samples past it (#2343 B1). A longer freehand outline is refitted:
    // Paper's simplify within a quarter document unit, widened only until it fits. A stroke that fits keeps every anchor.
    function fitRecord(path){
      if(path.segments.length<=256)return;
      var source=path.segments.map(function(s){return s.clone();});
      for(var tolerance=.25;tolerance<=64;tolerance*=2){
        path.removeSegments();path.addSegments(source.map(function(s){return s.clone();}));path.simplify(tolerance);
        if(path.segments.length<=256)return;
      }
      throw new Error('This stroke is too long for a native vector record.');
    }
    function nativeDictionary(raw,style) {
      if(raw.length<2)return null;
      if(raw.some(function(s){return !s.every(Number.isFinite);}))throw new Error('Brush samples must stay finite.');
      var smp=filters.angle(raw),pts=smp.map(function(s){return new Point(s[0],s[1]);}),widths=smp.map(function(s){return s[2];}),path;
      var out={strokeColor:style.strokeEnabled?style.strokeColor:null,hasRealStroke:style.strokeEnabled,strokeWidth:style.brushSize,
        strokeCap:style.strokeCap,strokeJoin:style.strokeJoin,fillColor:style.fillEnabled?style.fillColor:null,opacity:style.opacity/100};
      function segments(item){return item.segments.map(function(s){return {point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};});}
      if(style.tool==='fillbrush')return Paint.fillBrush(root,pts,widths,style,segments);
      // draw-bridge.js @1065e9d8: a pressure stroke with Stroke off is one closed fill of its smoothed centreline, no ribbon
      // and no taper, at the Paper defaults of a new Path.
      if(style.vectorBrush&&!style.strokeEnabled){
        var fo=buildCenterSegmentsFromRawStroke(pts,widths,style.smoothing);
        path=new Path({insert:false});
        fo.forEach(function(s){path.add(new root.Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
        path.closed=true;
        out.strokeWidth=1;out.strokeCap='butt';out.strokeJoin='miter';out.miterLimit=10;out.dashOffset=0;
      }else if(style.vectorBrush){
        var cs=buildCenterSegmentsFromRawStroke(pts,widths,style.smoothing);
        if(style.taperEnds)applyTaperToCenterSegments(cs,0.15);
        var sampled=sampleVectorBrushCenterline(cs,cs.widthProfile);
        path=buildVariableWidthPath(sampled.pts,sampled.widths);
        if(!path)throw new Error('Native brush outline is unavailable.');
        out.isVectorBrush=true;out.centerSegments=cs;out.widthProfile=cs.widthProfile;
        out.strokeColor=null;out.hasRealStroke=false;out.fillColor=style.strokeColor;
        // Matches a new baseline pressure Path: no actual stroke, Paper default stroke width/cap/join, no width to paint.
        out.strokeWidth=1;out.strokeCap='butt';out.strokeJoin='miter';out.miterLimit=10;out.dashOffset=0;
        // Shadow mode draws neither texture nor linked fill (draw-bridge.js ribbon branch @1065e9d8).
        if(!style.shadowSwatch&&style.brushPreset&&style.brushPreset!=='none')Paint.textureRibbon(out,style.brushPreset);
        if(style.fillEnabled&&!style.shadowSwatch){
          // The companion is the ribbon's sampled centreline, closed and filled (rebuildVectorBrushOutline), below the
          // ribbon, under a fresh link id of its own (ensureStrokeId shape), with no stroke of its own.
          var link='s'+Date.now().toString(36)+'_'+Math.floor(Math.random()*1e9)+'_lf';
          out.linkedFillId=link;
          var fill=new Path({insert:false});
          sampled.centerSegments.forEach(function(s){fill.add(new root.Segment(new Point(s.point.x,s.point.y),new Point(s.handleIn.x,s.handleIn.y),new Point(s.handleOut.x,s.handleOut.y)));});
          fill.closed=true;
          try{fitRecord(fill);var fillSegments=segments(fill);}finally{fill.remove();}
          if(fillSegments.length<2)delete out.linkedFillId;else // a tap: one anchor, invisible in the baseline, below the 2..256 a record needs
          linkedCompanions.set(out,{segments:fillSegments,
            closed:true,strokeColor:null,hasRealStroke:false,strokeWidth:1,strokeCap:'butt',strokeJoin:'miter',miterLimit:10,dashOffset:0,
            fillColor:style.fillColor,opacity:style.opacity/100,isLinkedFillCompanion:true,linkedFillId:link});
        }
      }else{
        path=new Path({insert:false});pts.forEach(function(p){path.add(p);});simplifyIfNeeded(path,style.smoothing);
        var stroke={strokeWidth:style.brushSize,data:{}};applyStrokeStyle(stroke);
        out.miterLimit=stroke.miterLimit;out.dashOffset=stroke.dashOffset;
        if(stroke.dashArray.length)out.dashArray=stroke.dashArray.slice();
        if(stroke.data.paintOrder!==undefined)out.paintOrder=stroke.data.paintOrder;
        // applyBrushTexture on a plain stroke drawn with Stroke on: the path becomes its texture's invisible anchor.
        if(!style.shadowSwatch&&!style.bitmap&&style.brushPreset&&style.brushPreset!=='none'&&style.strokeEnabled)Paint.texturePlain(out,style.brushPreset);
      }
      try{fitRecord(path);out.segments=segments(path);out.closed=path.closed;
        if(style.shadowSwatch)Paint.shadow(out,style.shadowSwatch,style.vectorBrush&&!style.strokeEnabled,style.brushSize);
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
