use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").is_ok_and(|target_os| target_os == "windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()
            .expect("failed to embed the executable icon");
        copy_ffmpeg_binaries();
    }
}

fn copy_ffmpeg_binaries() {
    let manifest_directory =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let ffmpeg_binaries = manifest_directory.join("third_party/ffmpeg/bin");
    let output_directory = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let profile_directory = output_directory
        .ancestors()
        .nth(3)
        .expect("OUT_DIR sits three levels below the profile directory");

    println!("cargo:rerun-if-changed={}", ffmpeg_binaries.display());
    let Ok(entries) = fs::read_dir(&ffmpeg_binaries) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let wanted = path.extension().is_some_and(|extension| extension == "dll")
            || path.file_name().is_some_and(|name| name == "ffmpeg.exe");
        if wanted {
            copy_if_changed(&path, profile_directory);
        }
    }
}

fn copy_if_changed(source: &Path, destination_directory: &Path) {
    let destination = destination_directory.join(source.file_name().expect("file has a name"));
    let up_to_date = fs::metadata(&destination)
        .and_then(|existing| Ok(existing.len() == fs::metadata(source)?.len()))
        .unwrap_or(false);
    if !up_to_date {
        fs::copy(source, &destination)
            .expect("failed to copy an FFmpeg binary next to the executable");
    }
}
