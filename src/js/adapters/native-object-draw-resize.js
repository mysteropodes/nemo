/* Alt+drag brush resize on native Draw / Fill Brush (draw-bridge.js onDown/onMove/onUp `sizing` @1065e9d8, feedback #24).
 * Pure UI state: a horizontal drag scales state.brushSize with a live circle at the press point, and no stroke is committed,
 * no command is sent and no revision changes. The size math, the p-sw field write and the toast are the baseline's. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeDrawResize=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var SVG='http://www.w3.org/2000/svg';
  var pinKeys=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  function same(a,b){return !!a&&!!b&&pinKeys.every(function(k){return a[k]===b[k];});}
  // 0.5px of size per px of drag, clamped to the Size field's 1..80 span (a 1:1 map would burn the range in a wrist flick).
  function sized(startValue,startX,x){return Math.max(1,Math.min(80,startValue+(x-startX)*0.5));}
  // ctx: Mapping (canvas mapping), session(), bounds(pin), scale({pin,presentation,bounds}), stageOrigin(e), notice(msg), consume(e).
  function create(root,ctx){
    var g=null;
    function element(){return root.document.getElementById('canvas-area')||root.document.getElementById('drawing-canvas');}
    function ready(){
      var s=ctx.session(),pin=s&&s.info(),shown=s&&s.readPresentedObjectSession(),state=root.state;
      if(!s||!s.isActive()||!pin||!shown||!same(pin,shown)||!shown.presentation||shown.presentation.owner!=='native'||shown.presentation.status!=='presented'||
          !ctx.Mapping||typeof ctx.Mapping.matchesView!=='function'||!ctx.Mapping.matchesView(root,shown.presentation)||state.appMode!=='anim2d'||state.playing)
        throw new Error('Native brush resize is unavailable.');
      var bounds=ctx.bounds(pin);
      return {pin:pin,presentation:shown.presentation,bounds:bounds,scale:ctx.scale({pin:pin,presentation:shown.presentation,bounds:bounds})||bounds.width/pin.canvasW};
    }
    function ring(){
      if(!g)return;var c=g.circle,r=g.size/2*g.scale;
      c.setAttribute('r',r);
    }
    function field(value){var f=root.document.getElementById('p-sw');if(f)f.value=Math.round(value);}
    function clear(){if(g&&g.overlay)g.overlay.remove();}
    function end(){clear();try{var el=element();if(g&&el.releasePointerCapture)el.releasePointerCapture(g.pointer);}catch(_){}g=null;}
    function begin(e){
      ctx.consume(e);
      if(e.button!==0||e.isPrimary===false||!Number.isSafeInteger(e.pointerId)||g)return true;
      try{
        var v=ready(),d=root.document,svg=d.createElementNS(SVG,'svg'),circle=d.createElementNS(SVG,'circle');
        Object.assign(svg.style,{position:'fixed',pointerEvents:'none',zIndex:'20',left:v.bounds.left+'px',top:v.bounds.top+'px',width:v.bounds.width+'px',height:v.bounds.height+'px'});
        svg.setAttribute('aria-hidden','true');svg.setAttribute('viewBox','0 0 '+v.bounds.width+' '+v.bounds.height);
        circle.setAttribute('cx',e.clientX-v.bounds.left);circle.setAttribute('cy',e.clientY-v.bounds.top);
        circle.setAttribute('fill','rgba(255,255,255,0.157)');circle.setAttribute('stroke','rgba(74,158,255,0.863)');circle.setAttribute('stroke-width','1');
        svg.appendChild(circle);d.body.appendChild(svg);
        g={pointer:e.pointerId,startX:e.clientX,startSize:root.state.brushSize,size:root.state.brushSize,scale:v.scale,overlay:svg,circle:circle};
        var el=element();if(el.setPointerCapture)el.setPointerCapture(e.pointerId);ring();
      }catch(error){end();ctx.notice(error.message||'Native brush resize is unavailable.');}
      return true;
    }
    function handle(kind,e){
      ctx.consume(e);
      if(!g||e.pointerId!==g.pointer)return true;
      if(kind==='cancel'){end();return true;}
      // As the baseline, only a move resizes; the release ends the gesture at the size the last move set.
      if(kind==='move'&&Number.isFinite(e.clientX)){g.size=sized(g.startSize,g.startX,e.clientX);root.state.brushSize=g.size;field(g.size);ring();}
      if(kind==='up'){var final=g.size;end();ctx.notice('Taille du pinceau : '+Math.round(final)+'px');}
      return true;
    }
    return Object.freeze({begin:begin,handle:handle,active:function(){return !!g;},cancel:end});
  }
  return Object.freeze({create:create,sized:sized});
}));
