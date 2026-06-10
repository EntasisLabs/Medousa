<script lang="ts">
  import { chat } from "$lib/stores/chat.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import {
    approveTurnBudgetRequest,
    denyTurnBudgetRequest,
    listTurnBudgetRequests,
  } from "$lib/daemon";
  import { homeChannelSurface } from "$lib/platform";
  import { SLASH_COMMAND_HINTS } from "$lib/utils/slashCommands";

  interface Props {
    open: boolean;
    onClose: () => void;
    onOpenWork?: () => void;
    onFocusChat?: () => void;
  }

  let { open, onClose, onOpenWork, onFocusChat }: Props = $props();

  let query = $state("");
  let busy = $state(false);
  let message = $state<string | null>(null);

  interface PaletteCommand {
    id: string;
    label: string;
    hint?: string;
    keywords: string;
    run: () => void | Promise<void>;
  }

  const staticCommands: PaletteCommand[] = [
    {
      id: "help",
      label: "Show slash commands",
      hint: "/help",
      keywords: "help commands slash",
      run: () => {
        message = SLASH_COMMAND_HINTS.join("\n");
      },
    },
    {
      id: "focus-chat",
      label: "Focus chat",
      keywords: "chat message compose",
      run: () => {
        onFocusChat?.();
        onClose();
      },
    },
    {
      id: "open-work",
      label: "Open Work board",
      keywords: "work kanban blocked",
      run: () => {
        onOpenWork?.();
        onClose();
      },
    },
  ];

  const budgetCommands = $derived.by((): PaletteCommand[] => {
    const pending = chat.pendingBudgetApprovals;
    if (pending.length === 0) {
      return [
        {
          id: "budget-list",
          label: "List pending budget approvals",
          hint: "/budget list",
          keywords: "budget approve rounds pending",
          run: async () => {
            const rows = await listTurnBudgetRequests(true);
            if (rows.length === 0) {
              message = "No pending budget approvals.";
              return;
            }
            message = rows
              .map(
                (row) =>
                  `${row.request_id} · +${row.requested_rounds} rounds · ${row.progress_summary ?? row.reason}`,
              )
              .join("\n");
          },
        },
      ];
    }
    return pending.flatMap((item) => [
      {
        id: `approve-${item.requestId}`,
        label: `Approve +${item.requestedRounds ?? "?"} tool rounds`,
        hint: item.requestId.slice(0, 8),
        keywords: `budget approve ${item.requestId}`,
        run: async () => {
          await approveTurnBudgetRequest(
            item.requestId,
            item.requestedRounds ?? undefined,
            homeChannelSurface(),
          );
          chat.noteBudgetResolved(item.requestId);
          chat.clearBudgetAlert();
          message = "Approved — turn resuming.";
          onClose();
        },
      },
      {
        id: `deny-${item.requestId}`,
        label: "Deny budget extension",
        hint: item.requestId.slice(0, 8),
        keywords: `budget deny ${item.requestId}`,
        run: async () => {
          await denyTurnBudgetRequest(item.requestId, homeChannelSurface());
          chat.noteBudgetResolved(item.requestId);
          chat.clearBudgetAlert();
          message = "Denied.";
          onClose();
        },
      },
      {
        id: `work-${item.requestId}`,
        label: "Open approval in Work",
        keywords: `work card ${item.requestId}`,
        run: async () => {
          onOpenWork?.();
          await workspace.selectCard(item.requestId);
          onClose();
        },
      },
    ]);
  });

  const allCommands = $derived([...budgetCommands, ...staticCommands]);

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return allCommands;
    return allCommands.filter(
      (cmd) =>
        cmd.label.toLowerCase().includes(q) ||
        cmd.keywords.toLowerCase().includes(q) ||
        cmd.hint?.toLowerCase().includes(q),
    );
  });

  $effect(() => {
    if (!open) {
      query = "";
      message = null;
      busy = false;
    }
  });

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }

  async function runCommand(command: PaletteCommand) {
    if (busy) return;
    busy = true;
    message = null;
    try {
      await command.run();
    } catch (err) {
      message = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div
    class="command-palette-backdrop fixed inset-0 z-[80] flex items-start justify-center bg-surface-950/60 p-4 pt-[12vh]"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <div
      class="command-palette-panel w-full max-w-lg overflow-hidden rounded-xl border border-surface-700/60 bg-surface-900 shadow-2xl"
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
    >
      <input
        class="w-full border-0 border-b border-surface-800 bg-transparent px-4 py-3 text-sm text-surface-50 outline-none"
        placeholder="Search commands… (budget, approve, work)"
        bind:value={query}
        autofocus
      />
      <ul class="max-h-72 overflow-y-auto py-1">
        {#each filtered as command (command.id)}
          <li>
            <button
              type="button"
              class="flex w-full items-center justify-between gap-3 px-4 py-2.5 text-left text-sm hover:bg-surface-800/80"
              disabled={busy}
              onclick={() => void runCommand(command)}
            >
              <span class="text-surface-100">{command.label}</span>
              {#if command.hint}
                <span class="workshop-faint text-xs">{command.hint}</span>
              {/if}
            </button>
          </li>
        {:else}
          <li class="px-4 py-6 text-center text-sm text-surface-500">No matching commands</li>
        {/each}
      </ul>
      {#if message}
        <pre
          class="max-h-40 overflow-auto whitespace-pre-wrap border-t border-surface-800 px-4 py-3 text-xs text-surface-300"
        >{message}</pre>
      {/if}
      <p class="border-t border-surface-800 px-4 py-2 text-[11px] text-surface-500">
        Tip: type <code class="text-surface-400">/budget approve</code> in chat · Esc to close
      </p>
    </div>
  </div>
{/if}
