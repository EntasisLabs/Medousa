<script lang="ts">
  import { onMount, tick, type Snippet } from "svelte";
  import { restoreCodeBlockState } from "$lib/markdown/codeBlocks";

  let { children, userId, keepMounted = false }: { children: Snippet; userId?: string; keepMounted?: boolean } = $props();
  let element = $state<HTMLDivElement>();
  let body = $state<HTMLDivElement>();
  let nearby = $state(typeof IntersectionObserver === "undefined");
  let height = $state(260);
  const mounted = $derived(nearby || keepMounted);
  let detailsOpen: boolean[] = [];
  let codeChoices: Array<{ wrapped?: string; expanded?: string; scrollTop: number; scrollLeft: number }> = [];

  function captureChoices() {
    if (!body) return;
    detailsOpen = [...body.querySelectorAll("details")].map(details => details.open);
    codeChoices = [...body.querySelectorAll<HTMLElement>(".markdown-code-block")].map(block => ({
      wrapped: block.dataset.codeWrapped, expanded: block.dataset.codeExpanded,
      scrollTop: block.querySelector("pre")?.scrollTop ?? 0,
      scrollLeft: block.querySelector("pre")?.scrollLeft ?? 0,
    }));
  }

  onMount(() => {
    const root = element?.closest(".chat-scroll, .mobile-chat-scroll, .vault-workshop-chat-scroll, .mobile-you-scroll") as HTMLElement | null;
    if (!element || !root || typeof IntersectionObserver === "undefined") { nearby = true; return; }
    const observer = new IntersectionObserver(entries => {
      for (const entry of entries) {
        if (!entry.isIntersecting && body) {
          height = Math.max(1, element!.getBoundingClientRect().height);
          captureChoices();
        }
        nearby = entry.isIntersecting;
      }
    }, { root, rootMargin: "800px 0px", threshold: 0 });
    observer.observe(element);
    return () => observer.disconnect();
  });

  $effect(() => {
    const node = body;
    if (!node || typeof ResizeObserver === "undefined") return;
    const measure = () => { if (element && node.isConnected) height = Math.max(1, element.getBoundingClientRect().height); };
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    void tick().then(() => {
      if (!node.isConnected) return;
      node.querySelectorAll("details").forEach((details, index) => {
        if (detailsOpen[index] !== undefined) details.open = detailsOpen[index];
      });
      // Historical beats are immutable; retain choices by block order without copying code text.
      restoreCodeBlockState(node, codeChoices.map(choice => ({ ...choice, source: "" })));
      measure();
    });
    return () => observer.disconnect();
  });
</script>

<div bind:this={element} class="virtual-chat-beat" data-chat-history-anchor data-chat-turn-user-id={userId} style:height={mounted ? undefined : `${height}px`}>
  {#if mounted}<div bind:this={body} class="virtual-chat-beat-body">{@render children()}</div>{/if}
</div>

<style>
  .virtual-chat-beat, .virtual-chat-beat-body { display: flow-root; }
</style>
