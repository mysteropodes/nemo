/* Native script reads (CS8-SDK #2354, read increment).
 * Owner: Ilya/Claude clean-slate session. Fixture: tests/script-native-reads.test.cjs.
 * A script's reads are synchronous but the native owner answers asynchronously, so a run that reads tracks or timing is
 * executed in rounds over ONE pinned cache: a quiet pass (no goToFrame, no toast, no write sent) records every read the
 * script asked for and could not be served, the module fetches exactly those through the declared queries, and the pass
 * repeats until nothing is missing. The last pass is the real one. A read the cache cannot serve in the real pass throws
 * a typed unavailable instead of guessing. Everything held is detached JSON in frame units; a cache belongs to one pin
 * (revision + snapshot) and is dropped if the document moves between rounds.
 *   sample  query.document.object.property.read  rawSample at (layerUid, property, frame)  -> [numbers]
 *   keys    query.document.object.property.read  authoredKeys, every page                  -> [{frame,value,hold}]
 *   cells   query.document.object.layer.timing   whole-row cells                            -> [[start,count,kind,source,content,manual]]
 * Injected ports.reads: property(view) -> Promise<result>, cells(layerUid) -> Promise<runs>, composition() -> projection|null.
 */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoScriptNativeReads = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var CONTEXT = 'scene-root', PAGE = 8, MAX_ROUNDS = 16, MAX_READS = 1024, MAX_PAGES = 512, WIDTH = 8;
  var PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  function detached(value) { return value === undefined ? undefined : JSON.parse(JSON.stringify(value)); }
  function key(info) { return PIN.map(function (k) { return String(info && info[k]); }).join('|'); }
  // Whether the source can ask for a read that must be fetched first (Object.keys is not one).
  function needs(source) { return /\.(get|keys)\b|\bisKeyframe\b/.test(String(source).replace(/\bObject\s*\.\s*keys\b/g, '')); }
  function miss() { var e = new Error('nemo: read not fetched yet'); e.code = 'need-read'; return e; }
  function cache() { return { pin: null, entries: Object.create(null), wanted: Object.create(null), count: 0 }; }
  // True while the cache still belongs to the document in `info`; a new or reset cache binds to it.
  function bind(c, info, collecting) {
    var k = key(info);
    if (c.pin === null || c.pin === k) { c.pin = k; return true; }
    if (!collecting) return false;
    c.pin = k; c.entries = Object.create(null); c.wanted = Object.create(null); c.count = 0;
    return true;
  }
  // The cached answer for a read, a recorded miss while collecting, or the caller's typed refusal.
  function read(c, desc, collecting, refuse) {
    var k = JSON.stringify(desc), e = c && c.entries[k];
    if (e) { if (e.ok) return detached(e.value); throw refuse(e.reason); }
    if (!c || !collecting) throw refuse('not-prefetched');
    c.wanted[k] = desc; throw miss();
  }
  function reason(error) { return 'read-' + String(error && error.code || 'unavailable').toLowerCase().replace(/[^a-z0-9]+/g, '-'); }
  function target(desc) { return { contextId: CONTEXT, layerUid: desc.uid }; }
  async function fetchOne(reads, desc) {
    if (desc.kind === 'sample') {
      var s = await reads.property({ kind: 'rawSample', target: target(desc), property: desc.prop, frame: desc.frame });
      if (!s || !Array.isArray(s.value)) throw Object.assign(new Error('malformed sample'), { code: 'malformed' });
      return s.value.slice();
    }
    if (desc.kind === 'keys') {
      var out = [], offset = 0;
      for (var page = 0; page < MAX_PAGES; page++) {
        var r = await reads.property({ kind: 'authoredKeys', target: target(desc), property: desc.prop, offset: offset, limit: PAGE });
        if (!r || !Array.isArray(r.items)) throw Object.assign(new Error('malformed keys'), { code: 'malformed' });
        r.items.forEach(function (k) { out.push({ frame: k.frame, value: k.v.slice(), hold: k.hold === true }); });
        if (r.nextOffset === null || r.nextOffset === undefined) return out;
        if (!(r.nextOffset > offset)) throw Object.assign(new Error('keys page did not advance'), { code: 'malformed' });
        offset = r.nextOffset;
      }
      throw Object.assign(new Error('too many key pages'), { code: 'too-large' });
    }
    var runs = await reads.cells(desc.uid);
    if (!Array.isArray(runs)) throw Object.assign(new Error('timing unavailable'), { code: 'unavailable' });
    return runs.map(function (run) { return Array.prototype.slice.call(run); });
  }
  // Fetches what the last pass asked for; resolves false when the script asks for more than one run may hold.
  async function fill(c, reads) {
    var list = Object.keys(c.wanted).map(function (k) { return [k, c.wanted[k]]; });
    c.wanted = Object.create(null);
    if (c.count + list.length > MAX_READS) return false;
    for (var i = 0; i < list.length; i += WIDTH) {
      await Promise.all(list.slice(i, i + WIDTH).map(function (pair) {
        return fetchOne(reads, pair[1]).then(function (value) { c.entries[pair[0]] = { ok: true, value: value }; },
          function (error) { c.entries[pair[0]] = { ok: false, reason: reason(error) }; });
      }));
    }
    c.count += list.length;
    return true;
  }
  function pending(c) { return Object.keys(c.wanted).length > 0; }
  // isKeyframe over the whole-row cells: the run that covers the frame says 'key'.
  function isKey(runs, frame) {
    for (var i = 0; i < runs.length; i++) if (frame >= runs[i][0] && frame < runs[i][0] + runs[i][1]) return runs[i][2] === 'key';
    return false;
  }
  // The baseline grid over the composition projection (bpm-grid.js beatFrames/snapFrame, on the pinned values).
  function beats(comp, fps, total) {
    var per = (60 / Math.max(1, comp.effectiveBpm)) * Math.max(1, fps), out = [], i = 0, f;
    if (per < 0.5) return out;
    while ((f = Math.round(comp.effectiveBpmOffset + i * per)) < total && i < 4096) { if (f >= 0) out.push(f); i++; }
    return out;
  }
  function snap(comp, fps, total, frame) {
    var per = (60 / Math.max(1, comp.effectiveBpm)) * Math.max(1, fps);
    if (!comp.effectiveBpmShow || per < 0.5) return frame;
    var i = Math.round((frame - comp.effectiveBpmOffset) / per);
    return Math.max(0, Math.min(total - 1, Math.round(comp.effectiveBpmOffset + i * per)));
  }
  function markers(comp) {
    return comp.markers.map(function (m) { return { frame: m.frame, name: m.name || '', color: m.color || null }; });
  }
  return Object.freeze({ MAX_ROUNDS: MAX_ROUNDS, needs: needs, cache: cache, bind: bind, read: read, fill: fill, pending: pending,
    isKey: isKey, beats: beats, snap: snap, markers: markers, pinKey: key, detached: detached });
}));
