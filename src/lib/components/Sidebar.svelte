<script lang="ts">
  import { commands } from "$lib/bindings";
  import { attempt, library, type Folder } from "$lib/app.svelte";
  import { confirm } from "$lib/confirm.svelte";
  import { contextMenu } from "$lib/contextMenu.svelte";
  import { dirTree, type Collection } from "$lib/sounds";
  import DirTree from "./DirTree.svelte";
  import Icon, { type IconName } from "./Icon.svelte";

  let { width }: { width: number } = $props();

  let expanded = $state<Record<number, boolean>>({});
  let tagFilter = $state("");
  let showAllTags = $state(false);
  let renaming = $state<{ from: string; to: string } | null>(null);

  const counts = $derived.by(() => {
    let favorites = 0,
      loops = 0,
      recent = 0,
      unreadable = 0;
    for (const s of library.sounds) {
      if (s.favorite) favorites++;
      if (s.loopKind) loops++;
      if (s.lastPlayed) recent++;
      if (s.error) unreadable++;
    }
    return { favorites, loops, recent, unreadable };
  });

  const collections = $derived([
    { id: "all", label: "All sounds", icon: "library", count: library.sounds.length },
    { id: "favorites", label: "Favourites", icon: "star", count: counts.favorites },
    { id: "loops", label: "Loops", icon: "repeat", count: counts.loops },
    { id: "recent", label: "Recently played", icon: "clock", count: counts.recent },
    { id: "added", label: "Recently added", icon: "circle-plus", count: null },
    ...(counts.unreadable
      ? [{ id: "unreadable", label: "Unreadable", icon: "triangle-alert", count: counts.unreadable }]
      : []),
  ] as { id: Collection; label: string; icon: IconName; count: number | null }[]);

  /** Tags of the visible sounds, with the active ones first. */
  const tags = $derived.by(() => {
    const filter = tagFilter.trim().toLowerCase();
    const source = filter ? library.allTags : library.visibleTags;
    const active = library.filters.tags;
    const list = source.filter((t) => !active.includes(t.tag) && (!filter || t.tag.includes(filter)));
    return { active, list: showAllTags || filter ? list : list.slice(0, 40), more: list.length > 40 };
  });

  function selectCollection(id: Collection) {
    library.filters.collection = id;
    library.filters.folder = null;
    library.similarTo = null;
  }

  function folderMenu(e: MouseEvent, folder: Folder) {
    contextMenu.open(e, [
      { label: "Rescan", icon: "refresh", action: () => library.rescan([folder.id]) },
      {
        label: "Open in file manager",
        icon: "folder-open",
        action: () => attempt(() => commands.openFolder(folder.path)),
      },
      null,
      { label: "Remove from library", icon: "trash", danger: true, action: () => removeFolder(folder) },
    ]);
  }

  async function removeFolder(folder: Folder) {
    const ok = await confirm({
      title: "Remove folder?",
      message: `${folder.path}\n\nIts ${folder.soundCount} sounds leave the library, with their tags and favourites. The files themselves are not touched.`,
      okLabel: "Remove",
      danger: true,
    });
    if (!ok) return;
    await attempt(async () => {
      await commands.removeFolder(folder.id);
      await library.reload();
    }, "Folder removed");
  }

  async function finishRename() {
    const r = renaming;
    renaming = null;
    if (r && r.to.trim() && r.to.trim().toLowerCase() !== r.from) await library.renameTag(r.from, r.to);
  }
</script>

<aside class="flex shrink-0 flex-col overflow-hidden bg-surface" style:width="{width}px">
  <div class="flex items-center gap-2 px-4 pt-3 pb-2">
    <img src="/favicon.png" alt="" class="size-6" />
    <span class="text-[15px] font-semibold tracking-tight">SoundHive</span>
  </div>

  <div class="min-h-0 flex-1 overflow-y-auto px-2 pb-4">
    <nav class="space-y-px">
      {#each collections as c (c.id)}
        {@const active = library.filters.collection === c.id && !library.filters.folder}
        <button
          class="flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left text-[13px] hover:bg-raised"
          class:bg-raised={active}
          class:text-accent={active}
          onclick={() => selectCollection(c.id)}
        >
          <span class:text-muted={!active}><Icon name={c.icon} size={15} /></span>
          <span class="flex-1">{c.label}</span>
          {#if c.count !== null}<span class="text-[11px] text-muted tabular-nums"
              >{c.count.toLocaleString()}</span
            >{/if}
        </button>
      {/each}
    </nav>

    <div class="mt-5 mb-1 flex items-center px-2">
      <h3 class="flex-1 text-[11px] font-semibold tracking-wider text-muted uppercase">Folders</h3>
      <button
        class="icon-btn p-1!"
        title="Rescan all folders"
        disabled={!library.folders.length}
        onclick={() => library.rescan()}
      >
        <Icon name="refresh" size={13} />
      </button>
      <button class="icon-btn p-1!" title="Add a folder" onclick={() => library.addFolder()}>
        <Icon name="plus" size={14} />
      </button>
    </div>
    {#each library.folders as folder (folder.id)}
      {@const active = library.filters.folder?.id === folder.id && library.filters.folder.dir === ""}
      {@const tree = expanded[folder.id] ? dirTree(library.sounds, folder.id) : []}
      <div
        class="group flex items-center rounded-md pr-1 text-[13px] hover:bg-raised"
        class:bg-raised={active}
        class:text-accent={active}
        role="group"
        oncontextmenu={(e) => folderMenu(e, folder)}
      >
        <button
          class="grid size-6 shrink-0 place-items-center text-muted"
          aria-label={expanded[folder.id] ? "Collapse" : "Expand"}
          onclick={() => (expanded[folder.id] = !expanded[folder.id])}
        >
          <Icon name={expanded[folder.id] ? "chevron-down" : "chevron-right"} size={12} />
        </button>
        <button
          class="flex min-w-0 flex-1 items-center gap-2 py-1.5 text-left"
          title={folder.available
            ? folder.path
            : `${folder.path}\nNot found during the last scan (drive unplugged?)`}
          onclick={() => {
            library.filters.folder = active ? null : { id: folder.id, dir: "" };
            library.filters.collection = "all";
            library.similarTo = null;
          }}
        >
          <span class:text-muted={!active} class:text-warning={!folder.available}>
            <Icon name={folder.available ? "folder" : "triangle-alert"} size={14} />
          </span>
          <span class="truncate">{folder.name}</span>
          <span class="ml-auto text-[11px] text-muted tabular-nums group-hover:hidden">
            {folder.soundCount.toLocaleString()}
          </span>
        </button>
        <button
          class="icon-btn hidden p-1! group-hover:inline-flex"
          title="Folder actions"
          onclick={(e) => folderMenu(e, folder)}
        >
          <Icon name="settings" size={13} />
        </button>
      </div>
      {#if expanded[folder.id]}
        <DirTree nodes={tree} folderId={folder.id} />
      {/if}
    {:else}
      <button
        class="mx-2 mt-1 flex w-[calc(100%-1rem)] items-center gap-2 rounded-md border border-dashed border-line px-3 py-2 text-left text-xs text-muted hover:border-accent hover:text-fg"
        onclick={() => library.addFolder()}
      >
        <Icon name="folder-plus" size={15} /> Add a folder with sounds
      </button>
    {/each}

    <div class="mt-5 mb-1.5 flex items-center gap-2 px-2">
      <h3 class="text-[11px] font-semibold tracking-wider text-muted uppercase">Tags</h3>
      <input class="field ml-auto w-28 py-0.5! text-xs" placeholder="Find tag…" bind:value={tagFilter} />
    </div>
    <div class="flex flex-wrap gap-1 px-2">
      {#each tags.active as tag (tag)}
        <button class="chip on" onclick={() => library.toggleTagFilter(tag)}>
          {tag}
          <Icon name="x" size={10} />
        </button>
      {/each}
      {#each tags.list as { tag, count } (tag)}
        {#if renaming?.from === tag}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="field w-32 py-0! text-xs"
            bind:value={renaming.to}
            autofocus
            onblur={finishRename}
            onkeydown={(e) => {
              if (e.key === "Enter") finishRename();
              if (e.key === "Escape") {
                e.stopPropagation();
                renaming = null;
              }
            }}
          />
        {:else}
          <button
            class="chip"
            title="Filter by “{tag}” · right-click to rename"
            onclick={() => library.toggleTagFilter(tag)}
            oncontextmenu={(e) =>
              contextMenu.open(e, [
                { label: `Filter by “${tag}”`, icon: "filter", action: () => library.toggleTagFilter(tag) },
                {
                  label: "Rename tag everywhere…",
                  icon: "pencil",
                  action: () => (renaming = { from: tag, to: tag }),
                },
              ])}
          >
            {tag}
            <span class="text-[10px] opacity-60 tabular-nums">{count}</span>
          </button>
        {/if}
      {:else}
        {#if !tags.active.length}<p class="px-1 text-xs text-muted">No tags here.</p>{/if}
      {/each}
      {#if tags.more && !tagFilter}
        <button class="chip border-dashed" onclick={() => (showAllTags = !showAllTags)}>
          {showAllTags ? "Fewer" : "All tags…"}
        </button>
      {/if}
    </div>
  </div>
</aside>
