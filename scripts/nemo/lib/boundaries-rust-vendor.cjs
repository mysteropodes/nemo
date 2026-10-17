'use strict';
// Exact E2 vendor classification, distinct from handwritten module ownership.
// The ordinary coverage and size/ratchet APIs are unchanged. No exclusions are
// added: each selected Rust input is explicitly ordinary, vendor, or a failure.
// A stable checkout is required, as for the existing source-size checker.
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { checkSourceCoverage } = require('./boundaries-coverage.cjs');
const POLICY = 'engineering/boundaries/profiles/rust.vendor.json';
const MANIFEST_SHA256 = '930b6c4fa140e28e2c97769ee988d8ada853362bc9010b4b37298e905d1c22b9';
const PACKAGE_ROOT = 'native-engine/third_party/rquickjs-sys-0.14.0';
const ARCHIVE_SHA256 = 'cee271d0eeba64f0915b846cb7ae02e16faf3dfdffdca91731101d9d30fe3423';
const PATCH_SHA256 = 'd869202059e4252b397419aa4023e807f5e67e438570ddb352bee18617fe8ede';
const PROVENANCE_SHA256 = '370de49f96abca5bb0fc562a3692f7673c126b5ba2dd5899bc71d56a8548ab92';
const sha256 = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');
function invalid(message) { throw new Error(`invalid Rust vendor accounting: ${message}`); }
function relative(value) {
  if (typeof value !== 'string' || !value || /[\\\x00-\x1f\x7f:]/.test(value)
    || path.posix.isAbsolute(value) || value.split('/').some((part) => !part || part === '.' || part === '..')) {
    invalid('path must be normalized and repository-relative');
  }
}
function keys(value, expected, label) {
  if (!value || typeof value !== 'object' || Array.isArray(value)
    || Object.keys(value).sort().join('\0') !== [...expected].sort().join('\0')) invalid(`${label} fields changed`);
}
// Exported to test malformed structured inputs separately. Enforcement ALSO
// requires the exact raw manifest hash; a caller cannot supply a replacement.
function validateVendorManifest(policy) {
  keys(policy, ['schemaVersion', 'package', 'root', 'archive', 'patch', 'provenance', 'files'], 'manifest');
  if (policy.schemaVersion !== 1 || policy.package !== 'rquickjs-sys@0.14.0' || policy.root !== PACKAGE_ROOT) invalid('package scope changed');
  keys(policy.archive, ['name', 'sha256', 'upstreamCommit'], 'archive');
  if (policy.archive.name !== 'rquickjs-sys-0.14.0.crate' || policy.archive.sha256 !== ARCHIVE_SHA256
    || policy.archive.upstreamCommit !== 'd7ef5eeae702fea24c03643064de454f1c1dd4b0') invalid('archive provenance changed');
  for (const [name, suffix, hash] of [['patch', 'quickjs.patch', PATCH_SHA256], ['provenance', 'PROVENANCE.md', PROVENANCE_SHA256]]) {
    keys(policy[name], ['path', 'sha256'], name);
    if (policy[name].path !== `${PACKAGE_ROOT}.${suffix}` || policy[name].sha256 !== hash) invalid(`${name} pin changed`);
  }
  if (!Array.isArray(policy.files) || policy.files.length !== 306) invalid('expected 306 preserved vendor files');
  let previous = '', rust = 0;
  const patched = [];
  for (const record of policy.files) {
    keys(record, ['path', 'upstreamSha256', 'sha256'], 'file');
    relative(record.path);
    if (record.path <= previous) invalid('file paths must be unique and sorted');
    previous = record.path;
    for (const hash of [record.sha256, record.upstreamSha256]) if (!/^[a-f0-9]{64}$/.test(hash)) invalid('file hash is invalid');
    if (record.path.endsWith('.rs')) {
      rust++;
      if (record.sha256 !== record.upstreamSha256) invalid('Rust vendor source differs from upstream');
    }
    if (record.sha256 !== record.upstreamSha256) patched.push(record.path);
  }
  if (rust !== 25 || patched.join('\0') !== 'quickjs/quickjs.c\0quickjs/quickjs.h') invalid('only the two declared C files may differ from upstream');
  return policy;
}
function regularFile(root, relativePath) {
  relative(relativePath);
  let absolute = root, stat;
  const parts = relativePath.split('/');
  for (const [index, part] of parts.entries()) {
    absolute = path.join(absolute, part);
    try { stat = fs.lstatSync(absolute, { bigint: true }); } catch { invalid(`missing file ${relativePath}`); }
    if (stat.isSymbolicLink()) invalid(`symlink is forbidden: ${relativePath}`);
    if (index < parts.length - 1 && !stat.isDirectory()) invalid(`non-directory parent: ${relativePath}`);
  }
  if (!stat.isFile()) invalid(`not a regular file: ${relativePath}`);
  return { absolute, identity: `${stat.dev}:${stat.ino}` };
}
function inspectVendor(root) {
  const policyFile = regularFile(root, POLICY);
  const bytes = fs.readFileSync(policyFile.absolute);
  if (sha256(bytes) !== MANIFEST_SHA256) invalid('manifest raw hash changed');
  const policy = validateVendorManifest(JSON.parse(bytes));
  const expected = new Map(policy.files.map((record) => [record.path, record]));
  const found = [];
  function walk(dir, prefix) {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
      if (entry.isDirectory()) walk(path.join(dir, entry.name), rel);
      else { if (!entry.isFile()) invalid(`non-regular vendor entry ${rel}`); found.push(rel); }
    }
  }
  // Validate all parent components before walking, including the vendor root.
  regularFile(root, `${PACKAGE_ROOT}/Cargo.toml`);
  walk(path.join(root, PACKAGE_ROOT), '');
  if (found.length !== expected.size || found.some((file) => !expected.has(file))) invalid('vendor inventory has missing or extra files');
  const identities = new Set(), rustPaths = [];
  for (const [file, record] of expected) {
    const fullPath = `${PACKAGE_ROOT}/${file}`;
    const state = regularFile(root, fullPath);
    if (identities.has(state.identity)) invalid(`aliased vendor file ${fullPath}`);
    identities.add(state.identity);
    if (sha256(fs.readFileSync(state.absolute)) !== record.sha256) invalid(`vendor bytes changed: ${fullPath}`);
    if (file.endsWith('.rs')) rustPaths.push(fullPath);
  }
  for (const name of ['patch', 'provenance']) {
    const record = policy[name], state = regularFile(root, record.path);
    if (identities.has(state.identity)) invalid(`aliased ${name} file`);
    identities.add(state.identity);
    if (sha256(fs.readFileSync(state.absolute)) !== record.sha256) invalid(`${name} bytes changed`);
  }
  return { rustPaths, identities, integrity: { package: policy.package, preservedFileCount: found.length,
    upstreamIdenticalRustCount: rustPaths.length, archiveSha256: ARCHIVE_SHA256,
    patchSha256: PATCH_SHA256, provenanceSha256: PROVENANCE_SHA256, manifestSha256: MANIFEST_SHA256 } };
}
function checkRustSourceCoverage(profile, opts = {}) {
  if (!opts || typeof opts !== 'object' || Array.isArray(opts)) invalid('options must be an object');
  for (const key of Object.keys(opts)) if (!['root', 'sourcePaths', 'exclusions'].includes(key)) invalid(`unknown option ${key}`);
  if (!Array.isArray(opts.sourcePaths) || !opts.sourcePaths.every((file) => typeof file === 'string' && file.endsWith('.rs'))) invalid('sourcePaths must contain only Rust paths');
  if (opts.exclusions !== undefined && (!Array.isArray(opts.exclusions) || opts.exclusions.length)) invalid('Rust exclusions must stay empty');
  if (opts.root !== undefined && (typeof opts.root !== 'string' || !opts.root.trim())) invalid('invalid root');
  const root = fs.realpathSync(opts.root === undefined ? process.cwd() : opts.root);
  if (!fs.statSync(root).isDirectory()) invalid('root must be a directory');
  const vendor = inspectVendor(root), vendorPaths = new Set(vendor.rustPaths);
  const declared = profile.modules.flatMap((module) => module.files.map((file) => path.posix.join(module.dir, file)));
  if (declared.some((file) => vendorPaths.has(file))) invalid('vendor source overlaps handwritten module ownership');
  const selected = new Set();
  for (const file of opts.sourcePaths) {
    relative(file);
    if (selected.has(file)) invalid(`duplicate selected path ${file}`);
    selected.add(file);
  }
  for (const file of vendorPaths) if (!selected.has(file)) invalid(`vendor Rust input missing from independent census: ${file}`);
  const ordinary = opts.sourcePaths.filter((file) => !vendorPaths.has(file));
  for (const file of ordinary) {
    if (vendor.identities.has(regularFile(root, file).identity)) invalid(`handwritten source aliases vendor bytes: ${file}`);
  }
  const report = checkSourceCoverage(profile, { root, sourcePaths: ordinary, exclusions: [] });
  return { ...report, sourcePathCount: selected.size, handwrittenSourcePathCount: ordinary.length,
    vendorSourcePathCount: vendorPaths.size, vendorSourcePaths: [...vendorPaths], vendorIntegrity: vendor.integrity };
}
module.exports = { checkRustSourceCoverage, validateVendorManifest };
