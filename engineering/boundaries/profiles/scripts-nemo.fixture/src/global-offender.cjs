'use strict';
// Retained fixture: direct `window.SM*` access outside the literal
// "adapters"/"bootstrap" layer names the checker hardcodes as exempt.
function touch() {
  return window.SMProject;
}
module.exports = { touch };
