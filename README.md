# Replay Library

A fast Windows viewer for game replay clips from AMD Adrenalin, NVIDIA ShadowPlay, Xbox Game Bar and the Snipping Tool. Written in Rust with [gpui](https://crates.io/crates/gpui) and FFmpeg.

- Opens and lists clips without any indexing pass. Metadata and thumbnails fill in lazily, visible clips first, and are cached.
- Plays and scrubs with software decode that keeps up with 4K60 HEVC.
- **Trim & export**: pick a range, resolution, frame rate and codec (HEVC, AV1, H.264). GPU encoders (NVENC, AMF, QuickSync) are used when present.
- **Compress** to 8 MB, 20 MB or a custom size.

## Build

Needs Rust (edition 2024), LLVM (`winget install LLVM.LLVM`) and PowerShell.

```powershell
./scripts/fetch-ffmpeg.ps1
cargo run --release
```

`scripts/fetch-ffmpeg.ps1` downloads the pinned LGPL FFmpeg build into `third_party/ffmpeg` and verifies its SHA256. `build.rs` copies the FFmpeg DLLs and `ffmpeg.exe` next to the executable.

## Keys

| Key | Action |
|---|---|
| Ctrl K | Search (library) |
| Space / K | Play or pause |
| Left / Right | Seek 5 s (Shift: 1 s) |
| , / . | Step one frame |
| I / O | Set trim in / out |
| F | Fullscreen |
| M | Mute |
| D | Playback statistics |
| Esc | Back |

See `AGENTS.md`, `CODE_STYLE.md` and `PLAN.md` for the project rules and design.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option. The bundled FFmpeg build (LGPL) and fonts (SIL Open Font License) keep their own licenses.
