<!-- Backward-compatible alias — prefer VaultExportPreviewModal. -->
<script lang="ts">
  import VaultExportPreviewModal from "./VaultExportPreviewModal.svelte";

  interface Props {
    open: boolean;
    title: string;
    content: string;
    labelByPath: Map<string, string>;
    notePath?: string | null;
    onClose: () => void;
    onPreparingChange?: (preparing: boolean) => void;
  }

  let {
    open,
    title,
    content,
    labelByPath,
    notePath = null,
    onClose,
    onPreparingChange,
  }: Props = $props();
</script>

<VaultExportPreviewModal
  {open}
  {title}
  {content}
  {labelByPath}
  {notePath}
  initialFormat="pdf"
  {onClose}
  {onPreparingChange}
/>
