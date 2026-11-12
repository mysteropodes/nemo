/* Derived composition projection only. Project supplies the existing queue and native SDK port. */
(function(root,factory){var api=factory(typeof module==='object'&&module.exports?require('./native-composition-contract.js'):root.NemoNativeCompositionContract);
  if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeCompositionDesktop=api;
}(typeof globalThis==='object'?globalThis:this,function(Contract){
  'use strict';
  var pinKeys=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  function same(a,b){return !!a&&!!b&&pinKeys.every(function(k){return a[k]===b[k];});}
  function freeze(value){if(value&&typeof value==='object'){Object.keys(value).forEach(function(k){freeze(value[k]);});Object.freeze(value);}return value;}
  function fault(code){var e=new Error('Native composition is unavailable or unconfirmed');e.code=code||'unavailable';return e;}
  function presented(value,info){return value&&same(value,info)&&same(value.presentation,info)&&
    value.presentation.owner==='native'&&value.presentation.status==='presented';}
  function create(ports){
    var generation=0,retired=false,cache=null;
    function invalidate(){generation++;cache=null;}
    function capture(){
      var session=ports.currentSession(),info=session&&session.info();
      if(retired||!session||!session.isActive()||!info||typeof session.runResourceCommand!=='function'||
        typeof session.readSubscriptionGeneration!=='function'||!presented(session.readPresentedObjectSession(),info)||
        ports.available(session,info)!==true)throw fault();
      var pin={};pinKeys.forEach(function(k){pin[k]=info[k];});
      return {session:session,info:Object.freeze(pin),ticket:session.readSubscriptionGeneration(),generation:generation};
    }
    function current(stamp,failed){
      function check(value,code){if(!value&&failed)failed(code);return value;}
      return check(!retired,'retired')&&check(stamp.generation===generation,'generation-stale')&&
        check(ports.currentSession()===stamp.session,'session-replaced')&&check(stamp.session.isActive(),'inactive')&&
        check(same(stamp.session.info(),stamp.info),'five-pin-stale')&&
        check(stamp.session.readSubscriptionGeneration()===stamp.ticket,'subscription-stale');
    }
    function guard(stamp){if(!current(stamp))throw fault('stale_revision');}
    function actionReady(stamp,ctx){guard(stamp);if(!ctx||!presented(ctx.readPresentedObjectSession(),stamp.info))throw fault();}
    // Identity stamps remain narrow; readiness receives the same-pin full native metadata.
    function available(stamp,failed){
      var info=stamp.session.info();
      if(!same(info,stamp.info)){if(failed)failed('five-pin-stale');return false;}
      var value=ports.available(stamp.session,info)===true;
      if(!value&&failed)failed('projection-unready');return value;
    }
    function response(request,value){
      if(!value||value.apiVersion!==2||value.requestId!==request.requestId||typeof value.ok!=='boolean')throw fault();
      Contract.correlate(request,value);
      if(value.ok!==true)throw fault(value.error&&value.error.code);
      return value;
    }
    async function query(stamp,ctx,selector){
      actionReady(stamp,ctx);var pin=stamp.info;
      var request={apiVersion:2,requestId:stamp.session.requestId('composition-read'),instanceId:pin.instanceId,documentId:pin.documentId,
        operation:Contract.READ,payload:{atRevision:pin.contentRevision,documentSnapshotId:pin.documentSnapshotId,contextId:'scene-root',selector:selector}};
      Contract.payload(request.operation,request.payload);var value=response(request,await ctx.dispatch(request));
      guard(stamp);return value.result.result;
    }
    async function refresh(expected){
      cache=null;var stamp=capture();if(expected&&!same(expected,stamp.info))throw fault('stale_revision');
      var data=await (stamp.session.runResourceQuery || stamp.session.runResourceCommand).call(stamp.session, async function(ctx){
        var settings=await query(stamp,ctx,{kind:'settings'}),items=[],cursor=0;
        do {
          var page=await query(stamp,ctx,{kind:'markerPage',cursor:cursor,limit:16});
          if(page.markerCount!==settings.markerCount||page.markersPresent!==settings.markersPresent)throw fault();
          page.items.forEach(function(item){items.push(Object.assign({},item));});cursor=page.nextCursor;
        }while(cursor!==null);
        if(items.length!==settings.markerCount)throw fault();
        return freeze(Object.assign({},settings,{markers:items}));
      });
      guard(stamp);if(!presented(stamp.session.readPresentedObjectSession(),stamp.info)||!available(stamp))throw fault();
      var projection=freeze({info:stamp.info,composition:data});
      if(typeof ports.publish==='function'&&ports.publish(projection)!==true)throw fault();
      guard(stamp);cache={stamp:stamp,value:projection};return projection;
    }
    async function edit(value,expected){
      var intent=Contract.action(value);invalidate();var stamp=capture(),receipt=null,after=null;
      if(expected&&!same(expected,stamp.info))throw fault('stale_revision');
      try {
        await stamp.session.runResourceCommand(async function(ctx){
          actionReady(stamp,ctx);var pin=stamp.info;
          var request={apiVersion:2,requestId:stamp.session.requestId('composition-edit'),instanceId:pin.instanceId,documentId:pin.documentId,
            operation:Contract.EDIT,expectedRevision:pin.contentRevision,payload:{command:'object.project.composition.edit',action:intent}};
          Contract.payload(request.operation,request.payload);receipt=response(request,await ctx.dispatch(request));
          guard(stamp);
          // A native edit receipt does NOT advance Session.info. Inspect/reconcile is required.
          after=await ctx.reconcile(receipt.contentRevision);
          if(!after||after.instanceId!==pin.instanceId||after.documentId!==pin.documentId||after.lifecycleGeneration!==pin.lifecycleGeneration||
            after.contentRevision!==receipt.contentRevision||typeof after.documentSnapshotId!=='string'||
            !same(stamp.session.info(),after)||stamp.session.readSubscriptionGeneration()!==stamp.ticket||
            (!receipt.result.applied&&after.documentSnapshotId!==pin.documentSnapshotId)||
            (receipt.result.applied&&after.documentSnapshotId===pin.documentSnapshotId))throw fault();
        });
        if(retired||stamp.generation!==generation||ports.currentSession()!==stamp.session)throw fault();
        var projection=await refresh(after);
        return Object.freeze({status:receipt.result.applied?'applied-presented':'unchanged',applied:receipt.result.applied,projection:projection});
      } catch(error){cache=null;return Object.freeze({status:receipt?'applied-unconfirmed':'unconfirmed',applied:receipt?receipt.result.applied:null,code:error&&error.code||'unavailable'});}
    }
    return Object.freeze({refresh:refresh,edit:edit,invalidate:invalidate,
      // Only an explicit caller receives the first refusal; normal presentation reads stay silent.
      read:function(onRefusal){
        var code=null;function failed(value){if(code===null)code=value;}
        try{
          if(!cache)failed('cache-empty');
          else if(current(cache.stamp,failed)){
            var shown=cache.stamp.session.readPresentedObjectSession();
            if(!presented(shown,cache.stamp.info))failed(shown?'presentation-mismatch':'presentation-missing');
            else if(available(cache.stamp,failed))return cache.value;
          }
        }catch(_){failed('exception');}
        cache=null;
        if(typeof onRefusal==='function')try{onRefusal(code||'unknown');}catch(_){}
        return null;
      },
      dispose:function(){retired=true;invalidate();}});
  }
  return Object.freeze({create:create});
}));
