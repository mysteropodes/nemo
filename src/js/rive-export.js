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
// range — never true for hand-drawn frame-by-frame strokes.
//
// So instead of morphing, this builds a "flipbook": one static Shape per
// (layer, stroke-slot, contiguous run of frames with IDENTICAL stroke
// data) with hold-interpolated opacity keyframes (1 while it's the active
// frame's content, 0 the instant the next run starts) — mechanically the
// same "one thing gets swapped for another every N frames" idea Lottie's
// own per-slot layer-splitting already uses, just substituting an opacity
// toggle for what Lottie gets for free via its ip/op frame range.
//
// Known gaps vs the Lottie exporter (documented, not silently dropped):
// - Blend modes: path_editor's paint schema has no blendMode field, so
//   layer blend modes are not carried over.
// - Symbols/components: getEffectiveStrokes() already flattens these into
//   plain per-frame stroke data (same as Lottie), so they DO export, just
//   as baked geometry rather than a reusable Rive component.
// - Held-frame run collapsing (fewer Shapes when nothing changed for many
//   frames) only happens when the camera layer is off — with the camera
//   on, every frame's geometry differs after the bake (same as Lottie has
//   no choice but to do too), so every frame gets its own Shape.
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
    var parsed=text?JSON.parse(text):null;
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
  // camera-bake math, since it's already exactly the geometry we need,
  // just missing the final "add the handle offset to get an absolute
  // control point" step Lottie's own i/o convention leaves implicit.
  function riveBuildCommands(shapeVal){
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

  function riveBuildPaints(sd){
    var paints=[];
    if(sd.hasRealStroke)paints.push({paintType:'stroke',color:riveColorHex(sd.strokeColor,sd.opacity),width:sd.strokeWidth||2});
    if(sd.fillColor)paints.push({paintType:'fill',color:riveColorHex(sd.fillColor,sd.opacity)});
    return paints;
  }

  // Content signature for held-frame collapsing — two consecutive frames
  // with an identical signature reuse the SAME Shape (one hold-1 span
  // instead of a new Shape per frame). Only the raw (pre-camera) fields
  // that affect the actual drawn geometry/paint matter here.
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
    var collapseRuns=!camActive;

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
    // pending[i] = {name, opacityKeys:[{frame,value}]} — filled in as shapes
    // are planned, resolved to real object ids after creation (createShapes
    // doesn't return ids, so we look them up afterward by the unique names
    // we assign here).
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
        var runStartFrame=null,runSig=null;
        for(f=r.start;f<=r.end+1;f++){
          var sd=(f<=r.end&&framesStrokes[f])?framesStrokes[f][slot]:null;
          var sig=sd?(collapseRuns?riveSignature(sd):('f'+f)):null;
          if(sig!==runSig){
            if(runStartFrame!==null){
              // close the previous run at f (hold 0 the instant it ends,
              // unless it's already the very last exported frame)
              var lastShape=pending[pending.length-1];
              lastShape.opacityKeys.push({frame:f-r.start,value:0});
            }
            if(sd){
              var name='n'+(shapeCounter++);
              var cm=camByFrame?camByFrame[f]:null;
              var shapeVal=lottieShapeValue(sd,cm);
              shapeDefs.push({
                name:name,x:0,y:0,parentId:artboardId,
                paints:riveBuildPaints(sd),
                paths:[{name:'P',commands:riveBuildCommands(shapeVal)}]
              });
              pending.push({name:name,opacityKeys:[{frame:f-r.start,value:1}]});
              runStartFrame=f;
            }else{
              runStartFrame=null;
            }
            runSig=sig;
          }
        }
      }
    }

    onProgress('Création de '+shapeDefs.length+' formes dans Rive…');
    await riveCreateShapesBatch(artboardId,shapeDefs);

    onProgress('Résolution des identifiants…');
    var tree=await riveCall('get_artboard_hierarchy',{artboardId:artboardId,depth:1});
    var idByName={};
    (tree.objects||[]).forEach(function(o){if(o.types&&o.types.indexOf('Shape')>=0)idByName[o.name]=o.id;});

    // createShapes doesn't guarantee children end up appended in creation
    // order (confirmed live: a shape created AFTER Background still showed
    // up ahead of it in the artboard's own children list) — draw order is
    // therefore NOT something to infer from call order. Pin the background
    // explicitly instead of assuming it.
    if(idByName['Background']){
      await riveCall('reorder_objects',{operations:[{objectId:idByName['Background'],order:'sendToBack'}]});
    }

    onProgress('Écriture des animations…');
    var keyframes=[];
    pending.forEach(function(p){
      var objectId=idByName[p.name];
      if(!objectId)return; // shape creation silently skipped this one — nothing to key
      p.opacityKeys.forEach(function(k){
        keyframes.push({objectId:objectId,propertyKey:18,frame:k.frame,value:k.value,interpolationType:'hold'});
      });
    });
    await riveKeyframeBatch(animationId,keyframes);

    onProgress('Terminé.');
    return{ok:true,artboardId:artboardId,artboardName:artboardName,animationId:animationId,shapeCount:shapeDefs.length};
  }

  window.SMExport=window.SMExport||{};
  window.SMExport.exportRive=exportRive;
})();
