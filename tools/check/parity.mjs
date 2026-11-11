// Compares each prototype page with the same route from `atrium --demo`, at 1440 and 390 px
// wide, in the day and night themes: with scripts off on both sides, and with `--scripts`,
// with scripts on under reduced motion once the modules have started, both clocks at the
// demo's 09:41 in Stockholm. Each pair is a full-page screenshot; the share is the pixels
// that differ over the larger of the two pages, so a page that grew or shrank counts its
// extra height as different. Run through tools/browser.sh (mise run parity), which sets
// PROTOTYPE, ATRIUM_URL and ATRIUM_AWAY_URL.
//
// node tools/check/parity.mjs [--scripts] [route ...]
import { createServer } from 'node:http';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import { chromium } from 'playwright';
import { ROUTES } from './routes.mjs';
import { SIZES } from './sizes.mjs';

const args = process.argv.slice(2);
const scripts = args.includes('--scripts');
const only = args.filter((arg) => arg !== '--scripts');
const out = new URL(`../../target/check/parity${scripts ? '-scripts' : ''}/`, import.meta.url).pathname;
const DEMO_NOW = new Date('2026-11-11T08:41:00Z');
const TYPES = { '.wasm': 'application/wasm', '.html': 'text/html; charset=utf-8', '.css': 'text/css', '.js': 'text/javascript', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.json': 'application/json' };
// A channel that moves by more than this counts as a different pixel: enough to ignore
// antialiasing noise, not enough to ignore a changed glyph or colour.
const TOLERANCE = 24;

function serve(dir) {
  const server = createServer(async (req, res) => {
    const path = normalize(decodeURIComponent(new URL(req.url, 'http://x').pathname)).replace(/^\/+/, '') || 'index.html';
    try {
      const body = await readFile(join(dir, path));
      res.writeHead(200, { 'content-type': TYPES[extname(path)] || 'application/octet-stream' });
      res.end(body);
    } catch {
      res.writeHead(404);
      res.end();
    }
  });
  return new Promise((done) => server.listen(0, '127.0.0.1', () => done({ base: `http://127.0.0.1:${server.address().port}/`, close: () => server.close() })));
}

async function shot(browser, url, width, theme) {
  const context = await browser.newContext({ ...SIZES[width], colorScheme: theme, javaScriptEnabled: scripts, reducedMotion: 'reduce', timezoneId: 'Europe/Stockholm' });
  const page = await context.newPage();
  if (scripts) await page.clock.install({ time: DEMO_NOW });
  await page.goto(url, { waitUntil: 'networkidle' });
  if (scripts) {
    await page.waitForFunction(() => document.documentElement.classList.contains('gl-on'), null, { timeout: 10_000 }).catch(() => {});
    await page.waitForTimeout(500);
  }
  await page.evaluate(() => document.fonts.ready).catch(() => {});
  const png = await page.screenshot({ fullPage: true, animations: 'disabled' });
  await context.close();
  return png;
}

// The comparison runs in a page of its own, which can decode PNGs and read their pixels.
async function compare(browser, a, b) {
  const context = await browser.newContext();
  const page = await context.newPage();
  const result = await page.evaluate(async ({ a, b, tolerance }) => {
    const load = async (data) => { const image = new Image(); image.src = `data:image/png;base64,${data}`; await image.decode(); return image; };
    const [ia, ib] = await Promise.all([load(a), load(b)]);
    const w = Math.max(ia.width, ib.width), h = Math.max(ia.height, ib.height);
    const pixels = (image) => { const c = new OffscreenCanvas(w, h); const g = c.getContext('2d'); g.drawImage(image, 0, 0); return g.getImageData(0, 0, w, h).data; };
    const pa = pixels(ia), pb = pixels(ib);
    const diff = new OffscreenCanvas(w, h), dg = diff.getContext('2d'), dd = dg.createImageData(w, h);
    let differ = 0;
    for (let i = 0; i < pa.length; i += 4) {
      const off = Math.abs(pa[i] - pb[i]) > tolerance || Math.abs(pa[i + 1] - pb[i + 1]) > tolerance || Math.abs(pa[i + 2] - pb[i + 2]) > tolerance || Math.abs(pa[i + 3] - pb[i + 3]) > tolerance;
      if (off) { differ++; dd.data[i] = 220; dd.data[i + 1] = 30; dd.data[i + 2] = 30; dd.data[i + 3] = 255; }
      else { const v = 255 - (255 - (pa[i] + pa[i + 1] + pa[i + 2]) / 3) * 0.25; dd.data[i] = dd.data[i + 1] = dd.data[i + 2] = v; dd.data[i + 3] = 255; }
    }
    dg.putImageData(dd, 0, 0);
    const blob = await diff.convertToBlob({ type: 'image/png' });
    const bytes = new Uint8Array(await blob.arrayBuffer());
    let bin = ''; for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    return { differ, total: w * h, heights: [ia.height, ib.height], diff: btoa(bin) };
  }, { a: a.toString('base64'), b: b.toString('base64'), tolerance: TOLERANCE });
  await context.close();
  return result;
}

const server = await serve(process.env.PROTOTYPE);
const browser = await chromium.launch();
await mkdir(out, { recursive: true });
const rows = [];
for (const route of ROUTES.filter((r) => only.length === 0 || only.includes(r.name))) {
  for (const width of [1440, 390]) {
    for (const theme of ['light', 'dark']) {
      const base = route.away ? process.env.ATRIUM_AWAY_URL : process.env.ATRIUM_URL;
      const [a, b] = await Promise.all([shot(browser, server.base + route.file, width, theme), shot(browser, base + route.path, width, theme)]);
      const r = await compare(browser, a, b);
      const tag = `${route.name}-${width}-${theme}`;
      await writeFile(join(out, `${tag}-prototype.png`), a);
      await writeFile(join(out, `${tag}-atrium.png`), b);
      await writeFile(join(out, `${tag}-diff.png`), Buffer.from(r.diff, 'base64'));
      const percent = (100 * r.differ) / r.total;
      rows.push({ route: route.path, page: route.name, width, theme, percent: +percent.toFixed(2), heights: r.heights });
      console.log(`${route.name.padEnd(9)} ${String(width).padEnd(5)} ${theme.padEnd(6)} ${percent.toFixed(2).padStart(6)}%  heights ${r.heights.join(' / ')}`);
    }
  }
}
await writeFile(join(out, 'parity.json'), JSON.stringify(rows, null, 2));
await browser.close();
server.close();
