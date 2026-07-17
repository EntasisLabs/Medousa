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

  it("parses YAML block-scalar card bodies (clarification style)", () => {
    const src = [
      "```card",
      "title: 🤖 /dev?",
      "body: |-",
      "Is that a developer workspace you're reaching for — or did something slip off the keyboard?",
      "I can open a dev surface, pull up canvas debug, or we can keep sitting in the shadow trio. Your call.",
      "emoji: 🧰",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="card"');
    expect(out).not.toContain("|-");
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title: string;
      body?: string;
      emoji?: string;
    }>(match![1]);
    expect(props?.title).toBe("🤖 /dev?");
    expect(props?.emoji).toBe("🧰");
    expect(props?.body).toContain("developer workspace");
    expect(props?.body).toContain("shadow trio");
    expect(props?.body).not.toContain("|-");
  });

  it("parses callout body block scalars the same way", () => {
    const src = [
      "```callout",
      "tone: note",
      "title: Check",
      "body: |",
      "Line one of the note.",
      "Line two of the note.",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ body: string; title?: string }>(match![1]);
    expect(props?.title).toBe("Check");
    expect(props?.body).toBe("Line one of the note.\nLine two of the note.");
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

  it("parses carousel --- blocks with expandable detail fields", () => {
    const src = [
      "```carousel",
      "title: Where it stands",
      "",
      "---",
      "title: Tradeoff",
      "subtitle: Less proven independently",
      "emoji: 📋",
      "body: Less proven independently",
      "meta: Caveat · AI model · Early read",
      "summary: Treat benchmarks as provisional.",
      "chips: Benchmarks | Claims | Validation",
      "point: Launch framing | Most early coverage echoes company claims. | 📰",
      "point: Independent testing | Third-party evals usually lag launches.",
      "---",
      "title: Strengths",
      "subtitle: Coding and agent work",
      "summary: Strong early signal on agentic coding.",
      "chips: Coding | Agents",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="carousel"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      items: {
        title: string;
        meta?: string;
        summary?: string;
        chips?: string[];
        points?: { label: string; body: string; emoji?: string }[];
      }[];
    }>(match![1]);
    expect(props?.items).toHaveLength(2);
    expect(props?.items[0].title).toBe("Tradeoff");
    expect(props?.items[0].meta).toBe("Caveat · AI model · Early read");
    expect(props?.items[0].summary).toBe("Treat benchmarks as provisional.");
    expect(props?.items[0].chips).toEqual(["Benchmarks", "Claims", "Validation"]);
    expect(props?.items[0].points).toHaveLength(2);
    expect(props?.items[0].points?.[0]).toEqual({
      label: "Launch framing",
      body: "Most early coverage echoes company claims.",
      emoji: "📰",
    });
    expect(props?.items[1].title).toBe("Strengths");
    expect(props?.items[1].chips).toEqual(["Coding", "Agents"]);
  });

  it("parses a single card with points and --- summary alias", () => {
    const src = [
      "```card",
      "title: Tradeoff",
      "emoji: 📋",
      "meta: Caveat · Early read",
      "chips: Benchmarks | Claims",
      "point: Launch framing | Echoes company claims.",
      "---",
      "The biggest caveat is independent validation.",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title: string;
      meta?: string;
      summary?: string;
      chips?: string[];
      points?: { label: string; body: string }[];
    }>(match![1]);
    expect(props?.title).toBe("Tradeoff");
    expect(props?.meta).toBe("Caveat · Early read");
    expect(props?.chips).toEqual(["Benchmarks", "Claims"]);
    expect(props?.points?.[0].label).toBe("Launch framing");
    expect(props?.summary).toBe("The biggest caveat is independent validation.");
  });

  it("captures freeform lines after a one-line body as summary", () => {
    const src = [
      "```card",
      "title: 🚀 Medousa — soft launch",
      "emoji: 🚀",
      "body: Dropping Medousa into the wild with my AI buddies 🧠",
      "",
      "We've been building a permanent AI workspace — chat, vault, calendar,",
      "and agents that actually stick around.",
      "",
      "Soft launch with the crew. Feedback welcome.",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title: string;
      body?: string;
      summary?: string;
    }>(match![1]);
    expect(props?.body).toBe("Dropping Medousa into the wild with my AI buddies 🧠");
    expect(props?.summary).toContain("Dropping Medousa into the wild with my AI buddies 🧠");
    expect(props?.summary).toContain("permanent AI workspace");
    expect(props?.summary).toContain("Feedback welcome.");
  });

  it("treats title-only freeform prose as summary", () => {
    const src = [
      "```card",
      "title: Soft launch",
      "Hey #projects — Medousa is out in the wild.",
      "Come break it with me.",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ title: string; summary?: string; body?: string }>(match![1]);
    expect(props?.title).toBe("Soft launch");
    expect(props?.body).toBeUndefined();
    expect(props?.summary).toBe("Hey #projects — Medousa is out in the wild.\nCome break it with me.");
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

  it("parses compare mode faceoff (and face-off alias)", () => {
    const src = [
      "```compare",
      "title: Head to head",
      "mode: face-off",
      "recommendation: Alpha",
      "",
      "| | Alpha | Beta |",
      "| --- | --- | --- |",
      "| Speed | Fast | Slow |",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ mode?: string; entities: { label: string }[] }>(match![1]);
    expect(props?.mode).toBe("faceoff");
    expect(props?.entities).toHaveLength(2);
  });

  it("omits unknown compare mode (matrix default)", () => {
    const src = [
      "```compare",
      "mode: radar",
      "",
      "| | A | B |",
      "| --- | --- | --- |",
      "| X | 1 | 2 |",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ mode?: string }>(match![1]);
    expect(props?.mode).toBeUndefined();
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

  it("turns a timeline fence into events payload", () => {
    const src = [
      "```timeline",
      "title: Japan trip so far",
      "subtitle: What we locked in",
      "granularity: day",
      "",
      "---",
      "ts: Day 1 · Jul 12",
      "label: Arrive Narita → Shinjuku",
      "detail: N’EX in, hotel near the station.",
      "lane: travel",
      "emoji: ✈️",
      "---",
      "ts: Days 2–4",
      "label: Tokyo base",
      "detail: Markets and neon nights.",
      "lane: stay",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="timeline"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      subtitle?: string;
      granularity?: string;
      events: {
        label: string;
        ts?: string;
        detail?: string;
        lane?: string;
        emoji?: string;
      }[];
    }>(match![1]);
    expect(props?.title).toBe("Japan trip so far");
    expect(props?.subtitle).toBe("What we locked in");
    expect(props?.granularity).toBe("day");
    expect(props?.events).toHaveLength(2);
    expect(props?.events[0].label).toBe("Arrive Narita → Shinjuku");
    expect(props?.events[0].ts).toBe("Day 1 · Jul 12");
    expect(props?.events[0].lane).toBe("travel");
    expect(props?.events[1].label).toBe("Tokyo base");
  });

  it("accepts timeline with time/body aliases and missing ts", () => {
    const src = [
      "```timeline",
      "- title: Ship log",
      "",
      "---",
      "title: Compare landed",
      "body: First sacred-seven organism",
      "---",
      "label: Plan landed",
      "time: later",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="timeline"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      events: { label: string; ts?: string; detail?: string }[];
    }>(match![1]);
    expect(props?.title).toBe("Ship log");
    expect(props?.events[0].label).toBe("Compare landed");
    expect(props?.events[0].detail).toBe("First sacred-seven organism");
    expect(props?.events[0].ts).toBeUndefined();
    expect(props?.events[1].ts).toBe("later");
  });

  it("rejects timeline with fewer than two events", () => {
    const src = [
      "```timeline",
      "title: Alone",
      "",
      "---",
      "label: Only one",
      "```",
    ].join("\n");
    expect(preprocessLiquidEmbeds(src)).toContain("```timeline");
  });

  it("turns a shortlist fence into items payload", () => {
    const src = [
      "```shortlist",
      "title: Great neighborhoods",
      "subtitle: Mid-range stays",
      "criteria: energy · food · transit",
      "density: comfortable",
      "",
      "---",
      "label: Shinjuku",
      "summary: Best for energy and late nights",
      "score: 9.2",
      "meta: Big energy",
      "emoji: 🌃",
      "---",
      "label: Asakusa",
      "summary: Traditional feel near Senso-ji",
      "score: 8.4",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="shortlist"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      subtitle?: string;
      criteria?: string;
      density?: string;
      items: {
        label: string;
        summary?: string;
        score?: string;
        meta?: string;
        emoji?: string;
      }[];
    }>(match![1]);
    expect(props?.title).toBe("Great neighborhoods");
    expect(props?.criteria).toBe("energy · food · transit");
    expect(props?.density).toBe("comfortable");
    expect(props?.items).toHaveLength(2);
    expect(props?.items[0].label).toBe("Shinjuku");
    expect(props?.items[0].score).toBe("9.2");
    expect(props?.items[1].label).toBe("Asakusa");
  });

  it("accepts shortlist with title/body aliases and list-marker chrome", () => {
    const src = [
      "```shortlist",
      "- title: Picks",
      "density: compact",
      "",
      "---",
      "title: Option A",
      "body: Solid all-rounder",
      "---",
      "label: Option B",
      "summary: Budget pick",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="shortlist"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      density?: string;
      items: { label: string; summary?: string }[];
    }>(match![1]);
    expect(props?.title).toBe("Picks");
    expect(props?.density).toBe("compact");
    expect(props?.items.map((i) => i.label)).toEqual(["Option A", "Option B"]);
    expect(props?.items[0].summary).toBe("Solid all-rounder");
  });

  it("rejects shortlist with fewer than two items", () => {
    const src = [
      "```shortlist",
      "title: Alone",
      "",
      "---",
      "label: Only one",
      "```",
    ].join("\n");
    expect(preprocessLiquidEmbeds(src)).toContain("```shortlist");
  });

  it("turns a decision fence into options with pros/cons", () => {
    const src = [
      "```decision",
      "title: Which laptop?",
      "subtitle: For video",
      "factors: display · battery · price",
      "recommendation: MacBook Pro 14",
      "",
      "---",
      "label: MacBook Pro 14",
      "score: 9.1",
      "pros: Best display | Long battery | Quiet fans",
      "cons: Expensive | Ports need dongle",
      "---",
      "label: XPS 15",
      "score: 7.8",
      "pros: Great screen | More ports",
      "cons: Thermals",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="decision"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      recommendation?: string;
      factors?: string;
      options: {
        label: string;
        pros: string[];
        cons: string[];
        score?: string;
      }[];
    }>(match![1]);
    expect(props?.title).toBe("Which laptop?");
    expect(props?.recommendation).toBe("MacBook Pro 14");
    expect(props?.options).toHaveLength(2);
    expect(props?.options[0].pros).toEqual(["Best display", "Long battery", "Quiet fans"]);
    expect(props?.options[0].cons).toEqual(["Expensive", "Ports need dongle"]);
    expect(props?.options[1].label).toBe("XPS 15");
  });

  it("accepts decision with highlight alias and pro/con singular keys", () => {
    const src = [
      "```decision",
      "- title: Pick one",
      "highlight: Alpha",
      "",
      "---",
      "title: Alpha",
      "pro: Fast",
      "con: Pricey",
      "---",
      "label: Beta",
      "pros: Cheap",
      "cons: Slow",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="decision"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      recommendation?: string;
      options: { label: string; pros: string[]; cons: string[] }[];
    }>(match![1]);
    expect(props?.recommendation).toBe("Alpha");
    expect(props?.options[0].label).toBe("Alpha");
    expect(props?.options[0].pros).toEqual(["Fast"]);
    expect(props?.options[0].cons).toEqual(["Pricey"]);
  });

  it("rejects decision with fewer than two options", () => {
    const src = [
      "```decision",
      "title: Alone",
      "",
      "---",
      "label: Only one",
      "pros: Yes",
      "```",
    ].join("\n");
    expect(preprocessLiquidEmbeds(src)).toContain("```decision");
  });

  it("turns a brief fence into sections and sources", () => {
    const src = [
      "```brief",
      "title: Why Tokyo first",
      "subtitle: For two",
      "tone: research",
      "",
      "---",
      "heading: Easy logistics",
      "body: One simple route, minimal backtracking.",
      "---",
      "heading: Food mix",
      "body: Street food, markets, sushi.",
      "",
      "===",
      "---",
      "title: JNTO Tokyo guide",
      "url: https://example.com/tokyo",
      "quote: Start in the capital for energy.",
      "---",
      "title: JR East",
      "url: https://example.com/jr",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="brief"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      tone?: string;
      sections: { heading: string; body: string }[];
      sources?: { title: string; url?: string; quote?: string }[];
    }>(match![1]);
    expect(props?.title).toBe("Why Tokyo first");
    expect(props?.tone).toBe("research");
    expect(props?.sections).toHaveLength(2);
    expect(props?.sections[0].heading).toBe("Easy logistics");
    expect(props?.sources).toHaveLength(2);
    expect(props?.sources?.[0].title).toBe("JNTO Tokyo guide");
    expect(props?.sources?.[0].url).toBe("https://example.com/tokyo");
  });

  it("accepts brief with nested --- body and title alias", () => {
    const src = [
      "```brief",
      "- title: Memo",
      "",
      "---",
      "title: Point one",
      "---",
      "Prose body after nested separator.",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="brief"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      sections: { heading: string; body: string }[];
    }>(match![1]);
    expect(props?.title).toBe("Memo");
    expect(props?.sections[0].heading).toBe("Point one");
    expect(props?.sections[0].body).toBe("Prose body after nested separator.");
  });

  it("rejects brief with no sections", () => {
    const src = ["```brief", "title: Empty", "```"].join("\n");
    expect(preprocessLiquidEmbeds(src)).toContain("```brief");
  });

  it("hydrates Mon Laferte-style brief with ## sections and freeform tone", () => {
    const src = [
      "```brief",
      "title: Why Mon Laferte Resonates So Deeply",
      "subtitle: An analysis of the artist behind the ache",
      "tone: warm, analytical, reverent",
      "---",
      "## The Wound That Became Her Voice",
      "She *wears* her pain — never flinches.",
      "",
      "## The Music: An Alto Built for Brokenness",
      "- **Bolero**",
      "- **Cumbia / Salsa**",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="brief"');
    expect(out).not.toContain("```brief");
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      tone?: string;
      sections: { heading: string; body: string }[];
    }>(match![1]);
    expect(props?.title).toBe("Why Mon Laferte Resonates So Deeply");
    expect(props?.tone).toBe("research");
    expect(props?.sections).toHaveLength(2);
    expect(props?.sections[0].heading).toBe("The Wound That Became Her Voice");
    expect(props?.sections[0].body).toContain("wears");
    expect(props?.sections[1].heading).toBe("The Music: An Alto Built for Brokenness");
  });

  it("keeps pipe characters inside cite titles", () => {
    const src = [
      "```cite",
      "title: Mon Laferte - Biography, Discography, Albums | AllMusic",
      "url: https://www.allmusic.com/artist/mon-laferte",
      "quote: Chilean and Mexican singer-songwriter.",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="cite"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ title?: string; url?: string }>(match![1]);
    expect(props?.title).toBe("Mon Laferte - Biography, Discography, Albums | AllMusic");
    expect(props?.url).toContain("allmusic.com");
  });

  it("unwraps mistaken prose ```code fences into plain markdown", () => {
    const src = [
      "Intro.",
      "",
      "```code",
      "That's the full analysis. She hits the soul hard because she **never flinches**.",
      "```",
      "",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).not.toContain("```code");
    expect(out).toContain("**never flinches**");
    expect(out).toContain("That's the full analysis");
  });

  it("unwraps bare ``` prose fences and soft-converts | Source: | lines", () => {
    const src = [
      "| Source: AllMusic, Wikipedia |",
      "",
      "```",
      "Mon Laferte is a rare artist who navigated pop to auteur.",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain("*Source: AllMusic, Wikipedia*");
    expect(out).not.toMatch(/^```/m);
    expect(out).toContain("rare artist");
  });

  it("leaves real code fences alone", () => {
    const src = ["```python", "def hello():", "    return 1", "```"].join("\n");
    expect(preprocessLiquidEmbeds(src)).toContain("```python");
  });

  it("does not rewrite ```tabs examples inside a documentation fence", () => {
    // Exact shape from LED: docs fence wrapping a tabs example, plus a redundant
    // outer closer that must not open a code block over trailing prose.
    const src = [
      "So: the format is:",
      "",
      "```",
      "```tabs",
      "title: … | default: …",
      "",
      "---",
      "label: Panel name",
      "body: Content here",
      "---",
      "label: Another panel",
      "body: More content",
      "```",
      "```",
      "",
      "Straightforward. Now I'm ready to actually **build something** with these. What's the move?",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).not.toContain("data-liquid-embed");
    expect(out).toContain("```tabs");
    expect(out).toContain("label: Panel name");
    expect(out).toContain(
      "Straightforward. Now I'm ready to actually **build something** with these. What's the move?",
    );

    const lines = out.split("\n");
    const proseLine = lines.findIndex((l) => l.startsWith("Straightforward."));
    expect(proseLine).toBeGreaterThan(0);
    let depth = 0;
    for (let i = 0; i < proseLine; i++) {
      const open = lines[i]!.match(/^(````*)([a-zA-Z0-9_-]*)[ \t]*$/);
      if (depth === 0 && open && open[1]!.length >= 3) {
        depth = 1;
        continue;
      }
      if (depth === 1 && /^(````*)[ \t]*$/.test(lines[i]!)) {
        depth = 0;
      }
    }
    expect(depth).toBe(0);
  });

  it("still hydrates a live ```tabs fence at top level", () => {
    const src = [
      "```tabs",
      "title: Blume × Medousa | default: Demo",
      "",
      "---",
      "label: Demo",
      "body: Working tabs embed",
      "---",
      "label: Next Move",
      "body: Pick a direction",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="tabs"');
    expect(out).not.toContain("```tabs");
  });

  it("resolves nested cite inside brief without eating the cite closer (Mon Laferte shape)", () => {
    const src = [
      "# Why Mon Laferte Hits the Soul So Hard",
      "",
      "```brief",
      "title: Why Mon Laferte Resonates So Deeply",
      "subtitle: An analysis",
      "tone: warm, analytical, reverent",
      "",
      "---",
      "",
      "## The Wound",
      "",
      "She *wears* her pain — **never flinches**.",
      "",
      "## The Verdict",
      "",
      "She's singing *from* it.",
      "",
      "```cite",
      "title: Mon Laferte — Biography | AllMusic",
      "url: https://www.allmusic.com/artist/mon-laferte-mn0003223857",
      'quote: "Award-winning Chilean singer."',
      "```",
      "",
      "```cite",
      "title: Mon Laferte — Wikipedia",
      "url: https://en.wikipedia.org/wiki/Mon_Laferte",
      'quote: "Chilean and Mexican singer-songwriter."',
      "```",
      "```",
      "",
      "That's the full analysis. She hits the soul hard because she **never flinches**.",
      "",
    ].join("\n");

    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="brief"');
    // Innermost-first: cites become placeholders *inside* brief section bodies
    // (Brief re-enters MarkdownContent → hydrate), not sibling top-level embeds.
    expect(out).not.toContain("```brief");
    expect(out).not.toContain("```cite");
    // Trailing bare fence must not swallow the finale into a code block
    expect(out).toContain("That's the full analysis");
    expect(out).toContain("**never flinches**");
    expect(out.trimEnd()).not.toMatch(/```\s*$/);

    const briefMatch = out.match(
      /data-liquid-embed="brief"[^>]*data-liquid-props="([^"]+)"/,
    );
    const brief = decodeLiquidProps<{
      title?: string;
      sections: { heading: string; body: string }[];
    }>(briefMatch![1]);
    expect(brief?.title).toBe("Why Mon Laferte Resonates So Deeply");
    expect(brief?.sections.some((s) => s.heading === "The Wound")).toBe(true);
    expect(brief?.sections.some((s) => s.body.includes("never flinches"))).toBe(
      true,
    );
    const nestedCiteCount = brief!.sections.reduce(
      (n, s) => n + (s.body.match(/data-liquid-embed="cite"/g)?.length ?? 0),
      0,
    );
    expect(nestedCiteCount).toBe(2);
  });

  it("turns a dashboard fence into title + tiles", () => {
    const src = [
      "```dashboard",
      "title: Trip pulse",
      "subtitle: Japan · mid-range for two",
      "columns: 2",
      "",
      "---",
      "label: Days locked",
      "value: 7",
      "delta: +2 vs draft",
      "tone: success",
      "emoji: 📅",
      "---",
      "label: Budget used",
      "value: 42%",
      "delta: on track",
      "tone: accent",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="dashboard"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      subtitle?: string;
      columns?: string;
      tiles: {
        label: string;
        value: string;
        delta?: string;
        tone?: string;
        emoji?: string;
      }[];
    }>(match![1]);
    expect(props?.title).toBe("Trip pulse");
    expect(props?.subtitle).toBe("Japan · mid-range for two");
    expect(props?.columns).toBe("2");
    expect(props?.tiles).toHaveLength(2);
    expect(props?.tiles[0].label).toBe("Days locked");
    expect(props?.tiles[0].value).toBe("7");
    expect(props?.tiles[0].delta).toBe("+2 vs draft");
    expect(props?.tiles[0].tone).toBe("success");
    expect(props?.tiles[0].emoji).toBe("📅");
    expect(props?.tiles[1].tone).toBe("accent");
  });

  it("accepts dashboard list-marker title and parses feed:/field:", () => {
    const src = [
      "```dashboard",
      "- title: Pulse",
      "",
      "---",
      "title: Open questions",
      "value: 3",
      "body: hotels · JR pass",
      "tone: warn",
      "feed: trip.pulse",
      "field: summary",
      "---",
      "label: Next milestone",
      "value: Book N’EX",
      "binding: work:board",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="dashboard"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      tiles: {
        label: string;
        value: string;
        hint?: string;
        tone?: string;
        feed?: string;
        field?: string;
      }[];
    }>(match![1]);
    expect(props?.title).toBe("Pulse");
    expect(props?.tiles[0].label).toBe("Open questions");
    expect(props?.tiles[0].hint).toBe("hotels · JR pass");
    expect(props?.tiles[0].tone).toBe("warn");
    expect(props?.tiles[0].feed).toBe("trip.pulse");
    expect(props?.tiles[0].field).toBe("summary");
    expect(JSON.stringify(props)).not.toContain("work:board");
  });

  it("rejects dashboard with fewer than two tiles", () => {
    const src = [
      "```dashboard",
      "title: Alone",
      "",
      "---",
      "label: Only one",
      "value: 1",
      "```",
    ].join("\n");
    expect(preprocessLiquidEmbeds(src)).toContain("```dashboard");
  });

  it("turns a chart fence into typed series payload", () => {
    const src = [
      "```chart",
      "type: bar",
      "title: Visitors",
      "description: January - June 2024",
      "stacked: true",
      "legend: bottom",
      "tooltip: true",
      "labels: none",
      "trend: Trending up by 5.2% this month",
      "trendDirection: up",
      "caption: Showing total visitors for the last 6 months",
      "",
      "| Month | Desktop | Mobile |",
      "| ----- | ------- | ------ |",
      "| Jan   | 186     | 80     |",
      "| Feb   | 305     | 200    |",
      "| Mar   | 237     | 120    |",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="chart"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      type: string;
      title?: string;
      description?: string;
      stacked?: boolean;
      legend?: string | boolean;
      tooltip?: boolean;
      labels?: string;
      trend?: string;
      trendDirection?: string;
      caption?: string;
      categories: string[];
      series: { key: string; label: string; values: number[] }[];
    }>(match![1]);
    expect(props?.type).toBe("bar");
    expect(props?.title).toBe("Visitors");
    expect(props?.description).toBe("January - June 2024");
    expect(props?.stacked).toBe(true);
    expect(props?.legend).toBe("bottom");
    expect(props?.tooltip).toBe(true);
    expect(props?.labels).toBe("none");
    expect(props?.trendDirection).toBe("up");
    expect(props?.categories).toEqual(["Jan", "Feb", "Mar"]);
    expect(props?.series).toHaveLength(2);
    expect(props?.series[0].label).toBe("Desktop");
    expect(props?.series[0].values).toEqual([186, 305, 237]);
    expect(props?.series[1].label).toBe("Mobile");
    expect(props?.series[1].values).toEqual([80, 200, 120]);
  });

  it("parses donut charts with center chrome", () => {
    const src = [
      "```chart",
      "type: donut",
      "title: Traffic by browser",
      "centerValue: 1,125",
      "centerLabel: Visitors",
      "labels: value",
      "separator: true",
      "",
      "| Browser | Visitors |",
      "| ------- | -------- |",
      "| Chrome  | 275      |",
      "| Safari  | 200      |",
      "| Firefox | 187      |",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="chart"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      type: string;
      centerValue?: string;
      centerLabel?: string;
      labels?: string;
      separator?: boolean;
      categories: string[];
      series: { values: number[] }[];
    }>(match![1]);
    expect(props?.type).toBe("donut");
    expect(props?.centerValue).toBe("1,125");
    expect(props?.centerLabel).toBe("Visitors");
    expect(props?.labels).toBe("value");
    expect(props?.separator).toBe(true);
    expect(props?.categories).toEqual(["Chrome", "Safari", "Firefox"]);
    expect(props?.series[0].values).toEqual([275, 200, 187]);
  });

  it("parses chart width and surface wash", () => {
    const src = [
      "```chart",
      "type: radar",
      "title: Coverage",
      "width: sm",
      "surface: muted",
      "height: lg",
      "",
      "| Axis | Alpha |",
      "| ---- | ----- |",
      "| A    | 80    |",
      "| B    | 70    |",
      "| C    | 60    |",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      type: string;
      width?: string;
      height?: string;
      surface?: string;
    }>(match![1]);
    expect(props?.type).toBe("radar");
    expect(props?.width).toBe("sm");
    expect(props?.height).toBe("lg");
    expect(props?.surface).toBe("muted");
  });

  it("accepts reserved radar type without failing parse", () => {
    const src = [
      "```chart",
      "type: radar",
      "title: Coverage",
      "",
      "| Axis | Score |",
      "| ---- | ----- |",
      "| A    | 80    |",
      "| B    | 60    |",
      "| C    | 70    |",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="chart"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ type: string }>(match![1]);
    expect(props?.type).toBe("radar");
  });

  it("accepts radial type with hydrate props", () => {
    const src = [
      "```chart",
      "type: radial",
      "title: Progress",
      "centerValue: 75%",
      "centerLabel: Goal",
      "",
      "| Metric | Value |",
      "| ------ | ----- |",
      "| Done   | 75    |",
      "| Target | 100   |",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="chart"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      type: string;
      centerValue?: string;
      centerLabel?: string;
      categories: string[];
    }>(match![1]);
    expect(props?.type).toBe("radial");
    expect(props?.centerValue).toBe("75%");
    expect(props?.centerLabel).toBe("Goal");
    expect(props?.categories).toEqual(["Done", "Target"]);
  });

  it("rejects chart with unknown type", () => {
    const src = [
      "```chart",
      "type: waterfall",
      "",
      "| Month | Value |",
      "| ----- | ----- |",
      "| Jan   | 10    |",
      "| Feb   | 20    |",
      "```",
    ].join("\n");
    expect(preprocessLiquidEmbeds(src)).toContain("```chart");
  });

  it("rejects chart with fewer than two data rows", () => {
    const src = [
      "```chart",
      "type: line",
      "",
      "| Month | Value |",
      "| ----- | ----- |",
      "| Jan   | 10    |",
      "```",
    ].join("\n");
    expect(preprocessLiquidEmbeds(src)).toContain("```chart");
  });

  it("parses scatter charts with optional group column", () => {
    const src = [
      "```chart",
      "type: scatter",
      "title: Spend vs conversion",
      "legend: bottom",
      "colors: blue, purple",
      "",
      "| X | Y | Cohort |",
      "| - | - | ------ |",
      "| 12 | 40 | Alpha |",
      "| 18 | 55 | Alpha |",
      "| 9 | 22 | Beta |",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="chart"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      type: string;
      points?: { x: number; y: number; group?: string }[];
      series: { label: string }[];
    }>(match![1]);
    expect(props?.type).toBe("scatter");
    expect(props?.points).toHaveLength(3);
    expect(props?.points?.[0]).toEqual({ x: 12, y: 40, group: "Alpha" });
    expect(props?.series.map((s) => s.label)).toEqual(["Alpha", "Beta"]);
  });

  it("rejects scatter with fewer than two points", () => {
    const src = [
      "```chart",
      "type: scatter",
      "",
      "| X | Y |",
      "| - | - |",
      "| 12 | 40 |",
      "```",
    ].join("\n");
    expect(preprocessLiquidEmbeds(src)).toContain("```chart");
  });

  it("parses combo charts with seriesMarks", () => {
    const src = [
      "```chart",
      "type: combo",
      "title: Revenue and growth",
      "seriesMarks: bar, line",
      "",
      "| Month | Revenue | Growth % |",
      "| ----- | ------- | -------- |",
      "| Jan   | 120     | 4        |",
      "| Feb   | 148     | 7        |",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      type: string;
      seriesMarks?: string[];
      categories: string[];
      series: { label: string; values: number[] }[];
    }>(match![1]);
    expect(props?.type).toBe("combo");
    expect(props?.seriesMarks).toEqual(["bar", "line"]);
    expect(props?.categories).toEqual(["Jan", "Feb"]);
    expect(props?.series[0].values).toEqual([120, 148]);
  });

  it("parses heatmap matrix tables", () => {
    const src = [
      "```chart",
      "type: heatmap",
      "title: Activity by hour",
      "colors: blue",
      "",
      "|           | Mon | Tue | Wed |",
      "| --------- | --- | --- | --- |",
      "| Morning   | 2   | 5   | 3   |",
      "| Afternoon | 8   | 6   | 9   |",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      type: string;
      matrix?: { rows: string[]; cols: string[]; values: number[][] };
    }>(match![1]);
    expect(props?.type).toBe("heatmap");
    expect(props?.matrix?.rows).toEqual(["Morning", "Afternoon"]);
    expect(props?.matrix?.cols).toEqual(["Mon", "Tue", "Wed"]);
    expect(props?.matrix?.values).toEqual([
      [2, 5, 3],
      [8, 6, 9],
    ]);
  });

  it("rejects heatmap with non-numeric cells", () => {
    const src = [
      "```chart",
      "type: heatmap",
      "",
      "|     | Mon |",
      "| --- | --- |",
      "| A   | x   |",
      "```",
    ].join("\n");
    expect(preprocessLiquidEmbeds(src)).toContain("```chart");
  });

  it("parses report with nested chart placeholders", () => {
    const src = [
      "```report",
      "title: Q2 growth review",
      "subtitle: North America",
      "columns: 2",
      "",
      "Opening prose.",
      "",
      "```chart",
      "type: bar",
      "title: Visitors",
      "",
      "| Month | Desktop |",
      "| ----- | ------- |",
      "| Jan   | 186     |",
      "| Feb   | 305     |",
      "```",
      "",
      "## Deep dive",
      "",
      "More prose.",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="report"');
    expect(out).not.toContain("```report");
    expect(out).not.toContain("```chart");
    const match = out.match(/data-liquid-embed="report"[^>]*data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      subtitle?: string;
      columns?: string;
      body: string;
    }>(match![1]);
    expect(props?.title).toBe("Q2 growth review");
    expect(props?.subtitle).toBe("North America");
    expect(props?.columns).toBe("2");
    expect(props?.body).toContain("Opening prose.");
    expect(props?.body).toContain('data-liquid-embed="chart"');
    expect(props?.body).toContain("## Deep dive");
  });

  it("defaults report columns to 2", () => {
    const src = [
      "```report",
      "title: Solo",
      "",
      "Just prose.",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ columns?: string }>(match![1]);
    expect(props?.columns).toBe("2");
  });

  it("turns a tabs fence into panels payload", () => {
    const src = [
      "```tabs",
      "title: Setup",
      "default: Run",
      "",
      "---",
      "label: Install",
      "body: npm install medousa",
      "---",
      "label: Run",
      "body: medousa up",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="tabs"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      default?: string;
      panels: { label: string; body: string }[];
    }>(match![1]);
    expect(props?.title).toBe("Setup");
    expect(props?.default).toBe("Run");
    expect(props?.panels).toHaveLength(2);
    expect(props?.panels[0].label).toBe("Install");
    expect(props?.panels[1].body).toBe("medousa up");
  });

  it("rejects tabs with fewer than two panels", () => {
    const src = ["```tabs", "---", "label: Only", "body: one", "```"].join("\n");
    expect(preprocessLiquidEmbeds(src)).toContain("```tabs");
  });

  it("turns a steps fence into numbered steps", () => {
    const src = [
      "```steps",
      "title: Ship",
      "",
      "---",
      "label: Build",
      "body: cargo build --release",
      "status: done",
      "---",
      "label: Deploy",
      "body: Upload the binary",
      "status: current",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="steps"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      steps: { label: string; status?: string }[];
    }>(match![1]);
    expect(props?.steps).toHaveLength(2);
    expect(props?.steps[0].status).toBe("done");
    expect(props?.steps[1].label).toBe("Deploy");
  });

  it("turns an accordion fence into collapsible items", () => {
    const src = [
      "```accordion",
      "title: FAQ",
      "multiple: true",
      "",
      "---",
      "label: What is Liquid?",
      "body: Paste-first markdown embeds.",
      "open: true",
      "---",
      "label: Who hydrates?",
      "body: The client runtime.",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="accordion"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      multiple?: boolean;
      items: { label: string; open?: boolean }[];
    }>(match![1]);
    expect(props?.multiple).toBe(true);
    expect(props?.items).toHaveLength(2);
    expect(props?.items[0].open).toBe(true);
  });

  it("turns a structured code fence into an enhanced snippet", () => {
    const src = [
      "```code",
      "lang: typescript",
      "title: greet.ts",
      "---",
      "export const greet = (n: string) => `hi ${n}`;",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="code"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      lang?: string;
      title?: string;
      source: string;
    }>(match![1]);
    expect(props?.lang).toBe("typescript");
    expect(props?.title).toBe("greet.ts");
    expect(props?.source).toContain("export const greet");
  });

  it("parses diff code fences", () => {
    const src = [
      "```code",
      "lang: diff",
      "title: patch",
      "---",
      "- const a = 1",
      "+ const a = 2",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{ diff?: boolean; source: string }>(match![1]);
    expect(props?.diff).toBe(true);
    expect(props?.source).toContain("+ const a = 2");
  });

  it("still unwraps mistaken prose ```code fences", () => {
    const src = [
      "```code",
      "That's the full analysis. She hits the soul hard because she **never flinches**.",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).not.toContain("```code");
    expect(out).not.toContain('data-liquid-embed="code"');
    expect(out).toContain("**never flinches**");
  });

  it("turns a tree fence into nested file nodes", () => {
    const src = [
      "```tree",
      "title: Project",
      "---",
      "src/",
      "  lib/",
      "    index.ts",
      "  routes/",
      "    +page.svelte",
      "README.md",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="tree"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      nodes: { name: string; kind: string; children?: { name: string }[] }[];
    }>(match![1]);
    expect(props?.title).toBe("Project");
    expect(props?.nodes).toHaveLength(2);
    expect(props?.nodes[0].name).toBe("src");
    expect(props?.nodes[0].kind).toBe("folder");
    expect(props?.nodes[0].children?.[0].name).toBe("lib");
    expect(props?.nodes[1].name).toBe("README.md");
    expect(props?.nodes[1].kind).toBe("file");
  });

  it("rejects empty tree and bare code without liquid chrome", () => {
    expect(preprocessLiquidEmbeds("```tree\n```")).toContain("```tree");
    expect(
      preprocessLiquidEmbeds(["```code", "function foo() {}", "```"].join("\n")),
    ).toContain("```code");
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
