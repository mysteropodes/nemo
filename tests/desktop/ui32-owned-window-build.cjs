#!/usr/bin/env node
'use strict';
// Explicit UI32-only helper build. Existing three-purpose recipes are unchanged.
// This compiles frozen sources and runs static controls only; never launches Nemo.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');
const original=require('./p38-owned-window-build.cjs');
const EXTRA=Object.freeze(['ui32-owned-policy.swift','ui32-owned-window.swift','ui32-owned-fixture.swift','ui32-owned-keys.swift','ui32-owned-picker.swift','ui32-owned-readiness.swift','ui32-owned-profile.swift','ui32-owned-phase2.swift','ui32-owned-content.swift','f03-owned-opacity.swift','f03-owned-opacity-controls.swift','f03-owned-save.swift','f03-owned-save-controls.swift','ui32-owned-fixture-controls.swift','ui32-owned-native-busy.swift','ui32-owned-camera.swift','cs7-input.swift']);
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function requireValid(ok){if(!ok)throw Error('ui32-helper-build-refused');}
function fresh(file){
  requireValid(typeof file==='string'&&path.isAbsolute(file)&&path.normalize(file)===file);
  const parent=path.dirname(file),s=fs.lstatSync(parent);
  requireValid(s.isDirectory()&&!s.isSymbolicLink()&&fs.realpathSync(parent)===parent);
  try{fs.lstatSync(file);throw Error('ui32-helper-build-refused');}catch(e){if(e.code!=='ENOENT')throw e;}
}
function manifest(directory=__dirname){
  requireValid(fs.realpathSync(directory)===directory);const base=original.sourceManifest(directory);
  const extra=EXTRA.map(name=>{
    const file=path.join(directory,name);requireValid(fs.realpathSync(file)===file);
    const fd=fs.openSync(file,fs.constants.O_RDONLY|fs.constants.O_NOFOLLOW|fs.constants.O_NONBLOCK);
    try{
      const before=fs.fstatSync(fd);requireValid(before.isFile()&&before.size>0&&before.size<=32000);
      const bytes=fs.readFileSync(fd),after=fs.lstatSync(file);
      requireValid(bytes.toString('utf8').split(/\r?\n/).filter(line=>line.trim()).length<=500);
      requireValid(after.isFile()&&!after.isSymbolicLink()&&after.dev===before.dev&&after.ino===before.ino&&bytes.length===before.size);
      return {name,bytes,sha256:hash(bytes)};
    }finally{fs.closeSync(fd);}
  });
  const sources=[...base.sources,...extra],inputs=sources.map(x=>({path:'tests/desktop/'+x.name,sha256:x.sha256}));
  return {sources,inputs,sourceSha256:hash(inputs.map(x=>x.path+'\0'+x.sha256+'\n').join(''))};
}
function buildUI32Helper(output,proofFile){
  requireValid(process.platform==='darwin');fresh(output);fresh(proofFile);requireValid(output!==proofFile);
  const identity=require('../../scripts/nemo/lib/identity.cjs');
  const source=identity.sourceIdentity();requireValid(!source.dirty&&/^[a-f0-9]{40}$/.test(source.head));
  const frozen=manifest(),scratch=fs.mkdtempSync(path.join(path.dirname(output),'.ui32-helper-'));
  fs.chmodSync(scratch,0o700);let published=false;
  try{
    const files=frozen.sources.map(x=>{const p=path.join(scratch,x.name);fs.writeFileSync(p,x.bytes,{flag:'wx',mode:0o600});return p;});
    const executable=path.join(scratch,'ui32-helper');
    const compile=spawnSync('/usr/bin/swiftc',['-parse-as-library','-D','UI32_VALIDATION',...files,'-o',executable],
      {encoding:'utf8',timeout:120000,maxBuffer:65536});
    requireValid(!compile.error&&compile.status===0);
    const controls=[];
    for(const flag of ['--self-test','--ui32-self-test']){
      const test=spawnSync(executable,[flag],{encoding:'utf8',timeout:30000,maxBuffer:16384});
      requireValid(!test.error&&test.status===0);const result=JSON.parse(test.stdout);
      requireValid(result.status===(flag==='--self-test'?'static-model-pass':'ui32-static-model-pass')&&result.runtimeAX===false&&result.foreignReads===0);
      if(flag==='--ui32-self-test')requireValid(result.f03?.opacity===140&&result.f03?.save===126&&result.f03?.total===266&&result.f03?.join===107&&result.f03?.all===373);
      controls.push({flag,result});
    }
    const after=identity.sourceIdentity();requireValid(!after.dirty&&after.head===source.head&&manifest().sourceSha256===frozen.sourceSha256);
    fresh(output);fresh(proofFile);fs.copyFileSync(executable,output,fs.constants.COPYFILE_EXCL);published=true;fs.chmodSync(output,0o700);
    const proof={schema:'nemo.ui32-window-helper/1',purpose:'UI32',sourceHead:source.head,sourceSha256:frozen.sourceSha256,
      inputs:frozen.inputs,executableSha256:hash(fs.readFileSync(output)),compiler:'swiftc -parse-as-library -D UI32_VALIDATION (twenty frozen inputs)',
      runtimeAX:false,foreignReads:0,controls};
    fs.writeFileSync(proofFile,JSON.stringify(proof,null,2)+'\n',{flag:'wx',mode:0o600});return proof;
  }catch(e){if(published)fs.rmSync(output,{force:true});throw e;}
  finally{fs.rmSync(scratch,{recursive:true,force:true});}
}
if(require.main===module){
  try{
    requireValid(process.argv.length===4);const proof=buildUI32Helper(process.argv[2],process.argv[3]);
    process.stdout.write(JSON.stringify({schema:proof.schema,purpose:proof.purpose,sourceHead:proof.sourceHead,
      sourceSha256:proof.sourceSha256,executableSha256:proof.executableSha256,runtimeAX:false})+'\n');
  }catch{process.stderr.write('UI32 helper build refused; no app launch.\n');process.exitCode=1;}
}
module.exports={manifest,buildUI32Helper};
