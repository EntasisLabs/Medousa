<script lang="ts">
  import { X } from "@lucide/svelte";
  import type { AddressedConversation } from "$lib/spotlight/addressedConversation.svelte";
  let { conversation }: { conversation: AddressedConversation } = $props();
  let previews = $state<{ file: File; url: string }[]>([]);
  $effect(() => {
    const next = conversation.files.map(file => ({ file, url: URL.createObjectURL(file) }));
    previews = next;
    return () => next.forEach(item => URL.revokeObjectURL(item.url));
  });
</script>
{#if previews.length}
  <div class="spotlight-image-draft" aria-label="Attached screenshots">
    {#each previews as item, index (item.url)}
      <figure><img src={item.url} alt={item.file.name}/><button type="button" aria-label={`Remove ${item.file.name}`} disabled={conversation.sending} onclick={() => conversation.removeFile(index)}><X size={12}/></button><figcaption>{item.file.name}</figcaption></figure>
    {/each}
    {#if conversation.uploading}<span role="status">Uploading…</span>{/if}
  </div>
{/if}
<style>
  .spotlight-image-draft { display: flex; gap: 8px; align-items: center; padding: 8px 18px; overflow-x: auto; }
  figure { position: relative; margin: 0; width: 96px; flex-shrink: 0; }
  img { width: 96px; height: 64px; object-fit: contain; background: rgb(var(--theme-text) / .035); border: 1px solid rgb(var(--theme-border) / .4); border-radius: 7px; }
  figcaption { font-size: 10px; color: rgb(var(--theme-text) / .55); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button { position: absolute; right: 3px; top: 3px; padding: 3px; border-radius: 50%; background: rgb(var(--theme-pane) / .9); color: rgb(var(--theme-text)); }
  span { font-size: 11px; color: rgb(var(--theme-text) / .55); }
</style>
