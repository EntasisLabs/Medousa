import { join } from "node:path";
import type { Config } from "tailwindcss";
import forms from "@tailwindcss/forms";
import { skeleton } from "@skeletonlabs/tw-plugin";
import {
  cupertinoDarkTheme,
  cupertinoLightTheme,
  graphiteDarkTheme,
  graphiteLightTheme,
  midnightDarkTheme,
  midnightLightTheme,
} from "./themes/apple-themes";
import { blackLilyTheme } from "./black-lily-theme";
import { blackLilyLightTheme, medousaLightTheme } from "./themes/light-themes";
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
        custom: [
          medousaTheme,
          medousaLightTheme,
          blackLilyTheme,
          blackLilyLightTheme,
          cupertinoLightTheme,
          cupertinoDarkTheme,
          graphiteLightTheme,
          graphiteDarkTheme,
          midnightLightTheme,
          midnightDarkTheme,
        ],
      },
    }),
  ],
} satisfies Config;
