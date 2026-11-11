// Every test fails on a console error, an uncaught exception or a CSP violation.
import { test as base, expect } from '@playwright/test';

export { expect };

export const test = base.extend({
  page: async ({ page }, use) => {
    const errors = [];
    page.on('console', (message) => { if (message.type() === 'error') errors.push(message.text()); });
    page.on('pageerror', (error) => errors.push(String(error)));
    await use(page);
    expect(errors, 'console errors').toEqual([]);
  },
});

// The ui bundle mounts every module in one synchronous pass, and prefs marks the current
// intensity as it mounts, so once a swatch is pressed every module is in place.
export async function open(page, path) {
  await page.goto(path);
  await expect(page.locator('.prefs-mast .swatch[aria-pressed="true"]')).toHaveCount(1);
}

export const cookie = async (page, name) => (await page.context().cookies()).find((c) => c.name === name)?.value;
