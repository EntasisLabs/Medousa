<script lang="ts">
  import {
    BookOpen,
    FolderOpen,
    Inbox,
    Pin,
    Sparkles,
    Wallet,
    Wand2,
  } from "@lucide/svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import { vault } from "$lib/stores/vault.svelte";

  let captureLine = $state("");

  const lifeSpaces = [
    { icon: BookOpen, label: "Journal", hint: "Daily notes & reviews" },
    { icon: FolderOpen, label: "Projects", hint: "Plans you're chasing" },
    { icon: Wallet, label: "Finance", hint: "Budgets & ledgers" },
    { icon: Inbox, label: "Inbox", hint: "Capture now, sort later" },
  ];

  async function handleCreateDaily() {
    await vault.createDailyNote();
  }

  async function handleQuickCapture() {
    if (!captureLine.trim()) return;
    await vault.quickCapture(captureLine);
    captureLine = "";
  }

  async function handleOpenLast() {
    const path = vault.lastNotePath;
    if (path) await vault.openNote(path);
  }

  function handleSetupGarage() {
    vault.openGarageWizard();
  }

  async function handlePinFolder() {
    await externalDesk.pinFolder();
  }
</script>

<div class="vault-empty-state flex flex-1 flex-col items-center justify-center gap-8 p-8">
  <div class="max-w-lg space-y-3 text-center">
    <div class="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary-500/12 text-primary-300">
      <Sparkles size={22} strokeWidth={2} />
    </div>
    <h2 class="text-xl font-semibold tracking-tight text-surface-50">Bring your mess</h2>
    <p class="text-sm leading-relaxed text-surface-400">
      Pick up where you left off — notes, files, and chat stay connected without switching apps.
    </p>
  </div>

  <div class="w-full max-w-xl">
    <p class="mb-3 text-center text-[11px] font-semibold uppercase tracking-wide text-surface-500">
      What belongs here
    </p>
    <ul class="grid gap-2 sm:grid-cols-2">
      {#each lifeSpaces as space (space.label)}
        {@const Icon = space.icon}
        <li class="rounded-lg border border-surface-500/35 bg-surface-900/40 px-4 py-3 text-left">
          <div class="flex items-center gap-2">
            <Icon size={15} strokeWidth={2} class="text-primary-300" />
            <span class="text-sm font-medium text-surface-100">{space.label}</span>
          </div>
          <p class="mt-1 text-xs text-surface-500">{space.hint}</p>
        </li>
      {/each}
    </ul>
  </div>

  <div class="flex flex-wrap items-center justify-center gap-2">
    <button
      type="button"
      class="btn variant-filled-primary"
      onclick={() => void handleCreateDaily()}
      disabled={vault.saving}
    >
      Start today's daily
    </button>
    {#if vault.shouldPromptGarageWizard()}
      <button type="button" class="btn variant-soft-primary" onclick={handleSetupGarage}>
        <Wand2 size={16} strokeWidth={2} />
        Set up your garage
      </button>
    {/if}
    <button
      type="button"
      class="btn variant-soft-surface"
      onclick={() => void handlePinFolder()}
    >
      <Pin size={16} strokeWidth={2} />
      Link existing folder
    </button>
    <button
      type="button"
      class="btn variant-soft-surface"
      onclick={() => vault.openNewNoteDialog()}
    >
      New note…
    </button>
    {#if vault.lastNotePath}
      <button type="button" class="btn variant-ghost-surface" onclick={() => void handleOpenLast()}>
        Open last note
      </button>
    {/if}
  </div>

  <form
    class="flex w-full max-w-md gap-2"
    onsubmit={(event) => {
      event.preventDefault();
      void handleQuickCapture();
    }}
  >
    <input
      class="input flex-1 text-sm"
      type="text"
      placeholder="Quick capture to Inbox…"
      bind:value={captureLine}
    />
    <button
      type="submit"
      class="btn variant-soft-primary"
      disabled={!captureLine.trim() || vault.saving}
    >
      <Inbox size={16} strokeWidth={2} />
      Capture
    </button>
  </form>
</div>
