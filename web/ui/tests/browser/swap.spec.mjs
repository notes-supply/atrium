import { test, expect, open } from './test.mjs';

// Stands in for the server's search page: only the parts the module swaps in.
async function server(page, { delay = () => 0 } = {}) {
  const asked = [];
  await page.route(/\/search\.html\?/, async (route) => {
    const url = new URL(route.request().url());
    asked.push(url.search);
    const q = url.searchParams.get('q') ?? '';
    await new Promise((resolve) => setTimeout(resolve, delay(q)));
    await route.fulfill({
      contentType: 'text/html; charset=utf-8',
      body: `<!doctype html><p data-count><b>1</b> page matches <q>${q}</q></p>`
        + `<section data-swap-target><ol class="result-list"><li><h3><a href="page.html" data-row>Result for ${q}</a></h3></li></ol></section>`,
    });
  });
  return asked;
}

test('swap: results follow the query as it is typed, and the address keeps it', async ({ page }) => {
  const asked = await server(page);
  await open(page, 'search.html');
  const length = await page.evaluate(() => history.length);
  await page.locator('#q').fill('corne');
  await expect(page.locator('[data-swap-target] a')).toHaveText(['Result for corne']);
  await expect(page.locator('[data-count]')).toHaveText('1 page matches corne');
  await expect(page).toHaveURL(/search\.html\?q=corne&sort=relevance$/);
  expect(asked).toEqual(['?q=corne&sort=relevance']);
  expect(await page.evaluate(() => history.length)).toBe(length);
});

test('swap: a filter is counted in the summary and sent with the query', async ({ page }) => {
  const asked = await server(page);
  await open(page, 'search.html');
  await page.locator('.filters summary').click();
  await page.locator('#changed').selectOption('7');
  await expect(page.locator('.filters summary b')).toHaveText('1 set');
  await expect(page).toHaveURL(/changed=7/);
  expect(asked.at(-1)).toBe('?q=restic&changed=7&sort=relevance');
});

test('swap: Enter swaps the results and focuses the first one', async ({ page }) => {
  await server(page);
  await open(page, 'search.html');
  await page.locator('#q').fill('tailnet');
  await page.locator('#q').press('Enter');
  await expect(page.locator('[data-swap-target] a')).toHaveText(['Result for tailnet']);
  await expect(page.locator('[data-swap-target] a').first()).toBeFocused();
  await expect(page).toHaveURL(/search\.html\?q=tailnet/);
});

test('swap: a slow answer to an older query never replaces a newer one', async ({ page }) => {
  await server(page, { delay: (q) => (q === 'a' ? 800 : 0) });
  await open(page, 'search.html');
  await page.locator('#q').fill('a');
  await page.waitForTimeout(200);
  await page.locator('#q').fill('ab');
  await expect(page.locator('[data-swap-target] a')).toHaveText(['Result for ab']);
  await page.waitForTimeout(1000);
  await expect(page.locator('[data-swap-target] a')).toHaveText(['Result for ab']);
  await expect(page).toHaveURL(/q=ab&/);
});
