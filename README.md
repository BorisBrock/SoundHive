# SoundHive

A fast desktop browser for large sound effect libraries. Add the folders that hold your sounds;
SoundHive finds every file, measures and tags it, and lets you search, audition and drag sounds
into your game and video projects.

- **Fast scans.** Files are decoded on all CPU cores; rescans only look at new and changed files.
  A library of 24,000 sounds (6 GB) is analysed in about 11 seconds and rescanned in under 0.1 s.
  Moved or renamed files keep their tags and favourite flag.
- **Instant playback** in the app (native audio, no browser player): click a row, or browse with
  ↑/↓ and every sound plays as it is selected. Click a waveform to jump to that point; loop playback
  with `L`.
- **Search** across names, folders, tags and embedded descriptions, with operators:
  `door slam -metal tag:foley ext:wav dur:<2 is:loop`. Filter by length, loops/one-shots,
  mono/stereo, file type, folder (with the folder tree) and tags. Sort by name, length, loudness,
  brightness, date, size or times played.
- **Metadata**: length, format, sample rate, bit depth, channels, peak and RMS level, attack time,
  brightness, waveform, and whether it is a **loop** (loop points in the file, a name like
  `*_loop`, or an end that flows seamlessly into the start).
- **Automatic tags** from file and folder names (editable keyword rules in Settings, applied to the
  whole library at once) and from the analysis (`loop`, `one-shot`, `long`). Add and remove tags
  yourself, for one sound or a whole selection; rename tags everywhere.
- **Find similar**: orders the library by how close sounds are in length, loudness, punch,
  brightness, attack and tags.
- **Getting sounds out**:
  - drag rows (or the drag area in the details) into Audacity, a DAW, a game engine, a video editor
    or a folder;
  - `Ctrl+C` puts the *file* on the clipboard, to paste into a file manager or any app that accepts
    files; `Ctrl+Shift+C` copies the path;
  - `E` opens the sound in Audacity (or another editor set in Settings);
  - `P` copies it into your project folder (never overwriting);
  - `O` shows it in the file manager.
- Favourites, recently played and recently added, multi-selection, 12 themes, and `?` for all
  keyboard shortcuts.

Supported formats: WAV, MP3, OGG (Vorbis), FLAC, AIFF and M4A (AAC).

## Requirements

- **Node.js** 22 or newer and **npm**.
- **Rust**: install [rustup](https://rustup.rs); the version pinned in `rust-toolchain.toml` is
  installed automatically on the first build.
- **Linux**: the WebKitGTK and ALSA build dependencies, e.g. on Debian/Ubuntu

  ```bash
  sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev libasound2-dev
  ```

  (Arch: `webkit2gtk-4.1 alsa-lib`). See [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/).
- **Windows**: the "Desktop development with C++" workload of Visual Studio Build Tools. WebView2 is
  part of Windows 10/11.

## Development

```bash
npm install
npm run tauri dev
```

In dev builds the library state is available as `soundhive` in the web inspector's console. Audio
decoding dependencies are compiled with optimisations even in dev builds, but SoundHive's own
analysis code is not, so scans are several times slower than in a release build.

To build a release binary without installers and start it: `build.bat` / `run.bat` on Windows,
`./build.sh` / `./run.sh` on Linux.

## Building a release

`release.bat` (Windows) and `./release.sh` (Linux) build a release into `.release/` (portable `.exe`
/ `.AppImage` plus a `.zip` / `.tar.gz`); pass `--installers` to also get the NSIS/MSI or `.deb`
installers. `npm run bundle:linux` and `npm run bundle:windows` build the installers only.

App data (the library database) lives in `~/.local/share/dev.soundhive.app` on Linux and
`%APPDATA%\dev.soundhive.app` on Windows. Sound files are only ever read, never changed.

## Checks and tests

```bash
npm run lint                                   # Prettier + svelte-check
npm test                                       # search, filter, sort and similarity logic
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo test                     # analysis, tagging, database and scanner
```

Scan benchmark against a real library:

```bash
cd src-tauri && SOUNDHIVE_BENCH=/path/to/sfx cargo test --release bench_scan -- --ignored --nocapture
```
