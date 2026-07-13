<script lang="ts">
  import { onDestroy } from "svelte";
  import { ExternalLink, X } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { getVaultNote } from "$lib/daemon";
  import { renderMarkdownPreview } from "$lib/markdown/render";
  import { hydrateMarkdownContainer } from "$lib/markdown/hydrateMarkdownContainer";
  import { destroyLiquidEmbeds } from "$lib/markdown/hydrateLiquidEmbeds";
  import { vault } from "$lib/stores/vault.svelte";
  import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";

  interface Props {
    target: string;
    anchor: { top: number; left: number; bottom: number; width: number };
    onClose: () => void;
    onOpen: (target: string) => void;
  }

  let { target, anchor, onClose, onOpen }: Props = $props();

  let loading = $state(true);
  let missing = $state(false);
  let title = $state("");
  let path = $state<string | null>(null);
  let html = $state("");
  let panelEl = $state<HTMLDivElement | null>(null);
  let markdownEl = $state<HTMLDivElement | null>(null);

  const position = $derived.by(() => {
    const width = 320;
    const gap = 8;
    const viewportPad = 12;
    const left = Math.min(
      Math.max(viewportPad, anchor.left),
      Math.max(viewportPad, window.innerWidth - width - viewportPad),
    );
    const preferBelow = anchor.bottom + gap;
    const top =
      preferBelow + 280 > window.innerHeight
        ? Math.max(viewportPad, anchor.top - gap - 280)
        : preferBelow;
    return { top, left, width };
  });

  $effect(() => {
    const raw = target;
    loading = true;
    missing = false;
    title = raw;
    path = null;
    html = "";

    const resolved = vault.resolveWikilinkPath(raw);
    if (!resolved) {
      loading = false;
      missing = true;
      return;
    }

    path = resolved;
    let cancelled = false;
    void (async () => {
      try {
        const response = await getVaultNote(resolved);
        if (cancelled) return;
        title = response.note.title || resolved;
        path = response.note.path;
        const body = stripFrontmatter(response.content).content.trim();
        const excerpt =
          body.length > 1600 ? `${body.slice(0, 1600).trimEnd()}…` : body;
        html = renderMarkdownPreview(excerpt || "_Empty note._", {
          titleByPath: new Map(
            vault.notes.map((note) => [note.path, note.title] as const),
          ),
          sourcePath: resolved,
          knownPaths: new Set(vault.notes.map((note) => note.path)),
          interactiveTasks: false,
          resolveLocalImages: true,
        });
      } catch {
        if (!cancelled) missing = true;
      } finally {
        if (!cancelled) loading = false;
      }
    })();

    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    const el = panelEl;
    if (!el) return;
    el.focus();
  });

  $effect(() => {
    html;
    const root = markdownEl;
    const notePath = path;
    if (!root) return;
    void hydrateMarkdownContainer(root, {
      liquidContext: {
        titleByPath: new Map(vault.notes.map((note) => [note.path, note.title] as const)),
        openLinksInWeb: false,
      },
      localImagePath: notePath,
      code: true,
      mermaid: true,
      liquid: true,
      localImages: true,
    });
    return () => destroyLiquidEmbeds(root);
  });

  onDestroy(() => {
    if (markdownEl) destroyLiquidEmbeds(markdownEl);
  });

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }

  function handleOpen() {
    onOpen(target);
    onClose();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<BodyPortal>
  <div
    class="vault-kanban-peek-backdrop"
    role="presentation"
    onclick={onClose}
    onkeydown={(event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        onClose();
      }
    }}
  ></div>
  <div
    bind:this={panelEl}
    class="vault-kanban-peek"
    style:top="{position.top}px"
    style:left="{position.left}px"
    style:width="{position.width}px"
    role="dialog"
    aria-modal="true"
    aria-label="Linked note peek"
    tabindex="-1"
    onclick={(event) => event.stopPropagation()}
    onkeydown={handleKeydown}
  >
    <header class="vault-kanban-peek-header">
      <div class="min-w-0 flex-1">
        <p class="vault-kanban-peek-title">{title}</p>
        {#if path}
          <p class="vault-kanban-peek-path">{path}</p>
        {/if}
      </div>
      <button
        type="button"
        class="vault-kanban-peek-icon"
        aria-label="Close peek"
        onclick={onClose}
      >
        <X size={14} strokeWidth={2} />
      </button>
    </header>

    <div class="vault-kanban-peek-body">
      {#if loading}
        <p class="vault-kanban-peek-status">Loading…</p>
      {:else if missing}
        <p class="vault-kanban-peek-status">
          Note not found. Open to create it.
        </p>
      {:else}
        <div
          bind:this={markdownEl}
          class="markdown-content vault-markdown-preview text-sm leading-relaxed"
        >
          {@html html}
        </div>
      {/if}
    </div>

    <footer class="vault-kanban-peek-footer">
      <button type="button" class="vault-kanban-peek-open" onclick={handleOpen}>
        <ExternalLink size={13} strokeWidth={2} />
        Open note
      </button>
    </footer>
  </div>
</BodyPortal>
