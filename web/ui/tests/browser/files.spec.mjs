// The wiki's files in the browser, against atrium --demo: an image in a page, a download and
// a resumed one, the list without scripts, a file's page, Jump finding a file, a script in
// an SVG opened directly, and the phone width.
import { test as plain } from '@playwright/test';
import { test, expect, open, url } from './test.mjs';
import { SIZES } from '../../../../tools/check/sizes.mjs';

const KIT = '/wiki/raw/brand/kit.zip';

test('a page shows its image within the column and marks its file links', async ({ page }) => {
  await open(page, 'brand');
  const image = page.locator('article.paper img');
  await expect(image).toHaveAttribute('src', '/wiki/raw/brand/logo.svg');
  await expect(image).toHaveAttribute('loading', 'lazy');
  await image.scrollIntoViewIfNeeded();
  await expect.poll(() => image.evaluate((img) => img.complete && img.naturalWidth)).toBeGreaterThan(0);
  const [img, paper] = await Promise.all([image.boundingBox(), page.locator('article.paper').boundingBox()]);
  expect(img.x + img.width).toBeLessThanOrEqual(paper.x + paper.width);
  await expect(page.locator('a.file-link').first()).toHaveText('The whole kit, ZIP, 1.2 KiB');
});

test('a download keeps its name and resumes from where it stopped', async ({ page }) => {
  await page.goto(url('file').replace('logo.svg', 'kit.zip'));
  await expect(page.locator('time.clock[datetime$="Z"]')).toHaveCount(1);
  const [download] = await Promise.all([page.waitForEvent('download'), page.locator('a.download-btn').click()]);
  expect(download.suggestedFilename()).toBe('kit.zip');
  const whole = await (await import('node:fs/promises')).readFile(await download.path());
  expect(whole.length).toBe(1179);
  const resumed = await page.evaluate(async (kit) => {
    const first = await fetch(kit);
    const tag = first.headers.get('etag');
    const rest = await fetch(kit, { headers: { range: 'bytes=1000-', 'if-range': tag } });
    const bytes = new Uint8Array(await rest.arrayBuffer());
    return { status: rest.status, range: rest.headers.get('content-range'), length: bytes.length, first: bytes[0] };
  }, KIT);
  expect(resumed).toEqual({ status: 206, range: 'bytes 1000-1178/1179', length: 179, first: whole[1000] });
});

test.describe('without scripts', () => {
  test.use({ javaScriptEnabled: false });
  test('the files list browses by folder, sorts and filters', async ({ page }) => {
    await page.goto(url('files'));
    await page.locator('tbody a.ttl', { hasText: 'brand/' }).click();
    await expect(page.locator('h1')).toHaveText('brand');
    await expect(page.locator('.crumbs a')).toHaveText(['Files']);
    await expect(page.locator('tbody a.ttl').first()).toHaveText('print/');
    await page.locator('thead a.sorter', { hasText: 'Size' }).click();
    await expect(page.locator('th[aria-sort=descending]')).toHaveText('Size');
    await expect(page.locator('tbody tr:not(.folder-row) a.ttl').first()).toHaveText('kit.zip');
    await page.locator('#file-q').fill('pal');
    await page.locator('form.file-find button').click();
    await expect(page).toHaveURL(/under=brand&q=pal/);
    await expect(page.locator('tbody a.ttl')).toHaveText(['palette.txt']);
    await page.locator('tbody a.ttl').click();
    await expect(page.locator('h1')).toHaveText('palette.txt');
    await expect(page.locator('.text-preview code')).toContainText('crumb   #f3e3c3');
  });
});

test('a file\'s page shows it, its facts and every version', async ({ page }) => {
  await open(page, 'file');
  await expect(page.locator('h1')).toHaveText('logo.svg');
  await expect.poll(() => page.locator('.img-preview img').evaluate((img) => img.complete && img.naturalWidth)).toBeGreaterThan(0);
  await expect(page.locator('.file-events tbody tr')).toHaveCount(2);
  await expect(page.locator('.file-events tbody tr').first()).toContainText('edited');
  const [download] = await Promise.all([
    page.waitForEvent('download'),
    page.locator('.file-events a[download]').last().click(),
  ]);
  expect(download.suggestedFilename()).toBe('logo.svg');
  const old = await (await import('node:fs/promises')).readFile(await download.path(), 'utf8');
  expect(old).toContain('<script>');
});

test('Jump finds a file by name and opens its page', async ({ page }) => {
  await open(page, 'wiki');
  await page.keyboard.press('Control+k');
  const dialog = page.locator('dialog.palette[open]');
  await page.keyboard.type('kit.zip');
  const first = dialog.locator('[role=option]').first();
  await expect(first).toContainText('kit.zip');
  await expect(first).toContainText('ZIP · 1.2 KiB');
  await expect(dialog.locator('.pal-group').first()).toHaveText('Files');
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/\/wiki\/files\/brand\/kit\.zip$/);
  await expect(page.locator('h1')).toHaveText('kit.zip');
});

test('Jump lists the files section', async ({ page }) => {
  await open(page, 'machines');
  await page.keyboard.press('Control+k');
  await page.keyboard.type('files');
  await expect(page.locator('dialog.palette [role=option]').first()).toContainText('Files');
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/\/wiki\/files$/);
});

// The logo's first export carries a script. Opened directly it is a document of Atrium's own
// origin, so Mneme's policy, which Atrium passes on, must keep it from running. The browser
// says so in the console, which is the point here, so this test uses Playwright's own page.
plain('an SVG with a script, opened directly, does not run it', async ({ page, baseURL }) => {
  await page.goto(`${baseURL}/wiki/files/brand/logo.svg`);
  const first = await page.locator('.file-events a[download]').last().getAttribute('href');
  expect(first).toMatch(/\?at=[0-9a-f]{40}$/);
  const response = await page.goto(baseURL + first);
  expect(response.headers()['content-security-policy']).toBe("default-src 'none'; style-src 'unsafe-inline'; sandbox");
  await page.waitForTimeout(200);
  expect(await page.evaluate(() => document.documentElement.getAttribute('data-ran'))).toBeNull();
  // Without the policy the same bytes run their script, so the check above can fail.
  await page.route(`**${first.replace('?', '\\?')}`, async (route) => {
    const real = await route.fetch();
    const headers = { ...real.headers() };
    delete headers['content-security-policy'];
    await route.fulfill({ response: real, headers });
  });
  await page.goto(baseURL + first);
  await expect.poll(() => page.evaluate(() => document.documentElement.getAttribute('data-ran'))).toBe('yes');
});

test.describe('on a phone', () => {
  test.use({ ...SIZES[390] });
  for (const name of ['files', 'folder', 'file', 'brand']) {
    test(`${name} fits the width`, async ({ page }) => {
      await open(page, name);
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
      for (const box of await page.locator('main img, main table, main .file-view').evaluateAll((all) => all.map((e) => e.getBoundingClientRect().right))) {
        expect(box).toBeLessThanOrEqual(390);
      }
    });
  }
});
