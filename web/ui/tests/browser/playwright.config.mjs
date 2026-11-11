// Runs inside mcr.microsoft.com/playwright:v1.64.1-noble (mise run test:browser). WebGL
// renders on the CPU through SwiftShader.
export default {
  testDir: '.',
  testMatch: '*.spec.mjs',
  outputDir: process.env.OUT ?? '/tmp/results',
  workers: 2,
  reporter: [['list']],
  use: {
    baseURL: 'http://127.0.0.1:4173/',
    viewport: { width: 1440, height: 900 },
    launchOptions: { args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] },
  },
  webServer: { command: 'node serve.mjs 4173', url: 'http://127.0.0.1:4173/index.html', reuseExistingServer: false },
};
