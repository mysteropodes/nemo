/* Pure original placement DTO/receipt contract; no document mirror, writer or host. */
(function(root,factory){const api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeProjectPlacementContract=api;}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  const OP='command.document.object.project.placement.edit';
  const BLANK=/^[\u0009-\u000D\u0020\u0085\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000]*$/;
  const bytes=s=>typeof TextEncoder==='function'?new TextEncoder().encode(s).length:unescape(encodeURIComponent(s)).length;
  const fail=message=>{throw new TypeError(message);};
  const has=(v,k)=>Object.prototype.hasOwnProperty.call(v,k);
  function exact(v,required,optional=[]){
    if(!v||typeof v!=='object'||Array.isArray(v)||![Object.prototype,null].includes(Object.getPrototypeOf(v)))fail('placement must be a plain object');
    if(required.some(k=>!has(v,k))||Object.keys(v).some(k=>!required.includes(k)&&!optional.includes(k)))fail('invalid placement fields');
    return v;
  }
  function sourceId(v){if(typeof v!=='string'||BLANK.test(v)||bytes(v)>255||v.includes('\0'))fail('invalid placement source id');}
  function offset(v,whole=false){if(typeof v!=='number'||!Number.isFinite(v)||Math.abs(v)>Number.MAX_SAFE_INTEGER||whole&&!Number.isSafeInteger(v))fail('invalid placement offset');}
  function target(v){
    if(v&&v.kind==='drawing'){
      exact(v,['kind','contextId','layerUid'],['strokeId']);sourceId(v.contextId);sourceId(v.layerUid);
      if(has(v,'strokeId'))sourceId(v.strokeId);
    }else if(v&&v.kind==='audioTrack'){exact(v,['kind','trackUid']);sourceId(v.trackUid);}
    else fail('invalid placement target');
  }
  // T12 video timing: bounded remap intents (baseline enable/disable/remove key) and the footage mode selector; the native owner admits the values.
  const finite=n=>typeof n==='number'&&Number.isFinite(n);
  const frames=(v,names)=>{for(const n of names)if(!finite(v[n]))fail('invalid time remap frame');};
  function curvePoints(c){if(!Array.isArray(c)||!c.every(p=>{exact(p,['x','y'],['tx','ty']);return ['x','y','tx','ty'].every(k=>!has(p,k)||finite(p[k]));}))fail('invalid time remap curve');}
  function interpMode(m){if(typeof m!=='string'||!m||bytes(m)>32||m.includes('\0'))fail('invalid interpolation mode');}
  function payload(op,v){
    if(op!==OP)fail('placement operation is not declared');
    if(v&&v.kind==='place'){
      exact(v,['kind','placement']);const p=exact(v.placement,['id','assetId','target','offsetFrames']);
      sourceId(p.id);sourceId(p.assetId);target(p.target);offset(p.offsetFrames,p.target.kind!=='audioTrack');
    }else if(v&&['setOffset','shiftOffset'].includes(v.kind)){
      const field=v.kind==='setOffset'?'offsetFrames':'deltaFrames';exact(v,['kind','placementId',field]);sourceId(v.placementId);offset(v[field]);
    }else if(v&&v.kind==='remove'){exact(v,['kind','placementId']);sourceId(v.placementId);}
    else if(v&&v.kind==='enableTimeRemap'){exact(v,['kind','placementId','inFrame','outFrame','lastSourceFrame']);sourceId(v.placementId);frames(v,['inFrame','outFrame','lastSourceFrame']);}
    else if(v&&v.kind==='removeTimeRemapKey'){exact(v,['kind','placementId','frame']);sourceId(v.placementId);frames(v,['frame']);}
    else if(v&&v.kind==='disableTimeRemap'){exact(v,['kind','placementId']);sourceId(v.placementId);}
    else if(v&&v.kind==='setTimeRemapKey'){exact(v,['kind','placementId','frame','value'],['curvePoints']);sourceId(v.placementId);frames(v,['frame','value']);if(has(v,'curvePoints'))curvePoints(v.curvePoints);}
    else if(v&&v.kind==='setInterpMode'){exact(v,['kind','placementId','interpMode']);sourceId(v.placementId);interpMode(v.interpMode);}
    else fail('invalid placement edit');
  }
  function result(op,v){
    if(op!==OP)fail('placement operation is not declared');
    exact(v,['applied','historyEntriesAdded']);
    if(typeof v.applied!=='boolean'||v.historyEntriesAdded!==(v.applied?1:0))fail('invalid placement disposition');
  }
  function correlate(request,response){
    if(response.instanceId!==request.instanceId)fail('placement response owner mismatch');
    const replaced=!response.ok&&response.error&&response.error.code==='wrong_document';
    if(response.documentId!==request.documentId&&!replaced)fail('placement response document mismatch');
    if(response.ok){
      result(OP,response.result);
      if(response.contentRevision!==request.expectedRevision+(response.result.applied?1:0))fail('placement revision mismatch');
    }
  }
  const OPAQUE=Object.freeze({has:at=>/^payload\.(placementId|placement\.(id|assetId|target\.(contextId|layerUid|strokeId|trackUid)))$/.test(at)});
  const admitted=op=>op===OP;
  const own=Object.freeze({admitted,requiresRevision:admitted,payload,result,correlate,paths:op=>admitted(op)?OPAQUE:null});
  function compose(base){
    if(!base)return own;
    return Object.freeze(Object.assign({},base,{
      admitted:op=>admitted(op)||base.admitted(op),
      requiresRevision:op=>admitted(op)||base.requiresRevision(op),
      payload:(op,v,envelope)=>admitted(op)?payload(op,v):base.payload(op,v,envelope),
      result:(op,v)=>admitted(op)?result(op,v):base.result(op,v),
      correlate:(request,response)=>admitted(request.operation)?correlate(request,response):base.correlate(request,response),
      paths:op=>admitted(op)?OPAQUE:base.paths(op),
    }));
  }
  return Object.freeze({OP,admitted,requiresRevision:admitted,payload,result,correlate,paths:own.paths,compose});
}));
