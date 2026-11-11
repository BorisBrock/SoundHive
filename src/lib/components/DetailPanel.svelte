<script lang="ts">
  import { commands } from "$lib/bindings";
  import { attempt, library } from "$lib/app.svelte";
  import { MOD } from "$lib/actions";
  import { player } from "$lib/player.svelte";
  import {
    formatChannels,
    formatClock,
    formatDate,
    formatDuration,
    formatRate,
    formatSize,
    LOOP_LABELS,
    normExt,
    tagCounts,
  } from "$lib/sounds";
  import Icon, { type IconName } from "./Icon.svelte";
  import Waveform from "./Waveform.svelte";

  let { width }: { width: number } = $props();

  const s = $derived(library.focused);
  const many = $derived(library.selection.length > 1);
  const current = $derived(!!s && player.currentId === s.id);
  const playing = $derived(current && player.playing);

  let editorName = $state<string | null>(null);
  $effect(() => {
    const editor = library.settings.editor;
    commands
      .editorName(editor)
      .then((n) => (editorName = n))
      .catch(() => (editorName = null));
  });

  let newTag = $state("");
  async function addTag() {
    const tag = newTag.trim();
    if (!tag) return;
    newTag = "";
    await library.addTag(tag);
  }

  /** Tags of the whole selection, with how many of the selected sounds have them. */
  const selectionTags = $derived(tagCounts(library.selection));

  function brightness(zcr: number) {
    if (!zcr) return "–";
    const word = zcr < 600 ? "dark" : zcr < 2500 ? "warm" : zcr < 6000 ? "bright" : "noisy";
    return `${word} (${Math.round(zcr).toLocaleString()} zc/s)`;
  }

  function db(v: number) {
    return v <= -119 ? "silent" : `${v.toFixed(1)} dBFS`;
  }

  const info = $derived(
    s
      ? [
          ["Length", formatDuration(s.duration)],
          ["Format", `${normExt(s.ext).toUpperCase()}${s.bits ? ` · ${s.bits}-bit` : ""}`],
          ["Sample rate", formatRate(s.sampleRate)],
          ["Channels", formatChannels(s.channels)],
          ["Peak", db(s.peakDb)],
          ["Loudness", db(s.rmsDb)],
          ["Attack", s.attack ? `${(s.attack * 1000).toFixed(0)} ms` : "–"],
          ["Tone", brightness(s.zcr)],
          ["Loop", s.loopKind ? (LOOP_LABELS[s.loopKind] ?? s.loopKind) : "One-shot"],
          ["File size", formatSize(s.size)],
          ["Modified", formatDate(s.modified)],
          ["Played", s.playCount ? `${s.playCount}× · last ${formatDate(s.lastPlayed)}` : "Never"],
        ]
      : [],
  );

  const totalDuration = $derived(library.selection.reduce((a, x) => a + x.duration, 0));

  function startDrag(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    const ids = library.selectionIds;
    library.draggingOut = ids;
    attempt(() => commands.startDrag(ids));
  }
</script>

{#snippet action(icon: IconName, label: string, run: () => void, hint = "", title = label)}
  <button
    class="flex items-center gap-2 rounded-md border border-line bg-raised/60 px-2.5 py-1.5 text-left text-[13px] hover:border-accent/60 hover:bg-raised"
    title={hint ? `${title} (${hint})` : title}
    onclick={run}
  >
    <span class="text-muted"><Icon name={icon} size={14} /></span>
    <span class="truncate">{label}</span>
  </button>
{/snippet}

<aside class="flex shrink-0 flex-col overflow-hidden bg-surface" style:width="{width}px">
  {#if s}
    <div class="min-h-0 flex-1 overflow-y-auto">
      <div class="space-y-4 p-4">
        <!-- Header -->
        <div class="flex items-start gap-2">
          <div class="min-w-0 flex-1">
            {#if many}
              <h2 class="text-base font-semibold">{library.selection.length} sounds selected</h2>
              <p class="text-xs text-muted">{formatDuration(totalDuration)} in total · focused: {s.name}</p>
            {:else}
              <h2 class="text-base leading-snug font-semibold break-words select-text">{s.name}</h2>
              <p class="truncate text-xs text-muted" title={s.path}>
                {library.folders.find((f) => f.id === s.folderId)?.name ?? ""}{s.dir ? ` / ${s.dir}` : ""}
              </p>
            {/if}
          </div>
          <button
            class="icon-btn"
            class:text-honey!={s.favorite}
            title={s.favorite ? "Remove from favourites (F)" : "Add to favourites (F)"}
            onclick={() => library.toggleFavorite()}
          >
            <Icon name="star" size={18} filled={s.favorite} />
          </button>
        </div>

        <!-- Waveform and transport -->
        <div class="rounded-lg border border-line bg-bg/60 p-2">
          <Waveform
            id={s.id}
            height={88}
            progress={playing && s.duration ? player.position / s.duration : null}
            duration={s.duration}
            onseek={(f) => player.seek(s, f * s.duration)}
          />
          <div class="mt-2 flex items-center gap-1">
            <button
              class="grid size-8 place-items-center rounded-full bg-accent text-bg hover:brightness-110"
              title={playing ? "Stop (Space)" : "Play (Space)"}
              onclick={() => player.toggle(s)}
            >
              <Icon name={playing ? "stop" : "play"} size={15} filled />
            </button>
            <button
              class="icon-btn"
              class:on={player.loop}
              title={player.loop ? "Looping playback is on (L)" : "Loop playback (L)"}
              onclick={() => player.toggleLoop()}
            >
              <Icon name="repeat" size={16} />
            </button>
            <span class="ml-auto font-mono text-xs text-muted tabular-nums">
              {formatClock(playing ? player.position : 0)} / {formatClock(s.duration)}
            </span>
          </div>
        </div>

        <!-- Drag out -->
        <div
          class="flex cursor-grab items-center gap-3 rounded-lg border border-dashed border-accent/50 bg-accent/5 px-3 py-2.5 text-[13px] hover:bg-accent/10 active:cursor-grabbing"
          role="button"
          tabindex="-1"
          title="Drag the file into Audacity, a DAW, a game engine, a video editor or a folder"
          onpointerdown={startDrag}
        >
          <span class="text-accent"><Icon name="hand" size={18} /></span>
          <div class="leading-tight">
            <div class="font-medium">
              Drag {many ? `${library.selection.length} files` : "the file"} into any app
            </div>
            <div class="text-xs text-muted">Audacity, your DAW, Godot, Unity, Resolve, …</div>
          </div>
        </div>

        <!-- Actions -->
        <div class="grid grid-cols-2 gap-1.5">
          {@render action(
            "clipboard",
            many ? "Copy files" : "Copy file",
            () => library.copyFiles(),
            `${MOD}C`,
            "Put the file on the clipboard; paste it in a file manager, DAW or editor",
          )}
          {@render action("copy", many ? "Copy paths" : "Copy path", () => library.copyPaths(), `${MOD}⇧C`)}
          {@render action(
            "external-link",
            editorName ? `Open in ${editorName}` : "Open in editor",
            () => library.openInEditor(),
            "E",
            editorName ? `Open in ${editorName}` : "Set an editor in Settings",
          )}
          {@render action(
            "folder-input",
            library.settings.exportFolder
              ? `To ${library.settings.exportFolder.split(/[\\/]/).filter(Boolean).pop()}`
              : "Copy to folder…",
            () => library.copyToFolder(),
            "P",
            library.settings.exportFolder
              ? `Copy into ${library.settings.exportFolder} (another folder: right-click a sound)`
              : "Copy into a folder, e.g. your game's assets",
          )}
          {@render action("folder-open", "Show in folder", () => library.reveal(s.id), "O")}
          {@render action("sparkles", "Find similar", () => library.findSimilar(s.id), "S")}
        </div>

        <!-- Tags -->
        <section>
          <h3 class="mb-1.5 text-[11px] font-semibold tracking-wider text-muted uppercase">Tags</h3>
          <div class="flex flex-wrap gap-1">
            {#each many ? selectionTags : s.tags.map((tag) => ({ tag, count: 1 })) as { tag, count } (tag)}
              {@const user = !many && s.userTags.includes(tag)}
              <span class="chip pr-1!" class:border-accent={user} title={user ? "Your tag" : "Automatic tag"}>
                <button class="hover:text-fg" onclick={() => library.toggleTagFilter(tag)}>{tag}</button>
                {#if many && count < library.selection.length}
                  <span class="text-[10px] opacity-60">{count}/{library.selection.length}</span>
                {/if}
                <button
                  class="rounded-full p-0.5 hover:bg-raised hover:text-danger"
                  title="Remove tag"
                  onclick={() => library.removeTag(tag)}
                >
                  <Icon name="x" size={10} />
                </button>
              </span>
            {/each}
          </div>
          <form
            class="mt-2 flex gap-1.5"
            onsubmit={(e) => {
              e.preventDefault();
              addTag();
            }}
          >
            <input
              id="tag-input"
              class="field min-w-0 flex-1 py-1! text-xs"
              placeholder={many ? `Add a tag to ${library.selection.length} sounds…` : "Add a tag…"}
              list="all-tags"
              bind:value={newTag}
              onkeydown={(e) => {
                if (e.key === "Escape") {
                  e.stopPropagation();
                  newTag = "";
                  (e.currentTarget as HTMLInputElement).blur();
                }
              }}
            />
            <button class="icon-btn border border-line" title="Add tag" disabled={!newTag.trim()}
              ><Icon name="plus" size={14} /></button
            >
            <datalist id="all-tags">
              {#each library.allTags.slice(0, 300) as t (t.tag)}<option value={t.tag}></option>{/each}
            </datalist>
          </form>
        </section>

        {#if !many}
          {#if s.description}
            <section>
              <h3 class="mb-1 text-[11px] font-semibold tracking-wider text-muted uppercase">Description</h3>
              <p class="text-[13px] leading-relaxed select-text">{s.description}</p>
            </section>
          {/if}

          {#if s.error}
            <p class="rounded-lg border border-warning/40 bg-warning/10 p-2.5 text-xs leading-relaxed">
              <strong>Could not be analysed:</strong>
              {s.error}
            </p>
          {/if}

          <!-- Info -->
          <section>
            <h3 class="mb-1.5 text-[11px] font-semibold tracking-wider text-muted uppercase">Info</h3>
            <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[13px]">
              {#each info as [label, value] (label)}
                <dt class="text-muted">{label}</dt>
                <dd class="min-w-0 truncate select-text" title={value}>{value}</dd>
              {/each}
            </dl>
            <button
              class="mt-2 w-full rounded-md bg-bg/60 px-2 py-1.5 text-left font-mono text-[11px] break-all text-muted hover:text-fg"
              title="Copy path"
              onclick={() => library.copyPaths([s.id])}
            >
              {s.path}
            </button>
          </section>
        {/if}
      </div>
    </div>
  {:else}
    <div class="grid flex-1 place-items-center p-6 text-center text-sm text-muted">
      <div>
        <div class="mb-3 flex justify-center opacity-50"><Icon name="audio-lines" size={40} /></div>
        Select a sound to see its details.
        <div class="mt-4 space-y-1 text-xs">
          <div><kbd>↑</kbd> <kbd>↓</kbd> browse &amp; audition</div>
          <div><kbd>Space</kbd> play / stop</div>
          <div><kbd>?</kbd> all shortcuts</div>
        </div>
      </div>
    </div>
  {/if}
</aside>
