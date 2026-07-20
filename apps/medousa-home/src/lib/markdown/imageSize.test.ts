import { describe, expect, it } from "vitest";

import {
  imageSizeStyle,
  parseImageSizeToken,
  splitImageAltSize,
  splitImageHrefSize,
} from "./imageSize";
import { preprocessWikiImageEmbeds } from "./preprocess";
import { renderMarkdown } from "./render";

describe("imageSize", () => {
  it("parses width and WxH tokens", () => {
    expect(parseImageSizeToken("400")).toEqual({ width: 400 });
    expect(parseImageSizeToken("400x240")).toEqual({ width: 400, height: 240 });
    expect(parseImageSizeToken("caption")).toBeNull();
  });

  it("splits size from href and alt", () => {
    expect(splitImageHrefSize("./shot.png|320")).toEqual({
      href: "./shot.png",
      size: { width: 320 },
    });
    expect(splitImageAltSize("Diagram|200x100")).toEqual({
      alt: "Diagram",
      size: { width: 200, height: 100 },
    });
  });

  it("builds inline size styles", () => {
    expect(imageSizeStyle({ width: 400 })).toBe(
      "width:400px;height:auto;max-width:100%",
    );
    expect(imageSizeStyle({ width: 400, height: 240 })).toBe(
      "width:400px;height:240px;max-width:100%",
    );
  });
});

describe("Obsidian image sizing", () => {
  it("applies |size from markdown image href", () => {
    const html = renderMarkdown("![](./shot.png|400)", {
      resolveLocalImages: true,
    });
    expect(html).toContain('data-local-image="./shot.png"');
    expect(html).toContain("width:400px");
    expect(html).toContain("markdown-image--sized");
    expect(html).not.toContain("|400");
  });

  it("applies |size from alt text", () => {
    const html = renderMarkdown("![Hero|280](https://example.com/a.png)");
    expect(html).toContain('alt="Hero"');
    expect(html).toContain("width:280px");
  });

  it("rewrites wiki image embeds with size", () => {
    expect(preprocessWikiImageEmbeds("![[assets/logo.png|180]]")).toBe(
      "![](assets/logo.png|180)",
    );
    const html = renderMarkdown("![[assets/logo.png|180x90]]", {
      resolveLocalImages: true,
    });
    expect(html).toContain('data-local-image="assets/logo.png"');
    expect(html).toContain("width:180px");
    expect(html).toContain("height:90px");
  });

  it("leaves note embeds untouched", () => {
    expect(preprocessWikiImageEmbeds("![[Other note|Alias]]")).toBe(
      "![[Other note|Alias]]",
    );
  });
});
