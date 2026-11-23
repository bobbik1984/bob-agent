import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
const scope = createHash('sha256').update(process.cwd()).digest('hex').slice(0, 16);
export default defineConfig({
  plugins: [vue()], clearScreen: false,
  cacheDir: process.env.BOB_VITE_CACHE_DIR || join(tmpdir(), 'bob-app-vite-cache', scope),
  server: { strictPort: true },
  build: { outDir: process.env.BOB_FRONTEND_DIST || 'dist', emptyOutDir: true },
});
