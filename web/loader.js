// The only script on a page. Its own element names the two bundles in data-ui, data-ui-wasm,
// data-field and data-field-wasm. It starts the field bundle when the page has the field,
// and the ui bundle when it names any other module. Module scripts run after parsing.
const bundles = document.querySelector("script[data-ui]").dataset;
const names = new Set();
for (const element of document.querySelectorAll("[data-module]")) {
  for (const name of element.dataset.module.split(/\s+/)) names.add(name);
}
const start = (glue, wasm) => import(glue).then((m) => m.default({ module_or_path: wasm }));
if (names.delete("field")) start(bundles.field, bundles.fieldWasm);
names.delete("");
if (names.size) start(bundles.ui, bundles.uiWasm);
