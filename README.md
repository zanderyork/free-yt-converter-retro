# RetroTube

A local-first YouTube → MP3/MP4 converter for Apple Silicon, wrapped in a
Windows 2000 / XP retro dialog. Single-purpose, minimalist, no telemetry, no
cloud, no accounts.

- **Cold start**: ~half a second on M1
- **Idle memory**: ~70–90 MB
- **Distributed binary**: ~89 MB total .app, ~61 MB compressed .dmg
  (4 MB native shell + ~85 MB of bundled `yt-dlp` + `ffmpeg`)
- **Stack**: Tauri (Rust core + WKWebView) + vanilla HTML/CSS/JS

---

## What it does

Paste a YouTube URL → pick MP3 or MP4 → pick quality → click Convert.
Files land in `~/Downloads/RetroTube/`.

- **MP3** path: extracts the best audio stream and transcodes to MP3 at
  ~190 kbps VBR (LAME `-V2`), with metadata and thumbnail embedded.
- **MP4** path: requests the best video+audio combination at or below the
  selected resolution cap, prefers streams that mux without re-encoding,
  and falls back to M1's hardware encoder (`h264_videotoolbox` via
  VideoToolbox) only when re-encoding is forced.

Live progress, cancel-mid-conversion, and an "Open Folder" shortcut to
reveal results in Finder.

---

## Building from source

### Prerequisites

- macOS on Apple Silicon (M1 / M2 / M3 / …)
- Xcode Command Line Tools: `xcode-select --install`
- [Rust](https://rustup.rs) with the `aarch64-apple-darwin` target
- Node 18+ (only used to run the Tauri CLI)
- `curl` and `unzip` (preinstalled on macOS)

```bash
# Install rust if you don't have it.
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add aarch64-apple-darwin
```

### One-time setup

```bash
git clone <this repo>
cd retro-yt-mp3

# Pull the bundled sidecar binaries (yt-dlp + arm64 ffmpeg).
# Idempotent; rerun with --force to refresh.
./scripts/fetch-sidecars.sh

# Install the Tauri CLI locally.
npm install
```

### Run in dev mode

```bash
npm run dev
```

Hot reload is wired up — edit anything under `ui/` and the WebView refreshes.
Edits to Rust under `src-tauri/src/` trigger a rebuild on next launch.

### Build the release `.app` and `.dmg`

```bash
npm run build
```

Outputs land in:

```
src-tauri/target/release/bundle/macos/RetroTube.app
src-tauri/target/release/bundle/dmg/RetroTube_1.0.0_aarch64.dmg
```

Drag `RetroTube.app` into `/Applications`.

### First launch & Gatekeeper

The build is unsigned. macOS will refuse to launch it the first time:

> "RetroTube" cannot be opened because Apple cannot check it for malicious software.

Right-click `RetroTube.app` → **Open** → confirm the dialog. macOS
remembers the choice; subsequent launches are normal. (See the build prompt:
notarization is intentionally skipped for personal use.)

If you are sharing the binary, sign it ad-hoc to satisfy Apple Silicon's
"signed binaries only" rule:

```bash
xattr -cr /Applications/RetroTube.app
codesign --force --deep --sign - /Applications/RetroTube.app
```

---

## Project layout

```
retro-yt-mp3/
├─ ui/                     # vanilla HTML/CSS/JS frontend
│  ├─ index.html           # the single dialog
│  ├─ styles.css           # Win2K bevels, gradients, segmented progress
│  └─ app.js               # IPC + DOM updates only
├─ src-tauri/              # Rust core + Tauri config
│  ├─ Cargo.toml
│  ├─ tauri.conf.json
│  ├─ capabilities/        # permission scopes for the WebView
│  ├─ binaries/            # bundled yt-dlp + ffmpeg (gitignored)
│  ├─ icons/               # generated app icons
│  └─ src/
│     ├─ main.rs           # entry point
│     ├─ lib.rs            # Tauri builder, plugins, lifecycle
│     ├─ commands.rs       # Tauri commands exposed to JS
│     ├─ conversion.rs     # subprocess management + arg construction
│     ├─ progress.rs       # parsing yt-dlp's structured stdout
│     └─ paths.rs          # output dir + sidecar resolution
├─ scripts/
│  ├─ fetch-sidecars.sh    # downloads yt-dlp + ARM64 ffmpeg
│  └─ gen-icon.js          # regenerates the source app icon
├─ package.json
└─ README.md
```

The Rust side exposes four commands (`start_conversion`, `cancel_conversion`,
`open_output_folder`, `reveal_file_in_finder`) and emits four events
(`conversion-progress`, `conversion-complete`, `conversion-error`,
`conversion-cancelled`). The frontend consumes them. That's the entire
contract.

---

## How conversions work, in detail

Each conversion is a single `yt-dlp` subprocess. We spawn it through Tauri's
shell plugin, parse its structured stdout (via `--progress-template` with a
`RTPROG\t…` prefix), and emit progress events as the download advances.
yt-dlp invokes our bundled `ffmpeg` (passed via `--ffmpeg-location`) for
muxing and any required transcoding.

### MP3 args (abridged)

```
--no-playlist --newline
--progress-template "RTPROG\t%(progress.status)s\t%(progress._percent_str)s\t…"
--print "after_move:RTDONE\t%(filepath)s"
--ffmpeg-location <bundled ffmpeg>
--paths home:~/Downloads/RetroTube
-o "%(title).200B [%(id)s].%(ext)s"
--concurrent-fragments 4
-f bestaudio[ext=m4a]/bestaudio/best
-x --audio-format mp3 --audio-quality 2
--embed-metadata --embed-thumbnail
<URL>
```

### MP4 args (abridged)

```
…shared flags…
-f "bestvideo[height<=1080][ext=mp4]+bestaudio[ext=m4a]/bestvideo[height<=1080]+bestaudio/best[height<=1080]"
--merge-output-format mp4
--postprocessor-args "VideoConvertor:-c:v h264_videotoolbox -b:v 6M -c:a aac -b:a 192k"
<URL>
```

(Where `[height<=1080]` is replaced with the user's quality cap, or omitted
for "Best".)

### Cancellation

The Rust core keeps the spawned `CommandChild` in an `AppState` map keyed by
job id. `cancel_conversion` removes it from the map and calls
`child.kill()`. The monitoring task notices the entry is gone when the
process terminates and emits `conversion-cancelled` instead of
`conversion-complete`. Closing the window also drains the map and kills any
in-flight subprocesses, so we never leave orphaned ffmpeg/yt-dlp around.

---

## Updating yt-dlp

YouTube changes its extractor surface every few weeks. When yt-dlp starts
failing on a video, refresh the bundled binary:

```bash
./scripts/fetch-sidecars.sh --force
npm run build
```

(A one-tap "Update yt-dlp" button is in the stretch-features list.)

---

## Hard rules (kept faithfully)

- No outbound network calls beyond YouTube itself. No analytics, no crash
  reporting, no auto-update pings.
- No cloud, no accounts, no telemetry, no history sync.
- No Electron / React / Vue. Tauri + vanilla JS, deliberately.
- The retro UI spec: 4-color bevels, segmented marching-squares progress,
  navy gradient title bar, dotted focus ring on the active button, Tahoma
  11px, sharp corners everywhere.

---

## Stretch features (post-v1)

Not implemented yet — see `BUILD_PROMPT.md`-style spec at the top of this
project for the full list. Highlights:

- Drag-and-drop a YouTube URL onto the window
- Global hotkey to pop the window with the clipboard URL
- Recent conversions list
- Trim mode (start/end timestamps)
- One-tap "update yt-dlp"

---

## License

Personal-use project. Bundled binaries (`yt-dlp`, `ffmpeg`) carry their own
licenses — check their respective projects.
