import { join } from "node:path";
import type { Config } from "tailwindcss";
import forms from "@tailwindcss/forms";
import { skeleton } from "@skeletonlabs/tw-plugin";
import { allThemes } from "./themes/theme-catalog";

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
    },
  },
  plugins: [
    forms,
    skeleton({
      themes: {
        custom: allThemes,
      },
    }),
  ],
} satisfies Config;
