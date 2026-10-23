/* L10-J composition seam: the one contract object the common v2 SDK injects. It is the font contract composed with the native
 * import contract (same interface), so native-application.js keeps a single injected seam and the font operations behave exactly
 * as before. Reads the two contracts only; owns nothing. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const font = commonjs ? require('./native-font-contract.js') : root.NemoNativeFontContract;
  const imports = commonjs ? require('./native-import-contract.js') : root.NemoNativeImportContract;
  const api = factory(font, imports);
  if (commonjs) module.exports = api;
  else root.NemoNativeImportSeam = api;
}(typeof globalThis === 'object' ? globalThis : this, function (font, imports) {
  'use strict';
  if (!imports || typeof imports.compose !== 'function') throw new TypeError('native import contract is unavailable');
  return imports.compose(font);
}));
