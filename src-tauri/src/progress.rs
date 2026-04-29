use serde::Serialize;

/// One progress update parsed from a yt-dlp stdout line.
#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub job_id: String,
    pub stage: String,
    pub percent: Option<f32>,
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub message: Option<String>,
}

/// We instruct yt-dlp to emit lines starting with this prefix using `--progress-template`.
pub const PROGRESS_PREFIX: &str = "RTPROG\t";

/// We instruct yt-dlp to print the final on-disk filepath after all postprocessing
/// using `--print after_move:RTDONE\t%(filepath)s`.
pub const DONE_PREFIX: &str = "RTDONE\t";

/// Try to parse a structured `RTPROG\t...` progress line into a ProgressEvent.
/// Returns None for lines that don't match the expected template.
pub fn parse_progress_line(job_id: &str, line: &str) -> Option<ProgressEvent> {
    let rest = line.strip_prefix(PROGRESS_PREFIX)?;
    let parts: Vec<&str> = rest.split('\t').collect();
    if parts.is_empty() {
        return None;
    }

    let status = parts.first().copied().unwrap_or("downloading").trim();
    let percent_raw = parts.get(1).copied().unwrap_or("");
    let speed = parts.get(2).map(|s| s.trim().to_string());
    let eta = parts.get(3).map(|s| s.trim().to_string());

    let percent = parse_percent(percent_raw);

    let stage = match status {
        "downloading" => "downloading",
        "finished" => "downloaded",
        "error" => "error",
        _ => "downloading",
    }
    .to_string();

    Some(ProgressEvent {
        job_id: job_id.to_string(),
        stage,
        percent,
        speed: clean(speed),
        eta: clean(eta),
        message: None,
    })
}

/// Best-effort parsing of yt-dlp's "informational" stdout lines that aren't
/// part of our structured progress template. This is what surfaces lines like
///   `[ExtractAudio] Destination: ...`
///   `[Merger] Merging formats into "..."`
///   `[ffmpeg] ...`
///
/// We surface them as a "postprocessing" stage so the UI can keep the user
/// informed even though yt-dlp doesn't emit numeric progress for these phases.
pub fn classify_info_line(job_id: &str, line: &str) -> Option<ProgressEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    // These are the lines worth surfacing.
    let interesting = trimmed.starts_with("[ExtractAudio]")
        || trimmed.starts_with("[Merger]")
        || trimmed.starts_with("[ffmpeg]")
        || trimmed.starts_with("[EmbedThumbnail]")
        || trimmed.starts_with("[Metadata]")
        || trimmed.starts_with("[Fixup");

    if !interesting {
        return None;
    }

    Some(ProgressEvent {
        job_id: job_id.to_string(),
        stage: "postprocessing".to_string(),
        percent: None,
        speed: None,
        eta: None,
        message: Some(trimmed.to_string()),
    })
}

fn parse_percent(s: &str) -> Option<f32> {
    let cleaned = s.trim().trim_end_matches('%').trim();
    if cleaned.is_empty() || cleaned == "NA" {
        return None;
    }
    cleaned.parse::<f32>().ok()
}

fn clean(s: Option<String>) -> Option<String> {
    s.and_then(|v| {
        let t = v.trim();
        if t.is_empty() || t == "NA" || t == "Unknown" {
            None
        } else {
            Some(t.to_string())
        }
    })
}
