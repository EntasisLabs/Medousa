<script lang="ts">
  import {
    medousaMarkSpriteFill,
    type MedousaMarkId,
  } from "$lib/theme/medousaMarks";

  type Variant = "abyss" | "amber" | "aurora" | "bone" | "jade" | "nebula" | "ocean" | "violet" | "white";
  type Action = "float" | "hit" | "idle" | "jump" | "power-up";

  interface Props {
    variant?: Variant;
    markId?: MedousaMarkId;
    darkMode?: boolean;
    size?: string;
    fps?: number;
    paused?: boolean;
    simplified?: boolean;
    action?: Action;
    loop?: boolean;
    label?: string | null;
    class?: string;
  }

  let {
    variant = "aurora",
    markId,
    darkMode = true,
    size = "7rem",
    fps = 8,
    paused = false,
    simplified = false,
    action = "idle",
    loop = true,
    label = "Medousa",
    class: className = "",
  }: Props = $props();

  const frameCount = $derived(simplified ? 8 : 6);
  const duration = $derived(`${frameCount / Math.max(1, Math.min(30, fps))}s`);
  const mask = $derived(
    simplified ? "/brand/sprites/medousa-mark-sprite.svg" : "/brand/sprites/medousa-mark-action-sprite.svg",
  );
  const aspectRatio = $derived(simplified ? "128 / 160" : "256 / 480");
  const actionRow = $derived(
    simplified
      ? "0%"
      : {
          idle: "0%",
          jump: "25%",
          hit: "50%",
          "power-up": "75%",
          float: "100%",
        }[action],
  );
  const maskSize = $derived(simplified ? "800% 100%" : "600% 500%");
  const variantFill: Record<Variant, string> = {
    abyss: "linear-gradient(135deg, #5eead4, #0f766e)",
    amber: "linear-gradient(135deg, #fcd34d, #b45309)",
    aurora: "linear-gradient(135deg, #f472b6 0%, #a855f7 38%, #38bdf8 72%, #34d399 100%)",
    bone: "#f2efe6",
    jade: "linear-gradient(135deg, #34d399, #065f46)",
    nebula: "linear-gradient(135deg, #c084fc, #6d28d9)",
    ocean: "linear-gradient(135deg, #7dd3fc, #0369a1)",
    violet: "linear-gradient(135deg, #a78bfa, #4c1d95)",
    white: "#fff",
  };
  const fill = $derived(markId ? medousaMarkSpriteFill(markId, darkMode) : variantFill[variant]);
</script>

<span
  class={`medousa-sprite-root ${className}`}
  data-variant={variant}
  data-simplified={simplified ? "true" : undefined}
  data-action={action}
  role={label ? "img" : undefined}
  aria-label={label || undefined}
  aria-hidden={label ? undefined : "true"}
  style={`--medousa-sprite-size: ${size}; --medousa-sprite-duration: ${duration}; --medousa-sprite-play-state: ${paused ? "paused" : "running"}; --medousa-sprite-iteration: ${loop ? "infinite" : "1"}; --medousa-sprite-mask: url('${mask}'); --medousa-sprite-mask-size: ${maskSize}; --medousa-sprite-row: ${actionRow}; --medousa-sprite-aspect-ratio: ${aspectRatio}; --medousa-sprite-fill: ${fill}`}
>
  <span class="medousa-sprite-frame" aria-hidden="true"></span>
</span>

<style>
  .medousa-sprite-root {
    display: inline-block;
    width: var(--medousa-sprite-size);
    aspect-ratio: var(--medousa-sprite-aspect-ratio);
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    vertical-align: middle;
  }

  .medousa-sprite-root:not([data-simplified="true"]) {
    overflow: visible;
  }

  .medousa-sprite-frame {
    display: block;
    width: 100%;
    height: 100%;
    background: var(--medousa-sprite-fill);
    -webkit-mask-image: var(--medousa-sprite-mask);
    mask-image: var(--medousa-sprite-mask);
    -webkit-mask-position: 0 var(--medousa-sprite-row);
    mask-position: 0 var(--medousa-sprite-row);
    -webkit-mask-repeat: no-repeat;
    mask-repeat: no-repeat;
    -webkit-mask-size: var(--medousa-sprite-mask-size);
    mask-size: var(--medousa-sprite-mask-size);
    animation: medousa-mark-action var(--medousa-sprite-duration) steps(5, end)
      var(--medousa-sprite-iteration);
    animation-fill-mode: forwards;
    animation-play-state: var(--medousa-sprite-play-state);
  }

  .medousa-sprite-root:not([data-simplified="true"]) .medousa-sprite-frame {
    width: 137.5%;
    height: 116.6667%;
    margin-top: -8.3333%;
    margin-left: -18.75%;
  }

  [data-simplified="true"] .medousa-sprite-frame {
    animation-name: medousa-mark-simplified;
    animation-timing-function: steps(7, end);
  }

  @keyframes medousa-mark-action {
    from {
      -webkit-mask-position: 0 var(--medousa-sprite-row);
      mask-position: 0 var(--medousa-sprite-row);
    }

    to {
      -webkit-mask-position: 100% var(--medousa-sprite-row);
      mask-position: 100% var(--medousa-sprite-row);
    }
  }

  @keyframes medousa-mark-simplified {
    from {
      -webkit-mask-position: 0 0;
      mask-position: 0 0;
    }

    to {
      -webkit-mask-position: 100% 0;
      mask-position: 100% 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .medousa-sprite-frame {
      animation: none;
    }
  }
</style>
