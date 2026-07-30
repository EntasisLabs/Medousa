<script lang="ts">
  import { onMount } from "svelte";
  import { Columns2, FileCode2, X } from "@lucide/svelte";
  import {
    codeWorkspace,
    type CodeDocumentTab,
  } from "$lib/stores/codeWorkspace.svelte";

  interface Props {
    tabs: CodeDocumentTab[];
    activeTabId: string | null;
    tabLabel: (tab: CodeDocumentTab) => string;
    onActivate: (tab: CodeDocumentTab) => void;
    onClose: (tab: CodeDocumentTab) => void;
    onOpenToSide: (tab: CodeDocumentTab) => void;
    onCopyPath: (tab: CodeDocumentTab) => void;
  }

  let {
    tabs,
    activeTabId,
    tabLabel,
    onActivate,
    onClose,
    onOpenToSide,
    onCopyPath,
  }: Props = $props();

  let menuEl = $state<HTMLDivElement | null>(null);
  let menuOpen = $state(false);
  let menuX = $state(0);
  let menuY = $state(0);
  let menuTab = $state<CodeDocumentTab | null>(null);
  let dragTabId = $state<string | null>(null);

  function clampPosition(x: number, y: number): { x: number; y: number } {
    if (typeof window === "undefined") return { x, y };
    const width = menuEl?.offsetWidth ?? 180;
    const height = menuEl?.offsetHeight ?? 160;
    const margin = 8;
    return {
      x: Math.min(Math.max(margin, x), window.innerWidth - width - margin),
      y: Math.min(Math.max(margin, y), window.innerHeight - height - margin),
    };
  }

  const position = $derived(clampPosition(menuX, menuY));

  function openMenu(event: MouseEvent, tab: CodeDocumentTab) {
    event.preventDefault();
    event.stopPropagation();
    menuTab = tab;
    menuX = event.clientX;
    menuY = event.clientY;
    menuOpen = true;
  }

  function closeMenu() {
    menuOpen = false;
    menuTab = null;
  }

  function onMiddleClick(event: MouseEvent, tab: CodeDocumentTab) {
    if (event.button !== 1) return;
    event.preventDefault();
    onClose(tab);
  }

  function closeOthers() {
    if (!menuTab) return;
    const keepId = menuTab.tabId;
    closeMenu();
    for (const tab of [...tabs]) {
      if (tab.tabId !== keepId) onClose(tab);
    }
  }

  function closeToRight() {
    if (!menuTab) return;
    const index = tabs.findIndex((tab) => tab.tabId === menuTab!.tabId);
    closeMenu();
    if (index < 0) return;
    for (const tab of tabs.slice(index + 1)) onClose(tab);
  }

  function onDragStart(event: DragEvent, tab: CodeDocumentTab) {
    dragTabId = tab.tabId;
    event.dataTransfer?.setData("text/plain", tab.tabId);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  }

  function onDragOver(event: DragEvent) {
    if (!dragTabId) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  }

  function onDrop(event: DragEvent, target: CodeDocumentTab) {
    event.preventDefault();
    const sourceId = dragTabId ?? event.dataTransfer?.getData("text/plain");
    dragTabId = null;
    if (!sourceId || sourceId === target.tabId) return;
    const from = tabs.findIndex((tab) => tab.tabId === sourceId);
    const to = tabs.findIndex((tab) => tab.tabId === target.tabId);
    if (from < 0 || to < 0) return;
    codeWorkspace.reorderTab(sourceId, to);
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") closeMenu();
  }

  function onWindowPointerDown(event: PointerEvent) {
    if (!menuOpen) return;
    if (menuEl?.contains(event.target as Node)) return;
    closeMenu();
  }

  onMount(() => {
    window.addEventListener("keydown", onWindowKeydown);
    window.addEventListener("pointerdown", onWindowPointerDown, true);
    return () => {
      window.removeEventListener("keydown", onWindowKeydown);
      window.removeEventListener("pointerdown", onWindowPointerDown, true);
    };
  });
</script>

<div
  class="flex shrink-0 items-end overflow-x-auto border-b border-surface-500/30 bg-surface-950/65 px-1 pt-1"
  role="tablist"
  aria-label="Open files"
>
  {#each tabs as tab (tab.tabId)}
    {@const selected = activeTabId === tab.tabId}
    <div
      class="group flex max-w-52 shrink-0 items-center border border-b-0 {selected
        ? 'border-surface-500/45 bg-surface-900 text-surface-100'
        : 'border-transparent text-surface-500 hover:bg-surface-900/60 hover:text-surface-300'}"
      role="presentation"
      draggable="true"
      ondragstart={(event) => onDragStart(event, tab)}
      ondragover={onDragOver}
      ondrop={(event) => onDrop(event, tab)}
      ondragend={() => (dragTabId = null)}
      oncontextmenu={(event) => openMenu(event, tab)}
      onauxclick={(event) => onMiddleClick(event, tab)}
    >
      <button
        type="button"
        role="tab"
        aria-selected={selected}
        class="flex min-w-0 flex-1 items-center gap-1.5 px-2 py-1.5 text-left text-[10px]"
        title={tab.path}
        onclick={() => onActivate(tab)}
      >
        <FileCode2 size={11} class="shrink-0 opacity-70" />
        <span class="truncate">{tabLabel(tab)}</span>
        {#if codeWorkspace.isDirty(tab)}
          <span class="size-1.5 shrink-0 rounded-full bg-primary-300" aria-label="Unsaved changes"></span>
        {/if}
      </button>
      <button
        type="button"
        class="mr-1 rounded p-0.5 opacity-60 hover:bg-surface-700 focus:opacity-100 sm:opacity-0 sm:group-hover:opacity-100"
        aria-label={`Close ${tab.title}`}
        onclick={() => onClose(tab)}
      ><X size={10} /></button>
      {#if !selected}
        <button
          type="button"
          class="mr-1 rounded p-0.5 opacity-60 hover:bg-surface-700 focus:opacity-100 sm:opacity-0 sm:group-hover:opacity-100"
          aria-label={`Open ${tab.title} to the side`}
          title="Open to side"
          onclick={() => onOpenToSide(tab)}
        ><Columns2 size={10} /></button>
      {/if}
    </div>
  {/each}
</div>

{#if menuOpen && menuTab}
  <div
    bind:this={menuEl}
    class="vault-context-menu"
    role="menu"
    style:left="{position.x}px"
    style:top="{position.y}px"
  >
    <button type="button" class="vault-context-menu-item" role="menuitem" onclick={() => { const tab = menuTab; closeMenu(); if (tab) onClose(tab); }}>Close</button>
    <button type="button" class="vault-context-menu-item" role="menuitem" disabled={tabs.length < 2} onclick={closeOthers}>Close Others</button>
    <button type="button" class="vault-context-menu-item" role="menuitem" disabled={!menuTab || tabs[tabs.length - 1]?.tabId === menuTab.tabId} onclick={closeToRight}>Close to the Right</button>
    <div class="vault-context-menu-sep" aria-hidden="true"></div>
    <button type="button" class="vault-context-menu-item" role="menuitem" disabled={activeTabId === menuTab.tabId} onclick={() => { const tab = menuTab; closeMenu(); if (tab) onOpenToSide(tab); }}>Open to the Side</button>
    <button type="button" class="vault-context-menu-item" role="menuitem" onclick={() => { const tab = menuTab; closeMenu(); if (tab) onCopyPath(tab); }}>Copy Path</button>
  </div>
{/if}
