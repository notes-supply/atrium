# Browser modules

Every behaviour a page has beyond plain HTML, in Rust compiled to Wasm. Pages work without them.

- `loader.js` is the only script a page loads.
- `ui/` is one bundle with every small module.
- `field/` is the WebGL2 background and its `?perf` readout.

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
| `light` | `<body>` | `data-hour` on `<html>`. |

The clock, the greeting and the light follow the visitor's own clock. The server writes them for its `--time-zone`, so the two agree, and nothing changes as the modules start, whenever the visitor is in that zone.

The preferences are cookies the server reads on the next request: `atrium-intensity` (`calm`, `normal`, `lively`), `atrium-theme` (`light`, `dark`) and `atrium-keys` (`on`, `off`). The server writes each as `data-intensity`, `data-theme` and `data-keys` on `<html>`.

The stylesheet carries `.pb-fill.pct-N` and `.pb-n.pct-N` for every N from 0 to 100, and keeps `hidden` slots hidden. Rules that only need scripts on ask `@media (scripting: enabled)`, so they hold from the first paint; `gl-on`, which the field sets once it draws, is the one class a module adds for the stylesheet.

## Checks

- `mise run gate` holds the bundles to their brotli budgets: `ui` 45 KB, `field` 30 KB, the loader 1 KB.
- `mise run test:browser` drives every module in Chromium against `atrium --demo`, under the server's own CSP: each module's behaviour, every route at 1440 and 390 px in both themes with no console error, uncaught exception or CSP violation, axe with scripts on and off, and the first paint. It runs in Playwright's image through `tools/browser.sh`, which installs the packages `package-lock.json` pins with `npm ci`. With `PROTOTYPE`, the prototype's folder, it also checks that the field draws the prototype's own pixels on the prototype's page.
- `CPUSET=2,3 mise run bench:field` times the field in SwiftShader on two pinned cores, beside the prototype's own field when `PROTOTYPE` is set. A CPU quota throttles the software renderer into slow frames that are not real.
