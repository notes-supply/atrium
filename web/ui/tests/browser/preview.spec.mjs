import { test, expect, open } from './test.mjs';

const cards = (page) => page.evaluate(() => JSON.parse(document.getElementById('previews').textContent));

test('preview: hovering a wiki link shows its card, anchored to it', async ({ page }) => {
  await open(page, 'page.html');
  const data = await cards(page);
  const link = page.locator('article [data-preview]').first();
  const key = await link.getAttribute('data-preview');
  await link.hover();
  const card = page.locator('#preview');
  await expect(card).toBeVisible();
  await expect(card.locator('.pv-title')).toHaveText(data[key].title);
  await expect(card.locator('.pv-path')).toHaveText(data[key].path);
  await expect(card.locator('.pv-meta')).toHaveText(data[key].meta);
  await expect(card).toContainText(data[key].text);
  await expect(link).toHaveClass(/\banchor\b/);
  await expect(link).toHaveAttribute('aria-describedby', 'preview');
  const [l, c] = [await link.boundingBox(), await card.boundingBox()];
  expect(Math.abs(c.y - (l.y + l.height)) < 40 || Math.abs(l.y - (c.y + c.height)) < 40, 'beside the link').toBe(true);
  await page.mouse.move(5, 5);
  await expect(card).toBeHidden();
  await expect(link).not.toHaveAttribute('aria-describedby');
});

test('preview: focus shows the card at once, Escape and blur hide it', async ({ page }) => {
  await open(page, 'page.html');
  const links = page.locator('article [data-preview]');
  await links.first().focus();
  const card = page.locator('#preview');
  await expect(card).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(card).toBeHidden();
  await links.nth(1).focus();
  await expect(card).toBeVisible();
  await expect(links.nth(1)).toHaveClass(/\banchor\b/);
  await expect(links.first()).not.toHaveClass(/\banchor\b/);
  await links.nth(1).blur();
  await expect(card).toBeHidden();
});

test('preview: a quick pass over a link shows nothing', async ({ page }) => {
  await open(page, 'page.html');
  await page.locator('article [data-preview]').first().hover();
  await page.mouse.move(5, 5);
  await page.waitForTimeout(500);
  await expect(page.locator('#preview')).toBeHidden();
});
