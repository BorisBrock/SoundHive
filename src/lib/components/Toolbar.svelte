<script lang="ts">
  import { library } from "$lib/app.svelte";
  import { MOD } from "$lib/actions";
  import { SORTS } from "$lib/sounds";
  import Icon from "./Icon.svelte";

  const DURATIONS: { label: string; range: [number, number] }[] = [
    { label: "< 0.5s", range: [0, 0.5] },
    { label: "0.5–2s", range: [0.5, 2] },
    { label: "2–10s", range: [2, 10] },
    { label: "10–60s", range: [10, 60] },
    { label: "> 1 min", range: [60, Infinity] },
  ];

  const f = $derived(library.filters);
  const similar = $derived(library.similarTo !== null ? library.byId.get(library.similarTo) : undefined);
  const sameRange = (a: [number, number] | null, b: [number, number]) =>
    !!a && a[0] === b[0] && a[1] === b[1];

  function toggleExt(ext: string) {
    f.exts = f.exts.includes(ext) ? f.exts.filter((e) => e !== ext) : [...f.exts, ext];
  }

  const anyFilter = $derived(
    !!(f.duration || f.exts.length || f.channels || f.loop || f.tags.length || f.folder || library.search),
  );
</script>

<div class="shrink-0 space-y-2 border-b border-line bg-surface px-3 py-2.5">
  <div class="flex items-center gap-2">
    <label class="relative flex min-w-0 flex-1 items-center">
      <span class="pointer-events-none absolute left-2.5 text-muted"><Icon name="search" size={15} /></span>
      <input
        id="search"
        class="field w-full py-1.5! pr-24! pl-8! text-sm"
        placeholder="Search sounds…  e.g.  door -metal tag:foley dur:<2 is:loop"
        bind:value={library.search}
        spellcheck="false"
        autocomplete="off"
        oninput={() => (library.similarTo = null)}
        onkeydown={(e) => {
          if (e.key === "Escape") {
            if (library.search) library.search = "";
            else (e.currentTarget as HTMLInputElement).blur();
            e.preventDefault();
            e.stopPropagation();
          } else if (e.key === "ArrowDown" || e.key === "Enter") {
            // Into the results.
            e.preventDefault();
            (e.currentTarget as HTMLInputElement).blur();
            library.move(e.key === "Enter" ? 0 : 1);
          }
        }}
      />
      <span class="pointer-events-none absolute right-2 flex items-center gap-1 text-[11px] text-muted">
        {#if library.search}
          <button
            class="pointer-events-auto rounded p-0.5 hover:text-fg"
            title="Clear search"
            onclick={() => (library.search = "")}><Icon name="x" size={13} /></button
          >
        {:else}
          <kbd>{MOD}K</kbd>
        {/if}
      </span>
    </label>

    <label class="flex items-center gap-1.5 text-xs text-muted">
      Sort
      <select
        class="field py-1! text-xs text-fg"
        value={library.sort}
        onchange={(e) => {
          library.sort = (e.currentTarget as HTMLSelectElement).value as typeof library.sort;
          library.sortDesc = false;
          library.similarTo = null;
        }}
      >
        {#each SORTS as s (s.key)}
          <option value={s.key}>{s.label}</option>
        {/each}
      </select>
    </label>
    <button
      class="icon-btn"
      title={library.sortDesc ? "Descending" : "Ascending"}
      onclick={() => (library.sortDesc = !library.sortDesc)}
    >
      <Icon name={library.sortDesc ? "arrow-down" : "arrow-up"} size={15} />
    </button>
  </div>

  <div class="flex flex-wrap items-center gap-1.5">
    {#if similar}
      <button class="chip on" title="Back to the normal order" onclick={() => (library.similarTo = null)}>
        <Icon name="sparkles" size={12} /> Similar to “{similar.name}” <Icon name="x" size={11} />
      </button>
      <span class="mx-1 h-4 w-px bg-line"></span>
    {/if}
    {#each DURATIONS as d (d.label)}
      <button
        class="chip"
        class:on={sameRange(f.duration, d.range)}
        onclick={() => (f.duration = sameRange(f.duration, d.range) ? null : d.range)}
      >
        {d.label}
      </button>
    {/each}
    <span class="mx-1 h-4 w-px bg-line"></span>
    <button
      class="chip"
      class:on={f.loop === "loop"}
      onclick={() => (f.loop = f.loop === "loop" ? null : "loop")}
    >
      <Icon name="repeat" size={11} /> Loops
    </button>
    <button
      class="chip"
      class:on={f.loop === "oneshot"}
      onclick={() => (f.loop = f.loop === "oneshot" ? null : "oneshot")}
    >
      One-shots
    </button>
    <span class="mx-1 h-4 w-px bg-line"></span>
    <button
      class="chip"
      class:on={f.channels === "mono"}
      onclick={() => (f.channels = f.channels === "mono" ? null : "mono")}
    >
      Mono
    </button>
    <button
      class="chip"
      class:on={f.channels === "stereo"}
      onclick={() => (f.channels = f.channels === "stereo" ? null : "stereo")}
    >
      Stereo
    </button>
    {#if library.exts.length > 1}
      <span class="mx-1 h-4 w-px bg-line"></span>
      {#each library.exts as ext (ext)}
        <button class="chip uppercase" class:on={f.exts.includes(ext)} onclick={() => toggleExt(ext)}
          >{ext}</button
        >
      {/each}
    {/if}
    {#each f.tags as tag (tag)}
      <button class="chip on" title="Remove tag filter" onclick={() => library.toggleTagFilter(tag)}>
        <Icon name="tag" size={11} />
        {tag}
        <Icon name="x" size={11} />
      </button>
    {/each}

    <span class="ml-auto text-xs text-muted tabular-nums">
      {library.visible.length.toLocaleString()}
      {#if library.visible.length !== library.sounds.length}of {library.sounds.length.toLocaleString()}{/if}
      sounds
    </span>
    {#if anyFilter}
      <button class="chip" title="Clear search and all filters" onclick={() => library.resetView()}>
        <Icon name="x" size={11} /> Reset
      </button>
    {/if}
  </div>
</div>
