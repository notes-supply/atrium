import { test, expect, open, cookie, url, at } from './test.mjs';

test('a swatch sets the field intensity and remembers it', async ({ page }) => {
  await open(page, 'index');
  const lively = page.locator('.prefs-mast .swatch[data-intensity=lively]');
  await lively.click();
  await expect(page.locator('html')).toHaveAttribute('data-intensity', 'lively');
  await expect(lively).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('.prefs-mast .swatch[data-intensity=normal]')).toHaveAttribute('aria-pressed', 'false');
  expect(await cookie(page, 'atrium-intensity')).toBe('lively');
});

test('the theme button switches day and night and remembers it', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'light' });
  await open(page, 'wiki');
  const button = page.locator('.prefs-mast .theme-btn');
  await expect(button).toHaveAttribute('aria-pressed', 'false');
  await button.click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await expect(button).toHaveAttribute('aria-pressed', 'true');
  expect(await cookie(page, 'atrium-theme')).toBe('dark');
  await button.click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});

test('the theme button follows the system until it is pressed', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await open(page, 'wiki');
  await expect(page.locator('.prefs-mast .theme-btn')).toHaveAttribute('aria-pressed', 'true');
});

test('the keys switch turns single-letter keys off, and the server keeps them off', async ({ page }) => {
  await open(page, 'wiki');
  const button = page.locator('.prefs-mast .keys-btn');
  await expect(button).toHaveAttribute('aria-pressed', 'true');
  await button.click();
  await expect(page.locator('html')).toHaveAttribute('data-keys', 'off');
  await expect(button).toHaveAttribute('aria-pressed', 'false');
  expect(await cookie(page, 'atrium-keys')).toBe('off');

  const served = await page.goto(url('wiki'));
  expect(await served.text()).toContain('data-keys="off"');
  await expect(page.locator('time.clock[datetime$="Z"]')).toHaveCount(1);
  await expect(button).toHaveAttribute('aria-pressed', 'false');
  await page.keyboard.press('3');
  await page.keyboard.press('Control+k');
  await expect(page.locator('dialog.palette')).toBeVisible();
  await expect(page).toHaveURL(at('wiki'));
  await page.keyboard.press('Escape');

  await button.click();
  expect(await cookie(page, 'atrium-keys')).toBe('on');
  const again = await page.goto(url('wiki'));
  expect(await again.text()).not.toContain('data-keys=');
  await expect(page.locator('time.clock[datetime$="Z"]')).toHaveCount(1);
  await expect(button).toHaveAttribute('aria-pressed', 'true');
  await page.keyboard.press('3');
  await expect(page).toHaveURL(at('changes'));
});
