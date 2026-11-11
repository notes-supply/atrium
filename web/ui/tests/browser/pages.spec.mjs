// Every route of the real server, at both widths and in both themes, under its own CSP: no
// console error, no uncaught exception, no CSP violation, every module mounted, the field
// drawing and the palette opening.
import { test, expect, open, ROUTES, FAILING, either } from './test.mjs';
import { SIZES } from '../../../../tools/check/sizes.mjs';

for (const { name } of ROUTES) {
  for (const width of [1440, 390]) {
    for (const scheme of ['light', 'dark']) {
      test.describe(() => {
        test.use({ ...SIZES[width], colorScheme: scheme, failing: either(FAILING[name]) });
        test(`${name} at ${width} in ${scheme}`, async ({ page }) => {
          await page.addInitScript(() => {
            window.violations = [];
            document.addEventListener('securitypolicyviolation', (e) => window.violations.push(`${e.violatedDirective} ${e.blockedURI}`));
          });
          await open(page, name, 'perf');
          await expect(page.locator('html')).toHaveClass(/\bgl-on\b/);
          await expect(page.locator('pre.perf')).toContainText('fps');
          await page.mouse.move(SIZES[width].viewport.width / 2, SIZES[width].viewport.height / 2);
          await page.keyboard.press('Control+k');
          await expect(page.locator('dialog.palette')).toBeVisible();
          await page.keyboard.press('Escape');
          await page.evaluate(() => scrollTo(0, document.body.scrollHeight / 2));
          await page.waitForTimeout(300);
          expect(await page.evaluate(() => window.violations)).toEqual([]);
        });
      });
    }
  }
}
