// axe on every route of the real server, at both widths and in both themes, with scripts
// on, once every module has started, and with the page's own scripts blocked, as a reader
// without scripts sees it (axe itself needs script). Under reduced motion, so no surface is
// caught halfway through its entrance, and the light takes the visitor's hour at once: the
// pages are not held at the demo's hour, so the contrast is checked at the hour of the run.
import { AxeBuilder } from '@axe-core/playwright';
import { test, expect, open, url, ROUTES, FAILING, either } from './test.mjs';
import { SIZES } from '../../../../tools/check/sizes.mjs';

const TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'];

for (const scripts of [true, false]) {
  for (const { name } of ROUTES) {
    for (const width of [1440, 390]) {
      for (const scheme of ['light', 'dark']) {
        test.describe(() => {
          test.use({ ...SIZES[width], colorScheme: scheme, reducedMotion: 'reduce', failing: either(FAILING[name], !scripts && /\.(js|wasm)$/) });
          test(`axe: ${name} at ${width} in ${scheme}, scripts ${scripts ? 'on' : 'off'}`, async ({ page }) => {
            if (scripts) await open(page, name, '', { held: false });
            else {
              await page.route(/\.(js|wasm)(\?|$)/, (request) => request.abort());
              await page.goto(url(name), { waitUntil: 'networkidle' });
            }
            const { violations } = await new AxeBuilder({ page }).withTags(TAGS).analyze();
            expect(violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(' ')).join(', ')}`)).toEqual([]);
          });
        });
      }
    }
  }
}
