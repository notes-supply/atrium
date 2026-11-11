import { test, expect, open, cookie } from './test.mjs';

test('a swatch sets the field intensity and remembers it', async ({ page }) => {
  await open(page, 'index.html');
  const lively = page.locator('.prefs-mast .swatch[data-intensity=lively]');
  await lively.click();
  await expect(page.locator('html')).toHaveAttribute('data-intensity', 'lively');
  await expect(lively).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('.prefs-mast .swatch[data-intensity=normal]')).toHaveAttribute('aria-pressed', 'false');
  expect(await cookie(page, 'atrium-intensity')).toBe('lively');
});

test('the theme button switches day and night and remembers it', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'light' });
  await open(page, 'wiki.html');
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
  await open(page, 'wiki.html');
  await expect(page.locator('.prefs-mast .theme-btn')).toHaveAttribute('aria-pressed', 'true');
});

test('a keys switch in the settings turns single-letter shortcuts off', async ({ page }) => {
  await open(page, 'wiki.html');
  await page.locator('.prefs-mast').evaluate((prefs) => {
    prefs.insertAdjacentHTML('beforeend', '<button type="button" class="keys-btn" aria-pressed="true">Keys</button>');
  });
  await page.locator('.prefs-mast .keys-btn').click();
  await expect(page.locator('html')).toHaveAttribute('data-keys', 'off');
  await expect(page.locator('.prefs-mast .keys-btn')).toHaveAttribute('aria-pressed', 'false');
  expect(await cookie(page, 'atrium-keys')).toBe('off');
});
