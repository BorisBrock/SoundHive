<script lang="ts">
  import { library } from "$lib/app.svelte";
  import type { DirNode } from "$lib/sounds";
  import DirTree from "./DirTree.svelte";
  import Icon from "./Icon.svelte";

  let { nodes, folderId, depth = 1 }: { nodes: DirNode[]; folderId: number; depth?: number } = $props();

  let open = $state<Record<string, boolean>>({});
</script>

{#each nodes as node (node.dir)}
  {@const active = library.filters.folder?.id === folderId && library.filters.folder.dir === node.dir}
  <div
    class="group flex items-center rounded-md pr-2 text-[13px] hover:bg-raised"
    class:bg-raised={active}
    class:text-accent={active}
    style:padding-left="{depth * 12 + 4}px"
  >
    <button
      class="grid size-5 shrink-0 place-items-center text-muted"
      class:invisible={!node.children.length}
      aria-label={open[node.dir] ? "Collapse" : "Expand"}
      onclick={() => (open[node.dir] = !open[node.dir])}
    >
      <Icon name={open[node.dir] ? "chevron-down" : "chevron-right"} size={12} />
    </button>
    <button
      class="flex min-w-0 flex-1 items-center gap-1.5 py-1 text-left"
      title={node.dir}
      onclick={() => {
        library.filters.folder = active ? null : { id: folderId, dir: node.dir };
        library.similarTo = null;
      }}
    >
      <span class="truncate">{node.name}</span>
      <span class="ml-auto text-[11px] text-muted tabular-nums">{node.count}</span>
    </button>
  </div>
  {#if open[node.dir] && node.children.length}
    <DirTree nodes={node.children} {folderId} depth={depth + 1} />
  {/if}
{/each}
