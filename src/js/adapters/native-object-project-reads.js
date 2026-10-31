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
  // #2256: the entered component the workspace controller holds (its id, frame count and layer uids), or null at the scene root. The timing and
  // presented-stroke reads ask THIS context, never a caller's text.
  const held = () => { try { const c = globalThis.NemoNativeComponentContext; return c && typeof c.readScope === 'function' ? c.readScope(globalThis) : null; } catch (_) { return null; } };
  const readContext = (id) => { const h = held(); return h ? id === h.contextId : id === 'scene-root'; };
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
    // #2255: the rig of one layer at the presented frame, read part by part in the bounded pages of the one rig view. `parts` names the
    // lists wanted (bones, vertices, chains, binds, bound); `weights` is one vertex of one bind and has its own call. A reason (`unavailable`)
    // replaces every list: nothing is guessed.
    const RIG_CAPS = { bones: 8, vertices: 16, chains: 8, binds: 8, bound: 64, weights: 8 };
    async function pageRig(o, selector, part, bind, vertex) {
      let cursor = 0, total = null, items = [], unavailable = null;
      for (;;) {
        const view = { kind: 'rig', contextId: selector.contextId, frame: selector.frame, workId: o.workId, viewGeneration: o.viewGeneration,
          layerUid: selector.layerUid, part: part, bind: bind, vertex: vertex, cursor: cursor, limit: RIG_CAPS[part] };
        const r = await queryBody(o, { atRevision: o.info.contentRevision, documentSnapshotId: o.info.documentSnapshotId, view: view });
        if (total !== null && total !== r.total) throw fault('uncorrelated', 'Rig list changed between pages', 'rig_page_total');
        total = r.total; items = items.concat(r.items); unavailable = r.unavailable;
        if (r.nextCursor === null) break;
        if (r.nextCursor <= cursor) throw fault('uncorrelated', 'Rig page cursor made no progress', 'rig_page_stale');
        cursor = r.nextCursor;
      }
      if (items.length !== total) throw fault('uncorrelated', 'Rig list is incomplete', 'rig_page_incomplete');
      return { items: items, unavailable: unavailable };
    }
    async function readRigParts(selector, pin, parts, bind, vertex) {
      if (!exact(selector, ['contextId', 'frame', 'layerUid'], []) || selector.contextId !== 'scene-root' || !integer(selector.frame, 0, 4294967295) ||
        typeof selector.layerUid !== 'string' || !selector.layerUid || !pins(pin) || !Array.isArray(parts) || !parts.length || !parts.every((p) => own(RIG_CAPS, p)))
        throw fault('invalid_request', 'Rig selector is invalid', 'local_validation');
      const o = origin(selector, pin);
      if (!o) throw fault('unavailable', 'No matching presented native rig view', 'rig_view_unavailable');
      await settle(o);
      const out = {};
      let unavailable = null;
      for (const part of parts) {
        const page = await pageRig(o, selector, part, bind, vertex);
        out[part] = page.items;
        if (page.unavailable) { unavailable = page.unavailable; break; }
      }
      requireCurrent(o);
      return freeze({ info: copyPins(o.info), presentation: o.presentation, layerUid: selector.layerUid, unavailable: unavailable, parts: freeze(out) });
    }
    const readRig = (selector, pin, parts) => readRigParts(selector, pin, parts, 0, 0);
    const readRigWeights = (selector, pin, bind, vertex) => (integer(bind, 0, 4294967295) && integer(vertex, 0, 4294967295)
      ? readRigParts(selector, pin, ['weights'], bind, vertex) : Promise.reject(fault('invalid_request', 'Rig weight selector is invalid', 'local_validation')));
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
    // #2358 D23: the onion / Ghost All ghost frames of one layer around the presented frame: stored vector geometry of each ghost frame,
    // read in the bounded pages of the one ghosts view. Every page is pinned to the same revision, snapshot and presentation, so a frame
    // or revision change anywhere in the sequence throws `stale_revision` and nothing partial is returned.
    // The page budget follows what the answers report instead of a fixed count: a page covers at least one segment, so a ghost frame of
    // `total` segments is at most `max(total, 1)` pages and the whole read at most one page plus the sum of the frames' totals. A ceiling on
    // the segments of the whole read bounds what one read may assemble.
    const GHOST_SEGMENT_CEILING = 2000000;
    const GHOST_KEYS = ['contextId', 'frame', 'layerUid', 'onion', 'ghostAll', 'from', 'to', 'previousOpacity', 'nextOpacity', 'mode'];
    async function readGhosts(selector, pin) {
      if (!exact(selector, GHOST_KEYS, ['previousTint', 'nextTint', 'shift']) || selector.contextId !== 'scene-root' || !integer(selector.frame, 0, 4294967295) || !text(selector.layerUid) || !pins(pin))
        throw fault('invalid_request', 'Ghost selector is invalid', 'local_validation');
      const o = origin(selector, pin);
      if (!o) throw fault('unavailable', 'No matching presented native ghost view', 'ghost_view_unavailable');
      await settle(o);
      const frames = [];
      let at = { ghost: 0, cursor: 0 }, pages = 0, allowed = 1, segments = 0;
      for (;;) {
        if (++pages > allowed) throw fault('unavailable', 'Ghost frames exceed the page budget', 'ghost_pages_exceeded');
        const view = Object.assign({ kind: 'ghosts', workId: o.workId, viewGeneration: o.viewGeneration, ghost: at.ghost, cursor: at.cursor }, selector);
        const r = await queryBody(o, { atRevision: o.info.contentRevision, documentSnapshotId: o.info.documentSnapshotId, view: view });
        if (r.ghost !== null) {
          if (at.cursor === 0) {
            segments += r.total;
            if (segments > GHOST_SEGMENT_CEILING) throw fault('unavailable', 'Ghost frames exceed the segment budget', 'ghost_segments_exceeded');
            allowed += Math.max(r.total, 1);
          }
          if (!frames[r.ghost.index]) frames[r.ghost.index] = { frame: r.ghost.frame, kind: r.ghost.kind, opacity: r.ghost.opacity, translation: r.ghost.translation, paths: [] };
          const paths = frames[r.ghost.index].paths;
          r.items.forEach((item) => {
            if (item.from === 0) paths[item.path] = { sourceId: item.sourceId, closed: item.closed, width: item.width, fill: item.fill, stroke: item.stroke, opacity: item.opacity, count: item.count, segments: [] };
            const path = paths[item.path];
            if (!path || path.segments.length !== item.from || path.count !== item.count) throw fault('uncorrelated', 'Ghost pieces are out of order', 'ghost_page_order');
            path.segments = path.segments.concat(item.segments);
          });
        }
        if (r.next === null) break;
        if (r.next.ghost < at.ghost || (r.next.ghost === at.ghost && r.next.cursor <= at.cursor)) throw fault('uncorrelated', 'Ghost page cursor made no progress', 'ghost_page_stale');
        at = r.next;
      }
      requireCurrent(o);
      const ok = frames.every((f) => f && f.paths.every((p) => p && p.segments.length === p.count));
      if (!ok) throw fault('uncorrelated', 'Ghost frames are incomplete', 'ghost_page_incomplete');
      return freeze({ info: copyPins(o.info), presentation: o.presentation, layerUid: selector.layerUid, frames: frames });
    }
    async function subselectGeometry(o, stableTarget, intent, paint, hitTest, center) {
      let cursor=0,segments=[],total=null,closed=null,widths=[],pointTypes=[],typed=false;
      for(;;){
        const page={cursor:cursor,limit:16};if(intent!==undefined)page.preview=intent;if(paint)page.paint=true;if(hitTest)page.hitTest=true;if(center)page.center=true;
        const r=(await queryBody(o,{atRevision:o.info.contentRevision,documentSnapshotId:o.info.documentSnapshotId,stableTarget:stableTarget,subselect:page})).subselect;
        if(total!==null&&(r.total!==total||r.closed!==closed))throw fault('uncorrelated','Subselect geometry changed between pages');
        total=r.total;closed=r.closed;segments=segments.concat(r.segments);if(center)widths=widths.concat(r.widths);if(own(r,'pointTypes')){typed=true;pointTypes=pointTypes.concat(r.pointTypes);}
        if(r.nextCursor===null)break;cursor=r.nextCursor;
      }
      requireCurrent(o);if(center)return {closed:closed,segments:segments,widths:widths};
      // #2356: the type each vertex shows, when the plain editable read lists it (a ribbon centreline has none).
      return typed&&pointTypes.length===segments.length?{closed:closed,segments:segments,pointTypes:pointTypes}:{closed:closed,segments:segments};
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
        item.segments=geometry.segments;if(geometry.pointTypes)item.pointTypes=geometry.pointTypes;
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
    // #2372: the listing of one layer (and the geometry paged for its records) is a pure function of the pinned revision, the
    // presentation (work id, view generation), the layer and the frame, so a hover that walks every visible layer on each move
    // reads each unchanged layer once; any commit or re-presentation changes the key and reads afresh. Bounded, oldest first out.
    const presented = new Map();
    function presentedEntry(o, selector) {
      const key = [o.info.instanceId, o.info.documentId, o.info.lifecycleGeneration, o.info.contentRevision, o.info.documentSnapshotId,
        o.workId, o.viewGeneration, selector.layerUid, selector.frame].join('|');
      if (!presented.has(key)) {
        if (presented.size >= 24) presented.delete(presented.keys().next().value);
        presented.set(key, { items: null, geometry: new Map() });
      }
      return presented.get(key);
    }
    async function readPresentedStrokes(selector,pin,wanted,centres){
      if(!exact(selector,['contextId','layerUid','frame'],[])||!readContext(selector.contextId)||!text(selector.layerUid)||!integer(selector.frame,0,4294967295)||!pins(pin))throw fault('invalid_request','Invalid presented-stroke selection');
      const o=origin(selector,pin);if(!o)throw fault('unavailable','Presented strokes need a current native presentation');
      await settle(o);const entry=presentedEntry(o,selector);let items;
      if(entry.items)items=entry.items.map(i=>Object.assign({},i));
      else{
        let cursor=0,total=null;items=[];
        for(;;){
          const view=Object.assign({kind:'subselect',workId:o.workId,viewGeneration:o.viewGeneration,cursor:cursor,limit:8,hitTest:true},selector);
          const r=await queryBody(o,{atRevision:o.info.contentRevision,documentSnapshotId:o.info.documentSnapshotId,view:view});
          if(total!==null&&total!==r.total)throw fault('uncorrelated','Presented strokes changed between pages');
          total=r.total;items=items.concat(r.items);if(r.nextCursor===null)break;cursor=r.nextCursor;
        }
        requireCurrent(o);entry.items=items.map(i=>Object.assign({},i));
      }
      const geometryOf=async(item,center)=>{
        const k=stable(item.stableTarget)+(center?'|c':''),hit=entry.geometry.get(k);if(hit)return hit;
        const g=await subselectGeometry(o,item.stableTarget,undefined,false,true,center);requireCurrent(o);entry.geometry.set(k,g);return g;
      };
      for(const item of items){
        // #2372: a raster image or text block has no outline to page; a kept one is hit as its painted rect/glyph box.
        if(item.role==='raster'||item.role==='text'){if(typeof wanted!=='function'||wanted(item))item.segments=boxSegments(item.bounds);continue;}
        if(typeof wanted==='function'&&!wanted(item))continue;
        const geometry=await geometryOf(item,false);
        if(geometry.closed!==item.closed||geometry.segments.length!==item.count)throw fault('uncorrelated','Presented geometry differs from its listing');
        item.segments=geometry.segments;
        if(centres&&item.role==='ribbon'){const c=await geometryOf(item,true);
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
      const scope = held(); // #2256: inside an entered component the row, frame count and context are the component's (its placement facts stay scene-only)
      function ready() {
        const s = ports.state(), p = s && s.presented;
        return s && !s.opening && samePins(s.info, pin) && p && p.available === true &&
          p.presentation && samePins(p.presentation, pin) && (view === 'cells' || p.presentation.frame === frame) && integer(frame, 0) && frame < (scope ? scope.totalFrames : pin.totalFrames) &&
          (scope ? held() && held().contextId === scope.contextId && scope.layerUids.filter((u) => u === layerUid).length === 1 && p.presentation.contextId === scope.contextId
            : Array.isArray(pin.layers) && pin.layers.filter((l) => l && l.layerUid === layerUid).length === 1);
      }
      if (scope && view === 'placement') return null;
      try {
        if (!pins(pin)) return null;
        if (!await waitLegacy()) return null;
        const ticket = ports.state().ticket, request = { apiVersion: 2, requestId: ports.requestId(view || 'timing'), instanceId: pin.instanceId,
          documentId: pin.documentId, operation: 'query.document.object.layer.timing', payload: { atRevision: pin.contentRevision,
            documentSnapshotId: pin.documentSnapshotId, contextId: scope ? scope.contextId : 'scene-root', layerUid: layerUid, frame: frame } };
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
    // #2376 G26: the live undo/redo stacks as one labelled page, pinned to the project's current revision and snapshot. It reads no
    // presentation: the answer is discarded unless the project is still on the same pin when it lands.
    async function readHistory(pin, offset) {
      if (!pins(pin) || offset !== undefined && !integer(offset, 0)) throw fault('invalid_request', 'History pin or offset is invalid', 'local_validation');
      const still = () => { const s = ports.state(); return readyState(s) && samePins(s.info, pin); };
      await ports.settled();
      if (!still()) throw fault('stale_revision', 'Native history changed before it was read', 'history_stale');
      const body = { atRevision: pin.contentRevision, documentSnapshotId: pin.documentSnapshotId };
      if (offset !== undefined) body.offset = offset;
      const request = { apiVersion: 2, requestId: ports.requestId('history-read'), instanceId: pin.instanceId, documentId: pin.documentId,
        operation: 'query.document.object.history', payload: body };
      let response;
      try { response = await ports.dispatch(request); } catch (e) { throw e && e.code ? e : fault('unavailable', 'Native history read transport failed', 'native_read_transport'); }
      if (!still()) throw fault('stale_revision', 'Native history changed while it was read', 'history_stale');
      if (!object(response) || response.requestId !== request.requestId || response.documentId !== request.documentId || typeof response.ok !== 'boolean')
        throw fault('uncorrelated', 'Native history response is uncorrelated', 'response_identity');
      responseError(response);
      const r = response.result;
      if (response.ok !== true || !object(r) || r.atRevision !== pin.contentRevision || !Array.isArray(r.entries))
        throw fault('uncorrelated', 'Native history result is invalid', 'response_shape');
      return freeze({ info: copyPins(pin), undoDepth: r.undoDepth, redoDepth: r.redoDepth, offset: r.offset,
        entries: r.entries.map((e) => ({ index: e.index, label: e.label })) });
    }
    return Object.freeze({ readLayerTiming: (l, f, p) => readLegacy(l, f, p), readLayerRange: (l, f, p) => readLegacy(l, f, p, 'range'),
      readLayerPlacement: (l, f, p) => readLegacy(l, f, p, 'placement'), readLayerCells: readLayerCells, readHistory: readHistory, readImageMesh: readImageMesh, readWidgets: readWidgets, readGhosts: readGhosts, readRig: readRig, readRigWeights: readRigWeights, readSubselect: readSubselect, readPresentedStrokes: readPresentedStrokes, previewSubselect: previewSubselect });
  }
  function boxSegments(b) { return [[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]].map((p) => ({ point: p, handleIn: [0, 0], handleOut: [0, 0] })); }
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
