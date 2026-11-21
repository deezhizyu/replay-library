# Plan

## Context
`C:\Users\admin\Desktop\dev\replay-library` is empty. The user wants a Windows replay-clip viewer in Rust that replaces the slow NVIDIA/AMD viewers: opens instantly, lists clips and thumbnails instantly, plays and scrubs instantly, compresses a clip to 8 / 20 MB (or a custom size), and trims + exports a range with a chosen resolution, frame rate and video codec (HEVC, AV1, H.264) through a save-file picker. UI: gpui, following the design in `C:\Users\admin\Desktop\Rust Replay Viewer — UI Design-png` (Library, Player, Trim & export, Compress, Settings — video folders, Exporting, Exported). No proxies, no pre-indexing pass.

Answers from the user:
- **FFmpeg build:** LGPL shared build (GPU encoders AMF/NVENC/QSV, SVT-AV1 / openh264 software fallback). App license free to choose (MIT).
- **Toolchain:** agent installs LLVM (`winget install LLVM.LLVM`) and downloads a pinned, vetted BtbN FFmpeg LGPL shared build into gitignored `third_party/ffmpeg`.
- **Settings:** only the Folders page (+ indexing toggles, thumbnail cache rebuild/clear). Other nav items hidden.
- **Git:** local repo only, branch `main`. Workflows written but run once a remote exists.

Machine: Windows 11, AMD RX 7600 (AMF encoders, D3D11VA decode). Clips in `%USERPROFILE%\Videos\Radeon ReLive\<Game>\<Game>_replay_YYYY.MM.DD-HH.MM.mp4` and `Videos\Screen Recordings`.

## Order of work (from the user)
1. `git init -b main`.
2. Adapted copies from `blink-reminder`: `AGENTS.md` (same critical dependency-vetting rule, new project section, dependency table, FFmpeg binary vetting, commands), `CODE_STYLE.md` (verbatim — it is project-neutral), `CLAUDE.md` (`@AGENTS.md` / `@CODE_STYLE.md`), `PLAN.md` (this plan), `.gitignore` (`/target`, `/third_party`), `.github/workflows/ci.yml` + `release.yml` adapted (see CI below). First commit: `docs: add agent instructions, code style and plan`.
3. Toolchain: LLVM via winget, FFmpeg LGPL shared build (pinned tag + SHA256) into `third_party/ffmpeg`, `FFMPEG_DIR` set through `.cargo/config.toml` `[env]` (relative path).
4. Build the app, one Conventional Commit per logical step (scaffold, library scan, thumbnails, player, scrub, trim/export, compress, settings).

## Architecture (speed decisions)
- **Decode = FFmpeg libs in-process** via `ffmpeg-next` with **D3D11VA** hardware decode (falls back to software). Bypasses Windows' paid HEVC extension, handles H.264/HEVC/AV1 from ShadowPlay/ReLive/Game Bar.
- **Frame path:** GPU decode → `av_hwframe_transfer_data` (NV12) → `swscale` straight to **BGRA at the on-screen pixel size** (not source size) → `gpui::RenderImage` → `img()`. Previous frame image released with `window.drop_image` so the atlas does not grow. A 1440p clip in a ~1240×700 view uploads ~3.5 MB/frame.
- **Player threads:** demux+decode thread → bounded frame queue (≈8 frames) → UI pulls the frame matching the clock on `request_animation_frame`. Audio decoded + resampled (`swresample` → f32 at device rate), played through `cpal`; **audio clock is master**, video follows. Speed ≠ 1× goes through an `atempo` filter (libavfilter).
- **Instant scrub:** during drag, seek to the previous keyframe and show that frame at once; when the pointer rests (~60 ms) decode forward to the exact frame. Timeline filmstrip (~28 frames) and hover preview come from a second decoder that decodes **keyframes only** (`skip_frame = NONKEY`), in background, filling left to right.
- **Instant library:** `read_dir` (recursive) on a background thread; `DirEntry` metadata gives size and time without opening files. Title + date parsed from the filename (ReLive, ShadowPlay, Game Bar / Screen Recording patterns), falling back to modified time. Grid renders immediately. Duration/resolution/fps/codec and thumbnails are filled **lazily, visible rows first**, by a worker pool (N = cores/2) — no pre-index pass.
- **Persistent cache** (so the second launch is instant): `%LOCALAPPDATA%\replay-library\`: `probe.bin`-style TOML/JSON of probe results keyed by path+size+mtime, and `thumbs\<hash>.jpg` (320×180) written with FFmpeg's `mjpeg` encoder and read by gpui's built-in image loader. Hover-scrub strips (8 frames per clip) cached the same way when the setting is on.
- **Export / compress = bundled `ffmpeg.exe` subprocess** (same LGPL build), `-progress pipe:1` parsed for % / speed / remaining, cancel = kill. `-ss` before `-i` for fast accurate seek, `-hwaccel d3d11va` decode, encoder picked by availability: `{hevc,av1,h264}_{amf,nvenc,qsv}` → software `libsvtav1` / `libopenh264` / (HEVC has no LGPL software encoder → HEVC disabled if no GPU encoder). Encoder availability probed once at startup in background (1-frame null encode per candidate), cached.
- **Compress to size:** pure planner fn: `target_bytes, duration, source (w,h,fps)` → `(resolution, fps, video_kbps, audio_kbps)`; ladder per design (8 MB → 720p30 H.264 @ 64 kbps audio, 20 MB → 1080p30, custom scales). Budget 96% of target to absorb VBV overshoot; after encode, if file > target, re-run once at `bitrate × target/actual × 0.97`. Unit tested.
- **Startup:** window opens with last library list from cache before any scan; FFmpeg DLLs are loaded at startup (dynamic link, ~tens of ms) — measure, and switch to delay-load only if measured slow.

## Dependencies (each vetted per AGENTS.md before it enters `Cargo.toml`; versions confirmed at add time)
| Crate | Why |
|---|---|
| `gpui` 0.2.x | UI, windows, rendering, images, timers |
| `ffmpeg-next` 9.x (+ `ffmpeg-sys-next`) | Demux, decode (D3D11VA), swscale, swresample, avfilter, mjpeg thumbnails |
| `cpal` | Audio output (WASAPI) |
| `notify` | Watch folders for new clips (setting "Watch for new clips") |
| `rfd` | Native save-file / folder picker (check first whether gpui's `prompt_for_new_path` / `prompt_for_paths` covers it on Windows — if yes, no `rfd`) |
| `serde` + `toml` | Settings and probe cache |
| `anyhow` | Errors |
| `windows` (same version as gpui's, already in tree) | Registry reads for capture-folder detection, `SHGetKnownFolderPath`, "Show in folder" (`SHOpenFolderAndSelectItems`) |
| `winresource` (build) | Exe icon |
External binary: BtbN FFmpeg LGPL shared win64 build, pinned release + SHA256, recorded in AGENTS.md with vetting notes (official BtbN/FFmpeg-Builds GitHub releases built by github-actions).

## Layout
Cargo binary crate, edition 2024, file-per-module (no `mod.rs`), inline tests, `assets/` (Inter + JetBrains Mono TTF, icons as SVG, `icon.ico`).
```
src/main.rs              App::run, fonts, theme, window (custom titlebar), routing
src/theme.rs             Design tokens: #0A0B0D bg, #12151A/#171A1F surfaces, #16191E/#23272E borders, #E9ECF1 text, #8B929E muted, #FF7A1A accent
src/settings.rs          Folders (path, enabled, kind NVIDIA/AMD/Game Bar/Other), watch, hover previews; TOML in %APPDATA%\replay-library
src/capture_folders.rs   First-start detection of recorder save folders (see below); registry reads via the `windows` crate
src/library.rs           Clip list, scan, grouping (Today / Yesterday / This week / month), sort, search — pure fns tested
src/clip_name.rs         Filename → title + timestamp for ReLive / ShadowPlay / Game Bar / Screen Recording — tested
src/probe_cache.rs       Probe results + thumbnail paths keyed by path+size+mtime
src/media.rs             FFmpeg init, hw device, open input, probe
src/media/decoder.rs     Video decode thread, seek, frame queue, BGRA scaling
src/media/audio.rs       Audio decode, resample, atempo, cpal output, clock
src/media/thumbnails.rs  Worker pool: poster frame, keyframe strip, mjpeg write
src/media/encoders.rs    Encoder availability probe
src/media/transcode.rs   ffmpeg.exe command building (pure, tested) + run with progress + cancel
src/compress_plan.rs     Size-target planner — tested
src/views/library_view.rs, player_view.rs, trim_export_view.rs, compress_view.rs, settings_view.rs, export_dialog.rs (Exporting / Saved)
src/components/          titlebar, button, segmented, radio_card, toggle, slider, timeline (filmstrip + playhead + in/out handles + hover preview), search_input, badge
```

## Key behavior (per design)
- **Library:** search (Ctrl+K), thumbnail size slider (grid columns), sort menu (Newest / Oldest / Largest / Longest / Name), date groups, cards with res badge (`1440p60`), duration, title, `time · size`. Hover: orange ring, play button, hover-scrub across the strip. Footer: count · total size, index status. Click → Player. Virtualized list (`uniform_list` over rows) so thousands of clips cost nothing.
- **Player:** back to Library, title + date, info chip (`2560×1440 · 60 fps · HEVC · 48 Mbps`), video letterboxed, filmstrip timeline with playhead and hover preview + timestamp, prev/play/next clip, volume, time `00:18.42 / 00:42.00`, speed (0.25–2×), fullscreen, Compress, Trim & export. Keys: Space, ←/→ (5 s), ,/. (frame step), F, M, Esc.
- **Trim & export:** In/Out handles on the filmstrip, In/Out/Length fields, Reset, Loop clip, keys I / O / Space. Right panel: resolution (source + standard ladder ≤ source), fps (source, 30, 24 — only ≤ source), codec cards (AV1 disabled with reason when no AV1 encoder), audio AAC / Opus / Copy source, video/audio bitrate, encoder row, estimated size. **Export…** → native save dialog (default `<title>-clip.mp4`) → Exporting dialog (%, speed, elapsed, remaining, encoder, Cancel) → Saved dialog (size, length, took, format, Show in folder).
- **Compress:** 8 MB / 20 MB / Custom cards, "What you get" table from the planner, source vs result size bars, **Compress & save…** → save dialog → same Exporting / Saved dialogs.
- **Settings — Folders:** list with path, count · size, kind badge, enable toggle, delete; Add folder (folder picker + drop). Indexing: Watch for new clips, Hover-scrub previews, Thumbnail cache size + Rebuild / Clear.
- **First start (no settings file):** detect each recorder's save folder and add the ones that exist on disk, with the matching kind badge:
  - AMD Adrenalin / ReLive: `HKCU\Software\AMD\DVR\CaptureFileOutput` (REG_SZ; on this machine `C:\Users\admin\Videos\Radeon ReLive`), fallback `%USERPROFILE%\Videos\Radeon ReLive`.
  - NVIDIA ShadowPlay / NVIDIA App: `HKCU\Software\NVIDIA Corporation\Global\ShadowPlay\NVSPCAPS\DefaultPathW` (REG_BINARY, UTF-16), fallback `%USERPROFILE%\Videos` scanned for `*.DVR.mp4` game subfolders; verify the NVIDIA App key on a machine with it if available.
  - Xbox Game Bar: Captures known folder (`SHGetKnownFolderPath(FOLDERID_Captures)` / User Shell Folders `{EDC0FE71-98D8-4F4A-B920-C8DC133CB165}`), fallback `%USERPROFILE%\Videos\Captures`.
  - Windows Snipping Tool: `%USERPROFILE%\Videos\Screen Recordings` (exists here, 8 clips).
  - Nested duplicates removed (a folder inside another added folder is not added twice). Detection runs only when no settings file exists; result saved immediately. Settings > Folders gets a **Detect again** action using the same function. Path-parsing logic (UTF-16 binary, env expansion, dedupe) is pure and tested.
- Custom titlebar with minimize / maximize / close (gpui window control areas).

## CI (adapted from blink-reminder)
- `ci.yml`: `windows-latest`, download the pinned FFmpeg LGPL build (checksum-verified) to `third_party/ffmpeg`, LLVM is preinstalled on the runner, then `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`. Same `Swatinem/rust-cache` pin and cache rules.
- `release.yml`: tag `vX.Y.Z` = `Cargo.toml` version → build release → zip `replay-library.exe` + FFmpeg DLLs + `ffmpeg.exe` + FFmpeg LICENSE → SHA256 → GitHub release.
- FFmpeg download step in one shared script (`scripts/fetch-ffmpeg.ps1`) used locally and in both workflows.

## Skipped (not asked)
Settings pages other than Folders, macOS/Linux, stream-copy (lossless, keyframe-snapped) export, multi-clip merge, tagging/favorites, single-instance guard.

## Risks to check first
- gpui 0.2 per-frame `RenderImage` at 60 fps: measure upload cost and confirm `drop_image` frees atlas space. Fallback: reuse a fixed-size frame and keep the scaled size ≤ view size.
- `ffmpeg-next` 9.x vs FFmpeg 8.x headers compatibility on Windows/MSVC with bindgen.
- D3D11VA device creation when gpui already owns a D3D11 device — use a separate device (no sharing needed on the CPU-copy path).
- AMF HEVC/AV1 1-pass rate control overshoot for compress-to-size (mitigated by the re-run rule).
- HEVC export has no LGPL software encoder: disable with reason when no GPU HEVC encoder.

## Verification
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` locally (Windows) — tests cover clip-name parsing, grouping/sort/search, compress planner, ffmpeg argument building, progress parsing, settings round-trip.
- Run the app (`cargo run --release`) against `%USERPROFILE%\Videos`: measure cold start to first painted grid, thumbnail fill time for the 66 local clips, seek latency (log timings), 1440p/4K playback with dropped-frame counter in debug builds.
- Export a 20 s trim in each codec via AMF, verify output with bundled `ffprobe` (codec, res, fps, duration ±1 frame). Compress to 8 MB and 20 MB, assert size ≤ target.
- Screenshot each screen and compare against the design PNGs.
