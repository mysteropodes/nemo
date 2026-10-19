/* Shared stateless layer-bar hit tests; browser and native rows use the same DOM geometry. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoLayerInOutSelection=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var HANDLE_TOLERANCE_PX = 6;
  function partForBoxHit(bar, barRect, x0, x1) {
    var hl = bar.querySelector('.layer-inout-handle.left');
    var hr = bar.querySelector('.layer-inout-handle.right');
    if (!hl || !hr) return partForOverlap(barRect, Math.max(x0, barRect.left), Math.min(x1, barRect.right));
    var rl = hl.getBoundingClientRect(), rr = hr.getBoundingClientRect();
    var touches = function (r) { return (r.left - HANDLE_TOLERANCE_PX) <= x1 && (r.right + HANDLE_TOLERANCE_PX) >= x0; };
    var inHit = touches(rl), outHit = touches(rr);
    if (inHit && outHit) return 'both';
    if (inHit) return 'in';
    if (outHit) return 'out';
    // Aucune poignée dans le rectangle : on ne sélectionne AUCUN point.
    // Cyril : « ce qu'il faudrait c'est que les in/out point qui sont
    // SEULEMENT dans le rec de sélection soient sélectionnés ». Un premier
    // jet renvoyait 'both' ici — donc un rectangle tracé au milieu d'une
    // barre sélectionnait ses DEUX bords, exactement ce qu'il ne veut pas.
    // La barre n'est pas perdue pour autant : le lasso de la grille verse
    // alors ce calque dans la sélection de CALQUES (motion.js, endMarquee),
    // ce qui est la lecture honnête du geste — « j'ai encadré ces lignes,
    // sans viser de bord ».
    return null;
  }
  function partForOverlap(barRect, ox0, ox1) {
    var mid = (barRect.left + barRect.right) / 2;
    var touchesLeft = ox0 <= mid, touchesRight = ox1 >= mid;
    if (touchesLeft && !touchesRight) return 'in';
    if (touchesRight && !touchesLeft) return 'out';
    return 'both';
  }
  return Object.freeze({partForBoxHit:partForBoxHit,partForOverlap:partForOverlap});
}));
