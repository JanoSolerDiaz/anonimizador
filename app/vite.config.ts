import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Opciones pensadas para `tauri dev` y `tauri build`.
export default defineConfig({
  plugins: [svelte()],
  // No ocultar los errores de Rust en la consola.
  clearScreen: false,
  server: {
    // Tauri espera un puerto fijo (`devUrl` en tauri.conf.json).
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    // WebView2 y WKWebView (macOS 12+) admiten ES2021.
    target: "es2021",
  },
});
