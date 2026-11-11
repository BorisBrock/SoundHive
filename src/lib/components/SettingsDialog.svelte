<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { commands, type TagRule } from "$lib/bindings";
  import { library } from "$lib/app.svelte";
  import { THEMES } from "$lib/theme";
  import Modal from "./Modal.svelte";

  const close = () => (library.settingsOpen = false);

  const rulesToText = (rules: TagRule[]) => rules.map((r) => `${r.tag}: ${r.keywords.join(", ")}`).join("\n");
  function textToRules(text: string): TagRule[] {
    return text
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line && !line.startsWith("#") && line.includes(":"))
      .map((line) => {
        const [tag, ...rest] = line.split(":");
        return {
          tag: tag.trim().toLowerCase(),
          keywords: rest
            .join(":")
            .split(",")
            .map((k) => k.trim().toLowerCase())
            .filter(Boolean),
        };
      })
      .filter((r) => r.tag && r.keywords.length);
  }

  let editor = $state(library.settings.editor);
  let exportFolder = $state(library.settings.exportFolder);
  let rules = $state(rulesToText(library.settings.tagRules));
  let editorName = $state<string | null>(null);
  let saving = $state(false);

  $effect(() => {
    const e = editor;
    commands
      .editorName(e)
      .then((n) => (editorName = n))
      .catch(() => (editorName = null));
  });

  async function saveRules() {
    const parsed = textToRules(rules);
    if (JSON.stringify(parsed) === JSON.stringify(library.settings.tagRules)) return;
    saving = true;
    await library.updateSettings({ tagRules: parsed });
    saving = false;
  }

  async function resetRules() {
    rules = rulesToText(await commands.defaultTagRules());
    await saveRules();
  }

  async function browseEditor() {
    const picked = await open({ title: "Choose the editor program", defaultPath: editor || undefined });
    if (typeof picked === "string") {
      editor = picked;
      await library.updateSettings({ editor });
    }
  }

  async function browseExport() {
    const picked = await open({
      directory: true,
      title: "Project folder",
      defaultPath: exportFolder || undefined,
    });
    if (typeof picked === "string") {
      exportFolder = picked;
      await library.updateSettings({ exportFolder });
    }
  }
</script>

{#snippet toggle(label: string, hint: string, checked: boolean, onchange: (v: boolean) => void)}
  <label class="flex items-start gap-3 rounded-lg bg-raised px-3 py-2">
    <input
      type="checkbox"
      class="mt-1 accent-(--wz-accent)"
      {checked}
      onchange={(e) => onchange(e.currentTarget.checked)}
    />
    <span>
      <span class="block text-sm">{label}</span>
      <span class="block text-xs text-muted">{hint}</span>
    </span>
  </label>
{/snippet}

<Modal title="Settings" onclose={close} wide>
  <div class="grid gap-8 lg:grid-cols-2">
    <div class="space-y-6">
      <section>
        <h3 class="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Theme</h3>
        <div class="grid grid-cols-4 gap-2">
          {#each THEMES as theme (theme.id)}
            {@const selected = library.settings.theme === theme.id}
            <!-- Each card sets data-theme on itself, so it previews that theme's colours. -->
            <button
              data-theme={theme.id}
              class={[
                "rounded-lg border-2 bg-(--wz-bg) p-1.5 text-left text-(--wz-fg) transition",
                selected ? "border-accent" : "border-transparent hover:border-line",
              ]}
              aria-pressed={selected}
              title={theme.label}
              onclick={() => library.updateSettings({ theme: theme.id })}
            >
              <div class="mb-1.5 flex h-8 gap-1">
                <span class="w-1/4 rounded-sm bg-(--wz-surface)"></span>
                <span class="flex flex-1 items-center gap-px rounded-sm bg-(--wz-surface) px-1">
                  {#each [3, 6, 9, 5, 8, 4, 7, 2] as h, i (i)}
                    <span class="w-1 rounded-full bg-(--wz-accent)" style:height="{h * 2.5}px"></span>
                  {/each}
                </span>
              </div>
              <span class="block truncate text-xs font-medium">{theme.label}</span>
            </button>
          {/each}
        </div>
      </section>

      <section class="space-y-2">
        <h3 class="text-xs font-semibold tracking-wider text-muted uppercase">Browsing</h3>
        {@render toggle(
          "Auto-play",
          "Play a sound as soon as it is selected, also with the arrow keys.",
          library.settings.autoplay,
          (v) => library.updateSettings({ autoplay: v }),
        )}
        {@render toggle(
          "Rescan on start",
          "Look for new and changed files when SoundHive starts. Only changed files are analysed again.",
          library.settings.scanOnStart,
          (v) => library.updateSettings({ scanOnStart: v }),
        )}
      </section>

      <section class="space-y-3">
        <h3 class="text-xs font-semibold tracking-wider text-muted uppercase">Other apps</h3>
        <label class="block">
          <span class="mb-1 block text-xs font-medium text-muted">Sound editor (“Open in editor”, E)</span>
          <div class="flex gap-2">
            <input
              class="field min-w-0 flex-1 font-mono text-sm"
              bind:value={editor}
              onblur={() =>
                editor !== library.settings.editor && library.updateSettings({ editor: editor.trim() })}
              placeholder="Audacity, if installed"
            />
            <button
              type="button"
              class="rounded-md border border-line px-3 hover:bg-raised"
              onclick={browseEditor}
            >
              Browse…
            </button>
          </div>
          <span class="mt-1 block text-xs text-muted">
            {#if editorName}
              Opens sounds in <strong class="text-fg">{editorName}</strong>.
            {:else}
              Audacity was not found. Choose the program, or enter a command such as
              <code class="font-mono">flatpak run org.audacityteam.Audacity</code>.
            {/if}
          </span>
        </label>
        <label class="block">
          <span class="mb-1 block text-xs font-medium text-muted">Project folder (“Copy to folder”, P)</span>
          <div class="flex gap-2">
            <input
              class="field min-w-0 flex-1 font-mono text-sm"
              bind:value={exportFolder}
              onblur={() =>
                exportFolder !== library.settings.exportFolder &&
                library.updateSettings({ exportFolder: exportFolder.trim() })}
              placeholder="Asked the first time"
            />
            <button
              type="button"
              class="rounded-md border border-line px-3 hover:bg-raised"
              onclick={browseExport}
            >
              Browse…
            </button>
          </div>
          <span class="mt-1 block text-xs text-muted"
            >E.g. your game's <code class="font-mono">assets/sfx</code>. Existing files are never overwritten.</span
          >
        </label>
      </section>
    </div>

    <section class="flex min-h-0 flex-col space-y-2">
      <h3 class="text-xs font-semibold tracking-wider text-muted uppercase">Automatic tags</h3>
      {@render toggle(
        "Tag with folder names",
        "Every sound also gets the name of the folder it is in as a tag.",
        library.settings.folderTags,
        (v) => library.updateSettings({ folderTags: v }),
      )}
      <p class="text-xs leading-relaxed text-muted">
        One rule per line: <code class="font-mono">tag: keyword, keyword, …</code>. A sound gets the tag when
        its file name or a folder above it contains a keyword. Changes apply to the whole library when you
        leave the field — no rescan needed. <em>loop</em>, <em>one-shot</em> and <em>long</em> come from the sound
        analysis.
      </p>
      <textarea
        class="field min-h-80 flex-1 resize-none font-mono text-xs leading-relaxed"
        spellcheck="false"
        bind:value={rules}
        onblur={saveRules}></textarea>
      <div class="flex items-center gap-2">
        <button class="chip" onclick={resetRules}>Reset to defaults</button>
        {#if saving}<span class="text-xs text-muted">Re-tagging…</span>{/if}
      </div>
    </section>
  </div>
</Modal>
