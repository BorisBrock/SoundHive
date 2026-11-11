# Building SoundHive

SoundHive is a [Tauri 2](https://v2.tauri.app) app: a SvelteKit + TypeScript frontend in `src/`
and a Rust backend in `src-tauri/` (audio decoding with [Symphonia](https://github.com/pdeljanov/Symphonia),
playback with [rodio](https://github.com/RustAudio/rodio), the library in SQLite).

## Requirements

- **Node.js** 22 or newer and **npm**.
- **Rust**: install [rustup](https://rustup.rs); the version pinned in `rust-toolchain.toml` is
  installed automatically on the first build.
- **Linux**: the WebKitGTK and ALSA build dependencies, see
  [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/).

  ```bash
  # Debian / Ubuntu
  sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev libasound2-dev
  # Arch / CachyOS
  sudo pacman -S --needed webkit2gtk-4.1 alsa-lib base-devel
  ```

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

## Building a release locally

`release.bat` (Windows) and `./release.sh` (Linux) build a release into `.release/`:

| Platform | Files |
|---|---|
| Windows | `SoundHive-<version>-windows-<arch>.exe` (portable, runs without installation) and a `.zip` with the executable and the README |
| Linux | `SoundHive-<version>-linux-<arch>.AppImage` (portable, bundles its libraries) and a `.tar.gz` with the bare binary and the README (needs WebKitGTK installed) |

Pass `--installers` to also get the NSIS/MSI or `.deb` installers. `npm run bundle:linux` and
`npm run bundle:windows` build the installers only; they end up in `src-tauri/target/release/bundle/`.

## Checks and tests

```bash
npm run lint                                   # Prettier + svelte-check
npm test                                       # search, filter, sort and similarity logic
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo test                     # analysis, tagging, database and scanner
```

`cargo test` also regenerates `src/lib/bindings.ts` (the typed Tauri commands); CI fails if the
committed file is out of date.

Scan benchmark against a real library:

```bash
cd src-tauri && SOUNDHIVE_BENCH=/path/to/sfx cargo test --release bench_scan -- --ignored --nocapture
```

## App data

The library database lives in `~/.local/share/dev.soundhive.app` on Linux and
`%APPDATA%\dev.soundhive.app` on Windows. Sound files are only ever read, never changed.
