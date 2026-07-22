<script lang="ts">
  /**
   * `feed` organism — renders last-good feed output once on mount (or manual refresh).
   */
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import Media from "$lib/liquid/archetypes/atoms/media/Media.svelte";
  import CodeEmbed from "$lib/liquid/archetypes/molecules/code/Code.svelte";
  import { createNode } from "$lib/liquid/core";
  import {
    csvToMarkdownTable,
    prettyJsonBody,
    readFeedLatestGood,
    type FeedDatatype,
    type FeedLatestGoodResult,
  } from "$lib/liquid/feeds/feedLatestGood";
  import { formatFeedEmittedHint } from "$lib/liquid/feeds/feedTail";

  type FeedStatus = "idle" | "loading" | "ready" | "empty" | "error" | "stale";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const feedId = $derived(
    typeof node.props.feedId === "string" ? node.props.feedId.trim() : "",
  );
  const expectedDatatype = $derived.by((): FeedDatatype => {
    const raw = typeof node.props.datatype === "string" ? node.props.datatype.trim().toLowerCase() : "text";
    if (raw === "md" || raw === "text" || raw === "json" || raw === "csv" || raw === "image") {
      return raw;
    }
    return "text";
  });
  const title = $derived(typeof node.props.title === "string" ? node.props.title.trim() : "");
  const emptyLabel = $derived(
    typeof node.props.empty === "string" && node.props.empty.trim()
      ? node.props.empty.trim()
      : "No feed output yet",
  );
  const refreshMode = $derived.by(() => {
    const raw = typeof node.props.refresh === "string" ? node.props.refresh.trim().toLowerCase() : "load";
    return raw === "manual" ? "manual" : "load";
  });

  let status = $state<FeedStatus>("idle");
  let result = $state<FeedLatestGoodResult | null>(null);
  let inFlight = false;

  async function hydrate(force = false) {
    if (!feedId || inFlight) return;
    if (!force && status === "ready" && result) return;
    inFlight = true;
    status = result ? "stale" : "loading";
    const next = await readFeedLatestGood(feedId, expectedDatatype);
    inFlight = false;
    if (!next) {
      status = result ? "stale" : "empty";
      return;
    }
    result = next;
    status = "ready";
  }

  $effect(() => {
    const id = feedId;
    const mode = refreshMode;
    result = null;
    status = "idle";
    if (!id) {
      status = "error";
      return;
    }
    if (mode === "load") void hydrate(true);
  });

  const renderDatatype = $derived(result?.datatype ?? expectedDatatype);
  const finishedHint = $derived(
    result?.finishedAt ? formatFeedEmittedHint(result.finishedAt) : "",
  );

  const markdownBody = $derived.by(() => {
    if (!result?.body) return "";
    if (renderDatatype === "csv") return csvToMarkdownTable(result.body);
    return result.body;
  });

  const jsonBody = $derived(result?.body ? prettyJsonBody(result.body) : "");

  const mediaNode = $derived.by(() => {
    if (!result?.body || renderDatatype !== "image") return null;
    return createNode({
      id: "feed-media",
      type: "media",
      props: {
        src: result.body,
        ...(title ? { alt: title } : {}),
      },
      fillState: "ready",
    });
  });

  const codeNode = $derived.by(() => {
    if (!jsonBody || renderDatatype !== "json") return null;
    return createNode({
      id: "feed-json",
      type: "code",
      props: {
        source: jsonBody,
        lang: "json",
        copy: true,
      },
      fillState: "ready",
    });
  });
</script>

<section class="liquid-feed" aria-live="polite">
  <header class="liquid-feed__head">
    <div class="liquid-feed__titles">
      {#if title}
        <h3 class="liquid-feed__title">{title}</h3>
      {/if}
      <p class="liquid-feed__meta">
        <span class="liquid-feed__id">{feedId}</span>
        {#if finishedHint}
          <span class="liquid-feed__hint">{finishedHint}</span>
        {/if}
      </p>
    </div>
    {#if refreshMode === "manual"}
      <button
        type="button"
        class="liquid-feed__refresh"
        disabled={inFlight}
        onclick={() => void hydrate(true)}
      >
        {inFlight ? "Refreshing…" : "Refresh"}
      </button>
    {/if}
  </header>

  {#if status === "loading"}
    <p class="liquid-feed__placeholder">Loading feed…</p>
  {:else if status === "empty"}
    <p class="liquid-feed__placeholder">{emptyLabel}</p>
  {:else if status === "error"}
    <p class="liquid-feed__placeholder liquid-feed__placeholder--error">Invalid feed id</p>
  {:else if status === "stale" && !result}
    <p class="liquid-feed__placeholder">{emptyLabel}</p>
  {:else if result}
    <div class="liquid-feed__body" class:liquid-feed__body--text={renderDatatype === "text"}>
      {#if renderDatatype === "md" || renderDatatype === "csv"}
        <MarkdownContent
          content={markdownBody}
          titleByPath={ctx.titleByPath}
          openLinksInWeb={ctx.openLinksInWeb ?? false}
        />
      {:else if renderDatatype === "text"}
        <pre class="liquid-feed__text">{result.body}</pre>
      {:else if renderDatatype === "json" && codeNode}
        <CodeEmbed node={codeNode} />
      {:else if renderDatatype === "image" && mediaNode}
        <Media node={mediaNode} />
      {:else}
        <pre class="liquid-feed__text">{result.body}</pre>
      {/if}
    </div>
  {/if}
</section>

<style>
  .liquid-feed {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    min-width: 0;
  }

  .liquid-feed__head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .liquid-feed__titles {
    min-width: 0;
  }

  .liquid-feed__title {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 600;
  }

  .liquid-feed__meta {
    margin: 0.15rem 0 0;
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    font-size: 0.72rem;
    color: rgb(var(--color-surface-500));
  }

  .liquid-feed__id {
    font-family: var(--font-mono, ui-monospace, monospace);
  }

  .liquid-feed__hint {
    opacity: 0.85;
  }

  .liquid-feed__refresh {
    flex: 0 0 auto;
    border: 1px solid rgb(var(--color-surface-300));
    background: rgb(var(--color-surface-100));
    color: rgb(var(--color-surface-700));
    border-radius: 0.45rem;
    padding: 0.25rem 0.55rem;
    font-size: 0.72rem;
    cursor: pointer;
  }

  .liquid-feed__refresh:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .liquid-feed__placeholder {
    margin: 0;
    font-size: 0.82rem;
    color: rgb(var(--color-surface-500));
  }

  .liquid-feed__placeholder--error {
    color: rgb(var(--color-error-500, 220 38 38));
  }

  .liquid-feed__body {
    min-width: 0;
  }

  .liquid-feed__text {
    margin: 0;
    white-space: pre-wrap;
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 0.82rem;
    line-height: 1.5;
  }
</style>
