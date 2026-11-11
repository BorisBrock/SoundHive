<script lang="ts">
  import { library } from "$lib/app.svelte";
  import { waveforms } from "$lib/waveforms.svelte";

  /**
   * Draws a sound's peak envelope on a canvas, mirrored around the middle. With `onseek`, a
   * click picks a point in time (0..1) and hovering shows where.
   */
  let {
    id,
    peaks: given,
    progress = null,
    height = 28,
    dim = false,
    onseek,
    duration = 0,
  }: {
    id: number;
    /** The envelope to draw, instead of the library sound `id`'s. */
    peaks?: number[];
    /** Playhead position 0..1, or null when not playing. */
    progress?: number | null;
    height?: number;
    dim?: boolean;
    onseek?: (fraction: number) => void;
    duration?: number;
  } = $props();

  let canvas: HTMLCanvasElement;
  let width = $state(0);
  let hover = $state<number | null>(null);

  const peaks = $derived(given ? (given.length ? given : undefined) : waveforms.get(id));

  function color(name: string) {
    return getComputedStyle(canvas).getPropertyValue(name).trim() || "#7c9cff";
  }

  $effect(() => {
    const w = width;
    const p = peaks;
    const prog = progress;
    void library.settings.theme;
    if (!canvas || !w) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(height * dpr);
    const ctx = canvas.getContext("2d")!;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, w, height);
    const mid = height / 2;
    const accent = color("--wz-accent");
    const muted = color("--wz-muted");
    if (!p) {
      ctx.fillStyle = muted;
      ctx.globalAlpha = 0.25;
      ctx.fillRect(0, mid - 0.5, w, 1);
      return;
    }
    // Normalise so that quiet sounds are still visible.
    let max = 1;
    for (const v of p) max = Math.max(max, v);
    const bar = height > 40 ? 2 : 1.5;
    const gap = height > 40 ? 1 : 0.5;
    const n = Math.max(1, Math.floor(w / (bar + gap)));
    for (let i = 0; i < n; i++) {
      const from = Math.floor((i * p.length) / n);
      const to = Math.max(from + 1, Math.floor(((i + 1) * p.length) / n));
      let v = 0;
      for (let j = from; j < to; j++) v = Math.max(v, p[j]);
      const amp = Math.max(0.5, (v / max) * (mid - 1));
      const x = i * (bar + gap);
      const played = prog !== null && x / w <= prog;
      ctx.fillStyle = played ? accent : dim ? muted : accent;
      ctx.globalAlpha = played ? 1 : dim ? 0.45 : 0.55;
      ctx.fillRect(x, mid - amp, bar, amp * 2);
    }
    if (prog !== null) {
      ctx.globalAlpha = 1;
      ctx.fillStyle = color("--wz-fg");
      ctx.fillRect(Math.min(w - 1, prog * w), 0, 1, height);
    }
  });

  function fraction(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
  }

  function fmt(t: number) {
    return t < 60 ? `${t.toFixed(2)}s` : `${Math.floor(t / 60)}:${(t % 60).toFixed(1).padStart(4, "0")}`;
  }
</script>

<div
  class="relative w-full"
  class:cursor-pointer={!!onseek}
  style:height="{height}px"
  bind:clientWidth={width}
  role={onseek ? "slider" : undefined}
  aria-label={onseek ? "Playback position" : undefined}
  aria-valuenow={onseek ? Math.round((progress ?? 0) * 100) : undefined}
  onpointerdown={(e) => {
    if (!onseek || e.button !== 0) return;
    e.stopPropagation();
    onseek(fraction(e));
  }}
  onpointermove={(e) => (hover = onseek ? fraction(e) : null)}
  onpointerleave={() => (hover = null)}
>
  <canvas bind:this={canvas} class="absolute inset-0 h-full w-full"></canvas>
  {#if hover !== null && onseek}
    <div class="pointer-events-none absolute inset-y-0 w-px bg-fg/40" style:left="{hover * 100}%"></div>
    {#if duration}
      <div
        class="pointer-events-none absolute top-0 rounded bg-raised px-1 font-mono text-[10px] text-fg shadow"
        style:left="min(calc({hover * 100}% + 4px), calc(100% - 3.5rem))"
      >
        {fmt(hover * duration)}
      </div>
    {/if}
  {/if}
</div>
