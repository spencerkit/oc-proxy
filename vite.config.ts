import path from "node:path"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"
import electron from "vite-plugin-electron"

const rootDir = __dirname

export default defineConfig({
  base: "./",
  root: "src/renderer",
  server: {
    port: 5173,
  },
  plugins: [
    react(),
    electron([
      {
        entry: path.join(rootDir, "src/main/main.ts"),
        onstart(options) {
          options.startup()
        },
        vite: {
          build: {
            outDir: path.join(rootDir, "out/main"),
            rollupOptions: {
              external: ["electron"],
            },
          },
        },
      },
      {
        entry: path.join(rootDir, "src/main/preload.ts"),
        onstart(options) {
          options.reload()
        },
        vite: {
          build: {
            outDir: path.join(rootDir, "out/preload"),
            rollupOptions: {
              external: ["electron"],
            },
          },
        },
      },
      {
        entry: path.join(rootDir, "src/main/proxyWorker.ts"),
        vite: {
          build: {
            outDir: path.join(rootDir, "out/main"),
            rollupOptions: {
              external: ["electron"],
            },
          },
        },
      },
      {
        entry: path.join(rootDir, "src/main/proxyRuntimeClient.ts"),
        vite: {
          build: {
            outDir: path.join(rootDir, "out/main"),
            rollupOptions: {
              external: ["electron"],
            },
          },
        },
      },
      {
        entry: path.join(rootDir, "src/main/logStore.ts"),
        vite: {
          build: {
            outDir: path.join(rootDir, "out/main"),
            rollupOptions: {
              external: ["electron"],
            },
          },
        },
      },
    ]),
  ],
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
