<script lang="ts">
  import { BookOpen, FolderOpen, Inbox, Pin, Sparkles, Wallet, X } from "@lucide/svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import { vault } from "$lib/stores/vault.svelte";

  let step = $state(1);
  let pinning = $state(false);
  let starting = $state(false);

  const lifeSpaces = [
    {
      icon: BookOpen,
      label: "Journal",
      hint: "Daily notes and weekly reviews",
    },
    {
      icon: FolderOpen,
      label: "Projects",
      hint: "Goals, plans, and next steps",
    },
    {
      icon: Wallet,
      label: "Finance",
      hint: "Ledgers and linked spreadsheets",
    },
    {
      icon: Inbox,
      label: "Inbox",
      hint: "Quick captures before you sort them",
    },
  ];

  $effect(() => {
    if (vault.garageWizardOpen) step = 1;
  });

  function close() {
    vault.closeGarageWizard();
  }

  async function handlePinFolder() {
    pinning = true;
    try {
      await externalDesk.pinFolder();
    } finally {
      pinning = false;
    }
  }

  function finish() {
    vault.finishGarageOnboarding();
  }

  async function handleStartDaily() {
    starting = true;
    try {
      await vault.createDailyNote();
      finish();
    } finally {
      starting = false;
    }
  }

  function handleSkipToGarage() {
    finish();
  }
</script>

{#if vault.garageWizardOpen}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-surface-950/80 p-4 backdrop-blur-sm"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) close();
    }}
  >
    <div
      class="vault-garage-wizard flex max-h-[min(640px,90vh)] w-full max-w-lg flex-col overflow-hidden rounded-xl border border-surface-500/45 bg-surface-900 shadow-2xl"
      role="dialog"
      aria-modal="true"
      aria-labelledby="garage-wizard-title"
    >
      <div class="flex items-start justify-between gap-3 border-b border-surface-500/35 px-5 py-4">
        <div class="min-w-0">
          <p class="text-[11px] font-semibold uppercase tracking-wide text-primary-300">
            Step {step} of 3
          </p>
          <h2 id="garage-wizard-title" class="mt-1 text-lg font-semibold text-surface-50">
            {#if step === 1}
              Bring your mess
            {:else if step === 2}
              Link your desk
            {:else}
              You're set
            {/if}
          </h2>
        </div>
        <button
          type="button"
          class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-surface-500 hover:bg-surface-800 hover:text-surface-200"
          aria-label="Close setup"
          onclick={close}
        >
          <X size={16} strokeWidth={2} />
        </button>
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto px-5 py-5">
        {#if step === 1}
          <p class="text-sm leading-relaxed text-surface-300">
            Medousa Home is your garage — journals, projects, budgets, and the files you already
            have. Markdown underneath. Human on top.
          </p>

          <p class="mt-4 text-xs font-semibold uppercase tracking-wide text-surface-500">
            What belongs here
          </p>
          <ul class="mt-3 grid gap-2 sm:grid-cols-2">
            {#each lifeSpaces as space (space.label)}
              {@const Icon = space.icon}
              <li class="rounded-lg border border-surface-500/35 bg-surface-950/50 p-3 text-left">
                <div class="flex items-center gap-2">
                  <Icon size={16} strokeWidth={2} class="text-primary-300" />
                  <span class="text-sm font-medium text-surface-100">{space.label}</span>
                </div>
                <p class="mt-1 text-xs leading-snug text-surface-500">{space.hint}</p>
              </li>
            {/each}
          </ul>

          <p class="mt-4 text-xs text-surface-500">
            Bug reports and agent QA notes stay tucked away until you need them.
          </p>
        {:else if step === 2}
          <p class="text-sm leading-relaxed text-surface-300">
            Your real files live outside the vault too. Pin a folder from your Mac — Documents,
            Downloads, project dirs — and link PDFs or spreadsheets into notes without importing
            them.
          </p>

          <div class="mt-5 rounded-lg border border-dashed border-surface-500/45 bg-surface-950/40 p-5 text-center">
            <Pin size={24} strokeWidth={1.5} class="mx-auto text-surface-500" />
            <p class="mt-3 text-sm text-surface-200">Pin your first folder</p>
            <p class="mt-1 text-xs text-surface-500">Shows up under <strong class="font-medium text-surface-400">Your files</strong> in the sidebar</p>
            <button
              type="button"
              class="btn btn-sm variant-filled-primary mt-4"
              disabled={pinning}
              onclick={() => void handlePinFolder()}
            >
              {pinning ? "Opening picker…" : "Choose folder"}
            </button>
            {#if externalDesk.pinnedRoots.length > 0}
              <p class="mt-3 text-xs text-success-400">
                {externalDesk.pinnedRoots.length} folder{externalDesk.pinnedRoots.length === 1 ? "" : "s"} pinned
              </p>
            {/if}
          </div>
        {:else}
          <div class="flex flex-col items-center py-4 text-center">
            <div class="flex h-12 w-12 items-center justify-center rounded-full bg-primary-500/15 text-primary-300">
              <Sparkles size={22} strokeWidth={2} />
            </div>
            <p class="mt-4 max-w-sm text-sm leading-relaxed text-surface-300">
              Start with today's journal, capture a thought to Inbox, or browse what you already
              have. Your garage, your rhythm.
            </p>
          </div>
        {/if}
      </div>

      <div class="flex items-center justify-between gap-3 border-t border-surface-500/35 px-5 py-4">
        {#if step > 1}
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            onclick={() => (step -= 1)}
          >
            Back
          </button>
        {:else}
          <button type="button" class="btn btn-sm variant-ghost-surface" onclick={close}>
            Not now
          </button>
        {/if}

        <div class="flex gap-2">
          {#if step === 1}
            <button type="button" class="btn btn-sm variant-filled-primary" onclick={() => (step = 2)}>
              Continue
            </button>
          {:else if step === 2}
            <button
              type="button"
              class="btn btn-sm variant-soft-surface"
              onclick={() => (step = 3)}
            >
              Skip for now
            </button>
            <button
              type="button"
              class="btn btn-sm variant-filled-primary"
              onclick={() => (step = 3)}
            >
              Continue
            </button>
          {:else}
            <button
              type="button"
              class="btn btn-sm variant-soft-surface"
              onclick={handleSkipToGarage}
            >
              Browse vault
            </button>
            <button
              type="button"
              class="btn btn-sm variant-filled-primary"
              disabled={starting || vault.saving}
              onclick={() => void handleStartDaily()}
            >
              {starting ? "Opening…" : "Start daily note"}
            </button>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}
