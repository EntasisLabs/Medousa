<script lang="ts">
  import { Brain, Plus, UserPlus } from "@lucide/svelte";
  import {
    dispatchProfilesAddPerson,
    dispatchProfilesAddProfile,
    dispatchProfilesFocusTeach,
  } from "$lib/utils/profilesChromeEvents";

  interface Props {
    onAction?: () => void;
    variant?: "popover" | "rail-row";
  }

  let { onAction, variant = "popover" }: Props = $props();

  function addPerson() {
    onAction?.();
    dispatchProfilesAddPerson();
  }

  function focusTeach() {
    onAction?.();
    dispatchProfilesFocusTeach();
  }

  function addProfile() {
    onAction?.();
    dispatchProfilesAddProfile();
  }
</script>

{#if variant === "popover"}
  <div class="lme-dock-leading-ghost min-w-0 flex-1" aria-hidden="true"></div>
{/if}

<button
  type="button"
  class="vault-dock-icon-btn"
  title="Add person"
  aria-label="Add person"
  onclick={addPerson}
>
  <UserPlus size={15} strokeWidth={1.75} />
</button>

<button
  type="button"
  class="vault-dock-icon-btn"
  title="Teach"
  aria-label="Teach something"
  onclick={focusTeach}
>
  <Brain size={15} strokeWidth={1.75} />
</button>

{#if variant === "popover"}
  <div class="lme-dock-chrome-secondary flex shrink-0 items-center gap-0.5">
    <button
      type="button"
      class="vault-dock-icon-btn"
      title="Add profile"
      aria-label="Add profile"
      onclick={addProfile}
    >
      <Plus size={15} strokeWidth={1.75} />
    </button>
  </div>
{/if}
