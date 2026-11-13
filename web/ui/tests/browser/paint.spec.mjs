// What the first paint shows, and what the server and the visitor's clock agree on.
import { test, expect, open, url } from './test.mjs';

// The demo's clock: Wednesday 11 November 2026, 09:41 in Stockholm.
const DEMO_NOW = new Date('2026-11-11T08:41:00Z');

// What the masthead clock, the greeting and the light say, as served and once the
// modules have started.
async function told(page) {
  const served = await (await page.goto(url('index'))).text();
  const said = (html) => ({
    clock: html.match(/<time class="clock"[^>]*>([^<]*)</)[1],
    greeting: html.match(/<span data-module="greet">([^<]*)</)[1],
    hour: html.match(/<html[^>]* data-hour="(\d+)"/)[1],
  });
  await expect(page.locator('time.clock[datetime$="Z"]')).toHaveCount(1);
  return { served: said(served), shown: said(await page.content()) };
}

test.describe('in the server\'s time zone', () => {
  test.use({ timezoneId: 'Europe/Stockholm' });
  test('time: the modules change nothing the server wrote', async ({ page }) => {
    await page.clock.install({ time: DEMO_NOW });
    const { served, shown } = await told(page);
    expect(served).toEqual({ clock: '09:41', greeting: 'Good morning.', hour: '9' });
    expect(shown).toEqual(served);
  });
});

test.describe('in another time zone', () => {
  test.use({ timezoneId: 'America/New_York' });
  test('time: the visitor\'s clock wins for the clock, the greeting and the light', async ({ page }) => {
    await page.clock.install({ time: DEMO_NOW });
    const { served, shown } = await told(page);
    expect(served.clock).toBe('09:41');
    expect(shown).toEqual({ clock: '03:41', greeting: 'Still up.', hour: '3' });
  });
});

test('entrance: the whole entrance on arrival, the short one on the next page', async ({ page }) => {
  const first = await page.goto(url('index'));
  expect(await first.text()).toMatch(/<html[^>]* data-entrance="full"/);
  const next = await page.goto(url('wiki'));
  expect(await next.text()).toMatch(/<html[^>]* data-entrance="short"/);
});

// Records, for every frame until the page has settled, which animation the first surface
// runs, from before the page's own scripts.
const frames = (page) => page.addInitScript(() => {
  window.painted = [];
  const look = () => {
    const surface = document.querySelector('main .surface');
    if (surface) window.painted.push(getComputedStyle(surface).animationName);
    if (window.painted.length < 20) requestAnimationFrame(look);
  };
  requestAnimationFrame(look);
});

test('first paint: with scripts on, the entrance runs from the first frame', async ({ page }) => {
  await frames(page);
  await open(page, 'wiki');
  await expect.poll(() => page.evaluate(() => window.painted.length)).toBe(20);
  expect(new Set(await page.evaluate(() => window.painted))).toEqual(new Set(['develop, rise']));
});

test.describe('scripts off', () => {
  test.use({ javaScriptEnabled: false });
  test('first paint: with scripts off, no entrance, and the settings are a link away', async ({ page }) => {
    await page.goto(url('wiki'));
    await expect(page.getByRole('link', { name: 'Settings', exact: true })).toBeVisible();
    expect(await page.locator('main .surface').first().evaluate((s) => getComputedStyle(s).animationName)).toBe('none');
  });
});
