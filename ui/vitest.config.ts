import { fileURLToPath } from 'node:url';

import { defineConfig } from 'vite-plus/test/config';

export default defineConfig({
  root: fileURLToPath(new URL('..', import.meta.url)),
  test: {
    environment: 'node',
    include: ['ui/tests/**/*.test.ts', 'tests/scripts/**/*.test.mjs'],
  },
});
