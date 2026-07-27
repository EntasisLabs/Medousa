<script lang="ts">
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { X } from "@lucide/svelte";
  import { tick } from "svelte";

  interface Props {
    open: boolean;
    readOnly?: boolean;
    onClose: () => void;
  }

  let { open, readOnly = false, onClose }: Props = $props();

  let slug = $state("");
  let name = $state("");
  let nameEl: HTMLInputElement | undefined = $state();
  let wasOpen = $state(false);

  $effect(() => {
    if (open && !wasOpen) {
      slug = "";
      name = "";
      void tick().then(() => nameEl?.focus());
    }
    wasOpen = open;
  });

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (readOnly) return;
    const ok = await userProfiles.create(slug, name);
    if (ok) {
      slug = "";
      name = "";
      onClose();
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
    aria-labelledby="profiles-add-profile-title"
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
          <h3 id="profiles-add-profile-title" class="vault-interact-title">New profile</h3>
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
        placeholder="Work, home, studio…"
        bind:value={name}
        disabled={readOnly || userProfiles.saving}
        autocomplete="off"
      />

      <p class="vault-compose-sentence profiles-create-sentence">
        short id
        <input
          class="profiles-create-inline profiles-create-inline--mono"
          type="text"
          placeholder="work"
          bind:value={slug}
          disabled={readOnly || userProfiles.saving}
          autocomplete="off"
          spellcheck="false"
        />
      </p>

      <p class="vault-interact-note">
        Optional. Use another profile when you want different facts or preferences kept apart.
      </p>

      <div class="vault-compose-footer">
        <button
          type="submit"
          class="vault-interact-commit"
          disabled={readOnly || userProfiles.saving || !name.trim() || !slug.trim()}
        >
          {userProfiles.saving ? "Creating…" : "Create"}
        </button>
      </div>
    </form>
  </div>
{/if}
