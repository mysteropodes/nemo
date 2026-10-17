/* Native property public contracts; validation only, no mutable document mirror. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativePropertyContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  function create(ports) {
    const { exact, revision, objectUid, frame, id, plain, has } = ports;
    function metadataPayload(p) {
      exact(p, ['atRevision', 'documentSnapshotId', 'view'], [], 'metadata selector'); revision(p.atRevision, 'atRevision'); objectUid(p.documentSnapshotId);
      const v=p.view, keys=v && v.kind==='targets' ? ['kind','offset','limit'] : v && v.kind==='properties' ? ['kind','target','offset','limit'] : ['kind','target','property'];
      exact(v, keys, [], 'metadata view'); if (!['targets','properties','storedProperty'].includes(v.kind)) throw new TypeError('unknown metadata view');
      if (v.kind!=='targets') {exact(v.target,['contextId','layerUid'],['strokeId'],'property holder'); Object.values(v.target).forEach(objectUid);}
      if (v.kind==='storedProperty') objectUid(v.property);
      else {frame(v.offset); if (!Number.isInteger(v.limit)||v.limit<1||v.limit>8) throw new TypeError('invalid metadata page');}
    }
    function metadataResult(v) {
      exact(v,['instanceId','documentId','atRevision','documentSnapshotId','kind','target','items','nextOffset','unavailableCategories'],[],'metadata result');
      id(v.instanceId,'instanceId'); id(v.documentId,'documentId'); revision(v.atRevision,'atRevision'); objectUid(v.documentSnapshotId);
      if (!['targets','properties','storedProperty'].includes(v.kind)||!Array.isArray(v.items)||v.items.length>8) throw new TypeError('invalid metadata items');
      if (v.kind==='targets' ? v.target!==null : !plain(v.target)) throw new TypeError('invalid metadata target');
      if(v.target!==null){exact(v.target,['contextId','layerUid'],['strokeId'],'metadata target');Object.values(v.target).forEach(objectUid);}
      if (v.nextOffset!==null) frame(v.nextOffset);
      for (const x of v.items) {
        if (v.kind==='targets') {exact(x,x.kind==='context'?['kind','contextId']:['kind','target'],[],'metadata target entry'); if(x.kind==='context') objectUid(x.contextId); else {if(!['layer','element'].includes(x.kind)) throw new TypeError('unknown target kind'); exact(x.target,['contextId','layerUid'],x.kind==='element'?['strokeId']:[],'target'); if((x.kind==='element')!==has(x.target,'strokeId')) throw new TypeError('target kind mismatch'); Object.values(x.target).forEach(objectUid);} continue;}
        exact(x,['property','dimension','unit','defaultValue','animatable','controlType','controlName','authored','availability'],[],'metadata descriptor'); objectUid(x.property);
        if (!Number.isInteger(x.dimension)||x.dimension<1||x.dimension>4||typeof x.unit!=='string'||typeof x.animatable!=='boolean'||!['controlType','controlName'].every(k=>x[k]===null||typeof x[k]==='string')||!(x.defaultValue===null||Array.isArray(x.defaultValue)&&x.defaultValue.length===x.dimension&&x.defaultValue.every(Number.isFinite))) throw new TypeError('invalid descriptor scalars');
        const a=x.authored; if(a&&a.availability==='available'){exact(a,['availability','keyCount','hasStaticOverride','expressionEnabled'],[],'authored facts');frame(a.keyCount);if(['order','timeRemap'].includes(x.property)||typeof a.hasStaticOverride!=='boolean'||typeof a.expressionEnabled!=='boolean')throw new TypeError('invalid authored facts');}
        else {exact(a,['availability','reason'],[],'authored unavailable');if(a.availability!=='unavailable'||a.reason!=='separate-property-owner'||!['order','timeRemap'].includes(x.property))throw new TypeError('unowned authored facts');}
        exact(x.availability,['metadata','applicability','propertyWrite','reason'],[],'metadata availability'); const stored=v.kind==='storedProperty'; if(x.availability.metadata!=='available'||x.availability.propertyWrite!=='unavailable'||x.availability.applicability!==(stored?'unavailable':'available')||x.availability.reason!==(stored?'stored-metadata-does-not-prove-applicability':'read-only-description-not-command-availability'))throw new TypeError('metadata grants a consumer');
      }
      const cats=['conditionalMotionRows','pathVerticesTrimStrokeBrushMesh','referenceOnlyElements','expressionDiagnostics','layerEffectMatteReveal','propertyWrites'];
      if(!Array.isArray(v.unavailableCategories)||v.unavailableCategories.length!==cats.length||!cats.every(c=>v.unavailableCategories.filter(x=>x.category===c).length===1))throw new TypeError('missing unavailable family');
      v.unavailableCategories.forEach(x=>{exact(x,['category','state','reason'],[],'unavailable category');if(x.state!=='unavailable'||typeof x.reason!=='string'||!x.reason.length)throw new TypeError('unowned family');});
      const encoded=JSON.stringify(v), bytes=typeof TextEncoder==='function'?new TextEncoder().encode(encoded).length:unescape(encodeURIComponent(encoded)).length;
      if(bytes>3072)throw new TypeError('metadata inner response exceeds provider bound');
    }
    function metadataCorrelated(p,r) {
      if(r.atRevision!==p.atRevision||r.documentSnapshotId!==p.documentSnapshotId||r.kind!==p.view.kind||(p.view.kind==='targets'?r.target!==null:['contextId','layerUid','strokeId'].some(k=>r.target[k]!==p.view.target[k]||has(r.target,k)!==has(p.view.target,k))))throw new TypeError('metadata page identity mismatch');
      if(r.kind==='storedProperty'){if(r.items.length!==1||r.items[0].property!==p.view.property||r.nextOffset!==null)throw new TypeError('stored property mismatch');}
      else if(r.items.length>p.view.limit||r.nextOffset!==null&&(!r.items.length||r.nextOffset!==p.view.offset+r.items.length))throw new TypeError('nonadvancing/mismatched page cursor');
    }
    function trackPayload(value) {
      exact(value, ['command', 'selections', 'action'], [], 'property track edit');
      if (value.command !== 'object.property.track.edit' || !Array.isArray(value.selections) ||
          value.selections.length < 1 || value.selections.length > 32) {
        throw new TypeError('invalid property track edit selection');
      }
      for (const selection of value.selections) {
        exact(selection, ['target', 'property', 'frame'], [], 'property key selection');
        exact(selection.target, ['contextId', 'layerUid'], ['strokeId'], 'property holder');
        Object.values(selection.target).forEach(objectUid);
        objectUid(selection.property);
        if (!Number.isFinite(selection.frame)) throw new TypeError('invalid selected key frame');

      }
      const action = value.action;
      if (!plain(action)) throw new TypeError('property track action must be an object');
      if (action.kind === 'preset') {
        exact(action, ['kind', 'preset'], [], 'property curve preset');
        if (!['linear', 'ease', 'easeIn', 'easeOut', 'hold'].includes(action.preset)) {
          throw new TypeError('unsupported property curve preset');
        }
      } else if (action.kind === 'influence') {
        exact(action, ['kind', 'side', 'percent'], [], 'property curve influence');
        if (!['in', 'out'].includes(action.side) || !Number.isFinite(action.percent)) {
          throw new TypeError('invalid property curve influence');
        }
        // Native baseline clamps finite percentages; transport must preserve the input.
      } else if (action.kind === 'subdivide') {
        exact(action, ['kind'], [], 'property key subdivision');
      } else {
        throw new TypeError('unsupported property track action');
      }
    }
    function trackResult(value) {
      exact(value, ['applied', 'historyEntriesAdded'], [], 'property track result');
      if (typeof value.applied !== 'boolean' || value.historyEntriesAdded !== (value.applied ? 1 : 0)) {
        throw new TypeError('invalid property track disposition');
      }
    }
    return Object.freeze({ metadataPayload, metadataResult, metadataCorrelated, trackPayload, trackResult });
  }
  return Object.freeze({ create });
}));
