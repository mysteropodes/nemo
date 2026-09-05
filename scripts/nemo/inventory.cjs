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

const ROOT = path.resolve(__dirname, '..', '..');
const OUT_DIR = path.join(ROOT, 'engineering', 'inventory');
const GENERATOR_VERSION = 4;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------
function read(p) { return fs.readFileSync(p, 'utf8'); }
function sha256(s) { return crypto.createHash('sha256').update(s).digest('hex'); }
function relTo(root) { return (p) => path.relative(root, p).split(path.sep).join('/'); }
function lineOf(text, offset) { let n = 1; for (let i = 0; i < offset; i++) if (text.charCodeAt(i) === 10) n++; return n; }
function uniq(a) { return Array.from(new Set(a)); }
function esc(s) { return String(s).replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); }
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
// Lexical regions. Every offset of a file is code, comment or literal (string,
// template, regex). `code` is the source with comments blanked to spaces
// (newlines kept, so offsets and line numbers are those of the file); `kind`
// says what each offset is. Passes run on `code` and reject any match that
// starts inside a literal, so a lookup or listener quoted in a comment or a
// string is never a binding. Template `${…}` expressions are code.
// ---------------------------------------------------------------------------
const CODE = 0, COMMENT = 1, LITERAL = 2;
const REGEX_PREV_CHAR = /[(,=:[!&|?{};+\-*%<>~^]/;
const REGEX_PREV_WORD = /^(?:return|typeof|case|do|else|in|of|new|delete|void|throw|instanceof|yield|await)$/;
function lexRegions(text) {
  const n = text.length;
  const kind = new Uint8Array(n);
  const out = text.split('');
  const mark = (a, b, k, blank) => { for (let j = a; j < b && j < n; j++) { kind[j] = k; if (blank && text[j] !== '\n') out[j] = ' '; } };
  let i = 0, depth = 0, lastSig = -1, inTemplate = false;
  const tmpl = []; // brace depth at each `${` still open
  const regexAllowed = () => {
    if (lastSig < 0) return true;
    const c = text[lastSig];
    if (REGEX_PREV_CHAR.test(c)) return true;
    if (/[\w$]/.test(c)) { let s = lastSig; while (s > 0 && /[\w$]/.test(text[s - 1])) s--; return REGEX_PREV_WORD.test(text.slice(s, lastSig + 1)); }
    return false;
  };
  while (i < n) {
    const c = text[i];
    if (inTemplate) {
      if (c === '\\') { mark(i, i + 2, LITERAL); i += 2; continue; }
      if (c === '`') { kind[i] = LITERAL; inTemplate = false; lastSig = i; i++; continue; }
      if (c === '$' && text[i + 1] === '{') { mark(i, i + 2, LITERAL); tmpl.push(depth); depth++; inTemplate = false; i += 2; continue; }
      kind[i] = LITERAL; i++; continue;
    }
    const d = text[i + 1];
    if (c === '/' && d === '/') { let e = text.indexOf('\n', i); if (e < 0) e = n; mark(i, e, COMMENT, true); i = e; continue; }
    if (c === '/' && d === '*') { let e = text.indexOf('*/', i + 2); e = e < 0 ? n : e + 2; mark(i, e, COMMENT, true); i = e; continue; }
    if (c === '\'' || c === '"') {
      let j = i + 1;
      while (j < n && text[j] !== c && text[j] !== '\n') { if (text[j] === '\\') j++; j++; }
      mark(i, j + 1, LITERAL); lastSig = Math.min(j, n - 1); i = j + 1; continue;
    }
    if (c === '`') { kind[i] = LITERAL; inTemplate = true; i++; continue; }
    if (c === '/' && regexAllowed()) {
      let j = i + 1, cls = false, ok = false;
      while (j < n) {
        const ch = text[j];
        if (ch === '\\') { j += 2; continue; }
        if (ch === '\n') break;
        if (cls) { if (ch === ']') cls = false; } else if (ch === '[') cls = true; else if (ch === '/') { ok = true; break; }
        j++;
      }
      if (ok) { j++; while (j < n && /[a-z]/i.test(text[j])) j++; mark(i, j, LITERAL); lastSig = j - 1; i = j; continue; }
    }
    if (c === '{') depth++;
    else if (c === '}') {
      if (tmpl.length && depth - 1 === tmpl[tmpl.length - 1]) { tmpl.pop(); depth--; kind[i] = LITERAL; inTemplate = true; i++; continue; }
      depth--;
    }
    if (!/\s/.test(c)) lastSig = i;
    i++;
  }
  return { code: out.join(''), kind };
}

// Offset of the bracket closing the one at `open`, skipping literals.
function matchClose(f, open) {
  const { code, kind } = f;
  const o = code[open], c = o === '(' ? ')' : o === '[' ? ']' : '}';
  let depth = 0;
  for (let i = open; i < code.length; i++) {
    if (kind[i] !== CODE) continue;
    const ch = code[i];
    if (ch === o) depth++;
    else if (ch === c) { depth--; if (depth === 0) return i; }
  }
  return code.length - 1;
}
// Body span of the function whose parameter list or arrow starts at/after `from`.
function functionBodyAt(f, from) {
  for (let i = from; i < f.code.length; i++) if (f.kind[i] === CODE && f.code[i] === '{') return { start: i, end: matchClose(f, i) };
  return null;
}
// Same on a detached comment-free slice (menu-item literals): strings skipped.
function bodySpanInText(text, from) {
  const open = text.indexOf('{', from);
  if (open < 0) return null;
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    const c = text[i];
    if (c === '\'' || c === '"' || c === '`') { const q = c; for (i++; i < text.length; i++) { if (text[i] === '\\') { i++; continue; } if (text[i] === q) break; if (q !== '`' && text[i] === '\n') break; } continue; }
    if (c === '{') depth++;
    else if (c === '}') { depth--; if (depth === 0) return { start: open, end: i }; }
  }
  return { start: open, end: text.length - 1 };
}

// ---------------------------------------------------------------------------
// JS index: named functions and their bodies
// ---------------------------------------------------------------------------
const FN_DECL = /(?:^|[^\w$.])function\s+([A-Za-z_$][\w$]*)\s*\(|([A-Za-z_$][\w$]*)\s*:\s*function\s*\(|(?:^|[^\w$.])(?:var|let|const)?\s*([A-Za-z_$][\w$]*)\s*=\s*function\s*\(|(?:^|[^\w$])([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?\([^()]*\)\s*=>\s*\{|(?:^|[^\w$])([A-Za-z_$][\w$]*)\s*:\s*(?:async\s*)?\([^()]*\)\s*=>\s*\{/g;

function indexFile(file) {
  const text = read(file);
  const { code, kind } = lexRegions(text);
  const f = { file, text, code, kind, fns: [] };
  FN_DECL.lastIndex = 0;
  let m;
  while ((m = FN_DECL.exec(code))) {
    const name = m[1] || m[2] || m[3] || m[4] || m[5];
    const at = m.index + m[0].indexOf(name);
    if (kind[at] !== CODE) continue;
    const body = functionBodyAt(f, m.index + m[0].length - 1);
    if (!body) continue;
    const paren = code.indexOf('(', at);
    const params = paren >= 0 && paren < body.start ? code.slice(paren + 1, matchClose(f, paren)).split(',').map((x) => x.trim().replace(/=[\s\S]*$/, '').trim()).filter(Boolean) : [];
    f.fns.push({ name, params, start: body.start, end: body.end, line: lineOf(code, at), declStart: m.index });
  }
  indexBindingScopes(f);
  return f;
}

// Binding ownership only, not a general JavaScript parser. Braces delimit
// lexical declarations; function/arrow parameters and `var` belong to their
// function. Keep anonymous scopes separate from the named reporting index.
function indexBindingScopes(f) {
  f.rootScope = { start: 0, end: f.code.length, function: true, params: [] };
  f.scopes = [f.rootScope];
  const stack = [];
  for (let i = 0; i < f.code.length; i++) {
    if (f.kind[i] !== CODE) continue;
    if (f.code[i] === '{') stack.push(i);
    else if (f.code[i] === '}' && stack.length) f.scopes.push({ start: stack.pop(), end: i });
  }
  const functions = /\bfunction\s*(?:[A-Za-z_$][\w$]*\s*)?\(([^()]*)\)|(?:\(([^()]*)\)|\b([A-Za-z_$][\w$]*))\s*=>/g;
  let m;
  while ((m = functions.exec(f.code))) {
    if (f.kind[m.index] !== CODE) continue;
    let start = m.index + m[0].length;
    while (/\s/.test(f.code[start] || '') && start < f.code.length) start++;
    let scope = f.scopes.find((s) => s.start === start && s !== f.rootScope);
    if (!scope && m[1] === undefined) {
      // Concise arrow body: stop at its containing delimiter, not the next `{`.
      let end = start;
      for (; end < f.code.length; end++) {
        if (f.kind[end] !== CODE) continue;
        if (/[([{]/.test(f.code[end])) end = matchClose(f, end);
        else if (/[),;\]}\n]/.test(f.code[end])) break;
      }
      scope = { start, end: end - 1 };
      f.scopes.push(scope);
    }
    if (scope) {
      scope.function = true;
      scope.params = (m[1] ?? m[2] ?? m[3] ?? '').split(',').map((p) => p.trim().replace(/=[\s\S]*$/, '').trim());
    }
  }
  f.scopes.sort((a, b) => (a.end - a.start) - (b.end - b.start));
  f.declarations = [];
  for (const scope of f.scopes) for (const name of scope.params || []) f.declarations.push({ name, scope });
  const decl = /\b(var|let|const|function)\s+([A-Za-z_$][\w$]*)/g;
  while ((m = decl.exec(f.code))) {
    if (f.kind[m.index] !== CODE) continue;
    const scope = f.scopes.find((s) => s.start <= m.index && m.index <= s.end && (m[1] !== 'var' || s.function));
    f.declarations.push({ name: m[2], scope });
  }
}

function bindingOwner(f, name, at) {
  const owners = f.declarations.filter((d) => d.name === name && d.scope.start <= at && at <= d.scope.end).map((d) => d.scope);
  return owners.sort((a, b) => (a.end - a.start) - (b.end - b.start))[0] || f.rootScope;
}

function enclosingFunction(f, offset) {
  let best = null;
  for (const fn of f.fns) {
    if (fn.start <= offset && offset <= fn.end) {
      if (!best || (fn.end - fn.start) < (best.end - best.start)) best = fn;
    }
  }
  return best;
}

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
  const f = indexFile(p);
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
function build(opts) {
  const root = (opts && opts.root) || ROOT;
  const src = path.join(root, 'src');
  const rel = relTo(root);
  const html = read(path.join(src, 'index.html'));
  const domRows = parseHtml(html);
  const i18n = loadI18n(src);
  const files = listJs(path.join(src, 'js'));
  const idx = files.map(indexFile);
  const byFile = new Map(idx.map((f) => [f.file, f]));

  // global function index (first definition wins; classic scripts share one scope)
  const fnEntry = (f, fn) => ({ file: f.file, f, name: fn.name, body: f.code.slice(fn.start, fn.end + 1), line: fn.line, params: fn.params, start: fn.start, end: fn.end, owner: bindingOwner(f, fn.name, fn.declStart + 1) });
  const allFns = idx.flatMap((f) => f.fns.map((fn) => fnEntry(f, fn)));
  const fnIndex = new Map();
  for (const fn of allFns) if (!fnIndex.has(fn.name)) fnIndex.set(fn.name, fn);
  // Resolve at the call site. A same-name helper in a sibling function is not
  // visible; ambiguous global definitions are deliberately not guessed.
  const resolveFn = (f, name, at) => {
    const owner = bindingOwner(f, name, at);
    const local = allFns.filter((fn) => fn.f === f && fn.name === name && fn.owner === owner);
    if (local.length) return local.length === 1 ? local[0] : null;
    if (owner !== f.rootScope || f.declarations.some((d) => d.name === name && d.scope === owner)) return null;
    const global = allFns.filter((fn) => fn.name === name && fn.owner === fn.f.rootScope);
    return global.length === 1 ? global[0] : null;
  };

  // Functions that take an element id as a parameter (`function el(id){return document.getElementById(id)}`,
  // `wireBoolBtn(id, op)`): a call `name('literal')` is a lookup of that id.
  const paramLookupRe = (prm) => new RegExp("getElementById\\(\\s*" + esc(prm) + "\\s*\\)|querySelector\\(\\s*'#'\\s*\\+\\s*" + esc(prm) + "(?![\\w$])");
  const idTakers = new Map(); // lexical function entry -> parameter index
  for (const fn of allFns) {
    fn.params.forEach((prm, i) => {
      if (idTakers.has(fn) || !/^[A-Za-z_$][\w$]*$/.test(prm)) return;
      if (paramLookups(fn.f, fn, prm).length) idTakers.set(fn, i);
    });
  }
  // Calls of known id-takers inside [span] of `f` that pass `prm` at the taker's id position.
  function takerCallsWithParam(f, span, prm, selfName) {
    const out = [];
    const re = /(?:^|[^\w$.])([A-Za-z_$][\w$]*)\s*\(/g;
    const body = f.code.slice(span.start, span.end + 1);
    let m;
    while ((m = re.exec(body))) {
      const at = span.start + m.index + m[0].indexOf(m[1]);
      const fn = resolveFn(f, m[1], at);
      if (!idTakers.has(fn) || fn === selfName) continue;
      const open = span.start + m.index + m[0].length - 1;
      if (f.kind[open] !== CODE) continue;
      const { args, close } = argsOf(f, open);
      const a = args[idTakers.get(fn)];
      if (a && a.text === prm && parameterIntact(f, span, prm, a.start)) out.push({ fn, at, close });
    }
    return out;
  }
  // A helper that forwards its parameter to an id-taker (`bindWidgetField(id, apply)` → `el(id)`) is one too.
  for (let round = 0; round < 4; round++) {
    let added = 0;
    for (const fn of allFns) {
      if (idTakers.has(fn)) continue;
      fn.params.forEach((prm, i) => {
        if (idTakers.has(fn) || !/^[A-Za-z_$][\w$]*$/.test(prm)) return;
        if (takerCallsWithParam(fn.f, fn, prm, fn).length) { idTakers.set(fn, i); added++; }
      });
    }
    if (!added) break;
  }
  // Zero-argument helpers that return a fixed element (`function macBtn() { return document.getElementById('mac-update-btn'); }`):
  // a call `macBtn()` is a lookup of that id.
  const returners = new Map(); // lexical function entry -> id
  for (const fn of allFns) {
    if (fn.params.length) continue;
    const returns = returnsOf(fn);
    const ids = returns.map((r) => {
      const m = /^return\s+(?:window\.)?document\.getElementById\(\s*(['"])([^'"]+)\1\s*\)\s*$/.exec(r.text);
      return m ? m[2] : null;
    });
    if (ids.length && ids.every((id) => id && id === ids[0])) returners.set(fn, ids[0]);
  }

  function reach(bodyText, depth) {
    // Returns { consumers: {name: depth}, functionsVisited } following calls up to `depth`.
    const seen = new Set();
    const hits = {};
    let frontier = [bodyText || ''];
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

  // -------------------------------------------------------------------------
  // Binding analysis. A "subject" is an expression that evaluates to an element
  // (or a NodeList): a lookup call, the variable it was assigned to, a helper
  // parameter that receives it, a forEach callback parameter. The analysis
  // returns the event registrations reachable from that subject and the
  // selectors scoped to it; nothing else counts as a binding.
  // -------------------------------------------------------------------------
  // (function declarations: hoisted, so the id-taker pass above can use them)
  function isWs(c) { return c === ' ' || c === '\n' || c === '\t' || c === '\r'; }
  function skipWs(f, i) { while (i < f.code.length && (isWs(f.code[i]) || f.kind[i] === COMMENT)) i++; return i; }
  function stringLiteral(t) { const m = /^'((?:[^'\\]|\\.)*)'$|^"((?:[^"\\]|\\.)*)"$/.exec(t); return m ? (m[1] !== undefined ? m[1] : m[2]) : null; }
  function firstParam(s) { const p = (s || '').split(',')[0].trim().replace(/=[\s\S]*$/, '').trim(); return /^[A-Za-z_$][\w$]*$/.test(p) ? p : null; }
  function EMPTY() { return { bindings: [], scoped: [] }; }
  function merge(a, b) { a.bindings.push(...b.bindings); a.scoped.push(...b.scoped); return a; }

  // Top-level argument spans of the call whose '(' is at `open`.
  function argsOf(f, open) {
    const close = matchClose(f, open);
    const spans = [];
    let depth = 0, s = open + 1;
    for (let i = open + 1; i < close; i++) {
      if (f.kind[i] !== CODE) continue;
      const ch = f.code[i];
      if (ch === '(' || ch === '[' || ch === '{') depth++;
      else if (ch === ')' || ch === ']' || ch === '}') depth--;
      else if (ch === ',' && depth === 0) { spans.push([s, i]); s = i + 1; }
    }
    if (spans.length || f.code.slice(s, close).trim()) spans.push([s, close]);
    const args = spans.map(([a, b]) => { const start = skipWs(f, a); return { start, end: b, text: f.code.slice(start, b).trim() }; });
    return { args, close };
  }
  function statementStart(f, at) {
    for (let i = at - 1; i >= 0; i--) { if (f.kind[i] !== CODE) continue; const c = f.code[i]; if (c === ';' || c === '{' || c === '}') return i + 1; }
    return 0;
  }
  function statementEnd(f, k) {
    let depth = 0;
    for (let i = k; i < f.code.length; i++) {
      if (f.kind[i] !== CODE) continue;
      const c = f.code[i];
      if (c === '(' || c === '[' || c === '{') depth++;
      else if (c === ')' || c === ']' || c === '}') { if (depth === 0) return i; depth--; }
      else if (depth === 0 && (c === ';' || c === '\n' || c === ',')) return i;
    }
    return f.code.length;
  }
  function parameterIntact(f, span, name, at) {
    const owner = bindingOwner(f, name, span.start);
    if (bindingOwner(f, name, at) !== owner) return false;
    const re = new RegExp('(?:^|[^\\w$.])(' + esc(name) + ')\\s*(?:=(?!=|>)|[+*/%&|^?-]=|\\+\\+|--)|(?:\\+\\+|--)\\s*(' + esc(name) + ')(?![\\w$])', 'g');
    let m;
    const body = f.code.slice(span.start, at);
    while ((m = re.exec(body))) {
      const u = span.start + m.index + m[0].indexOf(name);
      if (f.kind[u] === CODE && bindingOwner(f, name, u) === owner) return false;
    }
    return true;
  }
  function paramLookups(f, span, prm) {
    const re = new RegExp(paramLookupRe(prm).source, 'g');
    const out = [];
    let m;
    while ((m = re.exec(f.code.slice(span.start, span.end + 1)))) {
      const at = span.start + m.index;
      if (f.kind[at] === CODE && bindingOwner(f, prm, at) === bindingOwner(f, prm, span.start)) out.push(at);
    }
    return out;
  }
  function returnsOf(fn) {
    const out = [], re = /\breturn\b/g;
    let m;
    while ((m = re.exec(fn.body))) {
      const at = fn.start + m.index;
      if (fn.f.kind[at] !== CODE) continue;
      const scope = fn.f.scopes.find((s) => s.function && s.start <= at && at <= s.end);
      if (!scope || scope.start !== fn.start) continue;
      out.push({ at, text: fn.f.code.slice(at, statementEnd(fn.f, at)).trim() });
    }
    return out;
  }
  const subjectStartAt = (f, at) => { let s = at; if (f.code.slice(s - 9, s) === 'document.') { s -= 9; if (f.code.slice(s - 7, s) === 'window.') s -= 7; } return s; };

  // The function an event is bound to: inline (body span) or by name.
  function callbackOf(f, arg) {
    const t = arg.text || '';
    const none = { f, body: '', param0: null, span: null };
    let m;
    if ((m = /^(?:async\s+)?function\b[^(]*\(([^)]*)\)/.exec(t))) {
      const span = functionBodyAt(f, arg.start + m[0].length);
      return { f, body: span ? f.code.slice(span.start, span.end + 1) : '', param0: firstParam(m[1]), span };
    }
    if ((m = /^(?:async\s*)?(?:\(([^)]*)\)|([A-Za-z_$][\w$]*))\s*=>/.exec(t))) {
      const param0 = firstParam(m[1] !== undefined ? m[1] : m[2]);
      const k = skipWs(f, arg.start + m[0].length);
      if (f.code[k] === '{') { const span = { start: k, end: matchClose(f, k) }; return { f, body: f.code.slice(span.start, span.end + 1), param0, span }; }
      return { f, body: f.code.slice(k, arg.end), param0, span: { start: k, end: arg.end - 1 } };
    }
    if ((m = /^([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*)\s*(?:\.bind\([\s\S]*\))?$/.exec(t))) {
      const fn = resolveFn(f, m[1].split('.').pop(), arg.start);
      if (fn) return { f: fn.f, body: fn.body, param0: fn.params[0] || null, span: { start: fn.start, end: fn.end } };
    }
    return none;
  }

  // `subject.member…` at the `.` (or `?.`) at `dot`.
  function analyzeMember(f, dot, ctx) {
    const code = f.code, out = EMPTY();
    const i = dot + (code[dot] === '?' ? 2 : 1);
    const m = /^[A-Za-z_$][\w$]*/.exec(code.slice(i, i + 64));
    if (!m) return out;
    const name = m[0], j = skipWs(f, i + name.length);
    if (name === 'addEventListener' && code[j] === '(') {
      const { args } = argsOf(f, j);
      const ev = args[0] ? stringLiteral(args[0].text) : null;
      const cb = callbackOf(f, args[1] || { text: '' });
      out.bindings.push({ event: ev || '(dynamic)', body: cb.body, f, at: dot, via: ctx.via });
    } else if (/^on[a-z]+$/.test(name) && code[j] === '=' && code[j + 1] !== '=') {
      const k = skipWs(f, j + 1), end = statementEnd(f, k);
      const rhs = { start: k, end, text: code.slice(k, end).trim() };
      if (rhs.text && !/^(?:null|undefined)$/.test(rhs.text)) out.bindings.push({ event: name.slice(2), body: callbackOf(f, rhs).body, f, at: dot, via: ctx.via });
    } else if ((name === 'querySelector' || name === 'querySelectorAll') && code[j] === '(') {
      const { args, close } = argsOf(f, j);
      const sel = args[0] ? stringLiteral(args[0].text) : null;
      if (sel) out.scoped.push({ selector: sel, multi: name === 'querySelectorAll', start: ctx.subjectStart, end: close + 1 });
    } else if (name === 'forEach' && ctx.multi && code[j] === '(') {
      const { args } = argsOf(f, j);
      const cb = args[0] ? callbackOf(f, args[0]) : null;
      if (cb && cb.param0 && cb.span) merge(out, analyzeVariable(cb.f, cb.param0, cb.span, Object.assign({}, ctx, { multi: false, depth: ctx.depth + 1, assignAt: undefined, declAt: cb.span.start, via: 'callback ' + cb.param0 })));
    }
    return out;
  }

  // Uses of the element variable `name` inside `scope`: member registrations,
  // scoped selectors and calls that pass it to a helper. A use counts only when
  // the most recent assignment of `name` before it is the one that produced the
  // element (`assignAt`) and no nested function shadows the name.
  function analyzeVariable(f, name, scope, ctx) {
    const out = EMPTY();
    if (ctx.depth > 4) return out;
    const key = f.file + '#' + name + '@' + scope.start + ':' + (ctx.assignAt === undefined ? 'p' : ctx.assignAt);
    if (ctx.seen.has(key)) return out;
    ctx.seen.add(key);
    const code = f.code;
    const re = new RegExp('(?:^|[^\\w$.])(' + esc(name) + ')(?![\\w$])', 'g');
    const slice = code.slice(scope.start, scope.end + 1);
    const assigns = [], uses = [];
    const home = bindingOwner(f, name, ctx.declAt !== undefined ? ctx.declAt : scope.start);
    let m;
    while ((m = re.exec(slice))) {
      const u = scope.start + m.index + (m[0].length - m[1].length);
      if (f.kind[u] !== CODE || bindingOwner(f, name, u) !== home) continue;
      const after = skipWs(f, u + name.length);
      if (code[after] === '=' && code[after + 1] !== '=' && code[after + 1] !== '>') { assigns.push(u); continue; }
      uses.push({ u, after });
    }
    const own = ctx.assignAt === undefined ? undefined : assigns.filter((a) => a < ctx.assignAt).pop();
    for (const { u, after } of uses) {
      if (ctx.assignAt !== undefined && u < ctx.assignAt) continue;
      const last = assigns.filter((a) => a < u).pop();
      if (ctx.assignAt === undefined ? last !== undefined : last !== own) continue;
      if (code[after] === '.' || (code[after] === '?' && code[after + 1] === '.')) merge(out, analyzeMember(f, after, Object.assign({}, ctx, { subjectStart: u })));
      else merge(out, analyzeArgument(f, u, u + name.length, ctx));
    }
    return out;
  }

  // The subject spanning [start, end) is an argument of a call: follow it into
  // the callee's parameter (`bindSlider(el)`), through `Array.prototype.forEach.call(list, cb)`
  // or through an array conversion (`Array.from(list).forEach(...)`).
  function analyzeArgument(f, start, end, ctx) {
    const out = EMPTY();
    const code = f.code, kind = f.kind;
    let k = start - 1;
    while (k >= 0 && (isWs(code[k]) || kind[k] !== CODE)) k--;
    if (k < 0 || (code[k] !== '(' && code[k] !== ',')) return out;
    let depth = 0, open = -1;
    for (let i = k; i >= 0; i--) {
      if (kind[i] !== CODE) continue;
      const ch = code[i];
      if (ch === ')' || ch === ']' || ch === '}') depth++;
      else if (ch === '(') { if (depth === 0) { open = i; break; } depth--; }
      else if (ch === '[' || ch === '{') { if (depth === 0) return out; depth--; }
    }
    if (open < 0) return out;
    let e = open - 1;
    while (e >= 0 && isWs(code[e])) e--;
    let s = e;
    while (s >= 0 && /[\w$.]/.test(code[s])) s--;
    const callee = code.slice(s + 1, e + 1);
    if (!callee) return out;
    const { args, close } = argsOf(f, open);
    const argIndex = args.findIndex((a) => a.start <= start && start < a.end);
    if (argIndex < 0) return out;
    if (/\.forEach\.call$/.test(callee) && argIndex === 0) {
      const cb = args[1] ? callbackOf(f, args[1]) : null;
      if (cb && cb.param0 && cb.span) merge(out, analyzeVariable(cb.f, cb.param0, cb.span, Object.assign({}, ctx, { multi: false, depth: ctx.depth + 1, assignAt: undefined, declAt: cb.span.start, via: 'callback ' + cb.param0 })));
      return out;
    }
    if (/(?:^|\.)(?:slice\.call|Array\.from|from)$/.test(callee) && argIndex === 0) return merge(out, analyzeSubject(f, s + 1, close + 1, Object.assign({}, ctx, { multi: true, depth: ctx.depth + 1 })));
    const fn = resolveFn(f, callee.split('.').pop(), s + 1);
    if (fn && fn.params[argIndex] && ctx.depth < 3) merge(out, analyzeVariable(fn.f, fn.params[argIndex], { start: fn.start, end: fn.end }, Object.assign({}, ctx, { depth: ctx.depth + 1, assignAt: undefined, declAt: fn.start, via: 'helper ' + fn.name + '()' })));
    return out;
  }

  // What follows a subject expression spanning [start, end): a member chain, an
  // assignment to a variable, or a position in an argument list.
  function analyzeSubject(f, start, end, ctx) {
    const out = EMPTY();
    const i = skipWs(f, end);
    if (f.code[i] === '.' || (f.code[i] === '?' && f.code[i + 1] === '.')) return merge(out, analyzeMember(f, i, Object.assign({}, ctx, { subjectStart: start, via: ctx.via || 'chain' })));
    const prefix = f.code.slice(statementStart(f, start), start);
    const asg = /(?:^|[\s;,({[])(?:var|let|const)?\s*([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*)\s*=\s*(?:[^=;]*?(?:\|\||\?\?)\s*)?$/.exec(prefix);
    if (asg) return merge(out, analyzeVariable(f, asg[1], bindingOwner(f, asg[1], start), Object.assign({}, ctx, { assignAt: start, declAt: start, via: 'variable ' + asg[1] })));
    return merge(out, analyzeArgument(f, start, end, ctx));
  }

  // A lookup: bindings on the element itself plus, for each selector scoped to
  // it, the bindings on the elements that selector yields.
  function resolveLookup(f, start, end, ctx) {
    const r = analyzeSubject(f, start, end, ctx);
    const out = { bindings: r.bindings, scoped: [] };
    for (const sc of r.scoped) {
      const sub = resolveLookup(f, sc.start, sc.end, Object.assign({}, ctx, { multi: sc.multi, depth: ctx.depth + 1, assignAt: undefined, via: undefined }));
      if (sub.bindings.length) out.scoped.push({ selector: sc.selector, bindings: sub.bindings });
      for (const s2 of sub.scoped) out.scoped.push({ selector: sc.selector + ' ' + s2.selector, bindings: s2.bindings });
    }
    return out;
  }
  const newCtx = (multi, via) => ({ multi: !!multi, depth: 0, seen: new Set(), via });

  // One lookup site -> record. `sites` are the registration statements (file,
  // line, function) the row's handler column names; `ref` is the lookup itself,
  // kept for rows that are only referenced.
  function recFrom(f, at, res, via) {
    const fn = enclosingFunction(f, at);
    const where = (ff, off) => { const g = enclosingFunction(ff, off); return rel(ff.file) + ':' + lineOf(ff.code, off) + (g ? ' ' + g.name + '()' : ''); };
    const sites = uniq(res.bindings.map((b) => where(b.f, b.at)));
    return {
      file: res.bindings.length ? rel(res.bindings[0].f.file) : rel(f.file), line: lineOf(f.code, at), fn: fn ? fn.name : null,
      sites, ref: where(f, at), events: uniq(res.bindings.map((b) => b.event)), body: res.bindings.map((b) => b.body).join('\n'),
      binds: res.bindings.length > 0, via: res.bindings.length ? (via || res.bindings[0].via || 'chain') : null,
      fnBody: () => (fn ? f.code.slice(fn.start, fn.end + 1) : ''),
    };
  }

  const bindings = new Map();      // id -> [rec] (binding and reference records)
  const classBindings = new Map(); // selector -> [rec]
  const prefixBindings = [];       // {prefix, rec}
  const addRec = (map, key, rec) => { if (!map.has(key)) map.set(key, []); map.get(key).push(rec); };
  function record(f, at, start, end, target, multi, via) {
    const res = resolveLookup(f, start, end, newCtx(multi, via));
    const rec = recFrom(f, at, res, via);
    if (target.id) addRec(bindings, target.id, rec);
    else if (target.prefix) prefixBindings.push({ prefix: target.prefix, rec });
    else if (target.selector) addRec(classBindings, target.selector, rec);
    const scopePrefix = target.id ? '#' + target.id : target.selector || null;
    if (scopePrefix) for (const sc of res.scoped) addRec(classBindings, scopePrefix + ' ' + sc.selector, recFrom(f, at, { bindings: sc.bindings, scoped: [] }, via));
    return rec;
  }

  // A helper that looks its parameter up and binds it (`wireBoolBtn(id, op)`)
  // binds every id passed to it; a helper that merely returns the element
  // (`el(id)`) is transparent and the call site is analysed instead.
  // Bindings reached from the element that parameter `prm` names inside [span]:
  // its direct lookup (`getElementById(prm)`) and every id-taker it is passed to.
  function bindingsForParam(f, span, prm, via, selfName) {
    const res = EMPTY();
    for (const at of paramLookups(f, span, prm)) {
      if (!parameterIntact(f, span, prm, at)) continue;
      const open = f.code.indexOf('(', at);
      merge(res, resolveLookup(f, subjectStartAt(f, at), matchClose(f, open) + 1, Object.assign(newCtx(false, via), { depth: 1 })));
    }
    for (const call of takerCallsWithParam(f, span, prm, selfName)) {
      const tb = takerBinding(call.fn);
      if (tb && tb.bindings.length) merge(res, tb);
      if (tb && tb.transparent) merge(res, resolveLookup(f, call.at, call.close + 1, Object.assign(newCtx(false, via), { depth: 1 })));
    }
    return res;
  }
  const takerCache = new Map();
  function takerBinding(fn) {
    if (takerCache.has(fn)) return takerCache.get(fn);
    takerCache.set(fn, null);
    const prm = fn.params[idTakers.get(fn)];
    const res = bindingsForParam(fn.f, fn, prm, 'helper ' + fn.name + '()', fn);
    const returns = returnsOf(fn);
    res.transparent = returns.length > 0 && returns.every((r) => {
      if (!parameterIntact(fn.f, fn, prm, r.at)) return false;
      return new RegExp('^return\\s+(?:window\\.)?document\\.(?:' + paramLookupRe(prm).source + ')\\s*$').test(r.text);
    });
    takerCache.set(fn, res);
    return res;
  }

  // DOM lookups: getElementById / querySelector(All) with a literal argument,
  // and calls of id-taking helpers with a literal argument.
  const takerNames = uniq(Array.from(idTakers.keys(), (fn) => fn.name)).filter((n) => n.length > 1 || n === '$');
  const LOOKUP_RE = new RegExp("((?:window\\.)?document\\.)?\\b(getElementById|querySelectorAll|querySelector)\\s*\\(\\s*(['\"])([^'\"]+)\\3\\s*(\\)|\\+)" + (takerNames.length ? "|(?:^|[^\\w$.])(" + takerNames.map(esc).join('|') + ")\\s*\\(" : ''), 'g');
  for (const f of idx) {
    LOOKUP_RE.lastIndex = 0;
    let m;
    while ((m = LOOKUP_RE.exec(f.code))) {
      if (m[2]) {
        const nameAt = m.index + (m[1] ? m[1].length : 0);
        if (f.kind[nameAt] !== CODE) continue;
        if (!m[1] && f.code[nameAt - 1] === '.') continue; // `something.querySelector(…)`: scoped to an element, analysed where that element is bound
        const open = f.code.indexOf('(', nameAt);
        const start = m.index, end = matchClose(f, open) + 1;
        const multi = m[2] === 'querySelectorAll';
        if (m[2] === 'getElementById') { if (m[5] === '+') record(f, nameAt, start, end, { prefix: m[4] }, false); else record(f, nameAt, start, end, { id: m[4] }, false); }
        else if (m[5] === ')') { if (/^#[\w-]+$/.test(m[4])) record(f, nameAt, start, end, { id: m[4].slice(1) }, multi); else record(f, nameAt, start, end, { selector: m[4] }, multi); }
      } else if (m[6]) {
        const nameAt = m.index + m[0].indexOf(m[6]);
        if (f.kind[nameAt] !== CODE) continue;
        const open = m.index + m[0].length - 1;
        const { args, close } = argsOf(f, open);
        const fn = resolveFn(f, m[6], nameAt);
        if (!idTakers.has(fn)) continue;
        const a = args[idTakers.get(fn)];
        if (!a) continue;
        const lit = stringLiteral(a.text);
        const pre = lit === null ? /^(['"])([^'"]+)\1\s*\+/.exec(a.text) : null;
        if (lit === null && !pre) continue;
        const via = 'helper ' + m[6] + '()';
        const tb = takerBinding(fn);
        if (tb && tb.bindings.length) { const rec = recFrom(f, nameAt, tb, via); if (lit !== null) addRec(bindings, lit, rec); else prefixBindings.push({ prefix: pre[2], rec }); }
        else if (tb && tb.transparent) {
          if (lit !== null) record(f, nameAt, nameAt, close + 1, { id: lit }, false, via);
          else record(f, nameAt, nameAt, close + 1, { prefix: pre[2] }, false, via);
        } else {
          const rec = recFrom(f, nameAt, EMPTY(), via);
          rec.limit = 'helper parameter has no proved registration or unchanged returned lookup (reassignment, shadowing or unsupported return flow)';
          if (lit !== null) addRec(bindings, lit, rec);
          else prefixBindings.push({ prefix: pre[2], rec });
        }
      }
    }
  }
  if (returners.size) {
    const RETURNER_CALL_RE = new RegExp('(?:^|[^\\w$.])(' + uniq(Array.from(returners.keys(), (fn) => fn.name)).map(esc).join('|') + ')\\s*\\(\\s*\\)', 'g');
    for (const f of idx) {
      RETURNER_CALL_RE.lastIndex = 0;
      let m;
      while ((m = RETURNER_CALL_RE.exec(f.code))) {
        const nameAt = m.index + m[0].indexOf(m[1]);
        if (f.kind[nameAt] !== CODE || /function\s*$/.test(f.code.slice(Math.max(0, nameAt - 12), nameAt))) continue; // the definition itself
        const fn = resolveFn(f, m[1], nameAt);
        if (returners.has(fn)) record(f, nameAt, nameAt, m.index + m[0].length, { id: returners.get(fn) }, false, 'helper ' + m[1] + '()');
      }
    }
  }
  // Lookups that go through a table of id literals — an object or an array,
  // flat or of objects — either directly (`getElementById(IDS[mode])`,
  // `getElementById(IDS.unite)`) or through the parameter of a callback that
  // iterates it (`tabs.forEach(function (t) { getElementById(t.btn)… })`), and
  // through an array of id literals (`['a', 'b'].forEach(id => getElementById(id)…)`).
  // Every id literal of the table is bound at the lookup site.
  const domIds = new Set(domRows.map((r) => r.id).filter(Boolean));
  const tableRoots = new Map(); // file -> [{name, ids, start, end}]: identifier that denotes the table (or one of its entries) inside [start, end]
  const addRoot = (f, root) => { if (!tableRoots.has(f.file)) tableRoots.set(f.file, []); tableRoots.get(f.file).push(root); };
  const TABLE_RE = /(?:^|[^\w$.])(?:var|let|const)\s+([A-Za-z_$][\w$]*)\s*=\s*([[{])/g;
  for (const f of idx) {
    TABLE_RE.lastIndex = 0;
    let m;
    while ((m = TABLE_RE.exec(f.code))) {
      const open = m.index + m[0].length - 1;
      if (f.kind[open] !== CODE) continue;
      const close = matchClose(f, open);
      if (close - open > 20000) continue;
      const lit = f.code.slice(open, close + 1);
      const ids = uniq((lit.match(/'([\w-]+)'|"([\w-]+)"/g) || []).map((s) => s.slice(1, -1)).filter((s) => domIds.has(s)));
      if (!ids.length) continue;
      const name = m[1];
      addRoot(f, { name, ids, start: 0, end: f.code.length });
      const iter = new RegExp('(?:^|[^\\w$.])' + esc(name) + '\\s*\\.\\s*(?:forEach|map|some|every|filter)\\s*\\(', 'g');
      let c;
      while ((c = iter.exec(f.code))) {
        const cb = callbackOf(f, argsOf(f, c.index + c[0].length - 1).args[0] || { text: '' });
        if (cb.param0 && cb.span && cb.f === f) addRoot(f, { name: cb.param0, ids, start: cb.span.start, end: cb.span.end });
      }
      const forOf = new RegExp('for\\s*\\(\\s*(?:var|let|const)\\s+([A-Za-z_$][\\w$]*)\\s+of\\s+' + esc(name) + '\\s*\\)', 'g');
      while ((c = forOf.exec(f.code))) { const span = functionBodyAt(f, c.index + c[0].length); if (span) addRoot(f, { name: c[1], ids, start: span.start, end: span.end }); }
    }
  }
  const TABLE_LOOKUP_RE = /getElementById\(\s*([A-Za-z_$][\w$]*)\s*[[.]/g;
  const ARRAY_FOREACH_RE = /\[((?:\s*'[\w-]+'\s*,?)+)\]\s*\.forEach\(\s*(?:function\s*\(\s*([\w$]+)\s*\)|\(?\s*([\w$]+)\s*\)?\s*=>)\s*\{/g;
  const recordAt = (f, at) => { const open = f.code.indexOf('(', at); return recFrom(f, at, resolveLookup(f, subjectStartAt(f, at), matchClose(f, open) + 1, newCtx(false))); };
  for (const f of idx) {
    let m;
    TABLE_LOOKUP_RE.lastIndex = 0;
    while ((m = TABLE_LOOKUP_RE.exec(f.code))) {
      if (f.kind[m.index] !== CODE) continue;
      const roots = (tableRoots.get(f.file) || []).filter((r) => r.name === m[1] && r.start <= m.index && m.index <= r.end);
      if (!roots.length) continue;
      const rec = recordAt(f, m.index);
      for (const id of uniq(roots.flatMap((r) => r.ids))) addRec(bindings, id, rec);
    }
    ARRAY_FOREACH_RE.lastIndex = 0;
    while ((m = ARRAY_FOREACH_RE.exec(f.code))) {
      if (f.kind[m.index] !== CODE) continue;
      const prm = m[2] || m[3];
      const span = functionBodyAt(f, m.index + m[0].length - 1);
      if (!span) continue;
      const ids = (m[1].match(/'([\w-]+)'/g) || []).map((s) => s.slice(1, -1)).filter((s) => domIds.has(s));
      if (!ids.length) continue;
      const rec = recFrom(f, span.start, bindingsForParam(f, span, prm, 'callback ' + prm, null), 'callback ' + prm);
      for (const id of ids) addRec(bindings, id, rec);
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
  const hasClass = (list, cls) => new RegExp('(^|\\s)' + esc(cls) + '(\\s|$)').test(list || '');
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
    for (const [sel, recs] of classBindings) if (matchesSelector(row, sel)) out.push(...recs.filter((r) => r.binds));
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
      const delegates = recs.filter((r) => r.binds && r.events.some((ev) => INPUT_EVENTS.test(ev)) && /\b(e|ev|evt|event)\.target\b|\.closest\(/.test(r.body));
      if (delegates.length) return { kind: 'delegation', recs: delegates, container: cid };
      const rebuilds = recs.filter((r) => { const b = r.fnBody(); return /\.innerHTML\s*=\s*(''|""|``)|\.replaceChildren\(/.test(b) && /createElement\(/.test(b); });
      if (rebuilds.length) {
        const r0 = rebuilds[0];
        const builder = r0.ref;
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
    const own = r.id ? (bindings.get(r.id) || []) : [];
    let recs = own.filter((x) => x.binds);
    const references = uniq(own.filter((x) => !x.binds).map((x) => x.ref));
    if (!recs.length && r.id) recs = prefixBindings.filter((b) => b.rec.binds && r.id.startsWith(b.prefix) && b.prefix.length >= 3).map((b) => b.rec);
    if (!recs.length) recs = delegatedFor(r);
    let placeholder = null, via = null;
    if (!recs.length) {
      const fb = containerFallback(r);
      if (fb && fb.kind === 'delegation') { recs = fb.recs; via = 'container delegation #' + fb.container; }
      else if (fb) { placeholder = fb; via = 'container rebuild #' + fb.container; }
    }
    if (!recs.length && r.href && /^(https?:|mailto:)/.test(r.href)) recs = [{ file: 'href', sites: ['href ' + r.href], events: ['click'], body: '', binds: true, via: 'href' }];
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
      handler: recs.length ? uniq(recs.flatMap((x) => x.sites)).slice(0, 4) : placeholder ? placeholder.handler : [],
      events: uniq(recs.flatMap((x) => x.events)),
      exposure, mcp: 'none (R14)', sdk: 'none',
      consumers, platforms: platformFor(recs, consumers), status,
      reason: placeholder ? placeholder.reason : status === 'unmapped' ? uniq(own.map((x) => x.limit).filter(Boolean)).join('; ') || undefined : undefined,
      nextGate: status === 'unmapped' ? 'R03 follow-up: bind a handler or record unavailable-with-reason' + (references.length ? ' (referenced without an event registration at ' + references[0] + ')' : '') : status === 'unavailable-with-reason' ? 'none for the static markup; characterize the runtime-built controls through ' + placeholder.builder : 'R12/R13: characterize against a fixture and diagnostics',
      source: 'src/index.html:' + r.line,
      meta: { data: r.data, i18n: r.i18n, container: r.container, modal: r.modal, hiddenAtLoad: r.hiddenAtLoad, functionsVisited: visited, via: via || (recs[0] && recs[0].via) || undefined, references },
    });
  }

  // 2. Shortcut tables (timeline.js)
  const tl = byFile.get(path.join(src, 'js', 'timeline.js'));
  if (tl) {
    for (const table of ['TOOL_SHORTCUTS', 'COMMAND_SHORTCUTS', 'READONLY_SHORTCUTS']) {
      const at = tl.code.indexOf('var ' + table + '=[');
      if (at < 0) continue;
      const end = tl.code.indexOf('\n];', at);
      const block = tl.code.slice(at, end);
      const re = /\{action:'([^']*)',key:'([^']*)'(?:,cat:'([^']*)')?,label:'([^']*)'|\{keys:'([^']*)',label:'([^']*)',cat:'([^']*)'\}/g;
      let m;
      while ((m = re.exec(block))) {
        const action = m[1] || 'readonly:' + m[6];
        const key = m[2] !== undefined ? m[2] : m[5];
        const label = m[4] || m[6];
        const runAt = block.indexOf('run:function', m.index);
        let body = '';
        if (m[1] && runAt > 0 && runAt < block.indexOf('}', m.index + 200) + 400) { const span = bodySpanInText(block, runAt); if (span) body = block.slice(span.start, span.end + 1); }
        if (table === 'TOOL_SHORTCUTS') body = 'setTool(' + JSON.stringify(m[1]) + ')';
        const rr = reach(body, 3);
        rows.push({
          id: 'shortcut:' + action, kind: 'shortcut', surface: 'keyboard ' + (key || '(unbound)'),
          capability: i18n[label] || label, area: table === 'TOOL_SHORTCUTS' ? 'drawing-selection' : 'timeline-layers-frames',
          handler: [rel(tl.file) + ':' + lineOf(tl.code, at + m.index) + ' ' + table], events: ['keydown'],
          exposure: ['keyboard', table === 'READONLY_SHORTCUTS' ? 'Settings > Shortcuts (read-only)' : 'Settings > Shortcuts (remappable)'], mcp: 'none (R14)', sdk: 'none',
          consumers: rr.consumers, platforms: 'browser and desktop', status: 'inventoried',
          nextGate: 'R12/R13: characterize against a fixture and diagnostics', source: rel(tl.file) + ':' + lineOf(tl.code, at + m.index),
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
    while ((m = re.exec(f.code))) {
      if (f.kind[m.index] !== CODE) continue;
      const span = functionBodyAt(f, m.index + m[0].length - 1);
      const body = span ? f.code.slice(span.start, span.end + 1) : '';
      const desc = (body.match(/describe\s*:\s*'([^']+)'/) || [])[1];
      const rr = reach(body, 2);
      rows.push({
        id: 'labs:' + m[1], kind: 'labs', surface: 'Settings > Labs / SMLabs.' + m[1], capability: (desc && i18n[desc]) || desc || m[1], area: 'labs',
        handler: [rel(f.file) + ':' + lineOf(f.code, m.index)], events: [], exposure: ['Labs panel (opt-in prototype)', 'script (window.SMLabs)'], mcp: 'none (R14)', sdk: 'SMLabs',
        consumers: rr.consumers, platforms: 'browser and desktop', status: 'inventoried',
        nextGate: 'R03/R18.6: decide ship, keep as Labs, or retire; Labs are not release surfaces', source: rel(f.file) + ':' + lineOf(f.code, m.index), meta: { functionsVisited: rr.functionsVisited },
      });
    }
  }

  // 4. Context-menu / dynamic menu items: {label:SM.t('key')...action:function
  for (const f of idx) {
    const re = /\{\s*label\s*:\s*(?:SM\.t\('([^']+)'\)|'((?:[^'\\]|\\.)*)'|"([^"]*)")/g;
    let m;
    while ((m = re.exec(f.code))) {
      if (f.kind[m.index] !== CODE) continue;
      const end = matchClose(f, m.index);
      const lit = f.code.slice(m.index, end + 1);
      if (!/\b(action|run|onClick|onclick|fn)\s*:/.test(lit)) continue; // not an actionable item
      const key = m[1];
      const label = (key && i18n[key]) || m[2] || m[3] || key;
      const act = lit.search(/\b(action|run|onClick|onclick|fn)\s*:/);
      let body = '';
      const span = act >= 0 ? bodySpanInText(lit, act) : null;
      if (span) body = lit.slice(span.start, span.end + 1);
      else { const ref = lit.slice(act).match(/:\s*([A-Za-z_$][\w$.]*)/); if (ref && fnIndex.get(ref[1].split('.').pop())) body = fnIndex.get(ref[1].split('.').pop()).body; }
      const fn = enclosingFunction(f, m.index);
      const rr = reach(body, 3);
      const line = lineOf(f.code, m.index);
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
    const f = byFile.get(path.join(src, 'js', apiFile));
    if (!f) continue;
    const text = f.code;
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
      if (f.kind[m.index] !== CODE) continue;
      const open = m.index + m[0].length - 1;
      const close = matchClose(f, open);
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
      const re = new RegExp("(?:^|[^\\w$.])" + esc(helper) + "\\(\\s*'([^']+)'", 'g');
      while ((m = re.exec(text))) {
        const lineEnd = text.indexOf('\n', m.index);
        pushSdk(h.cls + '.' + m[1], 'script object ' + h.cls + '.' + m[1], text.slice(m.index, lineEnd < 0 ? text.length : lineEnd), lineOf(text, m.index), humanize(m[1]) + ' (' + h.kind + ' property, via ' + helper + ')');
      }
    }
    // (b) prototype methods
    const PM_RE = /(?:^|[^\w$.])([A-Z][\w$]*)\.prototype\.([A-Za-z_$][\w$]*)\s*=\s*function\s*\(/g;
    while ((m = PM_RE.exec(text))) {
      if (m[2].startsWith('_')) continue;
      const span = functionBodyAt(f, m.index + m[0].length - 1);
      pushSdk(m[1] + '.' + m[2], 'script object ' + m[1] + '.' + m[2] + '()', span ? text.slice(span.start, span.end + 1) : '', lineOf(text, m.index));
    }
    // (c) object-literal members
    const nsSpans = [];
    const NS_RE = /\bapi\.([A-Za-z_$][\w$]*)\s*=\s*\{/g;
    while ((m = NS_RE.exec(text))) { const open = m.index + m[0].length - 1; nsSpans.push({ ns: m[1], start: open, end: matchClose(f, open) }); }
    const MEMBER_RE = /^\s{2,8}([A-Za-z_$][\w$]*)\s*(?::\s*function\s*\(|\(([^()'"]*)\)\s*\{)/gm;
    while ((m = MEMBER_RE.exec(text))) {
      const name = m[1];
      if (['if', 'for', 'while', 'switch', 'function', 'return', 'catch'].includes(name)) continue;
      if (descriptorSpans.some(([a, b]) => m.index > a && m.index < b)) continue; // accessor descriptor, modelled in (a)
      const ns = nsSpans.find((n) => m.index > n.start && m.index < n.end);
      const span = functionBodyAt(f, m.index + m[0].length - 1);
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
    while ((m = re.exec(f.code))) {
      if (f.kind[m.index] !== CODE) continue;
      const span = functionBodyAt(f, m.index + m[0].length - 1);
      const body = span ? f.code.slice(span.start, span.end + 1) : '';
      const members = uniq((body.match(/^\s{2,8}([A-Za-z_$][\w$]*)\s*:\s*(?:function|\()/gm) || []).map((s) => s.trim().split(/\s*:/)[0]));
      globals[m[1]] = { file: rel(f.file), line: lineOf(f.code, m.index), members: members.length };
    }
  }

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
  const cell = (s) => String(s).replace(/\|/g, '/');
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
  class in \`src/index.html\`. A row is \`inventoried\` only when static analysis finds
  an **event registration** on that element (\`addEventListener\` or \`on<event> =\`),
  reached from a \`getElementById('id')\` / \`querySelector('#id')\` lookup through the
  element's own member chain, the variable it is assigned to (same function scope,
  latest assignment wins, shadowed names excluded), a helper that receives the element
  or its id as a parameter, a \`forEach\` callback over \`querySelectorAll\`, a selector
  scoped to the element, an id table (\`getElementById(IDS[key])\`) or id array
  (\`['a','b'].forEach(id => …)\`), a compound selector (\`#bar .cls[data-x]\`), or
  delegation on a bound ancestor (\`e.target\` / \`.closest\`). Comments, strings and
  template literals are never read. A lookup that only reads or writes the element
  (\`.value\`, \`.disabled\`, \`.textContent\`, …) is a *reference*: it is listed with the
  row but does not bind it, so such controls stay explicit \`unmapped\` rows below.
  A control whose container is emptied and rebuilt at runtime is
  \`unavailable-with-reason\` (static placeholder).
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

Rows with no event registration found by static analysis. Each needs either a
binding (then it becomes \`inventoried\`) or an explicit \`unavailable-with-reason\`.
"Referenced at" lists the places that read or write the element without registering
an event: the control's effect, if any, goes through the handler of another surface
(a form field read on submit, a state toggled from a menu). Decorative or purely
CSS-driven controls belong in \`unavailable-with-reason\`.

| Row | Area | Capability | Source | Referenced at |
|---|---|---|---|---|
${unmapped.map((r) => `| \`${r.id}\` | ${r.area} | ${cell(r.capability)} | ${r.source} | ${cell(((r.meta && r.meta.references) || []).join(', ') || '—')} |`).join('\n')}

## Static placeholders (${placeholders.length})

Markup present in \`src/index.html\` that its container empties and rebuilds at runtime
before it can receive input; the live controls are created and bound by the builder
named in the reason. Status \`unavailable-with-reason\`.

| Row | Area | Reason |
|---|---|---|
${placeholders.map((r) => `| \`${r.id}\` | ${r.area} | ${cell(r.reason)} |`).join('\n')}
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
  console.log(`inventory: ${inv.rows.length} rows (${JSON.stringify(inv.counts.byStatus)}) -> ${relTo(ROOT)(OUT_DIR)}/`);
}

if (require.main === module) main();
module.exports = { build, renderOutputs, staleOutputs, lexRegions, OUT_DIR };
