'use strict';

// Exercise the existing real-process integration suite through the named
// integration gate. It starts two HTTP preview launchers, challenges their
// source/build identities, checks isolated roots/origins, and verifies owned
// shutdown instead of replacing those contracts with a second fake harness.
require('../../scripts/nemo/browser-runtime.test.cjs');
