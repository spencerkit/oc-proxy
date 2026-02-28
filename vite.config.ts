import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig({
  base: './',
  root: 'src/renderer',
  plugins: [react()],
  build: {
    outDir: '../../out/renderer',
    emptyOutDir: true,
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'src/renderer'),
      '@/types': path.resolve(__dirname, 'src/renderer/types'),
      '@/utils': path.resolve(__dirname, 'src/renderer/utils'),
      '@/store': path.resolve(__dirname, 'src/renderer/store'),
      '@/i18n': path.resolve(__dirname, 'src/renderer/i18n'),
    },
  },
});
