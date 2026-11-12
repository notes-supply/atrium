// The light of the hour (web/ui/src/light.rs): the hour on <html> follows the visitor's
// clock, and the light eases to it over forty minutes in steps, with the page still between
// them. The demo writes every page at 9; each test sets the visitor's clock with page.clock,
// and runs it forward rather than waiting.
import { test, expect, open } from './test.mjs';

// Wednesday 11 November 2026, in the container's time zone.
const clockAt = (hh, mm, ss = 0) => new Date(2026, 10, 11, hh, mm, ss);
const MINUTE = 60_000;
const STEP = 30_000;

// Sets the visitor's clock just before `time` and stops it at `time`, so only the test
// moves it: the light module's steps then fall at known times.
async function stopClockAt(page, time) {
  await page.clock.install({ time: new Date(time.getTime() - 1000) });
  await page.clock.pauseAt(time);
}

// The light as <html> computes it, and whether the module is holding it partway.
const light = (page) => page.evaluate(() => {
  const root = document.documentElement;
  const style = getComputedStyle(root);
  return { hour: root.dataset.hour, sun: style.getPropertyValue('--sun'), warm: style.getPropertyValue('--warm'), stepping: root.style.length > 0 };
});

// Hour `hour`'s numbers as the stylesheet writes them.
const hourly = (page, hour) => page.evaluate((hour) => {
  const probe = document.createElement('span');
  probe.hidden = true;
  probe.dataset.hour = hour;
  document.body.append(probe);
  const style = getComputedStyle(probe);
  const numbers = { sun: style.getPropertyValue('--sun'), warm: style.getPropertyValue('--warm') };
  probe.remove();
  return numbers;
}, String(hour));

// The page's style recalculations, layouts and paints while `during` runs, from a trace.
async function work(browser, page, during) {
  await browser.startTracing(page, { categories: ['devtools.timeline'] });
  await during();
  const { traceEvents } = JSON.parse((await browser.stopTracing()).toString());
  const count = (name) => traceEvents.filter((e) => e.name === name && (e.ph === 'X' || e.ph === 'B')).length;
  return { style: count('UpdateLayoutTree'), layout: count('Layout'), paint: count('Paint') };
}

test('light: the hour on <html> follows the visitor\'s clock, and turns while the page is open', async ({ page }) => {
  await page.clock.install({ time: clockAt(14, 59) });
  await open(page, 'wiki', '', { held: false });
  await expect(page.locator('html')).toHaveAttribute('data-hour', '14');
  await page.evaluate(() => { window.hours = 0; document.addEventListener('atrium:hour', () => window.hours++); });
  await page.clock.fastForward(61_000);
  await expect(page.locator('html')).toHaveAttribute('data-hour', '15');
  expect(await page.evaluate(() => window.hours)).toBe(1);
});

// A transition of the light restyled, laid out and repainted the whole page every frame for
// forty minutes, 120 times in this window at 60 frames a second, and restarted the page's
// own transitions of colour every frame, so it never settled. Here the page's own clock
// stands still between two steps, while the browser draws on.
test('light: a page opened on another hour eases to it in steps, with no style work, layout or paint between them', async ({ page, browser }) => {
  await stopClockAt(page, clockAt(20, 0));
  await open(page, 'wiki', '', { held: false });
  const [nine, twenty] = [await hourly(page, 9), await hourly(page, 20)];
  // As at an hour turn: the page names the new hour at once, and the light starts from the
  // server's.
  const start = await light(page);
  expect(start).toMatchObject({ hour: '20', stepping: true });
  expect(Math.abs(start.sun - nine.sun)).toBeLessThan(0.01);
  await expect.poll(() => page.evaluate(() => document.getAnimations().length), { message: 'the page settles: nothing animates' }).toBe(0);
  expect(await work(browser, page, () => page.waitForTimeout(2000)), 'between steps').toEqual({ style: 0, layout: 0, paint: 0 });
  // The next step moves the light by a step's worth, and the page restyles and repaints for it.
  const step = await work(browser, page, async () => {
    await page.clock.runFor(STEP);
    await page.waitForTimeout(1500);
  });
  expect(step.style, 'a step restyles').toBeGreaterThan(0);
  expect(step.paint, 'a step repaints').toBeGreaterThan(0);
  const moved = await light(page);
  expect(+moved.sun).toBeLessThan(+start.sun);
  expect(+start.sun - moved.sun).toBeLessThan((nine.sun - twenty.sun) * 2 * STEP / (40 * MINUTE));
  await page.clock.runFor(40 * MINUTE);
  expect(await light(page)).toEqual({ hour: '20', stepping: false, ...twenty });
});

test('light: the hour turns, and the light lands on the next hour\'s exact --sun and --warm', async ({ page }) => {
  await stopClockAt(page, clockAt(9, 59, 40));
  await open(page, 'wiki', '', { held: false });
  const [nine, ten] = [await hourly(page, 9), await hourly(page, 10)];
  expect(await light(page)).toEqual({ hour: '9', stepping: false, ...nine });
  await page.evaluate(() => { window.told = 0; document.addEventListener('atrium:hour', () => window.told++); });
  await page.clock.runFor(MINUTE);
  const turned = await light(page);
  expect(turned).toMatchObject({ hour: '10', stepping: true });
  expect(Math.abs(turned.sun - nine.sun)).toBeLessThan(0.01);
  await page.clock.runFor(20 * MINUTE);
  const halfway = await light(page);
  expect(+halfway.sun).toBeGreaterThan(+nine.sun + 0.3 * (ten.sun - nine.sun));
  expect(+halfway.sun).toBeLessThan(+nine.sun + 0.7 * (ten.sun - nine.sun));
  await page.clock.runFor(20 * MINUTE);
  expect(await light(page)).toEqual({ hour: '10', stepping: false, ...ten });
  // The field reads its colours again when the hour turns, every fourth of the eighty steps,
  // as every two minutes before, and as the light lands: 1 + 20 + 1. Then not while it stands.
  expect(await page.evaluate(() => window.told)).toBe(22);
  await page.clock.runFor(10 * MINUTE);
  expect(await page.evaluate(() => window.told)).toBe(22);
});

// Each step moves every colour the hour sets by at most one level of 255 in any channel, in
// either theme, the least a screen can show: here from 9 to 20, the farthest the demo's
// pages can open from their hour. A step a minute moved some by two.
test('light: no step moves a colour by more than one level', async ({ page }) => {
  await stopClockAt(page, clockAt(20, 0));
  await open(page, 'wiki', '', { held: false });
  const colours = () => page.evaluate(() => {
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 1;
    const g = canvas.getContext('2d', { willReadFrequently: true });
    const out = [];
    for (const scheme of ['scheme-light', 'scheme-dark']) {
      const probe = document.createElement('span');
      probe.hidden = true;
      probe.className = scheme;
      document.body.append(probe);
      for (const token of ['--ground', '--paper', '--rule', '--rule-2', '--code-bg', '--field-dot']) {
        probe.style.color = `var(${token})`;
        g.clearRect(0, 0, 1, 1);
        g.fillStyle = getComputedStyle(probe).color;
        g.fillRect(0, 0, 1, 1);
        out.push(...g.getImageData(0, 0, 1, 1).data.slice(0, 3));
      }
      probe.remove();
    }
    return out;
  });
  let last = await colours();
  let largest = 0;
  for (let step = 0; step < 80; step++) {
    await page.clock.runFor(STEP);
    const now = await colours();
    largest = Math.max(largest, ...now.map((value, i) => Math.abs(value - last[i])));
    last = now;
  }
  expect(await light(page)).toMatchObject({ hour: '20', stepping: false });
  expect(largest).toBe(1);
});

test('light: under reduced motion the light takes the new hour at once', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await stopClockAt(page, clockAt(20, 0));
  await open(page, 'wiki', '', { held: false });
  expect(await light(page)).toEqual({ hour: '20', stepping: false, ...await hourly(page, 20) });
  await page.clock.runFor(61 * MINUTE);
  expect(await light(page)).toEqual({ hour: '21', stepping: false, ...await hourly(page, 21) });
});

test('light: a page asked for one hour keeps it', async ({ page }) => {
  await page.clock.install({ time: clockAt(14, 0) });
  await open(page, 'wiki', 'hour=9');
  await expect(page.locator('html')).toHaveAttribute('data-hour', '9');
  // and its light, through the turn of the hour: the light module leaves it alone
  await page.clock.runFor(61 * MINUTE);
  expect(await light(page)).toEqual({ hour: '9', stepping: false, ...await hourly(page, 9) });
});
