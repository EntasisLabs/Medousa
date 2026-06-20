<script lang="ts">
  import { MessageCircle } from "@lucide/svelte";
  import VaultNoteChatSessionMenu from "$lib/components/vault/VaultNoteChatSessionMenu.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { launchVaultNoteWorkshop } from "$lib/utils/vaultNoteWorkshop";

  let menuOpen = $state(false);

  async function handleSelect(session: "fresh" | string) {
    if (!vault.selectedPath) return;
    menuOpen = false;
    await launchVaultNoteWorkshop({
      path: vault.selectedPath,
      title: vault.title,
      content: vault.content,
      wikilinksOut: vault.wikilinksOut,
      backlinks: vault.backlinks,
      session,
      flushSave: vault.dirty ? async () => { await vault.flushSave(); } : undefined,
    });
  }
</script>

<div class="vault-note-chat-fab-root">
  <VaultNoteChatSessionMenu
    open={menuOpen}
    onClose={() => (menuOpen = false)}
    onSelect={handleSelect}
    class="vault-note-chat-session-menu-fab"
  />

  <button
    type="button"
    class="vault-note-chat-fab"
    aria-label="Talk about this note"
    aria-haspopup="menu"
    aria-expanded={menuOpen}
    title="Talk about this note"
    disabled={vault.noteLoading}
    onclick={() => (menuOpen = !menuOpen)}
  >
    <MessageCircle size={22} strokeWidth={1.75} />
  </button>
</div>
