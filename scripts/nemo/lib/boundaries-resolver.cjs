'use strict';
// Local filesystem coverage only: no evaluation, network fetches or import-map guesses.
const fs = require('node:fs');
const path = require('node:path');
const { createRequire, isBuiltin } = require('node:module');
const { fileURLToPath, pathToFileURL } = require('node:url');

/** Return { path }, { external }, or { rule, message }; never hide a local resolution gap. */
function resolveImport(specifier, fromFile, kind = 'require') {
  const unsupported = (message) => ({ rule: 'unsupported-local-import', message });
  const unresolved = () => ({ rule: 'unresolved-local-import', message: `Local ${kind} target ${JSON.stringify(specifier)} could not be resolved` });
  if (!['require', 'import'].includes(kind)) return unsupported('Unknown loader kind');
  if (typeof specifier !== 'string' || !specifier || /[\\\x00-\x1f\x7f]/.test(specifier) || /^\s/.test(specifier)) {
    return unsupported('Empty, control-character, leading-whitespace or backslash specifiers require explicit runtime modeling');
  }
  if (isBuiltin(specifier)) return { external: 'builtin' };
  const relative = /^\.{1,2}(?:\/|$)/.test(specifier);
  const absoluteRequire = kind === 'require' && path.isAbsolute(specifier) && !specifier.startsWith('//');
  if (!relative && !absoluteRequire) {
    if (/^(https?:|data:)/i.test(specifier)) return { external: 'url' };
    if (/^[/#?]|^[a-z][a-z\d+.-]*:/i.test(specifier)) {
      return unsupported('Root/URL/alias specifiers need a declared runtime root, scheme or import map');
    }
    return { external: 'package' }; // Bare packages/aliases are outside this bounded graph.
  }

  try {
    let target;
    const sourcePath = path.resolve(fromFile), physicalSource = fs.realpathSync(sourcePath);
    if (kind === 'require') {
      // Node owns extension/index/package-main ordering. ? and # remain literal filename bytes.
      // resolve() reads metadata but never evaluates the imported module.
      target = createRequire(physicalSource).resolve(specifier);
    } else {
      // Nemo serves src directly in both browser and Tauri: relative ESM is URL-relative.
      // fileURLToPath decodes pathname escapes without confusing encoded ?/# with suffixes.
      if (physicalSource !== sourcePath) return unsupported('Symlinked ESM importers require an explicit runtime URL base');
      target = fileURLToPath(new URL(specifier, pathToFileURL(sourcePath)));
      if (!/\.(?:cjs|mjs|js)$/.test(target)) {
        return unsupported('Only explicit JavaScript ESM file paths are modeled; no extension/index inference or asset transforms');
      }
    }
    if (!fs.statSync(target).isFile()) return unresolved();
    return { path: fs.realpathSync(target) };
  } catch (error) {
    if (['MODULE_NOT_FOUND', 'ENOENT', 'ENOTDIR'].includes(error.code)) return unresolved();
    return unsupported(`Local ${kind} resolution failed (${error.code || error.name})`);
  }
}

/** Compatibility path-or-null lookup; checkProfile consumes resolveImport's diagnostics. */
function resolveSpecifier(specifier, fromFile, kind = 'require') {
  return resolveImport(specifier, fromFile, kind).path || null;
}

module.exports = { resolveImport, resolveSpecifier };
