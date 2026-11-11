import { commands } from "./bindings";

/** Waveforms are fetched lazily for the rows on screen, in batches, and kept. */
class Waveforms {
  private cache = new Map<number, Uint8Array>();
  private missing = new Set<number>();
  private requested = new Set<number>();
  private timer: ReturnType<typeof setTimeout> | undefined;
  /** Bumped when waveforms arrive. */
  version = $state(0);

  get(id: number): Uint8Array | undefined {
    void this.version;
    const w = this.cache.get(id);
    if (!w && !this.requested.has(id)) {
      this.missing.add(id);
      clearTimeout(this.timer);
      this.timer = setTimeout(() => this.fetch(), 16);
    }
    return w;
  }

  /** Forget a sound's waveform after it was analysed again. */
  invalidate() {
    this.cache.clear();
    this.requested.clear();
    this.version++;
  }

  private async fetch() {
    const ids = [...this.missing];
    this.missing.clear();
    ids.forEach((id) => this.requested.add(id));
    try {
      for (const w of await commands.getWaveforms(ids)) this.cache.set(w.id, Uint8Array.from(w.peaks));
    } catch {
      ids.forEach((id) => this.requested.delete(id));
    }
    this.version++;
  }
}

export const waveforms = new Waveforms();
