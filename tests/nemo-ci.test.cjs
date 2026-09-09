'use strict';
// Include the CI regressions in the established npm test / verify command.
require('../scripts/nemo/ci.test.cjs');

// The sole automatic metadata exception remains covered by normal local validation.
require('../.github/scripts/collaborator-pr-policy.test.cjs');
