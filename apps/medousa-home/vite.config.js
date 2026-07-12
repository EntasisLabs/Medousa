import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { buildThemeBootScript } from "./src/lib/theme/themeRegistry";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

function themeBootPlugin() {
  const bootScript = buildThemeBootScript();
  return {
    name: "medousa-theme-boot",
    /** @param {string} html */
    transformIndexHtml(html) {
      return html.replace(
        "<!-- MEDOUSA_THEME_BOOT -->",
        `<script>${bootScript}</script>`,
      );
    },
  };
}

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [sveltekit(), themeBootPlugin()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
