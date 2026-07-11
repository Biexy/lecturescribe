import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
  build: {
    target: "es2022",
  },
  optimizeDeps: {
    include: ["react", "react/jsx-runtime", "react-reconciler", "scheduler"],
  },
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
});
