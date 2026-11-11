// Serves the fixtures, the loader and the built bundles, with the server's own CSP, read
// from crates/atrium/src/middleware.rs so the two cannot drift.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';

const repo = new URL('../../../../', import.meta.url).pathname;
const source = await readFile(join(repo, 'crates/atrium/src/middleware.rs'), 'utf8');
const csp = source.match(/pub const CSP: &str = "([^;]*;[\s\S]*?)";/)[1].replace(/\\\n\s*/g, '');
const TYPES = { '.html': 'text/html; charset=utf-8', '.css': 'text/css', '.js': 'text/javascript', '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.json': 'application/json' };

function file(path) {
  if (path === 'web/loader.js') return join(repo, path);
  if (path.startsWith('web/')) return join(repo, 'target', path);
  return join(repo, 'web/ui/tests/browser/fixtures', path);
}

createServer(async (request, response) => {
  const path = normalize(decodeURIComponent(new URL(request.url, 'http://x').pathname)).replace(/^\/+/, '') || 'index.html';
  const headers = { 'content-security-policy': csp };
  try {
    const body = await readFile(file(path));
    response.writeHead(200, { ...headers, 'content-type': TYPES[extname(path)] || 'application/octet-stream' });
    response.end(body);
  } catch {
    response.writeHead(404, { ...headers, 'content-type': TYPES['.html'] });
    response.end(await readFile(file('404.html')));
  }
}).listen(+process.argv[2], '127.0.0.1');
