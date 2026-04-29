//! Conversion engine: builds yt-dlp arguments, spawns the subprocess, parses
//! progress, emits events, and tracks the running child for cancellation.

use crate::paths;
use crate::progress::{
    classify_info_line, parse_progress_line, ProgressEvent, DONE_PREFIX, PROGRESS_PREFIX,
};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::process::{CommandEvent, TerminatedPayload};
use tauri_plugin_shell::ShellExt;

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Deserialize)]
pub struct ConversionRequest {
    pub url: String,
    /// "mp3" or "mp4"
    pub format: String,
    /// "best", "2160", "1440", "1080", "720", "480", "360"
    pub quality: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversionStarted {
    pub job_id: String,
    pub url: String,
    pub format: String,
    pub quality: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversionComplete {
    pub job_id: String,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversionError {
    pub job_id: String,
    pub message: String,
}

/// Public entry point invoked from the Tauri command. Spawns yt-dlp and a
/// monitoring task; returns immediately with a fresh `job_id`.
pub async fn start(app: AppHandle, request: ConversionRequest) -> Result<String, String> {
    let url = request.url.trim().to_string();
    if url.is_empty() {
        return Err("URL is empty".into());
    }
    let parsed = url::Url::parse(&url).map_err(|e| format!("Invalid URL: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("URL must use http or https".into());
    }

    let format = request.format.to_lowercase();
    if format != "mp3" && format != "mp4" {
        return Err(format!("Unsupported format: {format}"));
    }
    let quality = request.quality.to_lowercase();

    let output_dir = paths::ensure_output_directory()
        .map_err(|e| format!("Cannot create output dir: {e}"))?;

    let ffmpeg_path = paths::ffmpeg_sidecar_path().ok_or_else(|| {
        "Could not locate bundled ffmpeg binary. \
         Run `scripts/fetch-sidecars.sh` to download it."
            .to_string()
    })?;

    let job_id = next_job_id();
    let args = build_args(&url, &format, &quality, &output_dir, &ffmpeg_path);

    let shell = app.shell();
    let sidecar = shell
        .sidecar("yt-dlp")
        .map_err(|e| format!("Sidecar lookup failed: {e}"))?
        .args(args);

    let (rx, child) = sidecar
        .spawn()
        .map_err(|e| format!("Failed to launch yt-dlp: {e}"))?;

    {
        let state = app.state::<AppState>();
        let mut jobs = state.jobs.lock().map_err(|_| "state lock poisoned".to_string())?;
        jobs.insert(job_id.clone(), child);
    }

    let started = ConversionStarted {
        job_id: job_id.clone(),
        url,
        format: format.clone(),
        quality,
    };
    let _ = app.emit("conversion-started", started);

    let app_for_task = app.clone();
    let job_id_for_task = job_id.clone();
    tauri::async_runtime::spawn(async move {
        monitor(app_for_task, job_id_for_task, rx).await;
    });

    Ok(job_id)
}

/// Cancel a running conversion by job_id. Returns Ok even if the job has
/// already finished — cancellation is best-effort and idempotent.
pub fn cancel(app: &AppHandle, job_id: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let removed = {
        let mut jobs = state
            .jobs
            .lock()
            .map_err(|_| "state lock poisoned".to_string())?;
        jobs.remove(job_id)
    };
    if let Some(child) = removed {
        let _ = child.kill();
        let _ = app.emit(
            "conversion-cancelled",
            serde_json::json!({ "job_id": job_id }),
        );
    }
    Ok(())
}

async fn monitor(
    app: AppHandle,
    job_id: String,
    mut rx: tauri::async_runtime::Receiver<CommandEvent>,
) {
    let mut stderr_tail: Vec<String> = Vec::new();
    let mut final_path: Option<String> = None;

    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stdout(bytes) => {
                let line = String::from_utf8_lossy(&bytes).to_string();
                handle_stdout_line(&app, &job_id, &line, &mut final_path);
            }
            CommandEvent::Stderr(bytes) => {
                let line = String::from_utf8_lossy(&bytes).to_string();
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    // yt-dlp prints WARNINGS and ERRORS on stderr. Keep a
                    // bounded tail so the user sees a useful message on failure.
                    stderr_tail.push(trimmed.to_string());
                    if stderr_tail.len() > 8 {
                        stderr_tail.remove(0);
                    }
                }
            }
            CommandEvent::Error(err) => {
                stderr_tail.push(format!("subprocess error: {err}"));
            }
            CommandEvent::Terminated(payload) => {
                handle_terminated(&app, &job_id, payload, &stderr_tail, final_path.clone());
                break;
            }
            _ => {}
        }
    }
}

fn handle_stdout_line(
    app: &AppHandle,
    job_id: &str,
    raw: &str,
    final_path: &mut Option<String>,
) {
    // yt-dlp can flush multiple progress updates in one read — split on newline
    // so we never miss an event.
    for line in raw.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix(DONE_PREFIX) {
            let path = rest.trim().to_string();
            if !path.is_empty() {
                *final_path = Some(path);
            }
            continue;
        }

        if line.starts_with(PROGRESS_PREFIX) {
            if let Some(ev) = parse_progress_line(job_id, line) {
                let _ = app.emit("conversion-progress", ev);
            }
            continue;
        }

        if let Some(ev) = classify_info_line(job_id, line) {
            let _ = app.emit("conversion-progress", ev);
        }
    }
}

fn handle_terminated(
    app: &AppHandle,
    job_id: &str,
    payload: TerminatedPayload,
    stderr_tail: &[String],
    final_path: Option<String>,
) {
    let state = app.state::<AppState>();
    let was_registered = {
        let mut jobs = state.jobs.lock().expect("state lock poisoned");
        jobs.remove(job_id).is_some()
    };

    if !was_registered {
        // Cancelled — `cancel()` already removed it and emitted the event.
        return;
    }

    let exit_ok = payload.code.unwrap_or(-1) == 0;
    if exit_ok {
        let ev = ProgressEvent {
            job_id: job_id.to_string(),
            stage: "finished".to_string(),
            percent: Some(100.0),
            speed: None,
            eta: None,
            message: Some("Done".to_string()),
        };
        let _ = app.emit("conversion-progress", ev);
        let _ = app.emit(
            "conversion-complete",
            ConversionComplete {
                job_id: job_id.to_string(),
                file_path: final_path,
            },
        );
    } else {
        let summary = if stderr_tail.is_empty() {
            format!("yt-dlp exited with code {}", payload.code.unwrap_or(-1))
        } else {
            stderr_tail.join("\n")
        };
        let _ = app.emit(
            "conversion-error",
            ConversionError {
                job_id: job_id.to_string(),
                message: summary,
            },
        );
    }
}

fn next_job_id() -> String {
    let n = JOB_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("job-{n}")
}

/// Build the yt-dlp argument vector for a given conversion request.
fn build_args(
    url: &str,
    format: &str,
    quality: &str,
    output_dir: &Path,
    ffmpeg_path: &Path,
) -> Vec<String> {
    let mut args: Vec<String> = Vec::with_capacity(40);

    // ----- shared flags (apply to every conversion) -----
    args.push("--no-playlist".into());
    args.push("--no-write-info-json".into());
    args.push("--no-write-description".into());
    args.push("--no-write-thumbnail".into());
    args.push("--no-write-comments".into());
    // Force one progress line per update so stdout parsing stays robust.
    args.push("--newline".into());
    // Quieter informational chatter without fully going silent.
    args.push("--no-warnings".into());
    args.push("--progress".into());
    args.push("--progress-template".into());
    args.push(format!(
        "{prefix}%(progress.status)s\t%(progress._percent_str)s\t%(progress._speed_str)s\t%(progress._eta_str)s\t%(progress.downloaded_bytes)s\t%(progress.total_bytes)s",
        prefix = PROGRESS_PREFIX
    ));
    // Ask yt-dlp to print the final on-disk path after all postprocessing.
    args.push("--print".into());
    args.push(format!("after_move:{DONE_PREFIX}%(filepath)s"));
    args.push("--ffmpeg-location".into());
    args.push(ffmpeg_path.to_string_lossy().to_string());
    args.push("--paths".into());
    args.push(format!("home:{}", output_dir.display()));
    args.push("--output".into());
    args.push("%(title).200B [%(id)s].%(ext)s".into());
    // Concurrent fragment downloads accelerate HLS/DASH considerably.
    args.push("--concurrent-fragments".into());
    args.push("4".into());

    // ----- format-specific arguments -----
    match format {
        "mp3" => {
            // Audio-only fast path: never download the video stream.
            args.push("-f".into());
            args.push("bestaudio[ext=m4a]/bestaudio/best".into());
            args.push("-x".into());
            args.push("--audio-format".into());
            args.push("mp3".into());
            // VBR ~190 kbps sweet spot (LAME -V2 equivalent).
            args.push("--audio-quality".into());
            args.push("2".into());
            args.push("--embed-metadata".into());
            args.push("--embed-thumbnail".into());
        }
        "mp4" => {
            let height_filter = match quality {
                "2160" => "[height<=2160]",
                "1440" => "[height<=1440]",
                "1080" => "[height<=1080]",
                "720" => "[height<=720]",
                "480" => "[height<=480]",
                "360" => "[height<=360]",
                _ => "",
            };
            // Prefer streams that mux without re-encoding (mp4+m4a). Fall back
            // to anything that fits the height cap if a clean mp4 isn't available.
            let format_string = format!(
                "bestvideo{h}[ext=mp4]+bestaudio[ext=m4a]/bestvideo{h}+bestaudio/best{h}",
                h = height_filter
            );
            args.push("-f".into());
            args.push(format_string);
            args.push("--merge-output-format".into());
            args.push("mp4".into());
            args.push("--embed-metadata".into());
            // When yt-dlp DOES need to re-encode (rare), use the M1 hardware
            // encoder via VideoToolbox for a substantial speed boost.
            args.push("--postprocessor-args".into());
            args.push(
                "VideoConvertor:-c:v h264_videotoolbox -b:v 6M -c:a aac -b:a 192k".into(),
            );
        }
        _ => {}
    }

    args.push(url.to_string());
    args
}
