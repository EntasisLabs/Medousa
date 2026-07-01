<script lang="ts">
  import { onMount } from "svelte";
  import { humanBrowserEmbedCoordProbe, humanBrowserEmbedHide } from "$lib/humanBrowser";
  import { isTauri } from "$lib/platform";
  import {
    chromeStackReport,
    depthColor,
    isShellLayoutDebugEnabled,
    scanShellLayout,
    setShellLayoutDebugEnabled,
    type ShellDebugBox,
  } from "$lib/utils/shellLayoutDebug";

  interface Props {
    rootEl: HTMLElement | null;
  }

  let { rootEl }: Props = $props();

  let active = $state(isShellLayoutDebugEnabled());
  let boxes = $state<ShellDebugBox[]>([]);
  let report = $state<ReturnType<typeof chromeStackReport> | null>(null);
  let coordProbe = $state<Record<string, unknown> | null>(null);
  let scanGen = 0;

  async function rescan() {
    if (!active || !rootEl) {
      boxes = [];
      report = null;
      coordProbe = null;
      return;
    }
    const gen = ++scanGen;
    requestAnimationFrame(async () => {
      if (gen !== scanGen || !active) return;
      const next = scanShellLayout(rootEl);
      boxes = next;
      report = chromeStackReport(next);
      const host = next.find((b) => b.isEmbedHost);
      if (isTauri() && host) {
        coordProbe = await humanBrowserEmbedCoordProbe({
          x: host.rect.x,
          y: host.rect.y,
          width: host.rect.w,
          height: host.rect.h,
        }).catch(() => null);
      } else {
        coordProbe = null;
      }
    });
  }

  function toggle(on: boolean) {
    active = on;
    setShellLayoutDebugEnabled(on);
    if (on && isTauri()) void humanBrowserEmbedHide();
    rescan();
  }

  onMount(() => {
    if (active && isTauri()) void humanBrowserEmbedHide();

    const onKey = (event: KeyboardEvent) => {
      if (!import.meta.env.DEV) return;
      if (!(event.ctrlKey || event.metaKey) || !event.shiftKey || event.key.toLowerCase() !== "l") return;
      event.preventDefault();
      toggle(!active);
    };

    const ro =
      rootEl && typeof ResizeObserver !== "undefined"
        ? new ResizeObserver(() => rescan())
        : null;
    ro?.observe(rootEl!);

    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", rescan);
    window.addEventListener("scroll", rescan, true);
    rescan();

    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", rescan);
      window.removeEventListener("scroll", rescan, true);
      ro?.disconnect();
    };
  });

  $effect(() => {
    rootEl;
    active;
    void rescan();
  });

  function fmtBox(box: Record<string, number>): string {
    return `${Math.round(box.x)},${Math.round(box.y)} ${Math.round(box.width)}×${Math.round(box.height)}`;
  }
</script>

{#if active}
  <div
    class="shell-layout-debug-root"
    data-shell-layout-debug-root
    aria-hidden="true"
  >
    {#each boxes as box (box.key)}
      {@const color = depthColor(box.depth)}
      <!-- content box -->
      <div
        class="shell-layout-debug-box"
        style="
          left:{box.rect.x}px;
          top:{box.rect.y}px;
          width:{box.rect.w}px;
          height:{box.rect.h}px;
          border-color:{color};
          background:{box.isEmbedHost ? 'rgb(16 185 129 / 0.12)' : box.isChrome ? 'rgb(59 130 246 / 0.10)' : 'transparent'};
        "
      >
        <span class="shell-layout-debug-label" style="background:{color}">
          {box.label}
          <em>y{box.rect.y} h{box.rect.h}</em>
          {#if box.padding.top || box.padding.bottom || box.padding.left || box.padding.right}
            <em>pad {box.padding.top}/{box.padding.right}/{box.padding.bottom}/{box.padding.left}</em>
          {/if}
          {#if box.margin.top || box.margin.bottom || box.margin.left || box.margin.right}
            <em>m {box.margin.top}/{box.margin.right}/{box.margin.bottom}/{box.margin.left}</em>
          {/if}
        </span>
      </div>

      <!-- margin ghost (outside content box) -->
      {#if box.margin.top || box.margin.right || box.margin.bottom || box.margin.left}
        <div
          class="shell-layout-debug-margin"
          style="
            left:{box.rect.x - box.margin.left}px;
            top:{box.rect.y - box.margin.top}px;
            width:{box.rect.w + box.margin.left + box.margin.right}px;
            height:{box.rect.h + box.margin.top + box.margin.bottom}px;
          "
        ></div>
      {/if}
    {/each}

    <aside class="shell-layout-debug-panel">
      <header>
        <strong>Shell layout trace</strong>
        <span class="shell-layout-debug-hint">⌃⇧L off · native webview hidden</span>
        <button type="button" class="shell-layout-debug-off" onclick={() => toggle(false)}>Close</button>
      </header>

      {#if report}
        <section class="shell-layout-debug-summary">
          <div>
            <span>chrome bottom</span>
            <code>{report.chromeBottom ?? "—"}</code>
          </div>
          <div>
            <span>embed host top</span>
            <code>{report.embedHostTop ?? "—"}</code>
          </div>
          <div>
            <span>gap (host − chrome)</span>
            <code class={report.gap && report.gap !== 0 ? "shell-layout-debug-warn" : ""}>
              {report.gap ?? "—"}px
            </code>
          </div>
          <div>
            <span>boxes marked</span>
            <code>{boxes.length}</code>
          </div>
        </section>

        {#if coordProbe}
          {@const frames = coordProbe.frames as Record<string, Record<string, number>> | undefined}
          {@const deltas = coordProbe.deltas as Record<string, number | null> | undefined}
          <section class="shell-layout-debug-coords">
            <p class="shell-layout-debug-coords-title">Coordinate frames (DOM vs Tauri vs native)</p>
            {#if frames?.domViewport}
              <div><span>DOM viewport (what compositor sends)</span><code>{fmtBox(frames.domViewport)}</code></div>
            {/if}
            {#if frames?.workshopLayout}
              <div><span>Workshop Rust (old math)</span><code>{fmtBox(frames.workshopLayout)}</code></div>
            {/if}
            {#if frames?.contentTauri}
              <div><span>Content Tauri readback</span><code>{fmtBox(frames.contentTauri)}</code></div>
            {/if}
            {#if frames?.contentNativeInWindow}
              <div><span>Content NSView in window</span><code>{fmtBox(frames.contentNativeInWindow)}</code></div>
            {/if}
            {#if deltas?.workshopYMinusDomY != null}
              <div>
                <span>workshop.y − dom.y</span>
                <code class="shell-layout-debug-warn">{deltas.workshopYMinusDomY}px</code>
              </div>
            {/if}
            {#if deltas?.workshopBottomMinusDomBottom != null}
              <div>
                <span>workshop bottom − dom bottom</span>
                <code class="shell-layout-debug-warn">{deltas.workshopBottomMinusDomBottom}px</code>
              </div>
            {/if}
          </section>
        {/if}

        <ol class="shell-layout-debug-stack">
          {#each report.rows as row, i (row.label + row.y + i)}
            <li>
              <span class="shell-layout-debug-idx">{i + 1}</span>
              <span class="shell-layout-debug-name">{row.label}</span>
              <span class="shell-layout-debug-metrics">
                y{row.y} h{row.h} → {row.bottom}
                {#if row.pad !== "—"} · pad {row.pad}{/if}
                {#if row.margin !== "—"} · m {row.margin}{/if}
              </span>
            </li>
          {/each}
        </ol>
      {/if}
    </aside>
  </div>
{/if}

<style>
  .shell-layout-debug-root {
    position: fixed;
    inset: 0;
    z-index: 99999;
    pointer-events: none;
  }

  .shell-layout-debug-box {
    position: fixed;
    box-sizing: border-box;
    border: 2px solid;
    pointer-events: none;
  }

  .shell-layout-debug-margin {
    position: fixed;
    box-sizing: border-box;
    border: 1px dashed rgb(250 204 21 / 0.75);
    background: rgb(250 204 21 / 0.06);
    pointer-events: none;
  }

  .shell-layout-debug-label {
    position: absolute;
    left: 0;
    top: 0;
    max-width: min(100%, 22rem);
    padding: 1px 4px;
    font: 10px/1.25 ui-monospace, SFMono-Regular, Menlo, monospace;
    color: rgb(15 23 42);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .shell-layout-debug-label em {
    font-style: normal;
    opacity: 0.85;
    margin-left: 4px;
  }

  .shell-layout-debug-panel {
    position: fixed;
    right: 8px;
    top: 8px;
    width: min(26rem, calc(100vw - 16px));
    max-height: calc(100vh - 16px);
    overflow: auto;
    pointer-events: auto;
    border-radius: 8px;
    border: 1px solid rgb(51 65 85);
    background: rgb(2 6 23 / 0.94);
    color: rgb(226 232 240);
    font: 11px/1.35 ui-monospace, SFMono-Regular, Menlo, monospace;
    box-shadow: 0 8px 32px rgb(0 0 0 / 0.45);
  }

  .shell-layout-debug-panel header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
    padding: 8px 10px;
    border-bottom: 1px solid rgb(51 65 85);
  }

  .shell-layout-debug-hint {
    color: rgb(148 163 184);
    font-size: 10px;
  }

  .shell-layout-debug-off {
    margin-left: auto;
    border-radius: 4px;
    border: 1px solid rgb(71 85 105);
    background: rgb(30 41 59);
    color: inherit;
    padding: 2px 8px;
    cursor: pointer;
  }

  .shell-layout-debug-summary {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px 10px;
    padding: 8px 10px;
    border-bottom: 1px solid rgb(51 65 85);
  }

  .shell-layout-debug-summary span {
    display: block;
    color: rgb(148 163 184);
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .shell-layout-debug-coords {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border-bottom: 1px solid rgb(51 65 85);
  }

  .shell-layout-debug-coords-title {
    margin: 0;
    color: rgb(251 191 36);
    font-size: 10px;
    font-weight: 600;
  }

  .shell-layout-debug-coords div {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 8px;
    align-items: baseline;
  }

  .shell-layout-debug-coords span {
    color: rgb(148 163 184);
    font-size: 10px;
  }

  .shell-layout-debug-warn {
    color: rgb(250 204 21);
  }

  .shell-layout-debug-stack {
    list-style: none;
    margin: 0;
    padding: 6px 0;
  }

  .shell-layout-debug-stack li {
    display: grid;
    grid-template-columns: 1.4rem 1fr;
    gap: 2px 6px;
    padding: 4px 10px;
    border-bottom: 1px solid rgb(30 41 59);
  }

  .shell-layout-debug-idx {
    color: rgb(100 116 139);
  }

  .shell-layout-debug-name {
    color: rgb(125 211 252);
    font-weight: 600;
  }

  .shell-layout-debug-metrics {
    grid-column: 2;
    color: rgb(148 163 184);
    font-size: 10px;
  }
</style>
