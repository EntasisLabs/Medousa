<script lang="ts">
  interface Props {
    value?: string;
    placeholder?: string;
    disabled?: boolean;
    class?: string;
    maxHeight?: number;
    minHeight?: number;
    element?: HTMLTextAreaElement | null;
    onkeydown?: (event: KeyboardEvent) => void;
    onblur?: (event: FocusEvent) => void;
    onfocus?: (event: FocusEvent) => void;
    oninput?: (event: Event) => void;
    onclick?: (event: MouseEvent) => void;
    onkeyup?: (event: KeyboardEvent) => void;
    onselect?: (event: Event) => void;
    "aria-label"?: string;
  }

  let {
    value = $bindable(""),
    placeholder = "",
    disabled = false,
    class: className = "",
    maxHeight = 128,
    minHeight = 36,
    element = $bindable<HTMLTextAreaElement | null>(null),
    onkeydown,
    onblur,
    onfocus,
    oninput,
    onclick,
    onkeyup,
    onselect,
    "aria-label": ariaLabel,
  }: Props = $props();

  function resize() {
    if (!element) return;
    element.style.height = "0px";
    const scroll = element.scrollHeight;
    const height = Math.min(Math.max(scroll, minHeight), maxHeight);
    element.style.height = `${height}px`;
    element.style.overflowY = scroll > maxHeight ? "auto" : "hidden";
    element.dataset.expanded = scroll > minHeight + 6 ? "true" : "false";
  }

  function handleInput(event: Event) {
    resize();
    oninput?.(event);
  }

  $effect(() => {
    value;
    queueMicrotask(resize);
  });
</script>

<textarea
  bind:this={element}
  bind:value
  {placeholder}
  {disabled}
  {onkeydown}
  {onblur}
  {onfocus}
  {onclick}
  {onkeyup}
  {onselect}
  aria-label={ariaLabel}
  rows="1"
  class="composer-bar-input {className}"
  oninput={handleInput}
></textarea>
