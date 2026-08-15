import { performance } from "node:perf_hooks";
import { Window } from "happy-dom";
import { marked } from "marked";
import createDOMPurify from "dompurify";

const window = new Window();
const DOMPurify = createDOMPurify(window);

function fixture(targetBytes) {
  const block = [
    "## Streaming benchmark\n\n",
    "Prose with a [link](https://example.invalid) and `inline code`.\n\n",
    "| column | value |\n| --- | ---: |\n| latency | 42 |\n\n",
    "```rust\nfn main() { println!(\"bounded\"); }\n```\n\n",
    "```mermaid\ngraph TD; provider-->pipeline-->home;\n```\n\n",
    "{% card id=\"synthetic-benchmark\" %}\n\n",
  ].join("");
  return block.repeat(Math.ceil(targetBytes / block.length)).slice(0, targetBytes);
}

function percentile(values, percent) {
  const ordered = [...values].sort((a, b) => a - b);
  return ordered[Math.floor((ordered.length - 1) * percent / 100)];
}

function run(targetBytes, fragmentBytes) {
  const source = fixture(targetBytes);
  const host = window.document.createElement("main");
  let streamed = "";
  const renderTimes = [];
  const started = performance.now();
  let replacements = 0;
  for (let offset = 0; offset < source.length; offset += fragmentBytes) {
    streamed += source.slice(offset, offset + fragmentBytes);
    const renderStarted = performance.now();
    const html = DOMPurify.sanitize(marked.parse(streamed));
    host.innerHTML = html;
    renderTimes.push(performance.now() - renderStarted);
    replacements += 1;
  }
  const heap = process.memoryUsage();
  return {
    fixture: "P02-home-whole-answer-render-v1",
    source_bytes: source.length,
    fragment_bytes: fragmentBytes,
    fragments: Math.ceil(source.length / fragmentBytes),
    elapsed_ms: performance.now() - started,
    render_p50_ms: percentile(renderTimes, 50),
    render_p95_ms: percentile(renderTimes, 95),
    render_p99_ms: percentile(renderTimes, 99),
    dom_replacements: replacements,
    final_dom_nodes: host.querySelectorAll("*").length,
    completed_block_hydrations: replacements,
    heap_used_bytes: heap.heapUsed,
  };
}

const full = process.argv.includes("--full");
const sizes = full ? [1_000, 10_000, 100_000] : [10_000];
const fragments = full ? [8, 32, 256] : [256];
for (const targetBytes of sizes) {
  for (const fragmentBytes of fragments) {
    console.log(JSON.stringify(run(targetBytes, fragmentBytes)));
  }
}
