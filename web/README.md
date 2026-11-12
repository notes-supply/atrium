# Browser modules

Every behaviour a page has beyond plain HTML, in Rust compiled to Wasm. Pages work without them.

- `loader.js` is the only script a page loads.
- `ui/` is one bundle with every small module.
- `field/` is the WebGL2 background and its `?perf` readout.

## The field's stages

The field watches its own frames. Each leaves a GPU fence, and a frame the GPU has not finished by the time the next is due is late. Half a second of late frames steps it down one stage, and it never steps back up within a page:

| Stage | Draws |
| --- | --- |
| `full` | one canvas pixel per CSS pixel, at the display's rate |
| `half` | one canvas pixel per two CSS pixels each way, a quarter of the lattice pass |
| `slow` | half, at most ten frames a second |
| `still` | half, the frame reduced motion draws, only on change and at most ten times a second |

Hidden tabs, reduced motion and calm work as before at every stage. Why these stages, this measure and this window is in `docs/decisions.md`.

With `?perf`, the readout's last lines name the stage, its late time and late frames since the last step, and each step: to what, when, and on how many late frames. `?perf=full` holds the field at full quality. `window.__field`, which exists only with `?perf`, has `stage()` and `slow(ms)`, which adds `ms` of busy work to each drawn frame, counts it late and lets go of the hold.

`mise run build:wasm` builds both bundles into `target/web`, where the server embeds them as hashed assets (`crates/atrium/src/assets.rs`).

## What a page writes

The server writes the loader's element, naming the bundles' hashed files:

```html
<script type="module" src="/assets/loader.….js"
  data-ui="/assets/ui.….js" data-ui-wasm="/assets/ui_bg.….wasm"
  data-field="/assets/field.….js" data-field-wasm="/assets/field_bg.….wasm"></script>
```

The loader starts the field bundle when the page has `data-module="field"`, and the ui bundle when the page names any other module. Each module mounts on the elements whose `data-module` names it. One element may name several, as in `data-module="keys light carry"`.

| Module | Element | What it needs from the server |
| --- | --- | --- |
| `field` | `canvas.field` | `data-surface` on surfaces, `data-want` (1 or 2) on what wants the reader, `data-reading` on the article, the map's `.dock` and `.dock-card` with `data-m` and `data-want`, and `data-entrance` (`full` or `short`) on `<html>`. |
| `palette` | the Jump link, and home's `.cmd` | `data-pages` on either: `/wiki/index.json`, answering `[{ "t", "path", "m", "href" }]`. Sections come from `.index a[data-key]`, named by their `data-title`. |
| `keys` | `<body>` | `.index a[data-key]`, `main [data-row]`, `#q` or `.cmd input`. |
| `prefs` | each `.prefs` | `.swatch[data-intensity]`, `.theme-btn`, and a `.keys-btn` for single-letter keys. |
| `swap` | the search form | `[data-swap-target]` and `[data-count]` in the page the form's action answers. |
| `preview` | the article | `data-previews` on it: `/wiki/preview.json`, which answers `?href=` and a link's `data-preview` with `{ "path", "title", "text", "meta" }`. A card is fetched when the reader means the link, after a short hover or on focus, and kept for the page's life. |
| `toc` | `details.toc` | links to `#id` headings. |
| `continue` | the article, with `data-page`; home's Continue reading tile | the tile, empty and `hidden`, with only its header; on the article, `.resume-note` `hidden` with the new mark and an empty `<span>`. |
| `diff` | the diff section | `[data-only-changes]`. |
| `carry` | `<body>` | `a[data-carry]` on titles, `.doc-title` on a page's heading. |
| `clock` | `time.clock` | nothing more. |
| `greet` | the greeting's `<span>` | nothing more. |
| `light` | `<body>` | `data-hour` on `<html>`, and in the stylesheet each hour's `--sun` and `--warm` and the ease's length, `--hour-ease`. |

The clock, the greeting and the light follow the visitor's own clock. The server writes them for its `--time-zone`, so the two agree, and nothing changes as the modules start, whenever the visitor is in that zone.

The light eases to a new hour, as a page opens on another hour than the server's or when the hour turns, over `--hour-ease`, forty minutes, in steps. Every thirty seconds `light` sets `--sun` and `--warm` on `<html>`, a step that moves no colour channel by more than one level of 255, and once the ease is over it takes them off, so the hour's own rule holds. Between steps the page does no work. Each hour's numbers are read back from the stylesheet, which stays the only place they are written. Under reduced motion the light takes the new hour at once, and a page asked for one hour with `?hour` keeps the hour it was written in. `light` tells the field to read its colours again when the hour turns, every two minutes of the ease and when it ends.

The preferences are cookies the server reads on the next request: `atrium-intensity` (`calm`, `normal`, `lively`), `atrium-theme` (`light`, `dark`) and `atrium-keys` (`on`, `off`). The server writes each as `data-intensity`, `data-theme` and `data-keys` on `<html>`.

The stylesheet carries `.pb-fill.pct-N` and `.pb-n.pct-N` for every N from 0 to 100, and keeps `hidden` slots hidden. Rules that only need scripts on ask `@media (scripting: enabled)`, so they hold from the first paint; `gl-on`, which the field sets once it draws, is the one class a module adds for the stylesheet.

## Checks

- `mise run gate` holds the bundles to their brotli budgets: `ui` 45 KB, `field` 30 KB, the loader 1 KB.
- `mise run test:browser` drives every module in Chromium against `atrium --demo`, under the server's own CSP: each module's behaviour, every route at 1440 and 390 px in both themes with no console error, uncaught exception or CSP violation, axe with scripts on and off, and the first paint. It runs in Playwright's image through `tools/browser.sh`, which installs the packages `package-lock.json` pins with `npm ci`. The field, the routes and axe run with WebGL in SwiftShader, where the field steps down for real, so the tests of the full picture hold it with `?perf=full`; the test that a cheap field stays at full quality runs alone after the rest (`--no-deps` runs it without them). The other modules run without WebGL, on the CSS lattice: each page starts the field at full quality, and on one core its first second or two still took the CPU those tests need. With `PROTOTYPE`, the prototype's folder, it also checks that the field draws the prototype's own pixels on the prototype's page.
- `CPUSET=7 mise run test:browser` runs the browser, its GPU process and both servers on one core, about as slow as GitHub's two-core runner, where a test that waits for a time instead of a condition fails first.
- `CPUSET=2,3 mise run bench:field` times the field in SwiftShader, held at full quality, on two pinned cores, beside the prototype's own field when `PROTOTYPE` is set. A CPU quota throttles the software renderer into slow frames that are not real.
- `mise run bench:light` counts the page's style recalculations, layouts, paints and frames, and the CPU of the renderer and the GPU process, while the light eases and with it held, on `/wiki` at 1440 by 900 without the field.
- The tests open pages held at the demo's hour with `?hour=9` (`open` in `test.mjs`), so the light does not move under a test that is not about it. The light's own tests set the page's clock instead, and axe runs under reduced motion, where the light does not ease, at the hour of the run.
