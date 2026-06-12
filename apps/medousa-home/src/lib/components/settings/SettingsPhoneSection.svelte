<script lang="ts">
  import PhonePairPanel from "$lib/components/pairing/PhonePairPanel.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Phone</h2>
    <p class="workshop-faint mt-1 text-sm">
      Pair your phone as a second portal to this brain — same Wi‑Fi, private by default.
    </p>
  </header>

  {#if mobile && isTauriMobilePlatform()}
    <div
      class="mt-5 rounded-xl border border-surface-500/35 bg-surface-950/50 px-4 py-5 text-sm leading-relaxed text-surface-300"
    >
      Pairing is managed on your Mac. Open Medousa → Settings → Phone to
      scan the QR code, then connect this app under Settings → Connection using your computer's LAN address.
    </div>
  {:else}
    <div class="mt-5">
      <PhonePairPanel mode="settings" />
    </div>
  {/if}
</section>
