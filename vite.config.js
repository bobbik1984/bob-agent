import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';

const viteCacheDir = process.env.DEV_CACHE_ROOT
  ? resolve(process.env.DEV_CACHE_ROOT, 'node', 'vite-cache', 'bob.agent')
  : resolve(tmpdir(), 'bob.agent', 'vite-cache');

export default defineConfig({
  plugins: [vue()],
  root: '.',
  cacheDir: viteCacheDir,
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        design: resolve(__dirname, 'design_system.html')
      }
    },
    outDir: 'dist',
    emptyOutDir: true,
    target: 'esnext',
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**', '**/ignore_sync/**'],
    },
  },
  optimizeDeps: {
    entries: ['index.html'],
  },
});
