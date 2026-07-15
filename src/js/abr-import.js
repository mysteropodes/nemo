// ---- ABR (Photoshop Brush) IMPORT (2026-07) ----
// Extracts SAMPLED brush tip shapes (real scanned/painted grayscale
// bitmaps — the kind of tip bitmap-brush.js's whole raster-texture
// architecture wants) from a .abr file, for use as Bitmap Brush tips
// (bitmap-brush.js's customTips registry, see its buildTipCanvas).
//
// Reverse-engineered from publicly available community documentation of
// the .abr binary layout (the same posture as this app's own from-scratch
// ffmpeg-piping rewrite, CLAUDE.md — implement the format independently,
// never embed someone else's code that reads it). NOT sourced from or
// derived from any Adobe specification or Adobe code.
//
// HONEST SCOPE, stated plainly because this could NOT be verified against
// a real Photoshop-exported .abr file in this environment (no such file
// was available to test with, and fetching an arbitrary binary from the
// internet to test against wasn't appropriate here):
// - Legacy format (file version 1 or 2, header at byte 0): the sampled-
//   brush sub-structure (bounds/depth/compression/PackBits pixel data)
//   this parses is verified via a SYNTHETIC round-trip test (this file's
//   own bytes written then read back) proving the parsing CODE is
//   internally consistent with this file's own understanding of the
//   layout — it does NOT prove that understanding matches Adobe's actual
//   format byte-for-byte. Needs real-world testing with an actual .abr
//   file before being trusted in production.
// - Modern format (version 6/7/10 — what current Photoshop actually
//   writes): brushes are wrapped in a nested OSType "descriptor" grammar
//   this file does NOT fully parse (a much larger undertaking). Instead
//   it scans for the 'samp' resource block and reuses the SAME sampled-
//   brush sub-parser on its contents, since community references
//   describe that inner pixel-data sub-structure as unchanged across
//   versions — best-effort, not as confident as the legacy path.
// - Computed/parametric brushes (procedural shapes with no embedded
//   bitmap at all) are not supported and are skipped — there is no
//   pixel data to extract for those by definition.
// - Any brush whose parsed dimensions fail a sanity check (0 or absurdly
//   large) is skipped rather than risked — a wrong guess here should
//   fail loudly (skip + toast), never render garbage silently.
(function () {
  function readAbrArrayBuffer(buf, opts) {
    opts = opts || {};
    var dv = new DataView(buf);
    if (buf.byteLength < 4) throw new Error('Fichier .abr trop court / invalide');
    var version = dv.getUint16(0, false);
    var brushes;
    if (version === 1 || version === 2) brushes = parseLegacy(dv, version);
    else brushes = parseModernSamp(dv);
    if (!brushes.length) throw new Error('Aucun tip exploitable trouvé dans ce .abr (brush calculé/paramétrique non supporté, ou structure non reconnue)');
    return brushes;
  }

  // ---- PackBits (RLE) decode — standard algorithm, same as TIFF/PSD ----
  function unpackBits(dv, offset, expectedLen) {
    var out = new Uint8Array(expectedLen);
    var oi = 0, i = offset;
    var end = dv.byteLength;
    while (oi < expectedLen && i < end) {
      var n = dv.getInt8(i); i++;
      if (n >= 0) {
        var count = n + 1;
        for (var k = 0; k < count && oi < expectedLen && i < end; k++) out[oi++] = dv.getUint8(i++);
      } else if (n !== -128) {
        var reps = 1 - n;
        var v = dv.getUint8(i); i++;
        for (var k2 = 0; k2 < reps && oi < expectedLen; k2++) out[oi++] = v;
      }
      // n === -128: no-op byte, some encoders emit it
    }
    return { data: out, nextOffset: i };
  }

  // Sampled-brush pixel sub-block, shared by legacy and modern paths:
  // Rect bounds (4x Int32BE: top,left,bottom,right), Int16BE depth,
  // Uint8 compression flag, then pixel rows.
  function readSampledBrushPixels(dv, offset) {
    var top = dv.getInt32(offset, false), left = dv.getInt32(offset + 4, false);
    var bottom = dv.getInt32(offset + 8, false), right = dv.getInt32(offset + 12, false);
    var depth = dv.getInt16(offset + 16, false);
    var compression = dv.getUint8(offset + 18);
    var p = offset + 19;
    var w = right - left, h = bottom - top;
    // Sanity bounds — abort this brush rather than allocate/parse garbage
    // on a misread offset (e.g. having landed one byte off inside a
    // descriptor stream on the modern-format best-effort path).
    if (w <= 0 || h <= 0 || w > 4096 || h > 4096 || (depth !== 1 && depth !== 8 && depth !== 16)) return null;

    var gray = new Uint8Array(w * h); // 0-255, will become the tip's alpha mask
    if (compression === 1) {
      // RLE: one 2-byte (legacy) row-length table isn't always present in
      // every documented variant — the more consistently-described layout
      // is a per-row Int16BE byte-count immediately before that row's
      // packed data, repeated per scanline, which is what's implemented
      // here (matches PSD's own row-length-table-less RLE image data
      // convention for a single channel).
      var rowStart = p;
      for (var row = 0; row < h; row++) {
        if (rowStart + 2 > dv.byteLength) return null;
        var rowLen = dv.getUint16(rowStart, false); rowStart += 2;
        var unpacked = unpackBits(dv, rowStart, w);
        rowStart += rowLen;
        if (depth === 8) {
          gray.set(unpacked.data.subarray(0, w), row * w);
        } else if (depth === 1) {
          expand1bit(unpacked.data, w, gray, row * w);
        } else {
          // 16-bit: take the high byte of each big-endian sample as an
          // 8-bit approximation — tip masks don't need more than 8-bit
          // precision for stamping purposes.
          for (var xi = 0; xi < w; xi++) gray[row * w + xi] = unpacked.data[xi * 2] || 0;
        }
      }
    } else {
      // Raw, uncompressed.
      if (depth === 8) {
        var need = w * h;
        if (p + need > dv.byteLength) return null;
        for (var i2 = 0; i2 < need; i2++) gray[i2] = dv.getUint8(p + i2);
      } else if (depth === 1) {
        var bytesPerRow = Math.ceil(w / 8);
        for (var row2 = 0; row2 < h; row2++) {
          var rowOff = p + row2 * bytesPerRow;
          if (rowOff + bytesPerRow > dv.byteLength) return null;
          expand1bit(new Uint8Array(dv.buffer, dv.byteOffset + rowOff, bytesPerRow), w, gray, row2 * w);
        }
      } else {
        var need2 = w * h * 2;
        if (p + need2 > dv.byteLength) return null;
        for (var i3 = 0; i3 < w * h; i3++) gray[i3] = dv.getUint8(p + i3 * 2);
      }
    }
    return { w: w, h: h, gray: gray };
  }
  function expand1bit(packed, w, out, outOffset) {
    for (var x = 0; x < w; x++) {
      var byte = packed[x >> 3] || 0;
      var bit = (byte >> (7 - (x & 7))) & 1;
      out[outOffset + x] = bit ? 0 : 255; // 1-bit brush masks: 0=paint(white/opaque), 1=no-paint by PSD convention
    }
  }

  function grayToTipCanvas(w, h, gray) {
    var c = document.createElement('canvas'); c.width = w; c.height = h;
    var ctx = c.getContext('2d');
    var imgData = ctx.createImageData(w, h);
    for (var i = 0; i < w * h; i++) {
      imgData.data[i * 4 + 0] = 255; imgData.data[i * 4 + 1] = 255; imgData.data[i * 4 + 2] = 255;
      imgData.data[i * 4 + 3] = gray[i]; // grayscale value IS the alpha mask (white tip, shaped by alpha)
    }
    ctx.putImageData(imgData, 0, 0);
    return c;
  }

  // ---- Legacy (version 1/2) top-level brush list ----
  function parseLegacy(dv, version) {
    var count = dv.getInt16(2, false);
    var offset = 4;
    var out = [];
    for (var i = 0; i < count && offset < dv.byteLength - 6; i++) {
      var type = dv.getInt16(offset, false); offset += 2;
      var size = dv.getInt32(offset, false); offset += 4;
      var blockEnd = offset + size;
      if (type === 2) { // sampled brush
        var p = offset;
        p += 4; // misc long
        p += 2; // spacing short
        var name = '';
        if (version === 2) {
          var nameLen = dv.getInt32(p, false); p += 4;
          // UTF-16BE Pascal-ish string, nameLen UTF-16 code units
          var chars = [];
          for (var ci = 0; ci < nameLen && p + 1 < dv.byteLength; ci++) { chars.push(dv.getUint16(p, false)); p += 2; }
          name = String.fromCharCode.apply(null, chars).replace(/\0+$/, '');
        }
        p += 1; // antialiasing byte (documented as present here in several references)
        p += 4; // bounds-for-brush (short rect, 4x Int16) placeholder skip — see note below
        // Note on the line above: some documented layouts place a 4x
        // Int16 "short bounds" here before the real 4x Int32 bounds used
        // by readSampledBrushPixels; kept as a best-effort offset. Given
        // the sanity checks inside readSampledBrushPixels, a wrong guess
        // here fails closed (null, brush skipped) rather than producing
        // a garbage tip.
        var px = readSampledBrushPixels(dv, p);
        if (px) out.push({ name: name || ('Brush ' + (i + 1)), canvas: grayToTipCanvas(px.w, px.h, px.gray) });
      }
      offset = blockEnd;
    }
    return out;
  }

  // ---- Modern (version 6/7/10) best-effort: locate the 'samp' resource
  // block and parse its contents with the SAME sampled-brush sub-parser.
  // The modern container is an 8BIM-resource list; each resource is
  // tagged with a 4-byte ASCII key ('desc', 'samp', 'phry', ...) — this
  // scans for the literal bytes 's','a','m','p' rather than fully walking
  // the resource list structure (which needs correct offsets this file
  // isn't confident enough in to compute reliably) — a pragmatic
  // trade-off, see this file's header comment. ----
  function parseModernSamp(dv) {
    var bytes = new Uint8Array(dv.buffer, dv.byteOffset, dv.byteLength);
    var out = [];
    for (var i = 0; i < bytes.length - 4; i++) {
      if (bytes[i] === 0x73 && bytes[i + 1] === 0x61 && bytes[i + 2] === 0x6d && bytes[i + 3] === 0x70) { // 's','a','m','p'
        // Immediately after the tag, resource blocks in this container
        // family are commonly followed by a 4-byte length — treat the
        // next 4 bytes as the samp block's total length and scan WITHIN
        // it for plausible sampled-brush sub-blocks (bounds+depth+
        // compression header) rather than assuming an exact fixed
        // preamble size, since that varies by version.
        var blockLen = dv.getUint32(i + 4, false);
        var start = i + 8, end = Math.min(dv.byteLength, start + blockLen);
        var found = scanForSampledBrushes(dv, start, end);
        found.forEach(function (b) { out.push(b); });
        if (found.length) break; // one samp block per file
      }
    }
    return out;
  }
  // Slides a window through [start,end) looking for byte offsets where
  // interpreting the next 19 bytes as (bounds Int32x4 + depth Int16 +
  // compression Uint8) yields SANE values (positive, reasonably-sized
  // bounds, depth in {1,8,16}, compression in {0,1}) — a heuristic scan,
  // not a real parse of the surrounding descriptor grammar, but bounded
  // and defensive (readSampledBrushPixels itself re-validates and bails
  // on anything not-sane before touching pixel data).
  function scanForSampledBrushes(dv, start, end) {
    var out = [];
    for (var off = start; off < end - 19; off++) {
      var top = dv.getInt32(off, false), left = dv.getInt32(off + 4, false);
      var bottom = dv.getInt32(off + 8, false), right = dv.getInt32(off + 12, false);
      var depth = dv.getInt16(off + 16, false);
      var comp = dv.getUint8(off + 18);
      var w = right - left, h = bottom - top;
      if (top < -1 || left < -1 || w <= 0 || h <= 0 || w > 2048 || h > 2048) continue;
      if (depth !== 1 && depth !== 8 && depth !== 16) continue;
      if (comp !== 0 && comp !== 1) continue;
      var px = readSampledBrushPixels(dv, off);
      if (px) {
        out.push({ name: 'Brush ' + (out.length + 1), canvas: grayToTipCanvas(px.w, px.h, px.gray) });
        off += 19 + (comp === 0 ? w * h : w * h); // rough skip past this brush's data before resuming the scan, avoids re-matching inside pixel data
      }
    }
    return out;
  }

  // ---- self-consistency test (no real .abr file available to verify
  // against in this environment — see header comment) — round-trips a
  // SYNTHETIC legacy-v1 buffer built from this file's own understanding
  // of the layout through readAbrArrayBuffer, confirming the parsing
  // CODE is internally correct relative to that understanding. Exposed
  // for manual invocation, not run automatically. ----
  function _selfTest() {
    var w = 8, h = 6;
    var gray = new Uint8Array(w * h);
    for (var i = 0; i < gray.length; i++) gray[i] = (i * 37) % 256;
    // Build: version(2) count(2) [type(2) size(4) misc(4) spacing(2) antialias(1) shortbounds(4) bounds(16) depth(2) compress(1) raw pixel data]
    var brushBodyLen = 4 + 2 + 1 + 4 + 16 + 2 + 1 + w * h;
    var total = 4 + 2 + 4 + brushBodyLen;
    var buf = new ArrayBuffer(total);
    var dv = new DataView(buf);
    var o = 0;
    dv.setUint16(o, 1, false); o += 2; // version 1
    dv.setInt16(o, 1, false); o += 2; // count
    dv.setInt16(o, 2, false); o += 2; // type=sampled
    dv.setInt32(o, brushBodyLen, false); o += 4; // size
    o += 4; // misc
    o += 2; // spacing
    o += 1; // antialias
    o += 4; // short bounds placeholder
    dv.setInt32(o, 0, false); o += 4; // top
    dv.setInt32(o, 0, false); o += 4; // left
    dv.setInt32(o, h, false); o += 4; // bottom
    dv.setInt32(o, w, false); o += 4; // right
    dv.setInt16(o, 8, false); o += 2; // depth
    dv.setUint8(o, 0); o += 1; // compression raw
    for (var gi = 0; gi < gray.length; gi++) { dv.setUint8(o, gray[gi]); o += 1; }
    var result = readAbrArrayBuffer(buf);
    var ok = result.length === 1;
    if (ok) {
      var ctx = result[0].canvas.getContext('2d');
      var back = ctx.getImageData(0, 0, w, h).data;
      for (var pi = 0; pi < gray.length; pi++) { if (back[pi * 4 + 3] !== gray[pi]) { ok = false; break; } }
    }
    return { ok: ok, brushCount: result.length, dims: result[0] && { w: result[0].canvas.width, h: result[0].canvas.height } };
  }

  window.SMAbrImport = { readAbrArrayBuffer: readAbrArrayBuffer, _selfTest: _selfTest };
})();
