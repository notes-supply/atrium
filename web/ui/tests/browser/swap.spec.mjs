import { test, expect, open } from './test.mjs';

// Records each query the module sends to the server's search page, and holds back the
// answers `delay` names before they reach the server.
async function watch(page, { delay = () => 0 } = {}) {
  const asked = [];
  await page.route(/\/wiki\/search\?/, async (route) => {
    const url = new URL(route.request().url());
    if (route.request().resourceType() === 'fetch') asked.push(url.search);
    await new Promise((resolve) => setTimeout(resolve, delay(url.searchParams.get('q') ?? '')));
    await route.continue();
  });
  return asked;
}

test('swap: results follow the query as it is typed, and the address keeps it', async ({ page }) => {
  const asked = await watch(page);
  await open(page, 'search');
  const length = await page.evaluate(() => history.length);
  await page.locator('#q').fill('corne');
  await expect(page.locator('[data-swap-target] h3 a').first()).toHaveText('Keyboard build: Corne v4');
  await expect(page.locator('[data-count]')).toContainText('corne');
  await expect(page).toHaveURL(/\/wiki\/search\?q=corne&sort=relevance$/);
  expect(asked).toEqual(['?q=corne&sort=relevance']);
  expect(await page.evaluate(() => history.length)).toBe(length);
});

test('swap: a filter is counted in the summary and sent with the query', async ({ page }) => {
  const asked = await watch(page);
  await open(page, 'search');
  await page.locator('.filters summary').click();
  await page.locator('#changed').selectOption('7');
  await expect(page.locator('.filters summary b')).toHaveText('1 set');
  await expect(page).toHaveURL(/changed=7/);
  expect(asked.at(-1)).toBe('?q=restic&changed=7&sort=relevance');
});

test('swap: Enter swaps the results and focuses the first one', async ({ page }) => {
  await watch(page);
  await open(page, 'search');
  await page.locator('#q').fill('tailnet');
  await page.locator('#q').press('Enter');
  await expect(page.locator('[data-count]')).toContainText('tailnet');
  await expect(page.locator('[data-swap-target] a').first()).toBeFocused();
  await expect(page).toHaveURL(/\/wiki\/search\?q=tailnet/);
});

test('swap: a slow answer to an older query never replaces a newer one', async ({ page }) => {
  const asked = await watch(page, { delay: (q) => (q === 'apollo' ? 800 : 0) });
  await open(page, 'search');
  await page.locator('#q').fill('apollo');
  await expect.poll(() => asked, 'the older query went out').toEqual(['?q=apollo&sort=relevance']);
  await page.locator('#q').fill('corne');
  await expect(page.locator('[data-count]')).toContainText('corne');
  await expect.poll(() => page.evaluate(() => performance.getEntriesByType('resource').some((e) => e.name.includes('q=apollo'))), 'the older answer arrived').toBe(true);
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve)));
  await expect(page.locator('[data-count]')).toContainText('corne');
  await expect(page).toHaveURL(/q=corne&/);
});
