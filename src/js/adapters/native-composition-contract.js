/* Closed composition contract for the existing v2 SDK. Native owns all persistent values. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeCompositionContract=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var READ='query.document.object.project.composition.read',EDIT='command.document.object.project.composition.edit';
  var has=function(o,k){return Object.prototype.hasOwnProperty.call(o,k);};
  function fail(){throw new TypeError('Invalid native composition contract');}
  function data(o){
    if(!o||typeof o!=='object'||Array.isArray(o)||![Object.prototype,null].includes(Object.getPrototypeOf(o))||Object.getOwnPropertySymbols(o).length)fail();
    var keys=Object.keys(o);
    if(Object.getOwnPropertyNames(o).length!==keys.length||keys.some(function(k){return !has(Object.getOwnPropertyDescriptor(o,k),'value');}))fail();
  }
  function exact(o,required,optional){
    data(o);var keys=Object.keys(o),allowed=required.concat(optional||[]);
    if(required.some(function(k){return !has(o,k);})||keys.some(function(k){return !allowed.includes(k);}))fail();
  }
  function finite(v){if(typeof v!=='number'||!Number.isFinite(v))fail();}
  function integer(v,min,max){if(!Number.isSafeInteger(v)||v<min||v>max)fail();}
  function text(v){
    if(typeof v!=='string'||v.includes('\0'))fail();var n=0;
    for(var i=0;i<v.length;i++){var c=v.charCodeAt(i);
      if(c>=0xD800&&c<=0xDBFF){var next=v.charCodeAt(++i);if(!(next>=0xDC00&&next<=0xDFFF))fail();n+=4;}
      else if(c>=0xDC00&&c<=0xDFFF)fail();else n+=c<128?1:c<2048?2:3;
    }
    if(n>1024)fail();
  }
  function action(a){
    data(a);
    var required={addMarker:['frame'],updateMarker:['ordinal'],removeMarker:['ordinal'],clearMarkers:[],setBpm:[]};
    var optional={addMarker:['name','color'],updateMarker:['frame','name','color'],removeMarker:[],clearMarkers:[],setBpm:['bpm','bpmOffset','bpmShow']};
    if(!has(required,a.kind))fail();exact(a,['kind'].concat(required[a.kind]),optional[a.kind]);
    ['frame','bpm','bpmOffset'].forEach(function(k){if(has(a,k))finite(a[k]);});
    ['name','color'].forEach(function(k){if(has(a,k))text(a[k]);});
    if(has(a,'ordinal'))integer(a.ordinal,1,4096);
    if(has(a,'bpmShow')&&typeof a.bpmShow!=='boolean')fail();
    return Object.freeze(Object.assign({},a));
  }
  function selector(s){
    data(s);
    var fields={settings:[],markerAt:['index'],markerNamed:['name'],markerNearest:['frame'],markerPage:['cursor','limit']};
    if(!has(fields,s.kind))fail();exact(s,['kind'].concat(fields[s.kind]));
    if(s.kind==='markerAt')integer(s.index,1,Number.MAX_SAFE_INTEGER);
    if(s.kind==='markerNamed')text(s.name);if(s.kind==='markerNearest')finite(s.frame);
    if(s.kind==='markerPage'){integer(s.cursor,0,4096);integer(s.limit,1,16);}
  }
  function payload(op,p){
    if(op===EDIT){exact(p,['command','action']);if(p.command!=='object.project.composition.edit')fail();action(p.action);}
    else if(op===READ){exact(p,['atRevision','documentSnapshotId','contextId','selector']);integer(p.atRevision,0,Number.MAX_SAFE_INTEGER);
      if(typeof p.documentSnapshotId!=='string'||!p.documentSnapshotId||typeof p.contextId!=='string'||!p.contextId)fail();selector(p.selector);}
    else fail();
  }
  function markerCount(v){integer(v.markerCount,0,4096);if(typeof v.markersPresent!=='boolean'||!v.markersPresent&&v.markerCount!==0)fail();}
  function settings(v){
    exact(v,['markersPresent','markerCount','effectiveBpm','effectiveBpmOffset','effectiveBpmShow'],['bpm','bpmOffset','bpmShow']);markerCount(v);
    ['bpm','bpmOffset','effectiveBpm','effectiveBpmOffset'].forEach(function(k){if(has(v,k))finite(v[k]);});
    if(typeof v.effectiveBpmShow!=='boolean'||has(v,'bpmShow')&&typeof v.bpmShow!=='boolean'||
      v.effectiveBpm!==(has(v,'bpm')?v.bpm:120)||v.effectiveBpmOffset!==(has(v,'bpmOffset')?v.bpmOffset:0)||
      v.effectiveBpmShow!==(has(v,'bpmShow')?v.bpmShow:false))fail();
  }
  function page(v){
    exact(v,['kind','markersPresent','markerCount','cursor','items','nextCursor']);if(v.kind!=='markerPage')fail();markerCount(v);integer(v.cursor,0,v.markerCount);
    if(!Array.isArray(v.items)||v.items.length>16||v.cursor+v.items.length>v.markerCount)fail();
    v.items.forEach(function(item,i){exact(item,['ordinal','frame'],['name','color']);integer(item.ordinal,1,4096);
      if(item.ordinal!==v.cursor+i+1)fail();finite(item.frame);['name','color'].forEach(function(k){if(has(item,k))text(item[k]);});});
    var end=v.cursor+v.items.length;
    if(end<v.markerCount){if(v.items.length===0||v.nextCursor!==end)fail();}else if(v.nextCursor!==null)fail();
  }
  function result(op,v){
    if(op===EDIT){exact(v,['applied','historyEntriesAdded']);if(typeof v.applied!=='boolean'||v.historyEntriesAdded!==(v.applied?1:0))fail();return;}
    if(op!==READ)fail();exact(v,['instanceId','documentId','atRevision','documentSnapshotId','contextId','result']);
    if(v.contextId!=='scene-root')fail();integer(v.atRevision,0,Number.MAX_SAFE_INTEGER);
    ['instanceId','documentId','documentSnapshotId'].forEach(function(k){if(typeof v[k]!=='string'||!v[k])fail();});
    var r=v.result;if(!r||typeof r!=='object')fail();
    if(has(r,'kind'))page(r);else if(has(r,'markersPresent'))settings(r);
    else {exact(r,['index','frame','time','name','color']);integer(r.index,1,4096);finite(r.frame);finite(r.time);text(r.name);if(r.color!==null)text(r.color);}
  }
  function correlate(request,response){
    if(response.instanceId!==request.instanceId||response.documentId!==request.documentId)fail();if(!response.ok)return;
    result(request.operation,response.result);
    if(request.operation===EDIT){if(response.contentRevision!==request.expectedRevision+(response.result.applied?1:0))fail();return;}
    var p=request.payload,r=response.result,v=r.result;
    if(response.contentRevision!==p.atRevision||r.instanceId!==request.instanceId||r.documentId!==request.documentId||
      r.atRevision!==p.atRevision||r.documentSnapshotId!==p.documentSnapshotId||r.contextId!==p.contextId)fail();
    if(p.selector.kind==='settings'){if(!has(v,'effectiveBpm'))fail();}
    else if(p.selector.kind==='markerPage'){if(v.kind!=='markerPage'||v.cursor!==p.selector.cursor||v.items.length>p.selector.limit)fail();}
    else if(has(v,'kind')||has(v,'markersPresent')||p.selector.kind==='markerAt'&&v.index!==p.selector.index)fail();
  }
  function admitted(op){return op===READ||op===EDIT;}
  function paths(op){if(!admitted(op))return null;return {has:function(path){return /^(?:payload\.action|payload\.selector|result\.result(?:\.items\.\d+)?)\.(?:name|color)$/.test(path);}};}
  function join(base){
    function pick(op){if(admitted(op))return api;if(!base)fail();return base;}
    return Object.freeze({admitted:function(op){return admitted(op)||!!base&&base.admitted(op);},
      requiresRevision:function(op){return admitted(op)?op===EDIT:!!base&&base.requiresRevision(op);},
      requestByteLimit:function(op){return admitted(op)?4096:base&&base.requestByteLimit?base.requestByteLimit(op):4096;},
      responseByteLimit:function(op){return admitted(op)?4096:base&&base.responseByteLimit?base.responseByteLimit(op):4096;},
      payload:function(op,v,e){return pick(op).payload(op,v,e);},result:function(op,v){return pick(op).result(op,v);},
      correlate:function(q,r){return pick(q.operation).correlate(q,r);},paths:function(op){return pick(op).paths(op);}});
  }
  var api=Object.freeze({READ:READ,EDIT:EDIT,admitted:admitted,requiresRevision:function(op){return op===EDIT;},action:action,
    payload:payload,result:result,correlate:correlate,paths:paths,requestByteLimit:function(){return 4096;},responseByteLimit:function(){return 4096;},join:join});
  return api;
}));
