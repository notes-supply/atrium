// Runs inside mcr.microsoft.com/playwright:v1.64.1-noble against `atrium --demo`, through
// tools/browser.sh (mise run test:browser). WebGL renders on the CPU through SwiftShader.
// With PROTOTYPE set, a second server shows the prototype's own pages for the field's
// pixel check and its bench.
const prototype = process.env.PROTOTYPE_PORT;

const SWIFTSHADER = ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'];

// The field, every route and axe run with WebGL, the way a reader with a GPU sees the pages.
const WITH_FIELD = ['field.spec.mjs', 'pages.spec.mjs', 'axe.spec.mjs'];

export default {
  testDir: '.',
  testMatch: '*.spec.mjs',
  outputDir: process.env.OUT ?? '../../../../target/browser/results',
  workers: 2,
  reporter: [['list']],
  use: {
    baseURL: process.env.ATRIUM_URL,
    viewport: { width: 1440, height: 900 },
    launchOptions: { args: SWIFTSHADER },
  },
  projects: [
    { name: 'field', testMatch: WITH_FIELD },
    // Rendering the field in software takes the CPU that a slow machine's page needs for
    // everything else, so the other modules run without WebGL, with the CSS lattice a
    // browser without it gets.
    { name: 'modules', testIgnore: WITH_FIELD, use: { launchOptions: { args: ['--disable-webgl'] } } },
  ],
  webServer: prototype
    ? { command: `node serve.mjs ${prototype}`, url: `http://127.0.0.1:${prototype}/prototype/index.html`, reuseExistingServer: false }
    : undefined,
};
