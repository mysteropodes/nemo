'use strict';
// Measurements on a decoded export. Pure functions of the pixels; a row never reads a PNG any other way.
const crypto = require('node:crypto');
const png = require('../../fixtures/lib/png.cjs');

function decodeExport(bytes) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 33) throw new Error('export is not a PNG');
  const image = png.decode(bytes);
  if (!image || !(image.width > 0) || !(image.height > 0) || !image.rgba || image.rgba.length !== image.width * image.height * 4) throw new Error('export decoded to an invalid image');
  return image;
}
// The digest of what an export SHOWS: the colour channels of a fully transparent pixel are not observable (a straight-alpha PNG of a transparent
// canvas carries rounding residue such as 2,0,0,0 vs 4,1,1,0 for the same empty pixel), so they do not take part. Every other byte does.
const visible = image => { const rgba = Buffer.from(image.rgba); for (let i = 0; i < rgba.length; i += 4) if (rgba[i + 3] === 0) { rgba[i] = 0; rgba[i + 1] = 0; rgba[i + 2] = 0; } return rgba; };
const pixels = image => crypto.createHash('sha256').update(visible(image)).digest('hex');
// Composite over white, so a straight and a premultiplied export compare the same way.
// Every measurement takes a decoded image (export.image), never the export record: run 9 lost three rows to a record passed here.
function over(image, x, y) {
  if (!image || !image.rgba) throw new Error('probe needs a decoded image (export.image), not an export record');
  const [r, g, b, a] = png.pixelAt(image, x, y), k = a / 255;
  return [r, g, b].map(c => Math.round(c * k + 255 * (1 - k)));
}
// Two observations, never one standing in for the other. The export fills the project canvas background (project.new gives
// canvasBg #ffffff, and the baseline export fills canvasBg unless its alpha option is set: git show 1065e9d8:src/js/export.js lines 18-22;
// project.export has no alpha option, the transparent export is a project whose canvasBg is #00000000, set through
// command.document.object.project.settings.edit setCanvasBackground, whose example carries an alpha: capabilities-v2/native-project-settings.json).
//   - `alpha`: what is painted. On a transparent canvas the alpha channel IS the paint: a white fill is alpha 255 where the empty
//     canvas is alpha 0, so a white-painted item is never mistaken for clear.
//   - `distance`: what colour the paint has, composited over white. On an opaque canvas it is the only measure of paint, and a
//     white-painted item reads as clear there, which is why rows that assert clear/hole/no-ink run on a transparent canvas.
// `image.inkMode` ('alpha' | 'background', set by the kit from the canvas the row exports) picks the measure `inkAt` and `inkBounds` use.
const BACKGROUND = [255, 255, 255];
const alphaAt = (image, x, y) => png.pixelAt(image, x, y)[3];
// Largest channel difference (0..255) between the pixel, composited over white, and the white background.
const distanceAt = (image, x, y) => Math.max(...over(image, x, y).map((c, i) => Math.abs(c - BACKGROUND[i])));
const inkAt = (image, x, y) => (image && image.inkMode === 'alpha' ? alphaAt(image, x, y) : distanceAt(image, x, y));
// Opacity of a known paint colour, recovered from its composite over the background (the channel with the most contrast).
function paintAlphaAt(image, x, y, paintRgb) {
  const px = over(image, x, y);
  let best = 0;
  for (let i = 1; i < 3; i++) if (Math.abs(BACKGROUND[i] - paintRgb[i]) > Math.abs(BACKGROUND[best] - paintRgb[best])) best = i;
  return Math.round(255 * (BACKGROUND[best] - px[best]) / (BACKGROUND[best] - paintRgb[best]));
}
// Bounding box [x0, y0, x1, y1] (inclusive) of pixels whose ink (see inkAt) exceeds `minDistance`, or null for a blank image.
function inkBounds(image, { minDistance = 16, box = null } = {}) {
  const [bx0, by0, bx1, by1] = box || [0, 0, image.width - 1, image.height - 1];
  let x0 = Infinity, y0 = Infinity, x1 = -1, y1 = -1, count = 0;
  for (let y = Math.max(0, by0); y <= Math.min(image.height - 1, by1); y++) {
    for (let x = Math.max(0, bx0); x <= Math.min(image.width - 1, bx1); x++) {
      if (inkAt(image, x, y) > minDistance) { count++; if (x < x0) x0 = x; if (x > x1) x1 = x; if (y < y0) y0 = y; if (y > y1) y1 = y; }
    }
  }
  return count ? { box: [x0, y0, x1, y1], count } : null;
}
const near = (actual, expected, tol) => actual.length === expected.length && actual.every((v, i) => Math.abs(v - expected[i]) <= tol);
// How many pixels of `box` composite to within `tol` of `rgb`.
function countColor(image, rgb, { box = null, tol = 24 } = {}) {
  const [bx0, by0, bx1, by1] = box || [0, 0, image.width - 1, image.height - 1];
  let n = 0;
  for (let y = Math.max(0, by0); y <= Math.min(image.height - 1, by1); y++) {
    for (let x = Math.max(0, bx0); x <= Math.min(image.width - 1, bx1); x++) if (alphaAt(image, x, y) > 200 && near(over(image, x, y), rgb, tol)) n++;
  }
  return n;
}
// Pixels that differ between two images of the same size (0 when identical).
function differing(a, b) {
  if (a.width !== b.width || a.height !== b.height) return Infinity;
  let n = 0;
  for (let i = 0; i < a.rgba.length; i += 4) if (a.rgba[i + 3] === 0 && b.rgba[i + 3] === 0) continue; else if (a.rgba[i] !== b.rgba[i] || a.rgba[i + 1] !== b.rgba[i + 1] || a.rgba[i + 2] !== b.rgba[i + 2] || a.rgba[i + 3] !== b.rgba[i + 3]) n++;
  return n;
}
// Median of (coverage after / coverage before) at the strongest-ink pixels of `before`; 0.5 means "applied once".
function coverageRatio(before, after, box) {
  const cov = (image, x, y) => 255 - Math.min(...over(image, x, y)), ratios = [];
  for (let y = box[1]; y <= box[3]; y += 2) for (let x = box[0]; x <= box[2]; x += 2) if (inkAt(before, x, y) > 64) ratios.push(cov(after, x, y) / Math.max(1, cov(before, x, y)));
  ratios.sort((p, q) => p - q);
  return ratios.length ? ratios[Math.floor(ratios.length / 2)] : null;
}

module.exports = { decodeExport, pixels, over, alphaAt, distanceAt, inkAt, paintAlphaAt, BACKGROUND, inkBounds, countColor, differing, coverageRatio, near, encode: png.encodeRGBA };
