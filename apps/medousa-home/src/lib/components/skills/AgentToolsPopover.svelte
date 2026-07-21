<script lang="ts">
  import { Wrench } from "@lucide/svelte";
  import { onMount } from "svelte";
  import AgentToolsPicker from "$lib/components/skills/AgentToolsPicker.svelte";
  import type { ManuscriptScriptEntry } from "$lib/types/catalog";

  interface Props {
    open?: boolean;
    palette: string[];
    selected: string[];
    onToggle: (toolId: string, enabled: boolean) => void;
    openshellEnabled?: boolean;
    openshellDefaultPath?: string;
    openshellAllowScheduled?: boolean;
    scripts?: ManuscriptScriptEntry[];
    onOpenChange?: (open: boolean) => void;
  }

  let {
    open = $bindable(false),
    palette,
    selected,
    onToggle,
    openshellEnabled = false,
    openshellDefaultPath = "",
    openshellAllowScheduled = $bindable(false),
    scripts = [],
    onOpenChange,
  }: Props = $props();

  let showOpenshell = $state(false);
  let menuEl: HTMLDivElement | undefined = $state();
  let triggerEl: HTMLButtonElement | undefined = $state();
  let panelStyle = $state("");

  function placePanel() {
    if (!triggerEl) return;
    const rect = triggerEl.getBoundingClientRect();
    const width = Math.min(36 * 16, window.innerWidth - 24);
    let left = rect.right - width;
    left = Math.max(12, Math.min(left, window.innerWidth - width - 12));
    const top = Math.min(rect.bottom + 6, window.innerHeight - 48);
    panelStyle = `top:${top}px;left:${left}px;width:${width}px;`;
  }

  function setOpen(next: boolean) {
    open = next;
    onOpenChange?.(next);
    if (next) {
      showOpenshell = false;
      placePanel();
    }
  }

  function toggleOpen() {
    setOpen(!open);
  }

  onMount(() => {
    const onDocClick = (event: MouseEvent) => {
      if (!open) return;
      const target = event.target as Node | null;
      if (menuEl?.contains(target) || triggerEl?.contains(target)) return;
      setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && open) setOpen(false);
    };
    document.addEventListener("click", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("click", onDocClick);
      document.removeEventListener("keydown", onKey);
    };
  });

  $effect(() => {
    if (!open) return;
    placePanel();
    const onResize = () => placePanel();
    window.addEventListener("resize", onResize);
    window.addEventListener("scroll", onResize, true);
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("scroll", onResize, true);
    };
  });
</script>

<div class="agent-editor-popover relative shrink-0">
  <button
    bind:this={triggerEl}
    type="button"
    class="scripts-workbench-toolbar-btn {open ? 'scripts-workbench-toolbar-btn-active' : ''}"
    title="Tools"
    aria-label="Open tools"
    aria-haspopup="dialog"
    aria-expanded={open}
    onclick={toggleOpen}
  >
    <Wrench size={15} strokeWidth={1.75} />
    {#if selected.length > 0}
      <span class="agent-editor-popover-badge" aria-hidden="true">{selected.length}</span>
    {/if}
  </button>

  {#if open}
    <div
      bind:this={menuEl}
      class="agent-editor-popover-panel"
      style={panelStyle}
      role="dialog"
      aria-label="Tools"
    >
      <div class="agent-editor-popover-head">
        <div class="min-w-0">
          <p class="text-[11px] font-semibold tracking-[-0.01em] text-surface-100">Tools</p>
          <p class="mt-0.5 text-[10px] text-surface-500">
            {#if selected.length > 0}
              {selected.length} selected
            {:else}
              Tools this agent can use
            {/if}
          </p>
        </div>
      </div>

      <div
        class="agent-editor-popover-body flex min-h-0 flex-1 flex-col gap-3 overflow-hidden p-3"
        style="min-height: min(22rem, 55vh);"
      >
        <AgentToolsPicker {palette} {selected} {onToggle} fill />

        {#if openshellEnabled}
          <div class="shrink-0 rounded-lg border border-surface-500/25 bg-surface-950/30">
            <button
              type="button"
              class="flex w-full items-center justify-between px-3 py-2 text-left text-xs"
              onclick={() => (showOpenshell = !showOpenshell)}
            >
              <span class="text-surface-300">OpenShell sandbox</span>
              <span class="text-surface-500">{showOpenshell ? "−" : "+"}</span>
            </button>
            {#if showOpenshell}
              <div class="border-t border-surface-500/25 px-3 py-2.5 text-xs text-surface-300">
                <p>
                  Default path:
                  <span class="text-surface-200">{openshellDefaultPath}</span>
                </p>
                <label class="mt-3 flex items-center gap-2">
                  <input
                    type="checkbox"
                    class="checkbox"
                    bind:checked={openshellAllowScheduled}
                  />
                  Allow OpenShell tools on scheduled runs
                </label>
              </div>
            {/if}
          </div>
        {/if}

        {#if scripts.length > 0}
          <div class="shrink-0">
            <p class="agent-liquid-whisper">Scripts</p>
            <ul class="mt-2 max-h-24 space-y-1 overflow-y-auto text-[11px] text-surface-300">
              {#each scripts as script (script.relative_path)}
                <li class="font-mono">
                  {script.relative_path}
                  <span class="text-surface-500">({script.risk_class})</span>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>
