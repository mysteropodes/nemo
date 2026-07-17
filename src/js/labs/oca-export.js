// ---- LABS PROTOTYPE — OCA-inspired interchange export (Callipeg/TVPaint) ----
// feature-scouting.md #10: real XDTS/OCA format-guessing without a studio
// partner to validate against is low value. This ships the part every
// such pipeline interchange actually needs regardless of exact schema —
// a PNG-per-frame sequence + a JSON timing manifest (fps, canvas size,
// per-layer exposure/keyframe map) — bundled into a REAL .zip a partner
// can open in Finder/7-Zip/OCA-aware tools without any JS library:
// a from-scratch, dependency-free ZIP writer (STORE/uncompressed method,
// real CRC32, correct local-file-header + central-directory + EOCD
// records per the PKZIP APPNOTE). "OCA-inspired", not full-spec OCA
// (which also nests folders per layer and an XML timeline) — documented
// scope, not a hidden gap.
//
//   SMLabs.exportOCA({range:[0,23]})   — defaults to the whole timeline
// Downloads <project>.oca.zip. Reuses storyboard-mode's fixed rasterize
// technique (Layer scratch stays ATTACHED while populated,
// resolution:72 = 1 doc unit = 1px — both bugs found live in that
// prototype) to render each frame's flattened PNG.
(function () {
  // ---- CRC32 (standard table-based, PKZIP's polynomial) ----
  var CRC_TABLE = (function () {
    var t = new Uint32Array(256);
    for (var n = 0; n < 256; n++) {
      var c = n;
      for (var k = 0; k < 8; k++) c = (c & 1) ? (0xEDB88320 ^ (c >>> 1)) : (c >>> 1);
      t[n] = c >>> 0;
    }
    return t;
  })();
  function crc32(bytes) {
    var c = 0xFFFFFFFF;
    for (var i = 0; i < bytes.length; i++) c = CRC_TABLE[(c ^ bytes[i]) & 0xFF] ^ (c >>> 8);
    return (c ^ 0xFFFFFFFF) >>> 0;
  }

  // ---- minimal STORE-method ZIP writer ----
  function ZipWriter() { this.entries = []; }
  ZipWriter.prototype.add = function (name, bytes) {
    this.entries.push({ name: name, bytes: bytes, crc: crc32(bytes) });
  };
  function dosDateTime() {
    // PKZIP wants MS-DOS date/time; a fixed neutral stamp is fine for an
    // interchange export (no one greps a board's zip timestamp).
    return { time: 0, date: 0x21 }; // 1980-01-01, 00:00
  }
  function u16(v) { return [v & 0xFF, (v >>> 8) & 0xFF]; }
  function u32(v) { return [v & 0xFF, (v >>> 8) & 0xFF, (v >>> 16) & 0xFF, (v >>> 24) & 0xFF]; }
  function strBytes(s) { var out = []; for (var i = 0; i < s.length; i++) out.push(s.charCodeAt(i) & 0xFF); return out; }
  ZipWriter.prototype.build = function () {
    var dt = dosDateTime();
    var chunks = [], centralChunks = [];
    var offset = 0;
    this.entries.forEach(function (e) {
      var nameBytes = strBytes(e.name);
      var local = []
        .concat(u32(0x04034B50))      // local file header signature
        .concat(u16(20))              // version needed
        .concat(u16(0))                // flags
        .concat(u16(0))                // method = 0 (store)
        .concat(u16(dt.time)).concat(u16(dt.date))
        .concat(u32(e.crc))
        .concat(u32(e.bytes.length))   // compressed size
        .concat(u32(e.bytes.length))   // uncompressed size
        .concat(u16(nameBytes.length))
        .concat(u16(0))                // extra field length
        .concat(nameBytes);
      var localBytes = new Uint8Array(local.length + e.bytes.length);
      localBytes.set(local, 0);
      localBytes.set(e.bytes, local.length);
      chunks.push(localBytes);

      var central = []
        .concat(u32(0x02014B50))      // central directory header signature
        .concat(u16(20)).concat(u16(20))
        .concat(u16(0)).concat(u16(0))
        .concat(u16(dt.time)).concat(u16(dt.date))
        .concat(u32(e.crc))
        .concat(u32(e.bytes.length))
        .concat(u32(e.bytes.length))
        .concat(u16(nameBytes.length))
        .concat(u16(0)).concat(u16(0)) // extra, comment length
        .concat(u16(0)).concat(u16(0)) // disk number, internal attrs
        .concat(u32(0))                // external attrs
        .concat(u32(offset))
        .concat(nameBytes);
      centralChunks.push(new Uint8Array(central));
      offset += localBytes.length;
    });
    var centralStart = offset;
    var centralSize = centralChunks.reduce(function (s, c) { return s + c.length; }, 0);
    var eocd = new Uint8Array(
      []
        .concat(u32(0x06054B50))
        .concat(u16(0)).concat(u16(0))
        .concat(u16(this.entries.length)).concat(u16(this.entries.length))
        .concat(u32(centralSize))
        .concat(u32(centralStart))
        .concat(u16(0))
    );
    var total = offset + centralSize + eocd.length;
    var out = new Uint8Array(total);
    var p = 0;
    chunks.forEach(function (c) { out.set(c, p); p += c.length; });
    centralChunks.forEach(function (c) { out.set(c, p); p += c.length; });
    out.set(eocd, p);
    return out;
  };

  // ---- frame rasterization (same fixed technique as storyboard-mode) ----
  function pngBytesFor(frame, w, h) {
    var prevActiveLayer = project.activeLayer;
    var scratch = new Layer();
    for (var li = 0; li < state.layers.length; li++) {
      if (typeof layerIsEffectivelyVisible === 'function' && !layerIsEffectivelyVisible(li)) continue;
      if (state.layers[li].symbolId) continue;
      var strokes = getEffectiveStrokes(li, frame);
      strokes.forEach(function (sd) { if (sd.isRaster) desR(sd, scratch); else desP(sd, scratch); });
    }
    var cv = document.createElement('canvas');
    cv.width = w; cv.height = h;
    var ctx = cv.getContext('2d');
    try {
      if (scratch.children.length) {
        var raster = scratch.rasterize({ resolution: 72, insert: false }); // 1 doc unit = 1px
        if (raster && raster.canvas) ctx.drawImage(raster.canvas, 0, 0, w, h);
        if (raster) raster.remove();
      }
    } catch (e) { console.warn('[labs] OCA export rasterize a échoué', e); }
    scratch.remove();
    prevActiveLayer.activate();
    var dataUrl = cv.toDataURL('image/png');
    var b64 = dataUrl.slice(dataUrl.indexOf(',') + 1);
    var bin = atob(b64);
    var bytes = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return bytes;
  }

  window.SMLabs.exportOCA = function (opts) {
    opts = opts || {};
    var f0 = opts.range ? opts.range[0] : 0;
    var f1 = opts.range ? opts.range[1] : state.totalFrames - 1;
    f0 = Math.max(0, f0); f1 = Math.min(state.totalFrames - 1, f1);
    var w = state.canvasW || 1920, h = state.canvasH || 1080;
    var zip = new ZipWriter();
    var manifest = {
      format: 'oca-inspired-nemo-labs-v1',
      note: 'Scope réaliste : PNG par frame + timing JSON, PAS le schéma OCA complet (pas de dossiers par calque ni de XML de timeline)',
      fps: state.fps, width: w, height: h,
      frameStart: f0, frameEnd: f1,
      layers: state.layers.map(function (ld, li) {
        return {
          name: ld.name, index: li,
          keyframes: ld.frames.slice(f0, f1 + 1).map(function (fr, i) { return fr.isKeyframe ? f0 + i : null; }).filter(function (v) { return v !== null; }),
        };
      }),
      frames: [],
    };
    for (var f = f0; f <= f1; f++) {
      var name = 'frames/frame_' + String(f).padStart(4, '0') + '.png';
      zip.add(name, pngBytesFor(f, w, h));
      manifest.frames.push({ index: f, file: name });
    }
    zip.add('manifest.json', new TextEncoder().encode(JSON.stringify(manifest, null, 2)));
    var zipBytes = zip.build();
    var blob = new Blob([zipBytes], { type: 'application/zip' });
    var name = ((window.SMProject && SMProject.getProjectKey && SMProject.getProjectKey()) || 'nemo-project') + '.oca.zip';
    var u = URL.createObjectURL(blob);
    var a = document.createElement('a'); a.href = u; a.download = name; a.click();
    URL.revokeObjectURL(u);
    if (typeof showToast === 'function') showToast('Export OCA-inspiré : ' + (f1 - f0 + 1) + ' frame(s), ' + Math.round(zipBytes.length / 1024) + ' Ko');
    return { bytes: zipBytes.length, frames: f1 - f0 + 1 };
  };

  window.SMLabs.register('oca-export', {
    flag: 'nemo-labs-oca',
    describe: 'Export interchange OCA-inspiré (Callipeg/TVPaint, scope réaliste — voir feature-scouting #10) : SMLabs.exportOCA({range}) télécharge un .zip réel (écrit sans dépendance) — PNG par frame + manifest.json de timing',
  });
})();
