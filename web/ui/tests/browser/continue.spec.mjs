import { test, expect, open, ROUTES } from './test.mjs';

const RESTIC = ROUTES.find((route) => route.name === 'page').path;

test('continue: with nothing read, home keeps the tile hidden', async ({ page }) => {
  await open(page, 'index');
  await expect(page.locator('.tile.resume')).toBeHidden();
});

test('continue: a page remembers where you stopped, and home offers it', async ({ page }) => {
  await open(page, 'page');
  await expect(page.locator('.resume-note')).toBeHidden();
  await page.locator('#restore-drill-november').evaluate((h) => h.scrollIntoView({ block: 'start' }));
  await page.mouse.wheel(0, 40);
  await expect.poll(() => page.evaluate(() => localStorage.getItem('atrium-reading'))).toContain('restore-drill-november');

  await open(page, 'index');
  const tile = page.locator('.tile.resume');
  await expect(tile).toBeVisible();
  await expect(tile.locator('.resume-title a')).toHaveText('Restic backups on apollo');
  await expect(tile.locator('.resume-title a')).toHaveAttribute('href', RESTIC);
  await expect(tile.locator('.resume-where')).toHaveText('You stopped at Restore drill, November, just now.');
  await expect(tile.locator('.resume-where a')).toHaveAttribute('href', `${RESTIC}#restore-drill-november`);
  const label = await tile.locator('.progress-bar').getAttribute('aria-label');
  const percent = Number(label.match(/^(\d+) percent read$/)[1]);
  expect(percent).toBeGreaterThan(0);
  await expect(tile.locator('.pb-n')).toHaveText(`${percent}%`);
  const fill = await tile.locator('.pb-fill').evaluate((e) => e.getBoundingClientRect().width / e.parentElement.getBoundingClientRect().width);
  expect(Math.abs(fill * 100 - percent)).toBeLessThanOrEqual(3);

  await open(page, 'page');
  const note = page.locator('.resume-note');
  await expect(note).toBeVisible();
  await expect(note).toContainText(`You were here before, at Restore drill, November, ${percent}% through.`);
  await expect(note.locator('a')).toHaveAttribute('href', '#restore-drill-november');
});

test('continue: a page read to the end is no longer offered', async ({ page }) => {
  await open(page, 'page');
  await page.evaluate(() => scrollTo(0, document.body.scrollHeight));
  await page.mouse.wheel(0, 40);
  await expect.poll(() => page.evaluate(() => localStorage.getItem('atrium-reading'))).toMatch(/\t100\t/);
  await open(page, 'index');
  await expect(page.locator('.tile.resume')).toBeHidden();
});
