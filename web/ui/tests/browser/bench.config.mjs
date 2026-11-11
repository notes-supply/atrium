// The field's frame time, apart from the tests: one worker, no retries, a long timeout.
// CPUSET=2,3 ATRIUM_PROTOTYPE=… mise run test:browser -- -c bench.config.mjs
import base from './playwright.config.mjs';

export default { ...base, testMatch: '*.bench.mjs', workers: 1, timeout: 30 * 60_000 };
