<script lang="ts">
  import { botAvatar } from "$lib/utils/botAvatar";
  import { mascotImage } from "$lib/theme/medousaMarks";
  let { reference, size = 30 }: { reference?: string | null; size?: number } = $props();
  const avatar = $derived(botAvatar(reference));
</script>

<span class="bot-avatar" style:--avatar-color={avatar.color} style:width={`${size}px`} style:height={`${size}px`} aria-hidden="true">
  {#if avatar.legacy}<span class="legacy">{avatar.legacy}</span>
  {:else if avatar.mascot}<img class="mascot" class:starfish={avatar.mascot === "starfish"} src={mascotImage(avatar.mascot, avatar.expression ?? "default")} alt="" />
  {:else}<span class="mark"></span>{/if}
</span>

<style>
  .bot-avatar { display: inline-flex; flex: 0 0 auto; align-items: center; justify-content: center; border-radius: 28%; color: var(--avatar-color); background: color-mix(in srgb, var(--avatar-color) 12%, transparent); }
  .mark { width: 57%; height: 73%; background: currentColor; mask: url('/brand/medousa-mark-simplified.svg') center / contain no-repeat; -webkit-mask: url('/brand/medousa-mark-simplified.svg') center / contain no-repeat; }
  .legacy { font-size: 1em; line-height: 1; }
  .mascot { width: 92%; height: 92%; object-fit: contain; image-rendering: pixelated; }
  .mascot.starfish { transform: translateY(-5%) scale(1.1); }
</style>
