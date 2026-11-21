# AGENTS.md

## Critical rule: vet every dependency

Before adding or upgrading any dependency (crate, build dependency, tool, or bundled binary), check it for supply-chain attacks and malware, and confirm it can be trusted at all. Do this before it goes into `Cargo.toml` or the repository. Do not skip it for "small" or "well-known" crates.

Checklist:

- The crates.io page links to the official repository, and that repository links back to the crate.
- The owners and publishers are the expected organization or maintainers. Look for recent ownership changes.
- The crate name is not a typosquat of a popular crate.
- Download count, age, and release history look healthy. Be suspicious of a brand-new crate or a sudden release after a long silence.
- No open RustSec advisory (`cargo audit`). Run `cargo deny check` too if it is configured.
- `build.rs` files and procedural macros do not touch the network, run unexpected processes, or read files outside the build.
- The published crate contents match the repository (`cargo vendor` or the crates.io source view when in doubt).
- The dependency tree it brings in is acceptable. Prefer features that keep it small (`default-features = false`).

Prefer, in order: the standard library, a crate already in the tree, a new vetted crate. Pin versions through `Cargo.lock`, which is committed. Record any new dependency and the reason for it in the dependency table below.

Bundled binaries (FFmpeg) follow the same rule: download only from the official release page of the project that builds them, pin the exact release tag and the SHA256 of the archive, verify the hash in `scripts/fetch-ffmpeg.ps1`, and record the vetting in the table below.

## GitHub Actions

The same rule applies to third-party actions. Pin each one to a full commit SHA with the version in a trailing comment, and vet it before use. `actions/checkout` is official. Vetted third-party actions:

| Action | Pinned | Vetting |
|---|---|---|
| `Swatinem/rust-cache` | `d1527c8f67ef9af357290475d4af4b0fab8f12e0` (v2.9.3) | Single long-time maintainer, 100+ commits, runs on `node24` from a committed `dist/`. Source and `dist/` inspected for network calls and process execution: only the GitHub cache service and opt-in cache backends, and `sudo purge` on macOS. `dist/` was not rebuilt and diffed against `src/`. |

## Project

Replay Library is a Windows desktop app written in Rust. It replaces the slow NVIDIA and AMD replay viewers with one fast viewer for game clips.

Goals, in order of importance:

- **Speed.** The window opens instantly. The clip list and thumbnails appear instantly. Playback and scrubbing respond instantly. No proxy files and no pre-indexing pass: metadata and thumbnails are filled lazily, visible clips first, and cached on disk.
- **Compress to 8 MB, 20 MB, or a custom size**, quickly, inside the app.
- **Trim and export.** Select a start and end, then export with a chosen resolution, frame rate and video codec (HEVC, AV1, H.264) through a save-file dialog.
- **Look.** Dark, slick, following the design in `C:\Users\admin\Desktop\Rust Replay Viewer — UI Design-png` (Library, Player, Trim & export, Compress, Settings, Exporting, Saved).

Platform: Windows only. UI framework: [gpui](https://crates.io/crates/gpui). Decoding and transcoding: FFmpeg (LGPL shared build). Settings file: TOML in `%APPDATA%\replay-library`. Cache: `%LOCALAPPDATA%\replay-library`.

On the first start the app detects the save folders of AMD Adrenalin, NVIDIA ShadowPlay, Xbox Game Bar and the Snipping Tool, and adds the ones that exist.

See `PLAN.md` for the design and the build order.

## Dependencies

| Crate or binary | Purpose | Vetting |
|---|---|---|
| `gpui` 0.2.2 | UI, windows, rendering | Official zed-industries crate, 539k downloads. Same version and tree as vetted in blink-reminder. `cargo audit`: no vulnerabilities, only "unmaintained" notices on transitive crates (`ttf-parser`, `rustybuzz`, `paste`, `zbus` family). |
| `ffmpeg-next` `=9.0.0` | FFmpeg bindings: demux, decode, scale, resample, thumbnails | Repo `zmwangx/rust-ffmpeg`, 9M downloads, published by the main maintainer. Default features only. Pulls `ffmpeg-sys-next` 9.1.1 (owners now also include `AdrianEddy`, `dmtrKovalenko`, who are Gyroflow developers; 9.1.0 was published by `AdrianEddy`). Its `build.rs` runs `git clone` of FFmpeg and `sh configure` only under the `build` feature, which is never enabled: we link the prebuilt library from `FFMPEG_DIR`. Build-time deps: `bindgen` (rust-lang), `cc`, `pkg-config`, `vcpkg`, `num_cpus`. Needs LLVM `libclang`. |
| `cpal` 0.18.2 | Audio output (WASAPI) | RustAudio org, 30M downloads, since 2014. `build.rs` only reads `CPAL_ASIO_DIR`. Brings `windows` 0.62 next to gpui's 0.61 (duplicate, accepted). |
| `notify` 8.2.2 | Watch capture folders for new clips | notify-rs org, 205M downloads. No `build.rs`. Windows backend uses `windows-sys`. Stays on 8 (vetted version) although 9 exists. |
| `image` 0.25 | JPEG thumbnail encoding, pixel buffers for gpui images | image-rs org. Already in the tree through `gpui` (same version, same default features). |
| `serde`, `toml` | Settings and cache files | dtolnay / toml-rs maintainers, standard crates. |
| `anyhow` | Error handling | dtolnay. |
| `windows` 0.61 | Registry reads for capture-folder detection, shell calls | Microsoft (`kennykerr`). Same version gpui uses. |
| `winresource` (build) | Executable icon | Used and vetted in blink-reminder. |
| FFmpeg `n9.0.2-61-gd59078794d` LGPL shared win64 | Hardware decode, encoders, `ffmpeg.exe` for export | BtbN/FFmpeg-Builds release `autobuild-2026-11-21-12-37`, built and published by `github-actions[bot]`. Archive SHA256 `499fa5e468f9ea6a4d38647f81f9ca4a9ac33655f30faf54b7a800ca9f4d6e5f` matches both the release `checksums.sha256` and the GitHub asset digest; `scripts/fetch-ffmpeg.ps1` refuses any other hash. LGPL build: no x264/x265 (software HEVC via `libkvazaar`, AV1 via `libsvtav1`, H.264 via `libopenh264`, GPU encoders AMF, NVENC, QSV). |

Run `cargo audit` after every dependency change.

## Layout

Standard Cargo binary crate, edition 2024. Modules use the file-per-module style (`src/components.rs` plus `src/components/*.rs`, no `mod.rs`). Unit tests live inline in `#[cfg(test)] mod tests`. Assets are in `assets/`. The FFmpeg build lives in `third_party/ffmpeg` (gitignored, created by `scripts/fetch-ffmpeg.ps1`).

Module list and responsibilities are in `PLAN.md`.

## Commands

```bash
./scripts/fetch-ffmpeg.ps1
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo audit
```

The app targets Windows only. Building needs LLVM (`winget install LLVM.LLVM`) for the FFmpeg bindings; `.cargo/config.toml` points `LIBCLANG_PATH` at `C:/Program Files/LLVM/bin` and `FFMPEG_DIR` at `third_party/ffmpeg`. `build.rs` copies the FFmpeg DLLs and `ffmpeg.exe` next to the built executable.

## Workflow

- Follow `CODE_STYLE.md` for all code and commit messages.
- Search the codebase for an existing implementation before writing a new feature or helper.
- Keep `PLAN.md` and this file correct when the design or the dependencies change.
- Release: bump `version` in `Cargo.toml`, then push a tag `vX.Y.Z` that matches it. `.github/workflows/release.yml` builds and publishes the GitHub release.
- Build cache: only CI runs on the default branch save the cargo cache (`shared-key: windows`, debug and release profiles). Other branches and tag releases only restore it, because GitHub lets a run restore caches from its own ref or the default branch only. Keep the `shared-key` equal in `ci.yml` and `release.yml`. A tag push starts the release together with the default branch CI, so `release.yml` first waits for the CI run on the same commit to finish saving the cache. Push the commit to the default branch together with the tag.
- Develop on `main` unless a branch is given for the session. Do not open a pull request unless asked.
