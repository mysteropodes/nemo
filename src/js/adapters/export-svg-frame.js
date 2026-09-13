// @ts-check
// Single-frame SVG export adapter (P17): the document wrapper around Paper's
// `Layer.exportSVG()` that `exportFrameSVGString` in export.js used to inline.
//
// This module owns no state and reads no globals. Everything the wrapper needs
// comes in through explicit ports so the same code is exercisable headlessly:
//
//   ports.buildFrame(frameIdx) — the EXISTING export frame builder
//                                (export.js exportBuildFrame). It returns the
//                                hidden export Paper layer; it is not
//                                re-implemented or extended here.
//   ports.dimensions()         — { width, height } of the document canvas at
//                                call time (export.js reads state.canvasW/H).
//   ports.Rectangle            — the Paper `Rectangle` constructor, for the
//                                exportSVG bounds.
//
// The visibility toggle is part of the contract, not an incidental detail:
// exportSVG() skips invisible items, so the hidden export layer is flipped
// visible for the call itself and back off synchronously before anything can
// repaint (the same rule exportFrameDataURL follows for rasterize()). The
// flip happens in exactly the order the inline code had — no try/finally is
// added, so a throwing exportSVG leaves the layer exactly as before the move.
var NemoExportSvgFrame = (function () {
  'use strict';

  var XML_PREAMBLE = '<?xml version="1.0" encoding="UTF-8"?>\n';

  // The outer document: preamble, root <svg> sized to the canvas with a
  // matching viewBox, and the inner markup on its own line.
  function svgDocument(inner, width, height) {
    return XML_PREAMBLE +
      '<svg xmlns="http://www.w3.org/2000/svg" width="' + width + '" height="' + height + '" ' +
      'viewBox="0 0 ' + width + ' ' + height + '">\n' + inner + '\n</svg>';
  }

  function exportFrameSVGString(frameIdx, ports) {
    var L = ports.buildFrame(frameIdx);
    var dims = ports.dimensions();
    L.visible = true;
    var inner = L.exportSVG({ asString: true, bounds: new ports.Rectangle(0, 0, dims.width, dims.height) });
    L.visible = false;
    return svgDocument(inner, dims.width, dims.height);
  }

  return {
    XML_PREAMBLE: XML_PREAMBLE,
    svgDocument: svgDocument,
    exportFrameSVGString: exportFrameSVGString,
  };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoExportSvgFrame;
