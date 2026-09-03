<script lang="ts">
  import { untrack } from "svelte";
  import {
    updatePairingPolicy,
    type PairedDeviceSummary,
  } from "$lib/utils/pairingApi";

  type ExpirationChoice = "until-removed" | "7d" | "30d" | "90d" | "custom";
  type InactivityChoice = "never" | "7d" | "30d" | "90d";

  interface Props {
    device: PairedDeviceSummary;
    onupdated?: (device: PairedDeviceSummary) => void;
    onforget?: (pairingId: string) => Promise<void> | void;
    onerror?: (message: string) => void;
  }

  let { device, onupdated, onforget, onerror }: Props = $props();
  let saving = $state(false);
  let expiration = $state<ExpirationChoice>(
    untrack(() => (device.trustExpiresAt ? "custom" : "until-removed")),
  );
  let customDate = $state(untrack(() => dateInputValue(device.trustExpiresAt)));
  let inactivity = $state<InactivityChoice>(
    untrack(() => inactivityChoice(device.idleTimeoutSeconds)),
  );

  function inactivityChoice(seconds: number | null | undefined): InactivityChoice {
    if (seconds === 7 * 86_400) return "7d";
    if (seconds === 30 * 86_400) return "30d";
    if (seconds === 90 * 86_400) return "90d";
    return "never";
  }

  function dateInputValue(value: string | null | undefined): string {
    if (!value) return "";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return "";
    const month = String(date.getMonth() + 1).padStart(2, "0");
    const day = String(date.getDate()).padStart(2, "0");
    return `${date.getFullYear()}-${month}-${day}`;
  }

  function trustExpiration(): string | null {
    if (expiration === "until-removed") return null;
    if (expiration === "custom") {
      if (!customDate) throw new Error("Choose a trust expiration date.");
      const expiry = new Date(`${customDate}T23:59:59`);
      if (Number.isNaN(expiry.getTime()) || expiry.getTime() <= Date.now()) {
        throw new Error("Trust expiration must be in the future.");
      }
      return expiry.toISOString();
    }
    const days = Number.parseInt(expiration, 10);
    return new Date(Date.now() + days * 86_400_000).toISOString();
  }

  function idleTimeoutSeconds(): number | null {
    if (inactivity === "never") return null;
    return Number.parseInt(inactivity, 10) * 86_400;
  }

  async function savePolicy() {
    if (saving) return;
    saving = true;
    try {
      const updated = await updatePairingPolicy(device.pairingId, {
        trustExpiresAt: trustExpiration(),
        idleTimeoutSeconds: idleTimeoutSeconds(),
      });
      onupdated?.(updated);
    } catch (error) {
      onerror?.(error instanceof Error ? error.message : String(error));
    } finally {
      saving = false;
    }
  }

  async function forgetDevice() {
    if (saving) return;
    saving = true;
    try {
      await onforget?.(device.pairingId);
    } finally {
      saving = false;
    }
  }
</script>

<div class="pair-policy-body">
  <p class="pair-footnote">
    This device stays trusted independently of its short-lived session. Sessions renew
    automatically while this policy allows it.
  </p>
  <div class="pair-policy-grid">
    <label class="pair-policy-field">
      <span>Trust expires</span>
      <select class="input text-sm" bind:value={expiration}>
        <option value="until-removed">Until removed</option>
        <option value="7d">In 7 days</option>
        <option value="30d">In 30 days</option>
        <option value="90d">In 90 days</option>
        <option value="custom">Custom date</option>
      </select>
    </label>
    <label class="pair-policy-field">
      <span>Expire if unused</span>
      <select class="input text-sm" bind:value={inactivity}>
        <option value="never">Never</option>
        <option value="7d">After 7 days</option>
        <option value="30d">After 30 days</option>
        <option value="90d">After 90 days</option>
      </select>
    </label>
  </div>
  {#if expiration === "custom"}
    <label class="pair-policy-field pair-policy-custom">
      <span>Expiration date</span>
      <input
        class="input text-sm"
        type="date"
        min={dateInputValue(new Date(Date.now() + 86_400_000).toISOString())}
        bind:value={customDate}
      />
    </label>
  {/if}
  <div class="pair-policy-actions">
    <button type="button" class="btn btn-sm variant-soft" disabled={saving} onclick={savePolicy}>
      {saving ? "Saving…" : "Save trust"}
    </button>
    <button
      type="button"
      class="pair-tile-cta pair-tile-cta-danger"
      disabled={saving}
      aria-label="Forget {device.phoneName}"
      onclick={forgetDevice}
    >
      Forget device
    </button>
  </div>
</div>

<style>
  .pair-policy-body {
    display: grid;
    gap: 0.75rem;
    padding: 0 0.75rem 0.75rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.22);
  }

  .pair-footnote {
    margin: 0.55rem 0 0;
    font-size: 0.7rem;
    line-height: 1.4;
    color: rgb(var(--theme-text-quiet));
  }

  .pair-policy-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.65rem;
  }

  .pair-policy-field {
    display: grid;
    gap: 0.3rem;
    font-size: 0.68rem;
    font-weight: 600;
    color: rgb(var(--theme-text-tertiary));
  }

  .pair-policy-custom {
    max-width: 14rem;
  }

  .pair-policy-actions {
    display: flex;
    align-items: center;
    gap: 0.8rem;
  }

  .pair-tile-cta {
    flex-shrink: 0;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--theme-text-tertiary));
    cursor: pointer;
  }

  .pair-tile-cta-danger {
    color: rgb(var(--theme-error) / 0.9);
  }

  @media (max-width: 34rem) {
    .pair-policy-grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
