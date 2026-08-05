import assert from "node:assert/strict";
import test from "node:test";

import {
  decodeLiquidProps,
  normalizeLiquidIconId,
  preprocessLiquidEmbeds,
} from "../dist/index.js";

function propsFrom(source) {
  const rendered = preprocessLiquidEmbeds(source);
  const encoded = rendered.match(/data-liquid-props="([^"]+)"/)?.[1];
  assert.ok(encoded, "expected a Liquid placeholder payload");
  return decodeLiquidProps(encoded);
}

test("parses a Liquid card without a UI framework", () => {
  assert.deepEqual(
    propsFrom("```card\ntitle: Shared\nbody: Across surfaces\n```"),
    { title: "Shared", body: "Across surfaces" },
  );
});

test("keeps nested Liquid fences inside report sections", () => {
  const output = preprocessLiquidEmbeds([
    "```report",
    "title: Weekly",
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
    "## Metrics",
    "",
    "More prose.",
    "```",
  ].join("\n"));
  assert.match(output, /data-liquid-embed="report"/);
  assert.doesNotMatch(output, /```report/);
});

test("normalizes only allowlisted icon ids", () => {
  assert.equal(normalizeLiquidIconId("messageCircle"), "message-circle");
  assert.equal(normalizeLiquidIconId("made-up"), null);
});
