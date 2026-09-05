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

const ROOT = path.resolve(__dirname, '..', '..');
const SRC = path.join(ROOT, 'src');
const OUT_DIR = path.join(ROOT, 'engineering', 'inventory');
const GENERATOR_VERSION = 3;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------
function read(p) { return fs.readFileSync(p, 'utf8'); }
function sha256(s) { return crypto.createHash('sha256').update(s).digest('hex'); }
function rel(p) { return path.relative(ROOT, p).split(path.sep).join('/'); }
function lineOf(text, offset) { let n = 1; for (let i = 0; i < offset; i++) if (text.charCodeAt(i) === 10) n++; return n; }
function uniq(a) { return Array.from(new Set(a)); }
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

// Find the offset of the `}` matching the `{` at `open`, skipping string
// literals, template literals and comments. Regex literals are not modelled;
// a brace inside one can mis-pair, which only widens/narrows the body used
// for reachability, never crashes the generator.
function matchBrace(text, open) {
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    const c = text[i];
    if (c === '/' && text[i + 1] === '/') { i = text.indexOf('\n', i); if (i < 0) return text.length; continue; }
    if (c === '/' && text[i + 1] === '*') { i = text.indexOf('*/', i + 2); if (i < 0) return text.length; i++; continue; }
    if (c === '\'' || c === '"' || c === '`') {
      const q = c;
      for (i++; i < text.length; i++) {
        if (text[i] === '\\') { i++; continue; }
        if (text[i] === q) break;
        if (q !== '`' && text[i] === '\n') break;
      }
      continue;
    }
    if (c === '{') depth++;
    else if (c === '}') { depth--; if (depth === 0) return i; }
  }
  return text.length;
}

// Body span of the function whose parameter list or arrow starts at/after `from`.
function functionBodyAt(text, from) {
  const open = text.indexOf('{', from);
  if (open < 0) return null;
  return { start: open, end: matchBrace(text, open) };
}

// ---------------------------------------------------------------------------
// JS index: named functions, their bodies, and DOM lookups
// ---------------------------------------------------------------------------
const FN_DECL = /(?:^|[^\w$.])function\s+([A-Za-z_$][\w$]*)\s*\(|([A-Za-z_$][\w$]*)\s*:\s*function\s*\(|(?:^|[^\w$.])(?:var|let|const)?\s*([A-Za-z_$][\w$]*)\s*=\s*function\s*\(|(?:^|[^\w$])([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?\([^()]*\)\s*=>\s*\{|(?:^|[^\w$])([A-Za-z_$][\w$]*)\s*:\s*(?:async\s*)?\([^()]*\)\s*=>\s*\{/g;

function indexFile(file) {
  const text = read(file);
  const fns = []; // {name, start, end, line}
  FN_DECL.lastIndex = 0;
  let m;
  while ((m = FN_DECL.exec(text))) {
    const name = m[1] || m[2] || m[3] || m[4] || m[5];
    const body = functionBodyAt(text, m.index + m[0].length - 1);
    if (!body) continue;
    const paren = text.indexOf('(', m.index);
    const params = paren >= 0 && paren < body.start ? text.slice(paren + 1, text.indexOf(')', paren)).split(',').map((x) => x.trim()).filter(Boolean) : [];
    fns.push({ name, params, start: body.start, end: body.end, line: lineOf(text, m.index), declStart: m.index });
  }
  return { file, text, fns };
}

function enclosingFunction(idx, offset) {
  let best = null;
  for (const f of idx.fns) {
    if (f.start <= offset && offset <= f.end) {
      if (!best || (f.end - f.start) < (best.end - best.start)) best = f;
    }
  }
  return best;
}

function stripComments(src) { return src.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:\\'"`])\/\/[^\n]*/g, '$1'); }

const CALL_RE = /(?:^|[^\w$.])([A-Za-z_$][\w$]*)\s*\(/g;
const MEMBER_CALL_RE = /\.([A-Za-z_$][\w$]*)\s*\(/g;
function callsIn(body) {
  const names = new Set();
  let m;
  CALL_RE.lastIndex = 0;
  while ((m = CALL_RE.exec(body))) names.add(m[1]);
  MEMBER_CALL_RE.lastIndex = 0;
  while ((m = MEMBER_CALL_RE.exec(body))) names.add(m[1]);
  for (const kw of ['if', 'for', 'while', 'switch', 'function', 'return', 'catch', 'typeof']) names.delete(kw);
  return names;
}

// Consumer sentinels: substrings whose presence in a reachable body means the
// surface touches that document consumer (CLAUDE.md §1 list).
const CONSUMERS = {
  persist: ['saveActiveLayerFrame(', 'saveAllLayerFrames(', 'saveActiveLayerFrameOrPromote(', 'exportJSON(', 'importJSON(', 'saveProject', 'writeProject', 'autosave'],
  history: ['pushUndo(', 'pushUndoLayers(', 'pushUndoActiveFrame(', 'restoreLayersSnapshot(', 'undo(', 'redo('],
  selection: ['selectedPaths', 'clearSelection(', 'selectAll(', 'setSelection('],
  animation: ['SMMotion.', 'valueAtFrame(', 'setLayerValue(', 'toggleAnimated(', 'goToFrame(', 'insertKeyframe(', 'generateTweens(', 'cameraKeys'],
  render: ['renderNow(', 'updateUI(', 'renderTimeline(', 'renderLayerList(', 'loadFrame(', 'view.update(', 'buildSceneJson(', 'SMEngineBridge.'],
  export: ['exportFrame', 'exportVideo', 'exportPNG', 'exportMP4', 'exportGif', 'SMExport.', 'render_to_pixels', 'renderFrameToPixels', 'exportLottie', 'exportRive', 'SMRenderManager.'],
  native: ['__TAURI__', 'tauriOk(', 'exportTauri(', 'exportTauriAvailable(', 'SMNativeVideo.', 'Command.sidecar', 'SMLinkedMedia.'],
};

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
  const stack = []; // {tag, attrs, line, textParts:[]}
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
function loadI18n() {
  const p = path.join(SRC, 'js', 'i18n.js');
  if (!fs.existsSync(p)) return {};
  const text = read(p);
  const enAt = text.search(/\n\s*en\s*:\s*\{/);
  if (enAt < 0) return {};
  const open = text.indexOf('{', enAt);
  const body = text.slice(open + 1, matchBrace(text, open));
  const dict = {};
  const re = /([A-Za-z_$][\w$]*)\s*:\s*('(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*")/g;
  let m;
  while ((m = re.exec(body))) dict[m[1]] = m[2].slice(1, -1).replace(/\\'/g, '\'').replace(/\\"/g, '"');
  return dict;
}

// ---------------------------------------------------------------------------
// area classification (by handler file first, container id second)
// ---------------------------------------------------------------------------
const AREA_BY_FILE = [
  [/labs\//, 'labs'],
  [/(tools|draw-bridge|brush|eraser|fill-bridge|pen-bridge|shape-bridge|select-bridge|subselect|group-bridge|stroke-modeler|vector-text|text-selector|symmetry|perspective|gradient|viewtools|rulers|shadow-brush|bitmap|abr-import|color-picker|color-manager|palette|shapes-panel|vectorize)/, 'drawing-selection'],
  [/(timeline|layer-kind|layer-scroll|tweens|transplant|history-panel)/, 'timeline-layers-frames'],
  [/(motion|expr-|effector|motion-graph|layer-inout|markers|bpm-grid|timeline-zoom|text-animator|rig-|image-mesh|camera|tracker|comp-preview|playback-cache|second-viewer|path-fx)/, 'animation-rigs-expressions'],
  [/(effects-panel|shader-effects|custom-effects)/, 'effects-masks'],
  [/(export|render-manager|rive-export|lottie|ae-camera|images|media-library|linked-media|native-video|audio|drop-import|svg-import|psd-import|figma-import|reference-bridge|asset-tree|assets-panel|oca)/, 'media-import-export'],
  [/(project|updater|feedback|kitsu|p2p|idb-store|nemo-plugin|nemo-script|nemo-panel|tutorial|engine-bridge|gpu-gate)/, 'project-lifecycle-integrations'],
  [/storyboard/, 'storyboard'],
  [/(ui\.js|app\.js|tools-panel-dock|lipsync|i18n)/, 'preferences-workspace'],
];
const AREA_BY_CONTAINER = [
  [/settings|shortcut|labs/, 'preferences-workspace'],
  [/export|render|lottie|rive|media|video|audio|image|import|asset/, 'media-import-export'],
  [/history|start|project|kitsu|fb-|feedback|update/, 'project-lifecycle-integrations'],
  [/motion|expr|graph|curve|rig|mesh|camera|tracker|marker|bpm/, 'animation-rigs-expressions'],
  [/effect|matte|mask/, 'effects-masks'],
  [/tl-|timeline|layer|frame|onion|fg-|xsheet/, 'timeline-layers-frames'],
  [/story|montage/, 'storyboard'],
  [/track|combine|bool|widget|imagemesh|rig/, 'animation-rigs-expressions'],
  [/tool|brush|color|fill|stroke|text|shape|palette|p-|dims|align|pen|select|comment|scene/, 'drawing-selection'],
];
const HUB_FILE = /(timeline|app|ui|tools)\.js$/;
function areaFor(file, container, classes) {
  const byFile = () => { if (file) for (const [re, a] of AREA_BY_FILE) if (re.test(file)) return a; return null; };
  const byContainer = () => { const key = (container || '') + ' ' + (classes || ''); for (const [re, a] of AREA_BY_CONTAINER) if (re.test(key)) return a; return null; };
  if (file && !HUB_FILE.test(file)) return byFile() || byContainer() || 'unclassified';
  return byContainer() || byFile() || 'unclassified';
}

function humanize(s) { return String(s || '').replace(/^p-|^tl-|^btn-|^exp-|^set-/, '').replace(/[-_]+/g, ' ').trim(); }

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------
function build() {
  const html = read(path.join(SRC, 'index.html'));
  const domRows = parseHtml(html);
  const i18n = loadI18n();
  const files = listJs(path.join(SRC, 'js'));
  const idx = files.map(indexFile);
  const byFile = new Map(idx.map((f) => [f.file, f]));

  // global function index (first definition wins; classic scripts share one scope)
  const fnIndex = new Map();
  for (const f of idx) for (const fn of f.fns) if (!fnIndex.has(fn.name)) fnIndex.set(fn.name, { file: f.file, body: stripComments(f.text.slice(fn.start, fn.end + 1)), line: fn.line, params: fn.params });

  // Functions that take an element id as a parameter (`function el(id){return document.getElementById(id)}`,
  // `wireBoolBtn(id, op)`): a call `name('literal')` is a lookup of that id.
  const idTakers = new Map(); // name -> param index
  for (const [name, fn] of fnIndex) {
    fn.params.forEach((prm, i) => {
      if (!/^[A-Za-z_$][\w$]*$/.test(prm)) return;
      const re = new RegExp("getElementById\\(\\s*" + prm.replace(/\$/g, '\\$') + "\\s*\\)|querySelector\\(\\s*'#'\\s*\\+\\s*" + prm.replace(/\$/g, '\\$') + "\\b");
      if (re.test(fn.body) && !idTakers.has(name)) idTakers.set(name, i);
    });
  }

  const reachCache = new Map();
  function reach(bodyText, depth) {
    // Returns { consumers: {name: depth}, bodies: n } following calls up to `depth`.
    const seen = new Set();
    const hits = {};
    let frontier = [stripComments(bodyText || '')];
    for (let d = 1; d <= depth && frontier.length; d++) {
      const next = [];
      for (const body of frontier) {
        for (const [name, needles] of Object.entries(CONSUMERS)) {
          if (hits[name] !== undefined) continue;
          if (needles.some((n) => body.includes(n))) hits[name] = d;
        }
        for (const callee of callsIn(body)) {
          if (seen.has(callee)) continue;
          seen.add(callee);
          const fn = fnIndex.get(callee);
          if (fn) next.push(fn.body);
        }
      }
      frontier = next;
      if (seen.size > 600) break; // hub explosion guard
    }
    return { consumers: hits, functionsVisited: seen.size };
  }

  // One lookup site -> {file, line, fn, events, body}: the events wired on the
  // same statement, the callback body (inline or by reference) for reachability,
  // else the enclosing function.
  function recAt(f, at) {
    const fn = enclosingFunction(f, at);
    const tail = f.text.slice(at, at + 400);
    const events = uniq((tail.match(/\.addEventListener\(\s*'([^']+)'/g) || []).map((s) => s.match(/'([^']+)'/)[1]).concat((tail.match(/\.on(click|input|change|pointerdown|mousedown|keydown|dblclick|contextmenu)\s*=/g) || []).map((s) => s.match(/on(\w+)/)[1])));
    // callback body for reachability: the function passed to addEventListener/on*= on this statement, else enclosing fn
    let body = null;
    const cb = tail.match(/(?:\.addEventListener\(\s*'[^']+'\s*,|\.on\w+\s*=)\s*(?:async\s*)?(?:function\s*\([^)]*\)|\([^)]*\)\s*=>|[\w$]+\s*=>)\s*\{/);
    if (cb) { const span = functionBodyAt(f.text, at + cb.index + cb[0].length - 1); if (span) body = f.text.slice(span.start, span.end + 1); }
    else {
      const ref = tail.match(/(?:\.addEventListener\(\s*'[^']+'\s*,|\.on\w+\s*=)\s*([A-Za-z_$][\w$]*)\s*[,)]/);
      if (ref && fnIndex.get(ref[1])) body = fnIndex.get(ref[1]).body;
    }
    if (!body && fn) body = f.text.slice(fn.start, fn.end + 1);
    const rec = { file: rel(f.file), line: lineOf(f.text, at), fn: fn ? fn.name : null, events, body: stripComments(body || ''), fnBody: () => (fn ? f.text.slice(fn.start, fn.end + 1) : '') };
    return rec;
  }

  // DOM lookups: id -> [{file, line, fn, events, body}]
  const bindings = new Map();
  const classBindings = new Map();
  const prefixBindings = []; // {prefix, rec}
  const takerNames = Array.from(idTakers.keys()).filter((n) => n.length > 1 || n === '$').map((n) => n.replace(/\$/g, '\\$'));
  const LOOKUP_RE = new RegExp("getElementById\\(\\s*'([^']+)'\\s*(\\)|\\+)|querySelector(All)?\\(\\s*'([^']+)'\\s*\\)" + (takerNames.length ? "|(?:^|[^\\w$.])(" + takerNames.join('|') + ")\\(\\s*'([^']+)'\\s*(\\)|,|\\+)" : ''), 'g');
  for (const f of idx) {
    LOOKUP_RE.lastIndex = 0;
    let m;
    while ((m = LOOKUP_RE.exec(f.text))) {
      let id = null, sel = null, prefix = null;
      if (m[1]) { if (m[2] === '+') prefix = m[1]; else id = m[1]; }
      else if (m[4]) { sel = m[4]; if (/^#[\w-]+$/.test(sel)) id = sel.slice(1); }
      else if (m[5]) {
        if (idTakers.get(m[5]) !== 0) continue; // literal is not the id parameter
        if (m[7] === '+') prefix = m[6]; else id = m[6];
      }
      const rec = recAt(f, m.index);
      if (id) { if (!bindings.has(id)) bindings.set(id, []); bindings.get(id).push(rec); }
      else if (prefix) prefixBindings.push({ prefix, rec });
      else if (sel) { if (!classBindings.has(sel)) classBindings.set(sel, []); classBindings.get(sel).push(rec); }
    }
  }
  // Lookups that go through a table or an array of id literals:
  //   var IDS = {unite: 'btn-combine-unite', …};  getElementById(IDS[mode]).addEventListener(…)
  //   ['btn-a', 'btn-b'].forEach(function (id) { getElementById(id).addEventListener(…) })
  // Every id literal of the table/array is bound at the lookup site.
  const domIds = new Set(domRows.map((r) => r.id).filter(Boolean));
  const idTables = new Map(); // NAME -> [ids]
  const TABLE_RE = /(?:^|[^\w$.])(?:var|let|const)\s+([A-Za-z_$][\w$]*)\s*=\s*\{([^{}]*)\}/g;
  for (const f of idx) {
    TABLE_RE.lastIndex = 0;
    let m;
    while ((m = TABLE_RE.exec(f.text))) {
      const ids = (m[2].match(/'([\w-]+)'/g) || []).map((s) => s.slice(1, -1)).filter((s) => domIds.has(s));
      if (ids.length && !idTables.has(m[1])) idTables.set(m[1], ids);
    }
  }
  const TABLE_LOOKUP_RE = /getElementById\(\s*([A-Za-z_$][\w$]*)\s*\[/g;
  const ARRAY_FOREACH_RE = /\[((?:\s*'[\w-]+'\s*,?)+)\]\s*\.forEach\(\s*(?:function\s*\(\s*([\w$]+)\s*\)|\(?\s*([\w$]+)\s*\)?\s*=>)\s*\{/g;
  const bind = (id, rec) => { if (!bindings.has(id)) bindings.set(id, []); bindings.get(id).push(rec); };
  for (const f of idx) {
    let m;
    TABLE_LOOKUP_RE.lastIndex = 0;
    while ((m = TABLE_LOOKUP_RE.exec(f.text))) {
      const ids = idTables.get(m[1]);
      if (!ids) continue;
      const rec = recAt(f, m.index);
      for (const id of ids) bind(id, rec);
    }
    ARRAY_FOREACH_RE.lastIndex = 0;
    while ((m = ARRAY_FOREACH_RE.exec(f.text))) {
      const prm = (m[2] || m[3]).replace(/\$/g, '\\$');
      const span = functionBodyAt(f.text, m.index + m[0].length - 1);
      if (!span) continue;
      const body = f.text.slice(span.start, span.end + 1);
      const at = body.search(new RegExp("getElementById\\(\\s*" + prm + "\\s*\\)|querySelector\\(\\s*'#'\\s*\\+\\s*" + prm + "\\b"));
      if (at < 0) continue;
      const rec = recAt(f, span.start + at);
      const ids = (m[1].match(/'([\w-]+)'/g) || []).map((s) => s.slice(1, -1)).filter((s) => domIds.has(s));
      for (const id of ids) bind(id, rec);
    }
  }

  // Minimal CSS selector matcher for static rows: compound selectors of
  // tag / #id / .class / [attr] / [attr="v"], one optional #id or .class
  // ancestor (descendant or child combinator), and comma-separated lists.
  function parseCompound(s) {
    const out = { tag: null, id: null, classes: [], attrs: [] };
    const re = /^([a-zA-Z][\w-]*)|#([\w-]+)|\.([\w-]+)|\[([\w-]+)(?:=(?:"([^"]*)"|'([^']*)'|([^\]]+)))?\]/g;
    let m, consumed = 0;
    while ((m = re.exec(s))) {
      if (m.index !== consumed) return null; // pseudo-class or other unsupported syntax
      consumed = m.index + m[0].length;
      if (m[1]) out.tag = m[1].toLowerCase();
      else if (m[2]) out.id = m[2];
      else if (m[3]) out.classes.push(m[3]);
      else out.attrs.push({ name: m[4].toLowerCase(), value: m[5] !== undefined ? m[5] : m[6] !== undefined ? m[6] : m[7] });
    }
    return s.length && consumed === s.length ? out : null;
  }
  const hasClass = (list, cls) => new RegExp('(^|\\s)' + cls + '(\\s|$)').test(list || '');
  function compoundMatches(c, row) {
    if (c.tag && c.tag !== row.tag) return false;
    if (c.id && c.id !== row.id) return false;
    for (const cls of c.classes) if (!hasClass(row.classes, cls)) return false;
    for (const a of c.attrs) {
      let v;
      if (a.name.startsWith('data-')) v = row.data[a.name.slice(5)];
      else if (a.name === 'type') v = row.type;
      else if (a.name === 'id') v = row.id;
      else if (a.name === 'href') v = row.href;
      else return false; // an attribute the static row does not record
      if (v === undefined || v === null) return false;
      if (a.value !== undefined && v !== a.value) return false;
    }
    return true;
  }
  function ancestorMatches(c, row) {
    if (c.tag || c.attrs.length) return false; // ancestor tags/attributes are not recorded
    if (c.id && !row.ancestorIds.includes(c.id)) return false;
    for (const cls of c.classes) if (!hasClass(row.ancestorClasses, cls)) return false;
    return !!(c.id || c.classes.length);
  }
  function matchesSelector(row, sel) {
    return sel.split(',').some((part) => {
      const parts = part.trim().split(/\s+/).filter((x) => x !== '>');
      if (!parts.length || parts.length > 2) return false;
      const target = parseCompound(parts[parts.length - 1]);
      if (!target || !compoundMatches(target, row)) return false;
      if (parts.length === 1) return true;
      const anc = parseCompound(parts[0]);
      return !!anc && ancestorMatches(anc, row);
    });
  }
  function delegatedFor(row) {
    const out = [];
    for (const [sel, recs] of classBindings) if (matchesSelector(row, sel)) out.push(...recs);
    return out;
  }

  // Rows with no binding of their own but an ancestor that is either an event
  // delegate (`bar.addEventListener('click', e => … e.target … )`) or emptied
  // and rebuilt at runtime (`bar.innerHTML = ''` + createElement): the first
  // binds the row through delegation; the second makes the static markup a
  // placeholder the live UI replaces before it can receive input.
  const INPUT_EVENTS = /^(click|dblclick|pointerdown|pointerup|mousedown|mouseup|contextmenu|change|input|keydown|keyup)$/;
  function containerFallback(row) {
    for (let i = row.ancestorIds.length - 1; i >= 0; i--) {
      const cid = row.ancestorIds[i];
      const recs = bindings.get(cid) || [];
      const delegates = recs.filter((r) => r.events.some((ev) => INPUT_EVENTS.test(ev)) && /\b(e|ev|evt|event)\.target\b|\.closest\(/.test(r.body));
      if (delegates.length) return { kind: 'delegation', recs: delegates, container: cid };
      const rebuilds = recs.filter((r) => { const b = r.fnBody ? r.fnBody() : r.body; return /\.innerHTML\s*=\s*(''|""|``)|\.replaceChildren\(/.test(b) && /createElement\(/.test(b); });
      if (rebuilds.length) {
        const r0 = rebuilds[0];
        const builder = r0.file + ':' + r0.line + (r0.fn ? ' ' + r0.fn + '()' : '');
        return { kind: 'placeholder', container: cid, builder, handler: [builder], reason: 'static placeholder: #' + cid + ' is emptied and rebuilt at runtime by ' + builder + '; the live controls are created and bound there' };
      }
    }
    return null;
  }

  const rows = [];
  const summarizeBind = (recs) => {
    const consumers = {};
    let visited = 0;
    for (const r of recs) { const rr = reach(r.body, 3); for (const [k, v] of Object.entries(rr.consumers)) if (consumers[k] === undefined || v < consumers[k]) consumers[k] = v; visited = Math.max(visited, rr.functionsVisited); }
    return { consumers, visited };
  };
  const platformFor = (recs, consumers) => {
    const nativeGated = recs.some((r) => /__TAURI__|tauriOk\(|exportTauriAvailable\(/.test(r.body));
    const browserOnly = recs.some((r) => /MediaRecorder|exportVideoBrowser/.test(r.body));
    if (nativeGated && browserOnly) return 'browser and desktop (separate code paths)';
    if (nativeGated || consumers.native) return 'desktop (native bridge); browser degrades or is gated';
    return 'browser and desktop';
  };

  // 1. DOM rows
  for (const r of domRows) {
    let recs = r.id ? (bindings.get(r.id) || []) : [];
    if (!recs.length && r.id) recs = prefixBindings.filter((b) => r.id.startsWith(b.prefix) && b.prefix.length >= 3).map((b) => b.rec);
    if (!recs.length) recs = delegatedFor(r);
    let placeholder = null, via = null;
    if (!recs.length) {
      const fb = containerFallback(r);
      if (fb && fb.kind === 'delegation') { recs = fb.recs; via = 'container delegation #' + fb.container; }
      else if (fb) { placeholder = fb; via = 'container rebuild #' + fb.container; }
    }
    if (!recs.length && r.href && /^(https?:|mailto:)/.test(r.href)) recs = [{ file: 'href', line: 0, fn: null, events: ['click'], body: '' }];
    const { consumers, visited } = summarizeBind(recs);
    const label = (r.i18n && i18n[r.i18n]) || (r.i18nTitle && i18n[r.i18nTitle]) || r.title || r.text || humanize(r.id) || (r.data.tool ? 'tool ' + r.data.tool : '');
    const file = recs[0] ? recs[0].file : null;
    const status = recs.length ? 'inventoried' : placeholder ? 'unavailable-with-reason' : 'unmapped';
    const exposure = ['UI' + (r.modal ? ' (modal)' : r.hiddenAtLoad ? ' (contextual)' : '')];
    if (r.data.tool) exposure.push('keyboard (tool shortcut)');
    rows.push({
      id: r.id ? 'dom:#' + r.id : 'dom:' + r.tag + (r.data.tool ? '[data-tool=' + r.data.tool + ']' : '') + '@' + r.line,
      kind: 'dom', surface: (r.container ? r.container + ' > ' : '') + r.tag + (r.type ? '[' + r.type + ']' : '') + (r.id ? '#' + r.id : ''),
      capability: label, area: areaFor(file, r.container, r.classes + ' ' + r.id),
      handler: recs.length ? uniq(recs.map((x) => x.file === 'href' ? 'href ' + r.href : x.file + ':' + x.line + (x.fn ? ' ' + x.fn + '()' : ''))).slice(0, 4) : placeholder ? placeholder.handler : [],
      events: uniq(recs.flatMap((x) => x.events)),
      exposure, mcp: 'none (R14)', sdk: 'none',
      consumers, platforms: platformFor(recs, consumers), status,
      reason: placeholder ? placeholder.reason : undefined,
      nextGate: status === 'unmapped' ? 'R03 follow-up: bind a handler or record unavailable-with-reason' : status === 'unavailable-with-reason' ? 'none for the static markup; characterize the runtime-built controls through ' + placeholder.builder : 'R12/R13: characterize against a fixture and diagnostics',
      source: 'src/index.html:' + r.line, meta: { data: r.data, i18n: r.i18n, container: r.container, modal: r.modal, hiddenAtLoad: r.hiddenAtLoad, functionsVisited: visited, via: via || undefined },
    });
  }

  // 2. Shortcut tables (timeline.js)
  const tl = byFile.get(path.join(SRC, 'js', 'timeline.js'));
  if (tl) {
    for (const table of ['TOOL_SHORTCUTS', 'COMMAND_SHORTCUTS', 'READONLY_SHORTCUTS']) {
      const at = tl.text.indexOf('var ' + table + '=[');
      if (at < 0) continue;
      const end = tl.text.indexOf('\n];', at);
      const block = tl.text.slice(at, end);
      const re = /\{action:'([^']*)',key:'([^']*)'(?:,cat:'([^']*)')?,label:'([^']*)'|\{keys:'([^']*)',label:'([^']*)',cat:'([^']*)'\}/g;
      let m;
      while ((m = re.exec(block))) {
        const action = m[1] || 'readonly:' + m[6];
        const key = m[2] !== undefined ? m[2] : m[5];
        const label = m[4] || m[6];
        const runAt = block.indexOf('run:function', m.index);
        let body = '';
        if (m[1] && runAt > 0 && runAt < block.indexOf('}', m.index + 200) + 400) { const span = functionBodyAt(block, runAt); if (span) body = block.slice(span.start, span.end + 1); }
        if (table === 'TOOL_SHORTCUTS') body = 'setTool(' + JSON.stringify(m[1]) + ')';
        const rr = reach(body, 3);
        rows.push({
          id: 'shortcut:' + action, kind: 'shortcut', surface: 'keyboard ' + (key || '(unbound)'),
          capability: i18n[label] || label, area: table === 'TOOL_SHORTCUTS' ? 'drawing-selection' : 'timeline-layers-frames',
          handler: [rel(tl.file) + ':' + lineOf(tl.text, at + m.index) + ' ' + table], events: ['keydown'],
          exposure: ['keyboard', table === 'READONLY_SHORTCUTS' ? 'Settings > Shortcuts (read-only)' : 'Settings > Shortcuts (remappable)'], mcp: 'none (R14)', sdk: 'none',
          consumers: rr.consumers, platforms: 'browser and desktop', status: 'inventoried',
          nextGate: 'R12/R13: characterize against a fixture and diagnostics', source: rel(tl.file) + ':' + lineOf(tl.text, at + m.index),
          meta: { table, key, cat: m[3] || m[7] || null, functionsVisited: rr.functionsVisited },
        });
      }
    }
  }

  // 3. Labs registrations
  for (const f of idx) {
    if (!/\/labs\//.test(f.file)) continue;
    const re = /SMLabs\.register\(\s*'([^']+)'\s*,\s*\{/g;
    let m;
    while ((m = re.exec(f.text))) {
      const span = functionBodyAt(f.text, m.index + m[0].length - 1);
      const body = span ? f.text.slice(span.start, span.end + 1) : '';
      const desc = (body.match(/describe\s*:\s*'([^']+)'/) || [])[1];
      const rr = reach(body, 2);
      rows.push({
        id: 'labs:' + m[1], kind: 'labs', surface: 'Settings > Labs / SMLabs.' + m[1], capability: (desc && i18n[desc]) || desc || m[1], area: 'labs',
        handler: [rel(f.file) + ':' + lineOf(f.text, m.index)], events: [], exposure: ['Labs panel (opt-in prototype)', 'script (window.SMLabs)'], mcp: 'none (R14)', sdk: 'SMLabs',
        consumers: rr.consumers, platforms: 'browser and desktop', status: 'inventoried',
        nextGate: 'R03/R18.6: decide ship, keep as Labs, or retire; Labs are not release surfaces', source: rel(f.file) + ':' + lineOf(f.text, m.index), meta: { functionsVisited: rr.functionsVisited },
      });
    }
  }

  // 4. Context-menu / dynamic menu items: {label:SM.t('key')...action:function
  for (const f of idx) {
    const re = /\{\s*label\s*:\s*(?:SM\.t\('([^']+)'\)|'((?:[^'\\]|\\.)*)'|"([^"]*)")/g;
    let m;
    while ((m = re.exec(f.text))) {
      const end = matchBrace(f.text, m.index);
      const lit = f.text.slice(m.index, end + 1);
      if (!/\b(action|run|onClick|onclick|fn)\s*:/.test(lit)) continue; // not an actionable item
      const key = m[1];
      const label = (key && i18n[key]) || m[2] || m[3] || key;
      const act = lit.search(/\b(action|run|onClick|onclick|fn)\s*:/);
      let body = '';
      const span = act >= 0 ? functionBodyAt(lit, act) : null;
      if (span) body = lit.slice(span.start, span.end + 1);
      else { const ref = lit.slice(act).match(/:\s*([A-Za-z_$][\w$.]*)/); if (ref && fnIndex.get(ref[1].split('.').pop())) body = fnIndex.get(ref[1].split('.').pop()).body; }
      const fn = enclosingFunction(f, m.index);
      const rr = reach(body, 3);
      const line = lineOf(f.text, m.index);
      rows.push({
        id: 'menu:' + rel(f.file).replace(/^src\/js\//, '') + ':' + line, kind: 'menu', surface: 'dynamic menu' + (fn ? ' (' + fn.name + ')' : ''), capability: label, area: areaFor(f.file, '', ''),
        handler: [rel(f.file) + ':' + line + (fn ? ' ' + fn.name + '()' : '')], events: ['click'], exposure: ['contextual menu'], mcp: 'none (R14)', sdk: 'none',
        consumers: rr.consumers, platforms: platformFor([{ body }], rr.consumers), status: body ? 'inventoried' : 'unmapped',
        nextGate: body ? 'R12/R13: characterize against a fixture and diagnostics' : 'R03 follow-up: bind a handler or record unavailable-with-reason', source: rel(f.file) + ':' + line, meta: { i18n: key || null, functionsVisited: rr.functionsVisited },
      });
    }
  }

  // 5. Scripting API (nemo-script.js) and plugin API (nemo-plugin.js):
  //    (a) `Object.defineProperty(Class.prototype, 'prop', {get, set})` properties, also
  //        when declared through a local helper whose first parameter is the name
  //        (`switchProp('solo', …)`);
  //    (b) `Class.prototype.method = function` (leading `_` = private, skipped);
  //    (c) members of object literals, namespaced by the `api.<ns> = {` literal that
  //        contains them (`selection.layers`), root members bare (`layers`).
  for (const apiFile of ['nemo-script.js', 'nemo-plugin.js']) {
    const f = byFile.get(path.join(SRC, 'js', apiFile));
    if (!f) continue;
    const text = f.text;
    const base = apiFile.replace('.js', '');
    const rootSurface = apiFile === 'nemo-script.js' ? 'SMScript.api().' : 'SMPlugin.';
    const pushSdk = (name, surface, body, line, capability) => {
      const rr = reach(body, 3);
      rows.push({
        id: 'sdk:' + base + '.' + name, kind: 'sdk', surface, capability: capability || humanize(name), area: 'project-lifecycle-integrations',
        handler: [rel(f.file) + ':' + line + ' ' + name], events: [], exposure: ['script API'], mcp: 'none (R14)', sdk: apiFile === 'nemo-script.js' ? 'SMScript' : 'SMPlugin',
        consumers: rr.consumers, platforms: 'browser and desktop', status: 'inventoried', nextGate: 'R14: expose through the MCP adapter with the same handler', source: rel(f.file) + ':' + line, meta: { functionsVisited: rr.functionsVisited },
      });
    };
    let m;
    // (a) accessor properties
    const descriptorSpans = [];
    const helpers = new Map(); // helper name -> {cls, kind}
    const DP_RE = /Object\.defineProperty\(\s*([A-Za-z_$][\w$]*)\.prototype\s*,\s*(?:'([^']+)'|([A-Za-z_$][\w$]*))\s*,\s*\{/g;
    while ((m = DP_RE.exec(text))) {
      const open = m.index + m[0].length - 1;
      const close = matchBrace(text, open);
      descriptorSpans.push([m.index, close]);
      const body = text.slice(open, close + 1);
      const kind = [/\bget\s*:/.test(body) ? 'get' : null, /\bset\s*:/.test(body) ? 'set' : null].filter(Boolean).join('/') || 'value';
      if (m[2]) pushSdk(m[1] + '.' + m[2], 'script object ' + m[1] + '.' + m[2], body, lineOf(text, m.index), humanize(m[2]) + ' (' + kind + ' property)');
      else {
        const fn = enclosingFunction(f, m.index);
        if (fn && fn.params.indexOf(m[3]) === 0 && !helpers.has(fn.name)) helpers.set(fn.name, { cls: m[1], kind });
      }
    }
    for (const [helper, h] of helpers) {
      const re = new RegExp("(?:^|[^\\w$.])" + helper.replace(/\$/g, '\\$') + "\\(\\s*'([^']+)'", 'g');
      while ((m = re.exec(text))) {
        const lineEnd = text.indexOf('\n', m.index);
        pushSdk(h.cls + '.' + m[1], 'script object ' + h.cls + '.' + m[1], text.slice(m.index, lineEnd < 0 ? text.length : lineEnd), lineOf(text, m.index), humanize(m[1]) + ' (' + h.kind + ' property, via ' + helper + ')');
      }
    }
    // (b) prototype methods
    const PM_RE = /(?:^|[^\w$.])([A-Z][\w$]*)\.prototype\.([A-Za-z_$][\w$]*)\s*=\s*function\s*\(/g;
    while ((m = PM_RE.exec(text))) {
      if (m[2].startsWith('_')) continue;
      const span = functionBodyAt(text, m.index + m[0].length - 1);
      pushSdk(m[1] + '.' + m[2], 'script object ' + m[1] + '.' + m[2] + '()', span ? text.slice(span.start, span.end + 1) : '', lineOf(text, m.index));
    }
    // (c) object-literal members
    const nsSpans = [];
    const NS_RE = /\bapi\.([A-Za-z_$][\w$]*)\s*=\s*\{/g;
    while ((m = NS_RE.exec(text))) { const open = m.index + m[0].length - 1; nsSpans.push({ ns: m[1], start: open, end: matchBrace(text, open) }); }
    const MEMBER_RE = /^\s{2,8}([A-Za-z_$][\w$]*)\s*(?::\s*function\s*\(|\(([^()'"]*)\)\s*\{)/gm;
    while ((m = MEMBER_RE.exec(text))) {
      const name = m[1];
      if (['if', 'for', 'while', 'switch', 'function', 'return', 'catch'].includes(name)) continue;
      if (descriptorSpans.some(([a, b]) => m.index > a && m.index < b)) continue; // accessor descriptor, modelled in (a)
      const ns = nsSpans.find((n) => m.index > n.start && m.index < n.end);
      const span = functionBodyAt(text, m.index + m[0].length - 1);
      const full = (ns ? ns.ns + '.' : '') + name;
      pushSdk(full, rootSurface + full, span ? text.slice(span.start, span.end + 1) : '', lineOf(text, m.index));
    }
  }

  // Row ids are the keys diffs between revisions use: make same-line repeats
  // (two menu items on one line) deterministic instead of colliding.
  const idCount = new Map();
  for (const r of rows) { const n = (idCount.get(r.id) || 0) + 1; idCount.set(r.id, n); if (n > 1) r.id += '#' + n; }

  // module globals (not rows; exposure summary)
  const globals = {};
  for (const f of idx) {
    const re = /window\.(SM[A-Za-z]*)\s*=\s*(?:window\.SM[A-Za-z]*\s*\|\|\s*)?\{/g;
    let m;
    while ((m = re.exec(f.text))) {
      const span = functionBodyAt(f.text, m.index + m[0].length - 1);
      const body = span ? f.text.slice(span.start, span.end + 1) : '';
      const members = uniq((body.match(/^\s{2,8}([A-Za-z_$][\w$]*)\s*:\s*(?:function|\()/gm) || []).map((s) => s.trim().split(/\s*:/)[0]));
      globals[m[1]] = { file: rel(f.file), line: lineOf(f.text, m.index), members: members.length };
    }
  }

  // Fixture coverage (tests/fixtures/manifest.json, R03): each row lists the
  // fixtures whose areas include its own; the manifest is an input, so adding
  // or changing a fixture makes the committed inventory stale until regenerated.
  const manifestPath = path.join(ROOT, 'tests', 'fixtures', 'manifest.json');
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

function csvOf(inv) {
  const esc = (v) => '"' + String(v === undefined || v === null ? '' : Array.isArray(v) ? v.join(' | ') : typeof v === 'object' ? Object.entries(v).map(([k, d]) => k + '@' + d).join(' ') : v).replace(/"/g, '""') + '"';
  const cols = ['id', 'kind', 'area', 'surface', 'capability', 'handler', 'events', 'exposure', 'sdk', 'mcp', 'consumers', 'fixtures', 'platforms', 'status', 'nextGate', 'source', 'reason'];
  return cols.join(',') + '\n' + inv.rows.map((r) => cols.map((c) => esc(r[c])).join(',')).join('\n') + '\n';
}

function mdOf(inv) {
  const c = inv.counts;
  const tbl = (obj) => Object.entries(obj).sort((a, b) => b[1] - a[1]).map(([k, v]) => `| ${k} | ${v} |`).join('\n');
  const unmapped = inv.rows.filter((r) => r.status === 'unmapped');
  const placeholders = inv.rows.filter((r) => r.status === 'unavailable-with-reason');
  const fixturesByArea = inv.fixturesByArea || {};
  return `# Nemo surface inventory

Generated by \`scripts/nemo/inventory.cjs\` (v${inv.generatorVersion}) from source \`${inv.source.head.slice(0, 12)}\` (${inv.source.describe}).
Inputs digest \`${inv.inputsDigest.slice(0, 12)}\`. Do not edit by hand; run \`npm run inventory\`. \`npm run inventory -- --check\` fails when this file is stale.

Row schema: \`surface -> capability -> handler -> UI/SDK/MCP exposure -> consumers -> fixtures -> platform status -> next gate\`.
Full data: [surfaces.json](surfaces.json) (all fields) and [surfaces.csv](surfaces.csv).

## Counts

| Kind | Rows |
|---|---|
${tbl(c.byKind)}

| Status | Rows |
|---|---|
${tbl(c.byStatus)}

| Area | Rows | Fixtures covering the area |
|---|---|---|
${Object.entries(c.byArea).sort((a, b) => b[1] - a[1]).map(([k, v]) => `| ${k} | ${v} | ${(fixturesByArea[k] || []).join(', ') || '(none yet)'} |`).join('\n')}

| Consumer reached (≤3 calls from the handler) | Rows |
|---|---|
${tbl(c.byConsumer)}

Consumer detection is static reachability from the bound handler through named
function calls, capped at three hops and 600 visited functions; a depth of \`1\`
means the handler body itself contains the consumer call. It over-approximates
through hub functions (\`updateUI\`, \`renderNow\`) and cannot see dynamic dispatch,
so treat it as "may touch", never as proof of coverage.

## How rows are found

- **dom** — every \`button\`, \`input\`, \`select\`, \`textarea\`, \`a\`, element with a
  \`data-tool/-tab/-pane/-mode/-value/-key/-align/...\` attribute, or an actionable
  class in \`src/index.html\`; bound through \`getElementById('id')\` /
  \`querySelector('#id')\`, id tables (\`getElementById(IDS[key])\`) and id arrays
  (\`['a','b'].forEach(id => getElementById(id)…)\`), compound selectors
  (\`#bar .cls[data-x]\`) and container event delegation. A control whose container
  is emptied and rebuilt at runtime is \`unavailable-with-reason\` (static placeholder).
- **shortcut** — the \`TOOL_SHORTCUTS\`, \`COMMAND_SHORTCUTS\` and \`READONLY_SHORTCUTS\`
  tables in \`src/js/timeline.js\`.
- **menu** — every object literal \`{label: …, action|run|onClick: …}\` in \`src/js\`
  (context menus, layer "+" menu, tween/curve menus).
- **labs** — every \`SMLabs.register('name', {...})\` under \`src/js/labs\`.
- **sdk** — the scripting API (\`nemo-script.js\`) and plugin API (\`nemo-plugin.js\`): object-literal
  members (namespaced, e.g. \`selection.layers\`), \`Class.prototype\` accessor properties and
  methods (\`Layer.name\`, \`Layer.strokes()\`).

MCP exposure is \`none\` for every row until R14 adds the adapter.

## Module globals (not rows)

${Object.keys(inv.globals).length} \`window.SM*\` objects expose ${Object.values(inv.globals).reduce((s, g) => s + g.members, 0)} function members between files. They are internal bridges, not shipped surfaces; the scripting API rows above are the supported script entry points.

| Global | Defined in | Function members |
|---|---|---|
${Object.entries(inv.globals).sort().map(([k, g]) => `| ${k} | ${g.file}:${g.line} | ${g.members} |`).join('\n')}

## Unmapped surfaces (${unmapped.length})

Rows with no handler binding found by static analysis. Each needs either a handler
binding (then it becomes \`inventoried\`) or an explicit \`unavailable-with-reason\`.
Decorative or purely CSS-driven controls belong in the second group.

| Row | Area | Capability | Source |
|---|---|---|---|
${unmapped.map((r) => `| \`${r.id}\` | ${r.area} | ${r.capability.replace(/\|/g, '/')} | ${r.source} |`).join('\n')}

## Static placeholders (${placeholders.length})

Markup present in \`src/index.html\` that its container empties and rebuilds at runtime
before it can receive input; the live controls are created and bound by the builder
named in the reason. Status \`unavailable-with-reason\`.

| Row | Area | Reason |
|---|---|---|
${placeholders.map((r) => `| \`${r.id}\` | ${r.area} | ${r.reason} |`).join('\n')}
`;
}

function renderOutputs(inv) {
  const json = JSON.stringify(inv, null, 1) + '\n';
  return { 'surfaces.json': json, 'surfaces.csv': csvOf(inv), 'SURFACES.md': mdOf(inv) };
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
  console.log(`inventory: ${inv.rows.length} rows (${JSON.stringify(inv.counts.byStatus)}) -> ${rel(OUT_DIR)}/`);
}

if (require.main === module) main();
module.exports = { build, renderOutputs, staleOutputs, OUT_DIR };
