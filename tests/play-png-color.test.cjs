'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const {encodeRGBA,canvas,decode,crc32}=require('./fixtures/lib/png.cjs');
const {colorSpace,sampleInSRGB,srgbPixelReader}=require('./desktop/play-png-color.cjs');
function chunk(type,bytes){
  const out=Buffer.alloc(bytes.length+12);out.writeUInt32BE(bytes.length);out.write(type,4);
  Buffer.from(bytes).copy(out,8);out.writeUInt32BE(crc32(out.subarray(4,-4)),out.length-4);return out;
}
function png(tags=[],rgba=[247,206,205,255]){
  const im=canvas(1,1,rgba),base=encodeRGBA(1,1,im.rgba);
  return Buffer.concat([base.subarray(0,33),...tags.map(([t,b])=>chunk(t,b)),base.subarray(33)]);
}
const p3=['cICP',[12,13,0,1]];
test('original tagged samples use sRGB without mutating PNG bytes or alpha',()=>{
  // Independent ColorSync reference triplets, frozen before implementation.
  for(const [input,want] of [[[247,206,205],[255,204,204]],[[239,135,132],[255,128,128]],
    [[235,72,63],[255,51,51]],[[128,128,128],[128,128,128]],[[255,255,255],[255,255,255]]]){
    const b=png([p3],[...input,173]),original=Buffer.from(b),read=srgbPixelReader(b,decode(b)),actual=read(0,0);
    assert.ok(actual.slice(0,3).every((n,i)=>Math.abs(n-want[i])<=1),JSON.stringify(actual));
    assert.equal(actual[3],173);assert.deepEqual(b,original);
  }
});
test('supported cICP outranks lower precedence profiles; ordinary sRGB stays unchanged',()=>{
  const tags=[['iCCP',[1,0,0,1]],['gAMA',[0,0,177,143]],['cHRM',new Array(32).fill(0)],['sRGB',[0]]];
  assert.equal(colorSpace(png([p3,...tags])),'display-p3');
  for(const tags of [[],[['sRGB',[0]]],[['cICP',[1,13,0,1]]]]){
    const b=png(tags);assert.deepEqual(srgbPixelReader(b,decode(b))(0,0),[247,206,205,255]);
  }
});
test('unknown or ambiguous color metadata refuses instead of falling back',()=>{
  for(const tags of [[['cICP',[9,16,0,1]],['sRGB',[0]]],[['cICP',[12,13,0,0]]],
    [['cICP',[12,13,1,1]]],[['cICP',[12,13,0]]],[p3,p3],[['sRGB',[0]],['sRGB',[1]]],
    [['sRGB',[4]]],[['sRGB',[]]],[['iCCP',[1,0,0,1]]],[['gAMA',[0,0,177,143]]],
    [['cHRM',new Array(32).fill(0)]],[['sRGB',[0]],['iCCP',[1,0,0,1]]]]){
    assert.throws(()=>colorSpace(png(tags)),/visual-color/);
  }
});
test('bad CRC, late metadata, trailing chunks and truncation refuse',()=>{
  const good=png([p3]),bad=Buffer.from(good);bad[42]^=1;
  const base=png();const late=Buffer.concat([base.subarray(0,-12),chunk(...p3),base.subarray(-12)]);
  for(const b of [bad,late,good.subarray(0,-1),Buffer.concat([good,chunk('sRGB',[0])])])
    assert.throws(()=>colorSpace(b),/visual-color/);
});
test('out-of-gamut colors remain out of gamut and cannot be clipped into the oracle',()=>{
  const red=sampleInSRGB([255,0,0,255],'display-p3');
  assert.ok(red[0]>259 && red[1]<-4 && red[2]<-4);
  assert.deepEqual(sampleInSRGB([0,0,0,255],'display-p3'),[0,0,0,255]);
});
