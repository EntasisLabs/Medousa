import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { buildSync } from "esbuild";
import { buildThemeBootScript } from "./src/lib/theme/themeRegistry";

const homeRoot = dirname(fileURLToPath(import.meta.url));
const host = process.env.TAURI_DEV_HOST;
const devPort = 1420;

/** @param {string | undefined | null} devHost */
function viteOriginFromHost(devHost) {
  if (!devHost) return null;
  const wrapped = devHost.includes(":") ? `[${devHost}]` : devHost;
  return `http://${wrapped}:${devPort}`;
}

function themeCssPlugin() {
  async function writeSelectedThemeSheets() {
    const outfile = join(homeRoot, ".svelte-kit/medousa-theme-css-emit.mjs");
    mkdirSync(join(homeRoot, ".svelte-kit"), { recursive: true });
    buildSync({
      absWorkingDir: homeRoot,
      entryPoints: [join(homeRoot, "themes/emit-theme-css.ts")],
      outfile,
      bundle: true,
      format: "esm",
      platform: "node",
      packages: "external",
      logLevel: "silent",
    });
    const href = `${pathToFileURL(outfile).href}?t=${Date.now()}`;
    const { emitSelectedThemeSheets } = await import(href);
    emitSelectedThemeSheets(homeRoot);
  }
  return {
    name: "medousa-theme-css",
    async buildStart() {
      await writeSelectedThemeSheets();
    },
    async configureServer() {
      await writeSelectedThemeSheets();
    },
  };
}

function themeBootPlugin() {
  const bootScript = buildThemeBootScript();
  const viteOrigin = viteOriginFromHost(host);
  return {
    name: "medousa-theme-boot",
    /** @param {string} html */
    transformIndexHtml(html) {
      const originBoot = viteOrigin
        ? `<script>window.__MEDOUSA_VITE_ORIGIN=${JSON.stringify(viteOrigin)};</script>`
        : "";
      return html.replace(
        "<!-- MEDOUSA_THEME_BOOT -->",
        `${originBoot}<script>${bootScript}</script>`,
      );
    },
  };
}

/** Dev-only: tell the iOS WebView the real Vite origin so it can leave tauri://. */
function mobileDevOriginPlugin() {
  return {
    name: "medousa-mobile-dev-origin",
    /** @param {import('vite').ViteDevServer} server */
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const path = (req.url ?? "").split("?")[0];
        if (path !== "/__medousa_boot_info") {
          next();
          return;
        }
        const envOrigin = viteOriginFromHost(host);
        const headerHost = (req.headers.host || "").split(",")[0].trim();
        const fallbackOrigin = headerHost ? `http://${headerHost}` : null;
        res.statusCode = 200;
        res.setHeader("content-type", "application/json");
        res.setHeader("cache-control", "no-store");
        res.end(
          JSON.stringify({
            viteOrigin: envOrigin || fallbackOrigin,
          }),
        );
      });
    },
  };
}

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [themeCssPlugin(), mobileDevOriginPlugin(), sveltekit(), themeBootPlugin()],
  build: {
    manifest: true,
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: devPort,
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
