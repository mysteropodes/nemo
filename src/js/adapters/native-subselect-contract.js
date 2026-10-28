/* Closed native subselect intent and read-page validation. No state or geometry writer. */
(function(root,factory){const api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeSubselectContract=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  const EDIT='command.document.object.subselect.edit',own=(v,k)=>Object.prototype.hasOwnProperty.call(v,k);
  const object=v=>v!==null&&typeof v==='object'&&!Array.isArray(v)&&(Object.getPrototypeOf(v)===Object.prototype||Object.getPrototypeOf(v)===null);
  const exact=(v,keys,optional=[])=>object(v)&&keys.every(k=>own(v,k))&&Object.keys(v).every(k=>keys.includes(k)||optional.includes(k));
  const integer=(v,max)=>Number.isSafeInteger(v)&&v>=0&&(max===undefined||v<=max),point=v=>Array.isArray(v)&&v.length===2&&v.every(Number.isFinite);
  const indices=v=>Array.isArray(v)&&v.length>0&&v.length<=256&&v.every(n=>integer(n,255))&&new Set(v).size===v.length;
  const fail=message=>{throw new TypeError(message);};
  function target(v){return exact(v,['contextId','layerUid','frameScope','strokeId'])&&v.contextId==='scene-root'&&
    ['layerUid','strokeId'].every(k=>typeof v[k]==='string'&&v[k].length>0)&&exact(v.frameScope,['kind','frame'])&&
    ['authored','reference'].includes(v.frameScope.kind)&&integer(v.frameScope.frame,4294967295);}
  function intent(v){
    if(!object(v))return false;let keys,valid,optional=[];
    switch(v.kind){
      case 'move':keys=['kind','indices','delta'];valid=indices(v.indices)&&point(v.delta);break;
      case 'scale':keys=['kind','indices','scale','anchor'];valid=indices(v.indices)&&point(v.scale)&&point(v.anchor);break;
      case 'rotate':keys=['kind','indices','degrees','center'];valid=indices(v.indices)&&Number.isFinite(v.degrees)&&point(v.center);break;
      case 'pointType':keys=['kind','indices','pointType'];valid=indices(v.indices)&&['corner','smooth','symmetric'].includes(v.pointType);break;
      case 'handle':keys=['kind','index','handle','point','breakPair'];optional=['pairedPoint'];valid=(!own(v,'pairedPoint')||v.breakPair===true&&point(v.pairedPoint))&&integer(v.index,255)&&['handleIn','handleOut'].includes(v.handle)&&point(v.point)&&typeof v.breakPair==='boolean';break;
      case 'toggle':keys=['kind','index'];valid=integer(v.index,255);break;
      case 'insert':keys=['kind','curve','time'];valid=integer(v.curve,255)&&Number.isFinite(v.time)&&v.time>=1e-8&&v.time<=1-1e-8;break;
      case 'delete':keys=['kind','indices'];valid=indices(v.indices);break;
      default:return false;
    }
    return valid&&exact(v,keys,optional);
  }
  function payload(op,v){if(op!==EDIT||!exact(v,['command','stableTarget','intent'],['atFrame'])||v.command!=='object.subselect.edit'||!target(v.stableTarget)||!intent(v.intent)||own(v,'atFrame')&&!integer(v.atFrame,4294967295))fail('Invalid native subselect intent');}
  function result(op,v){if(op!==EDIT||!exact(v,['applied','historyEntriesAdded'])||typeof v.applied!=='boolean'||v.historyEntriesAdded!==Number(v.applied))fail('Invalid subselect receipt');}
  function correlate(q,r){result(q.operation,r.result);if(q.instanceId!==r.instanceId||q.documentId!==r.documentId||q.expectedRevision+Number(r.result.applied)!==r.contentRevision)fail('Subselect receipt origin mismatch');}
  function readPayload(v){if(!exact(v,['atRevision','documentSnapshotId','stableTarget','subselect'])||!integer(v.atRevision)||typeof v.documentSnapshotId!=='string'||!v.documentSnapshotId||!target(v.stableTarget)||
    !exact(v.subselect,['cursor','limit'],['preview','paint','hitTest'])||!integer(v.subselect.cursor,256)||!integer(v.subselect.limit,16)||!v.subselect.limit||own(v.subselect,'preview')&&!intent(v.subselect.preview)||own(v.subselect,'paint')&&v.subselect.paint!==true||
    // #2362: hit-test geometry is the painted outline of any record, never with a preview or the paint mode.
    own(v.subselect,'hitTest')&&(v.subselect.hitTest!==true||own(v.subselect,'preview')||own(v.subselect,'paint')))fail('Invalid subselect read selector');}
  function readResult(v){
    if(!exact(v,['atRevision','documentSnapshotId','subselect'])||!integer(v.atRevision)||typeof v.documentSnapshotId!=='string'||!v.documentSnapshotId)fail('Invalid subselect read result');
    const s=v.subselect;
    if(!exact(s,['stableTarget','closed','total','cursor','nextCursor','segments','preview'],['paint','hitTest'])||own(s,'hitTest')&&s.hitTest!==true||!target(s.stableTarget)||typeof s.closed!=='boolean'||typeof s.preview!=='boolean'||own(s,'paint')&&(s.paint!==true||s.closed!==true)||
      !integer(s.total,256)||!integer(s.cursor,s.total)||!Array.isArray(s.segments)||s.segments.length>16||s.cursor+s.segments.length>s.total||
      !s.segments.every(a=>exact(a,['point','handleIn','handleOut'])&&point(a.point)&&point(a.handleIn)&&point(a.handleOut)))fail('Invalid subselect geometry page');
    const end=s.cursor+s.segments.length;
    if(end===s.total?s.nextCursor!==null:s.nextCursor!==end||!s.segments.length)fail('Invalid subselect page progress');
  }
  const sameTarget=(a,b)=>['contextId','layerUid','strokeId'].every(k=>a[k]===b[k])&&['kind','frame'].every(k=>a.frameScope[k]===b.frameScope[k]);
  function readCorrelate(p,r,documentId){readPayload(p);readResult(r);const s=r.subselect;
    if(p.atRevision!==r.atRevision||p.documentSnapshotId!==r.documentSnapshotId||!sameTarget(p.stableTarget,s.stableTarget)||s.cursor!==p.subselect.cursor||
      s.segments.length!==Math.min(p.subselect.limit,s.total-s.cursor)||s.preview!==own(p.subselect,'preview')||own(s,'paint')!==own(p.subselect,'paint')||own(s,'hitTest')!==own(p.subselect,'hitTest')||
      documentId!==undefined&&r.documentSnapshotId!==`native-object:${documentId}:${p.atRevision}`)fail('Subselect read origin mismatch');return true;
  }
  const bytes=s=>new TextEncoder().encode(s).length;
  function viewPayload(p){
    const v=p&&p.view;
    if(!exact(p,['atRevision','documentSnapshotId','view'])||!integer(p.atRevision)||typeof p.documentSnapshotId!=='string'||!p.documentSnapshotId||
      !exact(v,['kind','contextId','layerUid','frame','workId','viewGeneration','cursor','limit'],['preview','hitTest'])||v.kind!=='subselect'||v.contextId!=='scene-root'||own(v,'hitTest')&&(v.hitTest!==true||own(v,'preview'))||
      typeof v.layerUid!=='string'||!v.layerUid||typeof v.workId!=='string'||!/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(v.workId)||
      !integer(v.frame,4294967295)||!integer(v.viewGeneration)||!v.viewGeneration||!integer(v.cursor,1024)||!integer(v.limit,8)||!v.limit||own(v,'preview')&&(!exact(v.preview,['stableTarget','intent'])||!target(v.preview.stableTarget)||!intent(v.preview.intent)))fail('Invalid subselect presentation selector');
  }
  function viewResult(r){
    if(!exact(r,['kind','atRevision','documentSnapshotId','contextId','layerUid','frame','workId','viewGeneration','cursor','total','nextCursor','items','preview'],['hitTest'])||own(r,'hitTest')&&r.hitTest!==true||
      typeof r.preview!=='boolean'||!integer(r.total,1024)||!integer(r.cursor,r.total)||!Array.isArray(r.items)||r.items.length>8||r.cursor+r.items.length>r.total)fail('Invalid subselect selection page');
    viewPayload({atRevision:r.atRevision,documentSnapshotId:r.documentSnapshotId,view:{kind:r.kind,contextId:r.contextId,layerUid:r.layerUid,frame:r.frame,workId:r.workId,viewGeneration:r.viewGeneration,cursor:r.cursor,limit:8}});
    for(const item of r.items){
      // A hit-test listing (#2362) gives every painted record its bounds and role instead of a paint count.
      if(own(r,'hitTest')?!exact(item,['stableTarget','closed','count','matrix','bounds','role'],['owner'])||!Array.isArray(item.bounds)||item.bounds.length!==4||!item.bounds.every(Number.isFinite)||
        !['plain','ribbon','fill','shape','brush'].includes(item.role)||own(item,'owner')&&(item.role!=='fill'||typeof item.owner!=='string'||!item.owner):!exact(item,['stableTarget','closed','count','matrix'],['paintCount']))fail('Invalid native subselect item');
      if(!target(item.stableTarget)||item.stableTarget.contextId!==r.contextId||item.stableTarget.layerUid!==r.layerUid||typeof item.closed!=='boolean'||
        !integer(item.count,256)||own(item,'paintCount')&&(!integer(item.paintCount,256)||item.paintCount<2||item.closed!==false)||!Array.isArray(item.matrix)||item.matrix.length!==6||!item.matrix.every(Number.isFinite))fail('Invalid native subselect transform');
      const m=item.matrix,det=m[0]*m[3]-m[1]*m[2];if(!Number.isFinite(det)||Math.abs(det)<1e-12)fail('Singular native subselect transform');
    }
    const end=r.cursor+r.items.length;if(end===r.total?r.nextCursor!==null:r.nextCursor!==end||!r.items.length)fail('Invalid subselect selection progress');
  }
  function viewCorrelate(p,r,documentId){viewPayload(p);viewResult(r);
    if(p.atRevision!==r.atRevision||p.documentSnapshotId!==r.documentSnapshotId||r.preview!==own(p.view,'preview')||own(r,'hitTest')!==own(p.view,'hitTest')||['kind','contextId','layerUid','frame','workId','viewGeneration','cursor'].some(k=>p.view[k]!==r[k])||
      r.items.length!==Math.min(p.view.limit,r.total-r.cursor)||documentId!==undefined&&r.documentSnapshotId!==`native-object:${documentId}:${p.atRevision}`)fail('Subselect presentation origin mismatch');return true;
  }
  return Object.freeze({EDIT,intent,target,payload,result,correlate,readPayload,readResult,readCorrelate,viewPayload,viewResult,viewCorrelate,bytes,sameTarget});
}));
