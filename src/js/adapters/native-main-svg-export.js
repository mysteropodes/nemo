/* Main SVG picker and directory workflow. Native jobs remain owned by Project/Rust;
 * the legacy evaluator/download ports are used only while no native owner blocks. */
(function (root, factory) {
  var api = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMainSvgExport = api;
}(typeof globalThis === 'object' ? globalThis : this, function (root) {
  'use strict';
  var PIN_FIELDS = ['instanceId', 'documentId', 'contentRevision', 'documentSnapshotId', 'lifecycleGeneration'];
  var DESKTOP_ONLY = 'Disponible uniquement dans l\'app Nemo (pas en preview navigateur).';
  function cancelled(opts) { return !!(opts && opts.signal && opts.signal.aborted); }
  function create(ports) {
    ['authority', 'available', 'range', 'pick', 'release', 'legacy', 'frame'].forEach(function (key) {
      if (!ports || typeof ports[key] !== 'function') throw new TypeError('Main SVG export requires ' + key);
    });
    function nativeAuthority(opts) {
      var authority = ports.authority();
      if (!authority) return null;
      if (!authority.isActive() || typeof authority.exportSvg !== 'function') {
        throw new Error('Native SVG export is unavailable for this owner');
      }
      if (opts && opts.scale !== undefined && opts.scale !== 1) throw new Error('Native SVG export supports only 1×');
      return authority;
    }
    function nativeRange(opts) {
      var range = ports.range(opts);
      if (!Number.isSafeInteger(range.start) || !Number.isSafeInteger(range.end) ||
          range.start < 0 || range.end < range.start || range.end - range.start >= 32) {
        throw new Error('Native SVG export frame range is invalid');
      }
      return { start: range.start, end: range.end };
    }
    function pin(authority) {
      var current = authority.identity();
      return PIN_FIELDS.map(function (key) { return current[key]; });
    }
    function assertLegacy() {
      if (ports.authority()) throw new Error('SVG export owner changed before legacy dispatch');
    }
    async function nativeToDir(authority, dir, range, opts) {
      if (cancelled(opts)) return { cancelled: true };
      var destination = dir.replace(/[\\/]+$/, '') + '/nemo-native-svg-' + Date.now() + '-' + Math.floor(Math.random() * 1000000);
      var frames = [];
      for (var frame = range.start; frame <= range.end; frame++) frames.push(frame);
      var receipt = await authority.exportSvg(destination, frames, opts && opts.onProgress, opts && opts.signal);
      if (receipt && receipt.status === 'cancelled') return { cancelled: true };
      if (!receipt || receipt.status !== 'succeeded' || receipt.published !== true || receipt.cleanupComplete !== true) {
        throw new Error('Native SVG export publication is unconfirmed');
      }
      return { ok: true, dir: destination };
    }
    async function toDir(dir, opts) {
      if (!ports.available()) return { ok: false, error: DESKTOP_ONLY };
      var authority = nativeAuthority(opts);
      if (authority) return nativeToDir(authority, dir, nativeRange(opts), opts);
      var range = ports.range(opts);
      assertLegacy();
      return ports.legacy().run({ dir: dir, start: range.start, end: range.end,
        requestId: opts && opts.requestId, onProgress: opts && opts.onProgress });
    }
    async function run(opts) {
      var authority = nativeAuthority(opts);
      if (authority) {
        if (!ports.available()) return { ok: false, error: DESKTOP_ONLY };
        var range = nativeRange(opts);
        if (cancelled(opts)) return { cancelled: true };
        var submitted = pin(authority);
        var dir = await ports.pick('Dossier de séquence SVG');
        if (!dir || cancelled(opts)) return { cancelled: true };
        var current = nativeAuthority(opts);
        var latest = current && pin(current);
        if (current !== authority || !latest || submitted.some(function (value, i) { return value !== latest[i]; })) {
          throw new Error('Native SVG export owner changed while choosing destination');
        }
        return nativeToDir(current, dir, range, opts);
      }
      await ports.release('svg-export');
      assertLegacy();
      var legacyRange = ports.range(opts);
      if (ports.available()) {
        var legacyDir = await ports.pick('Dossier de séquence SVG');
        if (!legacyDir) return { cancelled: true };
        assertLegacy();
        return toDir(legacyDir, opts);
      }
      // Browser fallback: download a single representative frame, as before.
      var svg = ports.frame(legacyRange.start);
      var blob = new root.Blob([svg], { type: 'image/svg+xml' });
      var url = root.URL.createObjectURL(blob);
      var link = root.document.createElement('a');
      link.href = url; link.download = 'frame_0001.svg'; link.click();
      root.URL.revokeObjectURL(url);
      return { ok: true, browserFallback: true };
    }
    return Object.freeze({ run: run, toDir: toDir });
  }
  return Object.freeze({ create: create });
}));
