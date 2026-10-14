import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

// Tauri expects a fixed dev port and serves the built files from ../ui/dist.
export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    // WebView2 (Chromium), WKWebView (Safari 15+) and WebKitGTK.
    target: ["es2022", "chrome110", "safari15"],
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  test: {
    environment: "jsdom",
  },
});
