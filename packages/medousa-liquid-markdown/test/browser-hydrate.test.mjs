import assert from "node:assert/strict";
import test from "node:test";

import { Window } from "happy-dom";

import {
  destroyLiquidEmbeds,
  hydrateLiquidMarkdown,
} from "../dist/browser/index.js";
import { preprocessLiquidEmbeds } from "../dist/index.js";

function browserFixture(markdown) {
  const window = new Window();
  const root = window.document.createElement("div");
  root.innerHTML = preprocessLiquidEmbeds(markdown);
  window.document.body.appendChild(root);
  return { window, root };
}

test("hydrates placeholders, nested Markdown, and shared styles", async () => {
  const { window, root } = browserFixture([
    "```report",
    "title: Weekly",
    "",
    "**Opening**",
    "",
    "```chart",
    "type: bar",
    "",
    "| Month | Visits |",
    "| ----- | ------ |",
    "| Jan   | 12     |",
    "| Feb   | 18     |",
    "```",
    "```",
  ].join("\n"));

  const handle = hydrateLiquidMarkdown(root, {
    renderMarkdown(source, target) {
      target.innerHTML = preprocessLiquidEmbeds(source).replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
    },
  });
  await handle.ready;

  assert.ok(root.querySelector('.medousa-liquid[data-liquid-kind="report"]'));
  assert.ok(root.querySelector('.medousa-liquid[data-liquid-kind="chart"] svg'));
  assert.equal(root.querySelector("strong")?.textContent, "Opening");
  assert.ok(window.document.head.querySelector("style[data-medousa-liquid-markdown]"));
  handle.destroy();
});

test("wires portable actions and removes listeners on destroy", async () => {
  const { root } = browserFixture([
    "```actions",
    "Ship it | deploy",
    "```",
  ].join("\n"));
  const intents = [];
  const handle = hydrateLiquidMarkdown(root, {
    onAction(event) {
      intents.push(event.intent);
    },
  });
  await handle.ready;

  const button = root.querySelector("[data-liquid-action]");
  assert.ok(button);
  button.click();
  assert.deepEqual(intents, ["deploy"]);

  destroyLiquidEmbeds(root);
  button.click();
  assert.deepEqual(intents, ["deploy"]);
});

test("switches tabs and loads feed content through host callbacks", async () => {
  const { root } = browserFixture([
    "```tabs",
    "default: Second",
    "",
    "---",
    "label: First",
    "body: One",
    "---",
    "label: Second",
    "body: Two",
    "```",
    "",
    "```feed",
    "id: metrics",
    "datatype: json",
    "refresh: load",
    "```",
    "",
    "```feed",
    "id: summary",
    "datatype: md",
    "refresh: load",
    "```",
  ].join("\n"));
  const handle = hydrateLiquidMarkdown(root, {
    renderMarkdown(source, target) {
      target.innerHTML = source.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
    },
    loadFeed({ feedId }) {
      return feedId === "summary"
        ? { content: "**Live summary**" }
        : { content: { feedId, total: 42 } };
    },
  });
  await handle.ready;

  const second = root.querySelector('[data-liquid-tab="1"]');
  assert.ok(second);
  second.click();
  assert.equal(second.getAttribute("aria-selected"), "true");
  assert.equal(root.querySelector('[data-liquid-tab-panel="0"]')?.hidden, true);
  assert.match(root.querySelector("[data-liquid-feed]")?.textContent ?? "", /"total": 42/);
  assert.equal(root.querySelector('[data-liquid-feed-id="summary"] strong')?.textContent, "Live summary");
  handle.destroy();
});
