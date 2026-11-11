// Every page, at both widths and in both themes, under the server's CSP: no console
// error, no uncaught exception, no CSP violation, every module mounted and the field
// drawing.
import { readdirSync } from 'node:fs';
import { test, expect, open } from './test.mjs';

const pages = readdirSync(new URL('fixtures/', import.meta.url)).filter((name) => name.endsWith('.html')).sort();
const widths = { 1440: { width: 1440, height: 900 }, 390: { width: 390, height: 844 } };

for (const name of pages) {
  for (const [width, viewport] of Object.entries(widths)) {
    for (const scheme of ['light', 'dark']) {
      test(`${name} at ${width} in ${scheme}`, async ({ page }) => {
        await page.setViewportSize(viewport);
        await page.emulateMedia({ colorScheme: scheme });
        await page.addInitScript(() => {
          window.violations = [];
          document.addEventListener('securitypolicyviolation', (e) => window.violations.push(`${e.violatedDirective} ${e.blockedURI}`));
        });
        await open(page, `${name}?perf`);
        await expect(page.locator('html')).toHaveClass(/\bjs\b/);
        await expect(page.locator('html')).toHaveClass(/\bgl-on\b/);
        await expect(page.locator('pre.perf')).toContainText('fps');
        await page.mouse.move(viewport.width / 2, viewport.height / 2);
        await page.keyboard.press('Control+k');
        await expect(page.locator('dialog.palette')).toBeVisible();
        await page.keyboard.press('Escape');
        await page.evaluate(() => scrollTo(0, document.body.scrollHeight / 2));
        await page.waitForTimeout(300);
        expect(await page.evaluate(() => window.violations)).toEqual([]);
      });
    }
  }
}
