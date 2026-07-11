// ---- Export to Rive, via a direct MCP connection to a locally-running
// Rive Editor (http://127.0.0.1:9791/mcp — the same server RiveBar talks
// to, but RiveBar itself is irrelevant here: Nemo is its own MCP client,
// no RiveBar involved) ----
//
// Architecture note (why this can't just mirror the Lottie exporter):
// Lottie's shape layers keyframe the RAW VERTEX ARRAY of a path directly —
// a completely different point count from one keyframe to the next is
// fine, the player just re-reads whatever array is at each keyframe. Rive
// has no equivalent: animating a path means keeping the SAME PointsPath's
// vertex objects alive and keyframing their x/y individually, which is
// only possible when the vertex count never changes across the animated
// range — never true for hand-drawn frame-by-frame strokes in general.
//
// BUT: Nemo's own tween engine resamples every interpolated frame between
// two keyframes to a fixed, uniform point count (confirmed live: 3-point
// authored keyframes produced 50-point interpolated frames, identically
// 50 on every single frame in between). That stretch — a run of 2+
// consecutive frames sharing the exact same point count — genuinely CAN
// be native Rive vertex animation: one Shape, one PointsPath, keyframe
// each vertex's x/y every frame. Two strategies coexist here per
// (layer, stroke-slot) run, chosen automatically:
//   - "static": 2+ consecutive frames with byte-identical stroke data
//     (a held/non-tweened stretch) → ONE unanimated Shape, full cubic
//     bezier fidelity (handles preserved), just an opacity hold to show
//     it only for its span. Cheapest — no per-vertex keyframing at all.
//   - "morph": 2+ consecutive frames with the same POINT COUNT but
//     different positions (genuine tween/camera-bake motion) → ONE Shape
//     built from STRAIGHT vertices (no bezier handles — see tradeoff
//     note below), x/y keyframed every frame, real native interpolation.
//   - Anything that's neither (an isolated single frame, or a boundary
//     frame whose point count doesn't match its neighbor — the RAW
//     authored keyframe frames themselves keep their original,
//     un-resampled point count, e.g. 3 vs the 50-point tween interior)
//     falls back to one flipbook Shape for just that one frame, full
//     cubic fidelity (same code path as "static" with a run length of 1).
//
// Straight-vertex tradeoff for "morph": Rive infers a vertex's actual
// type (Straight vs CubicMirrored vs a fully asymmetric cubic) from the
// handle geometry passed at creation time, and each type exposes DIFFERENT
// animatable property keys (a mirrored vertex keys rotation+distance, not
// separate in/out offsets) — keyframing would have to match whatever type
// Rive silently chose, and that choice isn't guaranteed stable across a
// batch of otherwise-identical vertices. Sidestepped entirely by building
// morph geometry from straight line segments only (x/y only, always the
// same 2 property keys) — visually a very close approximation given how
// dense Nemo's own resampling already is (50 points for what was a
// 3-point stroke), but not pixel-identical to the bezier "static" shapes.
//
// Known gaps vs the Lottie exporter (documented, not silently dropped):
// - Blend modes: path_editor's paint schema has no blendMode field, so
//   layer blend modes are not carried over.
// - Symbols/components: getEffectiveStrokes() already flattens these into
//   plain per-frame stroke data (same as Lottie), so they DO export, just
//   as baked geometry rather than a reusable Rive component.
// - Paint (color/width) is taken from the run's FIRST frame only, even
//   for "morph" runs — if color genuinely animates within one tween
//   (rare), that change is not reproduced.
// - Draw order BETWEEN Nemo layers is not independently verified: a live
//   test showed a shape created AFTER the background still ended up ahead
//   of it in the artboard's own children list, so "creation order = paint
//   order" is false — fixed for the background specifically via an
//   explicit reorder_objects sendToBack call below, but inter-layer
//   ordering among the frame shapes themselves still just relies on
//   creation order and hasn't been visually confirmed correct for a
//   multi-layer scene. Check this on a real multi-layer export before
//   trusting stacking on anything more complex than one drawing layer.
(function(){
  var RIVE_MCP_URL='http://127.0.0.1:9791/mcp';
  var _riveReqId=1,_riveInitialized=false;

  async function riveFetch(body){
    var res=await fetch(RIVE_MCP_URL,{
      method:'POST',
      headers:{'Content-Type':'application/json','Accept':'application/json, text/event-stream'},
      body:JSON.stringify(body)
    });
    if(!res.ok)throw new Error('Rive MCP indisponible (HTTP '+res.status+') — Rive Editor est-il ouvert avec le serveur MCP actif ?');
    var ct=res.headers.get('content-type')||'';
    if(ct.indexOf('application/json')<0)return null; // 202 Accepted on notifications, no body
    return await res.json();
  }
  async function riveRpc(method,params){
    var json=await riveFetch({jsonrpc:'2.0',id:_riveReqId++,method:method,params:params});
    if(json&&json.error)throw new Error('Rive MCP: '+json.error.message);
    return json&&json.result;
  }
  async function riveNotify(method,params){
    await riveFetch({jsonrpc:'2.0',method:method,params:params});
  }
  async function riveEnsureInit(){
    if(_riveInitialized)return;
    await riveRpc('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'nemo',version:'1.0'}});
    await riveNotify('notifications/initialized',{});
    _riveInitialized=true;
  }
  async function riveCall(toolName,args){
    await riveEnsureInit();
    var result=await riveRpc('tools/call',{name:toolName,arguments:args});
    var text=result&&result.content&&result.content[0]&&result.content[0].text;
    var parsed=null;
    try{parsed=text?JSON.parse(text):null;}catch(e){
      // Some failure paths return a plain error string instead of JSON
      // (confirmed live with a bad objectId) — surface it verbatim rather
      // than crashing on JSON.parse.
      throw new Error('Rive ('+toolName+'): '+text);
    }
    if(parsed&&parsed.success===false){
      throw new Error('Rive ('+toolName+'): '+(parsed.errors&&parsed.errors.length?JSON.stringify(parsed.errors):'échec inconnu'));
    }
    return parsed;
  }

  function riveColorHex(hex,opacity){
    if(!hex)return '#00000000';
    var h=hex.replace('#','');
    if(h.length===3)h=h[0]+h[0]+h[1]+h[1]+h[2]+h[2];
    var r=h.substr(0,2),g=h.substr(2,2),b=h.substr(4,2);
    var a=h.length>=8?h.substr(6,2):Math.round((opacity!==undefined?opacity:1)*255).toString(16).padStart(2,'0');
    return ('#'+a+r+g+b).toUpperCase();
  }

  // Builds absolute-coordinate cubicTo commands from a lottieShapeValue()
  // result ({i,o,v,c} — relative in/out handle offsets, absolute points,
  // closed flag) — reused as-is from export.js rather than duplicating the
  // camera-bake math. Used for "static" (single-instant, full-fidelity)
  // shapes.
  function riveBuildCubicCommands(shapeVal){
    var v=shapeVal.v,i=shapeVal.i,o=shapeVal.o,closed=shapeVal.c;
    var n=v.length;
    if(!n)return [];
    var cmds=[{commandType:'moveTo',x:v[0][0],y:v[0][1]}];
    for(var k=0;k<n-1;k++){
      var p1=v[k],p2=v[k+1];
      cmds.push({commandType:'cubicTo',
        control1X:p1[0]+o[k][0],control1Y:p1[1]+o[k][1],
        control2X:p2[0]+i[k+1][0],control2Y:p2[1]+i[k+1][1],
        endX:p2[0],endY:p2[1]});
    }
    if(closed&&n>1){
      var pl=v[n-1],p0=v[0];
      cmds.push({commandType:'cubicTo',
        control1X:pl[0]+o[n-1][0],control1Y:pl[1]+o[n-1][1],
        control2X:p0[0]+i[0][0],control2Y:p0[1]+i[0][1],
        endX:p0[0],endY:p0[1]});
    }
    if(closed)cmds.push({commandType:'close'});
    return cmds;
  }

  // Straight-segment-only commands (no handles) — used for "morph" shapes,
  // see the straight-vertex tradeoff note at the top of the file.
  function riveBuildStraightCommands(points,closed){
    var n=points.length;
    if(!n)return [];
    var cmds=[{commandType:'moveTo',x:points[0][0],y:points[0][1]}];
    for(var k=1;k<n;k++)cmds.push({commandType:'lineTo',x:points[k][0],y:points[k][1]});
    if(closed)cmds.push({commandType:'close'});
    return cmds;
  }

  function riveBuildPaints(sd,hasRealStroke){
    var paints=[];
    if(hasRealStroke)paints.push({paintType:'stroke',color:riveColorHex(sd.strokeColor,sd.opacity),width:sd.strokeWidth||2});
    if(sd.fillColor)paints.push({paintType:'fill',color:riveColorHex(sd.fillColor,sd.opacity)});
    return paints;
  }

  // Nemo's tween engine resamples a stroke's GEOMETRY onto every
  // interpolated frame but doesn't carry the hasRealStroke flag itself
  // along for the ride (confirmed live: strokeColor/strokeWidth are both
  // correctly present and interpolated, hasRealStroke is simply undefined)
  // — CLAUDE.md's documented reason this field exists at all is that
  // strokeColor has a legacy white fallback even on shapes that never had
  // a real stroke, so treating "strokeColor is truthy" as the signal would
  // resurrect exactly the phantom-white-stroke bug that field was added to
  // prevent. Resolve it from the nearest frame (search back then forward)
  // where it's actually set instead — paint identity doesn't change mid-
  // tween by construction (documented gap: color/width are also only ever
  // read from the run's first frame), so any frame in the same run gives
  // the right answer.
  function resolveHasRealStroke(framesStrokes,slot,frameIdx,rangeStart,rangeEnd){
    for(var f=frameIdx;f>=rangeStart;f--){
      var sd=framesStrokes[f]&&framesStrokes[f][slot];
      if(sd&&sd.hasRealStroke!==undefined)return !!sd.hasRealStroke;
    }
    for(var f2=frameIdx;f2<=rangeEnd;f2++){
      var sd2=framesStrokes[f2]&&framesStrokes[f2][slot];
      if(sd2&&sd2.hasRealStroke!==undefined)return !!sd2.hasRealStroke;
    }
    return false;
  }

  // Content signature — two frames with an identical signature are the
  // SAME drawing (a held/non-tweened frame), eligible for the cheap
  // "static" path. Only the raw (pre-camera) fields that affect the
  // actual drawn geometry/paint matter here.
  function riveSignature(sd){
    return JSON.stringify({s:sd.segments,c:!!sd.closed,f:sd.fillColor||null,
      sc:sd.hasRealStroke?sd.strokeColor:null,sw:sd.strokeWidth,o:sd.opacity});
  }

  async function riveCreateShapesBatch(parentId,shapeDefs){
    // Batched, but chunked — a long animation can produce thousands of
    // shapes and a single giant JSON-RPC payload risks timing out or
    // exceeding whatever the MCP transport's practical limit is.
    var CHUNK=40;
    for(var i=0;i<shapeDefs.length;i+=CHUNK){
      await riveCall('path_editor',{command:'createShapes',data:{shapes:shapeDefs.slice(i,i+CHUNK)}});
    }
  }

  async function riveKeyframeBatch(animationId,keyframes){
    var CHUNK=200;
    for(var i=0;i<keyframes.length;i+=CHUNK){
      await riveCall('animation_editor',{command:'modifyKeyFrames',data:{animationId:animationId,add:keyframes.slice(i,i+CHUNK)}});
    }
  }

  // Splits [r.start..r.end] into maximal runs of constant point count for
  // one (layer,slot) — the coarse pass shared by both the "static" and
  // "morph" strategies. A run boundary happens whenever the slot's stroke
  // disappears or its point count changes (which always happens exactly
  // at a raw authored keyframe, since that's the only place Nemo's own
  // resampled/un-resampled point counts can mismatch their neighbor).
  function riveCountRuns(framesStrokes,slot,start,end){
    var runs=[],curStart=null,curCount=null;
    for(var f=start;f<=end+1;f++){
      var sd=(f<=end&&framesStrokes[f])?framesStrokes[f][slot]:null;
      var cnt=sd?sd.segments.length:null;
      if(cnt!==curCount){
        if(curStart!==null&&curCount!==null)runs.push({start:curStart,end:f-1,count:curCount});
        curStart=sd?f:null;
        curCount=cnt;
      }
    }
    return runs;
  }

  async function exportRive(opts){
    var r=exportFrameRange(opts);
    var onProgress=(opts&&opts.onRiveProgress)||function(){};
    onProgress('Connexion à Rive Editor…');
    try{
      await riveEnsureInit();
    }catch(e){
      return{ok:false,error:'Impossible de joindre Rive Editor (MCP sur 127.0.0.1:9791). Ouvre Rive Editor avec le serveur MCP actif, puis réessaie. Détail: '+e.message};
    }

    onProgress('Création de l\'artboard…');
    var artboardName='Nemo Export '+new Date().toISOString().slice(0,16).replace('T',' ');
    var abRes=await riveCall('open_file_editor',{command:'createArtboard',data:{createArtboard:[{name:artboardName,width:state.canvasW,height:state.canvasH,x:0,y:0}]}});
    var artboardId=abRes.artboards[0].id;

    var animRes=await riveCall('animation_editor',{command:'createLinearAnimations',data:{createLinearAnimations:{linearAnimations:[{name:'Timeline',duration:(r.end-r.start+1)}]}}});
    var animationId=animRes.animations[0].id;

    var camActive=!!(window.SMCamera&&state.cameraLayerOn&&state.cameraKeys.length);
    var camByFrame=null;
    if(camActive){
      camByFrame={};
      for(var cf=r.start;cf<=r.end;cf++){
        var cam=SMCamera.cameraAtFrame(cf);
        camByFrame[cf]=cam?lottieCamMatrix(cam):null;
      }
    }

    // Background rect — created first so it paints behind everything else
    // (Rive, like most painter's-algorithm renderers, draws children in
    // list order — first child = furthest back).
    var shapeDefs=[{
      name:'Background',x:state.canvasW/2,y:state.canvasH/2,parentId:artboardId,
      paints:[{paintType:'fill',color:riveColorHex(state.canvasBg,1)}],
      paths:[{name:'P',commands:[
        {commandType:'moveTo',x:-state.canvasW/2,y:-state.canvasH/2},
        {commandType:'lineTo',x:state.canvasW/2,y:-state.canvasH/2},
        {commandType:'lineTo',x:state.canvasW/2,y:state.canvasH/2},
        {commandType:'lineTo',x:-state.canvasW/2,y:state.canvasH/2},
        {commandType:'close'}]}]
    }];
    // pending[i] = {name, opacityKeys, morph:null|{frames,pointsByFrame}} —
    // filled in as shapes are planned, resolved to real object ids after
    // creation (createShapes doesn't return ids, so we look them up
    // afterward by the unique names we assign here).
    var pending=[];
    var shapeCounter=0;

    onProgress('Préparation des tracés…');
    for(var li=state.layers.length-1;li>=0;li--){
      var ld=state.layers[li];if(!ld.visible)continue;
      var framesStrokes=[];
      for(var f=r.start;f<=r.end;f++)framesStrokes[f]=getEffectiveStrokes(li,f).filter(function(sd){return sd.opacity!==0;});
      var maxSlots=0;
      for(f=r.start;f<=r.end;f++)maxSlots=Math.max(maxSlots,framesStrokes[f].length);

      for(var slot=0;slot<maxSlots;slot++){
        var countRuns=riveCountRuns(framesStrokes,slot,r.start,r.end);
        countRuns.forEach(function(run){
          var firstSd=framesStrokes[run.start][slot];
          var allSame=true;
          if(!camActive){
            var sig0=riveSignature(firstSd);
            for(var f2=run.start+1;f2<=run.end;f2++){
              if(riveSignature(framesStrokes[f2][slot])!==sig0){allSame=false;break;}
            }
          }else{
            allSame=(run.end===run.start); // camera bake changes geometry every frame
          }
          var useMorph=(run.end>run.start)&&!allSame;
          var name='n'+(shapeCounter++);
          var opacityKeys=[{frame:run.start-r.start,value:1}];
          if(run.end<r.end)opacityKeys.push({frame:run.end-r.start+1,value:0});
          var hasRealStroke=resolveHasRealStroke(framesStrokes,slot,run.start,r.start,r.end);

          if(!useMorph){
            var cm0=camByFrame?camByFrame[run.start]:null;
            var shapeVal0=lottieShapeValue(firstSd,cm0);
            shapeDefs.push({
              name:name,x:0,y:0,parentId:artboardId,
              paints:riveBuildPaints(firstSd,hasRealStroke),
              paths:[{name:'P',commands:riveBuildCubicCommands(shapeVal0)}]
            });
            pending.push({name:name,opacityKeys:opacityKeys,morph:null});
          }else{
            var cmStart=camByFrame?camByFrame[run.start]:null;
            var startVal=lottieShapeValue(firstSd,cmStart);
            var pointsByFrame={};
            for(var f3=run.start;f3<=run.end;f3++){
              var sd3=framesStrokes[f3][slot];
              var cm3=camByFrame?camByFrame[f3]:null;
              pointsByFrame[f3]=lottieShapeValue(sd3,cm3).v;
            }
            shapeDefs.push({
              name:name,x:0,y:0,parentId:artboardId,
              paints:riveBuildPaints(firstSd,hasRealStroke),
              paths:[{name:'P',commands:riveBuildStraightCommands(startVal.v,startVal.c)}]
            });
            pending.push({name:name,opacityKeys:opacityKeys,morph:{start:run.start,end:run.end,pointsByFrame:pointsByFrame}});
          }
        });
      }
    }

    onProgress('Création de '+shapeDefs.length+' formes dans Rive…');
    await riveCreateShapesBatch(artboardId,shapeDefs);

    onProgress('Résolution des identifiants…');
    // depth:3 — artboard(0) -> Shape(1) -> PointsPath(2) -> vertices(3).
    // Confirmed live: depth:2 stops one level too early, the PointsPath
    // entries come back without their own `children` (vertex id) array.
    var tree=await riveCall('get_artboard_hierarchy',{artboardId:artboardId,depth:3});
    var objectsById={};
    (tree.objects||[]).forEach(function(o){objectsById[o.id]=o;});
    var shapeByName={};
    (tree.objects||[]).forEach(function(o){if(o.types&&o.types.indexOf('Shape')>=0)shapeByName[o.name]=o;});
    function vertexIdsForShape(shapeName){
      var shape=shapeByName[shapeName];if(!shape)return null;
      var pathChild=(shape.children||[]).map(function(id){return objectsById[id];}).find(function(o){return o&&o.types&&o.types.indexOf('PointsPath')>=0;});
      return pathChild?pathChild.children:null;
    }

    // createShapes doesn't guarantee children end up appended in creation
    // order (confirmed live: a shape created AFTER Background still showed
    // up ahead of it in the artboard's own children list) — draw order is
    // therefore NOT something to infer from call order. Pin the background
    // explicitly instead of assuming it.
    var bgShape=shapeByName['Background'];
    if(bgShape){
      await riveCall('reorder_objects',{operations:[{objectId:bgShape.id,order:'sendToBack'}]});
    }

    onProgress('Écriture des animations…');
    var keyframes=[];
    var VERT_X=24,VERT_Y=25; // confirmed live via query_property_keys — same for Straight and Cubic vertex types
    pending.forEach(function(p){
      var shape=shapeByName[p.name];
      if(!shape)return; // shape creation silently skipped this one — nothing to key
      p.opacityKeys.forEach(function(k){
        keyframes.push({objectId:shape.id,propertyKey:18,frame:k.frame,value:k.value,interpolationType:'hold'});
      });
      if(p.morph){
        var vids=vertexIdsForShape(p.name);
        if(vids){
          for(var f4=p.morph.start;f4<=p.morph.end;f4++){
            var pts=p.morph.pointsByFrame[f4];
            for(var k2=0;k2<vids.length&&k2<pts.length;k2++){
              keyframes.push({objectId:vids[k2],propertyKey:VERT_X,frame:f4-r.start,value:pts[k2][0],interpolationType:'linear'});
              keyframes.push({objectId:vids[k2],propertyKey:VERT_Y,frame:f4-r.start,value:pts[k2][1],interpolationType:'linear'});
            }
          }
        }
      }
    });
    await riveKeyframeBatch(animationId,keyframes);

    onProgress('Terminé.');
    return{ok:true,artboardId:artboardId,artboardName:artboardName,animationId:animationId,shapeCount:shapeDefs.length,keyframeCount:keyframes.length};
  }

  window.SMExport=window.SMExport||{};
  window.SMExport.exportRive=exportRive;
})();
