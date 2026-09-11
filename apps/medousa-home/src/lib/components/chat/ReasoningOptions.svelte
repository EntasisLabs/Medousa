<script lang="ts">
  import { Check } from "@lucide/svelte";
  import { REASONING_EFFORT_OPTIONS, type ReasoningCapability } from "$lib/types/reasoningEffort";
  let { capability, value, adjusted = false, disabled = false, onchange }: {
    capability: ReasoningCapability; value: string; adjusted?: boolean; disabled?: boolean; onchange: (value: string) => void;
  } = $props();
  const options = $derived(REASONING_EFFORT_OPTIONS.filter((option) => option.id === "default"
    || (capability.kind === "effort" && capability.levels.includes(option.id))));
  let budget = $state<number | undefined>();
  $effect(() => { budget = value.startsWith("budget:") ? Number(value.slice(7)) : capability.budgetMin ?? undefined; });
  const validBudget = $derived(budget !== undefined && Number.isSafeInteger(budget)
    && budget >= capability.budgetMin! && budget <= capability.budgetMax!);
</script>

<div class="reasoning-options" role="group" aria-label="Reasoning">
  {#if adjusted && capability.kind !== "unknown"}<p class="notice" role="status">Your saved choice is unavailable for this model. Using model default.</p>{/if}
  {#if capability.kind === "unknown"}
    <p class="notice">Reasoning options haven’t been confirmed for this model. Using model default.</p>
  {:else if capability.kind === "unsupported"}
    <p class="notice">This model has no adjustable reasoning setting.</p>
  {/if}
  {#each options as option (option.id)}
    <button type="button" class="option" aria-pressed={value === option.id} {disabled} onclick={() => onchange(option.id)}>
      <span><span class="label">{option.label}</span><span class="hint">{option.hint}</span></span>
      {#if value === option.id}<Check size={16} />{/if}
    </button>
  {/each}
  {#if capability.kind === "budget"}
    <form onsubmit={(event) => { event.preventDefault(); if (validBudget && !disabled) onchange(`budget:${budget}`); }}>
      <label>Thinking budget
        <span class="hint">{capability.budgetMin?.toLocaleString()}–{capability.budgetMax?.toLocaleString()} tokens{capability.budgetMin === 0 ? "; 0 turns thinking off" : ""}</span>
        <input type="number" min={capability.budgetMin ?? 0} max={capability.budgetMax ?? 0} step="1" bind:value={budget} {disabled} required />
      </label>
      <button type="submit" disabled={disabled || !validBudget}>Apply</button>
    </form>
  {/if}
</div>

<style>
  .reasoning-options { min-width: 0; }
  .option { display: flex; align-items: center; justify-content: space-between; gap: 12px; width: 100%; padding: 12px; text-align: left; border-radius: 10px; }
  .option:hover { background: var(--bg-hover, #ffffff08); }
  .label, .hint { display: block; }
  .hint, .notice { font-size: 12px; line-height: 1.5; opacity: .65; }
  .notice { padding: 8px 12px; margin: 0; }
  form { padding: 12px; }
  input { min-width: 0; width: 100%; padding: 10px; margin: 10px 0; border: 1px solid #ffffff25; border-radius: 8px; background: transparent; }
  form button { padding: 8px 12px; border-radius: 8px; background: #ffffff10; }
  button:disabled { opacity: .45; }
  @media (max-width: 640px) {
    .option { min-height: 60px; padding: 14px 18px; }
    .option + .option { border-top: 1px solid #ffffff08; border-radius: 0; }
    .hint, .notice { font-size: 14px; }
    .label { font-size: 16px; }
    .reasoning-options { border-radius: 18px; background: #ffffff03; overflow: hidden; }
  }
</style>
