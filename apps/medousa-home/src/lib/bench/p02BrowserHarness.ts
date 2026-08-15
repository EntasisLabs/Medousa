import { hydrateMarkdownContainer } from "$lib/markdown/hydrateMarkdownContainer";
import { createMarkdownRenderSession } from "$lib/markdown/render";
import { StreamingMarkdownBlocks } from "$lib/markdown/streamingBlocks";

declare global {
  interface Window {
    __p02Result?: P02BrowserResult;
  }
}

interface P02BrowserResult {
  source_bytes: number;
  fragment_bytes: number;
  fragments: number;
  elapsed_ms: number;
  update_p95_ms: number;
  update_p99_ms: number;
  max_update_ms: number;
  long_tasks: number;
  longest_task_ms: number;
  frame_gap_p95_ms: number;
  max_frame_gap_ms: number;
  task_delay_p95_ms: number;
  max_task_delay_ms: number;
  whole_answer_replacements: 0;
  tail_replacements: number;
  completed_block_hydrations: number;
  retained_stable_blocks: number;
  mount_teardowns: 0;
  final_dom_nodes: number;
  heap_used_bytes: number | null;
}

function fixture(targetBytes: number): string {
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

function percentile(values: number[], percent: number): number {
  if (values.length === 0) return 0;
  const ordered = [...values].sort((a, b) => a - b);
  return ordered[Math.floor((ordered.length - 1) * percent / 100)];
}

function nextFrame(): Promise<number> {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

function positiveIntParam(
  params: URLSearchParams,
  name: string,
  fallback: number,
  maximum: number,
): number {
  const parsed = Number(params.get(name) ?? fallback);
  return Number.isFinite(parsed) && parsed > 0
    ? Math.min(Math.floor(parsed), maximum)
    : fallback;
}

async function run(): Promise<P02BrowserResult> {
  const params = new URLSearchParams(location.search);
  const targetBytes = positiveIntParam(params, "bytes", 10_000, 1_000_000);
  const fragmentBytes = positiveIntParam(params, "fragment", 256, 65_536);
  const source = fixture(targetBytes);
  const host = document.querySelector<HTMLElement>("#host")!;
  const tail = document.createElement("div");
  tail.dataset.streamingMarkdownTail = "";
  host.append(tail);

  const blocks = new StreamingMarkdownBlocks();
  const renderer = createMarkdownRenderSession();
  const updateTimes: number[] = [];
  const frameGaps: number[] = [];
  const taskDelays: number[] = [];
  const hydrationTasks: Promise<void>[] = [];
  const longTasks: number[] = [];
  let streamed = "";
  let stableBlocks = 0;
  let tailReplacements = 0;
  let previousFrame = performance.now();

  const observer = new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) longTasks.push(entry.duration);
  });
  try {
    observer.observe({ type: "longtask", buffered: true });
  } catch {
    // Safari/WebView may not expose Long Tasks; update and frame gaps remain.
  }

  const started = performance.now();
  for (let offset = 0; offset < source.length; offset += fragmentBytes) {
    const frame = await nextFrame();
    frameGaps.push(frame - previousFrame);
    previousFrame = frame;
    const taskScheduled = performance.now();
    setTimeout(() => taskDelays.push(performance.now() - taskScheduled), 0);
    streamed += source.slice(offset, offset + fragmentBytes);

    const updateStarted = performance.now();
    const update = blocks.update(streamed, false);
    for (const markdown of update.completed) {
      const stable = document.createElement("div");
      stable.dataset.stableMarkdownBlock = "";
      stable.innerHTML = renderer.renderStable(markdown);
      host.insertBefore(stable, tail);
      stableBlocks += 1;
      hydrationTasks.push(
        hydrateMarkdownContainer(stable, {
          liquidContext: {},
          localImagePath: null,
          code: true,
          mermaid: true,
          liquid: true,
          localImages: false,
        }),
      );
    }
    tail.innerHTML = renderer.renderTail(update.tail);
    tailReplacements += 1;
    updateTimes.push(performance.now() - updateStarted);
  }

  const terminalStarted = performance.now();
  const terminal = blocks.update(source, true);
  for (const markdown of terminal.completed) {
    const stable = document.createElement("div");
    stable.dataset.stableMarkdownBlock = "";
    stable.innerHTML = renderer.renderStable(markdown);
    host.insertBefore(stable, tail);
    stableBlocks += 1;
    hydrationTasks.push(
      hydrateMarkdownContainer(stable, {
        liquidContext: {},
        localImagePath: null,
        code: true,
        mermaid: true,
        liquid: true,
        localImages: false,
      }),
    );
  }
  tail.innerHTML = "";
  tailReplacements += 1;
  updateTimes.push(performance.now() - terminalStarted);
  await Promise.all(hydrationTasks);
  await nextFrame();
  await new Promise((resolve) => setTimeout(resolve, 0));
  observer.disconnect();

  const memory = performance as Performance & {
    memory?: { usedJSHeapSize: number };
  };
  return {
    source_bytes: source.length,
    fragment_bytes: fragmentBytes,
    fragments: Math.ceil(source.length / fragmentBytes),
    elapsed_ms: performance.now() - started,
    update_p95_ms: percentile(updateTimes, 95),
    update_p99_ms: percentile(updateTimes, 99),
    max_update_ms: Math.max(...updateTimes),
    long_tasks: longTasks.length,
    longest_task_ms: Math.max(0, ...longTasks),
    frame_gap_p95_ms: percentile(frameGaps, 95),
    max_frame_gap_ms: Math.max(0, ...frameGaps),
    task_delay_p95_ms: percentile(taskDelays, 95),
    max_task_delay_ms: Math.max(0, ...taskDelays),
    whole_answer_replacements: 0,
    tail_replacements: tailReplacements,
    completed_block_hydrations: stableBlocks,
    retained_stable_blocks: host.querySelectorAll("[data-stable-markdown-block]").length,
    mount_teardowns: 0,
    final_dom_nodes: host.querySelectorAll("*").length,
    heap_used_bytes: memory.memory?.usedJSHeapSize ?? null,
  };
}

const resultNode = document.querySelector<HTMLElement>("#result")!;
const statusNode = document.querySelector<HTMLElement>("#status")!;
void run()
  .then((result) => {
    window.__p02Result = result;
    resultNode.textContent = JSON.stringify(result, null, 2);
    statusNode.textContent = "P02 complete";
  })
  .catch((error: unknown) => {
    statusNode.textContent = "P02 failed";
    resultNode.textContent = error instanceof Error ? error.stack ?? error.message : String(error);
    throw error;
  });
