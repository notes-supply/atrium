// Runs inside mcr.microsoft.com/playwright:v1.64.1-noble against `atrium --demo`, through
// tools/browser.sh (mise run test:browser). WebGL renders on the CPU through SwiftShader.
// With PROTOTYPE set, a second server shows the prototype's own pages for the field's
// pixel check and its bench.
const prototype = process.env.PROTOTYPE_PORT;

export default {
  testDir: '.',
  testMatch: '*.spec.mjs',
  outputDir: process.env.OUT ?? '../../../../target/browser/results',
  workers: 2,
  reporter: [['list']],
  use: {
    baseURL: process.env.ATRIUM_URL,
    viewport: { width: 1440, height: 900 },
    launchOptions: { args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] },
  },
  webServer: prototype
    ? { command: `node serve.mjs ${prototype}`, url: `http://127.0.0.1:${prototype}/prototype/index.html`, reuseExistingServer: false }
    : undefined,
};
