<script lang="ts">
  import { contextMenu } from "$lib/contextMenu.svelte";
  import Icon from "./Icon.svelte";

  let menu = $state<HTMLDivElement>();
  let pos = $state({ x: 0, y: 0 });

  // Keep the menu inside the window.
  $effect(() => {
    const c = contextMenu.current;
    if (!c || !menu) return;
    const r = menu.getBoundingClientRect();
    pos = {
      x: Math.min(c.x, window.innerWidth - r.width - 4),
      y: Math.min(c.y, window.innerHeight - r.height - 4),
    };
    menu.querySelector<HTMLElement>("button")?.focus();
  });

  function onkeydown(e: KeyboardEvent) {
    if (!contextMenu.current) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      contextMenu.close();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      const buttons = [...(menu?.querySelectorAll<HTMLElement>("button") ?? [])];
      const i = buttons.indexOf(document.activeElement as HTMLElement);
      const next = (i + (e.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length;
      buttons[next]?.focus();
    }
  }
</script>

<svelte:window onkeydowncapture={onkeydown} />

{#if contextMenu.current}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div
    class="fixed inset-0 z-40"
    onclick={() => contextMenu.close()}
    oncontextmenu={(e) => {
      e.preventDefault();
      contextMenu.close();
    }}
  ></div>
  <div
    bind:this={menu}
    role="menu"
    class="fixed z-50 min-w-56 rounded-lg border border-line bg-surface py-1 text-sm shadow-2xl"
    style:left="{pos.x}px"
    style:top="{pos.y}px"
  >
    {#each contextMenu.current.items as item, i (i)}
      {#if item}
        <button
          role="menuitem"
          class="flex w-full items-center gap-2.5 px-3 py-1.5 text-left outline-none hover:bg-raised focus:bg-raised"
          class:text-danger={item.danger}
          onclick={() => {
            contextMenu.close();
            item.action();
          }}
        >
          <span class="w-4 text-muted">
            {#if item.icon}<Icon name={item.icon} size={15} />{/if}
          </span>
          <span class="flex-1">{item.label}</span>
          {#if item.hint}<span class="text-xs text-muted">{item.hint}</span>{/if}
        </button>
      {:else}
        <div class="my-1 border-t border-line"></div>
      {/if}
    {/each}
  </div>
{/if}
