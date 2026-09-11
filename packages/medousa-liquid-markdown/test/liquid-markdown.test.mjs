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

test("parses a portable recipe with timed steps and follow-up actions", () => {
  assert.deepEqual(
    propsFrom([
      "```recipe",
      "title: Weeknight pasta",
      "servings: 4 servings",
      "notes: Salt the pasta water generously.",
      "ingredients:",
      "- 400 g spaghetti",
      "- 2 tbsp olive oil",
      "action: Scale to 2 | Scale this recipe to 2 servings",
      "---",
      "label: Boil pasta",
      "duration: 10m",
      "body: Cook until al dente.",
      "---",
      "label: Finish sauce",
      "duration: 1m 30s",
      "```",
    ].join("\n")),
    {
      title: "Weeknight pasta",
      yield: "4 servings",
      notes: "Salt the pasta water generously.",
      ingredients: ["400 g spaghetti", "2 tbsp olive oil"],
      actions: [{ label: "Scale to 2", intent: "Scale this recipe to 2 servings" }],
      steps: [
        { id: "step-boil-pasta", label: "Boil pasta", body: "Cook until al dente.", durationLabel: "10m", durationMs: 600000 },
        { id: "step-finish-sauce", label: "Finish sauce", durationLabel: "1m 30s", durationMs: 90000 },
      ],
    },
  );
});
