/* Snapshot-pinned text facts through the existing serialized Project resource route.
 * Results are detached read values; this module keeps no document or target cache. */
(function(root,factory){var contract=typeof module==='object'&&module.exports?require('./native-text-read-contract.js'):root.NemoNativeTextReadContract;
  var api=factory(contract);if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeTextRead=api;
}(typeof globalThis==='object'?globalThis:this,function(Contract){
  'use strict';
  function fault(code,message){var error=new Error(message);error.code=code;return error;}
  function clone(v){return JSON.parse(JSON.stringify(v));}
  function pinOf(session){var p=session&&session.info();if(!session||!session.isActive()||!p)throw fault('unavailable','Native text owner unavailable');return p;}
  function samePin(a,b){return a&&b&&['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'].every(function(k){return a[k]===b[k];});}
  function freeze(v){if(v&&typeof v==='object'){Object.keys(v).forEach(function(k){freeze(v[k]);});Object.freeze(v);}return v;}
  function create(session){
    function run(expected,action){
      var pin;try{pin=clone(expected||pinOf(session));if(!samePin(pin,pinOf(session)))throw fault('stale_revision','Native text view is stale');
        if(typeof session.runResourceCommand!=='function')throw fault('unavailable','Native text resource route unavailable');
      }catch(error){return Promise.reject(error);}
      function guard(){if(!samePin(pin,pinOf(session)))throw fault('stale_revision','Native text owner or snapshot changed');}
      return Promise.resolve().then(function(){return session.runResourceCommand(async function(ctx){guard();
        async function read(view){guard();var request={apiVersion:2,requestId:session.requestId('text-read'),instanceId:pin.instanceId,documentId:pin.documentId,
          operation:Contract.OP,payload:{atRevision:pin.contentRevision,documentSnapshotId:pin.documentSnapshotId,view:view}};
          Contract.payload(Contract.OP,request.payload);var response=await ctx.dispatch(request);guard();
          if(!response||response.apiVersion!==2||response.requestId!==request.requestId||typeof response.ok!=='boolean')throw fault('uncorrelated','Native text response is uncorrelated');
          Contract.correlate(request,response);
          if(!response.ok)throw fault(response.error&&response.error.code||'unavailable','Native text read refused');
          Contract.result(Contract.OP,response.result);return freeze(clone(response.result));
        }
        return action(read,pin);
      });});
    }
    return Object.freeze({
      listPage:function(scope,offset,limit,expected){var view=clone({kind:'targets',scope:scope,offset:offset,limit:limit});return run(expected,function(read){return read(view);});},
      readBlock:function(target,expected){target=clone(target);return run(expected,async function(read,pin){
        var meta=await read({kind:'inspect',target:target}),offset=0,parts=[];
        do{var chunk=await read({kind:'text',target:target,offset:offset,limit:1024});
          if(chunk.textBytes!==meta.textBytes)throw fault('uncorrelated','Native text length changed inside a snapshot');
          parts.push(chunk.text);offset=chunk.nextOffset;
        }while(offset!==null);
        var text=parts.join('');if(new TextEncoder().encode(text).length!==meta.textBytes)throw fault('uncorrelated','Native text is incomplete');
        return freeze({pin:clone(pin),target:clone(target),block:Object.assign({},meta.block,{text:text})});
      });},
    });
  }
  return Object.freeze({create:create,samePin:samePin});
}));
