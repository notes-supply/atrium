// Frame time of the field in SwiftShader (software GL on the CPU) at 1440 by 900, pixel
// ratio 1 and 2, for this Wasm field and, with PROTOTYPE set, the prototype's JavaScript
// one on the same page, alternating, five rounds each.
//
// Bench: the page's own loop is paused, then 150 forced frames are drawn through the
// ?perf handle, each closed with readPixels so the time includes rasterising. Four
// variants split the cost: the read-back alone, pass one, pass two, both.
// Live: the page's own loop runs with the pointer moving for three seconds, and the ?perf
// readout's numbers are read from the page.
// The cgroup's throttle counters are read around each run.
import { readFileSync, writeFileSync } from 'node:fs';
import { test } from '@playwright/test';

const ROUNDS = 5;
const WHAT = ['read-back only', 'pass one (focus, 6 px cells)', 'pass two (lattice, every pixel)', 'both passes'];
const cg = () => {
  try {
    const t = Object.fromEntries(readFileSync('/sys/fs/cgroup/cpu.stat', 'utf8').trim().split('\n').map((l) => l.split(' ')));
    return { n: +t.nr_throttled, us: +t.throttled_usec };
  } catch { return null; }
};
const stat = (ts) => {
  const s = [...ts].sort((a, b) => a - b);
  const q = (p) => +s[Math.min(s.length - 1, Math.floor(s.length * p))].toFixed(2);
  return { n: s.length, median: q(0.5), p90: q(0.9), max: q(1), over40: s.filter((x) => x > 40).length };
};

test('field frame time', async ({ browser }) => {
  const sides = [['wasm', 'wiki.html'], ...(process.env.PROTOTYPE ? [['js', 'prototype/wiki.html']] : [])];
  const out = { cpus: null, renderer: null, runs: [] };
  for (const dpr of [1, 2]) {
    for (let round = 0; round < ROUNDS; round++) {
      for (const [side, path] of round % 2 ? [...sides].reverse() : sides) {
        const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: dpr, colorScheme: 'light' });
        const page = await context.newPage();
        await page.goto(`${path}?hour=9&perf`);
        await page.waitForFunction(() => window.__field && document.documentElement.classList.contains('gl-on'));
        await page.evaluate(() => document.fonts.ready);
        await page.waitForTimeout(1500);
        if (!out.renderer) {
          out.renderer = await page.evaluate(() => {
            const gl = window.__field.gl;
            const info = gl.getExtension('WEBGL_debug_renderer_info');
            return info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER);
          });
          out.cpus = await page.evaluate(() => navigator.hardwareConcurrency);
        }
        const canvas = await page.evaluate(() => {
          const c = document.querySelector('canvas.field');
          return `${c.width}x${c.height} drawn at dpr ${devicePixelRatio}`;
        });
        await page.mouse.move(1100, 600);
        await page.evaluate(() => window.__field.pause(true));
        for (const which of [0, 1, 2, 3]) {
          await page.evaluate(() => window.__field.bench(10, 3));
          const c0 = cg();
          const ts = await page.evaluate((w) => window.__field.bench(150, w), which);
          const c1 = cg();
          out.runs.push({ side, dpr, round, canvas, what: WHAT[which], ...stat(ts), throttled: c0 && c1 ? c1.n - c0.n : null });
        }
        await page.evaluate(() => window.__field.pause(false));
        for (let i = 0; i < 30; i++) await page.mouse.move(400 + (i % 10) * 70, 300 + (i % 5) * 60, { steps: 2 });
        await page.waitForTimeout(800);
        const readout = await page.locator('pre.perf').textContent();
        const number = (re) => +(readout.match(re)?.[1] ?? NaN);
        out.runs.push({
          side, dpr, round, canvas, what: 'live readout',
          fps: number(/(\d+) fps/), gapP90: number(/gap p90 ([\d.]+)/), cpuMedian: number(/cpu {2}med ([\d.]+)/), cpuP90: number(/med [\d.]+ {2}p90 ([\d.]+)/),
          gpu: readout.match(/gpu {2}(.*)$/m)?.[1],
        });
        await context.close();
      }
    }
  }
  writeFileSync('/out/field-bench.json', JSON.stringify(out, null, 1));
  const rows = [];
  for (const dpr of [1, 2]) {
    for (const what of [...WHAT, 'live readout']) {
      for (const [side] of sides) {
        const runs = out.runs.filter((r) => r.dpr === dpr && r.what === what && r.side === side);
        const pick = (k) => runs.map((r) => r[k]).sort((a, b) => a - b);
        const mid = (a) => a[Math.floor(a.length / 2)];
        if (what === 'live readout') {
          rows.push(`dpr ${dpr}  ${side.padEnd(4)}  ${what.padEnd(32)}  cpu med ${mid(pick('cpuMedian'))} (${pick('cpuMedian')[0]}–${pick('cpuMedian').at(-1)})  cpu p90 ${mid(pick('cpuP90'))}  fps ${mid(pick('fps'))}  gpu ${runs[0].gpu}`);
        } else {
          rows.push(`dpr ${dpr}  ${side.padEnd(4)}  ${what.padEnd(32)}  median ${mid(pick('median'))} ms (rounds ${pick('median')[0]}–${pick('median').at(-1)})  p90 ${mid(pick('p90'))}  max ${pick('max').at(-1)}  >40ms ${pick('over40').reduce((a, b) => a + b, 0)}  throttled ${pick('throttled').reduce((a, b) => a + (b ?? 0), 0)}`);
        }
      }
    }
  }
  console.log(`${out.renderer}, ${out.cpus} cpus seen\n${rows.join('\n')}`);
});
