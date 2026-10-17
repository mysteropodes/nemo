// ---- LABS CORE — registry for feature-scouting prototypes ----
// (docs/feature-scouting.md). Each prototype lives in its own file under
// js/labs/, registers itself here, and is gated by its own localStorage
// flag (all off by default). The ONLY touch to core files is the single
// guarded `window.SMLabs.onStrokeCommitted` hook in draw-bridge.js — this
// file fans that one hook out to every registered prototype, so adding a
// new prototype never means touching draw-bridge.js again.
//
// Console API (no Réglages UI on purpose — these are prototypes to try,
// not shipped features; a real toggle comes only if one is picked):
//   SMLabs.list()            — every prototype + on/off state
//   SMLabs.enable('name')    — turn one on (persisted)
//   SMLabs.disable('name')   — turn one off
//   SMLabs.toggle('name')
(function () {
  var model=window.NemoLabsRegistryModel.create({
    getItem:function(key){return localStorage.getItem(key);},
    setItem:function(key,value){return localStorage.setItem(key,value);},
    warn:function(){return console.warn.apply(console,arguments);},
    toast:function(name,on){if(typeof showToast==='function')showToast('Labs — '+name+' : '+(on?SM.t('labsToastEnabledWord'):SM.t('labsToastDisabledWord')));},
    describe:function(key){return (window.SM&&SM.t&&key)?SM.t(key):key;}
  });
  window.SMLabs=window.SMLabs||{};
  var L=window.SMLabs;
  L.register=model.register;
  L.isOn=model.isOn;
  L.enable=model.enable;
  L.disable=model.disable;
  L.toggle=model.toggle;
  L.list=model.list;
  L.resetAll=model.resetAll;
  L.onStrokeCommitted=model.onStrokeCommitted;
  L.buildDrawPreviewExtras=model.buildDrawPreviewExtras;
})();
