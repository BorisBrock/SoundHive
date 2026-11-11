<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { library } from "$lib/app.svelte";
  import { confirmState } from "$lib/confirm.svelte";
  import { contextMenu } from "$lib/contextMenu.svelte";
  import { player } from "$lib/player.svelte";
  import { projects } from "$lib/project.svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import ContextMenu from "$lib/components/ContextMenu.svelte";
  import DetailPanel from "$lib/components/DetailPanel.svelte";
  import HelpDialog from "$lib/components/HelpDialog.svelte";
  import ProjectPanel from "$lib/components/ProjectPanel.svelte";
  import Icon from "$lib/components/Icon.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import SoundList from "$lib/components/SoundList.svelte";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import Toolbar from "$lib/components/Toolbar.svelte";

  let dropping = $state(false);

  onMount(() => {
    library.load();
    projects.load();
    // Folders dropped onto the window are added to the library.
    const unlisten = getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "enter") dropping = !library.draggingOut;
      else if (p.type === "leave") dropping = false;
      else if (p.type === "drop") {
        dropping = false;
        const ownDrag = library.draggingOut;
        library.draggingOut = null;
        // Our own sounds, dragged out and back in.
        if (ownDrag) return;
        for (const path of p.paths) library.addFolder(path);
      }
    });
    // An outgoing drag that ended outside the window leaves no drop event behind.
    const clear = () => setTimeout(() => (library.draggingOut = null), 500);
    window.addEventListener("pointerup", clear);
    window.addEventListener("focus", clear);
    // Back from the game, engine or editor: pick up files changed there.
    const refreshProject = () => projects.refresh(true);
    window.addEventListener("focus", refreshProject);
    return () => {
      unlisten.then((f) => f());
      window.removeEventListener("pointerup", clear);
      window.removeEventListener("focus", clear);
      window.removeEventListener("focus", refreshProject);
    };
  });

  // Panel widths are a per-machine layout preference, so they live in localStorage.
  function panel(key: string, initial: number, min: number, max: number) {
    const clamp = (w: number) => Math.round(Math.min(max, Math.max(min, w)));
    let width = $state(clamp(Number(localStorage.getItem(key)) || initial));
    return {
      get width() {
        return width;
      },
      set(w: number) {
        width = clamp(w);
        localStorage.setItem(key, String(width));
      },
      reset() {
        this.set(initial);
      },
    };
  }
  const left = panel("sidebarWidth", 264, 200, 480);
  const right = panel("detailWidth", 360, 300, 640);
  const middle = panel("projectWidth", 300, 240, 520);
  let resizing = $state<"left" | "middle" | "right" | null>(null);

  function onSplitterMove(e: PointerEvent) {
    if (resizing === "left") left.set(e.clientX);
    else if (resizing === "middle") middle.set(e.clientX - left.width);
    else if (resizing === "right") right.set(window.innerWidth - e.clientX);
  }

  const dialogOpen = $derived(library.settingsOpen || library.helpOpen || !!confirmState.current);

  /** Global shortcuts; single keys only when not typing somewhere. */
  function onkeydown(e: KeyboardEvent) {
    const typing =
      e.target instanceof Element && e.target.closest("input, textarea, select, [contenteditable=true]");
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();
    if (mod && key === ",") {
      e.preventDefault();
      library.settingsOpen = true;
      return;
    }
    if (dialogOpen || contextMenu.current) return;
    if ((mod && key === "k") || (!typing && !mod && e.key === "/")) {
      e.preventDefault();
      const input = document.getElementById("search") as HTMLInputElement | null;
      input?.focus();
      input?.select();
      return;
    }
    if (typing) return;
    const s = library.focused;
    const run = (f: () => unknown) => {
      e.preventDefault();
      f();
    };
    if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "PageDown" || e.key === "PageUp") {
      const step = e.key.startsWith("Page") ? 15 : 1;
      const delta = e.key === "ArrowDown" || e.key === "PageDown" ? step : -step;
      run(() => {
        const next = library.move(delta, e.shiftKey);
        if (next && library.settings.autoplay && !e.shiftKey) player.play(next);
      });
    } else if (e.key === "Home" && !mod) {
      run(() => library.move(-Infinity));
    } else if (e.key === "End" && !mod) {
      run(() => library.move(Infinity));
    } else if (e.key === " ") {
      run(() => player.toggle(s));
    } else if (e.key === "Enter") {
      run(() => s && player.play(s));
    } else if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      if (!s) return;
      const stepSec = Math.max(0.25, s.duration / 10);
      const from = player.currentId === s.id && player.playing ? player.position : 0;
      run(() => player.seek(s, from + (e.key === "ArrowRight" ? stepSec : -stepSec)));
    } else if (e.key === "Escape") {
      run(() => {
        if (player.playing) player.stop();
        else if (library.selected.size) library.selected = new Set();
        else if (library.similarTo !== null) library.similarTo = null;
      });
    } else if (mod && key === "a") {
      run(() => library.selectAll());
    } else if (mod && key === "c") {
      run(() => (e.shiftKey ? library.copyPaths() : library.copyFiles()));
    } else if (e.altKey && !mod && (e.key === "ArrowDown" || e.key === "ArrowUp") && projects.active) {
      run(() => projects.step(e.key === "ArrowDown" ? 1 : -1));
    } else if (mod || e.altKey) {
      return;
    } else if (key === "l") {
      run(() => player.toggleLoop());
    } else if (key === "f") {
      run(() => library.toggleFavorite());
    } else if (key === "s") {
      run(() => library.findSimilar());
    } else if (key === "e") {
      run(() => library.openInEditor());
    } else if (key === "o") {
      run(() => library.reveal());
    } else if (key === "p") {
      run(() => library.copyToFolder());
    } else if (key === "r" && projects.active) {
      run(() => projects.replace());
    } else if (key === "a" && projects.slot) {
      run(() => projects.toggleApproved());
    } else if (key === "g" && projects.slot) {
      run(() => player.toggleSlot(projects.slot));
    } else if (key === "t") {
      run(() => document.getElementById("tag-input")?.focus());
    } else if (e.key === "?") {
      run(() => (library.helpOpen = true));
    }
  }
</script>

<svelte:window {onkeydown} />

<div
  class="flex h-screen flex-col"
  class:select-none={!!resizing}
  class:cursor-col-resize={!!resizing}
  role="presentation"
  onpointermove={onSplitterMove}
  onpointerup={() => (resizing = null)}
>
  <div class="flex min-h-0 flex-1">
    <Sidebar width={left.width} />
    {@render splitter("left", "Resize sidebar", () => left.reset())}
    {#if projects.active}
      <ProjectPanel width={middle.width} />
      {@render splitter("middle", "Resize project panel", () => middle.reset())}
    {/if}

    <main class="flex min-w-0 flex-1 flex-col">
      {#if library.loaded && !library.folders.length}
        <div class="grid flex-1 place-items-center p-8">
          <div class="max-w-md text-center">
            <img src="/favicon.png" alt="" class="mx-auto mb-5 size-16 opacity-90" />
            <h1 class="mb-2 text-xl font-semibold">Welcome to SoundHive</h1>
            <p class="mb-6 text-sm leading-relaxed text-muted">
              Add the folders that hold your sound effects. SoundHive finds every WAV, MP3, OGG, FLAC and AIFF
              file in them, measures and tags it, and keeps the library up to date.
            </p>
            <button
              class="inline-flex items-center gap-2 rounded-lg bg-accent px-4 py-2 font-medium text-bg hover:brightness-110"
              onclick={() => library.addFolder()}
            >
              <Icon name="folder-plus" size={16} /> Add a folder
            </button>
            <p class="mt-3 text-xs text-muted">…or drop folders onto this window.</p>
          </div>
        </div>
      {:else}
        <Toolbar />
        <SoundList />
      {/if}
    </main>

    {@render splitter("right", "Resize details panel", () => right.reset())}
    <DetailPanel width={right.width} />
  </div>
  <StatusBar />
</div>

{#snippet splitter(side: "left" | "middle" | "right", label: string, reset: () => void)}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    title="Drag to resize, double-click to reset"
    aria-label={label}
    class="group relative w-px shrink-0 cursor-col-resize bg-line"
    onpointerdown={(e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      resizing = side;
    }}
    ondblclick={reset}
  >
    <div
      class="absolute inset-y-0 -right-1 -left-1 z-10 transition-colors group-hover:bg-accent/40"
      class:bg-accent={resizing === side}
    ></div>
  </div>
{/snippet}

{#if dropping}
  <div class="pointer-events-none fixed inset-0 z-50 grid place-items-center bg-bg/70 backdrop-blur-sm">
    <div class="rounded-2xl border-2 border-dashed border-accent px-10 py-8 text-center">
      <div class="mb-2 flex justify-center text-accent"><Icon name="folder-plus" size={36} /></div>
      <div class="text-lg font-semibold">Drop folders to add them to the library</div>
    </div>
  </div>
{/if}

{#if library.settingsOpen}
  <SettingsDialog />
{/if}
{#if library.helpOpen}
  <HelpDialog />
{/if}
<ContextMenu />
<Toasts />
<!-- Last, so it stacks above any dialog that asked for it. -->
{#if confirmState.current}
  {#key confirmState.current}
    <ConfirmDialog request={confirmState.current} />
  {/key}
{/if}
