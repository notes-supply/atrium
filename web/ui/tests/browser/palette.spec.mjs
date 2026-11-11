import { test, expect, open } from './test.mjs';

test('Ctrl+K opens the palette, filters pages and opens the best match', async ({ page }) => {
  await open(page, 'wiki.html');
  await page.keyboard.press('Control+k');
  const dialog = page.locator('dialog.palette[open]');
  await expect(dialog).toBeVisible();
  await expect(dialog.locator('input')).toBeFocused();
  await expect(dialog.locator('.pal-group')).toHaveText(['Sections', 'Pages']);
  await expect(dialog.locator('[role=option]')).toHaveCount(14);
  await page.keyboard.type('corne');
  const options = dialog.locator('[role=option]');
  await expect(options.first()).toContainText('Keyboard build: Corne v4');
  await expect(options.first().locator('b')).toHaveText(['Corne']);
  await expect(options.last()).toHaveText(/Search the wiki for “corne”/);
  await expect(options.first()).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('ArrowDown');
  await expect(options.nth(1)).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/\/page\.html$/);
});

test('Escape and Ctrl+K close the palette', async ({ page }) => {
  await open(page, 'changes.html');
  await page.keyboard.press('Control+k');
  await expect(page.locator('dialog.palette')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.locator('dialog.palette')).toBeHidden();
  await page.keyboard.press('Control+k');
  await expect(page.locator('dialog.palette')).toBeVisible();
  await page.keyboard.press('Control+k');
  await expect(page.locator('dialog.palette')).toBeHidden();
});

test('the Jump link opens the palette instead of the search page', async ({ page }) => {
  await open(page, 'history.html');
  await page.locator('a.jump').click();
  await expect(page.locator('dialog.palette')).toBeVisible();
  await expect(page).toHaveURL(/history\.html$/);
});

test('a query with no page by that name searches the wiki', async ({ page }) => {
  await open(page, 'wiki.html');
  await page.keyboard.press('Control+k');
  await page.keyboard.type('zqx');
  const options = page.locator('dialog.palette [role=option]');
  await expect(options).toHaveCount(1);
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/search\.html\?q=zqx$/);
});

test('a setting runs in place', async ({ page }) => {
  await open(page, 'wiki.html');
  await page.keyboard.press('Control+k');
  await page.keyboard.type('lively');
  await page.keyboard.press('Enter');
  await expect(page.locator('html')).toHaveAttribute('data-intensity', 'lively');
  await expect(page.locator('dialog.palette')).toBeHidden();
});

test('home has the same list inline, without the settings, under the command line', async ({ page }) => {
  await open(page, 'index.html');
  const input = page.locator('#cmd-q');
  const list = page.locator('#cmd-list');
  await expect(list).toBeHidden();
  await input.focus();
  await expect(list).toBeVisible();
  await expect(input).toHaveAttribute('aria-expanded', 'true');
  await expect(list.locator('.pal-group')).toHaveText(['Sections', 'Pages']);
  await input.fill('restic');
  await expect(list.locator('[role=option]').first()).toContainText('Restic backups on apollo');
  await page.keyboard.press('Escape');
  await expect(input).toHaveValue('');
  await page.keyboard.press('Escape');
  await expect(list).toBeHidden();
  await input.focus();
  await input.fill('zqx');
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/search\.html\?q=zqx$/);
});
