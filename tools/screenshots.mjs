// The README's screenshots, of `atrium --demo` in the browser tests' Chromium with the field
// in WebGL through SwiftShader, at 1440 by 900, into docs/images: home in day and night, a
// page, a diff and the machines. Reduced motion skips the entrance and holds the field on
// its still frame, which draws everything a surface asks of it at once, so every run draws
// the same pictures, but for one level of colour in the few pixels where Chromium dithers a
// gradient. The browser's clock is the demo's, 09:41 in Stockholm. Run through
// tools/browser.sh (mise run screenshots), which sets ATRIUM_URL.
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium } from 'playwright';
import { ROUTES } from './check/routes.mjs';
import { SIZES } from './check/sizes.mjs';

const out = new URL('../docs/images/', import.meta.url).pathname;
const atrium = process.env.ATRIUM_URL;
const DEMO_NOW = new Date('2026-11-11T08:41:00Z');
const GL = ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'];
const path = (name) => ROUTES.find((route) => route.name === name).path;

const SHOTS = [
  { file: 'home-light.png', route: 'index', theme: 'light' },
  { file: 'home-dark.png', route: 'index', theme: 'dark' },
  { file: 'page.png', route: 'page', theme: 'light' },
  { file: 'diff.png', route: 'diff', theme: 'dark' },
  { file: 'machines.png', route: 'machines', theme: 'light' },
];

const browser = await chromium.launch({ args: GL });
await mkdir(out, { recursive: true });
for (const shot of SHOTS) {
  const context = await browser.newContext({
    ...SIZES[1440],
    colorScheme: shot.theme,
    reducedMotion: 'reduce',
    timezoneId: 'Europe/Stockholm',
  });
  const page = await context.newPage();
  await page.clock.install({ time: DEMO_NOW });
  await page.goto(atrium + path(shot.route));
  // Every module has mounted once the clock writes a full time, and the field has drawn
  // once it sets gl-on.
  await page.locator('time.clock[datetime$="Z"]').waitFor({ state: 'attached' });
  await page.waitForFunction(() => document.documentElement.classList.contains('gl-on'));
  await page.evaluate(() => document.fonts.ready);
  await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
  // At full quality the field has one canvas pixel per CSS pixel; a stage down has half.
  const [canvas, window] = await page.locator('canvas.field').evaluate((c) => [c.width, innerWidth]);
  if (canvas !== window) throw new Error(`${shot.file}: the field stepped down to ${canvas} of ${window} px`);
  await page.screenshot({ path: join(out, shot.file), animations: 'disabled', caret: 'hide' });
  await context.close();
  console.log(shot.file);
}
await browser.close();
