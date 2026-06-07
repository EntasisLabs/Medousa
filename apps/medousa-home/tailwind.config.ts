import { join } from "node:path";
import type { Config } from "tailwindcss";
import forms from "@tailwindcss/forms";
import { skeleton } from "@skeletonlabs/tw-plugin";
import { medousaTheme } from "./medousa-theme";

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
        custom: [medousaTheme],
      },
    }),
  ],
} satisfies Config;
