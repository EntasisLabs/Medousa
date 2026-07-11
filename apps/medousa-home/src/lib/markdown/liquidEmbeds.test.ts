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

  it("accepts dashboard list-marker title and ignores feed: lines", () => {
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
      "---",
      "label: Next milestone",
      "value: Book N’EX",
      "```",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="dashboard"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      title?: string;
      tiles: { label: string; value: string; hint?: string; tone?: string }[];
    }>(match![1]);
    expect(props?.title).toBe("Pulse");
    expect(props?.tiles[0].label).toBe("Open questions");
    expect(props?.tiles[0].hint).toBe("hotels · JR pass");
    expect(props?.tiles[0].tone).toBe("warn");
    expect(JSON.stringify(props)).not.toContain("trip.pulse");
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
