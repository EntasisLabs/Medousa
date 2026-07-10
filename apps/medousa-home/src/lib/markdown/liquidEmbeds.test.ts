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

  it("turns a callout fence into a placeholder", () => {
    const src = ["```callout", "tone: warn", "title: Heads up", "body: Deprecated path", "```"].join(
      "\n",
    );
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="callout"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ body: string; tone?: string; title?: string }>(match![1]);
    expect(props?.body).toBe("Deprecated path");
    expect(props?.tone).toBe("warn");
    expect(props?.title).toBe("Heads up");
  });

  it("turns a section fence with --- body into title + prose", () => {
    const src = [
      "```section",
      "title: Model family",
      "subtitle: Availability",
      "---",
      "Sol is the flagship.",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="section"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ title: string; subtitle?: string; body?: string }>(
      match![1],
    );
    expect(props?.title).toBe("Model family");
    expect(props?.subtitle).toBe("Availability");
    expect(props?.body).toBe("Sol is the flagship.");
  });

  it("turns chips fence into chip payloads", () => {
    const src = [
      "```chips",
      "- Ultra | tone: accent | value: ultra",
      "Fast | tone: default",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="chips"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ chips: { label: string; tone?: string; value?: string }[] }>(
      match![1],
    );
    expect(props?.chips).toHaveLength(2);
    expect(props?.chips[0].label).toBe("Ultra");
    expect(props?.chips[0].tone).toBe("accent");
    expect(props?.chips[0].value).toBe("ultra");
  });

  it("turns a media fence into src payload", () => {
    const src = [
      "```media",
      "src: https://example.com/a.png",
      "alt: Diagram",
      "caption: Source",
      "ratio: 16/9",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="media"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ src: string; alt?: string; caption?: string }>(match![1]);
    expect(props?.src).toBe("https://example.com/a.png");
    expect(props?.alt).toBe("Diagram");
  });

  it("rejects callout without body and media without src", () => {
    expect(preprocessLiquidEmbeds("```callout\ntitle: Only\n```")).toContain("```callout");
    expect(preprocessLiquidEmbeds("```media\nalt: No src\n```")).toContain("```media");
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
