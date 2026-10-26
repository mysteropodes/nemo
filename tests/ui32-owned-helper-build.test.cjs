'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),crypto=require('node:crypto');
const B=require('./desktop/ui32-owned-window-build.cjs');
const inputs=['p38-owned-window.swift','p38-owned-projection.swift','p38-owned-children-query.swift',
  'ui32-owned-policy.swift','ui32-owned-window.swift','ui32-owned-fixture.swift','ui32-owned-keys.swift','ui32-owned-picker.swift','ui32-owned-readiness.swift','ui32-owned-profile.swift','ui32-owned-phase2.swift','ui32-owned-content.swift','f03-owned-opacity.swift','f03-owned-opacity-controls.swift','f03-owned-save.swift','f03-owned-save-controls.swift','ui32-owned-fixture-controls.swift','ui32-owned-native-busy.swift','ui32-owned-camera.swift','cs7-input.swift'];
test('UI32 manifest binds all twenty ordered source hashes without compilation or app input',()=>{
  const m=B.manifest();assert.deepEqual(m.inputs.map(x=>x.path),inputs.map(n=>'tests/desktop/'+n));
  const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
  for(const [i,input] of m.inputs.entries()){
    assert.equal(m.sources[i].name,inputs[i]);const disk=fs.readFileSync(path.join(__dirname,'desktop',inputs[i]));
    assert.equal(input.sha256,sha(disk));assert.deepEqual(m.sources[i].bytes,disk);
  }
  assert.equal(m.sourceSha256,sha(m.inputs.map(x=>x.path+'\0'+x.sha256+'\n').join('')));
});
test('UI32 extra source census refuses missing, aliased, empty, oversized and over-limit inputs',()=>{
  const root=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'ui32-source-proof-')));
  try{
    for(const name of inputs)fs.writeFileSync(path.join(root,name),'// '+name+'\n');
    const original=B.manifest(root);assert.equal(original.inputs.length,20);
    const ninth=path.join(root,'ui32-owned-readiness.swift'),saved=fs.readFileSync(ninth);
    fs.unlinkSync(ninth);assert.throws(()=>B.manifest(root));fs.writeFileSync(ninth,saved);
    assert.equal(B.manifest(root).sourceSha256,original.sourceSha256);
    const tenth=path.join(root,'ui32-owned-profile.swift'),profile=fs.readFileSync(tenth);
    fs.unlinkSync(tenth);assert.throws(()=>B.manifest(root));fs.writeFileSync(tenth,profile);
    assert.equal(B.manifest(root).sourceSha256,original.sourceSha256);
    for(const name of ['ui32-owned-phase2.swift','ui32-owned-content.swift','f03-owned-opacity.swift','f03-owned-opacity-controls.swift','f03-owned-save.swift','f03-owned-save-controls.swift','ui32-owned-fixture-controls.swift','ui32-owned-native-busy.swift','ui32-owned-camera.swift','cs7-input.swift']){
      const file=path.join(root,name),saved=fs.readFileSync(file);fs.unlinkSync(file);assert.throws(()=>B.manifest(root));
      fs.writeFileSync(file,saved);assert.equal(B.manifest(root).sourceSha256,original.sourceSha256);
    }
    const file=path.join(root,'ui32-owned-picker.swift');fs.appendFileSync(file,'// new content\n');
    assert.notEqual(B.manifest(root).sourceSha256,original.sourceSha256);
    fs.unlinkSync(file);assert.throws(()=>B.manifest(root));
    const other=path.join(root,'other');fs.writeFileSync(other,'// alias\n');fs.symlinkSync(other,file);
    assert.throws(()=>B.manifest(root));fs.unlinkSync(file);
    fs.writeFileSync(file,'');assert.throws(()=>B.manifest(root));
    fs.writeFileSync(file,'a'.repeat(32001));assert.throws(()=>B.manifest(root));
    fs.writeFileSync(file,'a\n'.repeat(501));assert.throws(()=>B.manifest(root));
  }finally{fs.rmSync(root,{recursive:true,force:true});}
});
