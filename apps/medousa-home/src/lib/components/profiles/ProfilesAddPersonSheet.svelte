<script lang="ts">
  import { rememberIdentityFact } from "$lib/daemon";
  import type { IdentityRememberRequest } from "$lib/types/identity";
  import { withIdentityUserId } from "$lib/utils/identityTeach";
  import { X } from "@lucide/svelte";
  import { tick } from "svelte";

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
  let nameEl: HTMLInputElement | undefined = $state();
  let wasOpen = $state(false);

  $effect(() => {
    if (open && !wasOpen) {
      name = "";
      role = "";
      message = null;
      ok = false;
      void tick().then(() => nameEl?.focus());
    }
    wasOpen = open;
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

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="vault-interact-backdrop"
    role="dialog"
    aria-modal="true"
    aria-labelledby="profiles-add-person-title"
    tabindex="-1"
    onkeydown={onKeydown}
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <form class="vault-interact-sheet vault-compose-sheet profiles-create-sheet" onsubmit={submit}>
      <header class="vault-interact-header vault-compose-header">
        <div class="min-w-0">
          <p class="vault-interact-kicker">You</p>
          <h3 id="profiles-add-person-title" class="vault-interact-title">Remember someone</h3>
        </div>
        <button
          type="button"
          class="vault-interact-dismiss"
          aria-label="Close"
          onclick={() => onClose()}
        >
          <X size={14} strokeWidth={2} />
        </button>
      </header>

      <input
        bind:this={nameEl}
        class="vault-compose-title"
        type="text"
        placeholder="Their name"
        bind:value={name}
        disabled={readOnly || busy}
        autocomplete="off"
      />

      <p class="vault-compose-sentence profiles-create-sentence">
        They’re my
        <input
          class="profiles-create-inline"
          type="text"
          placeholder="colleague, collaborator, family…"
          bind:value={role}
          disabled={readOnly || busy}
          autocomplete="off"
        />
      </p>

      {#if message}
        <p class="text-xs {ok ? 'text-success-400' : 'text-warning-400'}" role="status">
          {message}
        </p>
      {/if}

      <div class="vault-compose-footer">
        <button
          type="submit"
          class="vault-interact-commit"
          disabled={readOnly || busy || !name.trim() || !role.trim()}
        >
          {busy ? "Saving…" : "Remember"}
        </button>
      </div>
    </form>
  </div>
{/if}
