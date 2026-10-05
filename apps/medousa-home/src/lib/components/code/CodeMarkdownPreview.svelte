<script lang="ts">
  import "$lib/styles/markdown-content.postcss";
  import { tick } from "svelte";
  import { renderMarkdownPreview } from "$lib/markdown/render";
  import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";
  import { hydrateMarkdownContainer, destroyMarkdownContainer } from "$lib/markdown/hydrateMarkdownContainer";
  import { projectMarkdownTarget, type ProjectMarkdownImages } from "$lib/code/codeMarkdownDocument";

  let { content, path, workId, images, onOpenLocation }: { content: string; path: string; workId: string; images: ProjectMarkdownImages; onOpenLocation: (path: string, line: number) => void } = $props();
  let root = $state<HTMLDivElement>();
  function renderProjectMarkdown(content: string, path: string) {
    const html = renderMarkdownPreview(stripFrontmatter(content).content, { sourcePath: path, interactiveTasks: false, resolveLocalImages: true });
    if (typeof document === "undefined") return html;
    // Raw HTML images also belong to the project. Template contents are inert.
    const template = document.createElement("template");
    template.innerHTML = html;
    for (const image of template.content.querySelectorAll<HTMLImageElement>("img[src]")) {
      const src = image.getAttribute("src") ?? "";
      if (/^(https?:|data:image\/|blob:)/i.test(src)) continue;
      image.removeAttribute("src");
      image.dataset.localImage = src;
    }
    return template.innerHTML;
  }
  const html = $derived(renderProjectMarkdown(content, path));
  $effect(() => {
    void html;
    const container = root;
    if (!container) return;
    let cancelled = false;
    void tick().then(async () => {
      if (cancelled) return;
      await hydrateMarkdownContainer(container, { localImages: false, liquidContext: { localImagePath: null }, animate: false });
      if (cancelled) { await destroyMarkdownContainer(container); return; }
      for (const image of container.querySelectorAll<HTMLImageElement>("img[data-local-image]")) {
        const raw = image.dataset.localImage ?? "";
        void images.resolve(raw).then((url) => { if (!cancelled && image.isConnected) image.src = url; }).catch(() => {
          if (cancelled) return;
          image.replaceWith(Object.assign(document.createElement("span"), { textContent: `Image unavailable: ${image.alt || raw}`, className: "text-content-tertiary text-xs" }));
        });
      }
    });
    return () => { cancelled = true; void destroyMarkdownContainer(container); };
  });

  function navigate(event: MouseEvent) {
    if (!(event.target instanceof Element)) return;
    const anchor = event.target.closest<HTMLAnchorElement>("a[href]");
    if (!anchor) return;
    const href = anchor.getAttribute("href") ?? "";
    const target = projectMarkdownTarget(path, href);
    if (!target) {
      event.preventDefault();
      if (/^https?:/i.test(href)) void import("$lib/utils/openInBrowser").then(({ openInBrowser }) => openInBrowser(href, { workCardId: workId }));
      return;
    }
    event.preventDefault();
    if (target.path === path && target.hash) {
      let id = target.hash;
      try { id = decodeURIComponent(id); } catch { return; }
      const heading = [...(root?.querySelectorAll<HTMLElement>("[id]") ?? [])].find((el) => el.id === id);
      heading?.scrollIntoView({ block: "start" });
    } else onOpenLocation(target.path, 1);
  }
</script>

<!-- The delegated handler routes rendered anchors; their native keyboard behavior is preserved. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<div class="code-markdown-preview h-full overflow-auto px-8 py-7" role="region" aria-label="Markdown preview" onclick={navigate}>
  {#key html}<div class="markdown-content mx-auto max-w-3xl" bind:this={root}>{@html html}</div>{/key}
</div>

<style>
  .code-markdown-preview > .markdown-content { max-width: 48rem; margin-inline: auto; }
</style>
