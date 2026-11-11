import { library, type Sound } from "./app.svelte";
import { player } from "./player.svelte";
import type { MenuItem } from "./contextMenu.svelte";

const MOD = navigator.platform.toLowerCase().includes("mac") ? "⌘" : "Ctrl+";

/** Actions on the selection, shared by the context menu and the detail panel. */
export function soundMenu(s: Sound): (MenuItem | null)[] {
  const many = library.selectionIds.length > 1;
  const n = library.selectionIds.length;
  return [
    {
      label: player.playing && player.currentId === s.id ? "Stop" : "Play",
      icon: player.playing && player.currentId === s.id ? "stop" : "play",
      hint: "Space",
      action: () => player.toggle(s),
    },
    {
      label: s.favorite ? "Remove from favourites" : "Add to favourites",
      icon: "star",
      hint: "F",
      action: () => library.toggleFavorite(),
    },
    { label: "Find similar sounds", icon: "sparkles", hint: "S", action: () => library.findSimilar(s.id) },
    null,
    {
      label: many ? `Copy ${n} files` : "Copy file",
      icon: "clipboard",
      hint: `${MOD}C`,
      action: () => library.copyFiles(),
    },
    {
      label: many ? `Copy ${n} paths` : "Copy path",
      icon: "copy",
      hint: `${MOD}⇧C`,
      action: () => library.copyPaths(),
    },
    {
      label: many ? `Open ${n} in editor` : "Open in editor",
      icon: "external-link",
      hint: "E",
      action: () => library.openInEditor(),
    },
    {
      label: library.settings.exportFolder
        ? `Copy to ${library.settings.exportFolder.split(/[\\/]/).filter(Boolean).pop()}`
        : "Copy to folder…",
      icon: "folder-input",
      hint: "P",
      action: () => library.copyToFolder(),
    },
    ...(library.settings.exportFolder
      ? [
          {
            label: "Copy to other folder…",
            icon: "folder-input" as const,
            action: () => library.copyToFolder(undefined, true),
          },
        ]
      : []),
    null,
    { label: "Show in folder", icon: "folder-open", hint: "O", action: () => library.reveal(s.id) },
    { label: "Open with default app", icon: "file-audio", action: () => library.openDefault(s.id) },
  ];
}

export { MOD };
