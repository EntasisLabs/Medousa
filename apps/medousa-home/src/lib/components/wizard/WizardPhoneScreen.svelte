<script lang="ts">
  import PhonePairPanel from "$lib/components/pairing/PhonePairPanel.svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import type { PairedDeviceSummary } from "$lib/utils/pairingApi";

  let paired = $state(false);

  function handlePaired(_device: PairedDeviceSummary) {
    paired = true;
  }
</script>

<div class="flex h-full flex-col">
  <button
    type="button"
    class="workshop-text-action self-start text-sm"
    disabled={wizard.busy}
    onclick={() => void wizard.back()}
  >
    ← Back
  </button>

  <p class="mt-4 text-[11px] font-semibold uppercase tracking-wide text-primary-300">
    Add your phone
  </p>
  <h2 class="mt-2 text-2xl font-semibold text-surface-50">Optional — link a mobile device</h2>
  <p class="mt-3 text-sm leading-relaxed text-surface-300">
    Scan this code from Medousa on your phone to talk from anywhere on your home network. You can
    skip and do this later in Settings → Phone.
  </p>

  <div class="mt-5 min-h-0 flex-1 overflow-y-auto">
    <PhonePairPanel mode="wizard" onPaired={handlePaired} />
  </div>

  <div class="mt-auto flex flex-wrap items-center justify-between gap-3 pt-6">
    <button
      type="button"
      class="btn variant-ghost min-h-11"
      disabled={wizard.busy}
      onclick={() => void wizard.skipCurrent()}
    >
      Skip for now
    </button>
    <button
      type="button"
      class="btn variant-filled-primary min-h-11 px-6"
      disabled={wizard.busy}
      onclick={() => void wizard.continue()}
    >
      {paired ? "Continue" : "Finish setup"}
    </button>
  </div>
</div>
