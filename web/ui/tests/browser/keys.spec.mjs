import { test, expect, open, at } from './test.mjs';

test('a number jumps to its section', async ({ page }) => {
  await open(page, 'index');
  await page.keyboard.press('3');
  await expect(page).toHaveURL(at('changes'));
});

test('/ focuses the command line on home and opens the palette elsewhere', async ({ page }) => {
  await open(page, 'index');
  await page.keyboard.press('/');
  await expect(page.locator('#cmd-q')).toBeFocused();
  await open(page, 'wiki');
  await page.keyboard.press('/');
  await expect(page.locator('dialog.palette')).toBeVisible();
});

test('/ focuses the search field on the search page', async ({ page }) => {
  await open(page, 'search');
  await page.keyboard.press('/');
  await expect(page.locator('#q')).toBeFocused();
});

test('t switches the theme', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'light' });
  await open(page, 'wiki');
  await page.keyboard.press('t');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.keyboard.press('t');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});

test('j and k move through the rows and Enter opens one', async ({ page }) => {
  await open(page, 'changes');
  const rows = page.locator('main [data-row]');
  await page.keyboard.press('j');
  await expect(rows.nth(0)).toBeFocused();
  await expect(page.locator('html')).toHaveClass(/\bkbd\b/);
  await page.keyboard.press('j');
  await expect(rows.nth(1)).toBeFocused();
  await page.keyboard.press('k');
  await page.keyboard.press('k');
  await expect(rows.nth(0)).toBeFocused();
  const href = await rows.nth(0).getAttribute('href');
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(new RegExp(`${href.replace('.', '\\.')}$`));
});

test('the pointer hides the keyboard hints', async ({ page }) => {
  await open(page, 'wiki');
  await page.keyboard.press('Tab');
  await expect(page.locator('html')).toHaveClass(/\bkbd\b/);
  await page.mouse.down();
  await expect(page.locator('html')).not.toHaveClass(/\bkbd\b/);
});

test('keys typed into a field stay there', async ({ page }) => {
  await open(page, 'index');
  await page.locator('#cmd-q').focus();
  await page.keyboard.type('3t');
  await expect(page.locator('#cmd-q')).toHaveValue('3t');
  await expect(page).toHaveURL(at('index'));
  await expect(page.locator('html')).not.toHaveAttribute('data-theme');
});

test('with single-letter keys off, only chords work', async ({ page }) => {
  await open(page, 'wiki');
  await page.locator('html').evaluate((html) => { html.dataset.keys = 'off'; });
  await page.keyboard.press('3');
  await page.keyboard.press('t');
  await page.keyboard.press('j');
  await expect(page).toHaveURL(at('wiki'));
  await expect(page.locator('html')).not.toHaveAttribute('data-theme');
  await page.keyboard.press('Control+k');
  await expect(page.locator('dialog.palette')).toBeVisible();
});
