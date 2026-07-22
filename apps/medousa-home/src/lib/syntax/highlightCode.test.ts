/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import {
  highlightToHtml,
  resolveHighlightLang,
} from "./highlightCode";

describe("resolveHighlightLang", () => {
  it("maps common aliases to registered languages", () => {
    expect(resolveHighlightLang("cs")).toBe("csharp");
    expect(resolveHighlightLang("c#")).toBe("csharp");
    expect(resolveHighlightLang("c++")).toBe("cpp");
    expect(resolveHighlightLang("cxx")).toBe("cpp");
    expect(resolveHighlightLang("go")).toBe("go");
    expect(resolveHighlightLang("sql")).toBe("sql");
  });
});

describe("highlightToHtml", () => {
  it("highlights csharp and c++ with hljs spans", async () => {
    const cs = await highlightToHtml("class Foo { }", "csharp");
    expect(cs).toContain("hljs-");
    expect(cs).not.toBe("class Foo { }");

    const cpp = await highlightToHtml("int main() { return 0; }", "c++");
    expect(cpp).toContain("hljs-");
  });

  it("escapes unknown languages without throwing", async () => {
    await expect(
      highlightToHtml("<script>x</script>", "not-a-real-lang-xyz"),
    ).resolves.toBe("&lt;script&gt;x&lt;/script&gt;");
  });
});
