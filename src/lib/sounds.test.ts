import { describe, expect, it } from "vitest";
import type { Sound_Serialize } from "./bindings";
import {
  bySimilarity,
  compareBy,
  dirTree,
  emptyFilters,
  formatDuration,
  matches,
  parseQuery,
  prepareSound,
  searchFromName,
  tagCounts,
} from "./sounds";

let nextId = 1;
function sound(p: Partial<Sound_Serialize>) {
  return prepareSound({
    id: nextId++,
    folderId: 1,
    path: "/x",
    dir: "",
    name: "x",
    ext: "wav",
    size: 1,
    modified: 0,
    duration: 1,
    sampleRate: 44100,
    channels: 1,
    bits: 16,
    peakDb: -1,
    rmsDb: -12,
    zcr: 1000,
    attack: 0.01,
    loopKind: null,
    description: "",
    favorite: false,
    playCount: 0,
    lastPlayed: null,
    added: 0,
    autoTags: [],
    userTags: [],
    error: null,
    ...p,
  });
}

describe("parseQuery", () => {
  it("splits words, phrases, exclusions and operators", () => {
    const q = parseQuery('Door "wood creak" -metal tag:foley ext:wav,ogg dur:<2 is:loop is:fav');
    expect(q.words).toEqual(["door", "wood creak"]);
    expect(q.without).toEqual(["metal"]);
    expect(q.tags).toEqual(["foley"]);
    expect(q.exts).toEqual(["wav", "ogg"]);
    expect(q.maxDuration).toBe(2);
    expect(q.loop).toBe(true);
    expect(q.favorite).toBe(true);
  });

  it("reads duration ranges with units", () => {
    expect(parseQuery("dur:500ms-2s")).toMatchObject({ minDuration: 0.5, maxDuration: 2 });
    expect(parseQuery("dur:>1m").minDuration).toBe(60);
    expect(parseQuery("-is:loop").loop).toBe(false);
    // Not a duration: searched as text.
    expect(parseQuery("dur:abc").words).toEqual(["dur:abc"]);
  });
});

describe("matches", () => {
  const door = sound({
    name: "Door_Slam_01",
    dir: "Foley/Doors",
    autoTags: ["door", "impact"],
    duration: 0.8,
  });
  const rain = sound({ name: "Rain Loop", ext: "ogg", loopKind: "name", duration: 30, channels: 2 });
  const all = (text: string, f = emptyFilters()) =>
    [door, rain].filter((s) => matches(s, f, parseQuery(text))).map((s) => s.name);

  it("searches names, folders and tags", () => {
    expect(all("slam")).toEqual(["Door_Slam_01"]);
    expect(all("foley")).toEqual(["Door_Slam_01"]);
    expect(all("impact")).toEqual(["Door_Slam_01"]);
    expect(all("-door")).toEqual(["Rain Loop"]);
    expect(all("is:loop")).toEqual(["Rain Loop"]);
    expect(all("dur:<1")).toEqual(["Door_Slam_01"]);
    expect(all("ext:ogg")).toEqual(["Rain Loop"]);
  });

  it("applies sidebar and filter bar filters", () => {
    expect(all("", { ...emptyFilters(), tags: ["door"] })).toEqual(["Door_Slam_01"]);
    expect(all("", { ...emptyFilters(), channels: "stereo" })).toEqual(["Rain Loop"]);
    expect(all("", { ...emptyFilters(), loop: "oneshot" })).toEqual(["Door_Slam_01"]);
    expect(all("", { ...emptyFilters(), folder: { id: 1, dir: "Foley" } })).toEqual(["Door_Slam_01"]);
    expect(all("", { ...emptyFilters(), folder: { id: 1, dir: "Fol" } })).toEqual([]);
    expect(all("", { ...emptyFilters(), collection: "loops" })).toEqual(["Rain Loop"]);
  });
});

describe("sorting", () => {
  it("ranks name hits above folder hits", () => {
    const a = sound({ name: "ambience", dir: "door stuff" });
    const b = sound({ name: "wooden door" });
    const c = sound({ name: "door" });
    const q = parseQuery("door");
    expect([a, b, c].sort(compareBy("relevance", q)).map((s) => s.name)).toEqual([
      "door",
      "wooden door",
      "ambience",
    ]);
  });

  it("sorts names naturally", () => {
    const list = ["hit 10", "hit 2", "Hit 1"].map((name) => sound({ name }));
    expect(list.sort(compareBy("name", parseQuery(""))).map((s) => s.name)).toEqual([
      "Hit 1",
      "hit 2",
      "hit 10",
    ]);
  });

  it("finds similar sounds", () => {
    const hit = sound({ name: "hit", duration: 0.3, attack: 0.001, autoTags: ["impact"] });
    const hit2 = sound({ name: "hit2", duration: 0.35, attack: 0.002, autoTags: ["impact"] });
    const drone = sound({
      name: "drone",
      duration: 60,
      attack: 2,
      rmsDb: -20,
      zcr: 100,
      loopKind: "seamless",
    });
    expect([drone, hit2, hit].sort(bySimilarity(hit)).map((s) => s.name)).toEqual(["hit", "hit2", "drone"]);
  });
});

describe("facets", () => {
  it("counts tags and builds the folder tree", () => {
    const list = [
      sound({ dir: "A/B", autoTags: ["x"] }),
      sound({ dir: "A", autoTags: ["x", "y"] }),
      sound({ dir: "" }),
    ];
    expect(tagCounts(list)).toEqual([
      { tag: "x", count: 2 },
      { tag: "y", count: 1 },
    ]);
    const tree = dirTree(list, 1);
    expect(tree).toHaveLength(1);
    expect(tree[0]).toMatchObject({ name: "A", count: 2 });
    expect(tree[0].children[0]).toMatchObject({ name: "B", dir: "A/B", count: 1 });
  });

  it("formats durations", () => {
    expect(formatDuration(0.5)).toBe("0.50s");
    expect(formatDuration(12.34)).toBe("12.3s");
    expect(formatDuration(125)).toBe("2:05");
  });
});

describe("searchFromName", () => {
  it("turns project file names into search words", () => {
    expect(searchFromName("door_open")).toBe("door open");
    expect(searchFromName("SFX_DoorOpen_02")).toBe("door open");
    expect(searchFromName("snd-laser-shot-v2")).toBe("laser shot");
    expect(searchFromName("explosion3")).toBe("explosion");
    expect(searchFromName("ui click (copy)")).toBe("ui click");
    expect(searchFromName("01")).toBe("");
  });
});
