/* Private property ports. Project retains the session, queue and native authority. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProjectProperties=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  // Preserve submitted JSON types and members rather than normalizing malformed
  // undefined/nonfinite inputs into an otherwise admissible request.
  function snapshot(value,seen){
    if(value===null||typeof value==='string'||typeof value==='boolean')return value;
    if(typeof value==='number'&&Number.isFinite(value))return value;
    if(!value||typeof value!=='object')throw new TypeError('Property intent must contain JSON data');
    seen=seen||new Set();if(seen.has(value))throw new TypeError('Cyclic property intent');seen.add(value);
    var array=Array.isArray(value),proto=Object.getPrototypeOf(value);
    if(!array&&proto!==Object.prototype&&proto!==null||Object.getOwnPropertySymbols(value).length)throw new TypeError('Property intent must contain plain JSON data');
    var result=array?[]:{},keys=Object.keys(value);
    if(array&&(keys.length!==value.length||keys.some(function(key,i){return key!==String(i);})))throw new TypeError('Property intent array must be dense');
    keys.forEach(function(key){var descriptor=Object.getOwnPropertyDescriptor(value,key);
      if(!descriptor||!Object.prototype.hasOwnProperty.call(descriptor,'value'))throw new TypeError('Property intent cannot contain accessors');
      Object.defineProperty(result,key,{value:snapshot(descriptor.value,seen),enumerable:true,writable:true,configurable:true});
    });seen.delete(value);return result;
  }
  function create(ports){
    function ready(state){return !!state&&state.active&&!state.opening&&state.pending===0&&!state.synchronizing&&state.subscribed&&
      !!state.presented&&state.sceneRoot!==false&&state.dispatchAvailable&&state.projectionReady&&state.subscriptionActive;}
    function available(){return ready(ports.state());}
    async function read(selector,kind){
      var fields=['instanceId','documentId','atRevision','documentSnapshotId','view'];
      if(!selector||typeof selector!=='object'||Array.isArray(selector)||
        Object.keys(selector).length!==fields.length||!fields.every(function(k){return Object.prototype.hasOwnProperty.call(selector,k);}))
        throw new TypeError('Invalid native metadata selector');
      var state=ports.state(),pin=state.info,ticket=state.ticket,shown=state.presentation;
      function current(){var next=ports.state();return ticket===next.ticket&&shown===next.presentation&&ports.same(next.info,pin)&&ready(next);}
      if(!current()||selector.instanceId!==pin.instanceId||selector.documentId!==pin.documentId||
        selector.atRevision!==pin.contentRevision||selector.documentSnapshotId!==pin.documentSnapshotId)
        throw new Error('Native metadata owner or presentation is unavailable');
      var request={apiVersion:2,requestId:ports.requestId(kind),instanceId:selector.instanceId,documentId:selector.documentId,
        operation:kind==='metadata'?'query.document.object.property.metadata':'query.document.object.property.read',payload:{atRevision:selector.atRevision,
          documentSnapshotId:selector.documentSnapshotId,view:selector.view}};
      // The same subscribed desktop port calls the real SDK before transport.
      var response=await ports.dispatchApplication(request);
      if(!current())throw new Error('Native metadata owner or presentation changed');
      if(!response||response.apiVersion!==2||response.requestId!==request.requestId||response.instanceId!==pin.instanceId||
        response.documentId!==pin.documentId||response.contentRevision!==pin.contentRevision)
        throw new Error('Native metadata response is uncorrelated');
      if(response.ok!==true){var error=new Error('Native metadata read unavailable');
        error.code=response.error&&response.error.code||'unavailable';throw error;}
      var result=response.result;
      if(!result||result.instanceId!==pin.instanceId||result.documentId!==pin.documentId||
        result.atRevision!==pin.contentRevision||result.documentSnapshotId!==pin.documentSnapshotId)
        throw new Error('Native metadata result is uncorrelated');
      return ports.freeze(result);
    }
    function edit(payload,expectedPin,isCurrent,track){
      var copied,pin;
      try{
        if(typeof isCurrent!=='function'||isCurrent()!==true||!available())throw new Error('Native property edit owner or intent is unavailable');
        copied=ports.freeze(snapshot(payload));pin=ports.freeze(snapshot(expectedPin));
      }catch(error){return Promise.reject(error);}
      return ports.mutation(function(current){
        if(!ports.same(current,pin)||isCurrent()!==true)throw new Error('Native property edit intent is stale');
        var state=ports.state();
        // pending includes this queued action. The ordinary mutation path owns
        // its eventual inspect/publication/presentation, without a second queue.
        if(!state.active||state.opening||state.synchronizing||!state.subscribed||!state.presentationCurrent||
          !state.dispatchAvailable||!state.projectionReady||!state.subscriptionActive||state.sceneRoot===false)
          throw new Error('Native property edit owner or presentation is unavailable');
        return {apiVersion:2,requestId:ports.requestId(track?'property-track':'property'),instanceId:current.instanceId,documentId:current.documentId,
          operation:track?'command.document.object.property.track.edit':'command.document.object.property.edit',expectedRevision:current.contentRevision,payload:copied};
      },'nemo_native_dispatch');
    }
    return Object.freeze({available:available,readPropertyMetadata:function(selector){return read(selector,'metadata');},
      readProperty:function(selector){return read(selector,'property-read');},editProperty:function(payload,pin,current){return edit(payload,pin,current,false);},editPropertyTrack:function(payload,pin,current){return edit(payload,pin,current,true);}});
  }
  return Object.freeze({create:create});
}));
