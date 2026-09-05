'use strict';
// Retained fixture: half of a deliberate two-file `cycle`.
require('./cycle-b.cjs');
module.exports = { name: 'cycle-a' };
