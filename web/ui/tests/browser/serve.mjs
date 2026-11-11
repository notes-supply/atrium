// Serves the prototype for the field's pixel check and its bench, under the server's own
// CSP, read from crates/atrium/src/middleware.rs so the two cannot drift: its pages as they
// are under /prototype/, and under /wasm/ with its scripts swapped for the loader and the
// built bundles, as the server writes them.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';

const repo = new URL('../../../../', import.meta.url).pathname;
const source = await readFile(join(repo, 'crates/atrium/src/middleware.rs'), 'utf8');
const csp = source.match(/pub const CSP: &str = "([^;]*;[\s\S]*?)";/)[1].replace(/\\\n\s*/g, '');
const TYPES = { '.html': 'text/html; charset=utf-8', '.css': 'text/css', '.js': 'text/javascript', '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.json': 'application/json' };
const LOADER = '<script type="module" src="/web/loader.js" data-ui="/web/ui.js" data-ui-wasm="/web/ui_bg.wasm" data-field="/web/field.js" data-field-wasm="/web/field_bg.wasm"></script>';

async function body(path) {
  if (path === 'web/loader.js') return readFile(join(repo, path));
  if (path.startsWith('web/')) return readFile(join(repo, 'target', path));
  const [side, ...rest] = path.split('/');
  const file = await readFile(join(process.env.PROTOTYPE, rest.join('/')));
  if (side === 'prototype' || !path.endsWith('.html')) return file;
  return file.toString()
    .replace('<script src="assets/boot.js"></script>', '')
    .replace('<script type="module" src="assets/load.js"></script>', LOADER);
}

createServer(async (request, response) => {
  const path = normalize(decodeURIComponent(new URL(request.url, 'http://x').pathname)).replace(/^\/+/, '');
  try {
    const bytes = await body(path);
    response.writeHead(200, { 'content-security-policy': csp, 'content-type': TYPES[extname(path)] || 'application/octet-stream' });
    response.end(bytes);
  } catch {
    response.writeHead(404, { 'content-security-policy': csp });
    response.end();
  }
}).listen(+process.argv[2], '127.0.0.1');
