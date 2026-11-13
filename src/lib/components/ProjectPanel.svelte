<script lang="ts">
  import { attempt, library } from "$lib/app.svelte";
  import { commands } from "$lib/bindings";
  import { contextMenu } from "$lib/contextMenu.svelte";
  import { player } from "$lib/player.svelte";
  import { projects, type Slot } from "$lib/project.svelte";
  import { formatClock, formatDuration, normExt } from "$lib/sounds";
  import Icon, { type IconName } from "./Icon.svelte";
  import Waveform from "./Waveform.svelte";

  let { width }: { width: number } = $props();

  const project = $derived(projects.active);
  const slot = $derived(projects.slot);
  const total = $derived(projects.slots.length);
  const done = $derived(projects.doneCount);
  const candidate = $derived(library.focused);
  const fits = $derived(!!candidate && projects.fits(candidate.ext));
  const converts = $derived(!!candidate && projects.converts(candidate.ext));
  /** Upper-case formats of the slot and of the candidate sound. */
  const slotFormat = $derived(slot ? normExt(slot.ext).toUpperCase() : "");
  const candidateFormat = $derived(candidate ? normExt(candidate.ext).toUpperCase() : "");
  const replaceTitle = $derived.by(() => {
    if (!slot || !candidate) return "Select a sound in the library";
    if (!fits) {
      const needsFfmpeg = slotFormat === "MP3" || slotFormat === "M4A";
      return needsFfmpeg
        ? `The project uses ${slotFormat} here. Pick a ${slotFormat} sound, or install FFmpeg to convert other sounds to ${slotFormat}`
        : `The project uses ${slotFormat} here, which SoundHive cannot convert to; pick a ${slotFormat} sound`;
    }
    const conversion = converts ? `, converted from ${candidateFormat} to ${slotFormat}` : "";
    return `Write “${candidate.name}” into the project as ${slot.name}.${slot.ext}${conversion} (R)`;
  });
  const slotPlaying = $derived(!!slot && player.playing && player.currentId === -slot.id);

  /** Library sounds by path, to name what was tried. */
  const byPath = $derived(new Map(library.sounds.map((s) => [s.path, s])));
  const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

  const STATUS: Record<Slot["status"], { icon: IconName; label: string; class: string }> = {
    placeholder: { icon: "circle", label: "Placeholder", class: "text-muted" },
    trying: { icon: "circle-dot", label: "Trying out", class: "text-honey" },
    approved: { icon: "circle-check", label: "Done", class: "text-accent" },
  };

  function slotMenu(e: MouseEvent, s: Slot) {
    projects.selectSlot(s.id, false);
    contextMenu.open(e, [
      { label: "Play", icon: "play", hint: "G", action: () => player.playSlot(s) },
      {
        label: s.status === "approved" ? "Not done yet" : "Done",
        icon: "circle-check",
        hint: "A",
        action: () => projects.toggleApproved(),
      },
      ...(s.backedUp
        ? [{ label: "Restore the original", icon: "undo" as const, action: () => projects.restore() }]
        : []),
      null,
      { label: "Find sounds for it", icon: "search", action: () => findFor(s) },
      { label: "Show in folder", icon: "folder-open", action: () => projects.reveal(s) },
    ]);
  }

  function findFor(s: Slot) {
    projects.slotId = null;
    projects.selectSlot(s.id);
  }

  function useTried(path: string) {
    const sound = byPath.get(path);
    if (!sound) return;
    library.select(sound.id);
    player.play(sound);
  }
</script>

<aside class="flex shrink-0 flex-col overflow-hidden bg-surface" style:width="{width}px">
  {#if project}
    <!-- Header -->
    <div class="space-y-2 border-b border-line px-3 py-2.5">
      <div class="flex items-center gap-2">
        <span class="text-accent"><Icon name="target" size={16} /></span>
        <span class="min-w-0 flex-1 truncate text-[13px] font-semibold" title={project.path}
          >{project.name}</span
        >
        <button class="icon-btn p-1!" title="Look for changed files" onclick={() => projects.refresh()}>
          <Icon name="refresh" size={13} />
        </button>
        <button
          class="icon-btn p-1!"
          title="Open the folder"
          onclick={() => attempt(() => commands.openFolder(project.path))}
        >
          <Icon name="folder-open" size={13} />
        </button>
        <button class="icon-btn p-1!" title="Close the project" onclick={() => projects.close()}>
          <Icon name="x" size={14} />
        </button>
      </div>
      <div class="flex items-center gap-2 text-xs text-muted">
        <span class="h-1.5 flex-1 overflow-hidden rounded-full bg-raised">
          <span
            class="block h-full rounded-full bg-accent transition-[width]"
            style:width="{total ? (done / total) * 100 : 0}%"
          ></span>
        </span>
        <span class="tabular-nums">{done} / {total} done</span>
      </div>
    </div>

    <!-- The slot being worked on -->
    {#if slot}
      {@const status = STATUS[slot.status]}
      {@const current = slot.sourcePath ? byPath.get(slot.sourcePath) : undefined}
      <div class="space-y-2.5 border-b border-line p-3">
        <div class="flex items-start gap-2">
          <div class="min-w-0 flex-1">
            <h2 class="text-[15px] leading-snug font-semibold break-words select-text">{slot.name}</h2>
            <p class="truncate text-xs text-muted" title={slot.relPath}>
              {slot.dir ? `${slot.dir} · ` : ""}{normExt(slot.ext).toUpperCase()} · {formatDuration(
                slot.duration,
              )}
            </p>
          </div>
          <span class="flex shrink-0 items-center gap-1 text-xs font-medium {status.class}">
            <Icon name={status.icon} size={13} />
            {status.label}
          </span>
        </div>

        <div class="rounded-lg border border-line bg-bg/60 p-2">
          <Waveform
            id={-slot.id}
            peaks={slot.peaks}
            height={44}
            progress={slotPlaying && slot.duration ? player.position / slot.duration : null}
            duration={slot.duration}
            onseek={(f) => player.seekSlot(slot, f * slot.duration)}
          />
          <div class="mt-1.5 flex items-center gap-2 text-xs">
            <button
              class="grid size-7 shrink-0 place-items-center rounded-full bg-accent text-bg hover:brightness-110"
              title={slotPlaying ? "Stop (G)" : "Play what is in the project now (G)"}
              onclick={() => player.toggleSlot(slot)}
            >
              <Icon name={slotPlaying ? "stop" : "play"} size={13} filled />
            </button>
            <span class="min-w-0 flex-1 truncate text-muted" title={slot.sourcePath ?? "The original file"}>
              {#if slot.sourcePath}
                In the project: <span class="text-fg">{current?.name ?? fileName(slot.sourcePath)}</span>
              {:else}
                The original sound
              {/if}
            </span>
            <span class="font-mono text-muted tabular-nums">
              {formatClock(slotPlaying ? player.position : 0)}
            </span>
          </div>
        </div>

        {#if slot.error}
          <p class="rounded-md border border-warning/40 bg-warning/10 p-2 text-xs">
            Could not be read: {slot.error}
          </p>
        {/if}

        <!-- Replace -->
        <button
          class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-[13px] font-medium"
          class:bg-accent={fits}
          class:text-bg={fits}
          class:hover:brightness-110={fits}
          class:bg-raised={!fits}
          class:text-muted={!fits}
          disabled={!fits || projects.busy}
          title={replaceTitle}
          onclick={() => projects.replace()}
        >
          <Icon name="replace" size={15} />
          <span class="min-w-0 flex-1 truncate">
            {#if !candidate}
              Select a sound in the library
            {:else if fits}
              Use “{candidate.name}”{#if converts}<span class="opacity-75">
                  · {candidateFormat} → {slotFormat}</span
                >{/if}
            {:else}
              Needs a {slotFormat} sound
            {/if}
          </span>
          <kbd class="opacity-80">R</kbd>
        </button>

        <div class="grid grid-cols-2 gap-1.5">
          <button
            class="flex items-center gap-1.5 rounded-md border border-line bg-raised/60 px-2.5 py-1.5 text-[13px] hover:border-accent/60 hover:bg-raised"
            title={slot.status === "approved"
              ? "Open it again (A)"
              : "It fits: mark it as done and go to the next sound (A)"}
            disabled={projects.busy}
            onclick={() => projects.toggleApproved()}
          >
            <span class="text-accent"><Icon name="circle-check" size={14} /></span>
            {slot.status === "approved" ? "Not done yet" : "Done, next"}
          </button>
          <button
            class="flex items-center gap-1.5 rounded-md border border-line bg-raised/60 px-2.5 py-1.5 text-[13px] hover:border-accent/60 hover:bg-raised disabled:opacity-40"
            title="Put the original sound back"
            disabled={!slot.backedUp || !slot.sourcePath || projects.busy}
            onclick={() => projects.restore()}
          >
            <span class="text-muted"><Icon name="undo" size={14} /></span>
            Original
          </button>
        </div>

        {#if slot.tries.length}
          <section>
            <h3
              class="mb-1 flex items-center gap-1 text-[11px] font-semibold tracking-wider text-muted uppercase"
            >
              <Icon name="history" size={11} /> Tried
            </h3>
            <ul class="max-h-36 space-y-px overflow-y-auto">
              {#each slot.tries as t (t.sourcePath)}
                {@const sound = byPath.get(t.sourcePath)}
                {@const inUse = t.sourcePath === slot.sourcePath}
                <li class="group flex items-center gap-1.5 rounded px-1 text-xs hover:bg-raised">
                  <button
                    class="min-w-0 flex-1 truncate py-1 text-left"
                    class:text-fg={inUse}
                    class:text-muted={!inUse}
                    class:line-through={!sound}
                    title={sound
                      ? `${t.sourcePath}\nClick to select and play it`
                      : `${t.sourcePath}\nNo longer in the library`}
                    disabled={!sound}
                    onclick={() => useTried(t.sourcePath)}
                  >
                    {sound?.name ?? fileName(t.sourcePath)}
                  </button>
                  {#if inUse}
                    <span class="shrink-0 text-[10px] font-medium text-honey uppercase">in use</span>
                  {:else if sound}
                    <button
                      class="hidden shrink-0 rounded px-1.5 py-0.5 text-[11px] text-accent group-hover:block hover:bg-bg"
                      title="Put this one into the project again"
                      onclick={() => projects.replace(sound.id)}
                    >
                      Use again
                    </button>
                  {/if}
                </li>
              {/each}
            </ul>
          </section>
        {/if}
      </div>
    {/if}

    <!-- All sound files of the project -->
    <div class="flex items-center gap-2 px-3 pt-2.5 pb-1">
      <h3 class="flex-1 text-[11px] font-semibold tracking-wider text-muted uppercase">Project sounds</h3>
      <label class="flex items-center gap-1 text-[11px] text-muted">
        <input
          type="checkbox"
          checked={projects.hideDone}
          onchange={(e) => projects.setHideDone((e.currentTarget as HTMLInputElement).checked)}
        />
        Hide done
      </label>
    </div>
    <div class="min-h-0 flex-1 overflow-y-auto px-1.5 pb-3" role="listbox" aria-label="Project sounds">
      {#each projects.shownSlots as s (s.id)}
        {@const st = STATUS[s.status]}
        {@const active = s.id === projects.slotId}
        {@const playing = player.playing && player.currentId === -s.id}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          role="option"
          aria-selected={active}
          tabindex="-1"
          class="group flex cursor-default items-center gap-2 rounded-md px-1.5 py-1 text-[13px] hover:bg-raised"
          class:bg-raised={active}
          title={s.relPath}
          onclick={() => projects.selectSlot(s.id)}
          ondblclick={() => player.playSlot(s)}
          oncontextmenu={(e) => slotMenu(e, s)}
        >
          <span class="shrink-0 {st.class}" title={st.label}><Icon name={st.icon} size={13} /></span>
          <div class="min-w-0 flex-1 leading-tight">
            <div class="truncate" class:text-accent={active} class:font-medium={active}>{s.name}</div>
            {#if s.dir}<div class="truncate text-[11px] text-muted">{s.dir}</div>{/if}
          </div>
          <button
            class="grid size-5 shrink-0 place-items-center rounded-full text-muted hover:text-fg"
            class:invisible={!playing}
            class:group-hover:visible={!playing}
            class:text-accent={playing}
            title={playing ? "Stop" : "Play"}
            onclick={(e) => {
              e.stopPropagation();
              player.toggleSlot(s);
            }}
          >
            <Icon name={playing ? "stop" : "play"} size={11} filled />
          </button>
        </div>
      {:else}
        <p class="px-2 py-4 text-center text-xs text-muted">
          {#if total}
            All done! 🎉
          {:else}
            No sound files in this folder.
          {/if}
        </p>
      {/each}
    </div>

    <div class="border-t border-line px-3 py-2 text-[11px] leading-relaxed text-muted">
      <kbd>R</kbd> use the selected sound · <kbd>A</kbd> done, next · <kbd>G</kbd> play the project's sound ·
      <kbd>Alt+↑↓</kbd> other project sound
    </div>
  {/if}
</aside>
