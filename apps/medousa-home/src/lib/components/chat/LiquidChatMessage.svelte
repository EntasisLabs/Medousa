<script lang="ts">
  /**
   * Renders one chat message through the Liquid scene renderer (PR3, no daemon).
   * The message is adapted to a `document` scene; interaction flows back through
   * an event sink. Gated by the `liquidChat` flag in `ChatMessageList`.
   */
  import "$lib/liquid/archetypes";
  import { SceneRenderer } from "$lib/liquid/render";
  import type { LiquidRenderContext } from "$lib/liquid/render";
  import { chatMessageToScene } from "$lib/liquid/surfaces/chat/messageToScene";
  import { chatScenes } from "$lib/liquid/surfaces/chat/chatScenes.svelte";
  import { createChatEventSink } from "$lib/liquid/surfaces/chat/chatEventSink";
  import { chatInteractions } from "$lib/liquid/surfaces/chat/chatInteractions";
  import { settings } from "$lib/stores/settings.svelte";
  import { visibleChatStatusLine } from "$lib/utils/chatStreamDisplay";
  import type { ChatMessage } from "$lib/types/chat";
  import type { ToolHistorySliceRef } from "$lib/types/toolHistory";

  interface Props {
    message: ChatMessage;
    sessionId: string;
    mobile?: boolean;
    compact?: boolean;
    onPromoteToFlow?: (ref: ToolHistorySliceRef) => void | Promise<void>;
    onRetryWorker?: (workId: string) => void;
    /** Spawn a new interactive turn (action_row / button submit). */
    onSubmitIntent?: (text: string) => void;
  }

  let {
    message,
    sessionId,
    mobile = false,
    compact = false,
    onPromoteToFlow,
    onRetryWorker,
    onSubmitIntent,
  }: Props = $props();

  const statusLine = $derived(
    visibleChatStatusLine(message.statusLine, settings.showEngineDetailsInChat),
  );
  const statusWarn = $derived(
    message.phase === "worker_ack" || message.phase === "awaiting_operator",
  );

  /**
   * A daemon-authored scene (streamed `ui_scene` ops) wins over the client-side
   * adapter — this is how a model-authored structured turn takes over rendering.
   * Turns without a scene fall back to the deterministic `messageToScene` adapter.
   */
  const daemonScene = $derived(chatScenes.get(message.id));
  const scene = $derived(
    daemonScene?.root ?? chatMessageToScene(message, { statusLine, statusWarn }),
  );

  const sink = $derived(
    createChatEventSink({
      sessionId,
      messageId: message.id,
      onSubmitIntent,
      onRetryWorker,
      record: chatInteractions.record.bind(chatInteractions),
    }),
  );

  const context = $derived<LiquidRenderContext>({
    sink,
    sessionId,
    mobile,
    compact,
    openLinksInWeb: true,
    onPromoteToFlow,
  });
</script>

<SceneRenderer node={scene} {context} />
