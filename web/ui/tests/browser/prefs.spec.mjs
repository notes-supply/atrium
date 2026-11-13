import { test, expect, open, cookie, url, at } from './test.mjs';

// A choice's button on the settings page, by its setting and value.
const choice = (page, name, value) => page.locator(`form.prefs button[name=${name}][value=${value}]`);

// Marks the document, so a test can tell that a click changed the page in place.
const mark = (page) => page.evaluate(() => { window.unchanged = true; });
const unchanged = (page) => page.evaluate(() => window.unchanged === true);

test('every page has a settings button beside Jump that leads to the settings', async ({ page }) => {
  await open(page, 'wiki');
  const button = page.getByRole('link', { name: 'Settings', exact: true });
  await expect(button).toHaveAttribute('href', '/settings');
  const [jump, settings] = await Promise.all([page.locator('a.jump').boundingBox(), button.boundingBox()]);
  expect(settings.x).toBeGreaterThan(jump.x + jump.width);
  expect(settings.height).toBe(jump.height);
  await button.click();
  await expect(page).toHaveURL(/\/settings$/);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Settings');
  await expect(page.getByRole('link', { name: 'Settings', exact: true })).toHaveAttribute('aria-current', 'page');
});

test('a field choice sets the intensity in place and remembers it', async ({ page }) => {
  await open(page, 'settings');
  await mark(page);
  const lively = choice(page, 'intensity', 'lively');
  await lively.click();
  await expect(page.locator('html')).toHaveAttribute('data-intensity', 'lively');
  await expect(lively).toHaveAttribute('aria-pressed', 'true');
  await expect(choice(page, 'intensity', 'normal')).toHaveAttribute('aria-pressed', 'false');
  expect(await cookie(page, 'atrium-intensity')).toBe('lively');
  expect(await unchanged(page)).toBe(true);
});

test('the theme follows the system until a theme is chosen, and again when asked', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'light' });
  await open(page, 'settings');
  await mark(page);
  const system = choice(page, 'theme', 'system');
  const night = choice(page, 'theme', 'dark');
  await expect(system).toHaveAttribute('aria-pressed', 'true');
  await night.click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await expect(night).toHaveAttribute('aria-pressed', 'true');
  await expect(system).toHaveAttribute('aria-pressed', 'false');
  expect(await cookie(page, 'atrium-theme')).toBe('dark');
  await system.click();
  await expect(page.locator('html')).not.toHaveAttribute('data-theme');
  await expect(system).toHaveAttribute('aria-pressed', 'true');
  expect(await cookie(page, 'atrium-theme')).toBeUndefined();
  expect(await unchanged(page)).toBe(true);
});

test('T on the settings page presses the theme it switched to', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await open(page, 'settings');
  await page.keyboard.press('t');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(choice(page, 'theme', 'light')).toHaveAttribute('aria-pressed', 'true');
  await expect(choice(page, 'theme', 'system')).toHaveAttribute('aria-pressed', 'false');
});

test('the keys switch turns single-letter keys off, and the server keeps them off', async ({ page }) => {
  await open(page, 'settings');
  const off = choice(page, 'keys', 'off');
  const on = choice(page, 'keys', 'on');
  await expect(on).toHaveAttribute('aria-pressed', 'true');
  await off.click();
  await expect(page.locator('html')).toHaveAttribute('data-keys', 'off');
  await expect(off).toHaveAttribute('aria-pressed', 'true');
  expect(await cookie(page, 'atrium-keys')).toBe('off');

  const served = await page.goto(url('wiki'));
  expect(await served.text()).toContain('data-keys="off"');
  await expect(page.locator('time.clock[datetime$="Z"]')).toHaveCount(1);
  await page.keyboard.press('3');
  await page.keyboard.press('Control+k');
  await expect(page.locator('dialog.palette')).toBeVisible();
  await expect(page).toHaveURL(at('wiki'));
  await page.keyboard.press('Escape');

  await open(page, 'settings');
  await expect(off).toHaveAttribute('aria-pressed', 'true');
  await on.click();
  expect(await cookie(page, 'atrium-keys')).toBe('on');
  const again = await page.goto(url('wiki'));
  expect(await again.text()).not.toContain('data-keys=');
  await expect(page.locator('time.clock[datetime$="Z"]')).toHaveCount(1);
  await page.keyboard.press('3');
  await expect(page).toHaveURL(at('changes'));
});

test.describe('scripts off', () => {
  test.use({ javaScriptEnabled: false });

  test('each choice posts, and the page comes back with it pressed and drawn', async ({ page }) => {
    await page.goto(url('settings'));
    await choice(page, 'theme', 'dark').click();
    await expect(page).toHaveURL(/\/settings$/);
    await expect(choice(page, 'theme', 'dark')).toHaveAttribute('aria-pressed', 'true');
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    expect(await cookie(page, 'atrium-theme')).toBe('dark');

    await choice(page, 'intensity', 'calm').click();
    await expect(choice(page, 'intensity', 'calm')).toHaveAttribute('aria-pressed', 'true');
    await expect(page.locator('html')).toHaveAttribute('data-intensity', 'calm');

    await choice(page, 'keys', 'off').click();
    await expect(choice(page, 'keys', 'off')).toHaveAttribute('aria-pressed', 'true');
    await expect(page.locator('html')).toHaveAttribute('data-keys', 'off');

    await choice(page, 'theme', 'system').click();
    await expect(choice(page, 'theme', 'system')).toHaveAttribute('aria-pressed', 'true');
    await expect(page.locator('html')).not.toHaveAttribute('data-theme');
    expect(await cookie(page, 'atrium-theme')).toBeUndefined();
    await expect(page.getByRole('link', { name: 'Settings', exact: true })).toBeVisible();
  });
});

test('the server sets the same cookie the module does', async ({ browser }) => {
  const kept = async (scripts) => {
    const context = await browser.newContext({ javaScriptEnabled: scripts });
    const page = await context.newPage();
    if (scripts) await open(page, 'settings');
    else await page.goto(url('settings'));
    await choice(page, 'intensity', 'lively').click();
    await expect(choice(page, 'intensity', 'lively')).toHaveAttribute('aria-pressed', 'true');
    const { expires, ...rest } = (await context.cookies()).find((c) => c.name === 'atrium-intensity');
    await context.close();
    return { ...rest, year: Math.round((expires - Date.now() / 1000) / 86400) };
  };
  const server = await kept(false);
  expect(server).toEqual(await kept(true));
  expect(server).toMatchObject({ value: 'lively', path: '/', httpOnly: false, secure: false, sameSite: 'Lax', year: 365 });
});
