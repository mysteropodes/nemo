#!/usr/bin/env node
'use strict';
// node tests/desktop/p38-owned-window-build.cjs PNG_UI /absolute/helper /absolute/proof.json
// Compile the two frozen sources, then run --self-test only. Never start an app.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const INPUTS=Object.freeze(['p38-owned-window.swift','p38-owned-projection.swift']);
const SCHEMAS=Object.freeze({PNG_UI:'nemo.png-ui-window-helper/1',P38:'nemo.p38-window-helper/1',REFERENCE:'nemo.reference-window-helper/1'});
function check(ok){if(!ok)throw Object.assign(Error('window-helper-source-refused'),{code:'window-helper-source-refused'});}
function hash(bytes){return crypto.createHash('sha256').update(bytes).digest('hex');}
function sourceManifest(directory=__dirname){
  const root=fs.realpathSync(directory);check(root===directory);
  const sources=INPUTS.map(name=>{
    const file=path.join(root,name);check(fs.realpathSync(file)===file);
    const fd=fs.openSync(file,fs.constants.O_RDONLY|fs.constants.O_NOFOLLOW|fs.constants.O_NONBLOCK);
    try{
      const before=fs.fstatSync(fd);check(before.isFile()&&before.size>0&&before.size<=256000);
      const bytes=fs.readFileSync(fd),after=fs.lstatSync(file);
      check(after.isFile()&&!after.isSymbolicLink()&&after.dev===before.dev&&after.ino===before.ino&&bytes.length===before.size);
      return {name,bytes,sha256:hash(bytes)};
    }finally{fs.closeSync(fd);}
  });
  const inputs=sources.map(x=>({path:'tests/desktop/'+x.name,sha256:x.sha256}));
  return {sources,inputs,sourceSha256:hash(inputs.map(x=>x.path+'\0'+x.sha256+'\n').join(''))};
}
function verifySourceProof(proof,directory=__dirname){
  const manifest=sourceManifest(directory);
  check(proof&&proof.sourceSha256===manifest.sourceSha256);
  try{assert.deepEqual(proof.inputs,manifest.inputs);}catch{check(false);}
  return manifest.sourceSha256;
}
function fresh(file){
  check(typeof file==='string'&&path.isAbsolute(file)&&path.normalize(file)===file);
  const parent=path.dirname(file),stat=fs.lstatSync(parent);
  check(stat.isDirectory()&&!stat.isSymbolicLink()&&fs.realpathSync(parent)===parent);
  try{fs.lstatSync(file);check(false);}catch(e){if(e.code!=='ENOENT')throw e;}
}
function buildHelper({workflow,output,proofFile}){
  check(SCHEMAS[workflow]);fresh(output);fresh(proofFile);check(output!==proofFile);
  const source=require('../../scripts/nemo/lib/identity.cjs').sourceIdentity();
  check(!source.dirty&&/^[a-f0-9]{40}$/.test(source.head));
  const manifest=sourceManifest(),scratch=fs.mkdtempSync(path.join(path.dirname(output),'.p38-helper-build-'));
  fs.chmodSync(scratch,0o700);
  let published=false;
  try{
    const files=manifest.sources.map(x=>{
      const file=path.join(scratch,x.name);fs.writeFileSync(file,x.bytes,{flag:'wx',mode:0o600});return file;
    });
    const executable=path.join(scratch,'helper');
    const compile=spawnSync('/usr/bin/swiftc',['-parse-as-library',...files,'-o',executable],{encoding:'utf8',timeout:120000,maxBuffer:65536});
    check(!compile.error&&compile.status===0);
    const test=spawnSync(executable,['--self-test'],{encoding:'utf8',timeout:30000,maxBuffer:16384});
    check(!test.error&&test.status===0);
    const result=JSON.parse(test.stdout);check(result.status==='static-model-pass'&&result.runtimeAX===false&&result.foreignReads===0);
    const after=require('../../scripts/nemo/lib/identity.cjs').sourceIdentity();
    check(!after.dirty&&after.head===source.head&&sourceManifest().sourceSha256===manifest.sourceSha256);
    fresh(output);fresh(proofFile);
    fs.copyFileSync(executable,output,fs.constants.COPYFILE_EXCL);published=true;fs.chmodSync(output,0o700);
    const proof={schema:SCHEMAS[workflow],sourceHead:source.head,sourceSha256:manifest.sourceSha256,
      inputs:manifest.inputs,executableSha256:hash(fs.readFileSync(output)),
      compiler:'swiftc -parse-as-library (two frozen inputs)',runtimeAX:false,foreignReads:0,staticSelfTest:result};
    fs.writeFileSync(proofFile,JSON.stringify(proof,null,2)+'\n',{flag:'wx',mode:0o600});
    return proof;
  }catch(e){
    if(published)fs.rmSync(output,{force:true});
    throw e;
  }finally{fs.rmSync(scratch,{recursive:true,force:true});}
}
if(require.main===module){
  try{
    check(process.platform==='darwin'&&process.argv.length===5);
    const proof=buildHelper({workflow:process.argv[2],output:process.argv[3],proofFile:process.argv[4]});
    process.stdout.write(JSON.stringify({schema:proof.schema,sourceHead:proof.sourceHead,sourceSha256:proof.sourceSha256,
      executableSha256:proof.executableSha256,runtimeAX:false})+'\n');
  }catch{process.stderr.write('Owned-window helper build refused; no app launch.\n');process.exitCode=1;}
}
module.exports={INPUTS,sourceManifest,verifySourceProof,buildHelper};
