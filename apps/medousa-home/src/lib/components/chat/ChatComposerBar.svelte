<script lang="ts">
  import { LoaderCircle, Plus } from "@lucide/svelte";
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import ChatAttachmentChips from "$lib/components/chat/ChatAttachmentChips.svelte";
  import ChatModelPicker from "$lib/components/chat/ChatModelPicker.svelte";
  import ChatVoiceButton from "$lib/components/chat/ChatVoiceButton.svelte";
  import { chat } from "$lib/stores/chat.svelte";

  interface Props {
    mobile?: boolean;
    disabled?: boolean;
    composerBlocked?: boolean;
    onkeydown?: (event: KeyboardEvent) => void;
    onfocus?: () => void;
    onblur?: () => void;
    onOpenVoiceSettings?: () => void;
  }

  let {
    mobile = false,
    disabled = false,
    composerBlocked = false,
    onkeydown,
    onfocus,
    onblur,
    onOpenVoiceSettings,
  }: Props = $props();

  let voiceStatus = $state<string | null>(null);

  const blocked = $derived(disabled || composerBlocked);
  const canSend = $derived(
    !blocked && (chat.draft.trim().length > 0 || chat.pendingMediaRefs.length > 0),
  );
</script>

<ChatAttachmentChips {disabled} />

{#if voiceStatus}
  <p class="composer-voice-status" role="status">{voiceStatus}</p>
{/if}

<div
  class="composer-bar chat-composer-shell {mobile ? 'composer-bar-mobile' : 'chat-composer-bar'}"
>
  <button
    type="button"
    class="composer-bar-icon-btn"
    aria-label="Attach file"
    disabled={blocked || chat.pendingMediaUploading}
    onclick={() => void chat.attachFilesFromPicker()}
  >
    {#if chat.pendingMediaUploading}
      <LoaderCircle size={16} class="animate-spin" />
    {:else}
      <Plus size={18} strokeWidth={2} />
    {/if}
  </button>

  <ChatModelPicker {disabled} readonly={mobile} {onOpenVoiceSettings} />

  <GrowingTextarea
    bind:value={chat.draft}
    placeholder={mobile ? "Message" : "Message Medousa…"}
    disabled={blocked}
    maxHeight={mobile ? 144 : 128}
    minHeight={mobile ? 34 : 36}
    {onkeydown}
    {onfocus}
    {onblur}
    aria-label="Message"
  />

  <ChatVoiceButton {mobile} disabled={blocked} onStatus={(message) => (voiceStatus = message)} />

  <button
    type="submit"
    class="composer-bar-send"
    disabled={!canSend}
    aria-label="Send message"
    onmousedown={(event) => event.preventDefault()}
  >
    {composerBlocked ? "…" : "↑"}
  </button>
</div>
