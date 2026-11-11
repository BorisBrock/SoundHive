<script lang="ts">
  import { commands } from "$lib/bindings";
  import { attempt, library, type Sound } from "$lib/app.svelte";
  import { soundMenu } from "$lib/actions";
  import { contextMenu } from "$lib/contextMenu.svelte";
  import { player } from "$lib/player.svelte";
  import { formatDuration, LOOP_LABELS, normExt, type SortKey } from "$lib/sounds";
  import Icon from "./Icon.svelte";
  import Waveform from "./Waveform.svelte";

  const ROW = 38;
  const OVERSCAN = 8;

  let viewport: HTMLDivElement;
  let scrollTop = $state(0);
  let height = $state(0);

  const list = $derived(library.visible);
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const last = $derived(Math.min(list.length, Math.ceil((scrollTop + height) / ROW) + OVERSCAN));
  const rows = $derived(list.slice(first, last).map((s, i) => ({ s, i: first + i })));
  const selected = $derived(library.selected);

  // Keep the focused row on screen when it moves by keyboard.
  $effect(() => {
    const id = library.focusedId;
    if (id === null || !viewport) return;
    const i = list.findIndex((s) => s.id === id);
    if (i < 0) return;
    const top = i * ROW;
    if (top < viewport.scrollTop) viewport.scrollTop = top;
    else if (top + ROW > viewport.scrollTop + viewport.clientHeight)
      viewport.scrollTop = top + ROW - viewport.clientHeight;
  });

  // Back to the top when the result set changes.
  let lastQuery = "";
  $effect(() => {
    const key = JSON.stringify([library.search, library.filters, library.similarTo, library.sort]);
    if (key !== lastQuery && viewport) {
      lastQuery = key;
      viewport.scrollTop = 0;
    }
  });

  function onRowClick(e: MouseEvent, s: Sound) {
    const mode = e.shiftKey ? "range" : e.ctrlKey || e.metaKey ? "toggle" : "replace";
    library.select(s.id, mode);
    if (mode === "replace" && library.settings.autoplay) player.play(s);
  }

  // Dragging rows out of the window hands the files to other apps.
  let press: { x: number; y: number; s: Sound } | null = null;
  function onPointerDown(e: PointerEvent, s: Sound) {
    if (e.button !== 0) return;
    press = { x: e.clientX, y: e.clientY, s };
  }
  function onPointerMove(e: PointerEvent) {
    if (!press || !(e.buttons & 1)) {
      press = null;
      return;
    }
    if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < 6) return;
    const s = press.s;
    press = null;
    const ids = library.selected.has(s.id) ? library.selectionIds : [s.id];
    library.draggingOut = ids;
    attempt(() => commands.startDrag(ids));
  }

  const isSelected = (s: Sound) => selected.has(s.id) || (selected.size === 0 && library.focusedId === s.id);

  const columns: { key: SortKey | null; label: string; class: string }[] = [
    { key: "name", label: "Name", class: "flex-1 min-w-0" },
    { key: null, label: "Waveform", class: "w-40 hidden lg:block" },
    { key: "duration", label: "Length", class: "w-16 text-right" },
    { key: null, label: "Format", class: "w-24 hidden md:block" },
    { key: null, label: "Tags", class: "w-56 hidden xl:block" },
  ];

  function sortBy(key: SortKey) {
    if (library.sort === key) library.sortDesc = !library.sortDesc;
    else {
      library.sort = key;
      library.sortDesc = false;
    }
    library.similarTo = null;
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div
    class="flex h-8 shrink-0 items-center gap-3 border-b border-line bg-surface pr-4 pl-3 text-[11px] font-semibold tracking-wider text-muted uppercase"
  >
    <span class="w-12"></span>
    {#each columns as col (col.label)}
      <div class={col.class}>
        {#if col.key}
          {@const key = col.key}
          <button class="flex items-center gap-1 uppercase hover:text-fg" onclick={() => sortBy(key)}>
            {col.label}
            {#if library.sort === key && library.similarTo === null}
              <Icon name={library.sortDesc ? "arrow-down" : "arrow-up"} size={11} />
            {/if}
          </button>
        {:else}
          {col.label}
        {/if}
      </div>
    {/each}
  </div>

  <div
    bind:this={viewport}
    bind:clientHeight={height}
    class="relative min-h-0 flex-1 overflow-y-auto outline-none"
    onscroll={() => (scrollTop = viewport.scrollTop)}
    onpointermove={onPointerMove}
    role="listbox"
    aria-label="Sounds"
    aria-multiselectable="true"
    tabindex="-1"
  >
    <div style:height="{list.length * ROW}px" class="relative">
      {#each rows as { s, i } (s.id)}
        {@const sel = isSelected(s)}
        {@const current = player.currentId === s.id}
        {@const playing = current && player.playing}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          role="option"
          aria-selected={sel}
          tabindex="-1"
          class="group absolute inset-x-0 flex items-center gap-3 border-b border-line/40 pr-4 pl-3"
          class:bg-accent-row={sel}
          class:hover:bg-raised={!sel}
          class:odd-row={i % 2 === 1 && !sel}
          style:top="{i * ROW}px"
          style:height="{ROW}px"
          onclick={(e) => onRowClick(e, s)}
          ondblclick={() => player.play(s)}
          onpointerdown={(e) => onPointerDown(e, s)}
          oncontextmenu={(e) => {
            if (!library.selected.has(s.id)) library.select(s.id);
            contextMenu.open(e, soundMenu(s));
          }}
        >
          <div class="flex w-12 shrink-0 items-center gap-0.5">
            <button
              class="grid size-6 place-items-center rounded-full"
              class:text-accent={current}
              class:text-muted={!current}
              class:hover:text-fg={!current}
              title={playing ? "Stop" : "Play"}
              onclick={(e) => {
                e.stopPropagation();
                library.select(s.id);
                player.toggle(s);
              }}
            >
              <Icon name={playing ? "stop" : "play"} size={14} filled />
            </button>
            <button
              class="grid size-6 place-items-center rounded"
              class:text-honey={s.favorite}
              class:text-muted={!s.favorite}
              class:opacity-0={!s.favorite}
              class:group-hover:opacity-60={!s.favorite}
              title={s.favorite ? "Remove from favourites" : "Add to favourites"}
              onclick={(e) => {
                e.stopPropagation();
                library.toggleFavorite([s.id]);
              }}
            >
              <Icon name="star" size={13} filled={s.favorite} />
            </button>
          </div>

          <div class="min-w-0 flex-1 leading-tight">
            <div class="flex items-center gap-1.5">
              <span class="truncate font-medium" class:text-accent={current} title={s.name}>{s.name}</span>
              {#if s.loopKind}
                <span class="shrink-0 text-accent" title="Loop: {LOOP_LABELS[s.loopKind] ?? s.loopKind}">
                  <Icon name="repeat" size={12} />
                </span>
              {/if}
              {#if s.error}
                <span class="shrink-0 text-warning" title="Could not be analysed: {s.error}">
                  <Icon name="triangle-alert" size={12} />
                </span>
              {/if}
            </div>
            {#if s.dir || s.description}
              <div class="truncate text-xs text-muted" title={s.description || s.dir}>
                {s.description || s.dir}
              </div>
            {/if}
          </div>

          <div class="hidden w-40 shrink-0 lg:block">
            <Waveform
              id={s.id}
              height={24}
              dim={!sel && !current}
              progress={current && player.playing && s.duration ? player.position / s.duration : null}
              duration={s.duration}
              onseek={(f) => {
                library.select(s.id);
                player.seek(s, f * s.duration);
              }}
            />
          </div>

          <div class="w-16 shrink-0 text-right font-mono text-xs tabular-nums">
            {formatDuration(s.duration)}
          </div>

          <div class="hidden w-24 shrink-0 text-xs text-muted md:block">
            <span class="font-medium text-fg/80 uppercase">{normExt(s.ext)}</span>
            {#if s.sampleRate}· {Math.round(s.sampleRate / 100) / 10}k{/if}
            {#if s.channels === 1}· M{:else if s.channels === 2}· S{/if}
          </div>

          <div class="hidden w-56 shrink-0 gap-1 overflow-hidden xl:flex">
            {#each s.tags.slice(0, 4) as tag (tag)}
              <button
                class="chip shrink-0 py-0! text-[11px]!"
                class:on={library.filters.tags.includes(tag)}
                title="Filter by “{tag}”"
                onclick={(e) => {
                  e.stopPropagation();
                  library.toggleTagFilter(tag);
                }}
              >
                {tag}
              </button>
            {/each}
            {#if s.tags.length > 4}<span class="text-xs text-muted">+{s.tags.length - 4}</span>{/if}
          </div>
        </div>
      {/each}
    </div>

    {#if library.loaded && !list.length && library.sounds.length}
      <div class="absolute inset-0 grid place-items-center text-muted">
        <div class="text-center">
          <p class="mb-3">No sounds match.</p>
          <button class="chip" onclick={() => library.resetView()}>Show all sounds</button>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .bg-accent-row {
    background: color-mix(in srgb, var(--wz-accent) 16%, transparent);
  }
  .odd-row {
    background: color-mix(in srgb, var(--wz-raised) 35%, transparent);
  }
</style>
