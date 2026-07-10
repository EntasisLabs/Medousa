<script lang="ts">
  /**
   * Renders one chat message through the Liquid scene renderer.
   * Runtime turn template is the governor; daemon `ui_scene` roots still win when present.
   */
  import "$lib/liquid/archetypes";
  import { SceneRenderer } from "$lib/liquid/render";
  import type { LiquidRenderContext } from "$lib/liquid/render";
  import { resolveChatScene } from "$lib/liquid/surfaces/chat/mergeBuilderBody";
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
   * Builder body scenes merge into the runtime ceremony template.
   * Legacy freeform daemon roots still take over the whole turn.
   */
  const daemonScene = $derived(chatScenes.get(message.id));
  const scene = $derived(
    resolveChatScene(message, { statusLine, statusWarn }, daemonScene?.root ?? null),
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
