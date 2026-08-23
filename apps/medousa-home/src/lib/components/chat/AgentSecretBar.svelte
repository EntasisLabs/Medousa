<script lang="ts">
  import { KeyRound, ShieldCheck } from "@lucide/svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { denyAgentSecretRequest, fulfillAgentSecretRequest } from "$lib/daemon";
  import { homeChannelSurface } from "$lib/platform";
  import { haptic } from "$lib/haptics";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();
  let value = $state("");
  let busy = $state(false);
  let feedback = $state<string | null>(null);
  let lastRequestId = $state<string | null>(null);
  const pending = $derived(chat.secretAlert);
  const isGrapheme = $derived(pending?.backend === "grapheme_runtime");

  $effect(() => {
    const requestId = pending?.requestId ?? null;
    if (requestId !== lastRequestId) {
      value = "";
      feedback = null;
      lastRequestId = requestId;
    }
  });

  async function fulfill(event: SubmitEvent) {
    event.preventDefault();
    if (!pending || busy || !value) return;
    const credential = value;
    value = "";
    busy = true;
    feedback = null;
    try {
      await fulfillAgentSecretRequest(
        pending.requestId,
        credential,
        homeChannelSurface(),
      );
      chat.clearSecretAlert();
      haptic("success");
    } catch (err) {
      feedback = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function deny() {
    if (!pending || busy) return;
    value = "";
    busy = true;
    feedback = null;
    try {
      await denyAgentSecretRequest(pending.requestId, homeChannelSurface());
      chat.clearSecretAlert();
      haptic("light");
    } catch (err) {
      feedback = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

{#if pending}
  <form
    class="{mobile
      ? 'mx-3 mb-2 rounded-xl border border-emerald-500/30 bg-emerald-950/35 p-3'
      : 'mx-4 mb-2 rounded-lg border border-emerald-500/25 bg-emerald-950/30 px-3 py-3'}"
    aria-label="Secure credential handoff"
    onsubmit={fulfill}
  >
    <div class="flex items-start gap-2.5">
      <div class="mt-0.5 rounded-md bg-emerald-400/10 p-1.5 text-emerald-300" aria-hidden="true">
        <ShieldCheck size={16} />
      </div>
      <div class="min-w-0 flex-1">
        <p class="text-xs font-semibold text-emerald-200">Secure credential handoff</p>
        <p class="mt-0.5 text-sm text-surface-100">{pending.reason}</p>
        <div class="mt-1.5 flex flex-wrap gap-x-3 gap-y-0.5 text-xs text-content-tertiary">
          <span>{pending.label}</span>
          <span class="font-mono">{pending.credentialKey}</span>
          <span>{isGrapheme ? "Grapheme runtime" : `OpenShell: ${pending.providerType}`}</span>
        </div>
        {#if isGrapheme && pending.allowedHosts.length > 0}
          <p class="mt-1 text-xs text-content-tertiary">
            Approved HTTPS hosts: <span class="font-mono">{pending.allowedHosts.join(", ")}</span>
          </p>
        {/if}
        <p id="secret-handoff-help" class="mt-2 text-xs leading-relaxed text-content-tertiary">
          {#if isGrapheme}
            On approval, it is held in zeroizing runtime memory for one Grapheme run—not
            added to chat, script state, or shown to the agent. Grapheme receives only an
            opaque handle.
            {#if pending.allowedHosts.length > 0}
              Medousa limits authenticated requests to the hosts above.
            {:else}
              Signing is allowed; authenticated HTTP is disabled.
            {/if}
          {:else}
            On approval, it is saved in OpenShell on this workshop—not added to chat or
            shown to the agent. The sandbox gets a placeholder, and policy controls where it
            can be used.
          {/if}
        </p>
        <div class="mt-2.5 flex flex-wrap items-center gap-2">
          <label class="relative min-w-0 flex-1">
            <span class="sr-only">{pending.label}</span>
            <KeyRound
              size={15}
              class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-content-tertiary"
              aria-hidden="true"
            />
            <input
              type="password"
              bind:value
              class="input h-9 w-full rounded-md border border-surface-500/60 bg-surface-950/70 pl-8 pr-2 text-sm"
              placeholder={pending.label}
              autocomplete="off"
              autocapitalize="none"
              spellcheck={false}
              aria-describedby="secret-handoff-help"
              disabled={busy}
            />
          </label>
          <button
            type="submit"
            class="btn btn-sm variant-filled-primary"
            disabled={busy || !value}
          >
            {isGrapheme ? "Authorize & continue" : "Store & continue"}
          </button>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            disabled={busy}
            onclick={() => void deny()}
          >
            Deny
          </button>
        </div>
        {#if feedback}
          <p class="mt-1.5 text-xs text-content-error" role="alert">{feedback}</p>
        {/if}
      </div>
    </div>
  </form>
{/if}
