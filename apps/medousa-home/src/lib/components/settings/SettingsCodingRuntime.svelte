<script lang="ts">
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { CODING_RUNTIMES, normalizeCodingRuntimePreferences, type CodingRuntime, type CodingRuntimePreferences } from "$lib/types/workshopDefaults";

  const preferences = $derived(normalizeCodingRuntimePreferences(workshopDefaults.draft.codingRuntime));
  const remaining = $derived(CODING_RUNTIMES.filter((runtime) => runtime.value !== preferences.preferred && !preferences.fallbacks.includes(runtime.value)));
  const label = (runtime: CodingRuntime) => CODING_RUNTIMES.find((option) => option.value === runtime)?.label ?? runtime;

  function update(next: CodingRuntimePreferences) {
    workshopDefaults.draft = { ...workshopDefaults.draft, codingRuntime: next };
  }

  function move(index: number, offset: number) {
    const fallbacks = [...preferences.fallbacks];
    const target = index + offset;
    if (target < 0 || target >= fallbacks.length) return;
    [fallbacks[index], fallbacks[target]] = [fallbacks[target]!, fallbacks[index]!];
    update({ ...preferences, fallbacks });
  }
</script>

<div class="prefs-band">
  <div class="prefs-band-head">
    <h3 class="settings-subsection-heading">Coding runtime</h3>
    <p class="settings-subsection-lead">Choose once for future coding assignments. A runtime you request in chat takes priority.</p>
  </div>
  <label class="coding-picker">
    <span>Preferred runtime</span>
    <select value={preferences.preferred} disabled={workshopDefaults.saving || workshopDefaults.loading} onchange={(event) => {
      const preferred = event.currentTarget.value as CodingRuntime;
      update({ preferred, fallbacks: preferences.fallbacks.filter((runtime) => runtime !== preferred) });
    }}>
      {#each CODING_RUNTIMES as runtime}
        <option value={runtime.value}>{runtime.label}</option>
      {/each}
    </select>
  </label>
  <p class="settings-subsection-lead">If unavailable, try these fallbacks in order before starting work. With no fallbacks, Medousa will report the blocker.</p>
  <ol class="coding-fallbacks">
    {#each preferences.fallbacks as runtime, index (runtime)}
      <li>
        <span>{label(runtime)}</span>
        <button type="button" aria-label={`Move ${label(runtime)} earlier`} disabled={workshopDefaults.saving || workshopDefaults.loading || index === 0} onclick={() => move(index, -1)}>↑</button>
        <button type="button" aria-label={`Move ${label(runtime)} later`} disabled={workshopDefaults.saving || workshopDefaults.loading || index === preferences.fallbacks.length - 1} onclick={() => move(index, 1)}>↓</button>
        <button type="button" disabled={workshopDefaults.saving || workshopDefaults.loading} onclick={() => update({ ...preferences, fallbacks: preferences.fallbacks.filter((value) => value !== runtime) })}>Remove</button>
      </li>
    {/each}
  </ol>
  {#if remaining.length}
    <label class="coding-picker">
      <span>Add fallback</span>
      <select value="" disabled={workshopDefaults.saving || workshopDefaults.loading} onchange={(event) => {
        const runtime = event.currentTarget.value as CodingRuntime;
        if (runtime) update({ ...preferences, fallbacks: [...preferences.fallbacks, runtime] });
        event.currentTarget.value = "";
      }}>
        <option value="">Select runtime…</option>
        {#each remaining as runtime}<option value={runtime.value}>{runtime.label}</option>{/each}
      </select>
    </label>
  {/if}
</div>

<style>
  .prefs-band { margin-top: 1.25rem; }
  .prefs-band-head .settings-subsection-heading { margin-bottom: 0.15rem; }
  .prefs-band-head .settings-subsection-lead { margin-bottom: 0.6rem; }
  .coding-picker { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin: 12px 0; }
  select { padding: 6px 10px; max-width: 60%; border: 1px solid rgb(var(--theme-border) / 0.4); border-radius: var(--theme-control-radius, 6px); background: rgb(var(--theme-card)); color: inherit; }
  .coding-fallbacks { padding-left: 24px; }
  li { padding: 4px 0; }
  li span { display: inline-block; min-width: 120px; }
  button { padding: 4px 8px; }
</style>
