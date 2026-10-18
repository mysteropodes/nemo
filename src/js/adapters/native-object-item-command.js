/* One bounded command round trip for the lane-6 adapters: validate before dispatch,
 * send through the serialized native session, replay an ambiguous transport failure
 * with the SAME request id (the owner returns the stored receipt or executes once),
 * correlate the receipt, then reconcile. Holds no document state of any kind. */
(function(root,factory){
  var contract=typeof module==='object'&&module.exports?require('./native-object-item-contract.js'):root.NemoNativeObjectItemContract;
  var api=factory(contract);
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectItemCommand=api;
}(typeof globalThis==='object'?globalThis:this,function(contract){
  'use strict';
  function fault(code,message,extra){var e=new Error(message);e.code=code;return Object.assign(e,extra||{});}
  function ownerKey(pin){return JSON.stringify([pin.instanceId,pin.documentId,pin.lifecycleGeneration]);}
  function size(request){return new TextEncoder().encode(JSON.stringify(request)).length;}
  function create(session,label){
    if(!contract)throw new Error('Native object item contract is unavailable');
    function owner(){
      var pin=session.info();
      if(!session.isActive()||!pin)throw fault('unavailable','Native project is unavailable',{reason:'no_native_project'});
      return pin;
    }
    function refusal(response,request){
      var error=response.error||{};
      return fault(typeof error.code==='string'?error.code:'unavailable','Native '+label+' command refused: '+(error.code||'unavailable'),
        {reason:error.details&&error.details.reason||null,requestId:request.requestId,contentRevision:response.contentRevision,details:error.details||null});
    }
    async function send(ctx,request,key,finish,sent){
      try{return await ctx.dispatch(request);}catch(first){
        // Ambiguous transport outcome. The owner keys its receipt by request id and body: a second send returns the
        // stored response if the first executed, and executes exactly once if it did not.
        sent.second=true;
        try{return await ctx.dispatch(request);}catch(second){
          var settled=true,after=null;
          try{await ctx.reconcile();}catch(_){settled=false;}
          if(settled)after=session.info();
          // The document did not advance past the revision this request was sent against: nothing was published.
          if(after&&ownerKey(after)===key&&after.contentRevision===request.expectedRevision)
            throw fault('unavailable','Native '+label+' command was not delivered',{reason:'transport_failure',requestId:request.requestId});
          throw fault(label+'_command_indeterminate','Native '+label+' command outcome is indeterminate',
            {requestId:request.requestId,replay:function(){return replay(request,key,finish);}});
        }
      }
    }
    // Reconcile the session. A replay answers the STORED receipt, whose revision is the one the original execution published; any
    // edit published since moves the session past it, so a replay reconciles WITHOUT the revision equality (late) and never
    // wedges the session on a legitimate stored answer. A failure of the session itself is surfaced typed.
    async function reconcile(ctx,request,revision,late){
      try{return await (late||revision===undefined?ctx.reconcile():ctx.reconcile(revision));}catch(error){
        if(error&&typeof error.code==='string')throw error;
        throw fault('unavailable','Native '+label+' session could not be reconciled: '+(error&&error.message||error),
          {reason:'reconcile_failed',requestId:request.requestId});
      }
    }
    async function settle(ctx,request,key,finish,late){
      // An answer to the SECOND send is the owner's stored receipt when the first send executed: it may be older than the document, so it is reconciled late too.
      var sent={second:false},response=await send(ctx,request,key,finish,sent);
      late=late||sent.second;
      if(!response||response.apiVersion!==2||response.requestId!==request.requestId||typeof response.ok!=='boolean')
        throw fault('uncorrelated','Native '+label+' response is uncorrelated',{requestId:request.requestId});
      if(!response.ok){await reconcile(ctx,request,response.contentRevision,late);throw refusal(response,request);}
      try{contract.result(request.operation,response.result);contract.correlate(request,response);}catch(bad){
        try{await ctx.reconcile();}catch(_){}
        throw fault('uncorrelated','Native '+label+' receipt is uncorrelated: '+bad.message,{requestId:request.requestId});
      }
      await reconcile(ctx,request,response.contentRevision,late);
      var receipt=Object.freeze(Object.assign({requestId:request.requestId,contentRevision:response.contentRevision},response.result));
      return finish?finish(receipt):receipt;
    }
    function run(action){
      if(typeof session.runResourceCommand!=='function')
        return Promise.reject(fault('unavailable','Native resource command route is unavailable',{reason:'resource_route_unavailable'}));
      var started=false;
      try{owner();}catch(error){return Promise.reject(error);}
      // A rejection before the action ran is the shared route's own refusal (opening, synchronizing, inactive, epoch change): typed busy.
      var queued;
      try{queued=session.runResourceCommand(function(ctx){started=true;return action(ctx);});}catch(error){return Promise.reject(error);}
      return Promise.resolve(queued).catch(function(error){
        if(started||(error&&typeof error.code==='string'))throw error;
        throw fault('unavailable','Native '+label+' command route is busy or unavailable: '+(error&&error.message||error),{reason:'native_project_busy'});
      });
    }
    // Re-sends the exact request of an indeterminate outcome, and only to the same native project.
    function replay(request,key,finish){
      return run(function(ctx){
        if(ownerKey(owner())!==key)throw fault('stale_owner','Native project changed before the replay',{requestId:request.requestId});
        return settle(ctx,request,key,finish,true);
      });
    }
    // build(pin) runs at execution, so a queued call reads the current revision and snapshot, never values captured at click time.
    function execute(operation,build,finish){
      return run(function(ctx){
        var pin=owner(),key=ownerKey(pin);
        var request={apiVersion:2,requestId:session.requestId(label),instanceId:pin.instanceId,documentId:pin.documentId,
          expectedRevision:pin.contentRevision,operation:operation,payload:build(pin,ctx)};
        try{contract.payload(operation,request.payload);}catch(bad){throw fault('invalid_request',bad.message,{reason:'local_validation'});}
        if(size(request)>4096)throw fault('invalid_request','Native '+label+' request exceeds 4096 encoded bytes',{reason:'request_size'});
        return settle(ctx,request,key,finish);
      });
    }
    return Object.freeze({execute:execute});
  }
  return Object.freeze({create:create});
}));
