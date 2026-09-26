import autoprefixer from "autoprefixer";
import tailwindcss from "tailwindcss";
import restoreCascadeLayers from "./postcss.cascade-layers.js";

export default {
  plugins: [tailwindcss(), restoreCascadeLayers(), autoprefixer()],
};
