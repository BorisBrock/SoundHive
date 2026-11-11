# SoundHive user guide

Press `?` in SoundHive to see this reference in the app. `Ctrl` is `⌘` on macOS.

## Search

Type into the search box (`Ctrl+K` or `/`). Words are matched against file names, folders, tags
and the descriptions embedded in the files. Combine them with operators:

| Query | Finds |
|---|---|
| `door slam` | both words, in name, folder, tags or description |
| `"door slam"` | the exact phrase |
| `-metal` | sounds without "metal" |
| `tag:foley` (or `t:foley`) | sounds with the tag |
| `ext:wav,ogg` | these file types |
| `dur:<2`, `dur:>10`, `dur:1-3`, `dur:500ms` | length in seconds (units `s`, `ms`, `m`) |
| `is:loop`, `-is:loop`, `is:fav` | loops, one-shots, favourites |

Example: `door slam -metal tag:foley ext:wav dur:<2 is:loop`.

Besides the search, the sidebar filters by collection (all, favourites, loops, recently played,
recently added, unreadable), folder (with the folder tree), length, loops/one-shots, mono/stereo,
file type and tags. The list sorts by name, length, loudness, brightness, date, size or times played.

## Keyboard shortcuts

| Key | Action |
|---|---|
| `↑` / `↓` | Previous / next sound (plays it when auto-play is on) |
| `Shift+↑` / `↓` | Extend the selection |
| `Space` | Play / stop |
| `Enter` | Play from the start |
| `←` / `→` | Skip back / ahead |
| `L` | Loop playback on / off |
| `F` | Favourite |
| `S` | Find similar sounds |
| `T` | Add a tag |
| `Ctrl+C` | Copy the file(s) to the clipboard, to paste into other apps |
| `Ctrl+Shift+C` | Copy the path(s) |
| `E` | Open in the editor (Audacity, or the one set in Settings) |
| `O` | Show in the file manager |
| `P` | Copy into the project folder |
| `Ctrl+A` | Select all sounds in the list |
| `Ctrl+K` or `/` | Search |
| `Esc` | Stop, clear the search, close dialogs |
| `Ctrl+,` | Settings |
| `?` | Shortcuts and search help |

## Getting sounds into other apps

- **Drag** a row, or the drag area in the details panel, into Audacity, a DAW, a game engine, a
  video editor or a folder.
- **`Ctrl+C`** puts the *file* on the clipboard; paste it into a file manager or any app that
  accepts files. `Ctrl+Shift+C` copies the path instead.
- **`E`** opens the sound in Audacity, or the editor set in Settings.
- **`P`** copies it into your project folder (asked once, then remembered). Existing files are
  never overwritten.
- **`O`** shows it in the file manager.

## What SoundHive measures

For every file: length, format, sample rate, bit depth, channels, peak and RMS level, attack time,
brightness, a waveform, and whether it is a **loop**. A sound counts as a loop when the file has
loop points, its name looks like one (`*_loop`), or its end flows seamlessly into its start.

**Find similar** (`S`) orders the library by how close sounds are in length, loudness, punch,
brightness, attack and tags.

## Tags

Tags come from three places:

- **Keyword rules** match file and folder names (e.g. "gunshot", "reload" or "sword" → `weapon`). Edit them in
  Settings; changes apply to the whole library at once.
- **The analysis** adds `loop`, `one-shot` and `long`.
- **You**: add and remove tags for one sound or a whole selection, and rename a tag everywhere.
  Automatic tags you remove stay removed after the next scan.

Optionally, every sound can also be tagged with the names of the folders it is in.

## Scanning

Add the folders that hold your sounds in the sidebar. SoundHive decodes files on all CPU cores; a
library of 24,000 sounds (6 GB) is analysed in about 11 seconds. Rescans (by default on every
start) only look at new and changed files and take well under a second. Moved or renamed files
keep their tags and favourite flag.

Supported formats: WAV, MP3, OGG (Vorbis), FLAC, AIFF and M4A (AAC).
