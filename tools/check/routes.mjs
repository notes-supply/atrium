// Every server route with its prototype page, addressed as `atrium --demo` serves them. The
// ids are the demo's (crates/atrium-wiki/src/demo.rs); a test there keeps them the same.
// `away` routes are served by a second demo, `atrium --demo away`, on the next port.
const RESTIC = '/wiki/page/06746ebaa613d598';
const V20 = 'ba0d8d9add8250f0c8cf49841c43ad453cf0f1a7';
const V22 = 'ba0d8f9add825456c8cf47841c43a9df3cf0efa7';
const V23 = 'ba0d909add825609c8cf46841c43a82c3cf0f0a7';

export const ROUTES = [
  { name: 'index', file: 'index.html', path: '/' },
  { name: 'wiki', file: 'wiki.html', path: '/wiki?sort=updated&order=desc' },
  { name: 'page', file: 'page.html', path: RESTIC },
  { name: 'history', file: 'history.html', path: `${RESTIC}/history` },
  { name: 'version', file: 'version.html', path: `${RESTIC}/at/${V20}` },
  { name: 'diff', file: 'diff.html', path: `${RESTIC}/diff?from=${V22}&to=${V23}` },
  { name: 'changes', file: 'changes.html', path: '/wiki/changes' },
  { name: 'search', file: 'search.html', path: '/wiki/search?q=restic' },
  { name: 'machines', file: 'machines.html', path: '/wiki/machines' },
  { name: 'away', file: 'away.html', path: '/', away: true },
  { name: '404', file: '404.html', path: '/wiki/homelab/printer.md' },
];
