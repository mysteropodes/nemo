/* Read-only D35 metadata decoder. Queries use the correlated native SDK result.
 * query(request) must return only the already-correlated inner value from the
 * Lead-owned bounded common client. It never accepts a raw host envelope.
 * readContext() supplies pin, final presented receipt, current frame and a UI
 * viewGeneration token; it retains no native owner or writable holder. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeMotionPropertyView=api;}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var identity=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  function object(v){return !!v&&typeof v==='object'&&!Array.isArray(v);}
  function keys(v,names){return object(v)&&Object.keys(v).length===names.length&&names.every(function(k){return Object.prototype.hasOwnProperty.call(v,k);});}
  function text(v){return typeof v==='string'&&v.length>0;}
  function uint(v){return Number.isSafeInteger(v)&&v>=0&&v<=4294967295;}
  function size(v){return new TextEncoder().encode(JSON.stringify(v)).length;}
  function freeze(v){if(object(v)||Array.isArray(v)){Object.values(v).forEach(freeze);Object.freeze(v);}return v;}
  function refused(reason){return Object.freeze({available:false,readOnly:true,reason:reason});}
  function pin(v){return object(v)&&text(v.instanceId)&&text(v.documentId)&&text(v.documentSnapshotId)&&Number.isSafeInteger(v.contentRevision)&&v.contentRevision>=0&&Number.isSafeInteger(v.lifecycleGeneration)&&v.lifecycleGeneration>=1;}
  function same(a,b){return !!a&&!!b&&identity.every(function(k){return a[k]===b[k];});}
  function ready(context){var p=context&&context.pin,v=context&&context.presented,r=v&&v.presentation;return pin(p)&&uint(context.frame)&&Number.isSafeInteger(context.viewGeneration)&&context.viewGeneration>=0&&object(v)&&object(r)&&v.available===true&&same(p,v)&&same(p,r)&&r.owner==='native'&&r.status==='presented'&&r.frame===context.frame;}
  function capture(context){if(!ready(context))return null;var p={};identity.forEach(function(k){p[k]=context.pin[k];});return {pin:Object.freeze(p),viewGeneration:context.viewGeneration};}
  function target(v){if(!object(v))return null;var has=Object.prototype.hasOwnProperty.call(v,'strokeId');if(!keys(v,has?['contextId','layerUid','strokeId']:['contextId','layerUid'])||!text(v.contextId)||!text(v.layerUid)||has&&!text(v.strokeId))return null;return has?{contextId:v.contextId,layerUid:v.layerUid,strokeId:v.strokeId}:{contextId:v.contextId,layerUid:v.layerUid};}
  function targetKey(v){return JSON.stringify([v.contextId,v.layerUid,Object.prototype.hasOwnProperty.call(v,'strokeId'),v.strokeId]);}
  function view(v){if(!object(v))return null;var t;
    if(v.kind==='targets'&&keys(v,['kind','offset','limit'])&&uint(v.offset)&&uint(v.limit)&&v.limit>=1&&v.limit<=8)return {kind:v.kind,offset:v.offset,limit:v.limit};
    if(v.kind==='properties'&&keys(v,['kind','target','offset','limit'])&&(t=target(v.target))&&uint(v.offset)&&uint(v.limit)&&v.limit>=1&&v.limit<=8)return {kind:v.kind,target:t,offset:v.offset,limit:v.limit};
    if(v.kind==='storedProperty'&&keys(v,['kind','target','property'])&&(t=target(v.target))&&text(v.property))return {kind:v.kind,target:t,property:v.property};
    return null;
  }
  function selector(v){if(!keys(v,['instanceId','documentId','atRevision','documentSnapshotId','view'])||!text(v.instanceId)||!text(v.documentId)||!text(v.documentSnapshotId)||!Number.isSafeInteger(v.atRevision)||v.atRevision<0)return null;var w=view(v.view);return w?{instanceId:v.instanceId,documentId:v.documentId,atRevision:v.atRevision,documentSnapshotId:v.documentSnapshotId,view:w}:null;}
  function category(v){return keys(v,['category','state','reason'])&&text(v.category)&&v.state==='unavailable'&&text(v.reason)?{category:v.category,state:v.state,reason:v.reason}:null;}
  function row(v){var t;if(keys(v,['kind','contextId'])&&v.kind==='context'&&text(v.contextId))return {kind:v.kind,contextId:v.contextId};
    if(!keys(v,['kind','target'])||!(t=target(v.target)))return null;
    if(v.kind==='layer'&&!Object.prototype.hasOwnProperty.call(t,'strokeId')||v.kind==='element'&&Object.prototype.hasOwnProperty.call(t,'strokeId'))return {kind:v.kind,target:t};return null;}
  function authored(v){if(keys(v,['availability','keyCount','hasStaticOverride','expressionEnabled'])&&v.availability==='available'&&Number.isSafeInteger(v.keyCount)&&v.keyCount>=0&&typeof v.hasStaticOverride==='boolean'&&typeof v.expressionEnabled==='boolean')return {availability:v.availability,keyCount:v.keyCount,hasStaticOverride:v.hasStaticOverride,expressionEnabled:v.expressionEnabled};return keys(v,['availability','reason'])&&v.availability==='unavailable'&&v.reason==='separate-property-owner'?{availability:v.availability,reason:v.reason}:null;}
  function descriptor(v,stored){if(!keys(v,['property','dimension','unit','defaultValue','animatable','controlType','controlName','authored','availability'])||!text(v.property)||![1,2,4].includes(v.dimension)||typeof v.unit!=='string'||typeof v.animatable!=='boolean'||v.controlType!==null&&!text(v.controlType)||(v.controlType===null?v.controlName!==null:!text(v.controlName)))return null;
    var d=v.defaultValue,a=v.availability,facts=authored(v.authored);if(!facts||v.property==='timeRemap'&&facts.availability!=='unavailable'||!['order','timeRemap'].includes(v.property)&&facts.availability==='unavailable')return null;if(d!==null&&(!Array.isArray(d)||d.length!==v.dimension||!d.every(Number.isFinite)))return null;
    if(!keys(a,['metadata','applicability','propertyWrite','reason'])||a.metadata!=='available'||!['available','unavailable'].includes(a.applicability)||a.propertyWrite!=='unavailable'||!text(a.reason)||stored&&a.applicability!=='unavailable')return null;
    return {property:v.property,dimension:v.dimension,unit:v.unit,defaultValue:d===null?null:d.slice(),animatable:v.animatable,controlType:v.controlType,controlName:v.controlName,authored:facts,availability:{metadata:a.metadata,applicability:a.applicability,propertyWrite:a.propertyWrite,reason:a.reason}};
  }
  // Pure row filtering after decodePage; caller keeps the accepted page/UI identity. No holder or sampling fallback.
  function filterRow(value,mode){
    if(!['all','animated','modified','expressions'].includes(mode))return refused(['errors','effects','mattes'].includes(mode)?'native-filter-facts-unavailable':'invalid-filter');
    var row=descriptor(value,false);if(!row)return refused('malformed-native-row');if(row.availability.applicability!=='available')return refused('native-property-unavailable');
    if(mode!=='all'&&row.authored.availability!=='available')return refused('native-authored-facts-unavailable');
    var f=row.authored,match=mode==='all'||(mode==='animated'?f.keyCount>0:mode==='modified'?f.keyCount>0||f.hasStaticOverride:f.expressionEnabled);
    return Object.freeze({available:true,readOnly:true,matches:match});
  }
  function decodePage(request,value,context){try{
    var q=selector(request);if(!q||size(q)>4096)return refused('invalid-selector');if(!ready(context))return refused('unconfirmed-presentation');
    var p=context.pin;if(!keys(value,['instanceId','documentId','atRevision','documentSnapshotId','kind','target','items','nextOffset','unavailableCategories'])||size(value)>3072)return refused('malformed-native-page');
    if(['instanceId','documentId','documentSnapshotId'].some(function(k){return value[k]!==q[k]||q[k]!==p[k];})||value.atRevision!==q.atRevision||q.atRevision!==p.contentRevision)return refused('foreign-snapshot');
    if(value.kind!==q.view.kind||!Array.isArray(value.items)||!Array.isArray(value.unavailableCategories))return refused('malformed-native-page');
    var stored=q.view.kind==='storedProperty',limit=stored?1:q.view.limit,offset=stored?0:q.view.offset,t=value.target===null?null:target(value.target);
    if(q.view.kind==='targets'?value.target!==null:!t||targetKey(t)!==targetKey(q.view.target))return refused('foreign-target');
    if(value.items.length>limit||stored&&value.items.length!==1)return refused('malformed-native-page');
    var next=value.nextOffset;if(stored?next!==null:next!==null&&(!uint(next)||!value.items.length||next!==offset+value.items.length))return refused('invalid-continuation');
    var seen=new Set(),items=[];for(var raw of value.items){var item=q.view.kind==='targets'?row(raw):descriptor(raw,stored);if(!item)return refused('malformed-native-page');var key=q.view.kind==='targets'?item.kind==='context'?JSON.stringify(['context',item.contextId]):JSON.stringify([item.kind,targetKey(item.target)]):item.property;
      if(seen.has(key))return refused('duplicate-page-item');seen.add(key);if(stored&&item.property!==q.view.property)return refused('foreign-property');items.push(item);}
    var categories=[],names=new Set();for(var rawCategory of value.unavailableCategories){var c=category(rawCategory);if(!c||names.has(c.category))return refused('malformed-availability');names.add(c.category);categories.push(c);}
    if(!names.has('propertyWrites'))return refused('missing-read-only-disposition');
    var copied={};identity.forEach(function(k){copied[k]=p[k];});return freeze({available:true,readOnly:true,identity:copied,kind:value.kind,target:t,items:items,nextOffset:next,requestOffset:offset,requestLimit:limit,unavailableCategories:categories});
  }catch(_){return refused('malformed-native-page');}}
  function continuation(page){if(!page||page.available!==true||page.nextOffset===null||page.kind==='storedProperty')return null;var v={kind:page.kind,offset:page.nextOffset,limit:page.requestLimit};if(page.target)v.target=target(page.target);return freeze(v);}
  function createConsumer(ports){var serial=0,retired=false;
    async function load(input){var ticket=++serial;if(retired||!ports||typeof ports.readContext!=='function'||typeof ports.query!=='function')return refused('provider-unavailable');
      var before,q;try{before=capture(ports.readContext());var w=view(input);if(!before||!w)return refused('unavailable');q=freeze({instanceId:before.pin.instanceId,documentId:before.pin.documentId,atRevision:before.pin.contentRevision,documentSnapshotId:before.pin.documentSnapshotId,view:w});if(size(q)>4096)return refused('invalid-selector');}catch(_){return refused('unavailable');}
      var result;try{result=await ports.query(q);}catch(_){return refused('provider-refused');}
      if(retired||ticket!==serial)return refused('superseded');
      try{var after=ports.readContext();if(!ready(after)||!same(before.pin,after.pin)||before.viewGeneration!==after.viewGeneration)return refused('stale-ui-context');return decodePage(q,result,after);}catch(_){return refused('stale-ui-context');}
    }
    return Object.freeze({load:load,dispose:function(){retired=true;++serial;}});
  }
  function captureContext(context){var token=capture(context);return token?freeze(token):null;}
  function isCurrentContext(token,context){return !!token&&ready(context)&&same(token.pin,context.pin)&&token.viewGeneration===context.viewGeneration;}
  return Object.freeze({decodePage:decodePage,continuation:continuation,createConsumer:createConsumer,filterRow:filterRow,captureContext:captureContext,isCurrentContext:isCurrentContext,normalizeTarget:function(value){var v=target(value);return v?freeze(v):null;}});
}));
