'use strict';
// Fixed #1864 acceptance inputs/outputs. This is not a product resource policy.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const zlib = require('node:zlib');
const WORKFLOW = 'P37_REFERENCE';
const SHA = /^[a-f0-9]{64}$/;
const HEAD = /^[a-f0-9]{40}$/;
const ID = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const MAX_FILE = 4 * 1024 * 1024;
const errors = ['new-reference-grant-required','reference-config-refused','reference-input-refused',
  'reference-oracle-refused','reference-workspace-reused','reference-output-directory-changed',
  'reference-output-refused','reference-path-refused','reference-workspace-required','reference-save-mismatch',
  'reference-png-refused','reference-pixel-mismatch','reference-policy-refused'];
const states = new WeakMap();
const labels = Object.freeze(['Open', 'Open Project…', 'Cancel', 'A', 'B', 'A.json', 'B.json',
  'Motion', 'Opacity', 'R08 rectangle', 'File', 'Edit', 'Save', 'Save As…', 'Save As', 'Save as…', 'Open…',
  'Undo', 'Redo', 'Reference', 'Reference…', 'None', 'No reference', 'Visible', 'Remove',
  'Import Reference Image', 'ReferenceA.png', 'ReferenceB.png', 'Saved.json', 'frame_0001.png',
  'Export', 'Export…', 'Export PNG', 'PNG', 'PNG Sequence', 'SVG Sequence', 'Work area', 'Whole timeline',
  '1x', 'Choose', 'Select', 'Export Image', 'Replace']);
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function check(value, code = 'reference-policy-refused') {
  if (!value) throw Object.assign(Error(code), { code });
}
function authorize(config) {
  // confirmed asserts a Lead-delivered #1864 run grant; it is not independent
  // cryptographic proof of human authorization and cannot authorize P38.
  check(config?.workflow === WORKFLOW && config.grant?.issue === 1864 &&
    config.grant.confirmed === true && ID.test(config.grant.id || ''), 'new-reference-grant-required');
  check(ID.test(config.taskId || '') && HEAD.test(config.sourceHead || '') &&
    HEAD.test(config.driverHead || '') && Number.isInteger(config.timeoutMs) &&
    config.timeoutMs >= 1000 && config.timeoutMs <= 600000, 'reference-config-refused');
  check(Array.isArray(config.fixtures) && config.fixtures.length === 2 &&
    Array.isArray(config.references) && config.references.length === 2, 'reference-input-refused');
  for (const [items, expected] of [[config.fixtures, ['A','B']], [config.references, ['ReferenceA','ReferenceB']]]) {
    check(new Set(items.map(item => item?.label)).size === 2, 'reference-input-refused');
    check(new Set(items.map(item => item?.sha256)).size === 2, 'reference-input-refused');
    for (const label of expected) {
      const item = items.find(item => item?.label === label);
      check(item && typeof item.path === 'string' && path.isAbsolute(item.path) && SHA.test(item.sha256 || ''), 'reference-input-refused');
    }
  }
  check(SHA.test(config.savedCanonicalSha256 || ''), 'reference-oracle-refused');
  check(config.normalizeGeneratedReferenceId===undefined || typeof config.normalizeGeneratedReferenceId==='boolean','reference-oracle-refused');
  const expected = config.exportOracle;
  check(expected && Number.isInteger(expected.width) && Number.isInteger(expected.height) &&
    expected.width > 0 && expected.width <= 512 && expected.height > 0 && expected.height <= 512 &&
    Array.isArray(expected.pixels) && expected.pixels.length > 0 && expected.pixels.length <= 16, 'reference-oracle-refused');
  for (const p of expected.pixels) check(p && Number.isInteger(p.x) && Number.isInteger(p.y) &&
    p.x >= 0 && p.x < expected.width && p.y >= 0 && p.y < expected.height && Array.isArray(p.rgba) &&
    p.rgba.length === 4 && p.rgba.every(v => Number.isInteger(v) && v >= 0 && v <= 255), 'reference-oracle-refused');
}
function readFile(file) {
  check(path.isAbsolute(file) && fs.realpathSync(file) === file, 'reference-input-refused');
  const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
  try {
    const stat = fs.fstatSync(fd);
    check(stat.isFile() && stat.size > 0 && stat.size <= MAX_FILE, 'reference-input-refused');
    const bytes=Buffer.allocUnsafe(stat.size); let offset=0;
    while(offset<bytes.length) {
      const count=fs.readSync(fd,bytes,offset,bytes.length-offset,offset);
      check(count>0,'reference-input-refused'); offset+=count;
    }
    check(fs.readSync(fd,Buffer.alloc(1),0,1,offset)===0 && fs.fstatSync(fd).size===stat.size,'reference-input-refused');
    return { bytes, dev:stat.dev, ino:stat.ino };
  } finally { fs.closeSync(fd); }
}
function inputs(config) {
  return [...config.fixtures.map(item => ({...item, name:item.label+'.json', json:true})),
    ...config.references.map(item => ({...item, name:item.label+'.png', json:false}))];
}
function verifyInputs(config) {
  authorize(config);
  const state = states.get(config);
  if (state) state.guard.verify();
  for (const item of inputs(config)) {
    const original = readFile(item.path);
    check(hash(original.bytes) === item.sha256, 'reference-input-refused');
    if (item.json) JSON.parse(original.bytes);
    else check(original.bytes.subarray(0,8).equals(Buffer.from('89504e470d0a1a0a','hex')), 'reference-input-refused');
    if (state) {
      const staged = readFile(path.join(state.directory,item.name)), pin = state.pins[item.name];
      check(staged.dev === pin.dev && staged.ino === pin.ino && hash(staged.bytes) === item.sha256, 'reference-input-refused');
    }
  }
}
function prepareWorkspace(config, reportGuard) {
  authorize(config); check(!states.has(config), 'reference-workspace-reused');
  reportGuard.verify(); verifyInputs(config);
  const directory = path.join(reportGuard.directory,'files');
  // One exclusively created directory: outputs can never overwrite an input or
  // pre-existing user file. A failed preparation consumes this run's journal.
  fs.mkdirSync(directory,{mode:0o700});
  const root = fs.lstatSync(directory), pins = {};
  const guard = { verify() {
    reportGuard.verify(); const now = fs.lstatSync(directory);
    check(now.isDirectory() && !now.isSymbolicLink() && fs.realpathSync(directory) === directory &&
      now.dev === root.dev && now.ino === root.ino, 'reference-output-directory-changed');
  } };
  for (const item of inputs(config)) {
    guard.verify(); const original = readFile(item.path);
    check(hash(original.bytes) === item.sha256, 'reference-input-refused');
    const destination = path.join(directory,item.name);
    fs.writeFileSync(destination, original.bytes, {flag:'wx',mode:0o400});
    const staged = readFile(destination); pins[item.name] = {dev:staged.dev,ino:staged.ino};
  }
  const exportDirectory=path.join(directory,'Export'); fs.mkdirSync(exportDirectory,{mode:0o700});
  const exportPin=fs.lstatSync(exportDirectory),verifyParent=guard.verify;
  guard.verify=()=>{verifyParent();const current=fs.lstatSync(exportDirectory);
    check(current.isDirectory() && !current.isSymbolicLink() && fs.realpathSync(exportDirectory)===exportDirectory &&
      current.dev===exportPin.dev && current.ino===exportPin.ino,'reference-output-directory-changed');};
  states.set(config, {directory,exportDirectory,guard,pins}); verifyInputs(config);
  return { directory, inputs:inputs(config).map(item=>({label:item.label,path:path.join(directory,item.name)})),
    saved:path.join(directory,'Saved.json'), exportDirectory, pngName:'frame_0001.png' };
}
function verifySet(config, action) {
  verifyInputs(config); const state = states.get(config);
  check(state && action.target === 'sheet' && typeof action.value === 'string', 'reference-path-refused');
  const allowed = inputs(config).map(item=>path.join(state.directory,item.name));
  // No filename-only output: the file panel's current directory is not evidence
  // that a save would target this owned run. Supply the exact absolute path.
  allowed.push(path.join(state.directory,'Saved.json'),state.exportDirectory);
  const saved = path.join(state.directory,'Saved.json');
  if (fs.existsSync(saved)) { readFile(saved); allowed.push(saved); }
  check(allowed.includes(action.value), 'reference-path-refused');
  for (const name of ['Saved.json']) {
    try { const stat=fs.lstatSync(path.join(state.directory,name));
      check(stat.isFile() && !stat.isSymbolicLink(), 'reference-output-refused');
    } catch(error) { if(error.code !== 'ENOENT') throw error; }
  }
}
function canonical(value) {
  if (Array.isArray(value)) return '['+value.map(canonical).join(',')+']';
  if (value && typeof value === 'object') return '{'+Object.keys(value).sort().map(k=>JSON.stringify(k)+':'+canonical(value[k])).join(',')+'}';
  return JSON.stringify(value);
}
function savedValue(value,config,state) {
  if(!config.normalizeGeneratedReferenceId)return value;
  // A first UI attachment allocates project-ref-<request serial>. Only this
  // generated token may vary; both persistent links must agree, and every other
  // saved field/byte value still participates in the predeclared semantic hash.
  const reference=value?.viewportReference,assets=value?.assets,id=reference?.assetId;
  check(reference?.kind==='still-image' && /^project-ref-[1-9][0-9]{0,15}$/.test(id || '') &&
    Array.isArray(assets) && assets.length===1 && assets[0]?.id===id,'reference-save-mismatch');
  check(!state.savedReferenceId || state.savedReferenceId===id,'reference-save-mismatch');
  return {...value,viewportReference:{...reference,assetId:'@GENERATED_REFERENCE_ID'},
    assets:[{...assets[0],id:'@GENERATED_REFERENCE_ID'}]};
}
function crc32(bytes) {
  let crc=0xffffffff;
  for(const byte of bytes) { crc^=byte; for(let i=0;i<8;i++) crc=(crc>>>1)^((crc&1)?0xedb88320:0); }
  return (crc^0xffffffff)>>>0;
}
function pngPixels(bytes, expected) {
  check(bytes.subarray(0,8).equals(Buffer.from('89504e470d0a1a0a','hex')), 'reference-png-refused');
  let offset=8, width, height, ended=false, dataStarted=false, dataEnded=false; const chunks=[];
  while(offset < bytes.length) {
    check(offset+12<=bytes.length && !ended, 'reference-png-refused');
    const size=bytes.readUInt32BE(offset), end=offset+12+size;
    check(end<=bytes.length, 'reference-png-refused');
    const type=bytes.toString('ascii',offset+4,offset+8), data=bytes.subarray(offset+8,end-4);
    check(crc32(bytes.subarray(offset+4,end-4))===bytes.readUInt32BE(end-4), 'reference-png-refused');
    if(type==='IHDR') {
      check(offset===8 && size===13, 'reference-png-refused');
      width=data.readUInt32BE(0); height=data.readUInt32BE(4);
      check(width===expected.width && height===expected.height &&
        data.subarray(8).equals(Buffer.from([8,6,0,0,0])), 'reference-png-refused');
    } else if(type==='IDAT') {
      check(width && !dataEnded, 'reference-png-refused'); dataStarted=true; chunks.push(data);
    } else if(type==='IEND') {
      check(size===0 && dataStarted && end===bytes.length, 'reference-png-refused'); ended=true;
    } else {
      // The accepted native encoder emits its fixed, metadata-free RGBA profile.
      check(false, 'reference-png-refused');
    }
    if(type!=='IDAT' && dataStarted) dataEnded=true;
    offset=end;
  }
  check(ended, 'reference-png-refused');
  const stride=width*4+1, compressed=Buffer.concat(chunks);
  const inflated=zlib.inflateSync(compressed,{maxOutputLength:stride*height,info:true}),raw=inflated.buffer;
  check(inflated.engine.bytesWritten===compressed.length, 'reference-png-refused');
  check(raw.length===stride*height, 'reference-png-refused');
  for(let y=0;y<height;y++) check(raw[y*stride]===0, 'reference-png-refused');
  return expected.pixels.map(p=>{
    const rgba=[...raw.subarray(p.y*stride+1+p.x*4,p.y*stride+5+p.x*4)];
    check(rgba.every((v,i)=>v===p.rgba[i]), 'reference-pixel-mismatch'); return {x:p.x,y:p.y,rgba};
  });
}
function artifacts(config) {
  verifyInputs(config); const state=states.get(config);
  check(state, 'reference-workspace-required'); state.guard.verify();
  const saved=readFile(path.join(state.directory,'Saved.json')).bytes;
  const value=JSON.parse(saved),normalized=savedValue(value,config,state);
  check(hash(canonical(normalized))===config.savedCanonicalSha256, 'reference-save-mismatch');
  // Ordinary export.js creates one bounded native directory below the selected
  // output folder; the native one-frame publisher names its file frame_0001.png.
  const children=fs.readdirSync(state.exportDirectory,{withFileTypes:true});
  check(children.length===1 && children[0].isDirectory() && !children[0].isSymbolicLink() &&
    /^nemo-native-opacity-[0-9]{13}-[0-9]{1,6}$/.test(children[0].name),'reference-output-refused');
  const published=path.join(state.exportDirectory,children[0].name);
  check(fs.realpathSync(published)===published && fs.readdirSync(published).join(',')==='frame_0001.png','reference-output-refused');
  const png=readFile(path.join(published,'frame_0001.png')).bytes;
  const pixels=pngPixels(png,config.exportOracle); state.guard.verify();
  if(config.normalizeGeneratedReferenceId)state.savedReferenceId=value.viewportReference.assetId;
  return {savedSha256:hash(saved),savedCanonicalSha256:config.savedCanonicalSha256,
    ...(state.savedReferenceId?{referenceId:state.savedReferenceId}:{}),
    pngSha256:hash(png),width:config.exportOracle.width,height:config.exportOracle.height,pixels};
}
module.exports={WORKFLOW,labels,errors,hash,check,authorize,readFile,verifyInputs,prepareWorkspace,verifySet,
  canonical,crc32,pngPixels,artifacts};
