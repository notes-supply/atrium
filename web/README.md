# Browser modules

Every behaviour a page has beyond plain HTML, in Rust compiled to Wasm. Pages work without them.

- `loader.js` is the only script a page loads.
- `ui/` is one bundle with every small module.
- `field/` is the WebGL2 background and its `?perf` readout.

`mise run build:wasm` builds both bundles into `target/web`, where the server embeds them.

## What a page writes

The loader's own element names the bundles:

```html
<script type="module" src="/assets/loader.….js"
  data-ui="/assets/ui.….js" data-ui-wasm="/assets/ui_bg.….wasm"
  data-field="/assets/field.….js" data-field-wasm="/assets/field_bg.….wasm"></script>
```

It starts the field bundle when the page has `data-module="field"`, and the ui bundle when the page names any other module. It also puts the `js` class on `<html>`, which the stylesheet expects.

Each module mounts on the elements whose `data-module` names it. One element may name several, as in `data-module="keys light carry"`.

| Module | Element | What it needs from the server |
| --- | --- | --- |
| `field` | `canvas.field` | `data-surface` on surfaces, `data-want` (1 or 2) on what wants the reader, `data-reading` on the article, the map's `.dock` and `.dock-card` with `data-m` and `data-want`. |
| `palette` | the Jump link, and home's `.cmd` | `data-pages` on either: a URL answering `[{ "t", "path", "m", "href" }]`. Sections come from `.index a[data-key]`. |
| `keys` | `<body>` | `.index a[data-key]`, `main [data-row]`, `#q` or `.cmd input`. |
| `prefs` | each `.prefs` | `.swatch[data-intensity]`, `.theme-btn`, and a `.keys-btn` for single-letter keys. |
| `swap` | the search form | `[data-swap-target]` and `[data-count]` in the page the form's action answers. |
| `preview` | the article | a JSON block `<script type="application/json" id="previews">` keyed by each link's `data-preview`: `{ "path", "title", "text", "meta" }`. |
| `toc` | `details.toc` | links to `#id` headings. |
| `continue` | the article, with `data-page`; home's Continue reading tile | the tile, empty and `hidden`, with only its header; on the article, `.resume-note` `hidden` with an empty `<span>`. |
| `diff` | the diff section | `[data-only-changes]`. |
| `carry` | `<body>` | `a[data-carry]` on titles, `.doc-title` on a page's heading. |
| `clock` | `time.clock` | nothing more. |
| `greet` | the greeting's `<span>` | nothing more. |
| `light` | `<body>` | `data-hour` on `<html>`. |

The preferences are cookies the server reads on the next request: `atrium-intensity` (`calm`, `normal`, `lively`), `atrium-theme` (`light`, `dark`) and `atrium-keys` (`on`, `off`). The server writes each as `data-intensity`, `data-theme` and `data-keys` on `<html>`.

The stylesheet must carry `.pb-fill.pct-N` and `.pb-n.pct-N` for N from 0 to 100 in steps of 5, and keep `hidden` tiles and notes hidden. The fixtures' `assets/drawn.css` shows both.

## Checks

- `mise run gate` holds the bundles to their brotli budgets: `ui` 45 KB, `field` 30 KB, the loader 1 KB.
- `mise run test:browser` drives every module in Chromium in Playwright's image, against the prototype's pages in `ui/tests/browser/fixtures`, under the server's CSP. It needs `ATRIUM_PLAYWRIGHT_MODULES`, a `node_modules` holding `@playwright/test` 1.64.1. With `ATRIUM_PROTOTYPE`, the prototype's folder, it also checks that the field draws the prototype's pixels.
- `CPUSET=2,3 mise run test:browser -- -c bench.config.mjs` times the field in SwiftShader on two pinned cores. A CPU quota throttles the software renderer into slow frames that are not real.
- `ui/tests/browser/fixtures.sh <prototype folder>` rebuilds the fixtures from the prototype.
