'use strict';
// Retained fixture: half of a deliberate two-file `cycle`.
require('./cycle-a.cjs');
module.exports = { name: 'cycle-b' };
