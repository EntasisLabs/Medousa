<script lang="ts">
  import MedousaSprite from "$lib/components/brand/MedousaSprite.svelte";
</script>

<main class="home-splash" aria-labelledby="home-splash-title" aria-live="polite">
  <div class="home-splash-field" aria-hidden="true">
    <span class="home-splash-orbit home-splash-orbit-one"></span>
    <span class="home-splash-orbit home-splash-orbit-two"></span>
    <span class="home-splash-spark home-splash-spark-one"></span>
    <span class="home-splash-spark home-splash-spark-two"></span>
    <span class="home-splash-spark home-splash-spark-three"></span>
  </div>

  <div class="home-splash-content">
    <MedousaSprite
      variant="aurora"
      size="clamp(8.5rem, 22vw, 14rem)"
      fps={7}
      action="float"
      label="Medousa is starting"
    />

    <div class="home-splash-status">
      <h1 id="home-splash-title">Opening your workshop</h1>
      <span class="home-splash-dots" aria-hidden="true">
        <i></i><i></i><i></i>
      </span>
    </div>
  </div>
</main>

<style>
  .home-splash {
    position: fixed;
    z-index: 9999;
    inset: 0;
    display: grid;
    min-height: 100dvh;
    overflow: hidden;
    place-items: center;
    padding: max(2rem, env(safe-area-inset-top)) max(1.5rem, env(safe-area-inset-right))
      max(2rem, env(safe-area-inset-bottom)) max(1.5rem, env(safe-area-inset-left));
    background: #000;
    color: #f2efe6;
    isolation: isolate;
  }

  .home-splash::before {
    position: absolute;
    inset: 15% 10%;
    z-index: -2;
    border: 1px solid rgb(242 239 230 / 0.035);
    border-radius: 50%;
    content: "";
    transform: rotate(-8deg) scale(1.12, 0.72);
  }

  .home-splash::after {
    position: absolute;
    inset: -20%;
    z-index: -3;
    background: radial-gradient(circle at center, rgb(168 85 247 / 0.12), transparent 34%);
    content: "";
    animation: home-splash-breathe 4.8s ease-in-out infinite;
  }

  .home-splash-field {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  .home-splash-orbit {
    position: absolute;
    top: 50%;
    left: 50%;
    width: min(70vw, 32rem);
    aspect-ratio: 1;
    border: 1px solid rgb(56 189 248 / 0.08);
    border-radius: 50%;
    transform: translate(-50%, -50%) rotate(18deg) scaleY(0.52);
  }

  .home-splash-orbit-two {
    width: min(48vw, 22rem);
    border-color: rgb(244 114 182 / 0.08);
    transform: translate(-50%, -50%) rotate(-24deg) scaleY(0.52);
  }

  .home-splash-spark {
    position: absolute;
    width: 0.22rem;
    height: 0.22rem;
    border-radius: 50%;
    background: #f2efe6;
    opacity: 0.46;
    animation: home-splash-spark 3.8s ease-in-out infinite;
  }

  .home-splash-spark-one {
    top: 29%;
    left: 28%;
  }

  .home-splash-spark-two {
    top: 34%;
    right: 27%;
    width: 0.14rem;
    height: 0.14rem;
    animation-delay: -1.2s;
  }

  .home-splash-spark-three {
    right: 34%;
    bottom: 27%;
    width: 0.16rem;
    height: 0.16rem;
    animation-delay: -2.4s;
  }

  .home-splash-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1.35rem;
    padding-bottom: 4vh;
    text-align: center;
  }

  .home-splash-status {
    display: flex;
    align-items: baseline;
    gap: 0.18rem;
    color: rgb(242 239 230 / 0.7);
  }

  .home-splash-status h1 {
    margin: 0;
    font-size: clamp(0.68rem, 1.2vw, 0.8rem);
    font-weight: 500;
    letter-spacing: 0.28em;
    line-height: 1.4;
    text-transform: uppercase;
  }

  .home-splash-dots {
    display: inline-flex;
    gap: 0.16rem;
    width: 1.15rem;
  }

  .home-splash-dots i {
    display: block;
    width: 0.18rem;
    height: 0.18rem;
    border-radius: 50%;
    background: currentColor;
    animation: home-splash-dot 1.2s ease-in-out infinite;
  }

  .home-splash-dots i:nth-child(2) {
    animation-delay: 0.16s;
  }

  .home-splash-dots i:nth-child(3) {
    animation-delay: 0.32s;
  }

  @keyframes home-splash-breathe {
    0%,
    100% {
      opacity: 0.55;
      transform: scale(0.96);
    }

    50% {
      opacity: 1;
      transform: scale(1.04);
    }
  }

  @keyframes home-splash-spark {
    0%,
    100% {
      opacity: 0.22;
      transform: scale(0.8);
    }

    50% {
      opacity: 0.8;
      transform: scale(1.4);
    }
  }

  @keyframes home-splash-dot {
    0%,
    60%,
    100% {
      opacity: 0.25;
      transform: translateY(0);
    }

    30% {
      opacity: 1;
      transform: translateY(-0.14rem);
    }
  }

  @media (max-width: 768px) {
    .home-splash {
      padding-right: max(1.25rem, env(safe-area-inset-right));
      padding-left: max(1.25rem, env(safe-area-inset-left));
    }

    .home-splash-content {
      gap: 1.1rem;
      padding-bottom: 7vh;
    }

    .home-splash-status h1 {
      font-size: 0.62rem;
      letter-spacing: 0.2em;
    }

    .home-splash-orbit {
      width: 24rem;
    }

    .home-splash-orbit-two {
      width: 16rem;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .home-splash::after,
    .home-splash-spark,
    .home-splash-dots i {
      animation: none;
    }
  }
</style>
