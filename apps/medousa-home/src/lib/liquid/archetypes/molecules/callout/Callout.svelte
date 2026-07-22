<script lang="ts">
  /** `callout` molecule — GitHub-style aside (accent bar + icon title). */
  import { ChevronDown } from "@lucide/svelte";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import { renderInlineMarkdown } from "$lib/markdown";
  import {
    calloutDefaultTitle,
    calloutIconForTone,
    calloutIconSvg,
  } from "$lib/styles/calloutIcons";

  type Tone = "note" | "warn" | "error" | "success" | "tip" | "important";
  const TONES: Tone[] = ["note", "warn", "error", "success", "tip", "important"];

  let { node }: ArchetypeProps = $props();

  const tone = $derived<Tone>(
    TONES.includes(node.props.tone as Tone) ? (node.props.tone as Tone) : "note",
  );
  const titleRaw = $derived(typeof node.props.title === "string" ? node.props.title.trim() : "");
  const title = $derived(titleRaw || calloutDefaultTitle(tone));
  const body = $derived(typeof node.props.body === "string" ? node.props.body : "");
  const detail = $derived(
    typeof node.props.detail === "string" ? node.props.detail.trim() : "",
  );
  const iconHtml = $derived(calloutIconSvg(calloutIconForTone(tone)));
</script>

<aside
  class="liquid-callout"
  data-tone={tone}
  role={tone === "error" || tone === "warn" ? "alert" : undefined}
>
  <div class="liquid-callout-header">
    <span class="liquid-callout-icon">{@html iconHtml}</span>
    <p class="liquid-callout-title">{@html renderInlineMarkdown(title)}</p>
  </div>
  {#if body}
    <div class="liquid-callout-body">{@html renderInlineMarkdown(body)}</div>
  {/if}
  {#if detail}
    <details class="liquid-callout-details group/err-detail">
      <summary
        class="mt-1.5 flex cursor-pointer list-none items-center gap-1 text-[10px] font-medium text-surface-400 marker:content-none hover:text-surface-200"
      >
        <span>View details</span>
        <ChevronDown
          class="h-3 w-3 shrink-0 transition-transform duration-200 group-open/err-detail:rotate-180"
          strokeWidth={2}
          aria-hidden="true"
        />
      </summary>
      <pre
        class="liquid-callout-detail-body mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap break-words font-mono text-[10px] leading-relaxed text-surface-300"
      >{detail}</pre>
    </details>
  {/if}
</aside>
