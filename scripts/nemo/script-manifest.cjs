#!/usr/bin/env node
'use strict';
// CS3c/#2383 — src/index.html's body script region is generated from engineering/application/script-manifest.json.
//   node scripts/nemo/script-manifest.cjs --check                    exit 1 when the region differs from the manifest
//   node scripts/nemo/script-manifest.cjs --write                    rewrite the region from the manifest
//   node scripts/nemo/script-manifest.cjs --add js/x.js [--before js/y.js | --after js/y.js] [--module]
//                                                                    insert one entry (default: last) and rewrite
//   node scripts/nemo/script-manifest.cjs --extract                  rebuild the manifest from the current region
// Adding an adapter therefore needs no hand edit of index.html, ordinals, lines or counts: the coverage pins that
// record each tag's line and ordinal are derived by scripts/nemo/refresh-app-pins.cjs. Module-boundary policy is
// unchanged (profile membership and a coverage retained-source entry). The two head tags (gpu-gate.js, Paper.js)
// stay outside the region; tests/script-manifest.test.cjs pins them.
const fs = require('node:fs');
const path = require('node:path');
const M = require('./lib/script-manifest.cjs');

const ROOT = path.resolve(__dirname, '..', '..');
const HTML = 'src/index.html';

function main(argv, root = ROOT, out = console) {
  const html = fs.readFileSync(path.join(root, HTML), 'utf8');
  const manifestPath = path.join(root, M.MANIFEST);
  const option = (name) => {
    const at = argv.indexOf(name);
    if (at < 0) return undefined;
    const value = argv[at + 1];
    if (!value || value.startsWith('--')) throw new Error(`script-manifest: ${name} needs a value`);
    return value;
  };
  if (argv.includes('--extract')) {
    const lines = M.split(html).region.split('\n').slice(1, -1);
    const previous = fs.existsSync(manifestPath) ? JSON.parse(fs.readFileSync(manifestPath, 'utf8')) : {};
    const manifest = { format: 'nemo.script-manifest', formatVersion: 1, html: HTML,
      note: previous.note || 'Body <script> region of src/index.html in load order; written into the page by scripts/nemo/script-manifest.cjs.',
      entries: M.extract(lines) };
    fs.writeFileSync(manifestPath, M.format(M.parse(manifest)));
    out.log(`script manifest extracted: ${manifest.entries.length} entries`);
    return 0;
  }
  const manifest = M.readManifest(root);
  if (argv.includes('--add')) {
    const src = option('--add'), before = option('--before'), after = option('--after');
    const entry = argv.includes('--module') ? { src, type: 'module' } : { src };
    if (!M.validEntry(entry)) throw new Error(`script-manifest: invalid script path ${src}`);
    if (!fs.existsSync(path.join(root, 'src', src))) throw new Error(`script-manifest: src/${src} does not exist`);
    const index = (name) => {
      const at = manifest.entries.findIndex((e) => e.src === name);
      if (at < 0) throw new Error(`script-manifest: ${name} is not listed`);
      return at;
    };
    const at = before ? index(before) : after ? index(after) + 1 : manifest.entries.length;
    manifest.entries.splice(at, 0, entry);
    M.parse(manifest);
    const page = M.write(html, manifest), text = M.format(manifest);   // both computed before either is written
    fs.writeFileSync(manifestPath, text);
    fs.writeFileSync(path.join(root, HTML), page);
    out.log(`script manifest: added ${src} at entry ${at}. Still yours: its app-js.profile.json membership and an ` +
      'app-js.coverage.json retainedSources entry (a loadSites stub is enough), then commit and run refresh-app-pins.cjs and inventory --check.');
    return 0;
  }
  if (argv.includes('--write')) {
    fs.writeFileSync(path.join(root, HTML), M.write(html, manifest));
    out.log('script region written');
    return 0;
  }
  if (argv.includes('--check')) {
    if (M.split(html).region !== M.render(manifest)) {
      out.error('script region of src/index.html is stale — run node scripts/nemo/script-manifest.cjs --write (never edit the region by hand)');
      return 1;
    }
    out.log(`script region current (${manifest.entries.filter((e) => !e.comment).length} tags)`);
    return 0;
  }
  out.error('usage: script-manifest.cjs --check | --write | --add <src> [--before <src> | --after <src>] [--module] | --extract');
  return 2;
}

if (require.main === module) {
  try { process.exitCode = main(process.argv.slice(2)); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
module.exports = { main };
