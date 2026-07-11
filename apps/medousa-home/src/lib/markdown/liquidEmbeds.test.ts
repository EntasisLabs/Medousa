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

  it("turns a cite fence into quote/title/url/source payload", () => {
    const src = [
      "```cite",
      "title: Example Paper",
      "url: https://example.com/paper",
      "quote: A short excerpt from the source.",
      "source: web search",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="cite"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      quote?: string;
      title?: string;
      url?: string;
      source?: string;
    }>(match![1]);
    expect(props?.title).toBe("Example Paper");
    expect(props?.url).toBe("https://example.com/paper");
    expect(props?.quote).toBe("A short excerpt from the source.");
    expect(props?.source).toBe("web search");
  });

  it("accepts cite with body: as quote alias and title-only", () => {
    const withBody = preprocessLiquidEmbeds("```cite\nbody: Alias quote\nurl: https://x.test\n```");
    expect(withBody).toContain('data-liquid-embed="cite"');
    const bodyMatch = withBody.match(/data-liquid-props="([^"]+)"/);
    const bodyProps = decodeLiquidProps<{ quote?: string }>(bodyMatch![1]);
    expect(bodyProps?.quote).toBe("Alias quote");

    const titleOnly = preprocessLiquidEmbeds("```cite\ntitle: Just a title\n```");
    expect(titleOnly).toContain('data-liquid-embed="cite"');
  });

  it("rejects callout without body, media without src, and empty cite", () => {
    expect(preprocessLiquidEmbeds("```callout\ntitle: Only\n```")).toContain("```callout");
    expect(preprocessLiquidEmbeds("```media\nalt: No src\n```")).toContain("```media");
    expect(preprocessLiquidEmbeds("```cite\nsource: alone\n```")).toContain("```cite");
  });

  it("turns a compare fence into axes/entities payload", () => {
    const src = [
      "```compare",
      "title: Laptops for video",
      "subtitle: 2 picks",
      "recommendation: MacBook Pro 14",
      "",
      "| | MacBook Pro 14 | XPS 15 |",
      "| --- | --- | --- |",
      "| Display | Excellent | Good |",
      "| Battery | 18h | 12h |",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="compare"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      subtitle?: string;
      recommendation?: string;
      axes: { id: string; label: string }[];
      entities: { id: string; label: string; values: Record<string, string> }[];
    }>(match![1]);
    expect(props?.title).toBe("Laptops for video");
    expect(props?.subtitle).toBe("2 picks");
    expect(props?.recommendation).toBe("MacBook Pro 14");
    expect(props?.axes.map((a) => a.label)).toEqual(["Display", "Battery"]);
    expect(props?.entities.map((e) => e.label)).toEqual(["MacBook Pro 14", "XPS 15"]);
    const displayId = props!.axes[0].id;
    expect(props?.entities[0].values[displayId]).toBe("Excellent");
    expect(props?.entities[1].values[displayId]).toBe("Good");
  });

  it("accepts compare with highlight alias and list-marker title", () => {
    const src = [
      "```compare",
      "- title: Cameras",
      "highlight: A7IV",
      "",
      "| Axis | A7IV | R5 |",
      "| --- | --- | --- |",
      "| AF | Great | Great |",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="compare"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ recommendation?: string; title?: string }>(match![1]);
    expect(props?.title).toBe("Cameras");
    expect(props?.recommendation).toBe("A7IV");
  });

  it("rejects compare with fewer than two entities", () => {
    const src = [
      "```compare",
      "title: Alone",
      "",
      "| | Solo |",
      "| --- | --- |",
      "| Speed | Fast |",
      "```",
    ].join("\n");
    expect(preprocessLiquidEmbeds(src)).toContain("```compare");
  });

  it("turns a plan fence into segments payload", () => {
    const src = [
      "```plan",
      "title: Trip flow",
      "subtitle: Simple pacing",
      "grouping: day",
      "",
      "---",
      "label: Arrive in Tokyo",
      "time: Day 1",
      "emoji: ✈️",
      "image: https://example.com/nex.jpg",
      "subtitle: Arrival · Tokyo",
      "body: Check in and ease in",
      "badge: Start here",
      "---",
      "label: Explore Tokyo",
      "time: Days 2–4",
      "emoji: 🏙️",
      "body: Mix sights and food",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="plan"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      subtitle?: string;
      grouping?: string;
      segments: {
        label: string;
        time?: string;
        emoji?: string;
        image?: string;
        badge?: string;
        body?: string;
      }[];
    }>(match![1]);
    expect(props?.title).toBe("Trip flow");
    expect(props?.subtitle).toBe("Simple pacing");
    expect(props?.grouping).toBe("day");
    expect(props?.segments).toHaveLength(2);
    expect(props?.segments[0].label).toBe("Arrive in Tokyo");
    expect(props?.segments[0].time).toBe("Day 1");
    expect(props?.segments[0].image).toBe("https://example.com/nex.jpg");
    expect(props?.segments[0].badge).toBe("Start here");
    expect(props?.segments[1].label).toBe("Explore Tokyo");
  });

  it("accepts plan with title alias for label and list-marker chrome", () => {
    const src = [
      "```plan",
      "- title: Best first route",
      "",
      "---",
      "title: Tokyo",
      "time: Days 1–4",
      "---",
      "label: Kyoto",
      "time: Days 5–7",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="plan"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ title?: string; segments: { label: string }[] }>(match![1]);
    expect(props?.title).toBe("Best first route");
    expect(props?.segments.map((s) => s.label)).toEqual(["Tokyo", "Kyoto"]);
  });

  it("rejects plan with fewer than two segments", () => {
    const src = [
      "```plan",
      "title: Alone",
      "",
      "---",
      "label: Only one",
      "```",
    ].join("\n");
    expect(preprocessLiquidEmbeds(src)).toContain("```plan");
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
