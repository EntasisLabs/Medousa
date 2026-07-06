<script lang="ts">
  import { Bell, MessageCircle } from "@lucide/svelte";
  import type { ComponentDef } from "$lib/types/environment";
  import { layout } from "$lib/stores/layout.svelte";

  interface Props {
    component: ComponentDef;
    variant?: "fab" | "inline" | "header";
  }

  let { component, variant = "inline" }: Props = $props();

  const action = $derived(
    typeof component.config.action === "string" ? component.config.action : "",
  );
  const label = $derived(component.label ?? "Action");

  function handleClick() {
    if (action === "open-ask") {
      layout.openAskSheet();
      return;
    }
    if (action === "open-activity") {
      layout.toggleActivitySheet();
    }
  }
</script>

{#if action === "open-ask" || action === "open-activity"}
  <button
    type="button"
    class="chrome-action"
    class:chrome-action-fab={variant === "fab"}
    class:chrome-action-inline={variant === "inline"}
    class:chrome-action-header={variant === "header"}
    aria-label={label}
    title={label}
    onclick={handleClick}
  >
    {#if action === "open-ask"}
      <MessageCircle size={variant === "fab" ? 22 : 18} strokeWidth={2} />
    {:else}
      <Bell size={variant === "fab" ? 22 : 18} strokeWidth={2} />
    {/if}
    {#if variant !== "fab"}
      <span>{label}</span>
    {/if}
  </button>
{/if}

<style>
  .chrome-action {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    border: 0;
    cursor: pointer;
    font-size: 0.75rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-700) 70%, transparent);
    border-radius: 999px;
    padding: 0.4rem 0.75rem;
  }

  .chrome-action-fab {
    position: fixed;
    right: 1rem;
    bottom: calc(5.5rem + env(safe-area-inset-bottom, 0px));
    z-index: 40;
    width: 3.25rem;
    height: 3.25rem;
    padding: 0;
    justify-content: center;
    border-radius: 999px;
    background: rgb(var(--color-primary-500));
    color: white;
    box-shadow: 0 12px 28px rgb(0 0 0 / 0.28);
  }

  .chrome-action-header {
    background: transparent;
    padding: 0.25rem 0.5rem;
  }

  .chrome-action-inline {
    margin: 0.5rem 0;
  }
</style>
