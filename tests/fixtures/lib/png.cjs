'use strict';
// Minimal PNG codec (RGBA8 / RGB8, non-interlaced) with no dependencies,
// used to build deterministic fixture assets (embedded image data URLs and
// export frame sequences) and to read back frames ffmpeg produces. Encoding
// uses filter type 0 on every scanline and zlib level 9, so the same pixels
// always give the same bytes for a given Node zlib build; the manifest pins
// the committed bytes by SHA-256 either way.
const zlib = require('node:zlib');

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xEDB88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(buf) {
  let c = 0xFFFFFFFF;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xFF] ^ (c >>> 8);
  return (c ^ 0xFFFFFFFF) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4); len.writeUInt32BE(data.length, 0);
  const td = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(td), 0);
  return Buffer.concat([len, td, crc]);
}

const SIGNATURE = Buffer.from([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);

// rgba: Buffer/Uint8Array of width*height*4 bytes.
function encodeRGBA(width, height, rgba) {
  if (rgba.length !== width * height * 4) throw new Error(`encodeRGBA: expected ${width * height * 4} bytes, got ${rgba.length}`);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0); ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; ihdr[9] = 6; ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (stride + 1)] = 0;
    Buffer.from(rgba.buffer, rgba.byteOffset + y * stride, stride).copy(raw, y * (stride + 1) + 1);
  }
  const idat = zlib.deflateSync(raw, { level: 9 });
  return Buffer.concat([SIGNATURE, chunk('IHDR', ihdr), chunk('IDAT', idat), chunk('IEND', Buffer.alloc(0))]);
}

function paeth(a, b, c) {
  const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
  return pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
}

// Returns { width, height, channels, rgba } — always expanded to RGBA8.
function decode(png) {
  if (!SIGNATURE.equals(png.subarray(0, 8))) throw new Error('decode: not a PNG');
  let off = 8, width = 0, height = 0, depth = 0, colorType = 0, interlace = 0;
  const idats = [];
  while (off < png.length) {
    const len = png.readUInt32BE(off); const type = png.toString('ascii', off + 4, off + 8);
    const data = png.subarray(off + 8, off + 8 + len);
    if (type === 'IHDR') { width = data.readUInt32BE(0); height = data.readUInt32BE(4); depth = data[8]; colorType = data[9]; interlace = data[12]; }
    else if (type === 'IDAT') idats.push(data);
    else if (type === 'IEND') break;
    off += 12 + len;
  }
  if (depth !== 8 || interlace !== 0 || (colorType !== 2 && colorType !== 6)) throw new Error(`decode: unsupported PNG (depth ${depth}, color type ${colorType}, interlace ${interlace})`);
  const bpp = colorType === 6 ? 4 : 3;
  const stride = width * bpp;
  const raw = zlib.inflateSync(Buffer.concat(idats));
  const out = Buffer.alloc(stride * height);
  let prev = Buffer.alloc(stride);
  for (let y = 0; y < height; y++) {
    const f = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    const cur = Buffer.alloc(stride);
    for (let i = 0; i < stride; i++) {
      const a = i >= bpp ? cur[i - bpp] : 0, b = prev[i], c = i >= bpp ? prev[i - bpp] : 0;
      let v = line[i];
      if (f === 1) v += a; else if (f === 2) v += b; else if (f === 3) v += (a + b) >> 1; else if (f === 4) v += paeth(a, b, c);
      cur[i] = v & 0xFF;
    }
    cur.copy(out, y * stride);
    prev = cur;
  }
  if (bpp === 4) return { width, height, channels: 4, rgba: out };
  const rgba = Buffer.alloc(width * height * 4);
  for (let p = 0; p < width * height; p++) { rgba[p * 4] = out[p * 3]; rgba[p * 4 + 1] = out[p * 3 + 1]; rgba[p * 4 + 2] = out[p * 3 + 2]; rgba[p * 4 + 3] = 255; }
  return { width, height, channels: 4, rgba };
}

// Paint helpers over an RGBA buffer (integer pixel rects, no anti-aliasing:
// every fixture image is made of axis-aligned flat-color blocks so its pixel
// expectations are exact).
function canvas(width, height, fill = [255, 255, 255, 255]) {
  const rgba = Buffer.alloc(width * height * 4);
  for (let p = 0; p < width * height; p++) { rgba[p * 4] = fill[0]; rgba[p * 4 + 1] = fill[1]; rgba[p * 4 + 2] = fill[2]; rgba[p * 4 + 3] = fill[3]; }
  return { width, height, rgba };
}

function fillRect(img, x0, y0, w, h, rgba) {
  const x1 = Math.max(0, Math.min(img.width, x0 + w)), y1 = Math.max(0, Math.min(img.height, y0 + h));
  for (let y = Math.max(0, y0); y < y1; y++) for (let x = Math.max(0, x0); x < x1; x++) {
    const o = (y * img.width + x) * 4;
    img.rgba[o] = rgba[0]; img.rgba[o + 1] = rgba[1]; img.rgba[o + 2] = rgba[2]; img.rgba[o + 3] = rgba[3] === undefined ? 255 : rgba[3];
  }
  return img;
}

function pixelAt(img, x, y) {
  const o = (y * img.width + x) * 4;
  return [img.rgba[o], img.rgba[o + 1], img.rgba[o + 2], img.rgba[o + 3]];
}

function toDataUrl(png) { return 'data:image/png;base64,' + png.toString('base64'); }

module.exports = { encodeRGBA, decode, canvas, fillRect, pixelAt, toDataUrl, crc32 };
