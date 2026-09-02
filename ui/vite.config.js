import { defineConfig } from "vite";

// 端口需与 src-tauri/tauri.conf.json 的 build.devUrl (http://127.0.0.1:1420) 保持一致
export default defineConfig({
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2021",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
