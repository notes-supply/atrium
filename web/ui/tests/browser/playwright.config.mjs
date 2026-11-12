// Runs inside mcr.microsoft.com/playwright:v1.64.1-noble against `atrium --demo`, through
// tools/browser.sh (mise run test:browser). WebGL renders on the CPU through SwiftShader.
// With PROTOTYPE set, a second server shows the prototype's own pages for the field's
// pixel check and its bench.
const prototype = process.env.PROTOTYPE_PORT;

const SWIFTSHADER = ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'];

// The field, every route and axe run with WebGL, the way a reader with a GPU sees the pages.
const WITH_FIELD = ['field.spec.mjs', 'pages.spec.mjs', 'axe.spec.mjs'];
// In SwiftShader the field's GPU is the CPU, so a cheap field keeps up only while no other
// browser takes that CPU: the test that it stays at full quality runs alone, after the rest.
// `--no-deps` runs it, or the field's spec, without the rest.
const ALONE = /a fast field stays at full quality/;

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
    // CI uploads target/browser when the job fails.
    trace: process.env.CI ? 'retain-on-failure' : 'off',
  },
  projects: [
    // On GitHub's runner a page rendering in SwiftShader can stall for more than 5 s, as
    // one did after a resize at twice the pixel density.
    { name: 'field', testMatch: WITH_FIELD, grepInvert: ALONE, expect: { timeout: process.env.CI ? 15_000 : 5_000 } },
    // Rendering the field in software takes the CPU that a slow machine's page needs for
    // everything else. The field steps down to its still frame when it cannot keep up, but
    // each page starts it at full quality, and on one core, about as slow as GitHub's
    // runner, the modules still missed their waits with it. So they run without WebGL, with
    // the CSS lattice a browser without it gets.
    { name: 'modules', testIgnore: WITH_FIELD, use: { launchOptions: { args: ['--disable-webgl'] } } },
    { name: 'alone', testMatch: 'field.spec.mjs', grep: ALONE, dependencies: ['field', 'modules'] },
  ],
  webServer: prototype
    ? { command: `node serve.mjs ${prototype}`, url: `http://127.0.0.1:${prototype}/prototype/index.html`, reuseExistingServer: false }
    : undefined,
};
