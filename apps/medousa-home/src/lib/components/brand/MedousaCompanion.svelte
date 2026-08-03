<script lang="ts">
  import MedousaSprite from "$lib/components/brand/MedousaSprite.svelte";
  import type { MedousaMarkId } from "$lib/theme/medousaMarks";

  type Action = "float" | "hit" | "idle" | "jump" | "power-up";
  type Variant = "abyss" | "amber" | "aurora" | "bone" | "jade" | "nebula" | "ocean" | "violet" | "white";
  type CompanionState =
    | Action
    | "attention"
    | "error"
    | "launch"
    | "loading"
    | "recoil"
    | "squash"
    | "success"
    | "surge";

  interface Props {
    state?: CompanionState;
    variant?: Variant;
    markId?: MedousaMarkId;
    darkMode?: boolean;
    size?: string;
    fps?: number;
    paused?: boolean;
    loop?: boolean;
    label?: string | null;
    class?: string;
  }

  let {
    state = "float",
    variant = "aurora",
    markId,
    darkMode = true,
    size = "2rem",
    fps = 3,
    paused = false,
    loop = true,
    label = "Medousa companion",
    class: className = "",
  }: Props = $props();

  const stateAction: Record<CompanionState, Action> = {
    idle: "idle",
    float: "float",
    squash: "hit",
    launch: "jump",
    jump: "jump",
    recoil: "hit",
    hit: "hit",
    surge: "power-up",
    success: "power-up",
    "power-up": "power-up",
    attention: "jump",
    error: "hit",
    loading: "float",
  };

  const action = $derived(stateAction[state]);
</script>

<span
  class={`medousa-companion ${className}`}
  data-state={state}
  data-action={action}
  data-paused={paused ? "true" : undefined}
  role={label ? "img" : undefined}
  aria-label={label || undefined}
  aria-hidden={label ? undefined : "true"}
  style={`--medousa-companion-size: ${size}`}
>
  <MedousaSprite
    variant={variant}
    {markId}
    {darkMode}
    size={size}
    fps={fps}
    paused={paused}
    action={action}
    loop={loop}
    label={null}
    class="medousa-companion-mark"
  />
</span>

<style>
  .medousa-companion {
    display: inline-block;
    width: var(--medousa-companion-size);
    aspect-ratio: 256 / 480;
    min-width: 0;
    min-height: 0;
    overflow: visible;
    vertical-align: middle;
  }

  :global(.medousa-companion-mark) {
    overflow: visible;
  }

  [data-state="idle"] :global(.medousa-companion-mark),
  [data-state="float"] :global(.medousa-companion-mark),
  [data-state="loading"] :global(.medousa-companion-mark) {
    animation: medousa-companion-buoy 4.8s ease-in-out infinite;
  }

  [data-state="squash"] :global(.medousa-companion-mark) {
    animation: medousa-companion-compress 2.2s ease-in-out infinite;
  }

  [data-state="launch"] :global(.medousa-companion-mark),
  [data-state="jump"] :global(.medousa-companion-mark),
  [data-state="attention"] :global(.medousa-companion-mark) {
    animation: medousa-companion-lift 2.4s ease-in-out infinite;
  }

  [data-state="recoil"] :global(.medousa-companion-mark),
  [data-state="hit"] :global(.medousa-companion-mark),
  [data-state="error"] :global(.medousa-companion-mark) {
    animation: medousa-companion-recoil 1.6s ease-in-out infinite;
  }

  [data-state="surge"] :global(.medousa-companion-mark),
  [data-state="success"] :global(.medousa-companion-mark),
  [data-state="power-up"] :global(.medousa-companion-mark) {
    animation: medousa-companion-expand 2.1s ease-in-out infinite;
  }

  [data-paused="true"] :global(.medousa-companion-mark) {
    animation-play-state: paused;
  }

  @keyframes medousa-companion-buoy {
    0%,
    100% {
      transform: translateY(0) scaleY(1);
    }

    50% {
      transform: translateY(-0.16rem) scaleY(1.012);
    }
  }

  @keyframes medousa-companion-compress {
    0%,
    100% {
      transform: translateY(0) scale(1);
    }

    35% {
      transform: translateY(0.08rem) scale(1.045, 0.94);
    }

    70% {
      transform: translateY(-0.08rem) scale(0.98, 1.03);
    }
  }

  @keyframes medousa-companion-lift {
    0%,
    100% {
      transform: translateY(0) scaleY(1);
    }

    45% {
      transform: translateY(-0.3rem) scaleY(0.98);
    }

    70% {
      transform: translateY(0.04rem) scaleY(1.02);
    }
  }

  @keyframes medousa-companion-recoil {
    0%,
    100% {
      transform: translateX(0) scale(1);
    }

    25% {
      transform: translateX(-0.1rem) scale(0.97, 1.02);
    }

    75% {
      transform: translateX(0.1rem) scale(1.02, 0.98);
    }
  }

  @keyframes medousa-companion-expand {
    0%,
    100% {
      transform: translateY(0) scale(1);
    }

    35% {
      transform: translateY(-0.32rem) scale(1.04, 1.02);
    }

    65% {
      transform: translateY(0.04rem) scale(0.98, 1.04);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.medousa-companion-mark) {
      animation: none;
    }
  }
</style>
