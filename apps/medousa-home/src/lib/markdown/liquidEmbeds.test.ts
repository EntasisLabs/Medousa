import { describe, expect, it } from "vitest";
import {
  decodeLiquidProps,
  preprocessLiquidEmbeds,
  LIQUID_ICON_ALLOWLIST,
} from "./liquidEmbeds";
import { preprocessMarkdown } from "./preprocess";
import { renderMarkdown } from "./render";

describe("preprocessLiquidEmbeds", () => {
  it("turns a card fence into a placeholder with decodable props", () => {
    const src = ["```card", "title: Sol", "subtitle: Flagship", "body: Hard reasoning", "emoji: 🧠", "```"].join(
      "\n",
    );
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="card"');
    expect(out).toContain("data-liquid-props=");
    expect(out).not.toContain("```card");

    const match = out.match(/data-liquid-props="([^"]+)"/);
    expect(match).toBeTruthy();
    const props = decodeLiquidProps<{ title: string; subtitle?: string }>(match![1]);
    expect(props?.title).toBe("Sol");
    expect(props?.subtitle).toBe("Flagship");
  });

  it("turns a carousel fence into items payload", () => {
    const src = [
      "```carousel",
      "title: Sol | body: Flagship | emoji: 🧠",
      "title: Terra | body: Mid | emoji: ⚖️",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="carousel"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ items: { title: string }[] }>(match![1]);
    expect(props?.items).toHaveLength(2);
    expect(props?.items[0].title).toBe("Sol");
  });

  it("accepts list-marker carousel lines models often emit", () => {
    const src = [
      "```carousel",
      "- title: The Raven | body: grief | emoji: 🐦",
      "* title: Tell-Tale Heart | body: murder | emoji: 🫀",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="carousel"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ items: { title: string }[] }>(match![1]);
    expect(props?.items).toHaveLength(2);
    expect(props?.items[0].title).toBe("The Raven");
  });

  it("turns an actions fence into action rows", () => {
    const src = [
      "```actions",
      "Which one is best for coding? | coding",
      "Compare Sol vs Terra | compare",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="actions"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ actions: { label: string; intent?: string }[] }>(match![1]);
    expect(props?.actions).toHaveLength(2);
    expect(props?.actions[0].intent).toBe("coding");
  });

  it("strips Label: chrome and list markers from actions", () => {
    const src = [
      "```actions",
      '- Label: 📖 Read "The Raven"',
      "- Label: ❓ How did he die? | how-did-he-die",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="actions"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      actions: { label: string; intent?: string; emoji?: string }[];
    }>(match![1]);
    expect(props?.actions).toHaveLength(2);
    expect(props?.actions[0].label).toBe('Read "The Raven"');
    expect(props?.actions[0].emoji).toBe("📖");
    expect(props?.actions[1].intent).toBe("how-did-he-die");
  });

  it("leaves unknown fence langs alone", () => {
    const src = "```python\nprint(1)\n```";
    expect(preprocessLiquidEmbeds(src)).toBe(src);
  });

  it("emits allowlisted icons and drops unknown ones", () => {
    const out = preprocessLiquidEmbeds("{{icon:sparkles}} hello {{icon:not-a-real-icon}}");
    expect(out).toContain('data-liquid-icon="sparkles"');
    expect(out).not.toContain("not-a-real-icon");
    expect(LIQUID_ICON_ALLOWLIST.has("sparkles")).toBe(true);
  });

  it("does not rewrite icons inside code fences", () => {
    const src = "```\n{{icon:sparkles}}\n```";
    expect(preprocessLiquidEmbeds(src)).toContain("{{icon:sparkles}}");
  });
});

describe("renderMarkdown + liquid embeds", () => {
  it("preserves liquid placeholders through sanitize", () => {
    const md = ["Intro", "", "```card", "title: Sol", "body: Flagship", "```", ""].join("\n");
    const html = renderMarkdown(preprocessMarkdown(md));
    // preprocessMarkdown already runs liquid embeds; renderMarkdown runs it again — idempotent enough
    expect(html).toContain("data-liquid-embed");
    expect(html).toContain("data-liquid-props");
  });
});
