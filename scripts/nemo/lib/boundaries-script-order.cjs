'use strict';
// Static text/html inventory, using an explicitly injected parse5-compatible parser.
// No JavaScript evaluation, network access, global dependency or readiness inference.
const fs = require('node:fs');
const path = require('node:path');
const { fileURLToPath, pathToFileURL } = require('node:url');

const HTML = 'http://www.w3.org/1999/xhtml';
const trimASCII = (value) => value.replace(/^[\t\n\f\r ]+|[\t\n\f\r ]+$/g, '');
const lowerASCII = (value) => value.replace(/[A-Z]/g, (c) => c.toLowerCase());
const JS_TYPES = new Set([
  'application/ecmascript', 'application/javascript', 'application/x-ecmascript',
  'application/x-javascript', 'text/ecmascript', 'text/javascript',
  'text/javascript1.0', 'text/javascript1.1', 'text/javascript1.2',
  'text/javascript1.3', 'text/javascript1.4', 'text/javascript1.5',
  'text/jscript', 'text/livescript', 'text/x-ecmascript', 'text/x-javascript',
]);

function anchor(location) {
  if (!location) return null;
  const { startLine, startCol, startOffset, endLine, endCol, endOffset } = location;
  return { startLine, startCol, startOffset, endLine, endCol, endOffset };
}

function inside(root, target) {
  const rel = path.relative(root, target);
  return rel === '' || (!rel.startsWith(`..${path.sep}`) && rel !== '..' && !path.isAbsolute(rel));
}

function relativePath(value, label, allowDot = false) {
  if (typeof value !== 'string' || !value || /[\\\x00-\x1f\x7f]/.test(value)
      || path.posix.isAbsolute(value)
      || (!(allowDot && value === '.') && value.split('/').some((s) => !s || s === '.' || s === '..'))) {
    throw new TypeError(`${label} must be a normalized repository-relative path`);
  }
  return value;
}

function configuration({ root, htmlFile, sourceRoot }) {
  if (typeof root !== 'string' || !path.isAbsolute(root)) throw new TypeError('root must be absolute');
  relativePath(htmlFile, 'htmlFile');
  if (sourceRoot !== undefined) relativePath(sourceRoot, 'sourceRoot', true);
  const physicalRoot = fs.realpathSync(root);
  const htmlPath = path.resolve(physicalRoot, htmlFile);
  const servedRoot = sourceRoot === undefined ? null : path.resolve(physicalRoot, sourceRoot);
  if (servedRoot && !inside(servedRoot, htmlPath)) throw new TypeError('htmlFile must be within sourceRoot');
  // Do not guess whether a host serves an HTML symlink using logical or physical URLs.
  const htmlParent = path.dirname(htmlPath);
  const mappingUncertain = fs.realpathSync(htmlParent) !== htmlParent
    || (fs.existsSync(htmlPath) && fs.realpathSync(htmlPath) !== htmlPath);
  return { root: physicalRoot, htmlFile, htmlPath, servedRoot, mappingUncertain };
}

function scriptKind(attrs, namespace) {
  if (namespace !== HTML) return 'foreign';
  // WHATWG prepare-the-script-element: whitespace-only type is NOT an empty attribute.
  let type;
  if (attrs.type === '' || (attrs.type === undefined && !attrs.language)) type = 'text/javascript';
  else type = attrs.type === undefined ? `text/${attrs.language}` : trimASCII(attrs.type);
  type = lowerASCII(type);
  if (JS_TYPES.has(type)) return 'classic';
  return ['module', 'importmap', 'speculationrules'].includes(type) ? type : 'data';
}

function resolveSource(value, config, basePresent) {
  const unknown = (reason) => ({ status: 'unknown', reason });
  const src = trimASCII(value);
  if (!src) return { status: 'invalid', reason: 'empty-src' };
  if (/[\\\x00-\x1f\x7f]/.test(src)) return unknown('control-or-backslash-url');
  if (/^(https?:|\/\/)/i.test(src)) {
    try {
      new URL(src, 'https://inventory.invalid/');
      return { status: 'nonlocal', reason: 'network-url' };
    } catch { return { status: 'invalid', reason: 'invalid-network-url' }; }
  }
  if (/^[a-z][a-z\d+.-]*:/i.test(src)) return unknown('unsupported-url-scheme');
  if (basePresent) return unknown('base-href-not-modeled');
  if (config.mappingUncertain) return unknown('symlinked-html-url-base');
  if (src.startsWith('/') && !config.servedRoot) return unknown('root-relative-src-needs-sourceRoot');
  try {
    const base = src.startsWith('/')
      ? pathToFileURL(`${config.servedRoot}${path.sep}`) : pathToFileURL(config.htmlPath);
    const url = new URL(src.startsWith('/') ? `.${src}` : src, base);
    if (url.protocol !== 'file:' || url.host) return unknown('unsupported-url-mapping');
    const target = fileURLToPath(url); // Decode pathname only, preserving encoded ? and #.
    if (/[\\\x00-\x1f\x7f]/.test(target)) return unknown('unsafe-decoded-path');
    const boundary = config.servedRoot || config.root;
    if (!inside(boundary, target)) return unknown('path-outside-source-root');
    const physical = fs.realpathSync(target);
    if (!inside(boundary, physical) || !inside(config.root, physical)) return unknown('symlink-outside-source-root');
    if (!fs.statSync(physical).isFile()) return { status: 'unresolved', reason: 'not-a-file' };
    return {
      status: 'local', path: path.relative(config.root, physical).split(path.sep).join('/'),
      logicalPath: path.relative(config.root, target).split(path.sep).join('/'),
      query: url.search, fragment: url.hash,
    };
  } catch (error) {
    if (['ENOENT', 'ENOTDIR'].includes(error.code)) return { status: 'unresolved', reason: 'missing-file' };
    return unknown(`local-url-resolution-${error.code || error.name}`);
  }
}

function collectNodes(document) {
  const entries = [], stack = [{ node: document, inert: false, shadow: false }];
  while (stack.length) {
    const { node, inert, shadow } = stack.pop();
    const attrs = Object.fromEntries((node.attrs || []).filter((a) => !a.namespace).map((a) => [a.name, a.value]));
    if (node.tagName === 'script' || (node.tagName === 'base' && node.namespaceURI === HTML && attrs.href !== undefined)) {
      if (!node.sourceCodeLocation?.startTag) throw new TypeError('parseHTML must provide sourceCodeLocationInfo with the default tree adapter');
      entries.push({ node, attrs, inert, shadow });
    }
    for (const child of node.childNodes || []) stack.push({ node: child, inert, shadow });
    if (node.tagName === 'template' && node.namespaceURI === HTML && node.content) {
      stack.push({ node: node.content, inert: true, shadow: shadow || attrs.shadowrootmode !== undefined });
    }
  }
  return entries.sort((a, b) => a.node.sourceCodeLocation.startOffset - b.node.sourceCodeLocation.startOffset);
}

function executionMode(record, attrs, closed, textLength) {
  if (record.context === 'declarative-shadow-template') return 'unknown';
  if (record.context === 'template') return 'inert';
  if (record.kind === 'foreign') return 'unknown';
  if (record.kind === 'data') return 'data';
  if (!closed) return 'unknown';
  if (record.mode === 'inline' && textLength === 0) return 'empty';
  if (record.src?.status === 'invalid') return 'invalid-source';
  if (record.kind === 'importmap' || record.kind === 'speculationrules') return 'browser-data';
  if (record.kind === 'module') return record.async ? 'async-module' : 'module';
  if (record.nomodule) return 'conditional-classic';
  if (attrs.for !== undefined || attrs.event !== undefined) return 'unknown';
  if (record.mode === 'external') {
    if (record.async) return 'async-classic';
    if (record.defer) return 'defer-classic';
  }
  return 'eager-classic'; // async/defer have no scheduling effect on inline classic scripts.
}

function makeRecord(entry, config, index, basePresent) {
  const { node, attrs, inert, shadow } = entry, loc = node.sourceCodeLocation;
  const kind = scriptKind(attrs, node.namespaceURI);
  const content = {
    startLine: loc.startTag.endLine, startCol: loc.startTag.endCol, startOffset: loc.startTag.endOffset,
    endLine: loc.endTag?.startLine ?? loc.endLine, endCol: loc.endTag?.startCol ?? loc.endCol,
    endOffset: loc.endTag?.startOffset ?? loc.endOffset,
  };
  const record = {
    id: `${config.htmlFile}#script@${loc.startOffset}`, index, htmlFile: config.htmlFile,
    anchor: anchor(loc), startTag: anchor(loc.startTag), content,
    namespace: node.namespaceURI, kind,
    context: shadow ? 'declarative-shadow-template' : inert ? 'template' : 'document',
    mode: attrs.src === undefined ? 'inline' : 'external',
    type: attrs.type ?? null, language: attrs.language ?? null,
    async: attrs.async !== undefined, defer: attrs.defer !== undefined, nomodule: attrs.nomodule !== undefined,
    src: null,
  };
  if (attrs.src !== undefined) {
    const resolution = kind === 'foreign' ? { status: 'unknown', reason: 'foreign-script-source' }
      : !['classic', 'module'].includes(kind) ? { status: 'not-fetched', reason: 'non-javascript-script-type' }
      : inert ? { status: 'not-fetched', reason: shadow ? 'shadow-template-not-modeled' : 'template-content' }
      : resolveSource(attrs.src, config, basePresent);
    record.src = { value: attrs.src, anchor: anchor(loc.attrs?.src), ...resolution };
  }
  if (kind === 'foreign') record.foreignReferences = (node.attrs || [])
    .filter((a) => a.name === 'href').map((a) => ({ name: a.prefix ? `${a.prefix}:${a.name}` : a.name, value: a.value }));
  const textLength = (node.childNodes || []).filter((n) => n.nodeName === '#text').reduce((n, child) => n + child.value.length, 0);
  record.execution = executionMode(record, attrs, Boolean(loc.endTag), textLength);
  return record;
}

function recordFindings(record, entry, findings, diagnostics) {
  const add = (code, message) => findings.push({ code, scriptId: record.id, anchor: record.startTag, message });
  if (record.kind === 'foreign') add('unsupported-foreign-script', 'Foreign namespace script execution and href loading are not modeled.');
  if (entry.shadow) add('unsupported-shadow-template', 'Declarative shadow template activation is not modeled.');
  if (!entry.node.sourceCodeLocation.endTag) add('unsupported-unclosed-script', 'No end tag; do not infer execution from an incomplete script element.');
  if (entry.attrs.for !== undefined || entry.attrs.event !== undefined) add('unsupported-legacy-event-script', 'Legacy for/event preparation rules are not modeled.');
  if (record.nomodule) add('conditional-nomodule', 'No browser module-support assumption is made.');
  if (['module', 'async-module', 'async-classic', 'defer-classic'].includes(record.execution)) {
    add('readiness-not-inferred', 'This script is outside the eager classic subsequence; no evaluation or readiness edge is emitted.');
  }
  if (['importmap', 'speculationrules'].includes(record.kind)) add('unsupported-browser-data', 'Browser data processing and its effects are not modeled.');
  if (record.src && ['unknown', 'unresolved', 'invalid'].includes(record.src.status)) {
    diagnostics.push({ code: `${record.src.status}-script-src`, scriptId: record.id, anchor: record.src.anchor, reason: record.src.reason });
  }
}

/**
 * inventoryHtmlScripts(html, { parseHTML, root, htmlFile, sourceRoot? })
 * parseHTML: synchronous parse5.parse (default adapter). Caller owns async import.
 * root: absolute repository root. htmlFile: normalized repository-relative path.
 * sourceRoot: optional repository-relative directory served at URL '/', e.g. 'src'.
 * Result paths are repository-relative physical identities; no extension guessing.
 * IDs and edges identify tag occurrences, so repeated loads are never deduplicated.
 * Edges are adjacent source-order constraints on eligible classic evaluation IF it
 * occurs under unmodified parser insertion, not evidence of successful execution.
 * See https://html.spec.whatwg.org/multipage/scripting.html#prepare-the-script-element
 */
function inventoryHtmlScripts(html, options = {}) {
  if (typeof options.parseHTML !== 'function') throw new TypeError('parseHTML is required: inject synchronous parse5.parse');
  if (typeof html !== 'string') throw new TypeError('html must be a string');
  const config = configuration(options), diagnostics = [], findings = [];
  const document = options.parseHTML(html, {
    sourceCodeLocationInfo: true, scriptingEnabled: true,
    onParseError: (error) => diagnostics.push({ code: 'html-parse-error', parserCode: error.code, anchor: anchor(error) }),
  });
  if (document?.then) {
    Promise.resolve(document).catch(() => {});
    throw new TypeError('parseHTML must be synchronous; await import(\'parse5\') in the caller');
  }
  if (document?.nodeName !== '#document' || !Array.isArray(document.childNodes)) throw new TypeError('parseHTML must return a default-adapter HTML document');
  const scripts = [];
  let basePresent = false;
  for (const entry of collectNodes(document)) {
    if (entry.node.tagName === 'base') {
      if (!entry.inert && !basePresent) {
        basePresent = true;
        findings.push({ code: 'unsupported-base-href', anchor: anchor(entry.node.sourceCodeLocation), message: 'Following relative/root src resolution is unknown because base href is not modeled.' });
      }
      continue;
    }
    const record = makeRecord(entry, config, scripts.length, basePresent);
    scripts.push(record);
    recordFindings(record, entry, findings, diagnostics);
  }
  const eagerOrder = scripts.filter((s) => s.execution === 'eager-classic').map((s) => s.id);
  const edges = eagerOrder.slice(1).map((to, i) => ({ from: eagerOrder[i], to, relation: 'classic-evaluation-order' }));
  return {
    htmlFile: config.htmlFile, scripts, eagerOrder, edges, diagnostics, findings,
    limits: {
      parsing: 'Static text/html document with scripting enabled; no XML or dynamic DOM parsing.',
      ordering: 'Conditional evaluation order only: no successful load, global availability, callback, tool-handler, module, async or defer readiness inference.',
      runtime: 'JavaScript/document.write, DOM/base mutations, CSP, MIME responses, integrity, fetch failures and browser feature support are not evaluated.',
      mapping: 'Explicit local filesystem mapping only; no server rewrites, custom protocols, import maps or network fetches.',
    },
  };
}

module.exports = { inventoryHtmlScripts };
