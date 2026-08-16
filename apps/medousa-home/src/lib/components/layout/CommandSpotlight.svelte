<script lang="ts">
  import "$lib/styles/command-spotlight.postcss";
  import { buildWorkshopCommandContext } from "$lib/commands/context";
  import {
    collectWorkshopCommands,
    flattenGroups,
    parseSpotlightQuery,
  } from "$lib/commands/collectCommands";
  import { jumpPinSlot } from "$lib/commands/pinCommands";
  import { executeWorkshopCommand } from "$lib/commands/runWorkshopCommand";
  import { getVaultNote } from "$lib/daemon";
  import { chat } from "$lib/stores/chat.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
  import { sessionExportPreview } from "$lib/stores/sessionExportPreview.svelte";
  import { spotlightPins } from "$lib/stores/spotlightPins.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { GroupedCommands, WorkshopCommand } from "$lib/commands/types";
  import {
    popBrowserPopoverOverlay,
    pushBrowserPopoverOverlay,
  } from "$lib/utils/browserPopoverOverlay";
  import { noteExcerpt } from "$lib/utils/vaultNoteBridge";
  import { formatShortcut } from "$lib/platform";
  import { loadVaultExportPreviewModal } from "$lib/runtime/viewLoaders";

  interface Props {
    onFocusChat?: () => void;
  }

  let { onFocusChat }: Props = $props();

  let query = $state("");
  let highlightIndex = $state(0);
  let busy = $state(false);
  let inputEl = $state<HTMLInputElement | null>(null);
  let promptValue = $state("");
  let groups = $state<GroupedCommands[]>([]);
  let previewText = $state<string | null>(null);
  let previewTitle = $state<string | null>(null);
  let resultsEl = $state<HTMLDivElement | null>(null);
  const notesMode = $derived(commandSpotlight.mode === "notes");
  const promptStep = $derived(commandSpotlight.promptStep);

  const ctx = $derived(
    buildWorkshopCommandContext({
      close: () => {
        commandSpotlight.rememberQuery(query, commandSpotlight.mode);
        commandSpotlight.closeSpotlight();
      },
      focusChat: () => onFocusChat?.(),
    }),
  );

  const flatCommands = $derived(flattenGroups(groups));
  const activeCommand = $derived(flatCommands[highlightIndex] ?? null);

  const placeholder = $derived(
    notesMode
      ? "Search notes…"
      : "Search or + create · ! run · > advanced · pins 1–4",
  );

  /** Native browser embed draws over the DOM — hide it while spotlight is open. */
  $effect(() => {
    if (!commandSpotlight.open) return;
    void pushBrowserPopoverOverlay();
    return () => {
      void popBrowserPopoverOverlay();
    };
  });

  /** Side effects + command collection belong in $effect, never $derived. */
  $effect(() => {
    if (!commandSpotlight.open) {
      groups = [];
      return;
    }

    if (vault.notes.length === 0) {
      void vault.refreshNotes();
    }
    if (workshop.scripts.length === 0) {
      void workshop.refreshModulesAndScripts();
    }

    void vault.notes;
    void vault.labelByPathMap;
    void chat.sessions;
    void chat.pendingBudgetApprovals;
    void chat.contextUsage;
    void chat.liveStreamActive;
    void connection.offline;
    void workspace.cards;
    void workshop.scripts;
    void spotlightPins.slots;
    void commandSpotlight.mode;
    void query;
    void promptStep;
    void ctx;

    try {
      groups = collectWorkshopCommands(ctx, {
        query,
        notesMode,
      });
    } catch (err) {
      console.error("Command spotlight failed to collect commands", err);
      groups = [];
    }
  });

  /** Reset / focus when Spotlight opens or enters a prompt — not on every keystroke. */
  $effect(() => {
    const isOpen = commandSpotlight.open;
    const step = promptStep;
    if (!isOpen) {
      query = "";
      promptValue = "";
      busy = false;
      previewText = null;
      previewTitle = null;
      highlightIndex = 0;
      return;
    }

    // Intentionally do not read query/seed here — that re-ran this effect on every keystroke
    // and fought resume hydration.
    void step;
    highlightIndex = 0;

    const frame = requestAnimationFrame(() => {
      inputEl?.focus();
    });
    return () => cancelAnimationFrame(frame);
  });

  /** Hydrate query from resume / restore (open or already open). */
  $effect(() => {
    const seed = commandSpotlight.seedQuery;
    if (!commandSpotlight.open || seed == null || promptStep) return;
    query = seed;
    commandSpotlight.seedQuery = null;
    highlightIndex = 0;
  });

  $effect(() => {
    const len = flatCommands.length;
    if (highlightIndex >= len) {
      highlightIndex = Math.max(0, len - 1);
    }
  });

  /** Telescope-lite preview for highlighted row (keyed by command id). */
  $effect(() => {
    if (!commandSpotlight.open) {
      previewText = null;
      previewTitle = null;
      return;
    }
    const command = activeCommand;
    const commandId = command?.id ?? null;
    const preview = command?.preview;
    if (!commandId || !preview) {
      previewText = null;
      previewTitle = null;
      return;
    }

    let cancelled = false;
    previewTitle = command.label;

    if (preview.kind === "text") {
      previewText = preview.text;
      return;
    }
    if (preview.kind === "script") {
      previewText = preview.body?.trim() || "Loading script…";
      if (!preview.body) {
        void (async () => {
          try {
            const { getGraphemeScript } = await import("$lib/daemon");
            const detail = await getGraphemeScript(preview.scriptId);
            if (!cancelled) previewText = detail.body_preview || "(empty script)";
          } catch {
            if (!cancelled) previewText = "Couldn’t load script preview.";
          }
        })();
      }
      return () => {
        cancelled = true;
      };
    }
    if (preview.kind === "chat") {
      previewText = preview.text?.trim() || "Open this conversation.";
      return;
    }
    if (preview.kind === "note") {
      previewText = "Loading note…";
      void (async () => {
        try {
          const note = await getVaultNote(preview.path);
          if (cancelled) return;
          previewText = noteExcerpt(note.content ?? "", 900);
        } catch {
          if (!cancelled) previewText = "Couldn’t load note preview.";
        }
      })();
    }

    return () => {
      cancelled = true;
    };
  });

  async function runCommand(command: WorkshopCommand, args?: string) {
    if (busy) return;
    if (command.prompt && !args) {
      commandSpotlight.beginPrompt(
        {
          commandId: command.id,
          label: command.label,
          placeholder: command.prompt.placeholder,
          submitLabel: command.prompt.submitLabel ?? "Run",
        },
        command,
      );
      promptValue = "";
      requestAnimationFrame(() => inputEl?.focus());
      return;
    }
    busy = true;
    try {
      commandSpotlight.rememberQuery(query, commandSpotlight.mode);
      await executeWorkshopCommand(ctx, command, args);
    } catch (err) {
      ctx.error(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
  }

  async function submitPrompt() {
    const step = promptStep;
    const command = commandSpotlight.pendingCommand;
    if (!step || !command) {
      commandSpotlight.cancelPrompt();
      return;
    }
    const value = promptValue.trim();
    if (!value) return;
    commandSpotlight.cancelPrompt();
    await runCommand(command, value);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!commandSpotlight.open) return;

    if (event.key === "Escape") {
      event.preventDefault();
      if (promptStep) {
        commandSpotlight.cancelPrompt();
      } else {
        commandSpotlight.rememberQuery(query, commandSpotlight.mode);
        commandSpotlight.closeSpotlight();
      }
      return;
    }

    if (promptStep) {
      if (event.key === "Enter") {
        event.preventDefault();
        void submitPrompt();
      }
      return;
    }

    // Harpoon: digits 1–4 jump pins when query is empty.
    if (!query.trim() && /^[1-4]$/.test(event.key) && !event.metaKey && !event.ctrlKey && !event.altKey) {
      const slot = Number(event.key) - 1;
      if (jumpPinSlot(ctx, slot)) {
        event.preventDefault();
        return;
      }
    }

    if (flatCommands.length === 0) return;

    if (event.key === "ArrowDown") {
      event.preventDefault();
      highlightIndex = (highlightIndex + 1) % flatCommands.length;
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      highlightIndex = (highlightIndex - 1 + flatCommands.length) % flatCommands.length;
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const command = flatCommands[highlightIndex];
      if (command) void runCommand(command);
    }
  }

  function globalIndex(sectionIndex: number, itemIndex: number): number {
    let index = 0;
    for (let s = 0; s < sectionIndex; s += 1) {
      index += groups[s]?.commands.length ?? 0;
    }
    return index + itemIndex;
  }

  function queueScrollActiveRow() {
    requestAnimationFrame(() => {
      const row = resultsEl?.querySelector<HTMLElement>(
        `[data-spotlight-index="${highlightIndex}"]`,
      );
      row?.scrollIntoView({ block: "nearest" });
    });
  }

  $effect(() => {
    if (!commandSpotlight.open) return;
    void highlightIndex;
    void groups;
    queueScrollActiveRow();
  });

  const prefixHint = $derived(parseSpotlightQuery(query).mode);
</script>

<svelte:window onkeydown={handleKeydown} />

{#if commandSpotlight.open}
  <div
    class="command-spotlight-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) {
        commandSpotlight.rememberQuery(query, commandSpotlight.mode);
        commandSpotlight.closeSpotlight();
      }
    }}
  >
    <div
      class="command-spotlight-panel"
      class:command-spotlight-panel-wide={Boolean(previewText)}
      role="dialog"
      aria-modal="true"
      aria-label="Command spotlight"
    >
      {#if promptStep}
        <div class="command-spotlight-prompt-header">
          <p class="command-spotlight-kicker">Follow-up</p>
          <p class="command-spotlight-prompt-label">{promptStep.label}</p>
        </div>
        <input
          bind:this={inputEl}
          class="command-spotlight-input"
          placeholder={promptStep.placeholder}
          bind:value={promptValue}
          disabled={busy}
        />
      {:else}
        <input
          bind:this={inputEl}
          class="command-spotlight-input"
          {placeholder}
          bind:value={query}
          disabled={busy}
        />
        {#if prefixHint !== "default"}
          <p class="command-spotlight-mode-chip">
            {#if prefixHint === "create"}
              Create
            {:else if prefixHint === "run"}
              Run
            {:else}
              Advanced
            {/if}
          </p>
        {/if}
      {/if}

      <div class="command-spotlight-body">
        <div class="command-spotlight-results" bind:this={resultsEl}>
          {#each groups as group, sectionIndex (group.section)}
            <div class="command-spotlight-section-label">{group.label}</div>
            <ul class="command-spotlight-list">
              {#each group.commands as command, itemIndex (command.id)}
                {@const rowIndex = globalIndex(sectionIndex, itemIndex)}
                <li>
                  <button
                    type="button"
                    class="command-spotlight-row"
                    class:command-spotlight-row-active={rowIndex === highlightIndex}
                    data-spotlight-index={rowIndex}
                    disabled={busy}
                    onmouseenter={() => (highlightIndex = rowIndex)}
                    onclick={() => void runCommand(command)}
                  >
                    <span class="command-spotlight-row-copy">
                      <span class="command-spotlight-row-title">{command.label}</span>
                      {#if command.subtitle}
                        <span class="command-spotlight-row-subtitle">{command.subtitle}</span>
                      {/if}
                    </span>
                    <span class="command-spotlight-row-meta">
                      {#if command.risk === "attention"}
                        <span class="command-spotlight-attention">Needs attention</span>
                      {/if}
                      {#if command.hint}
                        <span class="command-spotlight-hint">{command.hint}</span>
                      {/if}
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="command-spotlight-empty">No matching commands</p>
          {/each}
        </div>

        {#if previewText}
          <aside class="command-spotlight-preview" aria-label="Preview">
            {#if previewTitle}
              <p class="command-spotlight-preview-title">{previewTitle}</p>
            {/if}
            <pre class="command-spotlight-preview-body">{previewText}</pre>
          </aside>
        {/if}
      </div>

      <footer class="command-spotlight-footer">
        <span>↑↓ · ↵ · esc · 1–4 pins</span>
        <span class="command-spotlight-kbd">{formatShortcut("K")}</span>
      </footer>
    </div>
  </div>
{/if}

{#if sessionExportPreview.open}
  {#await loadVaultExportPreviewModal() then { default: VaultExportPreviewModal }}
    <VaultExportPreviewModal
      open={sessionExportPreview.open}
      title={sessionExportPreview.title}
      content={sessionExportPreview.content}
      labelByPath={new Map()}
      notePath={null}
      initialFormat="pdf"
      onClose={() => sessionExportPreview.close()}
    />
  {/await}
{/if}
