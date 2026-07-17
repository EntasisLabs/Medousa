<script lang="ts">
  /**
   * Nested title + body editor for quiet liquid list rows.
   */
  interface Props {
    titleLabel?: string;
    bodyLabel?: string;
    titlePlaceholder?: string;
    bodyPlaceholder?: string;
    title: string;
    body: string;
    onTitleChange: (value: string) => void;
    onBodyChange: (value: string) => void;
  }

  let {
    titleLabel = "Title",
    bodyLabel = "Body",
    titlePlaceholder = "Title",
    bodyPlaceholder = "Body",
    title,
    body,
    onTitleChange,
    onBodyChange,
  }: Props = $props();
</script>

<div class="vault-liquid-quiet-item">
  <label class="vault-liquid-quiet-item__field">
    <span class="vault-liquid-quiet-item__label">{titleLabel}</span>
    <input
      class="vault-liquid-quiet-item__input"
      type="text"
      placeholder={titlePlaceholder}
      aria-label={titleLabel}
      value={title}
      oninput={(event) =>
        onTitleChange((event.currentTarget as HTMLInputElement).value)}
    />
  </label>
  <label class="vault-liquid-quiet-item__field">
    <span class="vault-liquid-quiet-item__label">{bodyLabel}</span>
    <textarea
      class="vault-liquid-quiet-item__area"
      rows="6"
      placeholder={bodyPlaceholder}
      aria-label={bodyLabel}
      value={body}
      oninput={(event) =>
        onBodyChange((event.currentTarget as HTMLTextAreaElement).value)}
    ></textarea>
  </label>
</div>
