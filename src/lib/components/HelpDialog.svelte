<script lang="ts">
  import { library } from "$lib/app.svelte";
  import { MOD } from "$lib/actions";
  import Modal from "./Modal.svelte";

  const keys: [string, string][] = [
    ["↑ / ↓", "Previous / next sound (plays it when auto-play is on)"],
    ["Shift+↑ / ↓", "Extend the selection"],
    ["Space", "Play / stop"],
    ["Enter", "Play from the start"],
    ["← / →", "Skip back / ahead"],
    ["L", "Loop playback on / off"],
    ["F", "Favourite"],
    ["S", "Find similar sounds"],
    ["T", "Add a tag"],
    [`${MOD}C`, "Copy the file(s) to the clipboard, to paste into other apps"],
    [`${MOD}⇧C`, "Copy the path(s)"],
    ["E", "Open in the editor (Audacity)"],
    ["O", "Show in the file manager"],
    ["P", "Copy into the project folder"],
    [`${MOD}A`, "Select all sounds in the list"],
    [`${MOD}K or /`, "Search"],
    ["Esc", "Stop, clear the search, close dialogs"],
    [`${MOD},`, "Settings"],
  ];

  const search: [string, string][] = [
    ["door slam", "both words, in name, folder, tags or description"],
    ['"door slam"', "the exact phrase"],
    ["-metal", "without “metal”"],
    ["tag:foley", "with the tag (also t:)"],
    ["ext:wav,ogg", "file types"],
    ["dur:<2  dur:>10  dur:1-3  dur:500ms", "length in seconds (s, ms, m)"],
    ["is:loop  -is:loop  is:fav", "loops, one-shots, favourites"],
  ];
</script>

<Modal title="Shortcuts & search" onclose={() => (library.helpOpen = false)} wide>
  <div class="grid gap-8 md:grid-cols-2">
    <section>
      <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Keyboard</h3>
      <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-sm">
        {#each keys as [k, v] (k)}
          <dt><kbd>{k}</kbd></dt>
          <dd class="text-muted">{v}</dd>
        {/each}
      </dl>
    </section>
    <section>
      <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Search</h3>
      <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-sm">
        {#each search as [k, v] (k)}
          <dt><code class="rounded bg-raised px-1.5 font-mono text-xs">{k}</code></dt>
          <dd class="text-muted">{v}</dd>
        {/each}
      </dl>
      <h3 class="mt-6 mb-2 text-xs font-semibold tracking-wider text-muted uppercase">
        Getting sounds into other apps
      </h3>
      <ul class="list-disc space-y-1.5 pl-5 text-sm text-muted">
        <li>
          <strong class="text-fg">Drag</strong> a row (or the drag area in the details) into Audacity, a DAW, a
          game engine, a video editor or a folder.
        </li>
        <li>
          <strong class="text-fg">{MOD}C</strong> puts the file on the clipboard; paste it into a file manager or
          any app that accepts files.
        </li>
        <li>
          <strong class="text-fg">E</strong> opens the sound in Audacity, or the editor set in Settings.
        </li>
        <li>
          <strong class="text-fg">P</strong> copies it into your project folder (asked once, then remembered).
        </li>
      </ul>
    </section>
  </div>
</Modal>
