'use strict';
// Interpret original screenshot samples; never rewrite capture bytes or change
// the Play fixture's sRGB expectations/tolerance. Unsupported metadata refuses.
const { crc32, pixelAt } = require('../fixtures/lib/png.cjs');
function check(ok, code) { if (!ok) throw Object.assign(new Error(code), { code }); }
function colorSpace(png) {
  const colors = new Map(); let offset = 8, data = false, ended = false, header = false;
  check(Buffer.isBuffer(png) && png.length <= 32 * 1024 * 1024 &&
    png.subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10])), 'visual-color-png');
  while (offset < png.length) {
    check(offset + 12 <= png.length, 'visual-color-chunk');
    const length = png.readUInt32BE(offset), end = offset + 12 + length;
    check(end <= png.length && !ended, 'visual-color-chunk');
    const type = png.toString('ascii',offset+4,offset+8), bytes = png.subarray(offset+8,end-4);
    check(crc32(png.subarray(offset+4,end-4)) === png.readUInt32BE(end-4), 'visual-color-crc');
    if (!header) check(type === 'IHDR' && length === 13, 'visual-color-header');
    if (type === 'IHDR') { check(!header && length === 13, 'visual-color-header'); header = true; }
    if (['cICP','sRGB','iCCP','gAMA','cHRM'].includes(type)) {
      check(!data && !colors.has(type), 'visual-color-order'); colors.set(type,bytes);
    }
    if (type === 'IDAT' || type === 'PLTE') data = true;
    if (type === 'IEND') { check(length === 0 && data, 'visual-color-end'); ended = true; }
    offset = end;
  }
  check(ended, 'visual-color-end');
  // PNG Third Edition 11.3.2.6: understood cICP has highest precedence,
  // including over iCCP. Example 4 specifies Display P3 as 0C 0D 00 01.
  // https://www.w3.org/TR/png-3/#11cICP
  const cicp = colors.get('cICP');
  if (cicp) {
    check(cicp.length === 4 && cicp[1] === 13 && cicp[2] === 0 && cicp[3] === 1 &&
      [1,12].includes(cicp[0]), 'visual-color-unsupported');
    return cicp[0] === 12 ? 'display-p3' : 'srgb';
  }
  check(!colors.has('iCCP'), 'visual-color-unsupported');
  const srgb = colors.get('sRGB');
  if (srgb) { check(srgb.length === 1 && srgb[0] <= 3, 'visual-color-unsupported'); return 'srgb'; }
  check(!colors.has('gAMA') && !colors.has('cHRM'), 'visual-color-unsupported');
  return 'srgb'; // Existing untagged deterministic fixture contract.
}
// D65 Display P3 -> XYZ -> linear sRGB. Standard matrices/transfer functions:
// https://www.w3.org/TR/css-color-4/#color-conversion-code
const P3_XYZ = [[608311/1250200,189793/714400,198249/1000160],
  [35783/156275,247089/357200,198249/2500400],[0,32229/714400,5220557/5000800]];
const XYZ_SRGB = [[12831/3959,-329/214,-1974/3959],
  [-851781/878810,1648619/878810,36519/878810],[705/12673,-2585/12673,705/667]];
const multiply = (matrix, vector) => matrix.map(row => row.reduce((sum,n,i) => sum+n*vector[i],0));
function sampleInSRGB(rgba, space) {
  if (space === 'srgb') return rgba;
  check(space === 'display-p3', 'visual-color-unsupported');
  const linear = rgba.slice(0,3).map(n => { const v=n/255; return v<=0.04045 ? v/12.92 : ((v+0.055)/1.055)**2.4; });
  const rgb = multiply(XYZ_SRGB,multiply(P3_XYZ,linear)).map(v => {
    const a=Math.abs(v), encoded=a<=0.0031308 ? 12.92*a : 1.055*a**(1/2.4)-0.055;
    // Do not clamp out-of-gamut values into an expected fixture color.
    return Math.round(Math.sign(v)*encoded*255);
  });
  return [...rgb,rgba[3]];
}
function srgbPixelReader(png, image) {
  const space = colorSpace(png);
  return (x,y) => sampleInSRGB(pixelAt(image,x,y),space);
}
module.exports = { colorSpace, sampleInSRGB, srgbPixelReader };
