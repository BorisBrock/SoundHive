import { open } from "@tauri-apps/plugin-dialog";
import {
  commands,
  events,
  type AppError,
  type Folder_Serialize as Folder,
  type ScanProgress,
  type Settings_Serialize as RawSettings,
} from "./bindings";
import {
  bySimilarity,
  compareBy,
  emptyFilters,
  matches,
  normExt,
  parseQuery,
  prepareSound,
  RECENT_LIMIT,
  reindex,
  tagCounts,
  type Filters,
  type Sound,
  type SortKey,
} from "./sounds";
import { applyTheme } from "./theme";

export type { Folder, Sound };
export type Settings = Omit<RawSettings, "volume"> & { volume: number };

export function errorMessage(e: unknown): string {
  if (typeof e === "object" && e && "kind" in e) {
    const err = e as AppError;
    return err.kind === "not_found" ? "Not found." : err.message;
  }
  return String(e);
}

// ---- toasts -----------------------------------------------------------------------------------

export interface Toast {
  id: number;
  text: string;
  kind: "info" | "error";
}

class Toasts {
  list = $state<Toast[]>([]);
  private next = 1;

  show(text: string, kind: Toast["kind"] = "info") {
    const id = this.next++;
    this.list = [...this.list.slice(-3), { id, text, kind }];
    setTimeout(() => this.dismiss(id), kind === "error" ? 6000 : 2200);
  }

  dismiss(id: number) {
    this.list = this.list.filter((t) => t.id !== id);
  }
}

export const toasts = new Toasts();

/** Runs an action and shows its error as a toast. */
export async function attempt<T>(action: () => Promise<T>, success?: string): Promise<T | undefined> {
  try {
    const result = await action();
    if (success) toasts.show(success);
    return result;
  } catch (e) {
    toasts.show(errorMessage(e), "error");
    return undefined;
  }
}

// ---- library ----------------------------------------------------------------------------------

/** Global app state: the library, what is shown of it, and the selection. */
class Library {
  folders = $state<Folder[]>([]);
  /** All sounds; not deeply reactive (thousands of objects): changed sounds are replaced. */
  sounds = $state.raw<Sound[]>([]);
  loaded = $state(false);
  settings = $state<Settings>({
    theme: "midnight",
    editor: "",
    autoplay: true,
    volume: 0.8,
    scanOnStart: true,
    folderTags: false,
    tagRules: [],
    exportFolder: "",
  });
  scan = $state<ScanProgress | null>(null);
  scanning = $state(false);

  search = $state("");
  filters = $state<Filters>(emptyFilters());
  sort = $state<SortKey>("relevance");
  sortDesc = $state(false);
  /** "Find similar": the list is ordered by similarity to this sound. */
  similarTo = $state<number | null>(null);

  /** The sound shown in the detail panel; also the anchor for shift-click ranges. */
  focusedId = $state<number | null>(null);
  selected = $state.raw<Set<number>>(new Set());

  /** Sounds being dragged out of the window, so that dropping them back in is ignored. */
  draggingOut: number[] | null = null;

  settingsOpen = $state(false);
  helpOpen = $state(false);

  byId = $derived(new Map(this.sounds.map((s) => [s.id, s])));

  query = $derived(parseQuery(this.search));

  /** Sounds after sidebar filters and search, sorted. */
  visible = $derived.by(() => {
    const f = this.filters;
    const q = this.query;
    let list = this.sounds.filter((s) => matches(s, f, q));
    const target = this.similarTo !== null ? this.byId.get(this.similarTo) : undefined;
    if (target) {
      list.sort(bySimilarity(target));
    } else if (f.collection === "recent") {
      list.sort((a, b) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0));
      list = list.slice(0, RECENT_LIMIT);
    } else if (f.collection === "added") {
      list.sort((a, b) => b.added - a.added);
      list = list.slice(0, RECENT_LIMIT);
    } else {
      list.sort(compareBy(this.sort, q));
      if (this.sortDesc) list.reverse();
    }
    return list;
  });

  /** Tags of the visible sounds, for the sidebar. */
  visibleTags = $derived(tagCounts(this.visible));
  allTags = $derived(tagCounts(this.sounds));

  focused = $derived(this.focusedId !== null ? this.byId.get(this.focusedId) : undefined);
  /** Selected sounds in list order; the focused one if nothing else is selected. */
  selection = $derived.by(() => {
    const ids = this.selected.size ? this.selected : new Set(this.focusedId !== null ? [this.focusedId] : []);
    return [...ids].map((id) => this.byId.get(id)).filter((s): s is Sound => !!s);
  });
  selectionIds = $derived(this.selection.map((s) => s.id));

  exts = $derived.by(() => [...new Set(this.sounds.map((s) => normExt(s.ext)))].sort());

  private listening = false;
  private reloadTimer: ReturnType<typeof setTimeout> | undefined;

  async load() {
    if (!this.listening) {
      this.listening = true;
      await events.scanProgressed.listen((e) => {
        this.scan = e.payload;
        this.scanning = e.payload.phase !== "done" && e.payload.phase !== "cancelled";
        if (!this.scanning && e.payload.summary.startsWith("Scan failed")) {
          toasts.show(e.payload.summary, "error");
        }
      });
      await events.libraryChanged.listen(() => this.scheduleReload());
    }
    await attempt(async () => {
      const settings = await commands.getSettings();
      this.settings = { ...settings, volume: settings.volume ?? 0.8 };
      applyTheme(this.settings.theme);
      this.scanning = await commands.isScanning();
      await this.reload();
    });
    this.loaded = true;
  }

  scheduleReload() {
    clearTimeout(this.reloadTimer);
    this.reloadTimer = setTimeout(() => this.reload(), 150);
  }

  async reload() {
    const [folders, sounds] = await Promise.all([commands.listFolders(), commands.listSounds()]);
    this.folders = folders;
    this.sounds = sounds.map(prepareSound);
    const ids = new Set(this.sounds.map((s) => s.id));
    if (this.focusedId !== null && !ids.has(this.focusedId)) this.focusedId = null;
    if ([...this.selected].some((id) => !ids.has(id))) {
      this.selected = new Set([...this.selected].filter((id) => ids.has(id)));
    }
    if (this.filters.folder && !folders.some((f) => f.id === this.filters.folder?.id)) {
      this.filters.folder = null;
    }
  }

  async updateSettings(patch: Partial<Settings>) {
    const next = { ...this.settings, ...patch };
    this.settings = next;
    applyTheme(next.theme);
    await attempt(() => commands.saveSettings(next));
  }

  // ---- folders ----

  async addFolder(path?: string) {
    const picked =
      path ?? (await open({ directory: true, multiple: false, title: "Add a folder with sounds" }));
    if (typeof picked !== "string") return;
    await attempt(async () => {
      await commands.addFolder(picked);
      this.scanning = true;
      this.folders = await commands.listFolders();
    }, "Folder added; scanning…");
  }

  async rescan(folderIds: number[] | null = null) {
    await attempt(async () => {
      await commands.rescan(folderIds);
      this.scanning = true;
    });
  }

  // ---- selection ----

  /** Click on a row: plain, ctrl/cmd (toggle) or shift (range from the focused one). */
  select(id: number, mode: "replace" | "toggle" | "range" = "replace") {
    if (mode === "toggle") {
      const next = new Set(
        this.selected.size ? this.selected : this.focusedId !== null ? [this.focusedId] : [],
      );
      if (next.has(id)) next.delete(id);
      else next.add(id);
      this.selected = next;
      this.focusedId = id;
      return;
    }
    if (mode === "range" && this.focusedId !== null) {
      const ids = this.visible.map((s) => s.id);
      const a = ids.indexOf(this.focusedId);
      const b = ids.indexOf(id);
      if (a >= 0 && b >= 0) {
        this.selected = new Set(ids.slice(Math.min(a, b), Math.max(a, b) + 1));
        return;
      }
    }
    this.selected = new Set();
    this.focusedId = id;
  }

  selectAll() {
    this.selected = new Set(this.visible.map((s) => s.id));
  }

  /** Moves the focus by `delta` rows; returns the newly focused sound. */
  move(delta: number, extend = false): Sound | undefined {
    const list = this.visible;
    if (!list.length) return;
    const i = this.focusedId !== null ? list.findIndex((s) => s.id === this.focusedId) : -1;
    const j = Math.max(0, Math.min(list.length - 1, i < 0 ? 0 : i + delta));
    const s = list[j];
    if (extend) {
      const next = new Set(
        this.selected.size ? this.selected : this.focusedId !== null ? [this.focusedId] : [],
      );
      next.add(s.id);
      this.selected = next;
      this.focusedId = s.id;
    } else {
      this.select(s.id);
    }
    return s;
  }

  // ---- edits ----

  /** Applies a change to copies of the sounds, so that views showing them update. */
  private touch(ids: number[], change: (s: Sound) => void) {
    const set = new Set(ids);
    this.sounds = this.sounds.map((s) => {
      if (!set.has(s.id)) return s;
      const copy = { ...s };
      change(copy);
      return reindex(copy);
    });
  }

  async toggleFavorite(ids = this.selectionIds) {
    if (!ids.length) return;
    const favorite = !ids.every((id) => this.byId.get(id)?.favorite);
    await attempt(async () => {
      await commands.setFavorite(ids, favorite);
      this.touch(ids, (s) => (s.favorite = favorite));
    });
  }

  async addTag(tag: string, ids = this.selectionIds) {
    if (!ids.length || !tag.trim()) return;
    await attempt(async () => {
      const normalized = await commands.addTag(ids, tag);
      this.touch(ids, (s) => {
        if (!s.autoTags.includes(normalized) && !s.userTags.includes(normalized)) {
          s.userTags = [...s.userTags, normalized].sort();
        }
      });
    });
  }

  async removeTag(tag: string, ids = this.selectionIds) {
    await attempt(async () => {
      await commands.removeTag(ids, tag);
      this.touch(ids, (s) => {
        s.userTags = s.userTags.filter((t) => t !== tag);
        s.autoTags = s.autoTags.filter((t) => t !== tag);
      });
    });
  }

  async renameTag(from: string, to: string) {
    await attempt(async () => {
      await commands.renameTag(from, to);
      this.filters.tags = this.filters.tags.map((t) => (t === from ? to.trim().toLowerCase() : t));
      await this.reload();
    }, `Renamed “${from}”`);
  }

  toggleTagFilter(tag: string) {
    const tags = this.filters.tags;
    this.filters.tags = tags.includes(tag) ? tags.filter((t) => t !== tag) : [...tags, tag];
  }

  findSimilar(id = this.focusedId) {
    if (id === null) return;
    this.similarTo = id;
    this.search = "";
    this.filters = emptyFilters();
    this.focusedId = id;
    this.selected = new Set();
  }

  /** Back to the whole library. */
  resetView() {
    this.search = "";
    this.filters = emptyFilters();
    this.similarTo = null;
  }

  // ---- getting sounds out ----

  async copyPaths(ids = this.selectionIds) {
    const paths = ids.map((id) => this.byId.get(id)?.path).filter(Boolean);
    if (!paths.length) return;
    await attempt(
      () => navigator.clipboard.writeText(paths.join("\n")),
      paths.length > 1 ? `Copied ${paths.length} paths` : "Copied path",
    );
  }

  async copyFiles(ids = this.selectionIds) {
    if (!ids.length) return;
    await attempt(
      () => commands.copyFiles(ids),
      ids.length > 1
        ? `${ids.length} files on the clipboard — paste them anywhere`
        : "File on the clipboard — paste it anywhere",
    );
  }

  async openInEditor(ids = this.selectionIds) {
    if (!ids.length) return;
    await attempt(() => commands.openInEditor(ids));
  }

  async reveal(id = this.focusedId) {
    if (id !== null) await attempt(() => commands.revealSound(id));
  }

  async openDefault(id = this.focusedId) {
    if (id !== null) await attempt(() => commands.openDefault(id));
  }

  /** Copies the selection into the export folder, asking for one if none is set (or `pick`). */
  async copyToFolder(ids = this.selectionIds, pick = false) {
    if (!ids.length) return;
    let folder = this.settings.exportFolder;
    if (pick || !folder) {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "Copy sounds to…",
        defaultPath: folder || undefined,
      });
      if (typeof picked !== "string") return;
      folder = picked;
      await this.updateSettings({ exportFolder: folder });
    }
    const name = folder.split(/[\\/]/).filter(Boolean).pop() ?? folder;
    await attempt(
      () => commands.copyToFolder(ids, folder),
      ids.length > 1 ? `Copied ${ids.length} files to ${name}` : `Copied to ${name}`,
    );
  }
}

export const library = new Library();

if (import.meta.env.DEV) {
  (globalThis as unknown as { soundhive: Library }).soundhive = library;
}
