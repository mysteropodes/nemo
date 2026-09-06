'use strict';

// Normal-discovery entry (tests/*.test.cjs) for the R03 fixture corpus
// verification, which lives next to the corpus, and for the loader contract
// of the fixture VM it runs against (production modules in src/index.html
// order, the R08 easing kernel included).
require('./fixtures/fixtures.test.cjs');
require('./fixtures/lib/sandbox.test.cjs');
