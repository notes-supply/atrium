// Runs axe on each route of `atrium --demo` at 1440 and 390 px in the day and night themes,
// with the page's own scripts blocked, as a reader without scripts sees it. Prints every
// violation and exits 1 if there is one.
//
// node axe.mjs <prototype folder, unused> <atrium base URL> <output folder>
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium } from 'playwright';
import { AxeBuilder } from '@axe-core/playwright';
import { ROUTES } from './routes.mjs';
import { SIZES } from './sizes.mjs';

const [, atrium, out] = process.argv.slice(2);
const browser = await chromium.launch();
const results = [];
let failed = 0;
for (const route of ROUTES) {
  const base = route.away ? atrium.replace(/:(\d+)$/, (_, port) => `:${+port + 1}`) : atrium;
  for (const width of [1440, 390]) {
    for (const theme of ['light', 'dark']) {
      const context = await browser.newContext({ ...SIZES[width], colorScheme: theme, reducedMotion: 'reduce' });
      await context.route(/\.(js|wasm)(\?|$)/, (request) => request.abort());
      const page = await context.newPage();
      await page.goto(base + route.path, { waitUntil: 'networkidle' });
      const { violations } = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice']).analyze();
      await context.close();
      failed += violations.length;
      results.push({ route: route.name, width, theme, violations: violations.map((v) => ({ id: v.id, impact: v.impact, nodes: v.nodes.map((n) => n.target.join(' ')) })) });
      console.log(`${route.name.padEnd(9)} ${String(width).padEnd(5)} ${theme.padEnd(6)} ${violations.length === 0 ? 'no violations' : violations.map((v) => `${v.id} (${v.nodes.length})`).join(', ')}`);
    }
  }
}
await writeFile(join(out, 'axe.json'), JSON.stringify(results, null, 2));
await browser.close();
process.exit(failed ? 1 : 0);
