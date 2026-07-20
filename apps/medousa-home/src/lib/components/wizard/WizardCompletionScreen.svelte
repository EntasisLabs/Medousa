<script lang="ts">
  import { wizard } from "$lib/stores/wizard.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { isWorkspaceOnlyMode } from "$lib/utils/preferredMode";
  import { loadAssistantName, loadPrincipalName } from "$lib/utils/onboardingIdentity";
  import { workshopMonogram } from "$lib/types/workshopRegistry";
  import { COLOR_THEME_OPTIONS } from "$lib/theme/themeRegistry";

  const workspaceOnly = $derived(isWorkspaceOnlyMode());
  const spaceLabel = $derived(workshops.activeLabel || "Home");
  const principal = loadPrincipalName();
  const assistant = loadAssistantName() || "Medousa";
  const monogram = $derived(workshopMonogram(spaceLabel));
  const accent = $derived(
    COLOR_THEME_OPTIONS.find((t) => t.id === settings.colorTheme)?.swatches[1] ??
      "rgb(var(--color-primary-400))",
  );
</script>

<div class="wizard-step wizard-stagger items-center justify-center text-center">
  <div
    class="wizard-beat flex h-24 w-24 items-center justify-center rounded-full border-2 bg-surface-950/80 text-3xl font-semibold tracking-tight text-surface-50"
    style:border-color={accent}
    aria-hidden="true"
  >
    {monogram}
  </div>

  <h2
    id="product-wizard-title"
    class="wizard-beat mt-8 text-3xl font-semibold tracking-tight text-surface-50"
  >
    {#if isTauriMobilePlatform()}
      You're set
    {:else if workspaceOnly}
      {spaceLabel} is waiting
    {:else}
      {spaceLabel} is ready — with a brain
    {/if}
  </h2>

  <p class="wizard-beat mt-3 max-w-md text-base leading-relaxed text-surface-300">
    {#if isTauriMobilePlatform()}
      Open chat when you're linked to your computer.
    {:else if workspaceOnly}
      {#if principal}
        Go write something, {principal}. The desk is yours.
      {:else}
        Go write something. The desk is yours.
      {/if}
      <span class="mt-2 block text-sm text-surface-500">
        Add a brain later in Settings if you want.
      </span>
    {:else if principal}
      Open Notes — or talk to {assistant} when a thought shows up, {principal}.
    {:else}
      Open Notes — or talk to {assistant} when a thought shows up.
    {/if}
  </p>

  <div class="wizard-beat mt-12">
    <button
      type="button"
      class="btn variant-filled-primary wizard-cta min-h-12 px-12"
      disabled={wizard.busy}
      onclick={() => void wizard.finish()}
    >
      {isTauriMobilePlatform() ? "Open chat →" : "Open your desk →"}
    </button>
  </div>
</div>
