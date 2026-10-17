/* L03-T composition seam: the font contract composed with the lane-3 object contract (same six-method interface), so
 * the common v2 SDK keeps ONE injected contract. `compose` is re-exported so the integrator can chain the seams of
 * several lanes (lane3.compose(otherSeam)) without a second SDK seam. Reads the two contracts only; owns nothing. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const font = commonjs ? require('./native-font-contract.js') : root.NemoNativeFontContract;
  const rig = commonjs ? require('./native-object-rig-contract.js') : root.NemoNativeObjectRigContract;
  const api = factory(font, rig);
  if (commonjs) module.exports = api;
  else root.NemoNativeObjectRigSeam = api;
}(typeof globalThis === 'object' ? globalThis : this, function (font, rig) {
  'use strict';
  if (!rig || typeof rig.compose !== 'function') throw new TypeError('native object rig contract is unavailable');
  return Object.freeze(Object.assign({}, rig.compose(font), { compose: rig.compose }));
}));
