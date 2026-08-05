import assert from "node:assert/strict";
import test from "node:test";

import {
  LIQUID_MARKDOWN_STYLES,
  renderLiquidEmbedHtml,
  renderLiquidIconHtml,
} from "../dist/browser/index.js";
import {
  decodeLiquidProps,
  preprocessLiquidEmbeds,
} from "../dist/index.js";

test("renders the parser placeholder contract without a UI framework", () => {
  const placeholder = preprocessLiquidEmbeds("```card\ntitle: Shared card\nbody: **Portable** body\n```");
  const kind = placeholder.match(/data-liquid-embed="([^"]+)"/)?.[1];
  const encoded = placeholder.match(/data-liquid-props="([^"]+)"/)?.[1];
  assert.equal(kind, "card");
  assert.ok(encoded);

  const html = renderLiquidEmbedHtml(kind, decodeLiquidProps(encoded));
  assert.match(html, /Shared card/);
  assert.match(html, /data-liquid-markdown/);
  assert.match(html, /\*\*Portable\*\*/);
});

test("has a portable rendering path for every Liquid embed kind", () => {
  const payloads = {
    card: { title: "Card" },
    carousel: { items: [{ title: "One" }, { title: "Two" }] },
    actions: { actions: [{ label: "Continue" }] },
    callout: { body: "Heads up" },
    section: { title: "Section", body: "Body" },
    block: { body: "Styled" },
    chips: { chips: [{ label: "Alpha" }] },
    media: { src: "https://example.test/image.png" },
    cite: { title: "Source" },
    compare: {
      axes: [{ id: "cost", label: "Cost" }],
      entities: [
        { id: "a", label: "A", values: { cost: "Low" } },
        { id: "b", label: "B", values: { cost: "High" } },
      ],
    },
    plan: { segments: [{ id: "a", label: "First" }, { id: "b", label: "Second" }] },
    timeline: { events: [{ id: "a", label: "First" }, { id: "b", label: "Second" }] },
    shortlist: { items: [{ id: "a", label: "A" }, { id: "b", label: "B" }] },
    decision: {
      options: [
        { id: "a", label: "A", pros: ["Fast"], cons: ["Cost"] },
        { id: "b", label: "B", pros: ["Cheap"], cons: ["Slow"] },
      ],
    },
    brief: { sections: [{ id: "a", heading: "Summary", body: "Body" }] },
    dashboard: { tiles: [{ id: "a", label: "Users", value: "12" }, { id: "b", label: "MRR", value: "$30" }] },
    chart: { type: "bar", categories: ["Jan", "Feb"], series: [{ key: "users", label: "Users", values: [12, 18] }] },
    report: { body: "Report body" },
    slides: { slides: [{ id: "a", label: "Opening", body: "Hello" }] },
    tabs: { panels: [{ id: "a", label: "A", body: "One" }, { id: "b", label: "B", body: "Two" }] },
    steps: { steps: [{ id: "a", label: "First" }, { id: "b", label: "Second" }] },
    accordion: { items: [{ id: "a", label: "Question", body: "Answer" }] },
    code: { source: "const shared = true;", lang: "ts" },
    tree: { nodes: [{ id: "a", name: "src", kind: "folder", children: [{ id: "b", name: "index.ts", kind: "file" }] }] },
    feed: { feedId: "weekly", datatype: "json" },
  };

  for (const [kind, payload] of Object.entries(payloads)) {
    assert.ok(renderLiquidEmbedHtml(kind, payload), `expected ${kind} to render`);
  }
});

test("escapes model text and rejects unsafe media protocols", () => {
  const html = renderLiquidEmbedHtml("card", {
    title: '<img src=x onerror="boom">',
    image: "javascript:alert(1)",
  });
  assert.match(html, /&lt;img src=x onerror=&quot;boom&quot;&gt;/);
  assert.doesNotMatch(html, /javascript:/);
  assert.doesNotMatch(html, /<img[^>]+onerror=/i);
});

test("renders interactive semantics and dependency-free SVG charts", () => {
  const tabs = renderLiquidEmbedHtml("tabs", {
    default: "second",
    panels: [
      { id: "first", label: "First", body: "One" },
      { id: "second", label: "Second", body: "Two" },
    ],
  });
  assert.match(tabs, /role="tablist"/);
  assert.match(tabs, /data-liquid-tab="1" aria-selected="true"/);

  const chart = renderLiquidEmbedHtml("chart", {
    type: "line",
    categories: ["Jan", "Feb", "Mar"],
    series: [{ key: "visits", label: "Visits", values: [3, 7, 5] }],
  });
  assert.match(chart, /<svg/);
  assert.match(chart, /<polyline/);
});

test("ships icon and static kanban styling with the browser entrypoint", () => {
  assert.match(renderLiquidIconHtml("messageCircle"), /data-liquid-icon-id="message-circle"/);
  assert.equal(renderLiquidIconHtml("not-real"), null);
  assert.match(LIQUID_MARKDOWN_STYLES, /\.liquid-mini-kanban/);
  assert.match(LIQUID_MARKDOWN_STYLES, /\.medousa-liquid__tab/);
});
