<script lang="ts">
  import { LoaderCircle, Plus } from "@lucide/svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { parsePairQrUrl } from "$lib/utils/pairingUrl";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { workshopQrScanHint } from "$lib/platformCopy";

  interface Props {
    open: boolean;
    variant?: "mobile" | "desktop" | "rail";
    onClose: () => void;
    onJoined?: () => void;
    onHealthChange?: (health: import("$lib/daemon").DaemonHealth | null) => void;
  }

  let {
    open,
    variant = "mobile",
    onClose,
    onJoined,
    onHealthChange,
  }: Props = $props();

  let pairLink = $state("");
  let daemonUrlOverride = $state("");

  $effect(() => {
    if (open) {
      pairLink = "";
      daemonUrlOverride = "";
      workshops.joinError = null;
    }
  });

  function applyParsedLink() {
    const parsed = parsePairQrUrl(pairLink);
    if (parsed && !daemonUrlOverride.trim()) {
      daemonUrlOverride = parsed.daemonUrl;
    }
  }

  async function submitJoin() {
    applyParsedLink();
    try {
      await workshops.joinFromPairLink(pairLink, {
        daemonUrl: daemonUrlOverride.trim() || undefined,
      });
      onJoined?.();
      onClose();
    } catch {
      // Error on store.
    }
  }

  const parsedPreview = $derived(parsePairQrUrl(pairLink.trim()));
</script>

{#if open}
  <div
    class="mobile-sheet-backdrop {variant === 'rail' ? 'workshop-rail-sheet-backdrop' : ''}"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <div
      class="mobile-sheet {variant === 'rail' ? 'workshop-rail-sheet' : 'max-w-lg'}"
      role="dialog"
      aria-label="Add workshop"
    >
      <header class="mobile-sheet-header">
        <div class="min-w-0">
          <h2 class="text-sm font-semibold text-surface-50">Add workshop</h2>
          <p class="workshop-faint mt-0.5 text-xs leading-relaxed">
            Connect to another Medousa engine — scan or paste the invite link from your team.
          </p>
        </div>
        <button type="button" class="btn btn-sm variant-ghost-surface shrink-0" onclick={onClose}>
          Cancel
        </button>
      </header>

      <div class="mobile-you-scroll space-y-4 px-4 pb-6 pt-3">
        {#if workshops.atWorkshopLimit}
          <p class="rounded-lg border border-warning-500/35 bg-warning-500/10 px-3 py-2 text-xs text-warning-100">
            You have {workshops.workshops.length} workshops saved. Remove one in Settings before
            adding another.
          </p>
        {/if}

        {#if isTauriMobilePlatform()}
          <p class="workshop-faint text-xs leading-relaxed">{workshopQrScanHint()}</p>
        {/if}

        <label class="block" for="workshop-pair-link">
          <span class="workshop-label">Pairing link</span>
          <textarea
            id="workshop-pair-link"
            class="textarea mt-1 min-h-[5rem] w-full font-mono text-xs"
            placeholder="medousa://pair/2.0?a=…"
            bind:value={pairLink}
            oninput={applyParsedLink}
          ></textarea>
        </label>

        <label class="block" for="workshop-daemon-url">
          <span class="workshop-label">Workshop address</span>
          <input
            id="workshop-daemon-url"
            class="input mt-1 w-full font-mono text-xs"
            placeholder="http://192.168.1.42:7419"
            bind:value={daemonUrlOverride}
          />
          <span class="workshop-faint mt-1 block text-xs">
            Filled automatically from the link when possible.
          </span>
        </label>

        {#if parsedPreview}
          <p class="text-xs text-surface-300">
            Joining <span class="font-medium text-surface-100">{parsedPreview.peerName}</span>
          </p>
        {/if}

        {#if workshops.joinError}
          <p class="text-sm text-error-400">{workshops.joinError}</p>
        {/if}

        <button
          type="button"
          class="btn variant-filled-primary w-full"
          disabled={workshops.joinBusy || workshops.atWorkshopLimit || !pairLink.trim()}
          onclick={() => submitJoin()}
        >
          {#if workshops.joinBusy}
            <LoaderCircle class="mr-2 h-4 w-4 animate-spin" aria-hidden="true" />
            Joining…
          {:else}
            <Plus class="mr-2 h-4 w-4" aria-hidden="true" />
            Join workshop
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}
