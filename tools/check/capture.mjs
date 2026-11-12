// Records what the real server shows with scripts on, into target/check/final: a video of a
// short visit (home's entrance, the command line, the palette, a page transition, a diff,
// the machines page), and home, a page and the machines page at 1440 and 390 px in day and
// night. The browser's clock is the demo's, 09:41 in Stockholm. Run through tools/browser.sh
// (mise run capture), which sets ATRIUM_URL.
import { mkdir, readdir, rename, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium } from 'playwright';
import { ROUTES } from './routes.mjs';
import { SIZES } from './sizes.mjs';

const out = new URL('../../target/check/final/', import.meta.url).pathname;
const atrium = process.env.ATRIUM_URL;
const DEMO_NOW = new Date('2026-11-11T08:41:00Z');
const path = (name) => ROUTES.find((route) => route.name === name).path;
const GL = ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'];

async function settled(page) {
  await page.locator('time.clock[datetime$="Z"]').waitFor({ state: 'attached' });
  await page.waitForFunction(() => document.documentElement.classList.contains('gl-on'));
  await page.evaluate(() => document.fonts.ready);
}

const browser = await chromium.launch({ args: GL });
await rm(out, { recursive: true, force: true });
await mkdir(join(out, 'video'), { recursive: true });

for (const name of ['index', 'page', 'machines']) {
  for (const width of [1440, 390]) {
    for (const theme of ['light', 'dark']) {
      const context = await browser.newContext({ ...SIZES[width], colorScheme: theme, timezoneId: 'Europe/Stockholm' });
      const page = await context.newPage();
      await page.clock.install({ time: DEMO_NOW });
      await page.goto(atrium + path(name));
      await settled(page);
      await page.waitForTimeout(2500);
      const file = `${name === 'index' ? 'home' : name}-${width}-${theme}.png`;
      await page.screenshot({ path: join(out, file) });
      await context.close();
      console.log(file);
    }
  }
}

const context = await browser.newContext({
  ...SIZES[1440],
  colorScheme: 'light',
  timezoneId: 'Europe/Stockholm',
  recordVideo: { dir: join(out, 'video'), size: SIZES[1440].viewport },
});
const page = await context.newPage();
await page.clock.install({ time: DEMO_NOW });
const started = Date.now();
await page.goto(`${atrium}/`);
await settled(page);
await page.waitForTimeout(2600);
await page.locator('#cmd-q').click();
await page.keyboard.type('restic', { delay: 140 });
await page.waitForTimeout(1200);
await page.keyboard.press('Escape');
await page.keyboard.press('Escape');
await page.keyboard.press('Control+k');
await page.waitForTimeout(500);
await page.keyboard.type('corne', { delay: 140 });
await page.waitForTimeout(1200);
await page.keyboard.press('Escape');
await page.waitForTimeout(400);
await page.locator('.tiles a[data-carry]', { hasText: 'Restic backups on laurel' }).first().click();
await settled(page);
await page.waitForTimeout(2200);
await page.locator('.doc-meta a', { hasText: 'What changed' }).click();
await settled(page);
await page.waitForTimeout(1600);
await page.mouse.wheel(0, 360);
await page.waitForTimeout(900);
await page.locator('[data-only-changes]').click();
await page.waitForTimeout(1400);
await page.keyboard.press('5');
await settled(page);
await page.waitForTimeout(2600);
const seconds = (Date.now() - started) / 1000;
await context.close();
const [video] = await readdir(join(out, 'video'));
await rename(join(out, 'video', video), join(out, 'visit.webm'));
await rm(join(out, 'video'), { recursive: true });
console.log(`visit.webm, about ${seconds.toFixed(1)} s`);
await browser.close();
