<script lang="ts">
  import { ChevronRight } from "@lucide/svelte";

  interface Props {
    label: string;
    value?: string;
    hint?: string | null;
    monogram?: string | null;
    valueAccent?: boolean;
    disabled?: boolean;
    destructive?: boolean;
    expanded?: boolean;
    onclick?: () => void;
  }

  let {
    label,
    value = "",
    hint = null,
    monogram = null,
    valueAccent = false,
    disabled = false,
    destructive = false,
    expanded = false,
    onclick,
  }: Props = $props();
</script>

<button
  type="button"
  class="settings-native-row"
  class:settings-native-row-destructive={destructive}
  class:settings-native-row-expanded={expanded}
  {disabled}
  {onclick}
>
  {#if monogram}
    <span class="settings-native-row-monogram" aria-hidden="true">{monogram}</span>
  {/if}
  <span class="settings-native-row-copy">
    <span class="settings-native-row-label">{label}</span>
    {#if hint}
      <span class="settings-native-row-hint">{hint}</span>
    {/if}
  </span>
  {#if value}
    <span
      class="settings-native-row-value {valueAccent ? 'settings-native-row-value-accent' : ''}"
    >
      {value}
    </span>
  {/if}
  <ChevronRight
    size={16}
    class="settings-native-row-chevron {expanded ? 'settings-native-row-chevron-open' : ''}"
    aria-hidden="true"
  />
</button>
