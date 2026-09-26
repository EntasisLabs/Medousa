import postcss from "postcss";

/**
 * Tailwind tags generated rules with `raws.tailwind.layer` but emits them
 * unlayered. Unlayered preflight (`button { padding: 0 }`, `* { border-width: 0 }`)
 * then beats every `@layer features` sheet, which is why dock popovers and the
 * new-note composer lose their borders, type, and padding.
 *
 * Group those tagged rules back into the cascade declared in app.postcss.
 */
const PUBLIC_LAYER = {
  defaults: "base",
  base: "base",
  components: "components",
  utilities: "utilities",
  user: "utilities",
};

function publicLayer(node) {
  const raw = node.raws?.tailwind;
  if (!raw?.layer) return null;
  if (raw.layer === "variants") {
    return PUBLIC_LAYER[raw.parentLayer] ?? "utilities";
  }
  return PUBLIC_LAYER[raw.layer] ?? null;
}

export default function restoreCascadeLayers() {
  return {
    postcssPlugin: "medousa-cascade-layers",
    OnceExit(root) {
      let bucket = null;
      for (const node of [...root.nodes]) {
        const layer = publicLayer(node);
        if (!layer) {
          bucket = null;
          continue;
        }
        if (!bucket || bucket.params !== layer) {
          bucket = postcss.atRule({ name: "layer", params: layer });
          node.before(bucket);
        }
        bucket.append(node);
      }
    },
  };
}

restoreCascadeLayers.postcss = true;
