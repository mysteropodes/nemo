'use strict';
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

// Load the complete shipped facade. Only startup DOM registration is stubbed;
// curve/track/key writers are the production functions, not extracted source.
module.exports = function loadMotion() {
  const context = {
    document: { readyState: 'loading', addEventListener() {} },
    localStorage: { getItem() { return null; } },
  };
  context.window = context;
  vm.createContext(context);
  const root = path.resolve(__dirname, '../..');
  for (const file of ['src/js/animation/curve.js', 'src/js/domain/animation/opacity.js', 'src/js/motion.js']) {
    const absolute = path.join(root, file);
    vm.runInContext(fs.readFileSync(absolute, 'utf8'), context, { filename: absolute });
  }
  return context.SMMotion;
};
