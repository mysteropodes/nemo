/* G23 symmetry copies of a native Draw, Pen or Shape commit (symmetry-bridge.js onStrokeCommitted @1065e9d8): the stroke
 * dictionary and its mirrored or rotated copies, in the stack order the baseline's insertAbove leaves them. Geometry only;
 * the shape tool commits the list through the existing native vector client. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeDrawSymmetry=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function fail(message){throw new Error(message);}
  function pair(p){return [p.x,p.y];}
  // adoptFreshIdentity: a pressure ribbon's centreline follows the copy (points and handles mapped, widths kept), and a ribbon
  // with a linked fill gets a fresh fill of its own below it, its outline rebuilt from the mapped centreline
  // (rebuildVectorBrushOutline); without one the copy keeps the reflected or rotated outline.
  // A texture anchor's copy is an anchor of its own (adoptFreshIdentity drops the membership tag): a fresh brush group, and the
  // stamping seed of copy k derived from the drawn stroke's, so native stamps its own dabs (seededRng, never Math.random) and the
  // result is the same on every build of the same stroke.
  function retexture(copy,k){
    if(!copy.brushTexturePreset)return copy;
    if(copy.brushGroupId)copy.brushGroupId+='_c'+k;
    copy.strokeSeed=(Math.imul(copy.strokeSeed^(k*0x9E3779B1),0x85EBCA6B)^(copy.strokeSeed>>>13)^k)>>>0;
    return copy;
  }
  function adopt(root,copy,companion,fp,fv,k){
    retexture(copy,k);
    if(copy.centerSegments)copy.centerSegments=copy.centerSegments.map(function(c){
      var out={point:fp(c.point[0],c.point[1]),width:c.width};
      if(c.handleIn)out.handleIn=fv(c.handleIn[0],c.handleIn[1]);
      if(c.handleOut)out.handleOut=fv(c.handleOut[0],c.handleOut[1]);
      return out;});
    if(!companion)return [copy];
    var cs=copy.centerSegments.map(function(c){return {point:c.point,handleIn:c.handleIn||[0,0],handleOut:c.handleOut||[0,0],width:c.width};});
    var sampled=root.sampleVectorBrushCenterline(cs,copy.widthProfile),path=root.buildVariableWidthPath(sampled.pts,sampled.widths);
    if(!path)fail('Native brush outline is unavailable.');
    try{copy.segments=path.segments.map(function(s){return {point:pair(s.point),handleIn:pair(s.handleIn),handleOut:pair(s.handleOut)};});copy.closed=true;}
    finally{path.remove();}
    var link='s'+Date.now().toString(36)+'_'+Math.floor(Math.random()*1e9)+'_lf';
    copy.linkedFillId=link;
    return [Object.assign({},companion,{linkedFillId:link,segments:sampled.centerSegments.map(function(s){
      return {point:pair(s.point),handleIn:pair(s.handleIn),handleOut:pair(s.handleOut)};})}),copy];
  }
  // The stroke and its copies, bottom to top. Mirrors clip the stroke at the axis unless Extend is on, reflect points and
  // handles and re-orient closed copies; radial adds n-1 rotations about the centre (2..24 sectors), stacked as insertAbove
  // leaves them (the stroke, then copy n-1 down to copy 1). `companion` is the stroke's linked fill, which lies below it.
  // `defer` (Trim Stroke Ends on a mirror that stops at the axis): the baseline trims the whole stroke first and clips it after
  // (draw-bridge.js, then symmetry-bridge.js), so the stroke and its copy leave whole and `out.clip` names the pair and the axis
  // for native to clip after its trim (`trimClip`).
  function copies(root,dict,pin,companion,defer){
    var s=root.state,base=companion?[companion,dict]:[dict];if(!s.symmetryEnabled)return base;
    function map(d,fp,fv){return Object.assign({},d,{segments:d.segments.map(function(x){
      return {point:fp(x.point[0],x.point[1]),handleIn:fv(x.handleIn[0],x.handleIn[1]),handleOut:fv(x.handleOut[0],x.handleOut[1])};})});}
    var w=pin.canvasW,h=pin.canvasH,out=base.slice();
    if(s.symmetryMode==='radial'){
      var c=s.symmetryRadialCenter||{x:w/2,y:h/2},n=Math.max(2,Math.min(24,s.symmetryRadialSectors|0||6));
      for(var k=n-1;k>=1;k--)(function(a){var cs=Math.cos(a),sn=Math.sin(a);
        var fp=function(x,y){var dx=x-c.x,dy=y-c.y;return [c.x+dx*cs-dy*sn,c.y+dx*sn+dy*cs];},fv=function(x,y){return [x*cs-y*sn,x*sn+y*cs];};
        out.push.apply(out,adopt(root,map(dict,fp,fv),companion,fp,fv,k));})(k*2*Math.PI/n);
      return out;
    }
    var cx=w/2,cy=h/2,r=Math.min(w,h)*0.4;
    var axis=s.symmetryAxis||(s.symmetryMode==='x'?{x1:0,y1:cy,x2:w,y2:cy}:s.symmetryMode==='free'?{x1:cx-r,y1:cy-r,x2:cx+r,y2:cy+r}:{x1:cx,y1:0,x2:cx,y2:h});
    var len=Math.hypot(axis.x2-axis.x1,axis.y2-axis.y1)||1,ux=(axis.x2-axis.x1)/len,uy=(axis.y2-axis.y1)/len;
    function side(p){return (axis.x2-axis.x1)*(p[1]-axis.y1)-(axis.y2-axis.y1)*(p[0]-axis.x1)>=0;}
    function vector(x,y){var t=x*ux+y*uy;return [2*t*ux-x,2*t*uy-y];}
    function point(x,y){var t=(x-axis.x1)*ux+(y-axis.y1)*uy;return [2*(axis.x1+t*ux)-x,2*(axis.y1+t*uy)-y];}
    var stroke=dict;
    // clipPathAtAxis clips the drawn path's segments: a ribbon's outline, never its centreline, which the stroke keeps whole
    // (the codec stores it as authored); the mirror then maps that whole centreline (adopt).
    if(!s.symmetryExtend&&dict.segments.length&&!defer){
      var first=side(dict.segments[0].point),cut=dict.segments.length;
      for(var i=1;i<dict.segments.length;i++)if(side(dict.segments[i].point)!==first){cut=i;break;}
      // The baseline keeps a one-anchor remnant; a native path needs two, so refuse rather than drop it silently.
      if(cut<2)fail('This stroke starts at the symmetry axis; nothing remains to mirror.');
      stroke=Object.assign({},dict,{segments:dict.segments.slice(0,cut)});out[out.length-1]=stroke;
    }
    var mirrored=map(stroke,point,vector);
    if(mirrored.closed)mirrored=Object.assign({},mirrored,{segments:mirrored.segments.slice().reverse().map(function(x){return {point:x.point,handleIn:x.handleOut,handleOut:x.handleIn};})});
    out.push.apply(out,adopt(root,mirrored,companion,point,vector,1));
    if(defer&&!s.symmetryExtend)out.clip={axis:[axis.x1,axis.y1,axis.x2,axis.y2],drawn:dict,mirror:out[out.length-1]};
    return out;
  }
  // The `trimClip` of a create: the axis and the [drawn, mirror] indices in the list sent (dicts, or batch steps {create: dict}).
  function clip(info,list){
    var at=function(d){return list.findIndex(function(x){return (x&&x.create||x)===d;});};
    return {axis:info.axis,mirrors:[[at(info.drawn),at(info.mirror)]]};
  }
  // The same copies as affine maps [a, b, c, d, e, f] (x' = a x + c y + e, y' = b x + d y + f), in stack order above the
  // path, for a commit whose final path only the native owner knows (the Fill Brush). A mirror needs Extend: stop-at-axis
  // would clip the committed path itself.
  function matrices(root,pin){
    var s=root.state,w=pin.canvasW,h=pin.canvasH,out=[];if(!s.symmetryEnabled)return out;
    if(s.symmetryMode==='radial'){
      var c=s.symmetryRadialCenter||{x:w/2,y:h/2},n=Math.max(2,Math.min(24,s.symmetryRadialSectors|0||6));
      for(var k=n-1;k>=1;k--){var a=k*2*Math.PI/n,cs=Math.cos(a),sn=Math.sin(a);out.push([cs,sn,-sn,cs,c.x-(cs*c.x-sn*c.y),c.y-(sn*c.x+cs*c.y)]);}
      return out;
    }
    if(!s.symmetryExtend)fail('Stop-at-axis symmetry with the Fill Brush is unavailable in native Drawing.');
    var cx=w/2,cy=h/2,r=Math.min(w,h)*0.4;
    var axis=s.symmetryAxis||(s.symmetryMode==='x'?{x1:0,y1:cy,x2:w,y2:cy}:s.symmetryMode==='free'?{x1:cx-r,y1:cy-r,x2:cx+r,y2:cy+r}:{x1:cx,y1:0,x2:cx,y2:h});
    var len=Math.hypot(axis.x2-axis.x1,axis.y2-axis.y1)||1,ux=(axis.x2-axis.x1)/len,uy=(axis.y2-axis.y1)/len;
    var r00=2*ux*ux-1,r01=2*ux*uy,r11=2*uy*uy-1;
    return [[r00,r01,r01,r11,axis.x1-(r00*axis.x1+r01*axis.y1),axis.y1-(r01*axis.x1+r11*axis.y1)]];
  }
  return Object.freeze({copies:copies,clip:clip,matrices:matrices});
}));
