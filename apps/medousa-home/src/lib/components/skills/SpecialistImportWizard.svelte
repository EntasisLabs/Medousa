<script lang="ts">
  import { catalog } from "$lib/stores/catalog.svelte";
  import type { ManuscriptImportPreset } from "$lib/types/manuscript";
  import { pickExternalFolder } from "$lib/utils/externalDeskApi";
  import { isTauri } from "$lib/window";

  interface Props {
    open: boolean;
    onClose: () => void;
    onImported?: (ids: string[]) => void;
  }

  let { open, onClose, onImported }: Props = $props();

  let step = $state(1);
  let sourceMode = $state<"folder" | ManuscriptImportPreset>("folder");
  let folderPath = $state<string | null>(null);
  let scope = $state<"user" | "project">("user");
  let force = $state(false);
  let picking = $state(false);
  let resultNames = $state<string[]>([]);

  $effect(() => {
    if (open) {
      step = 1;
      sourceMode = "folder";
      folderPath = null;
      scope = "user";
      force = false;
      resultNames = [];
    }
  });

  async function pickFolder() {
    picking = true;
    try {
      folderPath = await pickExternalFolder();
    } finally {
      picking = false;
    }
  }

  async function runImport() {
    const request =
      sourceMode === "folder"
        ? {
            path: folderPath ?? undefined,
            scope,
            force,
          }
        : {
            preset: sourceMode,
            scope,
            force,
          };

    const response = await catalog.importSpecialists(request);
    resultNames = response.imported.map((entry) => entry.name);
    step = 3;
    onImported?.(response.imported.map((entry) => entry.id));
  }

  function canContinueStep1(): boolean {
    if (sourceMode === "folder") {
      return Boolean(folderPath?.trim());
    }
    return true;
  }

  const presets: { id: ManuscriptImportPreset; label: string; hint: string }[] = [
    { id: "hermes", label: "Hermes", hint: "~/.hermes/skills" },
    { id: "openclaw", label: "OpenClaw", hint: "~/.openclaw/skills" },
    { id: "cursor", label: "Cursor", hint: "~/.cursor/skills" },
  ];
</script>

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-surface-950/80 p-4 backdrop-blur-sm"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <div
      class="flex max-h-[min(640px,90vh)] w-full max-w-lg flex-col overflow-hidden rounded-xl border border-surface-500/45 bg-surface-900 shadow-2xl"
      role="dialog"
      aria-modal="true"
      aria-labelledby="specialist-import-title"
    >
      <div class="flex items-start justify-between gap-3 border-b border-surface-500/35 px-5 py-4">
        <div>
          <p class="text-[11px] font-semibold uppercase tracking-wide text-primary-300">
            Step {step} of 3
          </p>
          <h2 id="specialist-import-title" class="mt-1 text-lg font-semibold text-surface-50">
            {#if step === 1}
              Bring your skills
            {:else if step === 2}
              Import options
            {:else}
              Imported
            {/if}
          </h2>
        </div>
        <button type="button" class="workshop-text-action" onclick={onClose}>Close</button>
      </div>

      <div class="mobile-you-scroll flex-1 overflow-y-auto px-5 py-4">
        {#if step === 1}
          <p class="text-sm leading-relaxed text-surface-300">
            Import a folder with <span class="font-mono text-surface-200">SKILL.md</span> or pull
            from a library you already use elsewhere.
          </p>

          <div class="mt-4 space-y-2">
            <button
              type="button"
              class="flex w-full items-start gap-3 rounded-lg border px-3 py-3 text-left transition {sourceMode ===
              'folder'
                ? 'border-primary-500/40 bg-surface-800/80'
                : 'border-surface-500/35 hover:bg-surface-800/50'}"
              onclick={() => (sourceMode = "folder")}
            >
              <span class="font-medium text-surface-100">Choose folder</span>
              <span class="workshop-faint block text-xs">Any skill folder with SKILL.md</span>
            </button>

            {#if sourceMode === "folder"}
              <div class="rounded-lg border border-surface-500/35 px-3 py-3">
                {#if !isTauri()}
                  <p class="text-xs text-warning-400">
                    Folder import needs the Medousa desktop app.
                  </p>
                {:else if folderPath}
                  <p class="break-all font-mono text-[11px] text-surface-300">{folderPath}</p>
                {:else}
                  <p class="workshop-faint text-xs">No folder selected yet.</p>
                {/if}
                <button
                  type="button"
                  class="workshop-text-action mt-2 text-sm"
                  disabled={!isTauri() || picking}
                  onclick={() => void pickFolder()}
                >
                  {picking ? "Opening picker…" : "Browse…"}
                </button>
              </div>
            {/if}
          </div>

          <p class="workshop-label mt-5">Or import a library</p>
          <div class="mt-2 grid gap-2">
            {#each presets as preset (preset.id)}
              <button
                type="button"
                class="rounded-lg border px-3 py-2 text-left transition {sourceMode === preset.id
                  ? 'border-primary-500/40 bg-surface-800/80'
                  : 'border-surface-500/35 hover:bg-surface-800/50'}"
                onclick={() => (sourceMode = preset.id)}
              >
                <span class="font-medium text-surface-100">{preset.label}</span>
                <span class="workshop-faint block font-mono text-[11px]">{preset.hint}</span>
              </button>
            {/each}
          </div>
        {:else if step === 2}
          <label class="block text-sm">
            <span class="workshop-label">Install scope</span>
            <select class="input mt-1 w-full text-sm" bind:value={scope}>
              <option value="user">User — ~/.config/medousa/manuscripts</option>
              <option value="project">Project — .medousa/manuscripts</option>
            </select>
          </label>

          <label class="mt-4 flex items-center gap-2 text-sm text-surface-300">
            <input type="checkbox" class="checkbox" bind:checked={force} />
            Replace existing specialists with the same id
          </label>

          {#if catalog.importError}
            <p class="mt-4 text-xs text-error-400">{catalog.importError}</p>
          {/if}
        {:else}
          <p class="text-sm text-surface-200">
            Imported {resultNames.length} specialist{resultNames.length === 1 ? "" : "s"}.
          </p>
          {#if resultNames.length > 0}
            <ul class="mt-3 space-y-1 text-sm text-surface-300">
              {#each resultNames as name (name)}
                <li>{name}</li>
              {/each}
            </ul>
          {/if}
        {/if}
      </div>

      <div class="flex items-center justify-between gap-3 border-t border-surface-500/35 px-5 py-4">
        {#if step > 1 && step < 3}
          <button type="button" class="workshop-text-action" onclick={() => (step -= 1)}>
            Back
          </button>
        {:else}
          <span></span>
        {/if}

        {#if step === 1}
          <button
            type="button"
            class="btn btn-sm variant-filled-primary"
            disabled={!canContinueStep1()}
            onclick={() => (step = 2)}
          >
            Continue
          </button>
        {:else if step === 2}
          <button
            type="button"
            class="btn btn-sm variant-filled-primary"
            disabled={catalog.importBusy}
            onclick={() => void runImport()}
          >
            {catalog.importBusy ? "Importing…" : "Import"}
          </button>
        {:else}
          <button type="button" class="btn btn-sm variant-filled-primary" onclick={onClose}>
            Done
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}
