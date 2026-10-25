<script lang="ts">
  import { onMount, type Snippet } from "svelte";
  import ResourceReferencePicker from "$lib/components/chat/ResourceReferencePicker.svelte";
  import SpotlightAttachmentDraft from "./SpotlightAttachmentDraft.svelte";
  import { handleChatImagePaste } from "$lib/utils/chatImagePaste";
  import { readMediaImagePath } from "$lib/daemon";
  import { isTauriDesktop } from "$lib/platform";
  import { MAX_MEDIA_REFS_PER_TURN } from "$lib/utils/normieErrors";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  import { X } from "@lucide/svelte";
  import { bots } from "$lib/stores/bots.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import type { AddressedConversation } from "$lib/spotlight/addressedConversation.svelte";
  import { conversationTargets, mentionQuery, type ConversationTarget } from "$lib/spotlight/conversationTargets";
  import SpotlightTargetPicker from "./SpotlightTargetPicker.svelte";
  import { guardTextNavigation } from "$lib/utils/guardTextNavigation";
  import { textNavigationKey } from "$lib/utils/composerKeyboard";
  let { query = $bindable(""), inputEl = $bindable<HTMLInputElement | null>(null), conversation, placeholder, busy, enabled, onQueryChange, idPrefix = "spotlight", defaultTarget, trailing, inlinePicker = false, onPickingChange, resultsOpen = false, activeResultId, onActivate }: {
    query: string; inputEl: HTMLInputElement | null; conversation: AddressedConversation; placeholder: string; busy: boolean; enabled: boolean; onQueryChange: () => void;
    idPrefix?: string; defaultTarget?: () => ConversationTarget;
    trailing?: Snippet; inlinePicker?: boolean; onPickingChange?: (value: boolean) => void;
    resultsOpen?: boolean; activeResultId?: string;
    onActivate?: () => void;
  } = $props();
  let dropElement: HTMLDivElement;
  let index = $state(0);
  function paste(event: ClipboardEvent) {
    handleChatImagePaste(event, { blocked: !enabled || busy || conversation.sending, attach: files => conversation.addFiles(files) });
  }
  function drop(event: DragEvent) {
    if (!enabled || busy || !event.dataTransfer?.files.length) return;
    event.preventDefault(); event.stopPropagation(); conversation.addFiles(Array.from(event.dataTransfer.files));
  }
  async function bindProject(workId: string) {
    await conversation.bindProject(workId);
  }
  onMount(() => {
    if (!isTauriDesktop()) return;
    let disposed = false, release: (() => void) | undefined;
    void (async () => {
      const [{ getCurrentWebview }, { getCurrentWindow }] = await Promise.all([import("@tauri-apps/api/webview"), import("@tauri-apps/api/window")]);
      const scale = await getCurrentWindow().scaleFactor();
      if (disposed) return;
      release = await getCurrentWebview().onDragDropEvent(({ payload }) => {
        if (payload.type !== "drop" || !enabled || busy || conversation.sending) return;
        const rect = dropElement?.closest(".command-spotlight-panel")?.getBoundingClientRect() ?? dropElement?.getBoundingClientRect();
        const x = payload.position.x / scale, y = payload.position.y / scale;
        if (!rect || rect.width <= 0 || x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) return;
        if (payload.paths.length + conversation.files.length > MAX_MEDIA_REFS_PER_TURN) { conversation.error = `Attach up to ${MAX_MEDIA_REFS_PER_TURN} images.`; return; }
        const scope = activeWorkshopId();
        void (async () => {
          try {
            const files: File[] = [];
            for (const path of payload.paths) {
              const media = await readMediaImagePath(path);
              if (disposed || scope !== activeWorkshopId()) return;
              files.push(new File([media.bytes as BlobPart], media.filename, { type: media.mime }));
            }
            conversation.addFiles(files);
          } catch (cause) { if (!disposed && scope === activeWorkshopId()) conversation.error = String(cause); }
        })();
      });
      if (disposed) release();
    })().catch(cause => { if (!disposed) conversation.error = String(cause); });
    return () => { disposed = true; release?.(); };
  });
  let dismissed = $state(false);
  const mention = $derived(enabled && !conversation.target ? mentionQuery(query) : null);
  const picking = $derived(Boolean(mention) && !dismissed);
  $effect(() => { onPickingChange?.(picking); });
  const targets = $derived(conversationTargets(chat.workshopScopeId ?? "", bots.bots, chat.sessions, conversation.runtimes, mention?.name ?? ""));
  function focus() { requestAnimationFrame(() => inputEl?.focus()); }
  function select(target: ConversationTarget) {
    const text = mention?.message ?? "";
    conversation.select(target); query = text; index = 0; dismissed = false; focus();
  }
  function remove() { conversation.clear(); query = ""; dismissed = false; focus(); }
  export async function send() {
    if (!enabled || busy || (!query.trim() && !conversation.files.length) || conversation.sending || mention) return;
    if (!conversation.target && defaultTarget) conversation.select(defaultTarget());
    if (!conversation.target && conversation.files.length) conversation.select({ id: "new:medousa", kind: "new", runtime: "medousa", label: "Medousa", description: "", group: "Start new", scope: chat.workshopScopeId ?? "" });
    const text = query; const target = conversation.target;
    try { await conversation.send(text, () => { if (conversation.target === target && query === text) query = ""; }); }
    catch { /* Inline error preserves the draft. */ }
    focus();
  }
  export function handleKeydown(event: KeyboardEvent): boolean {
    const key = textNavigationKey(event);
    if (event.isComposing || !enabled || (event.key === "Tab" && event.shiftKey)) return false;
    if (picking && event.key === "Escape") { event.preventDefault(); dismissed = true; return true; }
    if (event.target !== inputEl || (!mention && !conversation.target && !defaultTarget && !conversation.files.length)) return false;
    if (picking && ["ArrowDown", "ArrowUp", "Enter", "Tab"].includes(key)) {
      event.preventDefault();
      if (key === "Enter" || key === "Tab") { if (targets[index]) select(targets[index]); }
      else if (targets.length) index = (index + (key === "ArrowDown" ? 1 : -1) + targets.length) % targets.length;
      return true;
    }
    if (conversation.target && event.key === "Backspace" && !query) { event.preventDefault(); remove(); return true; }
    if (event.key === "Enter") { event.preventDefault(); void send(); return true; }
    return event.key !== "Escape";
  }
</script>

<div bind:this={dropElement} role="group" aria-label="Spotlight message; paste or drop screenshots" ondrop={drop} ondragover={event => { if (enabled && event.dataTransfer?.types.includes("Files")) event.preventDefault(); }}>
<div class="command-spotlight-search-row">
  {#if conversation.target}<button type="button" class="command-spotlight-recipient" aria-label={`Remove destination ${conversation.target.label}`} onclick={remove}><span>@{conversation.target.label}</span><X size={12}/></button>{/if}
  <input use:guardTextNavigation bind:this={inputEl} class="command-spotlight-input"
    placeholder={conversation.target ? `Message ${conversation.target.label}…` : placeholder}
    aria-label={conversation.target ? `Message ${conversation.target.label}` : "Search or address a conversation"}
    role="combobox" aria-expanded={picking || resultsOpen} aria-autocomplete="list"
    aria-controls={picking ? `${idPrefix}-targets` : resultsOpen ? `${idPrefix}-results` : undefined}
    aria-activedescendant={picking && targets[index] ? `${idPrefix}-target-${index}` : activeResultId}
    onpaste={paste} bind:value={query} onpointerdown={() => onActivate?.()} oninput={() => { index = 0; dismissed = false; onQueryChange(); }} disabled={busy}/>
  {@render trailing?.()}
  {#if picking && !inlinePicker}<SpotlightTargetPicker {targets} {index} {idPrefix} loading={bots.loading} error={bots.error} onSelect={select} onHighlight={value => index = value}/>{/if}
</div>
<SpotlightAttachmentDraft {conversation}/>
{#if conversation.error && !conversation.target}<p class="command-spotlight-conversation-notice" role="alert">{conversation.error}</p>{/if}
<ResourceReferencePicker bind:value={query} bind:references={conversation.references} element={inputEl} enabled={enabled && !mention && !conversation.sending && Boolean(conversation.target || defaultTarget)} onChange={onQueryChange} onBindProject={bindProject}/>
</div>
{#if picking && inlinePicker}<SpotlightTargetPicker {targets} {index} {idPrefix} inline loading={bots.loading} error={bots.error} onSelect={select} onHighlight={value => index = value}/>{/if}
