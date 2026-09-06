'use strict';

// Keep this process-isolated: the suite sets NEMO_ISOLATION_ROOT before the
// isolation module captures it at import time.
require('../scripts/nemo/native-runtime.test.cjs');
