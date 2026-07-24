<script lang="ts">
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import { vaultVersions } from "$lib/stores/vaultVersions.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  const readOnly = $derived(mobile && isTauriMobilePlatform());
  const versionsOn = $derived(workshopDefaults.draft.vaultGitEnabled ?? false);

  $effect(() => {
    if (!workshopDefaults.loaded) return;
    void vaultVersions.refresh();
  });

  function setVersions(enabled: boolean) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      vaultGitEnabled: enabled,
    };
  }

  async function applyEnable(enabled: boolean) {
    setVersions(enabled);
    try {
      await vaultVersions.setEnabled(enabled, true);
    } catch {
      /* error surfaced on store */
    }
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Versions</h2>
    <p class="workshop-faint mt-1 text-sm">
      Optional vault history. Off by default. Everyday language — Advanced Git stays tucked away.
    </p>
  </header>

  <div class="mt-5">
    <h3 class="settings-subsection-heading">Vault versioning</h3>
    <p class="settings-subsection-lead">
      Save named versions of notes, browse history, and restore. Backed by Git on this machine only.
    </p>
    <div class="mt-1 grid gap-2 sm:grid-cols-2">
      <button
        type="button"
        class="settings-depth-card {!versionsOn ? 'settings-depth-card-active' : ''}"
        disabled={readOnly || vaultVersions.busy}
        aria-pressed={!versionsOn}
        onclick={() => void applyEnable(false)}
      >
        <span class="block text-sm font-medium text-surface-100">Off</span>
        <span class="workshop-faint mt-1 block text-xs leading-snug">
          No history — saves keep using normal note conflict checks
        </span>
      </button>
      <button
        type="button"
        class="settings-depth-card {versionsOn ? 'settings-depth-card-active' : ''}"
        disabled={readOnly || vaultVersions.busy}
        aria-pressed={versionsOn}
        onclick={() => void applyEnable(true)}
      >
        <span class="block text-sm font-medium text-surface-100">On</span>
        <span class="workshop-faint mt-1 block text-xs leading-snug">
          Enable Versions for the active vault root
        </span>
      </button>
    </div>
  </div>

  {#if versionsOn}
    <div class="mt-5 space-y-3">
      <h3 class="settings-subsection-heading">Git on this device</h3>
      {#if vaultVersions.detect}
        <p class="workshop-faint text-sm leading-snug">
          {#if vaultVersions.detect.available}
            Found {vaultVersions.detect.version ?? "Git"}
            {#if vaultVersions.detect.path}
              <span class="block truncate font-mono text-xs opacity-80"
                >{vaultVersions.detect.path}</span
              >
            {/if}
          {:else}
            Git is not available yet.
            <span class="mt-1 block">{vaultVersions.detect.platformHint}</span>
          {/if}
        </p>
      {/if}

      {#if vaultVersions.status}
        <p class="text-sm text-surface-200">
          {#if !vaultVersions.status.available}
            Install Git to start versioning.
          {:else if !vaultVersions.status.isRepo}
            Vault is ready — start versioning to create the first snapshot store.
          {:else}
            Branch
            <span class="font-medium text-surface-50"
              >{vaultVersions.status.branch ?? "detached"}</span
            >
            ·
            {vaultVersions.status.dirtyCount === 0
              ? "clean"
              : `${vaultVersions.status.dirtyCount} changed`}
          {/if}
        </p>
      {/if}

      <div class="flex flex-wrap gap-2">
        {#if vaultVersions.detect && !vaultVersions.detect.available}
          <button
            type="button"
            class="btn btn-sm variant-soft-surface"
            disabled={readOnly || vaultVersions.busy}
            onclick={() => void vaultVersions.installGit()}
          >
            Install / locate Git
          </button>
        {/if}
        {#if vaultVersions.status?.available && !vaultVersions.status.isRepo}
          <button
            type="button"
            class="btn btn-sm variant-filled-primary"
            disabled={readOnly || vaultVersions.busy}
            onclick={() => void vaultVersions.startVersioning()}
          >
            Start versioning
          </button>
        {/if}
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          disabled={vaultVersions.busy}
          onclick={() => void vaultVersions.refresh()}
        >
          Refresh status
        </button>
      </div>
    </div>
  {/if}

  {#if vaultVersions.error}
    <p class="mt-3 text-sm text-rose-300/90" role="alert">{vaultVersions.error}</p>
  {/if}

  <div class="mt-6">
    <SettingsCharterSaveBar {mobile} />
  </div>
</section>
