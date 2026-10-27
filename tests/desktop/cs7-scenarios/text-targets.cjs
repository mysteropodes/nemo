'use strict';
// Clause4 of #2264 is the ordinary Text20 query/list, edit, font and reopen exit.
// The later #2267 row consumes the same qualified execution in this session.
const original = require('./text20.cjs');
const { targetWorkflow } = require('./text20-shared.cjs');
function prepare(context) { return targetWorkflow(original.prepare(context)); }
module.exports = { prepare };
