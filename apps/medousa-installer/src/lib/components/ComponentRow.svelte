<script lang="ts">
  import InstallerCheckbox from "./InstallerCheckbox.svelte";
  import StatusPill from "./StatusPill.svelte";

  interface Props {
    name: string;
    sizeLabel: string;
    selected?: boolean;
    optional?: boolean;
    installed?: boolean;
    updateAvailable?: boolean;
    ontoggle?: () => void;
  }

  let {
    name,
    sizeLabel,
    selected = false,
    optional = true,
    installed = false,
    updateAvailable = false,
    ontoggle,
  }: Props = $props();

  const pill = $derived(
    updateAvailable ? "update available" : installed ? "installed" : null,
  );
  const pillVariant = $derived(
    updateAvailable ? "update" : installed ? "installed" : "default",
  );

  function handleKeydown(event: KeyboardEvent) {
    if (!optional) return;
    if (event.key === " " || event.key === "Enter") {
      event.preventDefault();
      ontoggle?.();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="component-row"
  class:selected
  class:locked={!optional}
  role={optional ? "checkbox" : "group"}
  aria-checked={optional ? selected : undefined}
  aria-disabled={!optional}
  tabindex={optional ? 0 : -1}
  onclick={() => optional && ontoggle?.()}
  onkeydown={handleKeydown}
>
  <InstallerCheckbox checked={selected} disabled={!optional} />
  <span class="name">{name}</span>
  {#if pill}
    <StatusPill label={pill} variant={pillVariant} />
  {/if}
  <span class="size">{sizeLabel}</span>
</div>

<style>
  .component-row {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: center;
    gap: 0.65rem;
    width: 100%;
    padding: 0.5rem 0.35rem;
    margin: 0 -0.35rem;
    border-radius: var(--installer-radius-control);
    background: transparent;
    color: inherit;
    transition: background var(--installer-motion);
    cursor: default;
  }

  .component-row:not(.locked) {
    cursor: pointer;
  }

  .component-row:not(.locked):hover {
    background: var(--installer-surface-raised);
  }

  .component-row.selected:not(.locked) {
    background: rgb(131 68 245 / 0.06);
  }

  .component-row:focus-visible {
    outline: 2px solid var(--installer-accent);
    outline-offset: -2px;
  }

  .component-row :global(.installer-checkbox) {
    pointer-events: none;
  }

  .name {
    min-width: 0;
    font-size: var(--installer-body-size);
  }

  .size {
    color: var(--installer-muted);
    font-size: var(--installer-caption-size);
    justify-self: end;
  }
</style>
