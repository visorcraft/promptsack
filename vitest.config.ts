import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/**/*.test.ts'],
    exclude: ['node_modules/**', 'dist/**', 'src-tauri/**', 'ui/**'],
    testTimeout: 30_000,
  },
});
