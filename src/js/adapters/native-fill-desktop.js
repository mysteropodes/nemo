/* Ordinary Fill pointer intent -> the existing serialized native command session.
 * UI reads only; native owns tracing, provenance, geometry, history and persistence. */
(function(root,factory){
  const common=typeof module==='object'&&module.exports;
  const command=common?require('./native-object-vector-command.js'):root.NemoNativeObjectVectorCommand;
  const contract=common?require('./native-object-edit-contract.js'):root.NemoNativeObjectEditContract;
  const api=factory(command,contract);
  if(common)module.exports=api;else root.NemoNativeFillDesktop=api;
}(typeof globalThis==='object'?globalThis:this,function(command,contract){
  'use strict';
  const OP='command.document.object.fill.trace',BRUSH='command.document.object.fill.brush';
  const PIN=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  function hex(value){
    if(typeof value!=='string')throw new TypeError('Fill color is unavailable');
    if(/^#[0-9a-f]{3}$/i.test(value))value='#'+value.slice(1).split('').map((c)=>c+c).join('');
    if(/^#[0-9a-f]{6}$/i.test(value))value+='ff';
    if(!/^#[0-9a-f]{8}$/i.test(value))throw new TypeError('Fill color is unavailable');
    return value;
  }
  function create(ports){
    ['current','ready','target'].forEach((key)=>{if(!ports||typeof ports[key]!=='function')throw new TypeError('Fill requires '+key);});
    async function fill(input){
      const session=ports.current(),before=session&&session.info();
      if(!session||!session.isActive()||!before||!ports.ready(session,before))throw command.fault('unavailable','Native fill is unavailable');
      const target=ports.target(before);
      if(!target||typeof target.layerUid!=='string')throw command.fault('unavailable','Native fill target is unavailable');
      const captured=Object.assign({},target),pin=Object.fromEntries(PIN.map((key)=>[key,before[key]]));
      const payload={command:'object.fill.trace',contextId:target.contextId||'scene-root',layerUid:target.layerUid,
        frame:target.frame,point:input.point.slice(),color:hex(input.color),opacity:input.opacity,remove:input.remove===true};
      contract.payload(OP,payload);
      const runner=command.create(session,'fill',contract);
      return runner.execute(OP,(current)=>{
        const now=ports.target(current);
        if(ports.current()!==session||!ports.ready(session,current)||!PIN.every((key)=>pin[key]===current[key])
          ||!now||now.layerUid!==captured.layerUid||now.frame!==captured.frame||now.contextId!==captured.contextId)
          throw command.fault('stale_owner','Fill target changed before dispatch');
        return payload;
      });
    }
    // The Fill Brush commit (#2361): the drawn outline placed by fillBrushMode, then the same-colour merge, natively.
    async function brush(input){
      const session=ports.current(),before=session&&session.info();
      if(!session||!session.isActive()||!before||!ports.ready(session,before))throw command.fault('unavailable','Native Fill Brush is unavailable');
      const target=ports.target(before);
      if(!target||typeof target.layerUid!=='string')throw command.fault('unavailable','Native Fill Brush target is unavailable');
      const pin=Object.fromEntries(PIN.map((key)=>[key,before[key]]));
      const payload={command:'object.fill.brush',contextId:target.contextId||'scene-root',layerUid:target.layerUid,frame:target.frame,
        outline:input.outline.map((s)=>({point:s.point.slice(),handleIn:s.handleIn.slice(),handleOut:s.handleOut.slice()})),
        color:hex(input.color),opacity:input.opacity,placement:input.placement};
      contract.payload(BRUSH,payload);
      return command.create(session,'fill',contract).execute(BRUSH,(current)=>{
        if(ports.current()!==session||!ports.ready(session,current)||!PIN.every((key)=>pin[key]===current[key]))
          throw command.fault('stale_owner','Fill Brush target changed before dispatch');
        return payload;
      });
    }
    return Object.freeze({fill,brush});
  }
  function current(root){return root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root);}
  function nativeMode(root){
    try{
      const c=root.NemoNativeOpacityCutover;if(c&&typeof c.blocksLegacy==='function'&&c.blocksLegacy())return true;
      const s=current(root);return !!(s&&(s.isActive()||typeof s.blocksLegacy==='function'&&s.blocksLegacy()));
    }catch(_){return true;}
  }
  function handlePointerDown(root,event,ports){
    if(!nativeMode(root))return false;
    const state=root.state;
    if(!state||state.tool!=='fill'||state.playing)return false;
    event.stopImmediatePropagation();event.preventDefault();
    const toast=(text)=>{if(typeof root.showToast==='function')root.showToast(text);};
    const t=(key)=>root.SM&&typeof root.SM.t==='function'?root.SM.t(key):key;
    if(event.altKey){toast('Close Gap is unavailable.');return true;}
    const Mapping=ports&&ports.mapping;
    if(!Mapping||['bounds','point','matchesView'].some(key=>typeof Mapping[key]!=='function')){toast('Native fill coordinates are unavailable');return true;}
    const owner=current(root),shown=owner&&owner.readPresentedObjectSession&&owner.readPresentedObjectSession(),info=owner&&owner.info();
    const rect=info&&Mapping.bounds(root,info);
    if(!shown||!shown.presentation||!rect||!PIN.every(key=>shown[key]===info[key])||shown.presentation.frame!==state.currentFrame||!Mapping.matchesView(root,shown.presentation)){toast('Native fill coordinates are unavailable');return true;}
    const captured=shown.presentation;let actionReady=null;
    const guarded={info:owner.info,isActive:owner.isActive,requestId:owner.requestId,runResourceCommand:action=>owner.runResourceCommand(ctx=>{
      const exact=()=>{const p=ctx.readPresentedObjectSession&&ctx.readPresentedObjectSession();return p&&p.presentation===captured&&(captured.view?captured.view.mode:'canvas')===(state.cameraView===true?'stored-camera':'canvas')&&state.currentFrame===captured.frame&&JSON.stringify(Mapping.bounds(root,owner.info()))===JSON.stringify(rect);};
      if(!exact())throw command.fault('stale_owner','Fill presentation changed');
      actionReady=exact;try{return action({reconcile:ctx.reconcile,dispatch:q=>{if(!exact())throw command.fault('stale_owner','Fill presentation changed');return ctx.dispatch(q);}});}finally{actionReady=null;}
    })};
    const editor=create({current:()=>current(root)===owner?guarded:current(root),ready:(session,info)=>session===guarded&&current(root)===owner&&(actionReady?actionReady():Mapping.matchesView(root,captured))
      &&root.NemoNativeObjectProjectFile&&root.NemoNativeObjectProjectFile.projectionReadyForUI(root,info)===true,
      target:(info)=>{
        const row=info.layers&&info.layers[state.activeLayerIdx];
        return row&&!row.locked?{layerUid:row.layerUid,contextId:'scene-root',frame:state.currentFrame}:null;
      }});
    try{
      const mapped=Mapping.point(event,{bounds:rect,canvasW:info.canvasW,canvasH:info.canvasH,presentation:captured});
      const point=[mapped.x,mapped.y];
      editor.fill({point,color:state.fillColor,opacity:state.opacity/100,remove:event.shiftKey===true})
        .then((r)=>{if(r.applied)toast(t(event.shiftKey?'toastFillRemoved':'toastFillApplied'));})
        .catch((error)=>toast(error&&error.code==='stale_owner'?'Fill target changed. Try again.':'Fill is unavailable.'));
    }catch(_){toast('Fill is unavailable.');}
    return true;
  }
  return Object.freeze({create,handlePointerDown,hex});
}));
