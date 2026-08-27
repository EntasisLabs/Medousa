<script lang="ts">
  import { getRuntimeWorkerConfig, putRuntimeWorkerConfig } from "$lib/daemon";
  import { workshops } from "$lib/stores/workshops.svelte";
  import type { RuntimeWorkerConfig } from "$lib/types/runtime";

  type ShareKey = "agents" | "scheduled" | "delivery" | "maintenance";

  const DEFAULTS: RuntimeWorkerConfig = {
    maxInFlight: 8,
    agents: 2,
    scheduled: 2,
    delivery: 1,
    maintenance: 1,
  };

  const fields: Array<{ key: ShareKey; label: string; hint: string }> = [
    { key: "agents", label: "Agents", hint: "Delegated and long-running LLM work" },
    { key: "scheduled", label: "Scheduled", hint: "Recurring and general queued jobs" },
    { key: "delivery", label: "Delivery", hint: "Outbox and channel publication" },
    { key: "maintenance", label: "Maintenance", hint: "Retention and background upkeep" },
  ];

  let draft = $state<RuntimeWorkerConfig>({ ...DEFAULTS });
  let baseline = $state(JSON.stringify(DEFAULTS));
  let loadedFor = $state<string | null>(null);
  let requestSerial = 0;
  let loading = $state(false);
  let saving = $state(false);
  let message = $state<string | null>(null);
  let error = $state<string | null>(null);

  const reserved = $derived(
    draft.agents + draft.scheduled + draft.delivery + draft.maintenance,
  );
  const flexible = $derived(Math.max(0, draft.maxInFlight - reserved));
  const dirty = $derived(JSON.stringify(draft) !== baseline);
  const validationError = $derived.by(() => {
    if (!Number.isInteger(draft.maxInFlight) || draft.maxInFlight < 1) {
      return "Global capacity must be at least 1.";
    }
    if (fields.some(({ key }) => !Number.isInteger(draft[key]) || draft[key] < 0)) {
      return "Lane shares must be whole numbers of zero or more.";
    }
    if (reserved > draft.maxInFlight) {
      return `Lane shares total ${reserved}, which exceeds global capacity ${draft.maxInFlight}.`;
    }
    return null;
  });

  $effect(() => {
    const workshopId = workshops.activeWorkshopId;
    if (!workshopId || loadedFor === workshopId) return;
    loadedFor = workshopId;
    void load(workshopId);
  });

  async function load(workshopId: string) {
    const serial = ++requestSerial;
    loading = true;
    error = null;
    message = null;
    try {
      const config = await getRuntimeWorkerConfig();
      if (serial !== requestSerial || workshops.activeWorkshopId !== workshopId) return;
      draft = { ...config };
      baseline = JSON.stringify(config);
    } catch (err) {
      if (serial !== requestSerial) return;
      error = err instanceof Error ? err.message : String(err);
    } finally {
      if (serial === requestSerial) loading = false;
    }
  }

  function setNumber(key: keyof RuntimeWorkerConfig, event: Event) {
    const raw = Number((event.currentTarget as HTMLInputElement).value);
    const minimum = key === "maxInFlight" ? 1 : 0;
    draft = {
      ...draft,
      [key]: Number.isFinite(raw) ? Math.max(minimum, Math.round(raw)) : minimum,
    };
    message = null;
  }

  async function save() {
    if (validationError) return;
    saving = true;
    error = null;
    message = null;
    try {
      const saved = await putRuntimeWorkerConfig(draft);
      draft = { ...saved };
      baseline = JSON.stringify(saved);
      message = "Saved — restart the engine to apply the new capacity.";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }
</script>

<div class="capacity-band">
  <div class="capacity-head">
    <div>
      <h3 class="settings-subsection-heading">Worker capacity</h3>
      <p class="settings-subsection-lead">
        Queue concurrency and preferred lane shares. Idle lanes lend capacity automatically.
      </p>
    </div>
    <span class="capacity-total">{reserved}/{draft.maxInFlight} reserved</span>
  </div>

  {#if loading}
    <p class="workshop-faint text-sm">Loading worker capacity…</p>
  {:else}
    <div class="capacity-grid">
      <label class="capacity-tile capacity-global">
        <span class="capacity-copy">
          <span class="capacity-title">Global capacity</span>
          <span class="capacity-meta">Maximum jobs and deliveries in flight</span>
        </span>
        <input
          type="number"
          min="1"
          step="1"
          inputmode="numeric"
          value={draft.maxInFlight}
          disabled={saving}
          aria-label="Global worker capacity"
          oninput={(event) => setNumber("maxInFlight", event)}
        />
      </label>

      {#each fields as field (field.key)}
        <label class="capacity-tile">
          <span class="capacity-copy">
            <span class="capacity-title">{field.label}</span>
            <span class="capacity-meta">{field.hint}</span>
          </span>
          <input
            type="number"
            min="0"
            step="1"
            inputmode="numeric"
            value={draft[field.key]}
            disabled={saving}
            aria-label="{field.label} preferred worker share"
            oninput={(event) => setNumber(field.key, event)}
          />
        </label>
      {/each}
    </div>

    <div class="capacity-footer">
      <div class="capacity-status">
        <span>{flexible} flexible {flexible === 1 ? "slot" : "slots"}</span>
        <span aria-hidden="true">·</span>
        <span>Changes apply after an engine restart</span>
      </div>
      <div class="capacity-actions">
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            disabled={saving}
            onclick={() => {
              draft = { ...DEFAULTS };
              message = null;
            }}
          >
            Restore defaults
          </button>
          <button
            type="button"
            class="btn btn-sm variant-filled-primary"
            disabled={saving || !dirty || Boolean(validationError)}
            onclick={save}
          >
            {saving ? "Saving…" : "Save capacity"}
          </button>
      </div>
    </div>

    {#if validationError}
      <p class="capacity-error" role="alert">{validationError}</p>
    {:else if error}
      <p class="capacity-error" role="alert">{error}</p>
    {:else if message}
      <p class="capacity-success" role="status">{message}</p>
    {:else if dirty}
      <p class="capacity-dirty">Unsaved capacity changes</p>
    {/if}
  {/if}
</div>

<style>
  .capacity-band {
    margin-top: 1rem;
    padding-top: 1rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.28);
  }

  .capacity-head,
  .capacity-footer,
  .capacity-tile {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .capacity-total {
    flex: 0 0 auto;
    border-radius: 999px;
    background: rgb(var(--color-primary-500) / 0.12);
    padding: 0.25rem 0.55rem;
    font-size: 0.68rem;
    color: rgb(var(--theme-link));
  }

  .capacity-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.5rem;
    margin-top: 0.75rem;
  }

  .capacity-tile {
    min-height: 3.25rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.28);
    border-radius: 0.65rem;
    background: rgb(var(--color-surface-900) / 0.38);
    padding: 0.55rem 0.75rem;
  }

  .capacity-global {
    grid-column: 1 / -1;
    border-color: rgb(var(--color-primary-500) / 0.28);
    background: rgb(var(--color-primary-500) / 0.06);
  }

  .capacity-copy {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 0.1rem;
  }

  .capacity-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .capacity-meta,
  .capacity-status,
  .capacity-dirty {
    font-size: 0.68rem;
    color: rgb(var(--theme-text-quiet));
  }

  input {
    width: 4rem;
    flex: 0 0 auto;
    border: 1px solid rgb(var(--color-surface-500) / 0.4);
    border-radius: 0.4rem;
    background: rgb(var(--color-surface-950) / 0.45);
    padding: 0.25rem 0.4rem;
    text-align: right;
    font-size: 0.8rem;
    color: rgb(var(--color-surface-100));
  }

  input:focus {
    outline: none;
    border-color: rgb(var(--color-primary-500) / 0.55);
    box-shadow: 0 0 0 2px rgb(var(--color-primary-500) / 0.18);
  }

  input:disabled {
    opacity: 0.5;
  }

  .capacity-footer {
    margin-top: 0.75rem;
    align-items: flex-start;
  }

  .capacity-status,
  .capacity-actions {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .capacity-error,
  .capacity-success,
  .capacity-dirty {
    margin-top: 0.55rem;
    font-size: 0.72rem;
  }

  .capacity-error {
    color: rgb(var(--theme-warning));
  }

  .capacity-success {
    color: rgb(var(--theme-success));
  }

  @media (max-width: 620px) {
    .capacity-grid {
      grid-template-columns: 1fr;
    }

    .capacity-global {
      grid-column: auto;
    }

    .capacity-footer {
      flex-direction: column;
    }
  }
</style>
