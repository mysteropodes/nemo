/* L02-J composition seam: the one contract object the common v2 SDK injects. It is the font contract composed with the
 * native object edit contract (same interface), so native-application.js keeps a single injected seam and the font
 * operations behave exactly as before. Reads the two contracts only; owns nothing. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const font = commonjs ? require('./native-font-contract.js') : root.NemoNativeFontContract;
  const edit = commonjs ? require('./native-object-edit-contract.js') : root.NemoNativeObjectEditContract;
  const api = factory(font, edit);
  if (commonjs) module.exports = api;
  else root.NemoNativeObjectEditSeam = api;
}(typeof globalThis === 'object' ? globalThis : this, function (font, edit) {
  'use strict';
  if (!edit || typeof edit.compose !== 'function') throw new TypeError('native object edit contract is unavailable');
  return edit.compose(font);
}));
