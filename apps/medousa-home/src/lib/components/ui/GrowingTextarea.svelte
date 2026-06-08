<script lang="ts">
  interface Props {
    value?: string;
    placeholder?: string;
    disabled?: boolean;
    class?: string;
    maxHeight?: number;
    minHeight?: number;
    onkeydown?: (event: KeyboardEvent) => void;
    onblur?: (event: FocusEvent) => void;
    onfocus?: (event: FocusEvent) => void;
    "aria-label"?: string;
  }

  let {
    value = $bindable(""),
    placeholder = "",
    disabled = false,
    class: className = "",
    maxHeight = 128,
    minHeight = 36,
    onkeydown,
    onblur,
    onfocus,
    "aria-label": ariaLabel,
  }: Props = $props();

  let el: HTMLTextAreaElement | undefined = $state();

  function resize() {
    if (!el) return;
    el.style.height = "0px";
    const scroll = el.scrollHeight;
    const height = Math.min(Math.max(scroll, minHeight), maxHeight);
    el.style.height = `${height}px`;
    el.style.overflowY = scroll > maxHeight ? "auto" : "hidden";
    el.dataset.expanded = scroll > minHeight + 6 ? "true" : "false";
  }

  $effect(() => {
    value;
    queueMicrotask(resize);
  });
</script>

<textarea
  bind:this={el}
  bind:value
  {placeholder}
  {disabled}
  {onkeydown}
  {onblur}
  {onfocus}
  aria-label={ariaLabel}
  rows="1"
  class="composer-bar-input {className}"
  oninput={resize}
></textarea>
