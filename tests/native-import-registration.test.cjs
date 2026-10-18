'use strict';
// L10-J: ordinary registration of the import adapters and of their public declaration. A pure file-read test for the COMPOSED candidate
// (it needs the lead-applied patches 1 to 13); it fails closed on every drift between the declaration, the profiles, the load order and the
// session hook. It reads files only.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.join(__dirname, '..');
const read = (rel) => fs.readFileSync(path.join(ROOT, rel), 'utf8');
const json = (rel) => JSON.parse(read(rel));
const OP = 'command.document.object.import.batch';
const FILES = ['native-import-contract.js', 'native-import-seam.js', 'native-import-core.js', 'native-import-svg.js', 'native-import-figma.js', 'native-import-drop.js', 'native-import-psd.js', 'native-import-montage.js', 'native-import-desktop.js'];
const LAYERS = { 'native-import-contract.js': 'domain', 'native-import-seam.js': 'domain', 'native-import-core.js': 'application', 'native-import-svg.js': 'application' };

test('the transport schema declares the closed payload, the receipt, the revisioned request branch and the result entry', () => {
  const schema = json('engineering/application/native-transport-v2.schema.json');
  for (const name of ['ObjectImportBatchPayload', 'ObjectImportBatchResult', 'ObjectImportVectorRecord', 'ObjectImportLayer', 'ObjectImportRecord', 'ObjectImportSkip', 'ObjectImportUnavailable']) {
    assert.equal(schema.$defs[name].type, 'object', name);
    assert.equal(schema.$defs[name].additionalProperties, false, name);
  }
  assert.equal(schema.$defs.ObjectImportBatchPayload.properties.command.const, 'object.import.batch');
  assert.equal(schema.$defs.ObjectImportVectorRecord.properties.target.properties.strokeId.const, '', 'the owner mints stroke ids');
  assert.equal(schema.$defs.ObjectImportVectorRecord.properties.geometry.properties.segments.maxItems, 256);
  const branches = schema.$defs.Request.oneOf.filter((b) => b.allOf && JSON.stringify(b).includes(OP));
  assert.equal(branches.length, 1);
  assert.equal(branches[0].allOf[0].$ref, '#/$defs/RequiredRevisionRequest');
  assert.equal(branches[0].allOf[1].properties.payload.$ref, '#/$defs/ObjectImportBatchPayload');
  const results = JSON.stringify(schema.$defs.ResponseBase.properties.result.oneOf);
  assert.ok(results.includes('#/$defs/ObjectImportBatchResult'));
});

test('the descriptor is embedded in the native catalog and the schema operations equal the descriptor operations', () => {
  const caps = read('nemo-mcp/src/capabilities.rs');
  assert.equal(caps.split('capabilities-v2/native-object-import.json').length, 2);
  assert.ok(fs.existsSync(path.join(ROOT, 'engineering/application/capabilities-v2/native-object-import.json')));
  const schema = read('engineering/application/native-transport-v2.schema.json');
  assert.equal(schema.split(`"${OP}"`).length, 2, 'declared once in the Request union');
});

test('the nemo-mcp validators are wired: module, request arm, result arm, revision requirement, correlation, profiles', () => {
  const contractRs = read('nemo-mcp/src/native_contract.rs');
  assert.match(contractRs, /#\[path = "native_import_contract\.rs"\]\npub\(crate\) mod import;/);
  assert.match(contractRs, /import::OP => import::request\(payload\),/);
  assert.match(contractRs, /if operation == import::OP \{\n\s+return import::result\(result\);/);
  assert.match(read('nemo-mcp/src/native_request_input.rs'), /crate::native_contract::import::admitted\(&self\.operation\)/);
  const wire = read('nemo-mcp/src/wire.rs');
  assert.match(wire, /#\[path = "native_wire_response_owner\.rs"\]\s*mod response_owner;/);
  assert.equal(wire.split('let correlated_identity = response_owner::correlated(request, response);').length, 2);
  assert.match(wire, /let identity_is_valid = correlated_identity\s*&& response\.api_version == NATIVE_API_VERSION/);
  assert.match(wire, /if identity_is_valid && disposition_is_valid/);
  const responseOwner = read('nemo-mcp/src/native_wire_response_owner.rs');
  const correlatedOperation = responseOwner.match(/let correlated_operation = ([^;]+);/);
  assert.ok(correlatedOperation, 'the response owner declares the correlated operation set');
  assert.match(correlatedOperation[1], /(?:^|\|\|)\s*crate::native_contract::import::admitted\(&request\.operation\)\s*(?:\|\||$)/);
  const correlatedIdentity = responseOwner.match(/let correlated_identity = ([\s\S]*?);\s*correlated_identity/);
  assert.ok(correlatedIdentity, 'the correlated operation set gates the returned identity');
  assert.match(correlatedIdentity[1], /^!correlated_operation\s*\|\| \(response\.request_id == request\.request_id\s*&&/);
  assert.match(correlatedIdentity[1], /\.unwrap_or\(\s*response\.instance_id == request\.instance_id\s*&& \(response\.document_id == request\.document_id/);
  assert.match(correlatedIdentity[1], /\|\| \(!response\.ok\s*&& response[\s\S]*?e\.code == "wrong_document"\)/);
  assert.match(correlatedIdentity[1], /&& serde_json::to_vec\(response\)\.is_ok_and\(\|b\| b\.len\(\) <= NATIVE_MAX_MESSAGE_BYTES\)\)$/);
  const nativeCallStart = wire.indexOf('pub async fn call_native(');
  const nativeCallEnd = wire.indexOf('\nfn validate_native_response(');
  assert.ok(nativeCallStart >= 0 && nativeCallEnd > nativeCallStart);
  const nativeCall = wire.slice(nativeCallStart, nativeCallEnd);
  assert.match(nativeCall, /validate_native_response\(&response, &response_contract\)\?;/);
  assert.match(nativeCall, /response\.request_id != request_id\s*\|\| response\.instance_id != endpoint\.instance_id/);
  assert.match(wire, /crate::native_contract::import::response_matches\(request, response\)/);
  for (const profile of ['engineering/boundaries/profiles/mcp-rust.profile.json', 'engineering/boundaries/profiles/rust.profile.json']) {
    const text = read(profile);
    const policy = JSON.parse(text);
    const moduleId = profile.endsWith('/mcp-rust.profile.json') ? 'mcp.transport' : 'rust.mcp.transport';
    const owners = policy.modules.filter((module) => module.id === moduleId);
    assert.equal(owners.length, 1, moduleId);
    assert.equal(owners[0].dir, 'nemo-mcp');
    assert.equal(owners[0].layer, 'adapters');
    for (const file of ['src/native_import_contract.rs', 'src/native_import_contract_tests.rs', 'src/native_wire_response_owner.rs']) {
      assert.equal(text.split(`"${file}"`).length, 2, `${profile} lists ${file} once`);
      assert.equal(owners[0].files.filter((member) => member === file).length, 1, `${moduleId} owns ${file}`);
      assert.equal(owners[0].publicApi.includes(file), false, `${file} remains private`);
    }
  }
  for (const file of ['nemo-mcp/src/native_import_contract.rs', 'nemo-mcp/src/native_import_contract_tests.rs']) assert.ok(read(file).split('\n').length <= 500, file);
  assert.ok(json('engineering/boundaries/profiles/rust.coverage.json').provenance.nativeImportPublicContractRegistration);
});

test('the app-js profile declares the nine modules with their layers and the load order puts contract and seam before the SDK', () => {
  const profile = json('engineering/boundaries/profiles/app-js.profile.json');
  for (const file of FILES) {
    const rows = profile.modules.filter((m) => m.files.includes(file));
    assert.equal(rows.length, 1, file);
    assert.equal(rows[0].dir, 'src/js/adapters');
    assert.equal(rows[0].layer, LAYERS[file] || (file === 'native-import-desktop.js' || file.startsWith('native-import-') ? 'adapters' : null), file);
    assert.deepEqual(rows[0].publicApi, [file]);
  }
  const html = read('src/index.html');
  const at = (file) => html.indexOf(`<script src="js/adapters/${file}"></script>`);
  const order = [at('native-font-contract.js'), at('native-import-contract.js'), at('native-import-seam.js'), at('native-application.js'), at('native-import-core.js'), at('native-import-svg.js'), at('native-import-drop.js'),
    at('native-import-figma.js'), at('native-import-psd.js'), at('native-import-montage.js'), at('native-import-desktop.js')];
  assert.ok(order.every((i) => i >= 0), 'every script tag exists once');
  assert.deepEqual([...order].sort((a, b) => a - b), order, 'font contract < contract < seam < SDK < core < svg < drop < figma < psd < montage < desktop');
  for (const file of FILES) assert.equal(html.split(`js/adapters/${file}"`).length, 2, `${file} is loaded once`);
});

test('the legacy importer scripts are all loaded or all gone (the cutover is one decision), the retained ag-psd loader stays, and the SDK injects the import seam', () => {
  const html = read('src/index.html');
  const legacy = ['svg-import.js', 'drop-import.js', 'figma-import.js'].map((f) => html.includes(`<script src="js/${f}"></script>`));
  assert.ok(legacy.every(Boolean) || legacy.every((x) => !x), `partial cutover: ${legacy}`);
  assert.ok(html.includes('<script src="js/psd-import-bridge.js"></script>'), 'the retained loader keeps its script tag in both states');
  const bridge = read('src/js/psd-import-bridge.js');
  if (legacy.every((x) => !x)) {
    // Cutover applied: the bridge is reduced to the one literal vendor import and the loader port; no legacy writer or panel listener remains.
    assert.equal(bridge.split("import('./ag-psd.vendor.mjs')").length, 2, 'the one literal dynamic import of the unprofiled vendor module');
    assert.match(bridge, /window\.SMAgPsdLoader = \{ load: loadModule \};/);
    for (const gone of ['importChildren', 'importFile', 'SMPsdImport', 'addEventListener', 'createUserLayer', 'saveActiveLayerFrame']) assert.equal(bridge.includes(gone), false, `${gone} is gone`);
    assert.equal(bridge.split('\n')[29].includes("import('./ag-psd.vendor.mjs')"), true, 'the import site stays on line 30');
  }
  for (const file of FILES) assert.equal(/\bimport\s*\(\s*['"`]/.test(read(`src/js/adapters/${file}`).replace(/\/\/.*$/gm, '')), false, `${file} carries no dynamic import`);
  const sdk = read('src/js/adapters/native-application.js');
  assert.match(sdk, /\[[^\]]*require\('\.\/native-import-contract\.js'\) : root\.NemoNativeImportContract[^\]]*\]\.reduce\(\(base, lane\) => \(lane \? lane\.compose\(base\) : base\)/, 'the import contract is composed in the lane reduce of the single SDK line');
  assert.ok(sdk.replace(/\n$/, '').split('\n').length <= 502, 'the SDK did not grow (it is at its ceiling at the anchor base)');
});

test('the session exposes the pinned import mutation only while presented, with the stale-intent check of createLayer', () => {
  const project = read('src/js/adapters/native-object-project.js');
  assert.match(project, /function importBatch\(payload,expected\)\{return mutation\(function\(pin\)\{\n\s+if\(!same\(pin,expected\)/);
  assert.match(project, /operation:'command\.document\.object\.import\.batch',expectedRevision:pin\.contentRevision/);
  assert.match(project, /get importBatch\(\)\{return subscription&&!synchronizing&&active\(\)&&presentedSession\(\)&&ports\.projectionReady\(info\)&&/);
  assert.ok(project.replace(/\n$/, '').split('\n').length <= 500);
  assert.match(project, /'nemo_native_dispatch'\);\}\n\s+function editLayerPolicy/);
});

test('the real common SDK (seam injected) admits the example, correlates its receipt and rejects every drift', async () => {
  const sdk = require('../src/js/adapters/native-application.js');
  const example = json('engineering/application/capabilities-v2/native-object-import.json').examples[0];
  const respond = (edit) => async (request) => { const r = JSON.parse(JSON.stringify(example.response)); r.requestId = request.requestId; if (edit) edit(r); return r; };
  const adapter = (edit) => sdk.createNativeApplicationAdapter('ui', { dispatch: respond(edit) });
  const request = () => JSON.parse(JSON.stringify(example.request));
  const reply = await adapter().dispatch(request());
  assert.equal(reply.contentRevision, 8);
  assert.equal(reply.result.objects[0].strokeId, 'native-stroke:8:0');
  const refused = async (edit, req, label) => assert.rejects(async () => adapter(edit).dispatch(req || request()), TypeError, label);
  await refused((r) => { r.contentRevision = 9; }, undefined, 'revision advanced twice');
  await refused((r) => { r.result.applied = false; r.result.historyEntriesAdded = 0; }, undefined, 'no-op receipt but revision advanced');
  await refused((r) => { r.instanceId = 'instance-B'; }, undefined, 'foreign owner');
  await refused((r) => { r.result.document = {}; }, undefined, 'extra result member');
  await refused((r) => { r.result.objects[0].strokeId = ''; }, undefined, 'empty minted id');
  const noRevision = request(); delete noRevision.expectedRevision;
  await refused(undefined, noRevision, 'revision required');
  const extra = request(); extra.payload.extra = 1;
  await refused(undefined, extra, 'unknown payload member');
  const preset = request(); preset.payload.records[0].record.target.strokeId = 'x';
  await refused(undefined, preset, 'caller-chosen stroke id');
  const big = request();
  for (let i = 0; i < 6; i += 1) big.payload.records.push({ sourceId: `extra:${i}`, record: JSON.parse(JSON.stringify(big.payload.records[0].record)) });
  await refused(undefined, big, 'over 4096 bytes');
  const fontOp = request(); fontOp.operation = 'command.document.object.asset.font.import';
  await refused(undefined, fontOp, 'the font contract still owns its own operations');
  const named = request(); named.payload.layers[0].name = 'data:image/png;base64,AAAA';
  const ok = await adapter().dispatch(named);
  assert.equal(ok.ok, true, 'opaque authored strings may start with data:');
});
