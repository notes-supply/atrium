// Every test fails on a console error, an uncaught exception or a CSP violation.
import { test as base, expect } from '@playwright/test';

export { expect };

// Headless Chromium sometimes aborts the stylesheet's cross-document view transition on a
// link navigation, about one in five, with scripts off as well, so no module causes it.
const ABORTED_TRANSITION = 'InvalidStateError: Transition was aborted because of invalid state. ViewTransition opt-in disabled';

export const test = base.extend({
  page: async ({ page }, use) => {
    const errors = [];
    page.on('console', (message) => { if (message.type() === 'error') errors.push(message.text()); });
    page.on('pageerror', (error) => { if (String(error) !== ABORTED_TRANSITION) errors.push(String(error)); });
    await use(page);
    expect(errors, 'console errors').toEqual([]);
  },
});

// The ui bundle mounts every module in one synchronous pass, and the clock, on every
// page, rewrites its datetime as a full ISO time as it mounts. The server writes it to the
// minute, so once it ends in Z every module is in place.
export async function open(page, path) {
  await page.goto(path);
  await expect(page.locator('time.clock[datetime$="Z"]')).toHaveCount(1);
}

export const cookie = async (page, name) => (await page.context().cookies()).find((c) => c.name === name)?.value;
