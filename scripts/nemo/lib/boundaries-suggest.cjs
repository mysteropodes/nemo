'use strict';
// Read-only helper behind `boundaries --suggest`: for a source file the gate
// reports as unowned, print the profile entry to add. It never writes a file.
// Rust files follow their `mod`/`#[path]` declaration to the module that owns
// the declaring file; JS files follow the modules that own sibling files.

const fs = require('node:fs');
const path = require('node:path');
const { discoverSourcePaths } = require('./boundaries-discovery.cjs');
const { discoverRepositoryFiles } = require('./boundaries-repository.cjs');

const DIR = 'engineering/boundaries/profiles';
const AREAS = [
  { name: 'tooling', profiles: [`${DIR}/scripts-nemo.profile.json`], test: (f) => f.startsWith('scripts/nemo/') && f.endsWith('.cjs') },
  { name: 'app-js', profiles: [`${DIR}/app-js.profile.json`], policy: `${DIR}/app-js.coverage.json`,
    test: (f) => f.startsWith('src/') && /\.(?:js|mjs|cjs|ts)$/.test(f) },
  { name: 'rust', profiles: [`${DIR}/rust.profile.json`, `${DIR}/mcp-rust.profile.json`], test: (f) => f.endsWith('.rs') },
];

const readJson = (root, file) => JSON.parse(fs.readFileSync(path.join(root, file), 'utf8'));
const camel = (name) => name.replace(/\.(?:test\.)?[a-z]+$/, '').replace(/[-_]+([a-z0-9])/g, (_, c) => c.toUpperCase());

function load(root, area) {
  const modules = [];
  for (const profile of area.profiles) {
    for (const module of readJson(root, profile).modules) modules.push({ ...module, profile });
  }
  const owner = new Map();
  for (const module of modules) for (const file of module.files) owner.set(path.posix.join(module.dir, file), module);
  return { modules, owner };
}

function unownedFor(root, area, owner, policy) {
  const excluded = new Set((policy?.exclusions || []).map((record) => record.path));
  if (area.name === 'rust') {
    const vendor = readJson(root, `${DIR}/rust.vendor.json`).root;
    const inventory = discoverRepositoryFiles({ root });
    if (!inventory.ok) throw new Error(`repository inventory failed: ${inventory.diagnostics.map((d) => d.code).join(', ')}`);
    const found = inventory.entries.map((entry) => entry.path);
    return found.filter((f) => f.endsWith('.rs') && !f.startsWith(`${vendor}/`) && !owner.has(f) && !excluded.has(f));
  }
  const roots = area.name === 'tooling' ? ['scripts/nemo'] : [policy.scope.sourceRoot];
  const kinds = area.name === 'tooling' ? ['.cjs'] : policy.scope.sourceKinds;
  return discoverSourcePaths({ root, sourceRoots: roots, extensions: kinds }).filter((f) => !owner.has(f) && !excluded.has(f));
}

// Rust: declaring files are owned .rs files of the same crate whose text declares
// `mod <stem>` (same directory), `#[path = "…"]` to the file, or names its relative
// path as a literal (macro-listed modules).
const texts = new Map();
function readText(root, file) {
  const key = `${root}\0${file}`;
  if (!texts.has(key)) {
    let text = '';
    try { text = fs.readFileSync(path.join(root, file), 'utf8'); } catch { /* unreadable: no declaration */ }
    texts.set(key, text);
  }
  return texts.get(key);
}

function rustDeclarers(root, file, owner) {
  const stem = path.posix.basename(file, '.rs');
  const cut = file.replace(/\/(?:src|tests)\/.*$/, '');
  const crate = cut === file ? path.posix.dirname(file) : cut;
  const found = [];
  for (const [declarer, module] of owner) {
    if (!declarer.endsWith('.rs') || !declarer.startsWith(`${crate}/`) || declarer === file) continue;
    const text = readText(root, declarer);
    const rel = path.posix.relative(path.posix.dirname(declarer), file);
    if (text.includes(`"${rel}"`)) found.push({ module, reason: `${declarer} names "${rel}" (#[path] or macro list)` });
    else if (path.posix.dirname(declarer) === path.posix.dirname(file) && new RegExp(`\\bmod\\s+${stem}\\b`).test(text)) {
      found.push({ module, reason: `${declarer} declares \`mod ${stem}\`` });
    }
  }
  return found;
}

function siblings(file, modules, area) {
  let dir = path.posix.dirname(file);
  for (; dir && dir !== '.'; dir = path.posix.dirname(dir)) {
    const here = modules.filter((m) => m.dir === dir && (area.name !== 'rust' || m.files.some((f) => f.endsWith('.rs'))));
    if (here.length) return { dir, here };
  }
  return { dir: null, here: [] };
}

function newModule(file, exemplar) {
  const base = path.posix.basename(file);
  const prefix = exemplar.id.split('.').slice(0, -1).join('.');
  const entry = { id: `${prefix}.${camel(base)}`, layer: exemplar.layer, dir: path.posix.dirname(file), files: [base] };
  if (exemplar.publicApi) entry.publicApi = [base];
  entry.sizeProfile = exemplar.sizeProfile;
  return entry;
}

function suggestOne(root, file, area, state, policy) {
  const known = state.owner.get(file);
  if (known) return { file, area: area.name, status: 'owned', module: known.id, profile: known.profile };
  const result = { file, area: area.name, status: 'unowned', candidates: [], note: null };
  const add = (module, reason) => {
    const hit = result.candidates.find((c) => c.module === module.id);
    if (hit) hit.reasons.push(reason);
    else result.candidates.push({ module: module.id, profile: module.profile, reasons: [reason], layer: module.layer });
  };
  if (area.name === 'rust') for (const d of rustDeclarers(root, file, state.owner)) add(d.module, d.reason);
  if (!result.candidates.length) {
    const near = siblings(file, state.modules, area);
    const ext = path.posix.extname(file);
    const same = near.here.filter((m) => m.files.some((f) => f.endsWith(ext)));
    const singles = same.length >= 3 && same.filter((m) => m.files.length === 1).length * 5 >= same.length * 4;
    if (singles) {
      const isTest = /\.test\./.test(file);
      const kind = same.filter((m) => m.files.some((f) => /\.test\./.test(f) === isTest));
      const tally = new Map();
      for (const m of kind.length ? kind : same) {
        const key = `${m.layer}\0${m.sizeProfile}`;
        tally.set(key, [...(tally.get(key) || []), m]);
      }
      const exemplar = [...tally.values()].sort((x, y) => y.length - x.length)[0][0];
      result.newModule = newModule(file, exemplar);
      result.profile = exemplar.profile;
      result.note = `${near.dir} uses one module per file; no existing module to extend. Layer and sizeProfile follow the most common sibling choice (e.g. ${exemplar.id})`;
    } else {
      for (const m of same) add(m, `owns ${m.files.length} sibling file(s) in ${near.dir}`);
      if (!same.length) result.note = 'no module owns this directory or an ancestor; add a new module by hand';
    }
  }
  if (result.candidates.length === 1) result.module = result.candidates[0].module;
  else if (result.candidates.length > 1) result.note = 'ambiguous: pick the module whose responsibility the file belongs to';
  const chosen = result.candidates[0];
  result.profile = result.profile || chosen?.profile;
  if (result.module && area.policy) {
    const sibling = (policy.retainedSources || []).find((r) => r.moduleId === result.module);
    result.coverage = { path: file, moduleId: result.module, executionClass: sibling?.executionClass || '<executionClass>',
      loadSites: '<fill from src/index.html or the importing module; then run node scripts/nemo/refresh-app-pins.cjs>' };
  }
  return result;
}

/** @returns {{entries: object[], unmatched: string[]}} */
function suggest(root, requested = []) {
  const states = AREAS.map((area) => ({ area, state: load(root, area), policy: area.policy ? readJson(root, area.policy) : null }));
  const entries = [];
  const unmatched = [];
  const files = requested.length ? requested.map((f) => f.split(path.sep).join('/')) : null;
  for (const { area, state, policy } of states) {
    const list = files ? files.filter((f) => area.test(f)) : unownedFor(root, area, state.owner, policy);
    for (const file of list) entries.push(suggestOne(root, file, area, state, policy));
  }
  if (files) for (const f of files) if (!AREAS.some((a) => a.test(f))) unmatched.push(f);
  return { entries, unmatched };
}

function render({ entries, unmatched }) {
  const lines = [];
  for (const e of entries) {
    if (e.status === 'owned') { lines.push(`${e.file}  [${e.area}] already owned by ${e.module} (${e.profile})`); continue; }
    lines.push(`${e.file}  [${e.area}] UNOWNED`);
    if (e.newModule) lines.push(`  add this module to ${e.profile}:`, `    ${JSON.stringify(e.newModule)}`);
    else if (e.module) lines.push(`  add "${path.posix.basename(e.file)}" to modules[id="${e.module}"].files in ${e.profile}`);
    for (const c of e.candidates) for (const reason of c.reasons) lines.push(`  ${e.candidates.length > 1 ? 'candidate' : 'because'} ${c.module}: ${reason}`);
    if (e.note) lines.push(`  note: ${e.note}`);
    if (e.coverage) lines.push(`  add to retainedSources in ${'engineering/boundaries/profiles/app-js.coverage.json'}:`, `    ${JSON.stringify(e.coverage)}`);
  }
  for (const f of unmatched) lines.push(`${f}  no profile area covers this path (scripts/nemo/*.cjs, src/**/*.js, **/*.rs)`);
  if (!entries.length && !unmatched.length) lines.push('suggest: no unowned files');
  return lines.join('\n');
}

module.exports = { suggest, render, AREAS };
