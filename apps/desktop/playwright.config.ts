import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests/browser',
  use: { baseURL: 'http://127.0.0.1:1421', viewport: {width: 1672, height: 941} },
  webServer: {command: 'npm run build && npm exec vite preview -- --host 127.0.0.1 --port 1421', url: 'http://127.0.0.1:1421', reuseExistingServer: false, timeout: 180000},
});
