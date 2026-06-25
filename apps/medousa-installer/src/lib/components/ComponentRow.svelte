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
</script>

<div class="component-row">
  <InstallerCheckbox
    checked={selected}
    disabled={!optional}
    onchange={() => ontoggle?.()}
  />
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
    padding: 0.45rem 0;
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
