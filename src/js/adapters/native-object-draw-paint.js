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
  // The active native swatch a Shadow-mode commit inks with; without one the gesture refuses.
  function shadowStyle(root,s,out){
    if(!s.shadowMode)return out;
    var sw=root.SMShadowBrush&&root.SMShadowBrush.activeSwatch();
    if(!(sw&&typeof sw.id==='string'&&sw.id&&/^#(?:[a-fA-F0-9]{3}|[a-fA-F0-9]{6}|[a-fA-F0-9]{8})$/.test(sw.color)))throw new Error('The Shadow Brush swatch is unavailable.');
    out.shadowSwatch={id:sw.id,color:sw.color};return out;
  }
  // A native vector record holds 2..256 anchors (native-object-vector-stroke.js); the baseline Path has no cap, and a
  // pressure ribbon across 40% of an HD frame already samples past it (#2343 B1). A longer outline is refitted:
  // Paper's simplify within a quarter document unit, widened only until it fits. A path that fits keeps every anchor.
  function fitRecord(path){
    if(path.segments.length<=256)return;
    var source=path.segments.map(function(s){return s.clone();});
    for(var tolerance=.25;tolerance<=64;tolerance*=2){
      path.removeSegments();path.addSegments(source.map(function(s){return s.clone();}));path.simplify(tolerance);
      if(path.segments.length<=256)return;
    }
    throw new Error('This stroke is too long for a native vector record.');
  }
  // tools.js finalizePen with Taper ends @1065e9d8: the placed Pen path becomes a filled ribbon, its width tapering to the
  // floor over 18% of the path length at each end (buildCenterSegmentsFromPath + taperWidthAtFrac), outlined by
  // rebuildVectorBrushOutline from the sparse anchor widths (no pressure profile) and filled in the stroke colour at the
  // brush opacity, with a new Path's stroke defaults and no dashes. A closed Pen path keeps only its anchors, as there.
  function penTaper(root,dict){
    var Segment=root.Segment,Point=root.Point,path=new root.Path({insert:false}),outline=null;
    try{
      dict.segments.forEach(function(s){path.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));});
      path.closed=!!dict.closed;
      var cs=root.buildCenterSegmentsFromPath(path,function(frac){return root.taperWidthAtFrac(frac,dict.strokeWidth,0.18);});
      var sampled=root.sampleVectorBrushCenterline(cs);
      outline=root.buildVariableWidthPath(sampled.pts,sampled.widths);
      if(!outline)throw new Error('Native brush outline is unavailable.');
      fitRecord(outline);
      return {segments:outline.segments.map(function(s){return {point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]};}),
        closed:true,strokeColor:null,hasRealStroke:false,strokeWidth:1,strokeCap:'butt',strokeJoin:'miter',miterLimit:10,dashOffset:0,
        fillColor:dict.strokeColor,opacity:dict.opacity,isVectorBrush:true,centerSegments:cs};
    }finally{path.remove();if(outline)outline.remove();}
  }
  // Pen and Shape tag the committed path only (tools.js tags at pointer-up); the live preview keeps the drawing colours.
  // Taper ends (Pen only) also takes effect at commit; Shadow mode then turns the ribbon back into its centreline.
  function shade(out,final,root){
    var sw=out.shadowSwatch,taper=out.taperEnds;delete out.shadowSwatch;delete out.taperEnds;
    if(final&&taper)out=penTaper(root,out);
    return sw&&final?shadow(out,sw,false):out;
  }
  // draw-bridge.js Fill Brush branch: the samples' centreline and width profile (buildCenterSegmentsFromRawStroke),
  // outlined as one closed ribbon (rebuildVectorBrushOutline; a Fill Brush shape has no keyline), filled in the Fill
  // colour at the brush opacity and tagged isFillShape. In Shadow mode applyShadowBrushTag turns it back into its
  // centreline: construction ink like any other Shadow stroke (the flag has no record slot on that route and only ever
  // matters to an unstroked fill).
  function fillBrush(root,pts,widths,style,segments){
    var cs=root.buildCenterSegmentsFromRawStroke(pts,widths,style.smoothing);
    var sampled=root.sampleVectorBrushCenterline(cs,cs.widthProfile),path=root.buildVariableWidthPath(sampled.pts,sampled.widths);
    if(!path)throw new Error('Native brush outline is unavailable.');
    var out;
    try{out={segments:segments(path),closed:true,strokeColor:null,hasRealStroke:false,strokeWidth:1,strokeCap:'butt',strokeJoin:'miter',
      miterLimit:10,dashOffset:0,fillColor:style.fillColor,opacity:style.opacity/100,isFillShape:true};}finally{path.remove();}
    if(!style.shadowSwatch)return out;
    out.isVectorBrush=true;out.centerSegments=cs;delete out.isFillShape;
    return shadow(out,style.shadowSwatch,false,style.brushSize);
  }
  return Object.freeze({textureRibbon:textureRibbon,texturePlain:texturePlain,shadow:shadow,shadowStyle:shadowStyle,shade:shade,fitRecord:fitRecord,fillBrush:fillBrush});
}));
