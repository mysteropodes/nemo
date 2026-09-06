'use strict';
// Static surface rows, area classification and document-consumer reachability.
const path = require('node:path');
const { CODE, matchClose, functionBodyAt, bodySpanInText, enclosingFunction,
  lineOf, uniq, esc } = require('./inventory-lexer.cjs');

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

function createSurfaces({ src, rel, domRows, i18n, idx, fnIndex, bindings, classBindings, prefixBindings }) {
  const byFile = new Map(idx.map((f) => [f.file, f]));
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

  return { rows, globals };
}

module.exports = { createSurfaces, CONSUMERS };
