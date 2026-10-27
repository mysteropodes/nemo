/* Paint finishing of a native freehand stroke dictionary, as the baseline finishes the committed Path
 * (draw-bridge.js commitStroke and tools.js @1065e9d8): the brush-texture anchor camouflage and the Shadow Brush tag. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeDrawPaint=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  // The anchor's group and seed: draw-bridge's strokeSeed is a fresh uint32 per stroke, from which native stamps the dabs.
  function seed(out,preset){
    out.brushTexturePreset=preset;
    out.brushGroupId='bg'+Date.now().toString(36)+'_'+Math.floor(Math.random()*1e6);
    out.strokeSeed=(Math.random()*0xFFFFFFFF)>>>0;
  }
  // applyBrushTexture on a pressure ribbon (draw-bridge.js ribbon branch): the ribbon becomes the invisible anchor
  // (preTextureOpacity, opacity 0); its linked fill stays visible.
  function textureRibbon(out,preset){seed(out,preset);out.preTextureOpacity=out.opacity;out.opacity=0;}
  // applyBrushTexture on a plain stroke drawn with Stroke on: a filled anchor hides only its stroke (preTextureStroke,
  // colorHex8); an unfilled one hides itself (preTextureOpacity, opacity 0).
  function texturePlain(out,preset){
    seed(out,preset);
    if(out.fillColor){
      var h=out.strokeColor.slice(1),full=h.length===3?h.replace(/./g,function(c){return c+c;}):h;
      out.preTextureStroke='#'+(full.length===6?full+'ff':full).toLowerCase();out.strokeColor=null;
    }else{out.preTextureOpacity=out.opacity;out.opacity=0;}
  }
  // applyShadowBrushTag(p, preferFill): construction ink for the fill solver. A ribbon goes back to its open centreline;
  // the stroke takes the active swatch at max(1, width) with round ends, or, for the Stroke-off pressure fill
  // (preferFill), the fill takes it. The record keeps its channel and the swatch that drew it.
  function shadow(out,swatch,preferFill,brushSize){
    if(out.isVectorBrush&&out.centerSegments&&out.centerSegments.length){
      out.segments=out.centerSegments.map(function(s){return {point:[s.point[0],s.point[1]],
        handleIn:s.handleIn?[s.handleIn[0],s.handleIn[1]]:[0,0],handleOut:s.handleOut?[s.handleOut[0],s.handleOut[1]]:[0,0]};});
      out.closed=false;delete out.isVectorBrush;delete out.centerSegments;delete out.widthProfile;
    }
    if(preferFill){out.strokeColor=null;out.hasRealStroke=false;out.fillColor=swatch.color;}
    else{
      out.fillColor=null;out.strokeColor=swatch.color;out.hasRealStroke=true;
      out.strokeWidth=Math.max(1,out.strokeWidth||brushSize||3);out.strokeCap='round';out.strokeJoin='round';
    }
    out.channelTag='shadow';out.shadowSwatchId=swatch.id;
    return out;
  }
  return Object.freeze({textureRibbon:textureRibbon,texturePlain:texturePlain,shadow:shadow});
}));
