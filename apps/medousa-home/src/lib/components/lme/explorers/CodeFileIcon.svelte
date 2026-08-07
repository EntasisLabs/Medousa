<script lang="ts">
  import { codeFileIconForPath, codeFileIconSrc } from "$lib/code/codeFileIcons";

  interface Props {
    path: string;
    size?: number;
  }

  let { path, size = 14 }: Props = $props();

  const icon = $derived(codeFileIconForPath(path));
  let failed = $state(false);

  $effect(() => {
    void icon.id;
    failed = false;
  });
</script>

<span
  class="code-file-icon"
  style:width="{size}px"
  style:height="{size}px"
  title={icon.label}
  aria-hidden="true"
>
  {#if failed}
    <img
      class="code-file-icon__img"
      src={codeFileIconSrc("file")}
      width={size}
      height={size}
      alt=""
      draggable="false"
    />
  {:else}
    <img
      class="code-file-icon__img"
      src={codeFileIconSrc(icon.id)}
      width={size}
      height={size}
      alt=""
      draggable="false"
      onerror={() => {
        failed = true;
      }}
    />
  {/if}
</span>

<style>
  .code-file-icon {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    line-height: 0;
  }

  .code-file-icon__img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
    /* Keep SVG edges on whole pixels in WKWebView. */
    transform: translateZ(0);
  }
</style>
