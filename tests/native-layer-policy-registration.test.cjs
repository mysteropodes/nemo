'use strict';
// F06 ordinary registration. Run only on the actually composed candidate.
const test=require('node:test'),assert=require('node:assert/strict');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {spawnSync}=require('node:child_process');
const {checkRustSourceCoverage}=require('../scripts/nemo/lib/boundaries-rust-vendor.cjs');
const {discoverRepositoryFiles}=require('../scripts/nemo/lib/boundaries-repository.cjs');
const ROOT=path.resolve(__dirname,'..');
const PROFILE='engineering/boundaries/profiles/rust.profile.json';
const MCP='engineering/boundaries/profiles/mcp-rust.profile.json';
const COVERAGE='engineering/boundaries/profiles/rust.coverage.json';
const MEMBERS=[
  [
    "rust.native.engine.document",
    "native-engine/src",
    "object_layer_organization.rs"
  ],
  [
    "rust.native.engine.document",
    "native-engine/src",
    "object_layer_policy.rs"
  ],
  [
    "rust.native.engine.commands",
    "native-engine/src",
    "object_layer_policy_edit.rs"
  ],
  [
    "rust.native.engine.history",
    "native-engine/src",
    "object_layer_policy_history.rs"
  ],
  [
    "rust.native.engine.tests",
    "native-engine/tests",
    "object_layer_organization.rs"
  ],
  [
    "rust.native.engine.tests",
    "native-engine/tests",
    "object_layer_lock_policy.rs"
  ],
  [
    "rust.native.engine.tests",
    "native-engine/tests",
    "object_layer_lock_project.rs"
  ],
  [
    "rust.native.engine.tests",
    "native-engine/tests",
    "object_layer_policy_edit.rs"
  ],
  [
    "rust.mcp.transport",
    "nemo-mcp",
    "src/native_diagnostics_contract.rs"
  ],
  [
    "rust.mcp.transport",
    "nemo-mcp",
    "src/native_layer_policy_contract.rs"
  ],
  [
    "rust.mcp.transport",
    "nemo-mcp",
    "src/native_layer_policy_wire_tests.rs"
  ],
  [
    "rust.desktop.native.host.release.tests",
    "src-tauri/src",
    "native_layer_policy_dispatch_tests.rs"
  ]
];
const CODE=[
  "engineering/application/capabilities-v2/native-layer-policy.json",
  "engineering/application/native-transport-v2.schema.json",
  "native-engine/src/commands.rs",
  "native-engine/src/object_codec.rs",
  "native-engine/src/object_command_dispatch.rs",
  "native-engine/src/object_document.rs",
  "native-engine/src/object_fill.rs",
  "native-engine/src/object_fill_owner.rs",
  "native-engine/src/object_history.rs",
  "native-engine/src/object_layer_organization.rs",
  "native-engine/src/object_layer_policy.rs",
  "native-engine/src/object_layer_policy_edit.rs",
  "native-engine/src/object_layer_policy_history.rs",
  "native-engine/src/object_order_codec.rs",
  "native-engine/src/object_order_document.rs",
  "native-engine/src/object_project_history.rs",
  "native-engine/src/object_revision.rs",
  "native-engine/src/project_media_envelope.rs",
  "native-engine/tests/object_codec.rs",
  "native-engine/tests/object_layer_create.rs",
  "native-engine/tests/object_layer_lock_policy.rs",
  "native-engine/tests/object_layer_lock_project.rs",
  "native-engine/tests/object_layer_organization.rs",
  "native-engine/tests/object_layer_policy_edit.rs",
  "native-engine/tests/property_expression_views.rs",
  "nemo-mcp/src/capabilities.rs",
  "nemo-mcp/src/native_contract.rs",
  "nemo-mcp/src/native_diagnostics_contract.rs",
  "nemo-mcp/src/native_layer_policy_contract.rs",
  "nemo-mcp/src/native_layer_policy_wire_tests.rs",
  "nemo-mcp/src/native_request_input.rs",
  "nemo-mcp/src/wire.rs",
  "src/js/adapters/native-application.js",
  "src-tauri/src/native_layer_policy_dispatch_tests.rs",
  "src-tauri/src/native_object_client.rs",
  "src-tauri/src/native_revision_dispatch.rs",
  "tests/native-layer-policy-public.test.cjs"
];
const CANDIDATES=[PROFILE,MCP,COVERAGE,...CODE];
const read=(root,file)=>JSON.parse(fs.readFileSync(path.join(root,file),'utf8'));
function coverage(root,profile){
  const inventory=discoverRepositoryFiles({root});assert.equal(inventory.ok,true,JSON.stringify(inventory.diagnostics));
  const sourcePaths=inventory.entries.filter(entry=>entry.path.endsWith('.rs')).map(entry=>entry.path);
  return checkRustSourceCoverage(profile,{root,sourcePaths,exclusions:read(root,COVERAGE).exclusions});
}
test('F06 private children retain exact ordinary ownership and the single declared public policy operation',()=>{
  const profile=read(ROOT,PROFILE),mcp=read(ROOT,MCP);
  for(const [owner,dir,file]of MEMBERS){
    const module=profile.modules.find(entry=>entry.id===owner);
    assert.equal(module.dir,dir);assert.equal(module.files.filter(entry=>entry===file).length,1);
    assert.equal(module.publicApi.includes(file),false,`${dir}/${file} remains private`);
  }
  const transport=mcp.modules.find(entry=>entry.id==='mcp.transport');
  for(const [, ,file]of MEMBERS.filter(([owner])=>owner==='rust.mcp.transport')){
    assert.equal(transport.files.filter(entry=>entry===file).length,1);
    assert.equal(transport.publicApi.includes(file),false);
  }
  const policy=read(ROOT,COVERAGE);assert.deepEqual(policy.exclusions,[]);
  assert.match(policy.provenance.nativeLayerPolicyOrganizationRegistration,/F06\/#2147/);
  const descriptor=read(ROOT,'engineering/application/capabilities-v2/native-layer-policy.json');
  assert.deepEqual(descriptor.operations,['command.document.object.layer.policy.edit']);
  const sources=fs.readFileSync(path.join(ROOT,'nemo-mcp/src/capabilities.rs'),'utf8');
  assert.equal(sources.split('capabilities-v2/native-layer-policy.json').length-1,1);
  const schema=read(ROOT,'engineering/application/native-transport-v2.schema.json');
  const operations=schema.$defs.Request.oneOf.flatMap(branch=>{
    const declaration=branch.allOf[1].properties.operation;return declaration.enum||[declaration.const];
  });
  assert.equal(operations.filter(operation=>operation==='command.document.object.layer.policy.edit').length,1);
  assert.equal(operations.filter(operation=>operation==='command.document.object.undo').length,1);
  assert.equal(operations.filter(operation=>operation==='command.document.object.redo').length,1);
});
test('F06 full-inventory clone refuses each missing private membership and restores exact bytes',(t)=>{
  const temp=fs.mkdtempSync(path.join(os.tmpdir(),'nemo-f06-registration-'));
  t.after(()=>fs.rmSync(temp,{recursive:true,force:true,maxRetries:5,retryDelay:100}));
  const clone=path.join(temp,'repo');
  const git=spawnSync('git',['clone','--quiet','--local',ROOT,clone],{encoding:'utf8'});
  assert.equal(git.status,0,git.stderr);
  const source=new Map(CANDIDATES.map(file=>[file,fs.readFileSync(path.join(ROOT,file))]));
  for(const [file,bytes]of source){const target=path.join(clone,file);fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,bytes);}
  const original=fs.readFileSync(path.join(clone,PROFILE));
  assert.equal(coverage(clone,read(clone,PROFILE)).ok,true);
  for(const [owner,dir,file]of MEMBERS){
    try{
      const missing=JSON.parse(original);const module=missing.modules.find(entry=>entry.id===owner);
      module.files=module.files.filter(entry=>entry!==file);
      fs.writeFileSync(path.join(clone,PROFILE),JSON.stringify(missing));
      const refused=coverage(clone,read(clone,PROFILE));
      assert.deepEqual(refused.violations.map(entry=>[entry.rule,entry.file]),[['coverage-unprofiled-source',`${dir}/${file}`]]);
    }finally{fs.writeFileSync(path.join(clone,PROFILE),original);}
    assert.equal(coverage(clone,read(clone,PROFILE)).ok,true);
    assert.deepEqual(fs.readFileSync(path.join(clone,PROFILE)),original);
  }
  for(const [file,bytes]of source){
    assert.deepEqual(fs.readFileSync(path.join(clone,file)),bytes,`clone restoration ${file}`);
    assert.deepEqual(fs.readFileSync(path.join(ROOT,file)),bytes,`source unchanged ${file}`);
  }
});
