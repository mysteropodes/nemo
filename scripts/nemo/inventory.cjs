#!/usr/bin/env node
'use strict';
// Surface inventory generator (work package R03).
//
// Produces engineering/inventory/{surfaces.json,surfaces.csv,SURFACES.md} from
// the shipped source: every actionable control in src/index.html, every
// keyboard shortcut table, every Labs prototype registration, every
// context-menu item literal and the scripting API, each bound to the JS
// handler that serves it and to the document consumers that handler reaches
// (persistence, history, selection, animation, render, export, native bridge).
//
// The inventory is DERIVED, never hand-edited: a row that cannot be bound to a
// handler is emitted with status `unmapped`, which is the explicit state the
// remediation plan requires instead of silent omission. Re-run after any UI
// or handler change; `--check` fails when the committed files are stale.
//
// What "bound" means for a DOM control: static analysis found an EVENT
// REGISTRATION on that element — `addEventListener(...)` or an `on<event> =`
// assignment — reached from the element lookup through its own expression
// (member chain), the variable it is assigned to (within the enclosing function,
// honouring reassignment and shadowing), a helper that receives the element or
// its id as a parameter, a `forEach` callback over a `querySelectorAll` result,
// a selector scoped to the element, or delegation on a bound ancestor. A lookup
// that only reads or writes the element (`.value`, `.disabled`, `.textContent`,
// …) is a REFERENCE: it is recorded on the row (`meta.references`) but never
// makes the row `inventoried`. Every pass runs on the lexed source, so a lookup
// or listener that lives in a comment, a string or a template literal is not a
// binding at all.
//
// Read-only with respect to src/**. Static analysis only: no browser, no DOM.
//
// Usage:
//   node scripts/nemo/inventory.cjs            # regenerate
//   node scripts/nemo/inventory.cjs --check    # exit 1 if committed output is stale
//   node scripts/nemo/inventory.cjs --json     # print the JSON to stdout only

const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const { lexRegions, indexFile, lineOf, matchClose } = require('./lib/inventory-lexer.cjs');
const { createBindings } = require('./lib/inventory-bindings.cjs');
const { createSurfaces, CONSUMERS } = require('./lib/inventory-surfaces.cjs');
const { renderOutputs } = require('./lib/inventory-render.cjs');

const ROOT = path.resolve(__dirname, '..', '..');
const OUT_DIR = path.join(ROOT, 'engineering', 'inventory');
const GENERATOR_VERSION = 4;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------
function read(p) { return fs.readFileSync(p, 'utf8'); }
function sha256(s) { return crypto.createHash('sha256').update(s).digest('hex'); }
function relTo(root) { return (p) => path.relative(root, p).split(path.sep).join('/'); }
function git(args) { try { return execFileSync('git', args, { cwd: ROOT, encoding: 'utf8' }).trim(); } catch (e) { return ''; } }

function listJs(dir) {
  const out = [];
  (function walk(d) {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (/\.js$/.test(e.name) && !/\.min\.js$|\.vendor\.js$/.test(e.name)) out.push(p);
    }
  })(dir);
  return out.sort();
}

// ---------------------------------------------------------------------------
// HTML: tolerant tokenizer over index.html keeping an ancestor stack
// ---------------------------------------------------------------------------
const VOID = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'param', 'source', 'track', 'wbr']);
const ACTIONABLE_TAGS = new Set(['button', 'input', 'select', 'textarea', 'a', 'summary']);
const ACTIONABLE_DATA = ['data-tool', 'data-tab', 'data-pane', 'data-mode', 'data-value', 'data-val', 'data-key', 'data-align', 'data-distribute', 'data-flag', 'data-tag', 'data-sym'];
const ACTIONABLE_CLASSES = new Set(['tool-btn', 'pbtn', 'icon-btn', 'icon-only-btn', 'settings-tab', 'align-btn', 'combine-mode-btn', 'modal-x', 'menu-item', 'tp-style-btn', 'tp-case-btn', 'start-card', 'tb', 'xa-dot', 'lico', 'cw-mini', 'psel', 'dims-lock']);
const hasActionableClass = (cls) => cls.split(/\s+/).some((c) => ACTIONABLE_CLASSES.has(c));

function parseAttrs(s) {
  const attrs = {};
  const re = /([^\s=\/>"']+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+)))?/g;
  let m;
  while ((m = re.exec(s))) attrs[m[1].toLowerCase()] = m[2] !== undefined ? m[2] : m[3] !== undefined ? m[3] : m[4] !== undefined ? m[4] : '';
  return attrs;
}

function parseHtml(html) {
  const rows = [];
  const stack = []; // {tag, attrs, line, text, row}
  const re = /<!--[\s\S]*?-->|<script[\s\S]*?<\/script>|<style[\s\S]*?<\/style>|<\/([a-zA-Z][\w-]*)\s*>|<([a-zA-Z][\w-]*)([^>]*?)(\/?)>|([^<]+)/g;
  let m;
  const nearestId = () => { for (let i = stack.length - 1; i >= 0; i--) if (stack[i].attrs.id) return stack[i].attrs.id; return ''; };
  const inModal = () => stack.some((e) => /\bmodal-overlay\b/.test(e.attrs.class || ''));
  const hidden = () => stack.some((e) => /display\s*:\s*none/.test(e.attrs.style || '') || 'hidden' in e.attrs);
  const ancestorClasses = () => stack.map((e) => e.attrs.class || '').join(' ');
  const ancestorIds = () => stack.filter((e) => e.attrs.id).map((e) => e.attrs.id);
  while ((m = re.exec(html))) {
    if (m[5] !== undefined) { if (stack.length) stack[stack.length - 1].text += m[5]; continue; }
    if (m[1]) { // close tag
      for (let i = stack.length - 1; i >= 0; i--) {
        if (stack[i].tag === m[1].toLowerCase()) {
          const el = stack.splice(i);
          const closed = el[0];
          if (closed.row) closed.row.text = closed.text.replace(/\s+/g, ' ').trim().slice(0, 80);
          break;
        }
      }
      continue;
    }
    if (!m[2]) continue;
    const tag = m[2].toLowerCase();
    const attrs = parseAttrs(m[3] || '');
    const selfClose = !!m[4] || VOID.has(tag);
    const line = lineOf(html, m.index);
    const cls = attrs.class || '';
    const isActionable = ACTIONABLE_TAGS.has(tag) || ACTIONABLE_DATA.some((d) => d in attrs) || hasActionableClass(cls) || attrs.role === 'button' || 'onclick' in attrs;
    let row = null;
    if (isActionable && !(tag === 'input' && attrs.type === 'hidden')) {
      const data = {};
      for (const k of Object.keys(attrs)) if (k.startsWith('data-') && !/^data-i18n/.test(k)) data[k.slice(5)] = attrs[k];
      row = {
        kind: 'dom', tag, type: attrs.type || null, id: attrs.id || null, classes: cls, data,
        i18n: attrs['data-i18n'] || null, i18nTitle: attrs['data-i18n-title'] || null, title: attrs.title || null, href: attrs.href || null,
        container: nearestId(), modal: inModal(), hiddenAtLoad: hidden(), line, text: '', ancestorClasses: ancestorClasses(), ancestorIds: ancestorIds(),
      };
      rows.push(row);
    }
    if (!selfClose) stack.push({ tag, attrs, line, text: '', row });
  }
  return rows;
}

// ---------------------------------------------------------------------------
// i18n: English strings for labels
// ---------------------------------------------------------------------------
function loadI18n(src) {
  const p = path.join(src, 'js', 'i18n.js');
  if (!fs.existsSync(p)) return {};
  const f = indexFile(p, read(p));
  const enAt = f.code.search(/\n\s*en\s*:\s*\{/);
  if (enAt < 0) return {};
  const open = f.code.indexOf('{', enAt);
  const body = f.code.slice(open + 1, matchClose(f, open));
  const dict = {};
  const re = /([A-Za-z_$][\w$]*)\s*:\s*('(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*")/g;
  let m;
  while ((m = re.exec(body))) dict[m[1]] = m[2].slice(1, -1).replace(/\\'/g, '\'').replace(/\\"/g, '"');
  return dict;
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------
function build(opts) {
  const root = (opts && opts.root) || ROOT;
  const src = path.join(root, 'src');
  const rel = relTo(root);
  const html = read(path.join(src, 'index.html'));
  const domRows = parseHtml(html);
  const i18n = loadI18n(src);
  const files = listJs(path.join(src, 'js'));
  const idx = files.map((file) => indexFile(file, read(file)));
  const { fnIndex, bindings, classBindings, prefixBindings } = createBindings(idx, domRows, rel);
  const { rows, globals } = createSurfaces({ src, rel, domRows, i18n, idx, fnIndex, bindings, classBindings, prefixBindings });

  // Fixture coverage (tests/fixtures/manifest.json, R03): each row lists the
  // fixtures whose areas include its own; the manifest is an input, so adding
  // or changing a fixture makes the committed inventory stale until regenerated.
  const manifestPath = path.join(root, 'tests', 'fixtures', 'manifest.json');
  const manifestText = fs.existsSync(manifestPath) ? read(manifestPath) : null;
  const fixturesByArea = {};
  if (manifestText) for (const fx of (JSON.parse(manifestText).fixtures || [])) for (const a of fx.areas || []) (fixturesByArea[a] = fixturesByArea[a] || []).push(fx.id);
  for (const r of rows) r.fixtures = fixturesByArea[r.area] || [];

  rows.sort((a, b) => a.id.localeCompare(b.id));
  const head = git(['rev-parse', 'HEAD']);
  const inputs = { 'src/index.html': sha256(html) };
  if (manifestText) inputs['tests/fixtures/manifest.json'] = sha256(manifestText);
  for (const f of idx) inputs[rel(f.file)] = sha256(f.text);
  const counts = { total: rows.length, byKind: {}, byStatus: {}, byArea: {}, byConsumer: {} };
  for (const r of rows) {
    counts.byKind[r.kind] = (counts.byKind[r.kind] || 0) + 1;
    counts.byStatus[r.status] = (counts.byStatus[r.status] || 0) + 1;
    counts.byArea[r.area] = (counts.byArea[r.area] || 0) + 1;
    for (const c of Object.keys(r.consumers)) counts.byConsumer[c] = (counts.byConsumer[c] || 0) + 1;
  }
  return {
    schema: 'nemo.inventory/1', generator: 'scripts/nemo/inventory.cjs', generatorVersion: GENERATOR_VERSION,
    source: { head, describe: git(['describe', '--tags', '--always', '--dirty']) },
    inputsDigest: sha256(Object.entries(inputs).sort().map(([k, v]) => k + ':' + v).join('\n')),
    states: ['inventoried', 'characterized', 'legacy-adapter', 'migrated', 'validated', 'unavailable-with-reason', 'unmapped'],
    consumers: Object.keys(CONSUMERS), counts, fixturesByArea, globals, rows,
  };
}

// Names of the outputs under `dir` that are missing or differ from `outputs`,
// ignoring the source head/describe stamps (they change on every commit
// without a content change).
function staleOutputs(outputs, dir) {
  const norm = (s) => s.replace(/"head": "[0-9a-f]*"/, '').replace(/"describe": "[^"]*"/, '').replace(/source `[0-9a-f]*` \([^)]*\)/, '');
  const stale = [];
  for (const [name, content] of Object.entries(outputs)) {
    const p = path.join(dir, name);
    if (!fs.existsSync(p)) { stale.push(name + ' (missing)'); continue; }
    if (norm(read(p)) !== norm(content)) stale.push(name);
  }
  return stale;
}

function main() {
  const args = process.argv.slice(2);
  const inv = build();
  const outputs = renderOutputs(inv);
  if (args.includes('--json')) { process.stdout.write(outputs['surfaces.json']); return; }
  if (args.includes('--check')) {
    const stale = staleOutputs(outputs, OUT_DIR);
    if (stale.length) { console.error('inventory stale: ' + stale.join(', ') + ' — run `npm run inventory`'); process.exit(1); }
    console.log('inventory up to date (' + inv.rows.length + ' rows, digest ' + inv.inputsDigest.slice(0, 12) + ')');
    return;
  }
  fs.mkdirSync(OUT_DIR, { recursive: true });
  for (const [name, content] of Object.entries(outputs)) fs.writeFileSync(path.join(OUT_DIR, name), content);
  console.log(`inventory: ${inv.rows.length} rows (${JSON.stringify(inv.counts.byStatus)}) -> ${relTo(ROOT)(OUT_DIR)}/`);
}

if (require.main === module) main();
module.exports = { build, renderOutputs, staleOutputs, lexRegions, OUT_DIR };
