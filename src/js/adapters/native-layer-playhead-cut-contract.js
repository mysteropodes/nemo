/* F11 shared cut contract. Pure validation; no document, selection or history mirror. */
(function(root,factory){const api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeLayerPlayheadCutContract=api;}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  const OP='command.document.object.layer.cut';
  const IDENTIFIER=/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  const has=(value,key)=>Object.prototype.hasOwnProperty.call(value,key);
  const bytes=value=>typeof TextEncoder==='function'?new TextEncoder().encode(value).length:unescape(encodeURIComponent(value)).length;
  const fail=message=>{throw new TypeError(message);};
  function exact(value,required){
    if(!value||typeof value!=='object'||Array.isArray(value)||![Object.prototype,null].includes(Object.getPrototypeOf(value)))fail('cut value must be a plain object');
    if(Object.keys(value).length!==required.length||required.some(key=>!has(value,key)))fail('invalid cut fields');
    return value;
  }
  function authoredId(value){if(typeof value!=='string'||!value.length||bytes(value)>128)fail('invalid cut source identity');}
  function payload(operation,value){
    if(operation!==OP)fail('cut operation is not declared');
    exact(value,['command','contextId','layerUid','frame']);
    if(value.command!=='object.layer.cut')fail('invalid cut command');
    authoredId(value.contextId);authoredId(value.layerUid);
    if(!Number.isInteger(value.frame)||value.frame<0||value.frame>4294967295)fail('invalid cut frame');
  }
  function result(operation,value){
    if(operation!==OP)fail('cut operation is not declared');
    if(value&&value.applied===true){
      exact(value,['applied','historyEntriesAdded','layerUid']);
      if(value.historyEntriesAdded!==1||typeof value.layerUid!=='string'||!IDENTIFIER.test(value.layerUid))fail('invalid changed cut result');
    }else if(value&&value.applied===false){
      exact(value,['applied','historyEntriesAdded']);
      if(value.historyEntriesAdded!==0)fail('invalid no-op cut result');
    }else fail('invalid cut disposition');
  }
  function correlate(request,response){
    if(response.requestId!==request.requestId||response.instanceId!==request.instanceId)fail('cut response owner mismatch');
    const replaced=!response.ok&&response.error&&response.error.code==='wrong_document';
    if(response.documentId!==request.documentId&&!replaced)fail('cut response document mismatch');
    if(response.ok){
      result(OP,response.result);
      if(!Number.isSafeInteger(request.expectedRevision)||request.expectedRevision<0||!Number.isSafeInteger(response.contentRevision)||
          response.contentRevision!==request.expectedRevision+(response.result.applied?1:0))fail('cut revision mismatch');
    }
  }
  const OPAQUE=Object.freeze({has:at=>/^payload\.(contextId|layerUid)$/.test(at)});
  const admitted=operation=>operation===OP;
  const own=Object.freeze({admitted,requiresRevision:admitted,payload,result,correlate,paths:operation=>admitted(operation)?OPAQUE:null});
  function compose(base){
    if(!base)return own;
    return Object.freeze(Object.assign({},base,{
      admitted:operation=>admitted(operation)||base.admitted(operation),
      requiresRevision:operation=>admitted(operation)||base.requiresRevision(operation),
      payload:(operation,value,envelope)=>admitted(operation)?payload(operation,value):base.payload(operation,value,envelope),
      result:(operation,value)=>admitted(operation)?result(operation,value):base.result(operation,value),
      correlate:(request,response)=>admitted(request.operation)?correlate(request,response):base.correlate(request,response),
      paths:operation=>admitted(operation)?OPAQUE:base.paths(operation),
    }));
  }
  return Object.freeze({OP,admitted,requiresRevision:admitted,payload,result,correlate,paths:own.paths,compose});
}));
