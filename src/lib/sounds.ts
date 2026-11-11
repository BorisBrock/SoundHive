/**
 * Pure logic on the sound list: search queries, filters, sorting and similarity. Kept free of
 * Svelte and Tauri so that it can be unit-tested and stays fast on large libraries.
 */
import type { Sound_Serialize } from "./bindings";

/** Numbers that Rust sends as `f64` arrive as `number | null` (NaN); here they are plain numbers. */
type Measured = "duration" | "peakDb" | "rmsDb" | "zcr" | "attack";
export type Sound = Omit<Sound_Serialize, Measured> & { [K in Measured]: number } & {
  /** Auto and user tags together, sorted. */
  tags: string[];
  /** Lower-case text that free-text search looks in. */
  haystack: string;
};

export function prepareSound(raw: Sound_Serialize): Sound {
  const s = raw as unknown as Sound;
  for (const k of ["duration", "peakDb", "rmsDb", "zcr", "attack"] as const) s[k] = s[k] ?? 0;
  return reindex(s);
}

/** Recomputes the derived fields after tags change. */
export function reindex(s: Sound): Sound {
  s.tags = [...new Set([...s.autoTags, ...s.userTags])].sort();
  s.haystack = `${s.name}\n${s.dir}\n${s.description}\n${s.tags.join("\n")}\n${s.ext}`.toLowerCase();
  return s;
}

// ---- query ------------------------------------------------------------------------------------

/** A parsed search box query. */
export interface Query {
  /** Words (or quoted phrases) that must all occur somewhere. */
  words: string[];
  /** Words that must not occur. */
  without: string[];
  tags: string[];
  exts: string[];
  minDuration?: number;
  maxDuration?: number;
  loop?: boolean;
  favorite?: boolean;
}

/** Parses e.g. `door "wood creak" -metal tag:foley ext:wav dur:<2 is:loop`. */
export function parseQuery(text: string): Query {
  const q: Query = { words: [], without: [], tags: [], exts: [] };
  const tokens = text.toLowerCase().match(/-?"[^"]*"?|\S+/g) ?? [];
  for (let token of tokens) {
    const negative = token.startsWith("-") && token.length > 1;
    if (negative) token = token.slice(1);
    token = token.replace(/^"|"$/g, "");
    if (!token) continue;
    const [key, ...rest] = token.split(":");
    const value = rest.join(":");
    if (rest.length && value) {
      if (key === "tag" || key === "t") {
        if (negative) q.without.push(value.replace(/_/g, " "));
        else q.tags.push(value.replace(/_/g, " "));
        continue;
      }
      if (key === "ext" || key === "type") {
        q.exts.push(...value.split(","));
        continue;
      }
      if (key === "is") {
        if (value === "loop") q.loop = !negative;
        else if (value === "oneshot" || value === "one-shot") q.loop = negative;
        else if (value === "fav" || value === "favorite" || value === "favourite") q.favorite = !negative;
        continue;
      }
      if (key === "dur" || key === "duration" || key === "len") {
        const range = parseDuration(value);
        if (range) {
          if (range[0] !== undefined) q.minDuration = range[0];
          if (range[1] !== undefined) q.maxDuration = range[1];
          continue;
        }
      }
    }
    (negative ? q.without : q.words).push(token);
  }
  return q;
}

/** `<2`, `>10`, `1-3`, `1.5s`, `500ms`, `2m` → [min, max] in seconds. */
function parseDuration(text: string): [number | undefined, number | undefined] | null {
  const num = (t: string) => {
    const m = t.trim().match(/^(\d*\.?\d+)\s*(ms|s|m|min)?$/);
    if (!m) return NaN;
    const n = parseFloat(m[1]);
    return m[2] === "ms" ? n / 1000 : m[2] === "m" || m[2] === "min" ? n * 60 : n;
  };
  let m;
  if ((m = text.match(/^<=?(.+)$/))) return isNaN(num(m[1])) ? null : [undefined, num(m[1])];
  if ((m = text.match(/^>=?(.+)$/))) return isNaN(num(m[1])) ? null : [num(m[1]), undefined];
  if ((m = text.match(/^(.+?)-(.+)$/))) {
    const [a, b] = [num(m[1]), num(m[2])];
    return isNaN(a) || isNaN(b) ? null : [a, b];
  }
  const n = num(text);
  // A single value: about that long.
  return isNaN(n) ? null : [n * 0.8, n * 1.2];
}

// ---- filters ----------------------------------------------------------------------------------

/** Filters set with the sidebar and the filter bar (the search box adds a `Query`). */
export interface Filters {
  collection: Collection;
  /** Library folder, and optionally a folder inside it (`dir` prefix). */
  folder: { id: number; dir: string } | null;
  /** All must be present. */
  tags: string[];
  /** Duration range in seconds. */
  duration: [number, number] | null;
  /** Empty: all. */
  exts: string[];
  /** `mono`, `stereo` or null. */
  channels: "mono" | "stereo" | null;
  loop: "loop" | "oneshot" | null;
}

export type Collection = "all" | "favorites" | "loops" | "recent" | "added" | "unreadable";

export const emptyFilters = (): Filters => ({
  collection: "all",
  folder: null,
  tags: [],
  duration: null,
  exts: [],
  channels: null,
  loop: null,
});

/** How many sounds "Recently played" and "Recently added" show at most. */
export const RECENT_LIMIT = 200;

export function matches(s: Sound, f: Filters, q: Query): boolean {
  switch (f.collection) {
    case "favorites":
      if (!s.favorite) return false;
      break;
    case "loops":
      if (!s.loopKind) return false;
      break;
    case "recent":
      if (!s.lastPlayed) return false;
      break;
    case "unreadable":
      if (!s.error) return false;
      break;
  }
  if (f.folder) {
    if (s.folderId !== f.folder.id) return false;
    const d = f.folder.dir;
    if (d && s.dir !== d && !s.dir.startsWith(d + "/")) return false;
  }
  for (const t of f.tags) if (!s.tags.includes(t)) return false;
  if (f.duration && (s.duration < f.duration[0] || s.duration > f.duration[1])) return false;
  if (f.exts.length && !f.exts.includes(normExt(s.ext))) return false;
  if (f.channels === "mono" && s.channels !== 1) return false;
  if (f.channels === "stereo" && s.channels < 2) return false;
  if (f.loop === "loop" && !s.loopKind) return false;
  if (f.loop === "oneshot" && s.loopKind) return false;

  for (const t of q.tags) if (!s.tags.some((x) => x === t || x.startsWith(t))) return false;
  if (q.exts.length && !q.exts.some((e) => normExt(s.ext) === normExt(e))) return false;
  if (q.minDuration !== undefined && s.duration < q.minDuration) return false;
  if (q.maxDuration !== undefined && s.duration > q.maxDuration) return false;
  if (q.loop !== undefined && !!s.loopKind !== q.loop) return false;
  if (q.favorite !== undefined && s.favorite !== q.favorite) return false;
  for (const w of q.words) if (!s.haystack.includes(w)) return false;
  for (const w of q.without) if (s.haystack.includes(w)) return false;
  return true;
}

/** File types that are the same format. */
export function normExt(ext: string) {
  const e = ext.toLowerCase();
  return e === "wave" ? "wav" : e === "aif" ? "aiff" : e === "oga" ? "ogg" : e;
}

// ---- sorting ----------------------------------------------------------------------------------

export type SortKey =
  | "relevance"
  | "name"
  | "duration"
  | "folder"
  | "modified"
  | "added"
  | "size"
  | "loudness"
  | "brightness"
  | "plays";

export const SORTS: { key: SortKey; label: string }[] = [
  { key: "relevance", label: "Best match" },
  { key: "name", label: "Name" },
  { key: "duration", label: "Duration" },
  { key: "folder", label: "Folder" },
  { key: "loudness", label: "Loudness" },
  { key: "brightness", label: "Brightness" },
  { key: "modified", label: "Date modified" },
  { key: "added", label: "Date added" },
  { key: "size", label: "File size" },
  { key: "plays", label: "Times played" },
];

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** How well a sound matches the search words; higher is better. Name hits beat folder hits. */
export function relevance(s: Sound, q: Query): number {
  const name = s.name.toLowerCase();
  let score = 0;
  for (const w of q.words) {
    if (name === w) score += 10;
    else if (new RegExp(`(^|[^a-z0-9])${escapeRegExp(w)}`).test(name)) score += 6;
    else if (name.includes(w)) score += 4;
    else if (s.tags.some((t) => t === w)) score += 3;
    else if (s.dir.toLowerCase().includes(w)) score += 1;
  }
  if (s.favorite) score += 0.5;
  return score;
}

function escapeRegExp(s: string) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export function compareBy(key: SortKey, q: Query): (a: Sound, b: Sound) => number {
  const byName = (a: Sound, b: Sound) => collator.compare(a.name, b.name);
  switch (key) {
    case "relevance": {
      if (!q.words.length) return byName;
      const scores = new Map<number, number>();
      const score = (s: Sound) => {
        let v = scores.get(s.id);
        if (v === undefined) scores.set(s.id, (v = relevance(s, q)));
        return v;
      };
      return (a, b) => score(b) - score(a) || byName(a, b);
    }
    case "name":
      return byName;
    case "duration":
      return (a, b) => a.duration - b.duration || byName(a, b);
    case "folder":
      return (a, b) => collator.compare(a.dir, b.dir) || byName(a, b);
    case "modified":
      return (a, b) => b.modified - a.modified || byName(a, b);
    case "added":
      return (a, b) => b.added - a.added || byName(a, b);
    case "size":
      return (a, b) => a.size - b.size || byName(a, b);
    case "loudness":
      return (a, b) => b.rmsDb - a.rmsDb || byName(a, b);
    case "brightness":
      return (a, b) => b.zcr - a.zcr || byName(a, b);
    case "plays":
      return (a, b) => b.playCount - a.playCount || (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0) || byName(a, b);
  }
}

// ---- similarity -------------------------------------------------------------------------------

/** Features compared by "Find similar", scaled to roughly comparable ranges. */
function features(s: Sound): number[] {
  return [
    Math.log2(Math.max(s.duration, 0.01)) * 0.8,
    s.rmsDb / 8,
    (s.peakDb - s.rmsDb) / 6, // crest factor: punchy vs. dense
    Math.log2(Math.max(s.zcr, 10)) * 1.2, // brightness
    Math.log2(Math.max(s.attack, 0.001)) * 0.4,
    s.loopKind ? 1.5 : 0,
  ];
}

/** Distance between two sounds: their sound features plus how many tags they share. */
export function distance(a: Sound, b: Sound, fa = features(a)): number {
  const fb = features(b);
  let d = 0;
  for (let i = 0; i < fa.length; i++) d += (fa[i] - fb[i]) ** 2;
  d = Math.sqrt(d);
  const union = new Set([...a.tags, ...b.tags]).size;
  const shared = a.tags.filter((t) => b.tags.includes(t)).length;
  const tagSimilarity = union ? shared / union : 0;
  return d * (1.2 - tagSimilarity);
}

/** Sorter that puts the sounds most similar to `target` first. */
export function bySimilarity(target: Sound): (a: Sound, b: Sound) => number {
  const ft = features(target);
  const cache = new Map<number, number>();
  const dist = (s: Sound) => {
    let d = cache.get(s.id);
    if (d === undefined) cache.set(s.id, (d = s.id === target.id ? -1 : distance(target, s, ft)));
    return d;
  };
  return (a, b) => dist(a) - dist(b);
}

// ---- tags facet -------------------------------------------------------------------------------

/** Tag counts over the given sounds, most common first. */
export function tagCounts(sounds: Iterable<Sound>): { tag: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const s of sounds) for (const t of s.tags) counts.set(t, (counts.get(t) ?? 0) + 1);
  return [...counts]
    .map(([tag, count]) => ({ tag, count }))
    .sort((a, b) => b.count - a.count || collator.compare(a.tag, b.tag));
}

// ---- folder tree ------------------------------------------------------------------------------

export interface DirNode {
  name: string;
  /** Path relative to the library folder. */
  dir: string;
  count: number;
  children: DirNode[];
}

/** Folder tree of one library folder, with the number of sounds below each folder. */
export function dirTree(sounds: Iterable<Sound>, folderId: number): DirNode[] {
  const root: DirNode = { name: "", dir: "", count: 0, children: [] };
  const index = new Map<string, DirNode>([["", root]]);
  for (const s of sounds) {
    if (s.folderId !== folderId) continue;
    root.count++;
    if (!s.dir) continue;
    let path = "";
    let parent = root;
    for (const part of s.dir.split("/")) {
      path = path ? `${path}/${part}` : part;
      let node = index.get(path);
      if (!node) {
        node = { name: part, dir: path, count: 0, children: [] };
        index.set(path, node);
        parent.children.push(node);
      }
      node.count++;
      parent = node;
    }
  }
  const sort = (nodes: DirNode[]) => {
    nodes.sort((a, b) => collator.compare(a.name, b.name));
    nodes.forEach((n) => sort(n.children));
  };
  sort(root.children);
  return root.children;
}

// ---- formatting -------------------------------------------------------------------------------

export function formatDuration(seconds: number): string {
  if (!seconds || seconds < 0) return "–";
  if (seconds < 10) return `${seconds.toFixed(2)}s`;
  if (seconds < 60) return `${seconds.toFixed(1)}s`;
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
}

/** Playback clock: `0:03.25`. */
export function formatClock(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds - m * 60;
  return `${m}:${s.toFixed(2).padStart(5, "0")}`;
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

export function formatRate(hz: number): string {
  return hz ? `${(hz / 1000).toFixed(hz % 1000 ? 1 : 0)} kHz` : "–";
}

export function formatChannels(n: number): string {
  return n === 1 ? "Mono" : n === 2 ? "Stereo" : n ? `${n} ch` : "–";
}

export function formatDate(unix: number | null | undefined): string {
  return unix
    ? new Date(unix * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })
    : "–";
}

export const LOOP_LABELS: Record<string, string> = {
  marked: "Loop points in the file",
  name: "Named as a loop",
  seamless: "Seamless: the end flows into the start",
};

// ---- target projects --------------------------------------------------------------------------

/** Words in project file names that say nothing about the sound. */
const NOISE_WORDS = new Set([
  "sfx",
  "snd",
  "sound",
  "sounds",
  "fx",
  "se",
  "audio",
  "placeholder",
  "temp",
  "tmp",
  "test",
  "dummy",
  "new",
  "copy",
  "final",
  "wip",
]);

/** A search for a project file's name: `SFX_DoorOpen_02` → `door open`. */
export function searchFromName(name: string): string {
  return name
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .replace(/([A-Za-z]{2,})(\d)/g, "$1 $2")
    .toLowerCase()
    .split(/[^\p{L}\p{N}]+/u)
    .filter((w) => w && !NOISE_WORDS.has(w) && !/^\d+[a-z]?$/.test(w) && !/^v\d+$/.test(w))
    .join(" ");
}
