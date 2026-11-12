// What the light of the hour costs the browser while it eases, against the hour held with
// ?hour=9, on /wiki at 1440 by 900 without WebGL, so no field is drawn and the light is the
// only thing that moves. Three rounds each, alternating.
//
// The demo's clock is 09:41 in Stockholm, so the server writes data-hour="9". The visitor's
// time zone is set so that their hour is 15, whatever the time of the run, and the light
// module moves the page to it as it starts: the light then eases from 9 to 15.
//
// Over 5 s once the page has settled: the page's style recalculations and layouts from the
// DevTools protocol's Performance.getMetrics, the frames, paints and rasterisations from a
// trace, and the CPU time of the renderer and the GPU process from SystemInfo. While the
// light eases, also over the next 31 s, so a light that steps every 30 s steps once in it.
//
//   mise run bench:light
import { test } from '@playwright/test';

const ROUNDS = 3;
const WINDOW = 5000;
const HOUR = 15;
const SETTLE = 2500;
const STEPPED = 31_000;

test.use({ launchOptions: { args: ['--disable-webgl'] } });

// An Etc/GMT zone in which the visitor's hour is HOUR now. Etc/GMT-N is N hours ahead of UTC.
function zoneFor(hour) {
  const ahead = ((hour - new Date().getUTCHours() + 36) % 24) - 12;
  return ahead === 0 ? 'Etc/GMT' : `Etc/GMT${ahead > 0 ? '-' : '+'}${Math.abs(ahead)}`;
}

const TRACED = ['UpdateLayoutTree', 'Layout', 'PrePaint', 'Paint', 'Commit', 'DrawFrame', 'RasterTask'];

async function measure(browser, cdpBrowser, query, windows) {
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, colorScheme: 'light', timezoneId: zoneFor(HOUR) });
  const page = await context.newPage();
  await page.goto(`${process.env.ATRIUM_URL}/wiki${query}`);
  await page.waitForFunction(() => document.querySelector('time.clock')?.getAttribute('datetime')?.endsWith('Z'));
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(SETTLE);
  const cdp = await context.newCDPSession(page);
  await cdp.send('Performance.enable');
  const metrics = async () => Object.fromEntries((await cdp.send('Performance.getMetrics')).metrics.map((m) => [m.name, m.value]));
  const cpu = async () => {
    const { processInfo } = await cdpBrowser.send('SystemInfo.getProcessInfo');
    const sum = (type) => processInfo.filter((p) => p.type === type).reduce((a, p) => a + p.cpuTime, 0);
    return { renderer: sum('renderer'), gpu: sum('GPU') };
  };
  const state = await page.evaluate(() => ({
    hour: document.documentElement.dataset.hour,
    animations: document.getAnimations().map((a) => `${a.constructor.name} ${a.transitionProperty ?? a.animationName ?? ''}`.trim()),
  }));
  const out = [];
  for (const window of windows) {
    await browser.startTracing(page, { categories: ['devtools.timeline', 'disabled-by-default-devtools.timeline', 'disabled-by-default-devtools.timeline.frame'] });
    const sunBefore = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--sun'));
    const m0 = await metrics();
    const c0 = await cpu();
    const t0 = Date.now();
    await page.waitForTimeout(window);
    const seconds = (Date.now() - t0) / 1000;
    const m1 = await metrics();
    const c1 = await cpu();
    const trace = JSON.parse((await browser.stopTracing()).toString());
    const sunAfter = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--sun'));
    const events = trace.traceEvents ?? trace;
    const counts = Object.fromEntries(TRACED.map((name) => [name, events.filter((e) => e.name === name && ['X', 'B', 'I', 'i', 'n'].includes(e.ph)).length]));
    const per = (x) => +(x / seconds).toFixed(3);
    out.push({
      query: (query || '(easing)') + (window === WINDOW ? '' : ` ${window / 1000} s`),
      ...state,
      sunBefore,
      sunAfter,
      seconds: +seconds.toFixed(2),
      recalcStyle: m1.RecalcStyleCount - m0.RecalcStyleCount,
      recalcStyleMs: +((m1.RecalcStyleDuration - m0.RecalcStyleDuration) * 1000).toFixed(1),
      layout: m1.LayoutCount - m0.LayoutCount,
      ...counts,
      rendererCpus: per(c1.renderer - c0.renderer),
      gpuCpus: per(c1.gpu - c0.gpu),
    });
  }
  await context.close();
  return out;
}

test('light cost', async ({ browser }) => {
  test.setTimeout(15 * 60_000);
  const cdpBrowser = await browser.newBrowserCDPSession();
  const runs = [];
  for (let round = 0; round < ROUNDS; round++) {
    for (const query of round % 2 ? ['?hour=9', ''] : ['', '?hour=9']) {
      const windows = query ? [WINDOW] : [WINDOW, STEPPED];
      for (const run of await measure(browser, cdpBrowser, query, windows)) runs.push({ round, ...run });
    }
  }
  for (const run of runs) console.log(JSON.stringify(run));
  const mid = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
  const keys = ['recalcStyle', 'recalcStyleMs', 'layout', ...TRACED, 'rendererCpus', 'gpuCpus'];
  for (const query of ['(easing)', '?hour=9', `(easing) ${STEPPED / 1000} s`]) {
    const mine = runs.filter((r) => r.query === query);
    console.log(`${query.padEnd(14)} median of ${mine.length}: ${keys.map((k) => `${k} ${mid(mine.map((r) => r[k]))}`).join(', ')}`);
  }
});
