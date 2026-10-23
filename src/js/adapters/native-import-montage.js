/* L10-J montage / storyboard / shot intents of the native importers: placeMontageAsLayer, enter / exit montage view, split a shot
 * chain member, add an instance module from a layer. The accepted L10-A/L10-I kernels define the montage timing, the place plan and the
 * view workspace as data, but NO native operation commits them yet (the place commit needs lane 4's component table, the view needs
 * lane 4's workspace). Until a session declares an operation, every intent is a typed unavailable naming the missing provider; none
 * falls back to the legacy storyboard.js writer, and none invents a native method name. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeImportMontage = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  // intent -> the provider records the commit waits for (task ids and leaf names from the lane-10 contract).
  const INTENTS = Object.freeze({
    placeMontage: Object.freeze({ baseline: 'placeMontageAsLayer', tasks: Object.freeze(['I19']), waitsFor: Object.freeze(['L04-I component table', 'L10-I place commit']) }),
    enterMontageView: Object.freeze({ baseline: 'enterMontageView', tasks: Object.freeze(['I20']), waitsFor: Object.freeze(['L04-H workspace', 'L04-J context resolver']) }),
    exitMontageView: Object.freeze({ baseline: 'exitMontageView', tasks: Object.freeze(['I20']), waitsFor: Object.freeze(['L04-H workspace', 'L04-J context resolver']) }),
    splitShot: Object.freeze({ baseline: 'splitChainMember', tasks: Object.freeze(['I42']), waitsFor: Object.freeze(['L10-B history harness', 'L05 frame insertion']) }),
    addInstanceModule: Object.freeze({ baseline: 'addInstanceModule', tasks: Object.freeze(['I17', 'I18']), waitsFor: Object.freeze(['L04-I storyboard record']) }),
  });

  function create(ports) {
    const declared = (intent) => {
      try { return !!ports && typeof ports.declared === 'function' && ports.declared(intent) === true; } catch (_) { return false; }
    };
    function unavailable(intent) {
      const entry = INTENTS[intent];
      return Promise.resolve(Object.freeze({
        status: 'unavailable', intent, baseline: entry.baseline, reason: declared(intent) ? 'native_route_not_implemented' : 'native_declaration_absent',
        tasks: entry.tasks.slice(), waitsFor: entry.waitsFor.slice(),
      }));
    }
    const api = {};
    Object.keys(INTENTS).forEach((intent) => { api[intent] = () => unavailable(intent); });
    return Object.freeze(api);
  }
  return Object.freeze({ INTENTS, create });
}));
