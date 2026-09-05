'use strict';
// Retained fixture: reaches fx.privTarget's file outside its publicApi.
require('./priv-secret.cjs');
module.exports = { name: 'priv-importer' };
