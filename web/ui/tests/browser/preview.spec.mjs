import { test, expect, open } from './test.mjs';

// What the server previews for `link`, from the route the article names.
const card = (page, link) => link.evaluate(async (a) => {
  const route = a.closest('article').dataset.previews;
  return (await fetch(`${route}?href=${encodeURIComponent(a.dataset.preview)}`)).json();
});

// Counts the module's requests for previews, by the link they name.
const fetches = (page) => {
  const asked = [];
  page.on('request', (request) => {
    if (request.url().includes('/wiki/preview.json')) asked.push(new URL(request.url()).searchParams.get('href'));
  });
  return asked;
};

test('preview: hovering a wiki link fetches its card and shows it, anchored to it', async ({ page }) => {
  await open(page, 'page');
  const link = page.locator('article [data-preview]').first();
  const data = await card(page, link);
  const asked = fetches(page);
  await link.hover();
  const shown = page.locator('#preview');
  await expect(shown).toBeVisible();
  await expect(shown.locator('.pv-title')).toHaveText(data.title);
  await expect(shown.locator('.pv-path')).toHaveText(data.path);
  await expect(shown.locator('.pv-meta')).toHaveText(data.meta);
  await expect(shown).toContainText(data.text);
  await expect(link).toHaveClass(/\banchor\b/);
  await expect(link).toHaveAttribute('aria-describedby', 'preview');
  const [l, c] = [await link.boundingBox(), await shown.boundingBox()];
  expect(Math.abs(c.y - (l.y + l.height)) < 40 || Math.abs(l.y - (c.y + c.height)) < 40, 'beside the link').toBe(true);
  await page.mouse.move(5, 5);
  await expect(shown).toBeHidden();
  await expect(link).not.toHaveAttribute('aria-describedby');
  expect(asked).toEqual([await link.getAttribute('data-preview')]);
});

test('preview: a card is fetched once for the page\'s life', async ({ page }) => {
  await open(page, 'page');
  const asked = fetches(page);
  const link = page.locator('article [data-preview]').first();
  for (let i = 0; i < 3; i++) {
    await link.hover();
    await expect(page.locator('#preview')).toBeVisible();
    await page.mouse.move(5, 5);
    await expect(page.locator('#preview')).toBeHidden();
  }
  expect(asked).toHaveLength(1);
});

test('preview: focus shows the card, Escape and blur hide it', async ({ page }) => {
  await open(page, 'page');
  const links = page.locator('article [data-preview]');
  await links.first().focus();
  const shown = page.locator('#preview');
  await expect(shown).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(shown).toBeHidden();
  await links.nth(1).focus();
  await expect(shown).toBeVisible();
  await expect(links.nth(1)).toHaveClass(/\banchor\b/);
  await expect(links.first()).not.toHaveClass(/\banchor\b/);
  await links.nth(1).blur();
  await expect(shown).toBeHidden();
});

test('preview: a quick pass over a link shows and fetches nothing', async ({ page }) => {
  await open(page, 'page');
  const asked = fetches(page);
  await page.locator('article [data-preview]').first().evaluate((link) => {
    link.dispatchEvent(new PointerEvent('pointerenter'));
    link.dispatchEvent(new PointerEvent('pointerleave'));
  });
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 500)));
  await expect(page.locator('#preview')).toBeHidden();
  expect(asked).toEqual([]);
});

test.describe(() => {
  test.use({ failing: /\/wiki\/preview\.json/ });
  test('preview: a card the server cannot give shows nothing', async ({ page }) => {
    await page.route(/\/wiki\/preview\.json/, (route) => route.abort());
    await open(page, 'page');
    const link = page.locator('article [data-preview]').first();
    const failed = page.waitForEvent('requestfailed', (request) => request.url().includes('/wiki/preview.json'));
    await link.focus();
    await failed;
    await page.evaluate(() => new Promise((resolve) => setTimeout(resolve)));
    await expect(page.locator('#preview')).toBeHidden();
    await expect(link).not.toHaveAttribute('aria-describedby');
  });
});
