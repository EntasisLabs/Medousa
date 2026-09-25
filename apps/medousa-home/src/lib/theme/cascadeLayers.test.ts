import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import postcss from "postcss";
import { describe, expect, it } from "vitest";
import config from "../../../postcss.config.js";

const homeRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");

describe("cascade layers", () => {
  it("wraps Tailwind preflight in @layer base so feature chrome wins", async () => {
    const css = readFileSync(join(homeRoot, "src/app.postcss"), "utf8");
    const result = await postcss(config.plugins).process(css, {
      from: join(homeRoot, "src/app.postcss"),
    });
    const compiled = result.css;
    expect(compiled.startsWith("@layer base, components, utilities, features;")).toBe(true);

    const buttonPadding = compiled.indexOf("padding: 0; /* 3 */");
    expect(buttonPadding).toBeGreaterThan(0);
    const layerBeforeButton = compiled.slice(0, buttonPadding).match(/@layer\s+([^{]+)\{/g);
    expect(layerBeforeButton?.at(-1)).toContain("base");

    const borderReset = compiled.indexOf("border-width: 0; /* 2 */");
    expect(borderReset).toBeGreaterThan(0);
    const layerBeforeBorder = compiled.slice(0, borderReset).match(/@layer\s+([^{]+)\{/g);
    expect(layerBeforeBorder?.at(-1)).toContain("base");
  });
});
