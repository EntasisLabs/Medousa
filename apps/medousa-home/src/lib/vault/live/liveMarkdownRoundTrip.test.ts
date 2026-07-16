import { describe, expect, it } from "vitest";
import {
  detectFenceTitle,
  fencePreviewLine,
  splitMarkdownSegments,
} from "./fenceCard";
import { markdownToLiveDoc } from "./markdownToLiveDoc";
import { liveDocToMarkdown } from "./liveDocToMarkdown";
import {
  parseLiveMarkdown,
  serializeLiveMarkdown,
} from "./liveMarkdownCodec";

const DAILY_FIXTURE = `---
kind: daily
tags: [journal, focus]
---

# Tuesday

Met with [[projects/apollo|Apollo]] about the launch.

## Notes

- Ship the hybrid editor
- Keep markdown as source of truth
1. Write prose in Live
2. Edit fences in Build

> Depth stays behind the scenes.

\`\`\`report
title: Weekly pulse
range: 7d
\`\`\`

Some trailing thoughts with \`inline\` code.

\`\`\`chart
type: bar
title: Visitors
legend: bottom

| Category | Desktop | Mobile |
| --- | --- | --- |
| Jan | 186 | 80 |
| Feb | 305 | 200 |
\`\`\`
`;

describe("splitMarkdownSegments", () => {
  it("splits prose and fences without dropping text", () => {
    const body = "# Hi\n\n```chart\ntitle: A\n```\n\nBye";
    const segs = splitMarkdownSegments(body);
    expect(segs.map((s) => s.kind)).toEqual(["prose", "fence", "prose"]);
    const fence = segs[1];
    if (fence.kind !== "fence") throw new Error("expected fence");
    expect(fence.lang).toBe("chart");
    expect(fence.raw).toContain("```chart");
    expect(fence.raw.endsWith("```")).toBe(true);
    expect(segs.filter((s) => s.kind === "prose").map((s) => (s as { text: string }).text).join("")).toContain("# Hi");
    expect(segs.filter((s) => s.kind === "prose").map((s) => (s as { text: string }).text).join("")).toContain("Bye");
  });

  it("detects fence titles and previews", () => {
    expect(detectFenceTitle("title: Weekly pulse\nrange: 7d")).toBe("Weekly pulse");
    expect(fencePreviewLine("title: X\nrange: 7d")).toBe("range: 7d");
  });
});

describe("liveDoc markdown round-trip", () => {
  it("preserves fence raw bytes through TipTap JSON", () => {
    const { content: body } = (() => {
      const m = DAILY_FIXTURE.match(/^---\n[\s\S]*?\n---\n\n([\s\S]*)$/);
      return { content: m?.[1] ?? DAILY_FIXTURE };
    })();
    const reportRaw = body.match(/```report[\s\S]*?```/)?.[0];
    const chartRaw = body.match(/```chart[\s\S]*?```/)?.[0];
    expect(reportRaw).toBeTruthy();
    expect(chartRaw).toBeTruthy();

    const doc = markdownToLiveDoc(body);
    const fences = (doc.content ?? []).filter((n) => n.type === "fenceBlock");
    expect(fences).toHaveLength(2);
    expect(fences[0]?.attrs?.raw).toBe(reportRaw);
    expect(fences[1]?.attrs?.raw).toBe(chartRaw);

    const out = liveDocToMarkdown(doc);
    expect(out).toContain(reportRaw!);
    expect(out).toContain(chartRaw!);
    expect(out).toContain("[[projects/apollo|Apollo]]");
    expect(out).toContain("Ship the hybrid editor");
    expect(out).toContain("Depth stays behind the scenes");
    expect(out).toContain("trailing thoughts");
  });

  it("round-trips full note with frontmatter tags", () => {
    const parsed = parseLiveMarkdown(DAILY_FIXTURE);
    expect(parsed.tags).toEqual(["journal", "focus"]);
    expect(parsed.frontmatter).toContain("kind: daily");

    const out = serializeLiveMarkdown(parsed.doc, parsed.frontmatter);
    expect(out.startsWith("---\n")).toBe(true);
    expect(out).toContain("kind: daily");
    expect(out).toContain("tags:");
    expect(out).toContain("```report");
    expect(out).toContain("```chart");
    expect(out).toContain("[[projects/apollo|Apollo]]");
    expect(out).toContain("Ship the hybrid editor");
    // Fence bodies must not be eaten
    expect(out).toContain("Weekly pulse");
    expect(out).toContain("| Jan | 186 | 80 |");
  });

  it("does not put frontmatter into the live doc stream", () => {
    const parsed = parseLiveMarkdown(DAILY_FIXTURE);
    const text = JSON.stringify(parsed.doc);
    expect(text).not.toContain("kind: daily");
    expect(text).not.toContain('"tags"');
  });
});
