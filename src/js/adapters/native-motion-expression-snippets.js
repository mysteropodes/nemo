/* D18-C (#2376): the "Examples" menu of the native expression editor. Every code string is the baseline text of
 * buildExprEditorRow @1065e9d8 (src/js/motion.js), byte for byte, including the per-dimension branches: `value` is a bare [x,y]
 * array on a 2D property, so each entry is valid for the property it is invoked on. Pure data; no document access. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMotionExpressionSnippets = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const CONTROL_DIM = { number: 1, checkbox: 1, angle: 1, point: 2, color: 4 };
  const FOLLOW = ['rotation', 'opacity', 'scale', 'anchor'];
  // ctx: {dimension, property, others: [name, name] (real other layers), control: {name, type} | null}.
  // Returns [{id, labelKey, suffix, code}] in the baseline menu order; one-dimensional-only entries are dropped on 2D.
  function list(ctx) {
    const is2D = ctx.dimension === 2, pair = (one, two) => (is2D ? two : one);
    const others = Array.isArray(ctx.others) ? ctx.others : [];
    const a = others.length > 0 ? others[0] : 'Layer 1', b = others.length > 1 ? others[1] : 'Layer 2';
    const follow = FOLLOW.includes(ctx.property) ? ctx.property : 'position';
    const out = [];
    const add = (id, labelKey, code, suffix) => out.push(Object.freeze({ id, labelKey, suffix: suffix || '', code }));
    add('value', 'exprExValue', 'value');
    add('wiggle', 'exprExWiggle', pair('value + wiggle(2, 10)', '[value[0] + wiggle(2, 10)[0], value[1] + wiggle(2, 10)[1]]'));
    add('oscillate', 'exprExOscillate', pair('value + Math.sin(time * 3) * 10',
      'const w = Math.sin(time * 3) * 10;\nreturn [value[0] + w, value[1] + w];'));
    add('stepped', 'exprExStepped', pair('stepTime(4);\nreturn value + wiggle(3, 15);',
      'stepTime(4);\nconst w = wiggle(3, 15);\nreturn [value[0] + w[0], value[1] + w[1]];'));
    add('loopCycle', 'exprExLoopCycle', "loopAfter('cycle')");
    add('loopPingpong', 'exprExLoopPingpong', "loopAfter('pingpong')");
    add('loopOffset', 'exprExLoopOffset', "loopAfter('offset')");
    add('loopBefore', 'exprExLoopBefore', "loopBefore('cycle')");
    const head = 'const n = self.keys.count;\n' + 'if (!n) return value;\n' +
      'const t = (frame - self.keys.at(n).frame) / comp.fps;\n' + 'if (t <= 0) return value;\n' +
      'const b = 40 * Math.exp(-4 * t) * Math.sin(3 * 2 * Math.PI * t);\n';
    add('bounce', 'exprExBounce', head + pair('return value + b;', 'return [value[0], value[1] + b];'));
    add('remapOther', 'exprExRemapOther', pair("remap(layer('" + a + "').position[0], 0, comp.width, 0, 100)",
      "const k = remap(layer('" + a + "').position[0], 0, comp.width, 0, 1);\n" + 'return [value[0], value[1] + k * 100];'));
    add('stagger', 'exprExStagger', 'seed(self.index);\n' + pair('return value + randomFixed(-20, 20);',
      'const o = randomFixed(-20, 20);\nreturn [value[0] + o, value[1] + o];'));
    add('contentBox', 'exprExContentBox', pair('contentBox().width + 20', 'const b = contentBox();\nreturn [b.width + 20, b.height + 20];'));
    if (!is2D) add('angleTo', 'exprExAngleTo', "angleTo(layer('" + a + "').position, layer('" + b + "').position)");
    add('followLayer', 'exprExFollowLayer', "layer('" + a + "')." + follow, ' · ' + follow);
    add('firstKey', 'exprExFirstKey', 'self.keys.count ? self.keys.at(1).value : value');
    const control = ctx.control;
    if (control && typeof control.name === 'string') {
      const read = 'self.control(' + JSON.stringify(control.name) + ')';
      // A point/colour control hands back an array: inserted on its own line, never added to a scalar (feedback #134).
      add('control', 'exprExControl', (CONTROL_DIM[control.type] || 1) > 1 ? read :
        pair('value + ' + read, 'const c = ' + read + ';\nreturn [value[0] + c, value[1] + c];'), ' · ' + control.name);
    }
    return Object.freeze(out);
  }
  return Object.freeze({ list });
}));
