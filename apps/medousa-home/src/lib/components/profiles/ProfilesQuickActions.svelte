<script lang="ts">
  import { Clock, UserPlus } from "@lucide/svelte";
  import { rememberIdentityFact } from "$lib/daemon";
  import { withIdentityUserId } from "$lib/utils/identityTeach";
  import type { IdentityRememberRequest } from "$lib/types/identity";

  interface Props {
    readOnly?: boolean;
    onAddPerson?: () => void;
    onSaved?: (parsed: IdentityRememberRequest) => void | Promise<void>;
  }

  let { readOnly = false, onAddPerson, onSaved }: Props = $props();

  let timezoneOpen = $state(false);
  let timezoneDraft = $state("");
  let timezoneBusy = $state(false);
  let timezoneMessage = $state<string | null>(null);

  async function saveTimezone() {
    const value = timezoneDraft.trim();
    if (!value || readOnly) return;
    timezoneBusy = true;
    timezoneMessage = null;
    try {
      const parsed = withIdentityUserId({
        fact_kind: "preference",
        subject: "timezone",
        statement: value,
        source: "user_direct",
      });
      const result = await rememberIdentityFact(parsed);
      timezoneMessage = result.message;
      if (result.committed || !result.requires_confirmation) {
        timezoneOpen = false;
        await onSaved?.(parsed);
      }
    } catch (err) {
      timezoneMessage = err instanceof Error ? err.message : String(err);
    } finally {
      timezoneBusy = false;
    }
  }
</script>

<div class="mt-5 flex flex-wrap gap-2">
  <button
    type="button"
    class="profiles-quick-chip"
    disabled={readOnly}
    onclick={() => onAddPerson?.()}
  >
    <UserPlus size={14} aria-hidden="true" />
    Add person
  </button>
  <button
    type="button"
    class="profiles-quick-chip"
    disabled={readOnly}
    onclick={() => {
      timezoneOpen = !timezoneOpen;
      timezoneMessage = null;
    }}
  >
    <Clock size={14} aria-hidden="true" />
    Set timezone
  </button>
</div>

{#if timezoneOpen}
  <div class="composer-bar mt-3 max-w-md">
    <input
      class="composer-bar-input w-full bg-transparent px-1 py-1 text-sm text-surface-100 focus:outline-none"
      placeholder="America/New_York"
      bind:value={timezoneDraft}
      disabled={readOnly || timezoneBusy}
    />
    <button
      type="button"
      class="composer-bar-send shrink-0"
      disabled={readOnly || timezoneBusy || !timezoneDraft.trim()}
      onclick={() => void saveTimezone()}
    >
      {timezoneBusy ? "…" : "Save"}
    </button>
  </div>
  {#if timezoneMessage}
    <p class="mt-2 text-xs text-warning-400">{timezoneMessage}</p>
  {/if}
{/if}
