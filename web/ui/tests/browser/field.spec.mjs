import { test, expect, open, url } from './test.mjs';

const prototype = `http://127.0.0.1:${process.env.PROTOTYPE_PORT}`;

// Counts the field's calls for animation frames, from before the page's own scripts run.
const countFrames = (page) => page.addInitScript(() => {
  window.frames_asked = 0;
  const ask = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = (callback) => { window.frames_asked++; return ask(callback); };
});
const framesAsked = (page) => page.evaluate(() => window.frames_asked);
const framesOver = async (page, ms) => {
  const before = await framesAsked(page);
  await page.waitForTimeout(ms);
  return (await framesAsked(page)) - before;
};
// Waits until the page has asked for `count` more frames, as the field does frame after
// frame while it moves. A frame takes as long as the machine needs, so this counts frames,
// not time.
const keepsAsking = async (page, count = 5) => {
  const before = await framesAsked(page);
  await expect.poll(() => framesAsked(page), { message: 'asks for frame after frame', timeout: 20_000 }).toBeGreaterThan(before + count);
};

// Draws one frame through the ?perf bench and copies the canvas in the same task, before
// the drawing buffer is cleared. Then, for each box, the share of pixels that are dots,
// against the ground (the canvas's most common colour), and the count tinted hot.
async function sample(page, boxes) {
  return page.evaluate((boxes) => {
    window.__field.pause(true);
    window.__field.bench(1, 3);
    const canvas = document.querySelector('canvas.field');
    const copy = document.createElement('canvas');
    copy.width = canvas.width;
    copy.height = canvas.height;
    const g = copy.getContext('2d');
    g.drawImage(canvas, 0, 0);
    const all = g.getImageData(0, 0, copy.width, copy.height).data;
    const seen = new Map();
    for (let i = 0; i < all.length; i += 4) {
      const key = (all[i] << 16) | (all[i + 1] << 8) | all[i + 2];
      seen.set(key, (seen.get(key) ?? 0) + 1);
    }
    const top = [...seen].sort((a, b) => b[1] - a[1])[0][0];
    const ground = [top >> 16, (top >> 8) & 255, top & 255];
    const measure = ([x, y, w, h]) => {
      const d = g.getImageData(x, y, w, h).data;
      let dots = 0, hot = 0;
      for (let i = 0; i < d.length; i += 4) {
        const diff = Math.abs(d[i] - ground[0]) + Math.abs(d[i + 1] - ground[1]) + Math.abs(d[i + 2] - ground[2]);
        if (diff > 24) dots++;
        if (d[i] - d[i + 2] > 40 && d[i] - d[i + 1] > 30) hot++;
      }
      return { dots: dots / (w * h), hot, ground };
    };
    return [measure([0, 0, copy.width, copy.height]), ...boxes.map(measure)];
  }, boxes);
}

test('field: draws the lattice, finest and in the colour of trouble around what needs you', async ({ page }) => {
  await open(page, 'index', 'perf');
  await expect(page.locator('html')).toHaveClass(/\bgl-on\b/);
  const needs = await page.locator('.map [data-m=aeolus].dock circle').boundingBox();
  const at = [Math.round(needs.x + needs.width / 2) - 60, Math.round(needs.y + needs.height / 2) - 60, 120, 120];
  const [whole, near, corner] = await sample(page, [at, [0, 0, 160, 60]]);
  expect(whole.dots, 'the resting lattice').toBeGreaterThan(0);
  expect(near.dots, 'denser where something needs you').toBeGreaterThan(whole.dots * 2);
  expect(near.hot, 'tinted in the colour of trouble').toBeGreaterThan(0);
  expect(corner.hot, 'only there').toBe(0);
});

test('field: draws one pixel per CSS pixel on a dense screen', async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 });
  const page = await context.newPage();
  await page.goto(url('wiki'));
  await expect(page.locator('html')).toHaveClass(/\bgl-on\b/);
  const canvas = await page.locator('canvas.field').evaluate((c) => [c.width, c.height, getComputedStyle(c).imageRendering]);
  expect(canvas).toEqual([1440, 900, 'pixelated']);
  await page.setViewportSize({ width: 1000, height: 700 });
  await expect.poll(() => page.locator('canvas.field').evaluate((c) => [c.width, c.height])).toEqual([1000, 700]);
  await context.close();
});

test('field: breathes while shown, and stops asking for frames when hidden', async ({ page }) => {
  await countFrames(page);
  await open(page, 'wiki');
  await expect(page.locator('html')).toHaveClass(/\bgl-on\b/);
  await keepsAsking(page);
  await page.evaluate(() => {
    Object.defineProperty(document, 'hidden', { value: true, configurable: true });
    document.dispatchEvent(new Event('visibilitychange'));
  });
  expect(await framesOver(page, 500)).toBe(0);
  await page.evaluate(() => {
    Object.defineProperty(document, 'hidden', { value: false, configurable: true });
    document.dispatchEvent(new Event('visibilitychange'));
  });
  await keepsAsking(page);
});

test('field: holds a still frame under reduced motion', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await countFrames(page);
  await open(page, 'wiki');
  await expect(page.locator('html')).toHaveClass(/\bgl-on\b/);
  expect(await framesOver(page, 600)).toBe(0);
});

// The springs ease by at most 50 ms a frame, so on a slow machine calm takes more time to
// settle, never more frames.
test('field: settles to a still frame in calm', async ({ page }) => {
  await countFrames(page);
  await open(page, 'wiki');
  await expect(page.locator('html')).toHaveClass(/\bgl-on\b/);
  await keepsAsking(page);
  await page.locator('.prefs-mast .swatch[data-intensity=calm]').click();
  await page.mouse.move(5, 5);
  await expect.poll(() => framesOver(page, 600), { message: 'still', timeout: 20_000 }).toBe(0);
});

test('field: takes the night colours when the theme switches', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'light' });
  await open(page, 'wiki', 'perf');
  const [day] = await sample(page, []);
  await page.evaluate(() => window.__field.pause(false));
  await page.locator('.prefs-mast .theme-btn').click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  const [night] = await sample(page, []);
  const light = (rgb) => rgb[0] + rgb[1] + rgb[2];
  expect(light(night.ground)).toBeLessThan(light(day.ground) - 200);
});

test('perf: ?perf shows the readout, and nothing shows without it', async ({ page }) => {
  await open(page, 'wiki');
  await expect(page.locator('pre.perf')).toHaveCount(0);
  expect(await page.evaluate(() => 'window.__field' in window || window.__field !== undefined)).toBe(false);
  await open(page, 'wiki', 'perf');
  const readout = page.locator('pre.perf');
  await expect(readout).toContainText('field normal  1440x900 css px');
  await expect(readout).toContainText(/^\s*\d+ fps {3}gap p90 \d+\.\d ms$/m);
  await expect(readout).toContainText(/^cpu {2}med \d+\.\d\d {2}p90 \d+\.\d\d ms$/m);
  // SwiftShader answers a timer query once its GPU process has caught up with the page,
  // which on one core took 3.8 to 4.3 s for the first.
  await expect(readout).toContainText(/^gpu {2}(no timer query|\d+\.\d\d ms)$/m, { timeout: 20_000 });
});

// Once the field has drawn a few frames with the palette open (with ?perf, the readout
// asks for some of the frames counted), the lattice gathers under it, more than twice as
// dense as across the whole canvas.
async function pulled(page) {
  const palette = await page.locator('dialog.palette').boundingBox();
  const box = [Math.round(palette.x), Math.round(palette.y), Math.round(palette.width), Math.round(palette.height)];
  await keepsAsking(page, 20);
  const [whole, under] = await sample(page, [box]);
  expect(under.dots, 'denser under the palette').toBeGreaterThan(whole.dots * 2);
}

test('field: the open palette pulls the lattice to itself', async ({ page }) => {
  await countFrames(page);
  await open(page, 'wiki', 'perf');
  await expect(page.locator('html')).toHaveClass(/\bgl-on\b/);
  await page.keyboard.press('Control+k');
  await expect(page.locator('dialog.palette')).toBeVisible();
  await pulled(page);
});

test('field: a palette opened before the field starts pulls it too', async ({ page }) => {
  let release;
  const held = new Promise((resolve) => { release = resolve; });
  await page.route(/\/field_bg\.[0-9a-f]+\.wasm$/, async (route) => { await held; await route.continue(); });
  await countFrames(page);
  await open(page, 'wiki', 'perf');
  await page.keyboard.press('Control+k');
  await expect(page.locator('dialog.palette')).toBeVisible();
  release();
  // Only now does the field compile its Wasm and its shaders, in software, which the other
  // tests do while the page opens.
  await expect(page.locator('html')).toHaveClass(/\bgl-on\b/, { timeout: 15_000 });
  await pulled(page);
});

// The field is still under reduced motion: no time, no drift, no entrance. So the
// prototype's JavaScript and this Wasm must draw the same pixels on the prototype's page.
test('field: draws the prototype\'s pixels', async ({ browser }) => {
  test.skip(!process.env.PROTOTYPE, 'PROTOTYPE names the prototype folder');
  test.setTimeout(90_000);
  const frame = async (path) => {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce', colorScheme: 'light' });
    const page = await context.newPage();
    await page.goto(path);
    await page.waitForFunction(() => window.__field && document.documentElement.classList.contains('gl-on'));
    await page.evaluate(() => document.fonts.ready);
    await page.mouse.move(1439, 899);
    await page.waitForTimeout(400);
    const pixels = await page.evaluate(() => {
      window.__field.pause(true);
      window.__field.bench(1, 3);
      const canvas = document.querySelector('canvas.field');
      const copy = document.createElement('canvas');
      copy.width = canvas.width;
      copy.height = canvas.height;
      const g = copy.getContext('2d');
      g.drawImage(canvas, 0, 0);
      const rgba = g.getImageData(0, 0, copy.width, copy.height).data;
      let binary = '';
      for (let i = 0; i < rgba.length; i += 4) binary += String.fromCharCode(rgba[i], rgba[i + 1], rgba[i + 2]);
      return btoa(binary);
    });
    await context.close();
    return Buffer.from(pixels, 'base64');
  };
  const ours = await frame(`${prototype}/wasm/wiki.html?hour=9&perf`);
  const theirs = await frame(`${prototype}/prototype/wiki.html?hour=9&perf`);
  expect(ours.length).toBe(theirs.length);
  let dots = 0;
  let differ = 0;
  for (let i = 0; i < ours.length; i += 3) {
    if (ours[i] !== ours[0] || ours[i + 1] !== ours[1] || ours[i + 2] !== ours[2]) dots++;
    if (ours[i] !== theirs[i] || ours[i + 1] !== theirs[i + 1] || ours[i + 2] !== theirs[i + 2]) differ++;
  }
  expect(dots, 'a lattice, not a blank frame').toBeGreaterThan(1000);
  expect(differ, `${differ} of ${ours.length / 3} pixels differ`).toBe(0);
});
