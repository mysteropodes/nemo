/* Read-only project views. Owns no state and never fences or revives the project.
 * Ports: state() returns {info,presented,ticket,active,opening,pending,synchronizing,projectionReady}; settled() drains the
 * existing project queue; dispatch(request) uses the public native application route; requestId(kind) mints request IDs only.
 * `presented` is the actual native presented session with `.presentation`; image-mesh reads never reconstruct a view. */
(function (root, factory) {
  const contract = typeof module === 'object' && module.exports ? require('./native-object-query-contract.js') : root.NemoNativeObjectQueryContract;
  const api = factory(contract);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectProjectReads = api;
}(typeof globalThis === 'object' ? globalThis : this, function (QueryContract) {
  'use strict';
  if (!QueryContract) throw new TypeError('native object query contract is required');
  const OP = 'query.document.object';
  const PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  const PAGE_KINDS = ['outline', 'verts', 'tris', 'offsets'];
  const own = (v, k) => Object.prototype.hasOwnProperty.call(v, k);
  const object = (v) => !!v && typeof v === 'object' && !Array.isArray(v) &&
    (Object.getPrototypeOf(v) === Object.prototype || Object.getPrototypeOf(v) === null);
  const exact = (v, required, optional) => object(v) && Object.keys(v).length >= required.length &&
    required.every((k) => own(v, k)) && Object.keys(v).every((k) => required.includes(k) || optional.includes(k));
  const text = (v) => typeof v === 'string' && v.length > 0;
  const bounded = (v) => typeof v === 'string' && /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(v);
  const integer = (v, min, max) => Number.isSafeInteger(v) && v >= min && (max === undefined || v <= max);
  const finite = (v) => typeof v === 'number' && Number.isFinite(v);
  const bytes = (v) => {
    const s = JSON.stringify(v);
    return typeof TextEncoder === 'function' ? new TextEncoder().encode(s).length : unescape(encodeURIComponent(s)).length;
  };
  function fault(code, message, reason) {
    const e = new Error(message); e.code = code;
    if (reason) e.reason = reason;
    return e;
  }
  function freeze(v, seen) {
    if (!v || typeof v !== 'object' || Object.isFrozen(v)) return v;
    seen = seen || new Set(); if (seen.has(v)) return v; seen.add(v);
    Object.keys(v).forEach((k) => freeze(v[k], seen)); return Object.freeze(v);
  }
  function pins(v) { return object(v) && text(v.instanceId) && text(v.documentId) &&
    integer(v.lifecycleGeneration, 1) && integer(v.contentRevision, 0) && text(v.documentSnapshotId); }
  function samePins(a, b) { return !!a && !!b && PIN.every((k) => a[k] === b[k]); }
  function target(v) {
    return exact(v, ['contextId', 'layerUid', 'frameScope', 'strokeId'], []) && text(v.contextId) && text(v.layerUid) &&
      text(v.strokeId) && exact(v.frameScope, ['kind', 'frame'], []) && ['authored', 'reference'].includes(v.frameScope.kind) &&
      integer(v.frameScope.frame, 0, 4294967295);
  }
  const payload = QueryContract.payload, result = QueryContract.result, correlate = QueryContract.correlate;
  const paths = QueryContract.paths;

  function create(ports) {
    if (!ports || typeof ports.state !== 'function' || typeof ports.settled !== 'function' || typeof ports.dispatch !== 'function' || typeof ports.requestId !== 'function')
      throw new TypeError('project reads require state, settled, dispatch and requestId ports');
    function origin(selector, pin) {
      const s = ports.state(), shown = s && s.presented, p = shown && shown.presentation;
      if (!readyState(s) || !pins(s.info) || !samePins(s.info, pin) || !shown ||
        shown.available !== true || !p || p.owner !== 'native' || p.status !== 'presented' || !samePins(p, pin) ||
        p.contextId !== selector.contextId || p.frame !== selector.frame || !bounded(p.workId) || !integer(p.viewGeneration, 1)) return null;
      return { info: copyPins(pin), presentation: p, shown, ticket: s.ticket, contextId: p.contextId, frame: p.frame, workId: p.workId, viewGeneration: p.viewGeneration };
    }
    function current(o) {
      const s = ports.state(), shown = s && s.presented;
      return readyState(s) && samePins(s.info, o.info) && s.ticket === o.ticket &&
        shown && shown.presentation === o.presentation && samePins(shown, o.info) &&
        o.presentation.owner === 'native' && o.presentation.status === 'presented' && o.presentation.contextId === o.contextId &&
        o.presentation.frame === o.frame && o.presentation.workId === o.workId && o.presentation.viewGeneration === o.viewGeneration;
    }
    function readyState(s) {
      return !!s && s.active === true && s.opening === false && s.pending === 0 && s.synchronizing === false && s.projectionReady === true;
    }
    function requireCurrent(o) { if (!current(o)) throw fault('stale_revision', 'Native image-mesh view changed', 'mesh_view_stale'); }
    function copyPins(v) { const r = {}; PIN.forEach((k) => { r[k] = v[k]; }); return freeze(r); }
    async function settle(o) {
      requireCurrent(o);
      try { await ports.settled(); } finally { requireCurrent(o); }
    }
    function responseError(r) {
      if (r && r.ok === false && object(r.error)) throw fault(text(r.error.code) ? r.error.code : 'unavailable',
        text(r.error.message) ? r.error.message : 'Native image-mesh read refused', r.error.details && r.error.details.reason || 'native_read_refused');
    }
    async function query(o, selector, page) {
      requireCurrent(o);
      const view = { kind: 'imageMesh', contextId: selector.contextId, layerUid: selector.layerUid, frame: selector.frame,
        workId: o.workId, viewGeneration: o.viewGeneration, page: page };
      const body = { atRevision: o.info.contentRevision, documentSnapshotId: o.info.documentSnapshotId, view: view };
      return queryBody(o, body);
    }
    async function queryBody(o, body) {
      requireCurrent(o);
      payload(body);
      const request = { apiVersion: 2, requestId: ports.requestId('object-geometry-read'), instanceId: o.info.instanceId,
        documentId: o.info.documentId, operation: OP, payload: body };
      if (bytes(request) > 4096) throw fault('invalid_request', 'Native image-mesh query exceeds 4096 bytes', 'request_size');
      requireCurrent(o);
      let response, dispatchError;
      try { response = await ports.dispatch(request); } catch (e) { dispatchError = e; }
      requireCurrent(o);
      if (dispatchError) {
        if (dispatchError && dispatchError.code) throw dispatchError;
        throw fault('unavailable', 'Native image-mesh query transport failed', 'native_read_transport');
      }
      if (!object(response) || response.apiVersion !== 2 || response.requestId !== request.requestId ||
        response.instanceId !== request.instanceId || response.documentId !== request.documentId || !integer(response.contentRevision, 0) ||
        typeof response.ok !== 'boolean' || (response.ok ? !exact(response, ['apiVersion', 'requestId', 'instanceId', 'documentId', 'contentRevision', 'ok', 'result'], []) :
          !exact(response, ['apiVersion', 'requestId', 'instanceId', 'documentId', 'contentRevision', 'ok', 'error'], [])))
        throw fault('uncorrelated', 'Native image-mesh query envelope is uncorrelated', 'response_identity');
      responseError(response);
      if (response.ok !== true || response.contentRevision !== o.info.contentRevision || !own(response, 'result'))
        throw fault('uncorrelated', 'Native image-mesh query revision changed', 'response_revision');
      if (bytes(response) > 4096) throw fault('invalid_request', 'Native image-mesh response exceeds 4096 bytes', 'response_size');
      try { result(response.result); correlate(body, response.result, null, o.info.documentId); }
      catch (e) { throw fault('uncorrelated', 'Native image-mesh query result is invalid: ' + e.message, 'response_shape'); }
      requireCurrent(o);
      const v = response.result;
      return v;
    }
    async function readImageMesh(selector, pin) {
      if (!exact(selector, ['contextId', 'layerUid', 'frame'], []) || !text(selector.contextId) || !text(selector.layerUid) ||
        !integer(selector.frame, 0, 4294967295) || !pins(pin)) throw fault('invalid_request', 'Image-mesh selector is invalid', 'local_validation');
      const o = origin(selector, pin);
      if (!o) throw fault('unavailable', 'No matching presented native image-mesh view', 'mesh_view_unavailable');
      try {
        await settle(o);
        const first = await query(o, selector, { kind: 'selection' });
        requireCurrent(o);
        const base = { target: first.target, rect: first.rect, mesh: first.mesh };
        if (!target(base.target) || base.target.contextId !== selector.contextId || base.target.layerUid !== selector.layerUid ||
          base.target.frameScope.frame > selector.frame)
          throw fault('unavailable', 'Native image-mesh target is missing or ambiguous', 'mesh_target_mismatch');
        if (base.mesh === null) {
          requireCurrent(o);
          return Object.freeze({ info: copyPins(o.info), presentation: o.presentation, target: freeze(copyTarget(base.target)),
            rect: freeze(copyRect(base.rect)), meshId: null, mesh: null });
        }
        const assembled = {};
        for (const kind of PAGE_KINDS) {
          if (kind === 'offsets' && !base.mesh.offsetsPresent) continue;
          let cursor = 0, total = null, all = [], seen = new Set();
          for (;;) {
            if (seen.has(cursor)) throw fault('uncorrelated', 'Repeated image-mesh page cursor', 'mesh_page_repeated');
            seen.add(cursor);
            const pg = await query(o, selector, { kind, meshId: base.mesh.meshId, cursor, limit: 16 });
            requireCurrent(o);
            if (!sameBase(base, pg) || pg.target.contextId !== pg.contextId || pg.target.layerUid !== pg.layerUid ||
              kind === 'offsets' && !pg.mesh.offsetsPresent || pg.page.total !== expectedCount(kind, base.mesh) || pg.page.cursor !== cursor)
              throw fault('uncorrelated', 'Image-mesh pages do not share one complete target', 'mesh_page_mismatch');
            if (total !== null && total !== pg.page.total) throw fault('uncorrelated', 'Image-mesh page total changed', 'mesh_page_total');
            total = pg.page.total;
            if (pg.page.items.length === 0 && cursor < total || cursor + pg.page.items.length > total)
              throw fault('uncorrelated', 'Image-mesh page made no valid progress', 'mesh_page_incomplete');
            all = all.concat(pg.page.items);
            if (pg.page.nextCursor === null) {
              if (all.length !== total) throw fault('uncorrelated', 'Image-mesh page sequence is incomplete', 'mesh_page_incomplete');
              break;
            }
            if (pg.page.nextCursor !== cursor + pg.page.items.length || pg.page.nextCursor <= cursor)
              throw fault('uncorrelated', 'Image-mesh page cursor is stale', 'mesh_page_stale');
            cursor = pg.page.nextCursor;
          }
          assembled[kind] = all;
        }
        const meta = base.mesh;
        const outline = assembled.outline, verts = assembled.verts, tris = assembled.tris, offsets = assembled.offsets;
        if (outline.length !== meta.outlineCount || verts.length !== meta.vertexCount || tris.length !== meta.triangleIndexCount ||
          !prefixMatches(outline, verts) || tris.some((i) => i >= verts.length) || meta.offsetsPresent && (!offsets || offsets.length !== verts.length))
          throw fault('uncorrelated', 'Native image-mesh topology is incomplete or inconsistent', 'mesh_topology_invalid');
        const mesh = { outline: outline, verts: verts, tris: tris };
        if (meta.offsetsPresent) mesh.offsets = offsets;
        if (own(meta, 'cols')) mesh.cols = meta.cols;
        if (own(meta, 'rows')) mesh.rows = meta.rows;
        requireCurrent(o);
        return Object.freeze({ info: copyPins(o.info), presentation: o.presentation, target: freeze(copyTarget(base.target)),
          rect: freeze(copyRect(base.rect)), meshId: meta.meshId, mesh: freeze(mesh) });
      } catch (e) {
        if (e && e.code) throw e;
        throw fault('unavailable', 'Native image-mesh read failed: ' + (e && e.message || e), 'native_read_failed');
      }
    }
    // #2255: every widget layer of the presented frame (pad, puck, rest, axes), read in bounded pages of the one widget view.
    async function readWidgets(selector, pin) {
      if (!exact(selector, ['contextId', 'frame'], []) || selector.contextId !== 'scene-root' || !integer(selector.frame, 0, 4294967295) || !pins(pin))
        throw fault('invalid_request', 'Widget selector is invalid', 'local_validation');
      const o = origin(selector, pin);
      if (!o) throw fault('unavailable', 'No matching presented native widget view', 'widget_view_unavailable');
      await settle(o);
      let cursor = 0, total = null, items = [];
      for (;;) {
        const view = { kind: 'widget', contextId: selector.contextId, frame: selector.frame, workId: o.workId, viewGeneration: o.viewGeneration, cursor: cursor, limit: 2 };
        const r = await queryBody(o, { atRevision: o.info.contentRevision, documentSnapshotId: o.info.documentSnapshotId, view: view });
        if (total !== null && total !== r.total) throw fault('uncorrelated', 'Widget list changed between pages', 'widget_page_total');
        total = r.total; items = items.concat(r.items);
        if (r.nextCursor === null) break;
        if (r.nextCursor <= cursor) throw fault('uncorrelated', 'Widget page cursor made no progress', 'widget_page_stale');
        cursor = r.nextCursor;
      }
      if (items.length !== total) throw fault('uncorrelated', 'Widget list is incomplete', 'widget_page_incomplete');
      requireCurrent(o);
      return freeze({ info: copyPins(o.info), presentation: o.presentation, items: items });
    }
    async function subselectGeometry(o, stableTarget, intent, paint, hitTest, center) {
      let cursor=0,segments=[],total=null,closed=null,widths=[];
      for(;;){
        const page={cursor:cursor,limit:16};if(intent!==undefined)page.preview=intent;if(paint)page.paint=true;if(hitTest)page.hitTest=true;if(center)page.center=true;
        const r=(await queryBody(o,{atRevision:o.info.contentRevision,documentSnapshotId:o.info.documentSnapshotId,stableTarget:stableTarget,subselect:page})).subselect;
        if(total!==null&&(r.total!==total||r.closed!==closed))throw fault('uncorrelated','Subselect geometry changed between pages');
        total=r.total;closed=r.closed;segments=segments.concat(r.segments);if(center)widths=widths.concat(r.widths);
        if(r.nextCursor===null)break;cursor=r.nextCursor;
      }
      requireCurrent(o);return center?{closed:closed,segments:segments,widths:widths}:{closed:closed,segments:segments};
    }
    async function readSubselect(selector,pin) {
      if(!exact(selector,['contextId','layerUid','frame'],[])||selector.contextId!=='scene-root'||!text(selector.layerUid)||!integer(selector.frame,0,4294967295)||!pins(pin))throw fault('invalid_request','Invalid subselect selection');
      const o=origin(selector,pin);if(!o)throw fault('unavailable','Subselect needs a current native presentation');
      await settle(o);let cursor=0,total=null,items=[];
      for(;;){
        const view=Object.assign({kind:'subselect',workId:o.workId,viewGeneration:o.viewGeneration,cursor:cursor,limit:8},selector);
        const r=await queryBody(o,{atRevision:o.info.contentRevision,documentSnapshotId:o.info.documentSnapshotId,view:view});
        if(total!==null&&total!==r.total)throw fault('uncorrelated','Subselect selection changed between pages');
        total=r.total;items=items.concat(r.items);if(r.nextCursor===null)break;cursor=r.nextCursor;
      }
      const ids=new Set();
      for(const item of items){
        const key=stable(item.stableTarget);if(ids.has(key)||item.stableTarget.frameScope.kind!=='authored'||item.stableTarget.frameScope.frame>selector.frame)throw fault('uncorrelated','Subselect selection has invalid source identities');ids.add(key);
        const geometry=await subselectGeometry(o,item.stableTarget);
        if(geometry.closed!==item.closed||geometry.segments.length!==item.count)throw fault('uncorrelated','Subselect geometry differs from its native presentation');
        item.segments=geometry.segments;
        if(own(item,'paintCount')){
          const paint=await subselectGeometry(o,item.stableTarget,undefined,true);
          if(!paint.closed||paint.segments.length!==item.paintCount)throw fault('uncorrelated','Subselect paint differs from its native presentation');
          item.paint=paint;
        }
      }
      requireCurrent(o);return freeze({info:copyPins(o.info),presentation:o.presentation,items:items});
    }
    // #2362: Select and the Labs knife read every painted record of the presented layer (the hit-test listing: bounds
    // and role, never refused for being outside the editable scope); geometry is paged only for the records `wanted` keeps,
    // and with `centres` a kept ribbon also carries its centreline with widths (`center`, what sculpt deforms).
    async function readPresentedStrokes(selector,pin,wanted,centres){
      if(!exact(selector,['contextId','layerUid','frame'],[])||selector.contextId!=='scene-root'||!text(selector.layerUid)||!integer(selector.frame,0,4294967295)||!pins(pin))throw fault('invalid_request','Invalid presented-stroke selection');
      const o=origin(selector,pin);if(!o)throw fault('unavailable','Presented strokes need a current native presentation');
      await settle(o);let cursor=0,total=null,items=[];
      for(;;){
        const view=Object.assign({kind:'subselect',workId:o.workId,viewGeneration:o.viewGeneration,cursor:cursor,limit:8,hitTest:true},selector);
        const r=await queryBody(o,{atRevision:o.info.contentRevision,documentSnapshotId:o.info.documentSnapshotId,view:view});
        if(total!==null&&total!==r.total)throw fault('uncorrelated','Presented strokes changed between pages');
        total=r.total;items=items.concat(r.items);if(r.nextCursor===null)break;cursor=r.nextCursor;
      }
      for(const item of items){
        if(typeof wanted==='function'&&!wanted(item))continue;
        const geometry=await subselectGeometry(o,item.stableTarget,undefined,false,true);
        if(geometry.closed!==item.closed||geometry.segments.length!==item.count)throw fault('uncorrelated','Presented geometry differs from its listing');
        item.segments=geometry.segments;
        if(centres&&item.role==='ribbon'){const c=await subselectGeometry(o,item.stableTarget,undefined,false,true,true);
          item.center=c.segments.map((a,i)=>({point:a.point,handleIn:a.handleIn,handleOut:a.handleOut,width:c.widths[i]}));}
      }
      requireCurrent(o);return freeze({info:copyPins(o.info),presentation:o.presentation,items:items});
    }
    async function previewSubselect(selector,stableTarget,intent,pin){
      const o=origin(selector,pin);if(!o||!target(stableTarget)||stableTarget.contextId!==selector.contextId||stableTarget.layerUid!==selector.layerUid)throw fault('unavailable','Subselect preview origin changed');
      await settle(o);const geometry=await subselectGeometry(o,stableTarget,intent);requireCurrent(o);
      let cursor=0,matrix=null;
      for(;;){const view=Object.assign({kind:'subselect',workId:o.workId,viewGeneration:o.viewGeneration,cursor:cursor,limit:8,preview:{stableTarget:stableTarget,intent:intent}},selector);
        const r=await queryBody(o,{atRevision:o.info.contentRevision,documentSnapshotId:o.info.documentSnapshotId,view:view});
        const item=r.items.find(item=>stable(item.stableTarget)===stable(stableTarget));
        if(item){if(item.closed!==geometry.closed||item.count!==geometry.segments.length)throw fault('uncorrelated','Subselect preview geometry and transform differ');matrix=item.matrix;break;}
        if(r.nextCursor===null)throw fault('uncorrelated','Subselect preview target is absent');cursor=r.nextCursor;
      }
      requireCurrent(o);return freeze({info:copyPins(o.info),presentation:o.presentation,closed:geometry.closed,segments:geometry.segments,matrix:matrix});
    }
    async function readLegacy(layerUid, frame, pin, view) {
      function ready() {
        const s = ports.state(), p = s && s.presented;
        return s && !s.opening && samePins(s.info, pin) && p && p.available === true &&
          p.presentation && samePins(p.presentation, pin) && (view === 'cells' || p.presentation.frame === frame) && integer(frame, 0) && frame < pin.totalFrames &&
          Array.isArray(pin.layers) && pin.layers.filter((l) => l && l.layerUid === layerUid).length === 1;
      }
      try {
        if (!pins(pin)) return null;
        if (!await waitLegacy()) return null;
        const ticket = ports.state().ticket, request = { apiVersion: 2, requestId: ports.requestId(view || 'timing'), instanceId: pin.instanceId,
          documentId: pin.documentId, operation: 'query.document.object.layer.timing', payload: { atRevision: pin.contentRevision,
            documentSnapshotId: pin.documentSnapshotId, contextId: 'scene-root', layerUid: layerUid, frame: frame } };
        if (view) request.payload.view = view;
        let response, failed = false;
        try { response = await ports.dispatch(request); } catch (_) { failed = true; }
        if (failed || !ready() || ports.state().ticket !== ticket) return null;
        if (!await waitLegacy()) return null;
        const s1 = ports.state();
        if (!ready() || s1.ticket !== ticket || !response || response.ok !== true ||
          response.instanceId !== pin.instanceId || response.documentId !== pin.documentId || response.contentRevision !== pin.contentRevision) return null;
        const r = response.result;
        if (!object(r) || Object.keys(r).length !== (view ? 9 : 11) || r.instanceId !== pin.instanceId || r.documentId !== pin.documentId ||
          ['atRevision', 'documentSnapshotId', 'contextId', 'layerUid', 'frame'].some((k) => r[k] !== request.payload[k])) return null;
        if (view === 'placement' && !(r.kind === 'layerPlacement' && typeof r.placement === 'object')) return null;
        if (view === 'cells' && !(r.kind === 'layerCells' && r.cells && Array.isArray(r.cells.runs) &&
          r.cells.runs.length && r.cells.runs[0][0] === frame)) return null;
        if (view === 'range') {
          const range = r.range, fields = ['rawInPoint', 'rawOutPoint', 'inPoint', 'outPoint', 'hasCustomRange', 'hasTimeLink'];
          if (r.kind !== 'layerRange' || !object(range) || Object.keys(range).length !== 6 || !fields.every((k) => own(range, k)) ||
            !fields.slice(0, 4).every((k) => finite(range[k])) || typeof range.hasCustomRange !== 'boolean' || typeof range.hasTimeLink !== 'boolean') return null;
        }
        return freeze(view ? JSON.parse(JSON.stringify(r)) : Object.assign({}, r));
      } catch (_) { return null; }
      async function waitLegacy() {
        try { await ports.settled(); } catch (_) { ready(); return false; }
        return ready();
      }
    }
    // Whole-row timing keeps its original paged query and refuses the complete row if any page is unavailable.
    async function readLayerCells(layerUid, pin) {
      let runs = [], frame = 0;
      while (frame !== null) {
        const page = await readLegacy(layerUid, frame, pin, 'cells');
        if (!page) return null;
        runs = runs.concat(page.cells.runs);
        frame = page.cells.nextFrame;
      }
      return freeze(runs.map((run) => freeze(run.slice())));
    }
    return Object.freeze({ readLayerTiming: (l, f, p) => readLegacy(l, f, p), readLayerRange: (l, f, p) => readLegacy(l, f, p, 'range'),
      readLayerPlacement: (l, f, p) => readLegacy(l, f, p, 'placement'), readLayerCells: readLayerCells, readImageMesh: readImageMesh, readWidgets: readWidgets, readSubselect: readSubselect, readPresentedStrokes: readPresentedStrokes, previewSubselect: previewSubselect });
  }
  function expectedCount(kind, m) { return kind === 'outline' ? m.outlineCount : kind === 'verts' ? m.vertexCount : kind === 'tris' ? m.triangleIndexCount : m.vertexCount; }
  function copyTarget(t) { return { contextId: t.contextId, layerUid: t.layerUid, frameScope: { kind: t.frameScope.kind, frame: t.frameScope.frame }, strokeId: t.strokeId }; }
  function copyRect(r) { return { x: r.x, y: r.y, width: r.width, height: r.height, rotation: r.rotation }; }
  function sameBase(a, b) {
    return stable(a.target) === stable(b.target) && stable(a.rect) === stable(b.rect) && stable(a.mesh) === stable(b.mesh);
  }
  function stable(v) {
    if (Array.isArray(v)) return '[' + v.map(stable).join(',') + ']';
    if (object(v)) return '{' + Object.keys(v).sort().map((k) => JSON.stringify(k) + ':' + stable(v[k])).join(',') + '}';
    return JSON.stringify(v);
  }
  function prefixMatches(a, b) { return a.every((p, i) => p[0] === b[i][0] && p[1] === b[i][1]); }
  return Object.freeze({ payload, result, correlate, paths, create });
}));
