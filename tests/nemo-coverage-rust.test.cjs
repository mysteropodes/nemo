'use strict';
// Include the T04 (#1053) Rust-coverage regressions in the established
// npm test / verify command.
//
// Every other scripts/nemo/*.test.cjs is pulled in by a sibling of this file;
// this one was not, so its 24 tests have never run in local validation since
// T04 landed. Found by auditing which test surfaces any job or glob actually
// reaches, after #1318 showed the nemo-mcp crate had the same shape.
require('../scripts/nemo/coverage-rust.test.cjs');
