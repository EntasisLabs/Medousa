<script lang="ts">
  import { buildWorkshopCommandContext } from "$lib/commands/context";
  import { collectWorkshopCommands, flattenGroups } from "$lib/commands/collectCommands";
  import { executeWorkshopCommand } from "$lib/commands/runWorkshopCommand";
  import { chat } from "$lib/stores/chat.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { GroupedCommands, WorkshopCommand } from "$lib/commands/types";

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

  const notesMode = $derived(commandSpotlight.mode === "notes");
  const promptStep = $derived(commandSpotlight.promptStep);

  const ctx = $derived(
    buildWorkshopCommandContext({
      close: () => commandSpotlight.closeSpotlight(),
      focusChat: () => onFocusChat?.(),
    }),
  );

  const flatCommands = $derived(flattenGroups(groups));

  const placeholder = $derived(
    notesMode
      ? "Search notes…"
      : "Go somewhere, open a note, or run an action…",
  );

  /** Side effects + command collection belong in $effect, never $derived. */
  $effect(() => {
    if (!commandSpotlight.open) {
      groups = [];
      return;
    }

    if (vault.notes.length === 0) {
      void vault.refreshNotes();
    }

    void vault.notes;
    void vault.labelByPathMap;
    void chat.sessions;
    void chat.pendingBudgetApprovals;
    void chat.contextUsage;
    void chat.liveStreamActive;
    void connection.offline;
    void workspace.cards;
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

  $effect(() => {
    if (!commandSpotlight.open) {
      query = "";
      promptValue = "";
      busy = false;
      return;
    }

    if (!promptStep) {
      query = notesMode ? "" : query;
    }
    highlightIndex = 0;

    const frame = requestAnimationFrame(() => {
      inputEl?.focus();
    });
    return () => cancelAnimationFrame(frame);
  });

  $effect(() => {
    query;
    promptStep;
    flatCommands.length;
    if (highlightIndex >= flatCommands.length) {
      highlightIndex = Math.max(0, flatCommands.length - 1);
    }
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
</script>

<svelte:window onkeydown={handleKeydown} />

{#if commandSpotlight.open}
  <div
    class="command-spotlight-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) commandSpotlight.closeSpotlight();
    }}
  >
    <div
      class="command-spotlight-panel"
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
      {/if}

      <div class="command-spotlight-results">
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
                  disabled={busy}
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

      <footer class="command-spotlight-footer">
        <span>↑↓ navigate · ↵ run · esc close</span>
        <span class="command-spotlight-kbd">⌘K</span>
      </footer>
    </div>
  </div>
{/if}
