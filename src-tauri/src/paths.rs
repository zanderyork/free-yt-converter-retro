use std::path::PathBuf;

/// Output directory for converted files. Lives under the user's Downloads folder
/// so the result is exactly where users expect new files to land.
pub fn output_directory() -> PathBuf {
    if let Some(home) = dirs_home() {
        home.join("Downloads").join("RetroTube")
    } else {
        // Last-ditch fallback — should be unreachable on macOS.
        PathBuf::from("/tmp/RetroTube")
    }
}

/// Create the output directory tree if it doesn't already exist.
pub fn ensure_output_directory() -> std::io::Result<PathBuf> {
    let dir = output_directory();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Resolve the bundled ffmpeg sidecar binary path. yt-dlp invokes ffmpeg as a
/// child process for muxing/transcoding, so we need a real on-disk path to hand it.
///
/// Tauri's `externalBin` mechanism appends the target triple to the filename in
/// dev mode (e.g. `ffmpeg-aarch64-apple-darwin`), and places binaries adjacent
/// to the running executable in release builds.
pub fn ffmpeg_sidecar_path() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();

    let target_triple = current_target_triple();
    let triple_name = format!("ffmpeg-{target_triple}");

    let candidates = [
        exe_dir.join(&triple_name),
        exe_dir.join("ffmpeg"),
        // Dev fallback: when running `cargo run` directly without going through
        // the Tauri bundler, the binary may live at the workspace path.
        exe_dir.join("../../binaries").join(&triple_name),
        exe_dir.join("../../binaries/ffmpeg"),
    ];

    for c in candidates.iter() {
        if c.exists() {
            return Some(c.clone());
        }
    }
    None
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn current_target_triple() -> &'static str {
    // Tauri's externalBin uses Rust's host triple. On Apple Silicon this is
    // aarch64-apple-darwin; on Intel macs it would be x86_64-apple-darwin.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(not(target_os = "macos"), target_arch = "aarch64"))]
    {
        "aarch64-unknown-linux-gnu"
    }
    #[cfg(all(not(target_os = "macos"), target_arch = "x86_64"))]
    {
        "x86_64-unknown-linux-gnu"
    }
}
