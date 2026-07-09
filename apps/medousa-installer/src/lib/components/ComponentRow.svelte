<script lang="ts">
  import InstallerCheckbox from "./InstallerCheckbox.svelte";
  import StatusPill from "./StatusPill.svelte";

  interface Props {
    name: string;
    description?: string;
    sizeLabel: string;
    selected?: boolean;
    optional?: boolean;
    installed?: boolean;
    updateAvailable?: boolean;
    ontoggle?: () => void;
  }

  let {
    name,
    description = "",
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
  <div class="copy">
    <span class="name">{name}</span>
    {#if description}
      <span class="description">{description}</span>
    {/if}
  </div>
  {#if pill}
    <StatusPill label={pill} variant={pillVariant} />
  {/if}
  <span class="size" title="Download size">{sizeLabel}</span>
</div>

<style>
  .component-row {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: start;
    gap: 0.65rem;
    width: 100%;
    padding: 0.6rem 0.35rem;
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
    margin-top: 0.1rem;
  }

  .copy {
    min-width: 0;
    display: grid;
    gap: 0.1rem;
  }

  .name {
    font-size: var(--installer-body-size);
    font-weight: 500;
  }

  .description {
    font-size: var(--installer-caption-size);
    color: var(--installer-muted);
    line-height: 1.4;
  }

  .size {
    color: var(--installer-faint);
    font-size: 0.6875rem;
    justify-self: end;
    align-self: start;
    margin-top: 0.15rem;
  }
</style>
