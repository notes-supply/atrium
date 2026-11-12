import { test, expect, open, at } from './test.mjs';

// Wednesday 11 November 2026, in the container's time zone.
const clockAt = (hh, mm) => new Date(2026, 10, 11, hh, mm, 0);

test('diff: the only-changed link toggles in place, by click and by Space', async ({ page }) => {
  await open(page, 'diff');
  const section = page.locator('[data-module~=diff]');
  const only = section.locator('[data-only-changes]');
  await expect(only).toHaveAttribute('role', 'button');
  await expect(only).toHaveAttribute('aria-pressed', 'false');
  await only.click();
  await expect(only).toHaveAttribute('aria-pressed', 'true');
  await expect(section).toHaveClass(/\bonly-changes\b/);
  await expect(page).toHaveURL(at('diff'));
  await only.focus();
  await page.keyboard.press(' ');
  await expect(only).toHaveAttribute('aria-pressed', 'false');
  await expect(section).not.toHaveClass(/\bonly-changes\b/);
});

test('diff: ?only=changed starts with only the changes', async ({ page }) => {
  await open(page, 'diff', 'only=changed');
  await expect(page.locator('[data-only-changes]')).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('[data-module~=diff]')).toHaveClass(/\bonly-changes\b/);
});

test('toc: the heading in view is the current entry', async ({ page }) => {
  await open(page, 'page');
  const toc = page.locator('details.toc');
  await expect(toc).toHaveAttribute('open', '');
  await page.locator('#restore-drill-november').evaluate((h) => h.scrollIntoView({ block: 'start' }));
  await expect(toc.locator('a[href="#restore-drill-november"]')).toHaveAttribute('aria-current', 'true');
  await expect(toc.locator('a[aria-current=true]')).toHaveCount(1);
  await page.locator('#summary').evaluate((h) => h.scrollIntoView({ block: 'start' }));
  await expect(toc.locator('a[href="#summary"]')).toHaveAttribute('aria-current', 'true');
});

test('toc: closed on a phone', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await open(page, 'page');
  await expect(page.locator('details.toc')).not.toHaveAttribute('open');
});

test('carry: only the followed title carries its name, and not onto a page with a title', async ({ page }) => {
  await open(page, 'wiki');
  await page.evaluate(() => addEventListener('click', (e) => e.preventDefault()));
  const links = page.locator('main a[data-carry]');
  await links.nth(0).click();
  await expect(links.nth(0)).toHaveClass(/\bcarry\b/);
  await links.nth(1).click();
  await expect(page.locator('a.carry')).toHaveCount(1);
  await expect(links.nth(1)).toHaveClass(/\bcarry\b/);
  await page.evaluate(() => dispatchEvent(new PageTransitionEvent('pageshow')));
  await expect(page.locator('a.carry')).toHaveCount(0);
  await open(page, 'page');
  await page.evaluate(() => {
    addEventListener('click', (e) => e.preventDefault());
    document.querySelector('.rail-r').insertAdjacentHTML('beforeend', '<a href="#" data-carry>Another page</a>');
  });
  await page.locator('a[data-carry]').first().click();
  await expect(page.locator('a.carry')).toHaveCount(0);
});

test('clock: the masthead shows the visitor\'s time and turns with the minute', async ({ page }) => {
  await page.clock.install({ time: clockAt(14, 7) });
  await open(page, 'wiki');
  const clock = page.locator('time.clock');
  await expect(clock).toHaveText('14:07');
  await page.clock.fastForward(60_000);
  await expect(clock).toHaveText('14:08');
  const shown = new Date(await clock.getAttribute('datetime'));
  expect([shown.getHours(), shown.getMinutes()]).toEqual([14, 8]);
});

test('greet: the greeting follows the visitor\'s clock', async ({ page }) => {
  await page.clock.install({ time: clockAt(3, 30) });
  await open(page, 'index');
  await expect(page.locator('.sentence [data-module=greet]')).toHaveText('Still up.');
  await page.clock.install({ time: clockAt(19, 0) });
  await open(page, 'index');
  await expect(page.locator('.sentence [data-module=greet]')).toHaveText('Good evening.');
});

test('light: the hour on <html> follows the visitor\'s clock, and turns while the page is open', async ({ page }) => {
  await page.clock.install({ time: clockAt(14, 59) });
  await open(page, 'wiki', '', { held: false });
  await expect(page.locator('html')).toHaveAttribute('data-hour', '14');
  await page.evaluate(() => { window.hours = 0; document.addEventListener('atrium:hour', () => window.hours++); });
  await page.clock.fastForward(61_000);
  await expect(page.locator('html')).toHaveAttribute('data-hour', '15');
  expect(await page.evaluate(() => window.hours)).toBe(1);
});

test('light: a page asked for one hour keeps it', async ({ page }) => {
  await page.clock.install({ time: clockAt(14, 0) });
  await open(page, 'wiki', 'hour=9');
  await expect(page.locator('html')).toHaveAttribute('data-hour', '9');
});
