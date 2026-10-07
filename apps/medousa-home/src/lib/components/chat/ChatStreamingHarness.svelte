<script lang="ts">
  import ChatScrollChrome from "./ChatScrollChrome.svelte";
  import LiquidChatMessage from "./LiquidChatMessage.svelte";
  import type { ChatMessage } from "$lib/types/chat";

  let { message, mobile = false }: { message: ChatMessage; mobile?: boolean } = $props();
  let following = $state(true);
</script>

<ChatScrollChrome
  {mobile}
  showFab={!following}
  showTurnRail={false}
  showCurrentTurnAnchor={false}
  chatTurnItems={[]}
  activeChatTurnId={null}
  chatScrolling={false}
  bodyClass="chat-body"
  scrollClass="chat-scroll space-y-4"
  onAtBottomChange={(value) => { following = value; }}
>
  <LiquidChatMessage {message} sessionId="stream-test" {mobile} />
</ChatScrollChrome>
