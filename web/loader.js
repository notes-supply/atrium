// The only script on a page. Each element with a data-module attribute names its module's
// glue and .wasm in data-glue and data-wasm; this imports each module once and starts it.
// Module scripts run deferred, after the document is parsed.
const started = new Set();
for (const element of document.querySelectorAll("[data-module]")) {
  const { glue, wasm } = element.dataset;
  if (glue && wasm && !started.has(glue)) {
    started.add(glue);
    import(glue).then((module) => module.default({ module_or_path: wasm }));
  }
}
