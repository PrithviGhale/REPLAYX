// Dev-only static build of the mocked UI preview; the app build uses vite.config.ts.
// Build: npx vite build --config dev/vite.preview.ts   Serve: npx vite preview --config dev/vite.preview.ts
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
export default defineConfig({
  plugins: [react()],
  build: { outDir: 'dist-preview', emptyOutDir: true, target: 'es2022', rollupOptions: { input: 'dev/preview.html' } },
  preview: { port: 1430, strictPort: true, host: '127.0.0.1' },
});
