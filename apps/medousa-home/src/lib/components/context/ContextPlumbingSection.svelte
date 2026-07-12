<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title?: string;
    lead?: string;
    /** Close the door when the selected entry changes. */
    resetKey?: string | null;
    children: Snippet;
  }

  let {
    title = "Advanced",
    lead = "IDs, signals, raw capture",
    resetKey = null,
    children,
  }: Props = $props();

  let open = $state(false);

  $effect(() => {
    resetKey;
    open = false;
  });
</script>

<section class="context-plumbing">
  <button
    type="button"
    class="context-plumbing-toggle"
    onclick={() => (open = !open)}
    aria-expanded={open}
  >
    <span class="min-w-0">
      <span class="context-plumbing-title">{title}</span>
      <span class="context-plumbing-lead">{lead}</span>
    </span>
    <span class="context-plumbing-chevron" aria-hidden="true">{open ? "▾" : "▸"}</span>
  </button>
  {#if open}
    <div class="context-plumbing-body">
      {@render children()}
    </div>
  {/if}
</section>
