import { performance } from "node:perf_hooks";
import { Window } from "happy-dom";
import { marked } from "marked";
import createDOMPurify from "dompurify";

const window = new Window();
const DOMPurify = createDOMPurify(window);

function fixture(targetBytes) {
  const richPrelude = [
    "## Streaming benchmark\n\n",
    "Prose with a [link](https://example.invalid) and `inline code`.\n\n",
    "| column | value |\n| --- | ---: |\n| latency | 42 |\n\n",
    "```rust\nfn main() { println!(\"bounded\"); }\n```\n\n",
    "```mermaid\ngraph TD; provider-->pipeline-->home;\n```\n\n",
    "{% card id=\"synthetic-benchmark\" %}\n\n",
  ].join("");
  const prose =
    "Streaming prose keeps a [reference](https://example.invalid) and `small value` visible while the answer grows.\n\n";
  if (targetBytes <= richPrelude.length) return richPrelude.slice(0, targetBytes);
  return (
    richPrelude + prose.repeat(Math.ceil((targetBytes - richPrelude.length) / prose.length))
  ).slice(0, targetBytes);
}

function percentile(values, percent) {
  const ordered = [...values].sort((a, b) => a - b);
  return ordered[Math.floor((ordered.length - 1) * percent / 100)];
}

function render(source) {
  return DOMPurify.sanitize(marked.parse(source));
}

function result(
  fixtureName,
  source,
  fragmentBytes,
  started,
  startingHeapBytes,
  renderTimes,
  metrics,
) {
  const heap = process.memoryUsage();
  return {
    fixture: fixtureName,
    source_bytes: source.length,
    fragment_bytes: fragmentBytes,
    fragments: Math.ceil(source.length / fragmentBytes),
    elapsed_ms: performance.now() - started,
    render_p50_ms: percentile(renderTimes, 50),
    render_p95_ms: percentile(renderTimes, 95),
    render_p99_ms: percentile(renderTimes, 99),
    heap_delta_bytes: Math.max(0, heap.heapUsed - startingHeapBytes),
    ...metrics,
  };
}

function runWholeAnswer(targetBytes, fragmentBytes) {
  global.gc?.();
  const startingHeapBytes = process.memoryUsage().heapUsed;
  const source = fixture(targetBytes);
  const host = window.document.createElement("main");
  window.document.body.append(host);
  let streamed = "";
  const renderTimes = [];
  const started = performance.now();
  let replacements = 0;
  for (let offset = 0; offset < source.length; offset += fragmentBytes) {
    streamed += source.slice(offset, offset + fragmentBytes);
    const renderStarted = performance.now();
    host.innerHTML = render(streamed);
    renderTimes.push(performance.now() - renderStarted);
    replacements += 1;
  }
  const measured = result(
    "P02-home-whole-answer-render-v1",
    source,
    fragmentBytes,
    started,
    startingHeapBytes,
    renderTimes,
    {
      whole_answer_replacements: replacements,
      tail_replacements: 0,
      completed_block_hydrations: replacements,
      retained_stable_blocks: 0,
      final_dom_nodes: host.querySelectorAll("*").length,
    },
  );
  host.remove();
  return measured;
}

function runStableBlocks(targetBytes, fragmentBytes) {
  global.gc?.();
  const startingHeapBytes = process.memoryUsage().heapUsed;
  const source = fixture(targetBytes);
  const host = window.document.createElement("main");
  window.document.body.append(host);
  const tailHost = window.document.createElement("div");
  tailHost.dataset.streamingMarkdownTail = "";
  host.append(tailHost);
  let streamed = "";
  let committedLength = 0;
  let stableBlocks = 0;
  let tailReplacements = 0;
  const renderTimes = [];
  const started = performance.now();

  const publish = (terminal) => {
    const renderStarted = performance.now();
    const pending = streamed.slice(committedLength);
    const completed = terminal ? (pending ? [pending] : []) : confirmedBlocks(pending);
    for (const block of completed) {
      const stable = window.document.createElement("div");
      stable.dataset.stableMarkdownBlock = "";
      host.insertBefore(stable, tailHost);
      stable.innerHTML = render(block);
      committedLength += block.length;
      stableBlocks += 1;
    }
    tailHost.innerHTML = terminal ? "" : render(streamed.slice(committedLength));
    tailReplacements += 1;
    renderTimes.push(performance.now() - renderStarted);
  };

  for (let offset = 0; offset < source.length; offset += fragmentBytes) {
    streamed += source.slice(offset, offset + fragmentBytes);
    publish(false);
  }
  publish(true);

  const measured = result(
    "P02-home-stable-block-tail-v2",
    source,
    fragmentBytes,
    started,
    startingHeapBytes,
    renderTimes,
    {
      whole_answer_replacements: 0,
      tail_replacements: tailReplacements,
      completed_block_hydrations: stableBlocks,
      retained_stable_blocks: stableBlocks,
      final_dom_nodes: finalRenderedNodeCount(source) + stableBlocks,
    },
  );
  host.remove();
  return measured;
}

function confirmedBlocks(source) {
  if (!source) return [];
  const tokens = marked.lexer(source);
  const contentIndexes = [];
  let count = 0;
  for (let index = tokens.length - 1; index >= 0; index -= 1) {
    if (tokens[index].type === "space") continue;
    contentIndexes.push(index);
    if (contentIndexes.length === 3) {
      count = contentIndexes[1];
      break;
    }
  }
  const stableTokens = tokens.slice(0, count);
  const stableLength = stableTokens.reduce((length, token) => length + token.raw.length, 0);
  const stableSource = source.slice(0, stableLength);
  const blocks = [];
  let offset = 0;
  let blockStart = 0;
  for (let index = 0; index < stableTokens.length; index += 1) {
    offset += stableTokens[index].raw.length;
    const next = stableTokens[index + 1];
    if (
      (stableTokens[index].type !== "space" && next?.type !== "space") ||
      (stableTokens[index].type === "space" && next?.type !== "space")
    ) {
      blocks.push(stableSource.slice(blockStart, offset));
      blockStart = offset;
    }
  }
  if (blockStart < stableSource.length) blocks.push(stableSource.slice(blockStart));
  return blocks.filter(Boolean);
}

function finalRenderedNodeCount(source) {
  const verification = window.document.createElement("main");
  window.document.body.append(verification);
  verification.innerHTML = render(source);
  const count = verification.querySelectorAll("*").length;
  verification.remove();
  return count;
}

const full = process.argv.includes("--full");
const baseline = process.argv.includes("--baseline");
const sizes = full ? [1_000, 10_000, 100_000] : [10_000];
const fragments = full ? [8, 32, 256] : [256];
const run = baseline ? runWholeAnswer : runStableBlocks;
for (const targetBytes of sizes) {
  for (const fragmentBytes of fragments) {
    console.log(JSON.stringify(run(targetBytes, fragmentBytes)));
  }
}
