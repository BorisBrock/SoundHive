import { open } from "@tauri-apps/plugin-dialog";
import { commands, type Project, type Slot_Serialize } from "./bindings";
import { attempt, errorMessage, library, toasts } from "./app.svelte";
import { confirm } from "./confirm.svelte";
import { normExt, searchFromName } from "./sounds";

export type { Project };
/** Whether a library sound of type `ext` can go into the slot (as is, or converted to WAV). */
export function fitsSlot(slot: Slot, ext: string): boolean {
  const target = normExt(slot.ext);
  return target === "wav" || target === normExt(ext);
}

/** A sound file of the project; `duration` is a plain number (Rust's f64 may arrive as null). */
export type Slot = Omit<Slot_Serialize, "duration"> & { duration: number };

function prepare(raw: Slot_Serialize): Slot {
  return { ...raw, duration: raw.duration ?? 0 };
}

const ACTIVE_KEY = "activeProject";
const slotKey = (projectId: number) => `activeSlot:${projectId}`;

/**
 * Target projects: the sound files ("slots") of a game, a video or any other project are
 * replaced one by one with library sounds, tried out in the project and approved.
 */
class Projects {
  list = $state<Project[]>([]);
  /** The project whose panel is open. */
  activeId = $state<number | null>(Number(localStorage.getItem(ACTIVE_KEY)) || null);
  slots = $state.raw<Slot[]>([]);
  /** The slot that replacements go into. */
  slotId = $state<number | null>(null);
  /** Hide the slots that are done. */
  hideDone = $state(localStorage.getItem("hideDoneSlots") === "1");
  busy = $state(false);

  active = $derived(this.list.find((p) => p.id === this.activeId));
  slot = $derived(this.slots.find((s) => s.id === this.slotId));
  doneCount = $derived(this.slots.filter((s) => s.status === "approved").length);
  shownSlots = $derived(
    this.hideDone ? this.slots.filter((s) => s.status !== "approved" || s.id === this.slotId) : this.slots,
  );

  /** Library files tried in the active slot: `current` is the one in the project now. */
  tried = $derived.by(() => {
    const m = new Map<string, "current" | "tried">();
    const s = this.slot;
    if (!s) return m;
    for (const t of s.tries) m.set(t.sourcePath, "tried");
    if (s.sourcePath) m.set(s.sourcePath, "current");
    return m;
  });

  async load() {
    await attempt(async () => {
      this.list = await commands.listProjects();
      if (this.activeId !== null && !this.list.some((p) => p.id === this.activeId)) this.setActive(null);
      if (this.activeId !== null) await this.refresh(true);
    });
  }

  private setActive(id: number | null) {
    this.activeId = id;
    if (id === null) localStorage.removeItem(ACTIVE_KEY);
    else localStorage.setItem(ACTIVE_KEY, String(id));
  }

  async open(id: number) {
    if (id === this.activeId) return;
    this.setActive(id);
    this.slots = [];
    this.slotId = null;
    await this.refresh();
  }

  close() {
    this.setActive(null);
    this.slots = [];
    this.slotId = null;
  }

  async add() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: "Choose the sound folder of your game, video or other project",
    });
    if (typeof picked !== "string") return;
    const id = await attempt(() => commands.addProject(picked));
    if (id === undefined) return;
    this.list = await commands.listProjects();
    this.setActive(null);
    await this.open(id);
    toasts.show(
      this.slots.length
        ? `${this.slots.length} sound files found. Pick one, find a sound for it and press R.`
        : "No sound files found in this folder.",
    );
  }

  async remove(p: Project) {
    const ok = await confirm({
      title: "Remove project?",
      message: `${p.path}\n\nSoundHive forgets its progress and the backups of the original sounds. The project's files stay as they are now.`,
      okLabel: "Remove",
      danger: true,
    });
    if (!ok) return;
    await attempt(async () => {
      await commands.removeProject(p.id);
      if (p.id === this.activeId) this.close();
      this.list = await commands.listProjects();
    }, "Project removed");
  }

  /** Picks up files added, changed or removed outside SoundHive. */
  async refresh(quiet = false) {
    const id = this.activeId;
    if (id === null) return;
    try {
      const slots = (await commands.refreshProject(id)).map(prepare);
      if (id !== this.activeId) return;
      this.slots = slots;
      if (this.slotId === null) {
        const saved = Number(localStorage.getItem(slotKey(id)));
        if (slots.some((s) => s.id === saved)) this.slotId = saved;
      } else if (!slots.some((s) => s.id === this.slotId)) {
        this.slotId = null;
      }
      this.list = await commands.listProjects();
    } catch (e) {
      if (!quiet) toasts.show(errorMessage(e), "error");
    }
  }

  /** Makes a slot the one to replace; searches the library for its name unless `search` is off. */
  selectSlot(id: number, search = true) {
    if (this.activeId === null) return;
    const changed = id !== this.slotId;
    this.slotId = id;
    localStorage.setItem(slotKey(this.activeId), String(id));
    const s = this.slot;
    if (s && search && changed) {
      library.resetView();
      library.search = searchFromName(s.name);
      // Only WAV slots take any format (converted); others need a sound of their own format.
      const ext = normExt(s.ext);
      if (ext !== "wav") library.filters.exts = [ext];
    }
  }

  /** The next slot after the active one that is not done yet, wrapping around. */
  nextOpen(): Slot | undefined {
    const list = this.slots;
    const i = list.findIndex((s) => s.id === this.slotId);
    for (let k = 1; k <= list.length; k++) {
      const s = list[(i + k + list.length) % list.length];
      if (s.status !== "approved" && s.id !== this.slotId) return s;
    }
  }

  /** Moves to the previous / next slot in the list. */
  step(delta: number) {
    const list = this.shownSlots;
    if (!list.length) return;
    const i = list.findIndex((s) => s.id === this.slotId);
    const j = i < 0 ? 0 : Math.max(0, Math.min(list.length - 1, i + delta));
    this.selectSlot(list[j].id);
  }

  setHideDone(hide: boolean) {
    this.hideDone = hide;
    localStorage.setItem("hideDoneSlots", hide ? "1" : "0");
  }

  private update(slot: Slot_Serialize) {
    const s = prepare(slot);
    this.slots = this.slots.map((x) => (x.id === s.id ? s : x));
    const done = this.slots.filter((x) => x.status === "approved").length;
    this.list = this.list.map((p) => (p.id === s.projectId ? { ...p, approvedCount: done } : p));
  }

  private async run(action: () => Promise<Slot_Serialize>, success?: string) {
    if (this.busy) return undefined;
    this.busy = true;
    try {
      const slot = await attempt(action, success);
      if (slot) this.update(slot);
      return slot;
    } finally {
      this.busy = false;
    }
  }

  /** Puts the focused library sound into the active slot, to try it in the project. */
  async replace(soundId = library.focusedId) {
    const s = this.slot;
    if (!s) {
      toasts.show("First pick the project's sound file to replace, in the project panel.", "error");
      return;
    }
    const sound = soundId !== null ? library.byId.get(soundId) : undefined;
    if (!sound) return;
    await this.run(
      () => commands.replaceSlot(s.id, sound.id),
      `${s.name} is now “${sound.name}”. Try it in your project!`,
    );
  }

  /** Marks the active slot as done and moves on to the next open one; or opens it again. */
  async toggleApproved() {
    const s = this.slot;
    if (!s) return;
    const approve = s.status !== "approved";
    const updated = await this.run(() => commands.setSlotApproved(s.id, approve));
    if (!updated || !approve) return;
    const next = this.nextOpen();
    const left = this.slots.length - this.doneCount;
    if (next) {
      this.selectSlot(next.id);
      toasts.show(`${s.name} is done. ${left} to go. Next: ${next.name}`);
    } else {
      toasts.show("All sounds are done! 🎉");
    }
  }

  /** Puts the active slot's original sound back. */
  async restore() {
    const s = this.slot;
    if (!s?.backedUp) return;
    await this.run(() => commands.restoreSlot(s.id), `${s.name} has its original sound again`);
  }

  async reveal(s = this.slot) {
    if (s) await attempt(() => commands.revealSlot(s.id));
  }
}

export const projects = new Projects();
