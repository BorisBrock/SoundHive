<script lang="ts">
  import { commands } from "$lib/bindings";
  import { attempt, library } from "$lib/app.svelte";
  import { player } from "$lib/player.svelte";
  import { projects } from "$lib/project.svelte";
  import { formatClock } from "$lib/sounds";
  import Icon from "./Icon.svelte";

  const scan = $derived(library.scan);
  const pct = $derived(scan && scan.total ? Math.round((scan.done / scan.total) * 100) : 0);
  let lastVolume = 0.8;
  /** Project sounds play as negative ids. */
  const playingName = $derived(
    player.current?.name ??
      (player.currentId !== null && player.currentId < 0
        ? projects.slots.find((s) => s.id === -player.currentId!)?.name
        : undefined),
  );
</script>

<footer class="flex h-8 shrink-0 items-center gap-4 border-t border-line bg-surface px-3 text-xs text-muted">
  {#if playingName}
    <span class="flex min-w-0 items-center gap-1.5" class:text-accent={player.playing}>
      <Icon name={player.playing ? "audio-lines" : "stop"} size={13} />
      <span class="max-w-64 truncate">{playingName}</span>
      {#if player.playing}<span class="font-mono tabular-nums">{formatClock(player.position)}</span>{/if}
    </span>
  {/if}

  {#if library.scanning}
    <span class="flex items-center gap-2">
      <Icon name="refresh" size={13} />
      {#if scan?.phase === "analyzing"}
        Analysing {scan.done.toLocaleString()} / {scan.total.toLocaleString()}
        <span class="h-1.5 w-24 overflow-hidden rounded-full bg-raised">
          <span class="block h-full rounded-full bg-accent transition-[width]" style:width="{pct}%"></span>
        </span>
        <span class="max-w-56 truncate" title={scan.current}>{scan.current}</span>
      {:else}
        Looking for sounds{scan?.current ? ` in ${scan.current}` : ""}…
      {/if}
      <button
        class="rounded px-1.5 py-0.5 hover:bg-raised hover:text-fg"
        onclick={() => attempt(() => commands.cancelScan())}
      >
        Cancel
      </button>
    </span>
  {:else if scan?.summary}
    <span title="Last scan">{scan.phase === "cancelled" ? "Scan cancelled: " : "Scan: "}{scan.summary}</span>
  {/if}

  <span class="ml-auto flex items-center gap-1.5" title="Volume">
    <button
      class="hover:text-fg"
      aria-label="Mute"
      onclick={() => {
        if (library.settings.volume > 0) {
          lastVolume = library.settings.volume;
          player.setVolume(0);
        } else player.setVolume(lastVolume || 0.8);
      }}
    >
      <Icon name={library.settings.volume > 0 ? "volume" : "volume-x"} size={14} />
    </button>
    <input
      type="range"
      min="0"
      max="1"
      step="0.01"
      class="w-24 accent-(--wz-accent)"
      value={library.settings.volume}
      oninput={(e) => player.setVolume(Number((e.currentTarget as HTMLInputElement).value))}
    />
  </span>
  <button
    class="flex items-center gap-1 rounded px-1.5 py-0.5 hover:bg-raised hover:text-fg"
    title="Keyboard shortcuts (?)"
    onclick={() => (library.helpOpen = true)}
  >
    <Icon name="circle-help" size={13} /> Help
  </button>
  <button
    class="flex items-center gap-1 rounded px-1.5 py-0.5 hover:bg-raised hover:text-fg"
    title="Settings ({navigator.platform.toLowerCase().includes('mac') ? '⌘' : 'Ctrl+'},)"
    onclick={() => (library.settingsOpen = true)}
  >
    <Icon name="settings" size={13} /> Settings
  </button>
</footer>
