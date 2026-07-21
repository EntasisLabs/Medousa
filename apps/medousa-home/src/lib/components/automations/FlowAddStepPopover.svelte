<script lang="ts">
  import { ChevronLeft, FileCode2, MessageSquare, Plus, Wrench } from "@lucide/svelte";
  import { onMount } from "svelte";
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import { getGraphemeScript } from "$lib/daemon";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type { WorkflowStepSpec } from "$lib/types/workflow";
  import { newStepId } from "$lib/types/workflow";
  import { GRAPHEME_STEP_PLACEHOLDER } from "$lib/utils/flowStepLabels";
  import "$lib/components/skills/agentEditor.css";

  type StepKind = "grapheme" | "prompt" | "mcp";
  type Phase = "pick" | StepKind;

  interface Props {
    open?: boolean;
    mobile?: boolean;
    onAdd: (step: WorkflowStepSpec) => void;
    onOpenChange?: (open: boolean) => void;
  }

  let {
    open = $bindable(false),
    mobile = false,
    onAdd,
    onOpenChange,
  }: Props = $props();

  let phase = $state<Phase>("pick");
  let promptText = $state("");
  let scriptSource = $state("");
  let scriptId = $state("");
  let scriptName = $state("");
  let serverId = $state("");
  let toolName = $state("");
  let argsJson = $state("{}");
  let libraryBusy = $state(false);
  let argsError = $state<string | null>(null);

  let menuEl: HTMLDivElement | undefined = $state();
  let triggerEl: HTMLButtonElement | undefined = $state();
  let panelStyle = $state("");

  function placePanel() {
    if (!triggerEl) return;
    const rect = triggerEl.getBoundingClientRect();
    const width = Math.min(20 * 16, window.innerWidth - 24);
    let left = rect.right - width;
    left = Math.max(12, Math.min(left, window.innerWidth - width - 12));
    const top = Math.min(rect.bottom + 6, window.innerHeight - 48);
    panelStyle = `top:${top}px;left:${left}px;width:${width}px;`;
  }

  function resetForm() {
    phase = "pick";
    promptText = "";
    scriptSource = "";
    scriptId = "";
    scriptName = "";
    serverId = "";
    toolName = "";
    argsJson = "{}";
    argsError = null;
    libraryBusy = false;
  }

  function setOpen(next: boolean) {
    const wasOpen = open;
    open = next;
    onOpenChange?.(next);
    if (next && !wasOpen) {
      resetForm();
      placePanel();
      if (workshop.scripts.length === 0) {
        void workshop.refreshModulesAndScripts();
      }
    }
  }

  function toggleOpen(event: MouseEvent) {
    event.stopPropagation();
    setOpen(!open);
  }

  function selectKind(kind: StepKind, event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    phase = kind;
    requestAnimationFrame(() => placePanel());
  }

  function canConfirm(): boolean {
    if (phase === "prompt") return promptText.trim().length > 0;
    if (phase === "grapheme") return scriptSource.trim().length > 0;
    if (phase === "mcp") return serverId.trim().length > 0 && toolName.trim().length > 0;
    return false;
  }

  async function loadLibraryScript(id: string) {
    if (!id.trim()) {
      scriptId = "";
      scriptName = "";
      return;
    }
    libraryBusy = true;
    try {
      const detail = await getGraphemeScript(id);
      const entry = workshop.scripts.find((row) => row.id === id);
      scriptId = id;
      scriptName = entry?.name ?? detail.script.name;
      scriptSource = detail.body_preview;
    } finally {
      libraryBusy = false;
    }
  }

  function confirmAdd(event: MouseEvent) {
    event.stopPropagation();
    if (phase === "pick" || !canConfirm()) return;

    if (phase === "prompt") {
      onAdd({
        kind: "prompt",
        id: newStepId("prm"),
        user_prompt: promptText.trim(),
      });
    } else if (phase === "grapheme") {
      onAdd({
        kind: "grapheme",
        id: newStepId("gra"),
        source: scriptSource,
        script_id: scriptId || undefined,
        script_name: scriptName || undefined,
      });
    } else if (phase === "mcp") {
      let args: Record<string, unknown> = {};
      try {
        const parsed = JSON.parse(argsJson.trim() || "{}");
        if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
          args = parsed as Record<string, unknown>;
        } else {
          argsError = "Arguments must be a JSON object.";
          return;
        }
      } catch {
        argsError = "Arguments must be valid JSON.";
        return;
      }
      onAdd({
        kind: "mcp",
        id: newStepId("mcp"),
        server_id: serverId.trim(),
        tool_name: toolName.trim(),
        args,
      });
    }

    setOpen(false);
  }

  function isInsidePopover(target: EventTarget | null): boolean {
    if (!(target instanceof Node)) return false;
    return Boolean(menuEl?.contains(target) || triggerEl?.contains(target));
  }

  onMount(() => {
    // Use pointerdown in bubble phase so button handlers run first; ignore
    // presses that originated inside the panel/trigger via composedPath.
    const onDocPointerDown = (event: PointerEvent) => {
      if (!open) return;
      const path = event.composedPath();
      if (
        (menuEl && path.includes(menuEl)) ||
        (triggerEl && path.includes(triggerEl))
      ) {
        return;
      }
      if (isInsidePopover(event.target)) return;
      setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && open) setOpen(false);
    };
    document.addEventListener("pointerdown", onDocPointerDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDocPointerDown);
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
    title="Add step"
    aria-label="Add step"
    aria-haspopup="dialog"
    aria-expanded={open}
    onclick={toggleOpen}
  >
    <Plus size={15} strokeWidth={1.75} />
  </button>

  {#if open}
    <div
      bind:this={menuEl}
      class="agent-editor-popover-panel agent-editor-popover-panel-schedule"
      style={panelStyle}
      role="dialog"
      aria-label="Add step"
      onpointerdown={(event) => event.stopPropagation()}
    >
      <div class="agent-editor-popover-head">
        <div class="min-w-0">
          {#if phase === "pick"}
            <p class="text-[11px] font-semibold tracking-[-0.01em] text-surface-100">Add step</p>
            <p class="mt-0.5 text-[10px] text-surface-500">Choose a type</p>
          {:else}
            <button
              type="button"
              class="inline-flex items-center gap-0.5 text-[11px] text-surface-400 hover:text-surface-200"
              onclick={(event) => {
                event.stopPropagation();
                phase = "pick";
              }}
            >
              <ChevronLeft size={12} strokeWidth={2} />
              Back
            </button>
            <p class="mt-0.5 text-[11px] font-semibold tracking-[-0.01em] text-surface-100">
              {#if phase === "grapheme"}
                Script
              {:else if phase === "prompt"}
                Ask Medousa
              {:else}
                External tool
              {/if}
            </p>
          {/if}
        </div>
      </div>

      <div class="agent-editor-popover-body overflow-y-auto p-2">
        {#if phase === "pick"}
          <div class="flex flex-col gap-0.5" role="listbox" aria-label="Step types">
            <button
              type="button"
              class="flow-add-kind-row"
              role="option"
              onclick={(event) => selectKind("grapheme", event)}
            >
              <FileCode2 size={13} strokeWidth={1.75} class="shrink-0 opacity-65" />
              <span class="min-w-0 flex-1 truncate text-left text-[12px] text-surface-100"
                >Script</span
              >
              <span class="shrink-0 text-[10px] text-surface-500">Grapheme</span>
            </button>
            <button
              type="button"
              class="flow-add-kind-row"
              role="option"
              onclick={(event) => selectKind("prompt", event)}
            >
              <MessageSquare size={13} strokeWidth={1.75} class="shrink-0 opacity-65" />
              <span class="min-w-0 flex-1 truncate text-left text-[12px] text-surface-100"
                >Ask Medousa</span
              >
              <span class="shrink-0 text-[10px] text-surface-500">Prompt</span>
            </button>
            <button
              type="button"
              class="flow-add-kind-row"
              role="option"
              onclick={(event) => selectKind("mcp", event)}
            >
              <Wrench size={13} strokeWidth={1.75} class="shrink-0 opacity-65" />
              <span class="min-w-0 flex-1 truncate text-left text-[12px] text-surface-100"
                >External tool</span
              >
              <span class="shrink-0 text-[10px] text-surface-500">MCP</span>
            </button>
          </div>
        {:else if phase === "prompt"}
          <div class="px-1 pb-1">
            <label class="block">
              <span class="flow-liquid-whisper">Instructions</span>
              <div class="mt-1">
                <GrowingTextarea
                  bind:value={promptText}
                  placeholder="Summarize the top headlines…"
                  minHeight={mobile ? 48 : 56}
                  maxHeight={140}
                  aria-label="Prompt step text"
                />
              </div>
            </label>
            <button
              type="button"
              class="btn btn-sm variant-soft-primary mt-2.5"
              disabled={!canConfirm()}
              onclick={confirmAdd}
            >
              Add to flow
            </button>
          </div>
        {:else if phase === "grapheme"}
          <div class="px-1 pb-1">
            {#if workshop.scripts.length > 0}
              <label class="block">
                <span class="flow-liquid-whisper">From library</span>
                <select
                  class="mt-1 w-full rounded-md border border-surface-500/35 bg-surface-900 px-2 py-1.5 text-xs text-surface-200"
                  value={scriptId}
                  disabled={libraryBusy}
                  onchange={(event) =>
                    void loadLibraryScript((event.currentTarget as HTMLSelectElement).value)}
                  aria-label="Load saved script"
                >
                  <option value="">Write new…</option>
                  {#each workshop.scripts as entry (entry.id)}
                    <option value={entry.id}>{entry.name}</option>
                  {/each}
                </select>
              </label>
            {/if}
            <label class="mt-2.5 block">
              <span class="flow-liquid-whisper">Source</span>
              <textarea
                class="flow-script-editor mt-1"
                bind:value={scriptSource}
                placeholder={GRAPHEME_STEP_PLACEHOLDER}
                aria-label="Grapheme source"
                rows={7}
              ></textarea>
            </label>
            <button
              type="button"
              class="btn btn-sm variant-soft-primary mt-2.5"
              disabled={!canConfirm() || libraryBusy}
              onclick={confirmAdd}
            >
              Add to flow
            </button>
          </div>
        {:else if phase === "mcp"}
          <div class="px-1 pb-1">
            <div class="grid gap-2.5 sm:grid-cols-2">
              <label class="block">
                <span class="flow-liquid-whisper">Server</span>
                <input
                  class="mt-1 w-full rounded-md border border-surface-500/35 bg-surface-900 px-2 py-1.5 font-mono text-xs text-surface-200"
                  bind:value={serverId}
                  placeholder="my-mcp-server"
                  aria-label="MCP server"
                />
              </label>
              <label class="block">
                <span class="flow-liquid-whisper">Tool</span>
                <input
                  class="mt-1 w-full rounded-md border border-surface-500/35 bg-surface-900 px-2 py-1.5 font-mono text-xs text-surface-200"
                  bind:value={toolName}
                  placeholder="search"
                  aria-label="MCP tool"
                />
              </label>
            </div>
            <label class="mt-2.5 block">
              <span class="flow-liquid-whisper">Arguments (JSON)</span>
              <textarea
                class="flow-script-editor mt-1"
                bind:value={argsJson}
                rows={3}
                aria-label="MCP args JSON"
              ></textarea>
            </label>
            {#if argsError}
              <p class="mt-1 text-[11px] text-warning-400">{argsError}</p>
            {/if}
            <button
              type="button"
              class="btn btn-sm variant-soft-primary mt-2.5"
              disabled={!canConfirm()}
              onclick={confirmAdd}
            >
              Add to flow
            </button>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>
