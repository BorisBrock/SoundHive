<script lang="ts">
  import { toasts } from "$lib/app.svelte";
  import Icon from "./Icon.svelte";
</script>

<div
  class="pointer-events-none fixed bottom-12 left-1/2 z-50 flex -translate-x-1/2 flex-col items-center gap-2"
>
  {#each toasts.list as t (t.id)}
    <div
      class="pointer-events-auto flex max-w-[70vw] items-center gap-2 rounded-lg border bg-surface px-3 py-2 text-sm shadow-xl"
      class:border-danger={t.kind === "error"}
      class:border-line={t.kind !== "error"}
      role="status"
    >
      <span class:text-danger={t.kind === "error"} class:text-accent={t.kind !== "error"}>
        <Icon name={t.kind === "error" ? "circle-x" : "check"} size={15} />
      </span>
      <span class="select-text">{t.text}</span>
      <button class="ml-1 text-muted hover:text-fg" aria-label="Dismiss" onclick={() => toasts.dismiss(t.id)}>
        <Icon name="x" size={13} />
      </button>
    </div>
  {/each}
</div>
