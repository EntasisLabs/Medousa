import { join } from "node:path";
import type { Config } from "tailwindcss";
import forms from "@tailwindcss/forms";
import { skeleton } from "@skeletonlabs/tw-plugin";

export default {
  darkMode: "class",
  content: [
    "./src/**/*.{html,js,svelte,ts}",
    join(
      require.resolve("@skeletonlabs/skeleton"),
      "../**/*.{html,js,svelte,ts}",
    ),
  ],
  theme: {
    extend: {
      colors: {
        "content-primary": "rgb(var(--theme-text) / <alpha-value>)",
        "content-secondary": "rgb(var(--theme-text-secondary) / <alpha-value>)",
        "content-tertiary": "rgb(var(--theme-text-tertiary) / <alpha-value>)",
        "content-quiet": "rgb(var(--theme-text-quiet) / <alpha-value>)",
        "content-faint": "rgb(var(--theme-text-faint) / <alpha-value>)",
        "content-disabled": "rgb(var(--theme-text-disabled) / <alpha-value>)",
        "content-link": "rgb(var(--theme-link) / <alpha-value>)",
        "content-error": "rgb(var(--theme-error) / <alpha-value>)",
        "content-success": "rgb(var(--theme-success) / <alpha-value>)",
        "content-warning": "rgb(var(--theme-warning) / <alpha-value>)",
      },
      fontSize: {
        /* Dense UI chrome — floor 11px. Prefer these over text-[9px]/[11px]. */
        "chrome-xs": [
          "calc(var(--chrome-xs, 11px) * var(--content-zoom, 1))",
          { lineHeight: "1.25" },
        ],
        "chrome-sm": [
          "calc(var(--chrome-sm, 12px) * var(--content-zoom, 1))",
          { lineHeight: "1.3" },
        ],
        "chrome-md": [
          "calc(var(--chrome-md, 13px) * var(--content-zoom, 1))",
          { lineHeight: "1.35" },
        ],
      },
    },
  },
  plugins: [
    forms,
    skeleton({
      themes: {
        custom: [],
      },
    }),
  ],
} satisfies Config;
