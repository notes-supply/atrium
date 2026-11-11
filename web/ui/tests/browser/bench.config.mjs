// The field's frame time, apart from the tests: one worker, no retries, a long timeout.
// CPUSET=2,3 PROTOTYPE=… mise run bench:field
import base from './playwright.config.mjs';

export default { ...base, testMatch: '*.bench.mjs', projects: undefined, workers: 1, timeout: 30 * 60_000 };
