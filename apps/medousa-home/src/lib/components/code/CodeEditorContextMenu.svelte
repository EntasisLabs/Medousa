<script lang="ts">
  import { onMount } from "svelte";

  export type CodeEditorMenuAction =
    | "definition"
    | "references"
    | "rename"
    | "format"
    | "organize_imports"
    | "copy_path"
    | "copy_relative_path"
    | "reveal";

  interface Props {
    open: boolean;
    x: number;
    y: number;
    canDefinition?: boolean;
    canReference?: boolean;
    canRename?: boolean;
    canFormat?: boolean;
    canOrganize?: boolean;
    editable?: boolean;
    onAction: (action: CodeEditorMenuAction) => void;
    onClose: () => void;
  }

  let {
    open,
    x,
    y,
    canDefinition = false,
    canReference = false,
    canRename = false,
    canFormat = false,
    canOrganize = false,
    editable = false,
    onAction,
    onClose,
  }: Props = $props();

  let menuEl = $state<HTMLDivElement | null>(null);

  function clampPosition(px: number, py: number): { x: number; y: number } {
    if (typeof window === "undefined") return { x: px, y: py };
    const width = menuEl?.offsetWidth ?? 200;
    const height = menuEl?.offsetHeight ?? 220;
    const margin = 8;
    return {
      x: Math.min(Math.max(margin, px), window.innerWidth - width - margin),
      y: Math.min(Math.max(margin, py), window.innerHeight - height - margin),
    };
  }

  const position = $derived(clampPosition(x, y));

  function run(action: CodeEditorMenuAction) {
    onAction(action);
    onClose();
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") onClose();
  }

  function onWindowPointerDown(event: PointerEvent) {
    if (!open) return;
    if (menuEl?.contains(event.target as Node)) return;
    onClose();
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

{#if open}
  <div
    bind:this={menuEl}
    class="vault-context-menu"
    role="menu"
    style:left="{position.x}px"
    style:top="{position.y}px"
  >
    <button type="button" class="vault-context-menu-item" role="menuitem" disabled={!canDefinition} onclick={() => run("definition")}>Go to Definition</button>
    <button type="button" class="vault-context-menu-item" role="menuitem" disabled={!canReference} onclick={() => run("references")}>Find Uses</button>
    <button type="button" class="vault-context-menu-item" role="menuitem" disabled={!canRename || !editable} onclick={() => run("rename")}>Rename Symbol…</button>
    <div class="vault-context-menu-sep" aria-hidden="true"></div>
    <button type="button" class="vault-context-menu-item" role="menuitem" disabled={!canFormat || !editable} onclick={() => run("format")}>Format Document</button>
    <button type="button" class="vault-context-menu-item" role="menuitem" disabled={!canOrganize || !editable} onclick={() => run("organize_imports")}>Organize Imports</button>
    <div class="vault-context-menu-sep" aria-hidden="true"></div>
    <button type="button" class="vault-context-menu-item" role="menuitem" onclick={() => run("copy_path")}>Copy Path</button>
    <button type="button" class="vault-context-menu-item" role="menuitem" onclick={() => run("copy_relative_path")}>Copy Relative Path</button>
    <button type="button" class="vault-context-menu-item" role="menuitem" onclick={() => run("reveal")}>Reveal in Explorer</button>
  </div>
{/if}
