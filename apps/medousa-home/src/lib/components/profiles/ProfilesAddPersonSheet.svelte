<script lang="ts">
  import { rememberIdentityFact } from "$lib/daemon";
  import type { IdentityRememberRequest } from "$lib/types/identity";
  import { withIdentityUserId } from "$lib/utils/identityTeach";

  interface Props {
    open: boolean;
    readOnly?: boolean;
    onClose: () => void;
    onSaved?: (parsed: IdentityRememberRequest) => void | Promise<void>;
  }

  let { open, readOnly = false, onClose, onSaved }: Props = $props();

  let name = $state("");
  let role = $state("");
  let busy = $state(false);
  let message = $state<string | null>(null);
  let ok = $state(false);

  $effect(() => {
    if (open) {
      name = "";
      role = "";
      message = null;
      ok = false;
    }
  });

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    const displayName = name.trim();
    const relationship = role.trim();
    if (!displayName || !relationship || readOnly) return;

    busy = true;
    message = null;
    try {
      const parsed = withIdentityUserId({
        fact_kind: "person",
        subject: displayName,
        statement: relationship,
        source: "user_direct",
      });
      const result = await rememberIdentityFact(parsed);
      ok = result.committed || !result.requires_confirmation;
      message = result.message;
      if (ok) {
        await onSaved?.(parsed);
        onClose();
      }
    } catch (err) {
      ok = false;
      message = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

{#if open}
  <div
    class="mobile-sheet-backdrop z-50"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <div class="mobile-sheet" role="dialog" aria-label="Add person">
      <header class="mobile-sheet-header">
        <h2 class="text-sm font-semibold text-surface-50">Add someone</h2>
        <button type="button" class="btn btn-sm variant-ghost-surface" onclick={() => onClose()}>
          Cancel
        </button>
      </header>
      <form class="space-y-3 px-4 pb-6 pt-2" onsubmit={submit}>
        <label class="block">
          <span class="workshop-label">Name</span>
          <input
            class="input mt-1 w-full text-sm"
            placeholder="Mario"
            bind:value={name}
            disabled={readOnly || busy}
          />
        </label>
        <label class="block">
          <span class="workshop-label">Relationship</span>
          <input
            class="input mt-1 w-full text-sm"
            placeholder="partner, colleague, mom…"
            bind:value={role}
            disabled={readOnly || busy}
          />
        </label>
        <button
          type="submit"
          class="btn btn-sm variant-filled-primary"
          disabled={readOnly || busy || !name.trim() || !role.trim()}
        >
          {busy ? "Saving…" : "Remember"}
        </button>
        {#if message}
          <p class="text-xs {ok ? 'text-success-400' : 'text-warning-400'}" role="status">
            {message}
          </p>
        {/if}
      </form>
    </div>
  </div>
{/if}
