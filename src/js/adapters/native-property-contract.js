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
    function readTarget(value) {
      exact(value, ['contextId', 'layerUid'], ['strokeId'], 'property read target');
      Object.values(value).forEach(objectUid);
    }
    function readPayload(value) {
      exact(value, ['atRevision', 'documentSnapshotId', 'view'], [], 'property read');
      revision(value.atRevision, 'atRevision'); objectUid(value.documentSnapshotId);
      const view = value.view;
      if (!plain(view) || !['rawSample', 'authoredKeys'].includes(view.kind)) {
        throw new TypeError('unsupported property read view');
      }
      const fields = view.kind === 'rawSample' ? ['frame'] : ['offset', 'limit'];
      exact(view, ['kind', 'target', 'property', ...fields], [], 'property read view');
      readTarget(view.target); objectUid(view.property);
      if (view.kind === 'rawSample') {
        if (!Number.isFinite(view.frame)) throw new TypeError('invalid raw sample frame');
      } else {
        frame(view.offset);
        if (!Number.isInteger(view.limit) || view.limit < 1 || view.limit > 8) {
          throw new TypeError('invalid authored key page');
        }
      }
    }
    function readVector(value, dimension) {
      if (!Array.isArray(value) || value.length !== dimension || !Array.from(value).every(Number.isFinite)) {
        throw new TypeError('invalid property read vector');
      }
    }
    function readKey(value, dimension) {
      exact(value, ['frame', 'v'], ['hold', 'curvePoints', 'hIn', 'hOut'], 'authored property key');
      if (!Number.isFinite(value.frame)) throw new TypeError('invalid authored key frame');
      readVector(value.v, dimension);
      if (has(value, 'hold') && typeof value.hold !== 'boolean') throw new TypeError('invalid authored hold');
      for (const name of ['hIn', 'hOut']) if (has(value, name)) readVector(value[name], 2);
      if (has(value, 'curvePoints') && value.curvePoints !== null) {
        if (!Array.isArray(value.curvePoints)) throw new TypeError('invalid authored curve');
        let previous = -Infinity;
        for (const point of value.curvePoints) {
          exact(point, ['x', 'y'], ['tx', 'ty'], 'authored curve point');
          if (!Object.values(point).every(Number.isFinite) || point.x <= previous) {
            throw new TypeError('invalid authored curve coordinates');
          }
          previous = point.x;
        }
        if (value.curvePoints.length >= 2 && (value.curvePoints[0].x !== 0 || previous !== 1)) {
          throw new TypeError('invalid authored curve endpoints');
        }
      }
    }
    function readResult(value) {
      const raw = value && value.kind === 'rawSample';
      const fields = raw ? ['frame', 'value'] : ['trackPresent', 'items', 'nextOffset'];
      exact(value, ['instanceId', 'documentId', 'atRevision', 'documentSnapshotId',
        'kind', 'target', 'property', 'dimension', ...fields], [], 'property read result');
      id(value.instanceId, 'instanceId'); id(value.documentId, 'documentId');
      revision(value.atRevision, 'atRevision'); objectUid(value.documentSnapshotId);
      readTarget(value.target); objectUid(value.property);
      if (!Number.isInteger(value.dimension) || value.dimension < 1 || value.dimension > 4) {
        throw new TypeError('invalid property read dimension');
      }
      if (raw) {
        if (!Number.isFinite(value.frame)) throw new TypeError('invalid sampled frame');
        readVector(value.value, value.dimension);
      } else {
        if (value.kind !== 'authoredKeys' || typeof value.trackPresent !== 'boolean' ||
            !Array.isArray(value.items) || value.items.length > 8) throw new TypeError('invalid authored page');
        let previous = -Infinity;
        for (const key of value.items) {
          readKey(key, value.dimension);
          if (key.frame <= previous) throw new TypeError('authored keys must strictly increase');
          previous = key.frame;
        }
        if (value.nextOffset !== null) frame(value.nextOffset);
        if (!value.trackPresent && (value.items.length || value.nextOffset !== null)) {
          throw new TypeError('absent track cannot contain an authored page');
        }
      }
      const encoded = JSON.stringify(value);
      const bytes = typeof TextEncoder === 'function' ? new TextEncoder().encode(encoded).length
        : unescape(encodeURIComponent(encoded)).length;
      if (bytes > 3072) throw new TypeError('property read result exceeds provider bound');
    }
    function readCorrelated(payload, result) {
      const view = payload.view;
      if (result.atRevision !== payload.atRevision || result.documentSnapshotId !== payload.documentSnapshotId ||
          result.kind !== view.kind || result.property !== view.property ||
          ['contextId', 'layerUid', 'strokeId'].some(key => result.target[key] !== view.target[key] ||
            has(result.target, key) !== has(view.target, key))) throw new TypeError('property read selector mismatch');
      if (view.kind === 'rawSample') {
        if (result.frame !== view.frame) throw new TypeError('raw sample frame mismatch');
      } else if (result.items.length > view.limit || result.nextOffset !== null &&
          (result.items.length !== view.limit || result.nextOffset !== view.offset + result.items.length)) {
        throw new TypeError('authored page cursor mismatch');
      }
    }
    const readPaths = {has: name => /^(?:payload\.documentSnapshotId|result\.(?:documentSnapshotId|property)|payload\.view\.property|(?:payload\.view\.target|result\.target)\.(?:contextId|layerUid|strokeId))$/.test(name)};
    return Object.freeze({ metadataPayload, metadataResult, metadataCorrelated, trackPayload, trackResult,
      readPayload, readResult, readCorrelated, readPaths });
  }
  return Object.freeze({ create });
}));
