// Recent changes, on home's card and on the Changes page: the mark of what is new since the
// last visit sits in the gutter left of the time, clear of the card's edge and of the time,
// centred on the time's first line, and no row moves for having one.
import { test, expect, open } from './test.mjs';
import { SIZES } from '../../../../tools/check/sizes.mjs';

// Each row of the change lists in `card`: where its time's first line starts and the
// middle of its glyphs, its title's left edge, and the box of the mark, which ::before draws
// on the row or on its time, absolutely positioned against the element it is drawn on.
const rows = (page, card) => page.locator(card).evaluate((surface) => {
  const s = surface.getBoundingClientRect();
  const inner = s.left + surface.clientLeft;
  const pen = document.createElement('canvas').getContext('2d');
  return [...surface.querySelectorAll('.change-list li')].map((li) => {
    const time = li.querySelector('time');
    const text = document.createRange();
    text.selectNodeContents(time);
    const line = text.getClientRects()[0];
    const font = getComputedStyle(time);
    pen.font = `${font.fontWeight} ${font.fontSize} ${font.fontFamily}`;
    const ink = pen.measureText(time.textContent);
    const baseline = line.top + ink.fontBoundingBoxAscent;
    let mark = null;
    for (const element of [li, time]) {
      const css = getComputedStyle(element, '::before');
      if (css.content === 'none' || css.content === 'normal') continue;
      const at = element.getBoundingClientRect();
      const left = at.left + element.clientLeft + parseFloat(css.left);
      const top = at.top + element.clientTop + parseFloat(css.top);
      mark = { left, right: left + parseFloat(css.width), middle: top + parseFloat(css.height) / 2 };
    }
    return {
      new: li.hasAttribute('data-new'),
      inner,
      mark,
      time: { left: line.left, middle: baseline - (ink.actualBoundingBoxAscent - ink.actualBoundingBoxDescent) / 2 },
      title: li.querySelector('.ttl').getBoundingClientRect().left,
    };
  });
});

for (const [name, card] of [['index', '.changes'], ['changes', '.days']]) {
  for (const width of [1440, 390]) {
    test.describe(() => {
      test.use({ ...SIZES[width] });
      test(`the new mark on ${name} at ${width} sits in the gutter, centred on the time`, async ({ page }) => {
        await open(page, name);
        await page.evaluate(() => document.fonts.ready);
        const all = await rows(page, card);
        expect(all.filter((row) => row.new).length).toBeGreaterThan(0);
        for (const row of all) {
          if (!row.new) {
            expect(row.mark).toBeNull();
            continue;
          }
          expect(row.mark.left - row.inner, 'clear of the card\'s edge').toBeGreaterThanOrEqual(4);
          expect(row.time.left - row.mark.right, 'clear of the time').toBeGreaterThanOrEqual(4);
          expect(Math.abs(row.mark.middle - row.time.middle), 'centred on the time').toBeLessThanOrEqual(0.5);
        }
        expect(new Set(all.map((row) => row.time.left)).size, 'times in one column').toBe(1);
        expect(new Set(all.map((row) => row.title)).size, 'titles in one column').toBe(1);
      });
    });
  }
}

// A space at the start of an inline-block's content collapses, so the arrow keeps its
// distance by a margin.
test('"All changes" keeps a space before its arrow', async ({ page }) => {
  await open(page, 'index');
  const after = await page.locator('.changes a.more').evaluate((more) => {
    const css = getComputedStyle(more, '::after');
    return { content: css.content, space: parseFloat(css.marginLeft) + parseFloat(css.paddingLeft) };
  });
  expect(after.content).toBe('"→"');
  expect(after.space).toBeGreaterThanOrEqual(4);
});
