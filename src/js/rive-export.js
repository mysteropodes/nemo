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
//
// Three more real bugs found by actually testing an export in Rive
// (reported: "l'animation ne rejoue pas avec le bon timing" and "les
// formes non fermées apparaissent fermées"), all confirmed and fixed
// via live round-trips against a running Rive Editor:
// - createLinearAnimations' `duration` input is NOT frame count — it's
//   SECONDS at Rive's own default fps (60), so passing a raw Nemo frame
//   count made every exported animation ~60x too long, with all the
//   actual keyframes crammed into its first ~1.5%. Fixed by explicitly
//   setting the animation's own fps(56) to state.fps and its real
//   frame-count duration(57) directly via set_property_values right
//   after creation, bypassing the seconds-based creation input.
// - createShapes always creates a PointsPath with isclosed=true
//   regardless of whether a `close` command was included — every open
//   stroke this export makes needs isclosed explicitly forced back to
//   false afterward (batched into one set_property_values call).
// - A "morph" run that's just the SAME shape rigidly translated (drag a
//   drawing from one keyframe to another, let it tween) doesn't need
//   per-vertex keyframing at all — riveDetectTranslation catches this
//   and keyframes the Shape's own x/y instead, 2*nVertices cheaper.
//   Confirmed live: a 50-vertex translated run dropped from 1100
//   keyframes to 22.
//
// Brush textures: Nemo renders a textured stroke as one invisible "anchor"
// path (data.brushTexturePreset) plus dozens/hundreds of separate,
// semi-transparent, jittered "dab" copy shapes stamped along it
// (data.isBrushTextureCopy — see BRUSH_PRESETS/buildBrushDabs in
// tools.js). Exporting all of those dabs as literal Rive shapes would
// work but be enormous. A genuine Rive PathEffect script (Luau,
// multi-octave seeded sine wobble) capable of reproducing that per-dab
// look was written and verified live against a running Rive Editor —
// but there is NO MCP tool that can attach a custom Luau PathEffect to
// a Path as a live component (component_editor.addComponents rejects a
// script asset outright: "Component not found"), so it was deleted
// again rather than left as a dead, never-attachable asset in the
// user's real Rive file. Automating that attachment isn't possible
// today, and this feature explicitly must not require any manual step
// in Rive Editor afterward.
// So instead the wobble is baked directly into the EXPORTED geometry
// here, in JS, before it ever reaches Rive: dab copies are dropped
// entirely, and the anchor's own points get a smooth per-point
// perpendicular offset (riveApplyWobble) derived automatically from the
// stroke's own brush preset (riveBrushWobbleParams) — same multi-octave
// seeded-sine shape the (now-deleted) Luau script used, so the two
// stayed conceptually the same "look"; this is the version that
// actually ships since it needs zero manual Rive Editor step.
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

  // A degenerate single-point path (moveTo+lineTo to the SAME point) —
  // the minimum createShapes accepts ("at least one moveTo and one
  // lineTo/cubicTo"). Used as a throwaway placeholder for cubic-fidelity
  // shapes: see the file's vertex0-tangent-loss note below for why the
  // path can't just be built from cubicTo commands directly.
  function riveBuildPlaceholderCommands(pt){
    return [{commandType:'moveTo',x:pt[0],y:pt[1]},{commandType:'lineTo',x:pt[0],y:pt[1]}];
  }

  // Converts a lottieShapeValue() result into addVertices' vertex list
  // (explicit type:'cubic', same relative in/out offset convention
  // riveBuildCubicCommands already used) — used together with the
  // placeholder-then-delete dance below instead of createShapes' own
  // moveTo/cubicTo command list.
  //
  // Why: createShapes silently mis-classifies the FIRST vertex of an open
  // path as a plain StraightVertex (no curvature at all) even when the
  // very next cubicTo command clearly encodes a real out-tangent for it —
  // confirmed live, repeatedly, isolating the exact cause: a vertex
  // defined via `moveTo` never gets typed as cubic, no matter what the
  // following cubicTo's control1 says. Vertices 2+ (defined by their own
  // cubicTo) classify correctly. Reported by the user as "la première
  // n'a pas pris en compte les tangentes".
  // Fix: create the shape with a single-point PLACEHOLDER path instead,
  // then use addVertices (which accepts an explicit `type` per vertex,
  // bypassing whatever heuristic createShapes uses) to insert every real
  // vertex — including the first — after that placeholder, and finally
  // delete_objects the placeholder vertex itself (a generic object,
  // deletable the same way an artboard/shape is elsewhere in this file).
  // Isclosed doesn't need a duplicated closing vertex/cubic segment the
  // way the old command-list approach did — once isclosed is set (see
  // the existing ISCLOSED_KEY fix below), Rive wraps the last vertex's
  // own out-handle into the first vertex's own in-handle automatically.
  function riveCubicVerticesFromShapeVal(shapeVal){
    var v=shapeVal.v,i=shapeVal.i,o=shapeVal.o;
    var verts=[];
    for(var k=0;k<v.length;k++){
      verts.push({x:v[k][0],y:v[k][1],type:'cubic',inX:i[k][0],inY:i[k][1],outX:o[k][0],outY:o[k][1]});
    }
    return verts;
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

  function riveBuildPaints(sd,hasRealStroke,opacityOverride){
    var paints=[];
    // A brush-texture anchor's own opacity is deliberately 0 in Nemo (see
    // the filter comment above, in exportRive) — reading sd.opacity
    // directly here would make the exported stroke invisible. Callers
    // pass preTextureOpacity (the value the user actually authored,
    // before Nemo hid the anchor to let its dabs draw instead) for those.
    var op=opacityOverride!==undefined?opacityOverride:sd.opacity;
    if(hasRealStroke)paints.push({paintType:'stroke',color:riveColorHex(sd.strokeColor,op),width:sd.strokeWidth||2});
    if(sd.fillColor)paints.push({paintType:'fill',color:riveColorHex(sd.fillColor,op)});
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

  // Same "the tween engine doesn't carry this field onto resampled
  // interpolated frames" problem as hasRealStroke above, for the brush
  // preset tag — search neighboring frames in the run instead of trusting
  // whichever frame happens to be the run's own start.
  function resolveBrushTexturePreset(framesStrokes,slot,frameIdx,rangeStart,rangeEnd){
    for(var f=frameIdx;f>=rangeStart;f--){
      var sd=framesStrokes[f]&&framesStrokes[f][slot];
      if(sd&&sd.brushTexturePreset)return sd.brushTexturePreset;
      if(sd&&sd.brushTexturePreset===null)break; // explicit "not textured" on an authored frame — stop searching backward
    }
    for(var f2=frameIdx;f2<=rangeEnd;f2++){
      var sd2=framesStrokes[f2]&&framesStrokes[f2][slot];
      if(sd2&&sd2.brushTexturePreset)return sd2.brushTexturePreset;
    }
    return null;
  }

  // ---- Brush texture, automatic wobble mapping (no manual step, no
  // per-preset hand-tuning — see the file's top-of-file architecture
  // note for why this bakes into geometry instead of a Rive PathEffect) ----
  function riveNoise(t,seed){
    var s=seed*12.9898;
    return Math.sin(t*1.00+s)*0.55+Math.sin(t*2.13+s*1.7)*0.30+Math.sin(t*4.71+s*2.3)*0.15;
  }
  // djb2 — turns a stroke's own id (or any string) into a stable numeric
  // seed, so the SAME stroke wobbles identically every time it's
  // exported and every frame it appears in (not a fresh random look per
  // run — same reasoning as Nemo's own seededRng for tween re-stamping).
  function riveHashSeed(str){
    var h=5381;
    for(var i=0;i<str.length;i++)h=((h<<5)+h+str.charCodeAt(i))|0;
    return (h>>>0)%1000/37; // spread into a few dozen distinct phases
  }
  // Derives {amp (px), freqPer100 (wobble cycles per 100px of path)} from
  // a BRUSH_PRESETS entry — no per-preset hand authoring, every existing
  // AND future/custom preset (state.customBrushPresets) gets a value for
  // free from the same knobs Nemo's own dab-stamping already uses:
  //   - amplitude scales with strokeWidth * nibSize (how big the preset's
  //     dabs are) * scatter (how far Nemo already lets dabs stray off the
  //     centerline) — the direct geometric analogue of "scatter".
  //   - frequency scales inversely with spacing (tighter dab spacing =
  //     more dabs per unit length = a busier-looking wobble).
  function riveBrushWobbleParams(preset,strokeWidth){
    var w=strokeWidth||3;
    var amp=w*(preset.nibSize||1)*(preset.scatter||0)*0.9;
    var spacing=Math.max(preset.spacing||0.4,0.05);
    var freqPer100=Math.min(30,Math.max(1.5,100/(w*(preset.nibSize||1)*spacing*2)));
    return{amp:amp,freqPer100:freqPer100};
  }
  // Applies a smooth per-point perpendicular offset to an absolute point
  // array (v, as produced by lottieShapeValue) — index-fraction phased
  // (not true arc-length) since we already have a point ARRAY here, not
  // a live measurable Path; acceptable given Nemo's own points are
  // already reasonably evenly spaced along the stroke (especially true
  // for "morph" runs, which are Nemo's own uniform tween resampling).
  function riveApplyWobble(v,ampPx,freqPer100,seed,closed){
    var n=v.length;
    if(n<2||ampPx<=0)return v;
    var out=new Array(n);
    for(var k=0;k<n;k++){
      var prev=v[k>0?k-1:(closed?n-1:k)];
      var next=v[k<n-1?k+1:(closed?0:k)];
      var dx=next[0]-prev[0],dy=next[1]-prev[1];
      var len=Math.sqrt(dx*dx+dy*dy)||1;
      var nx=-dy/len,ny=dx/len; // unit normal
      var phase=(k/Math.max(n-1,1))*freqPer100*2*Math.PI;
      var w=riveNoise(phase,seed)*ampPx;
      out[k]=[v[k][0]+nx*w,v[k][1]+ny*w];
    }
    return out;
  }

  // Nested-artboard camera: derives the CONTENT instance's own x/y/scale/
  // rotation (Node transform keys 13/14/16/17/15, already proven live for
  // opacity/vertices on the same objects) that reproduces Nemo's camera
  // framing — same math as lottieCamMatrix/lottieCamPoint (export.js),
  // just solved for a single T*R*S transform applied to the WHOLE nested
  // artboard instance instead of re-deriving it per vertex:
  //   worldPos = (canvasW/2,canvasH/2) + R(rot)·S(s)·(localPos - camCenter)
  //            = T + R·S·localPos   where  T = (canvasW/2,canvasH/2) - R·S·camCenter
  // Rotation unit assumed radians (Rive/rive-cpp's own internal Node
  // convention) — NOT independently visually confirmed today (screen
  // access dropped mid-session before a rotating-camera test could be
  // scrubbed); position and scale were confirmed live via property
  // read-back using this exact same key set.
  function riveCameraInstanceTransform(cam){
    var rot=-(cam.rot||0)*Math.PI/180;
    var cos=Math.cos(rot),sin=Math.sin(rot);
    var s=state.canvasW/cam.w;
    return{
      x:state.canvasW/2-(s*cos*cam.x-s*sin*cam.y),
      y:state.canvasH/2-(s*sin*cam.x+s*cos*cam.y),
      sx:s,sy:s,r:rot
    };
  }

  // Optimization: a "morph" run where every frame is just the SAME shape
  // rigidly translated (every vertex moves by the identical delta, e.g.
  // Nemo user drags one drawing from one keyframe's position to another
  // and lets it tween) doesn't need per-vertex keyframing at all — one
  // Shape's x/y transform keyframed is geometrically identical and is
  // 2*nVertices times cheaper (2 keyframes per frame instead of
  // 2*nVertices). Returns {frame:[dx,dy], ...} keyed by absolute Nemo
  // frame number if every frame in [startFrame,endFrame] is a pure
  // translation of startFrame's own points, else null (falls back to the
  // full per-vertex morph).
  function riveDetectTranslation(pointsByFrame,startFrame,endFrame){
    var EPS=0.05;
    var basePts=pointsByFrame[startFrame];
    var n=basePts.length;
    var deltas={};
    deltas[startFrame]=[0,0];
    for(var f=startFrame+1;f<=endFrame;f++){
      var pts=pointsByFrame[f];
      if(!pts||pts.length!==n)return null;
      var dx=pts[0][0]-basePts[0][0],dy=pts[0][1]-basePts[0][1];
      for(var k=1;k<n;k++){
        var kdx=pts[k][0]-basePts[k][0],kdy=pts[k][1]-basePts[k][1];
        if(Math.abs(kdx-dx)>EPS||Math.abs(kdy-dy)>EPS)return null;
      }
      deltas[f]=[dx,dy];
    }
    return deltas;
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
    // animation_editor has no artboardId parameter anywhere in its schema
    // — createLinearAnimations operates on whatever artboard is currently
    // ACTIVE in the editor, not on the one just created. Confirmed live
    // via a real bug: every previous export's animation (with every
    // keyframe correctly written) was silently attaching to whatever
    // artboard the user had open before running the export (its own
    // artboardid property pointed at that OTHER artboard, not this
    // export's) — the exported shapes were all correct, but there was
    // never a real, visible, playable Timeline animation on the artboard
    // itself. Focusing the new artboard first is required before any
    // artboard-implicit tool call (animations, and by the same logic
    // possibly others) — this is the actual root cause of "the animation
    // doesn't replay" (reported), not (only) the fps/duration bug fixed
    // earlier.
    await riveCall('open_file_editor',{command:'focusArtboard',data:{focusArtboard:{artboardId:artboardId}}});

    var animRes=await riveCall('animation_editor',{command:'createLinearAnimations',data:{createLinearAnimations:{linearAnimations:[{name:'Timeline',duration:(r.end-r.start+1)}]}}});
    var animationId=animRes.animations[0].id;
    // createLinearAnimations' own `duration` input is NOT frame count —
    // confirmed live: passing 13 produced an animation whose internal
    // frame-duration read back as 780, i.e. it was treated as SECONDS at
    // Rive's own default fps (60), 13*60=780. Every keyframe this export
    // writes uses a literal Nemo frame index as `frame`, so the
    // animation's own fps MUST equal Nemo's fps for those indices to mean
    // the same thing Rive does — without this, the whole export crams
    // into the first ~1.5% of a needlessly 60x-too-long timeline (this is
    // why an exported animation "didn't replay with the right timing").
    // fps(56) and the real frame-count duration(57) are set directly,
    // bypassing the seconds-based creation input entirely.
    await riveCall('set_property_values',{propertyValues:(function(){var o={};o[animationId]={56:state.fps,57:(r.end-r.start+1)};return o;})()});

    var camActive=!!(window.SMCamera&&state.cameraLayerOn&&state.cameraKeys.length);
    // Camera: NOT baked into content geometry (that was the old approach —
    // every shape got per-frame-rebuilt points, forcing the expensive
    // per-vertex morph path everywhere even for otherwise-static drawings,
    // and re-deriving the exact same pan/zoom math shape by shape). A real
    // Rive nested-artboard instance does this properly instead: all
    // content goes into a separate CHILD artboard (isComponent:true,
    // built completely camera-unaware — content geometry below never
    // reads `camActive` again), instanced once into this export's real
    // artboard, and the INSTANCE's own x/y/scale/rotation (same transform
    // keys already proven live: 13/14/16/17/15) gets keyframed to
    // reproduce the camera's pan/zoom/roll. Content shapes stay eligible
    // for the "static"/translation cost optimizations regardless of
    // camera motion, since their own geometry no longer changes because
    // of it. Background stays OUTSIDE the nested instance (fixed
    // backdrop, not panned/zoomed — matches how a camera move in Nemo
    // itself never rewrites the canvas background).
    var contentArtboardId=artboardId;
    if(camActive){
      onProgress('Création de l\'artboard de contenu (caméra)…');
      var childRes=await riveCall('open_file_editor',{command:'createArtboard',data:{createArtboard:[{name:artboardName+' — Content',width:state.canvasW,height:state.canvasH,x:state.canvasW+200,y:0,isComponent:true}]}});
      contentArtboardId=childRes.artboards[0].id;
    }

    // ---- Pass 0: figure out per-(layer,slot) stroke data once, and how
    // many "tracks" (slots) each visible layer needs — shared by the
    // container pre-pass below and the real shape-building pass, so
    // getEffectiveStrokes/filtering only runs once per frame. ----
    var slotsInfo=[];
    for(var li0=state.layers.length-1;li0>=0;li0--){
      var ld0=state.layers[li0];if(!ld0.visible)continue;
      var framesStrokes0=[];
      // Dab copies (isBrushTextureCopy) are dropped entirely — their
      // visual contribution is replaced by wobbling the anchor's own
      // points below, automatically, per riveBrushWobbleParams. A brush
      // anchor is deliberately opacity:0 in Nemo itself (that's how it
      // stays invisible on-canvas while its dabs do the actual drawing —
      // see applyBrushTexture in tools.js) — the general opacity!==0
      // filter would silently drop exactly the stroke this export needs
      // to keep, so anchors are exempted from it (their real paint
      // opacity is overridden separately below, from the preset, not
      // read from this deliberately-zeroed field).
      for(var f0=r.start;f0<=r.end;f0++)framesStrokes0[f0]=getEffectiveStrokes(li0,f0).filter(function(sd){return !sd.isBrushTextureCopy&&(sd.opacity!==0||sd.brushTexturePreset);});
      var maxSlots0=0;
      for(f0=r.start;f0<=r.end;f0++)maxSlots0=Math.max(maxSlots0,framesStrokes0[f0].length);
      slotsInfo.push({li:li0,ld:ld0,framesStrokes:framesStrokes0,maxSlots:maxSlots0});
    }

    // ---- Pass 1: one named, empty container Shape per (layer,slot) —
    // every frame-shape for that slot gets reparented (at creation, via
    // parentId) under it instead of sitting loose as a sibling of the
    // artboard. Reported: an export with everything flat under the
    // artboard didn't read as organized "layers with their own timeline"
    // in Rive's hierarchy panel — this groups each animated stroke's
    // flipbook shapes together the way Nemo's own layer panel already
    // groups a stroke's frames. (A real Rive "Solo" component — which
    // additionally auto-hides every child but one — could theoretically
    // read even better, but no MCP tool creates or attaches one for
    // plain shapes; a plain container plus this export's own opacity
    // keyframes achieves the same visible result.)
    var containerDefs=[];
    var containerNameByKey={};
    slotsInfo.forEach(function(si){
      for(var slot=0;slot<si.maxSlots;slot++){
        var cname=si.ld.name+' / shape'+slot+' ['+si.li+']';
        containerNameByKey[si.li+'_'+slot]=cname;
        containerDefs.push({name:cname,x:0,y:0,parentId:contentArtboardId,paints:[],paths:[]});
      }
    });
    var containerIdByKey={};
    if(containerDefs.length){
      onProgress('Création des groupes…');
      await riveCreateShapesBatch(contentArtboardId,containerDefs);
      var cTree=await riveCall('get_artboard_hierarchy',{artboardId:contentArtboardId,depth:1});
      var cIdByName={};
      (cTree.objects||[]).forEach(function(o){if(o.types&&o.types.indexOf('Shape')>=0)cIdByName[o.name]=o.id;});
      Object.keys(containerNameByKey).forEach(function(key){
        var cid=cIdByName[containerNameByKey[key]];
        if(cid)containerIdByKey[key]=cid;
      });
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
    slotsInfo.forEach(function(si){
      var li=si.li,ld=si.ld,framesStrokes=si.framesStrokes,maxSlots=si.maxSlots;
      for(var slot=0;slot<maxSlots;slot++){
        var containerId=containerIdByKey[li+'_'+slot]||contentArtboardId;
        var countRuns=riveCountRuns(framesStrokes,slot,r.start,r.end);
        countRuns.forEach(function(run){
          var firstSd=framesStrokes[run.start][slot];
          // Content geometry never bakes the camera anymore (see the
          // camActive block above this loop) — held/non-tweened frames
          // stay eligible for the cheap "static" path regardless of
          // whether the camera happens to be moving at the same time.
          var allSame=true;
          var sig0=riveSignature(firstSd);
          for(var f2=run.start+1;f2<=run.end;f2++){
            if(riveSignature(framesStrokes[f2][slot])!==sig0){allSame=false;break;}
          }
          var useMorph=(run.end>run.start)&&!allSame;
          var name='n'+(shapeCounter++);
          // A shape whose run doesn't start at r.start needs an explicit
          // opacity:0 bookend at frame 0 — confirmed live as a real bug
          // otherwise: a shape with only ONE opacity keyframe (its own
          // "turn on" at frame N, with no closing key because its run
          // extends to the end of the export) rendered visible for the
          // ENTIRE timeline, including before frame N, instead of only
          // from N onward. Two-plus keyframes on the same property
          // behaved correctly (hidden before the first one) — a single
          // keyframe apparently gets treated as a constant value across
          // the whole animation rather than "hold from here". Bookending
          // both ends whenever the run doesn't already touch them
          // sidesteps the single-keyframe case entirely.
          var opacityKeys=[];
          if(run.start>r.start)opacityKeys.push({frame:0,value:0});
          opacityKeys.push({frame:run.start-r.start,value:1});
          if(run.end<r.end)opacityKeys.push({frame:run.end-r.start+1,value:0});
          var hasRealStroke=resolveHasRealStroke(framesStrokes,slot,run.start,r.start,r.end);

          // Automatic brush-texture wobble — no user step, no per-preset
          // authoring: resolve the preset (falls through to null for
          // plain strokes, a no-op), derive amp/frequency from it, seed
          // from the stroke's own identity so it stays stable across
          // frames instead of "boiling".
          var brushKey=resolveBrushTexturePreset(framesStrokes,slot,run.start,r.start,r.end);
          var brushPreset=brushKey?resolveBrushPreset(brushKey):null;
          var wobble=null;
          // preTextureOpacity is the value the user actually authored
          // before Nemo zeroed the anchor's own opacity to hide it (see
          // riveBuildPaints) — undefined for a non-textured stroke, in
          // which case riveBuildPaints falls back to sd.opacity as usual.
          var opacityOverride=brushPreset?(firstSd.preTextureOpacity!==undefined?firstSd.preTextureOpacity:1):undefined;
          if(brushPreset){
            var seed=riveHashSeed(firstSd.strokeId||(li+'_'+slot));
            var wp=riveBrushWobbleParams(brushPreset,firstSd.strokeWidth);
            wobble={amp:wp.amp,freqPer100:wp.freqPer100,seed:seed};
          }

          if(!useMorph){
            var shapeVal0=lottieShapeValue(firstSd,null);
            if(wobble)shapeVal0.v=riveApplyWobble(shapeVal0.v,wobble.amp,wobble.freqPer100,wobble.seed,shapeVal0.c);
            // Placeholder path at creation time — real cubic vertices
            // (including a correctly-typed first one) are inserted via
            // addVertices afterward. See riveCubicVerticesFromShapeVal's
            // comment for why.
            shapeDefs.push({
              name:name,x:0,y:0,parentId:containerId,
              paints:riveBuildPaints(firstSd,hasRealStroke,opacityOverride),
              paths:[{name:'P',commands:riveBuildPlaceholderCommands(shapeVal0.v[0])}]
            });
            pending.push({name:name,opacityKeys:opacityKeys,morph:null,translate:null,closed:shapeVal0.c,cubicVerts:riveCubicVerticesFromShapeVal(shapeVal0)});
          }else{
            // Raw points per frame — checked for a pure rigid translation
            // first (see riveDetectTranslation): a stroke that's just
            // been dragged from one keyframe to another and tweened is a
            // very common case, and doesn't deserve the full per-vertex
            // cost.
            var rawPointsByFrame={};
            for(var f3=run.start;f3<=run.end;f3++){
              rawPointsByFrame[f3]=lottieShapeValue(framesStrokes[f3][slot],null).v;
            }
            var translation=riveDetectTranslation(rawPointsByFrame,run.start,run.end);

            if(translation){
              var baseValT=lottieShapeValue(firstSd,null);
              if(wobble)baseValT.v=riveApplyWobble(baseValT.v,wobble.amp,wobble.freqPer100,wobble.seed,baseValT.c);
              shapeDefs.push({
                name:name,x:0,y:0,parentId:containerId,
                paints:riveBuildPaints(firstSd,hasRealStroke,opacityOverride),
                paths:[{name:'P',commands:riveBuildStraightCommands(baseValT.v,baseValT.c)}]
              });
              pending.push({name:name,opacityKeys:opacityKeys,morph:null,translate:{start:run.start,end:run.end,deltas:translation},closed:baseValT.c});
            }else{
              var startVal=lottieShapeValue(firstSd,null);
              if(wobble)startVal.v=riveApplyWobble(startVal.v,wobble.amp,wobble.freqPer100,wobble.seed,startVal.c);
              var pointsByFrame={};
              for(var f5=run.start;f5<=run.end;f5++){
                var pts3=rawPointsByFrame[f5];
                if(wobble)pts3=riveApplyWobble(pts3,wobble.amp,wobble.freqPer100,wobble.seed,startVal.c);
                pointsByFrame[f5]=pts3;
              }
              shapeDefs.push({
                name:name,x:0,y:0,parentId:containerId,
                paints:riveBuildPaints(firstSd,hasRealStroke,opacityOverride),
                paths:[{name:'P',commands:riveBuildStraightCommands(startVal.v,startVal.c)}]
              });
              pending.push({name:name,opacityKeys:opacityKeys,morph:{start:run.start,end:run.end,pointsByFrame:pointsByFrame},translate:null,closed:startVal.c});
            }
          }
        });
      }
    });

    onProgress('Création de '+shapeDefs.length+' formes dans Rive…');
    await riveCreateShapesBatch(artboardId,shapeDefs);

    onProgress('Résolution des identifiants…');
    // depth:3 — artboard(0) -> Shape(1) -> PointsPath(2) -> vertices(3).
    // Confirmed live: depth:2 stops one level too early, the PointsPath
    // entries come back without their own `children` (vertex id) array.
    // Two separate trees when the camera is active: content lives in
    // contentArtboardId (the nested child), Background stays directly in
    // artboardId (the exported/parent one) — merge both into one lookup.
    var treeArtboardIds=(contentArtboardId!==artboardId)?[artboardId,contentArtboardId]:[artboardId];
    var objectsById={};
    var shapeByName={};
    for(var ti=0;ti<treeArtboardIds.length;ti++){
      var t=await riveCall('get_artboard_hierarchy',{artboardId:treeArtboardIds[ti],depth:3});
      (t.objects||[]).forEach(function(o){
        objectsById[o.id]=o;
        if(o.types&&o.types.indexOf('Shape')>=0)shapeByName[o.name]=o;
      });
    }
    function pathObjForShape(shapeName){
      var shape=shapeByName[shapeName];if(!shape)return null;
      return (shape.children||[]).map(function(id){return objectsById[id];}).find(function(o){return o&&o.types&&o.types.indexOf('PointsPath')>=0;})||null;
    }
    function vertexIdsForShape(shapeName){
      var pathChild=pathObjForShape(shapeName);
      return pathChild?pathChild.children:null;
    }

    // createShapes always creates a PointsPath with isclosed=true
    // regardless of whether the commands included a `close` — confirmed
    // live (an open 3-point stroke with only moveTo/lineTo came back
    // reporting isclosed:true, rendering as a closed triangle). Every
    // path this export creates needs its OWN closed-ness set explicitly
    // afterward; batched into one call since it's just a boolean flip
    // per shape, not worth chunking.
    var ISCLOSED_KEY=32;
    var closedFix={};
    pending.forEach(function(p){
      var pathObj=pathObjForShape(p.name);
      if(!pathObj)return;
      closedFix[pathObj.id]={};
      closedFix[pathObj.id][ISCLOSED_KEY]=!!p.closed;
    });
    var bgPath=pathObjForShape('Background');
    if(bgPath)closedFix[bgPath.id]={32:true};
    if(Object.keys(closedFix).length)await riveCall('set_property_values',{propertyValues:closedFix});

    // Replace each cubic-fidelity shape's placeholder point with its real
    // vertices (see riveCubicVerticesFromShapeVal for why this can't just
    // be baked into the createShapes commands above) — insert the real
    // ones after the placeholder, then delete the placeholder itself.
    var cubicPending=pending.filter(function(p){return p.cubicVerts&&pathObjForShape(p.name);});
    if(cubicPending.length){
      // get_artboard_hierarchy silently OMITS the `children` field
      // entirely for a PointsPath with exactly one vertex — confirmed
      // live: a degenerate 1-point placeholder path came back with no
      // `children` key at all (a 4-vertex Background path, fetched in
      // the very same tree, had one fine), which meant the very first
      // live version of this fixup pass ran zero times, silently, with
      // no error — pathObjForShape(name).children was always undefined
      // for every placeholder. query_objects doesn't have this gap, so
      // it's used here instead, batched across every shape needing this
      // in one call rather than resolved from the earlier tree.
      var cubicPathIds=cubicPending.map(function(p){return pathObjForShape(p.name).id;});
      var vTree=await riveCall('query_objects',{objectIds:cubicPathIds,depth:1});
      var firstChildByPathId={};
      (vTree.objects||[]).forEach(function(o){
        if(o.types&&o.types.indexOf('PointsPath')>=0&&o.children&&o.children.length)firstChildByPathId[o.id]=o.children[0];
      });
      for(var pi=0;pi<cubicPending.length;pi++){
        var pc=cubicPending[pi];
        var cPathObj=pathObjForShape(pc.name);
        var placeholderId=firstChildByPathId[cPathObj.id];
        if(!placeholderId)continue;
        var vertsToAdd=pc.cubicVerts.slice();
        vertsToAdd[0]=Object.assign({},vertsToAdd[0],{insertAfterVertexId:placeholderId});
        onProgress('Tangentes des tracés… ('+(pi+1)+'/'+cubicPending.length+')');
        await riveCall('path_editor',{command:'addVertices',data:{pathId:cPathObj.id,vertices:vertsToAdd}});
        await riveCall('delete_objects',{objectIds:[placeholderId]});
      }
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
      if(p.translate){
        // Rigid translation optimization (riveDetectTranslation) — the
        // Shape's own x/y (13/14), not the vertices: 2 keyframes per
        // frame total instead of 2*nVertices.
        for(var f6=p.translate.start;f6<=p.translate.end;f6++){
          var d=p.translate.deltas[f6];
          keyframes.push({objectId:shape.id,propertyKey:13,frame:f6-r.start,value:d[0],interpolationType:'linear'});
          keyframes.push({objectId:shape.id,propertyKey:14,frame:f6-r.start,value:d[1],interpolationType:'linear'});
        }
      }
    });
    await riveKeyframeBatch(animationId,keyframes);

    // ---- Camera: instance the content artboard once into the exported
    // artboard, keyframe the instance's own transform. Must happen AFTER
    // the content shapes/keyframes above (the content artboard has to
    // exist and be finished first) and uses the SAME animationId so the
    // camera and the content play back on one synchronized timeline. ----
    if(camActive){
      onProgress('Insertion de la caméra…');
      // Reparenting to the artboard's own root needs an explicit parentId
      // even though there's only one sensible target — confirmed live: a
      // first attempt omitting it crashed the tool outright ("Null check
      // operator used on a null value"), not a graceful validation error.
      var instRes=await riveCall('component_editor',{command:'addComponents',data:{addComponents:[{componentId:contentArtboardId,artboardId:artboardId,parentId:artboardId,x:0,y:0}]}});
      var camInstanceId=instRes.components[0].id;
      var camKeyframes=[];
      for(var cf=r.start;cf<=r.end;cf++){
        var cam=SMCamera.cameraAtFrame(cf);
        if(!cam)continue;
        var ct=riveCameraInstanceTransform(cam);
        var ckf=cf-r.start;
        camKeyframes.push({objectId:camInstanceId,propertyKey:13,frame:ckf,value:ct.x,interpolationType:'linear'});
        camKeyframes.push({objectId:camInstanceId,propertyKey:14,frame:ckf,value:ct.y,interpolationType:'linear'});
        camKeyframes.push({objectId:camInstanceId,propertyKey:16,frame:ckf,value:ct.sx,interpolationType:'linear'});
        camKeyframes.push({objectId:camInstanceId,propertyKey:17,frame:ckf,value:ct.sy,interpolationType:'linear'});
        camKeyframes.push({objectId:camInstanceId,propertyKey:15,frame:ckf,value:ct.r,interpolationType:'linear'});
      }
      await riveKeyframeBatch(animationId,camKeyframes);
      keyframes=keyframes.concat(camKeyframes);
    }

    onProgress('Terminé.');
    return{ok:true,artboardId:artboardId,artboardName:artboardName,animationId:animationId,shapeCount:shapeDefs.length,keyframeCount:keyframes.length,cameraNested:camActive};
  }

  window.SMExport=window.SMExport||{};
  window.SMExport.exportRive=exportRive;
})();
