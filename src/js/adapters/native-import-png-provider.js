/* One DOM PNG group, one owned stage and one import batch. Desktop retains its
 * shared picker/pending/uncertain lifecycle; every dependency is an explicit port. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeImportPngProvider=api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  function create(ports) {
    // The original images.js numeric-sequence classifier; unsupported temporal imports never become stills.
    function baseName(path){var parts=path.split(/[\\/]/);return parts[parts.length-1];}
    function detectSequence(paths) {
      var re=/^(.*?)(\d+)(\.[^.]+)$/;
      var buckets={};
      paths.forEach(function(p){
        var name=baseName(p);var m=re.exec(name);
        if(!m)return;
        var key=m[1]+'||'+m[3].toLowerCase();
        (buckets[key]=buckets[key]||[]).push({path:p,num:parseInt(m[2],10),prefix:m[1]});
      });
      var bestKey=null;
      Object.keys(buckets).forEach(function(k){if(buckets[k].length>=2&&(!bestKey||buckets[k].length>buckets[bestKey].length))bestKey=k;});
      if(bestKey&&buckets[bestKey].length===paths.length){
        var items=buckets[bestKey].slice().sort(function(a,b){return a.num-b.num;});
        var prefix=items[0].prefix.replace(/[\s_\-]+$/,'')||'Sequence';
        return{isSeq:true,items:items,prefix:prefix};
      }
      return{isSeq:false};
    }
    async function importPngFiles(fileList) {
      if (!ports.stage) return { status: 'unavailable', reason: 'native_drop_handle_absent' };
      if (ports.retired() || !ports.declared()) return { status: 'unavailable', reason: 'native_image_route_absent' };
      if (ports.pending()) return { status: 'busy' };
      const unresolved = ports.uncertain();
      if (unresolved) return unresolved;
      const files = Array.prototype.slice.call(fileList || []);
      if (!files.length) return { status: 'nothing' };
      if (files.some(file => !file || typeof file.name !== 'string' || !file.name ||
          !(file.type === 'image/png' || /\.png$/i.test(file.name)))) return { status: 'unavailable', reason: 'native_png_only' };
      if (detectSequence(files.map(file => file.name)).isSeq) return { status: 'unavailable', reason: 'native_sequence_unavailable' };
      const started = ports.core.begin();
      if (!started.ok) return { status: started.status, reason: started.reason };
      const ctx = started.ctx, state = ports.state(), symbol = state.activeSymbolId, activeLayer = state.activeLayerIdx;
      const uid = ports.activeLayerUid(), layer = ctx.before.layers.find(row => row.layerUid === uid);
      if (!layer) return { status: 'unavailable', reason: 'native_active_layer_absent' };
      if (layer.locked) { ports.notify(ports.t('toastInvalidLockedLayer')); return { status: 'refused', reason: 'layer' }; }
      const hasSubscription = typeof ctx.session.readSubscriptionGeneration === 'function';
      const subscription = hasSubscription ? ctx.session.readSubscriptionGeneration() : null;
      const currentIntent = () => !ports.retired() && ports.current() === ctx.session && ctx.session.isActive() && ports.state() === state &&
        state.currentFrame === ctx.frame && state.activeSymbolId === symbol && state.activeLayerIdx === activeLayer &&
        ports.activeLayerUid() === uid && ports.samePins(ctx.session.info(), ctx.before) && ports.sameSelection(ports.selection(), ctx.selection) &&
        (!hasSubscription || typeof ctx.session.readSubscriptionGeneration === 'function' && ctx.session.readSubscriptionGeneration() === subscription) &&
        ctx.session.importBatch === ctx.write && ports.ready(ctx.before) === true &&
        ports.presented(ctx.session, ctx.before, ctx.frame);
      let attempt;
      function outcome(value) {
        if (value.status === 'unconfirmed' || value.indeterminate === true || value.status === 'imported' && value.readback !== 'confirmed') {
          ports.hold(ctx, state);
        }
        return value;
      }
      ports.setPending(true);
      try {
        attempt = ports.stage.createAttempt(ctx, { current: currentIntent });
        const batch = { layers: [], records: [], raster: [], sources: [], skipped: [], unavailable: [] };
        for (const [index, file] of files.entries()) {
          const source = await attempt.stageFile(`image:${index}:pixels`, file.name, file);
          const size = attempt.dimensions(source.sourceId);
          const fit = Math.min(1, ctx.before.canvasW / size.width, ctx.before.canvasH / size.height), offset = index * 24;
          batch.sources.push(source);
          batch.raster.push({ sourceId: `image:${index}:raster`, entry: {
            target: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: ctx.frame }, layerUid: uid, strokeId: '' },
            item: { assetId: source.sourceId, x: ctx.before.canvasW / 2 + offset, y: ctx.before.canvasH / 2 + offset,
              width: size.width * fit, height: size.height * fit, opacity: 1, rotation: 0 },
          } });
        }
        const result = outcome(await attempt.submit(batch));
        if (result.status === 'imported' && result.readback === 'confirmed') {
          ports.notify(files.length > 1 ? files.length + ports.t('toastImagesImportedSuffix') : ports.t('toastImageImported'));
        }
        return result;
      } catch (error) {
        const staging = attempt ? await attempt.abort() : { status: 'cleaned' };
        const code = error && error.code;
        const status = staging.reason === 'dispatch_may_have_started' ? 'unconfirmed'
          : code === 'staging_stale' ? 'stale' : code === 'staging_cancelled' ? 'cancelled' : 'invalid';
        return outcome({ status, reason: code || 'native_png_stage_failed', staging });
      } finally { ports.setPending(false); }
    }
    return importPngFiles;
  }
  return Object.freeze({ create });
}));
