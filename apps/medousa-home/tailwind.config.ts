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
    extend: {},
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
