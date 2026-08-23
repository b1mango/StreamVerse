<script lang="ts">
  import { onMount } from "svelte";

  let { paused = false }: { paused?: boolean } = $props();
  let canvas: HTMLCanvasElement;

  onMount(() => {
    const context = canvas.getContext("2d");
    if (!context) return;
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
    let frame = 0;
    let width = 0;
    let height = 0;
    let time = 0;
    let lastDraw = 0;

    const resize = () => {
      const ratio = Math.min(window.devicePixelRatio || 1, 2);
      width = canvas.clientWidth;
      height = canvas.clientHeight;
      canvas.width = Math.max(1, Math.floor(width * ratio));
      canvas.height = Math.max(1, Math.floor(height * ratio));
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
    };

    const draw = () => {
      context.clearRect(0, 0, width, height);
      const colors = ["rgba(62, 213, 177, .12)", "rgba(255, 87, 122, .1)", "rgba(87, 157, 255, .1)"];
      for (let lane = 0; lane < 9; lane += 1) {
        context.beginPath();
        context.strokeStyle = colors[lane % colors.length];
        context.lineWidth = lane % 3 === 0 ? 1.4 : 0.8;
        for (let x = -40; x <= width + 40; x += 24) {
          const y = height * (0.12 + lane * 0.095) + Math.sin(x * 0.006 + lane * 0.8 + time) * (14 + lane * 2);
          if (x === -40) context.moveTo(x, y);
          else context.lineTo(x, y);
        }
        context.stroke();
      }
    };

    const tick = (timestamp: number) => {
      if (!paused && !document.hidden && !reduceMotion.matches && timestamp - lastDraw >= 42) {
        time += 0.012;
        draw();
        lastDraw = timestamp;
      }
      frame = requestAnimationFrame(tick);
    };

    resize();
    draw();
    window.addEventListener("resize", resize);
    frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", resize);
    };
  });
</script>

<canvas bind:this={canvas} class="signal-field" aria-hidden="true"></canvas>
