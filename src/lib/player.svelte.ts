import { commands } from "./bindings";
import { attempt, library, type Sound } from "./app.svelte";

/**
 * Playback runs natively in the backend. The position is polled while a sound plays and
 * extrapolated in between, so the playhead moves smoothly.
 */
class PlayerState {
  currentId = $state<number | null>(null);
  playing = $state(false);
  /** Seconds into the current sound. */
  position = $state(0);
  /** Loop the next sounds played. */
  loop = $state(false);

  private duration = 0;
  private basePosition = 0;
  private baseTime = 0;
  private token = 0;
  private pollTimer: ReturnType<typeof setTimeout> | undefined;
  private frame = 0;
  private volumeTimer: ReturnType<typeof setTimeout> | undefined;

  current = $derived(this.currentId !== null ? library.byId.get(this.currentId) : undefined);

  async play(sound: Sound, start = 0) {
    const token = ++this.token;
    const ok = await attempt(async () => {
      await commands.play(sound.id, start, this.loop, library.settings.volume);
      return true;
    });
    if (!ok || token !== this.token) return;
    this.currentId = sound.id;
    this.duration = sound.duration;
    this.playing = true;
    this.setBase(start);
    this.position = start;
    this.startTicking();
    if (start === 0) {
      // Counted by the backend; reflect it without reloading.
      sound.playCount++;
      sound.lastPlayed = Math.floor(Date.now() / 1000);
    }
  }

  async stop() {
    this.token++;
    this.halt();
    await attempt(() => commands.stop());
  }

  /** Space: plays the sound, or stops it if it is playing. */
  async toggle(sound: Sound | undefined) {
    if (!sound) return;
    if (this.playing && this.currentId === sound.id) await this.stop();
    else await this.play(sound);
  }

  /** Jumps to a point of a sound, starting it if it is not the one playing. */
  async seek(sound: Sound, seconds: number) {
    seconds = Math.max(0, Math.min(seconds, Math.max(0, sound.duration - 0.01)));
    if (this.playing && this.currentId === sound.id) {
      this.setBase(seconds);
      this.position = seconds;
      await attempt(() => commands.seek(seconds));
    } else {
      await this.play(sound, seconds);
    }
  }

  async toggleLoop() {
    this.loop = !this.loop;
    // Restart the playing sound so that the change is heard right away.
    const s = this.current;
    if (this.playing && s) await this.play(s, this.position);
  }

  setVolume(volume: number) {
    library.settings.volume = volume;
    commands.setVolume(volume).catch(() => {});
    clearTimeout(this.volumeTimer);
    this.volumeTimer = setTimeout(() => library.updateSettings({ volume }), 400);
  }

  private setBase(position: number) {
    this.basePosition = position;
    this.baseTime = performance.now();
  }

  private halt() {
    this.playing = false;
    this.position = 0;
    clearTimeout(this.pollTimer);
    cancelAnimationFrame(this.frame);
  }

  private startTicking() {
    clearTimeout(this.pollTimer);
    cancelAnimationFrame(this.frame);
    const tick = () => {
      if (!this.playing) return;
      let p = this.basePosition + (performance.now() - this.baseTime) / 1000;
      if (this.loop && this.duration > 0) p %= this.duration;
      else p = Math.min(p, this.duration);
      this.position = p;
      this.frame = requestAnimationFrame(tick);
    };
    this.frame = requestAnimationFrame(tick);
    const token = this.token;
    const poll = async () => {
      if (token !== this.token) return;
      try {
        const state = await commands.playbackState();
        if (token !== this.token) return;
        if (state.soundId === null) {
          this.halt();
          return;
        }
        this.setBase(state.position ?? 0);
      } catch {
        // Keep extrapolating.
      }
      this.pollTimer = setTimeout(poll, 120);
    };
    this.pollTimer = setTimeout(poll, 120);
  }
}

export const player = new PlayerState();
