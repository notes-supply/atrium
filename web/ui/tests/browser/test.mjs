// Every test fails on a console error, an uncaught exception or a CSP violation, and
// addresses pages by the names in tools/check/routes.mjs, as `atrium --demo` serves them.
import { test as base, expect } from '@playwright/test';
import { ROUTES } from '../../../../tools/check/routes.mjs';

export { expect, ROUTES };

// Headless Chromium sometimes aborts the stylesheet's cross-document view transition on a
// link navigation, about one in five, with scripts off as well, so no module causes it.
const ABORTED_TRANSITION = 'InvalidStateError: Transition was aborted because of invalid state. ViewTransition opt-in disabled';

// Requests a route or a test means to fail, which Chromium still reports in the console:
// the away wiki's page list, and the not-found page itself.
export const FAILING = { away: /\/wiki\/index\.json$/, 404: /\.md(\?|$)/ };

/** One pattern for every request in `patterns` that is meant to fail. */
export const either = (...patterns) => new RegExp(patterns.filter(Boolean).map((p) => p.source).join('|') || '$^');

export const test = base.extend({
  failing: [/$^/, { option: true }],
  page: async ({ page, failing }, use) => {
    const errors = [];
    const meant = (message) => message.text().startsWith('Failed to load resource')
      && failing.test(message.location().url);
    page.on('console', (message) => { if (message.type() === 'error' && !meant(message)) errors.push(message.text()); });
    page.on('pageerror', (error) => { if (String(error) !== ABORTED_TRANSITION) errors.push(String(error)); });
    await use(page);
    expect(errors, 'console errors').toEqual([]);
  },
});

/** The address of the route `name`, with `query` added to its own. */
export function url(name, query = '') {
  const route = ROUTES.find((r) => r.name === name);
  const base = route.away ? process.env.ATRIUM_AWAY_URL : process.env.ATRIUM_URL;
  return base + route.path + (query ? (route.path.includes('?') ? '&' : '?') + query : '');
}

// The demo's hour, 09:41 in Stockholm, which every page it serves is written in. Asked for
// with ?hour, the light module keeps it, so the light does not move under a test.
export const HELD = 'hour=9';

/** The route's path as the address bar shows it, for `toHaveURL`, held or not. */
export const at = (name) => new RegExp(`${ROUTES.find((r) => r.name === name).path.replace(/[?.]/g, '\\$&')}([?&]${HELD})?$`);

// The ui bundle mounts every module in one synchronous pass, and the clock, on every
// page, rewrites its datetime as a full ISO time as it mounts. The server writes it to the
// minute, so once it ends in Z every module is in place. The page is held at the demo's
// hour unless the query names an hour or the test is about the light (`held: false`).
export async function open(page, name, query = '', { held = true } = {}) {
  const hold = held && !/(^|&)hour=/.test(query) ? HELD : '';
  await page.goto(url(name, [query, hold].filter(Boolean).join('&')));
  await expect(page.locator('time.clock[datetime$="Z"]')).toHaveCount(1);
}

export const cookie = async (page, name) => (await page.context().cookies()).find((c) => c.name === name)?.value;
