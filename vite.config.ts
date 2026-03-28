import path from "node:path"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

const rootDir = __dirname

export default defineConfig({
  base: "./",
  root: "src/renderer",
  define: {
    global: "globalThis",
  },
  optimizeDeps: {
    esbuildOptions: {
      define: {
        global: "globalThis",
      },
    },
  },
  server: {
    port: 5173,
    strictPort: true,
  },
  plugins: [react()],
  build: {
    outDir: path.join(rootDir, "out/renderer"),
    emptyOutDir: true,
  },
  resolve: {
    alias: {
      "@": path.resolve(rootDir, "src/renderer"),
      "@/types": path.resolve(rootDir, "src/renderer/types"),
      "@/utils": path.resolve(rootDir, "src/renderer/utils"),
      "@/store": path.resolve(rootDir, "src/renderer/store"),
      "@/i18n": path.resolve(rootDir, "src/renderer/i18n"),
      "@/contexts": path.resolve(rootDir, "src/renderer/contexts"),
    },
  },
})
