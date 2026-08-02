<script lang="ts">
  import {
    medousaMarkOption,
    type MedousaMarkId,
  } from "$lib/theme/medousaMarks";

  interface Props {
    markId: MedousaMarkId;
    darkMode?: boolean;
    simplified?: boolean;
    decorative?: boolean;
    label?: string;
  }

  let {
    markId,
    darkMode = true,
    simplified = false,
    decorative = false,
    label = "Medousa",
  }: Props = $props();

  const option = $derived(medousaMarkOption(markId));
  const asset = $derived(
    markId === "aurora"
      ? simplified
        ? "/brand/medousa-mark-simplified-aurora.svg"
        : "/brand/medousa-mark-aurora.svg"
      : simplified
        ? "/brand/medousa-mark-simplified.svg"
        : "/brand/medousa-mark.svg",
  );
  const color = $derived(darkMode ? option.darkColor : option.lightColor);
</script>

<span
  class="medousa-mark-root"
  role={decorative ? undefined : "img"}
  aria-label={decorative ? undefined : label}
  aria-hidden={decorative}
>
  {#if markId === "aurora"}
    <img
      class="medousa-mark-image"
      style="width: 100%; height: 100%; max-width: 100%; max-height: 100%; object-fit: contain"
      src={asset}
      alt=""
      aria-hidden="true"
    />
  {:else}
    <span
      class="medousa-mark-mask"
      style={`--medousa-mark-mask: url(${asset}); --medousa-mark-color: ${color}`}
      aria-hidden="true"
    ></span>
  {/if}
</span>

<style>
  .medousa-mark-root,
  .medousa-mark-image,
  .medousa-mark-mask {
    display: block;
    width: 100%;
    height: 100%;
  }

  .medousa-mark-root {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .medousa-mark-image {
    object-fit: contain;
  }

  .medousa-mark-mask {
    background: var(--medousa-mark-color);
    -webkit-mask-image: var(--medousa-mark-mask);
    mask-image: var(--medousa-mark-mask);
    -webkit-mask-position: center;
    mask-position: center;
    -webkit-mask-repeat: no-repeat;
    mask-repeat: no-repeat;
    -webkit-mask-size: contain;
    mask-size: contain;
  }
</style>
