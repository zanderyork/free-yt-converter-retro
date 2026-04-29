/* RetroTube — frontend logic.
 *
 * Single responsibility: read user input, talk to the Rust core via Tauri's
 * IPC, render progress and status. No business logic lives here.
 */
(function () {
  "use strict";

  // ---- Tauri global API hooks ---------------------------------------------
  // We've enabled `withGlobalTauri`, so the v2 API is exposed under window.__TAURI__.
  const tauri = window.__TAURI__ || {};
  const invoke =
    (tauri.core && tauri.core.invoke) ||
    function () {
      return Promise.reject(new Error("Tauri IPC unavailable"));
    };
  const listen =
    (tauri.event && tauri.event.listen) ||
    function () {
      return Promise.resolve(() => {});
    };
  const getCurrentWindow =
    (tauri.window && tauri.window.getCurrentWindow) ||
    function () {
      return null;
    };

  // ---- DOM refs -----------------------------------------------------------
  const $ = (sel) => document.querySelector(sel);
  const els = {
    url: $("#url"),
    formatRadios: () => Array.from(document.querySelectorAll('input[name="format"]')),
    quality: $("#quality"),
    progressWell: $("#progress-well"),
    statusLine: $("#status-line"),
    convertBtn: $("#btn-convert"),
    cancelBtn: $("#btn-cancel"),
    openFolderBtn: $("#btn-open-folder"),
    minBtn: $("#btn-minimize"),
    maxBtn: $("#btn-maximize"),
    closeBtn: $("#btn-close"),
    statusMain: $("#statusbar-main"),
    statusRight: $("#statusbar-right"),
  };

  // ---- Constants ----------------------------------------------------------
  const SEGMENT_COUNT = 32;
  const QUALITY_OPTIONS = {
    mp3: [
      { value: "best", label: "Best (≈190 kbps VBR)" },
    ],
    mp4: [
      { value: "best", label: "Best available" },
      { value: "2160", label: "4K (2160p)" },
      { value: "1440", label: "2K (1440p)" },
      { value: "1080", label: "1080p" },
      { value: "720", label: "720p" },
      { value: "480", label: "480p" },
      { value: "360", label: "360p" },
    ],
  };

  // ---- Mutable state ------------------------------------------------------
  let currentJobId = null;
  let segments = [];

  // ---- Init ---------------------------------------------------------------
  document.addEventListener("DOMContentLoaded", () => {
    buildSegments();
    populateQualityForFormat(currentFormat());
    wireFormatChange();
    wireButtons();
    wireWindowControls();
    wireKeyboard();
    setProgressPercent(0);
    setStatus("Ready", "idle");
    subscribeToConversionEvents();
    // Convert is the default button.
    els.convertBtn.classList.add("is-default");
    els.url.focus();
  });

  // ---- Segment construction ----------------------------------------------
  function buildSegments() {
    const frag = document.createDocumentFragment();
    segments = [];
    for (let i = 0; i < SEGMENT_COUNT; i++) {
      const seg = document.createElement("div");
      seg.className = "progress-segment";
      frag.appendChild(seg);
      segments.push(seg);
    }
    els.progressWell.innerHTML = "";
    els.progressWell.appendChild(frag);
  }

  function setProgressPercent(percent) {
    if (typeof percent !== "number" || isNaN(percent)) percent = 0;
    percent = Math.max(0, Math.min(100, percent));
    const filled = Math.floor((percent / 100) * SEGMENT_COUNT);
    for (let i = 0; i < segments.length; i++) {
      segments[i].classList.toggle("filled", i < filled);
    }
  }

  // ---- Form helpers -------------------------------------------------------
  function currentFormat() {
    const checked = els.formatRadios().find((r) => r.checked);
    return checked ? checked.value : "mp3";
  }

  function populateQualityForFormat(format) {
    const opts = QUALITY_OPTIONS[format] || QUALITY_OPTIONS.mp3;
    els.quality.innerHTML = "";
    for (const opt of opts) {
      const o = document.createElement("option");
      o.value = opt.value;
      o.textContent = opt.label;
      els.quality.appendChild(o);
    }
    els.quality.disabled = opts.length <= 1;
  }

  function wireFormatChange() {
    for (const r of els.formatRadios()) {
      r.addEventListener("change", () => {
        populateQualityForFormat(currentFormat());
      });
    }
  }

  // ---- Buttons ------------------------------------------------------------
  function wireButtons() {
    els.convertBtn.addEventListener("click", onConvertClicked);
    els.cancelBtn.addEventListener("click", onCancelClicked);
    els.openFolderBtn.addEventListener("click", onOpenFolderClicked);
  }

  async function onConvertClicked() {
    if (currentJobId) return; // already running
    const url = els.url.value.trim();
    if (!url) {
      setStatus("Paste a YouTube URL first.", "error");
      els.url.focus();
      return;
    }
    if (!/^https?:\/\//i.test(url)) {
      setStatus("URL must start with http(s)://", "error");
      els.url.focus();
      return;
    }

    const format = currentFormat();
    const quality = els.quality.value || "best";

    setProgressPercent(0);
    setStatus("Starting…", "info");
    setRightStatus("Working");
    setRunningUI(true);

    try {
      const jobId = await invoke("start_conversion", {
        request: { url, format, quality },
      });
      currentJobId = jobId;
    } catch (err) {
      currentJobId = null;
      setRunningUI(false);
      setStatus(formatError(err), "error");
      setRightStatus("Idle");
    }
  }

  async function onCancelClicked() {
    if (!currentJobId) return;
    const id = currentJobId;
    setStatus("Cancelling…", "info");
    try {
      await invoke("cancel_conversion", { jobId: id });
    } catch (err) {
      setStatus(formatError(err), "error");
    }
  }

  async function onOpenFolderClicked() {
    try {
      await invoke("open_output_folder");
    } catch (err) {
      setStatus(formatError(err), "error");
    }
  }

  // ---- Window control buttons --------------------------------------------
  function wireWindowControls() {
    const w = getCurrentWindow();
    if (!w) return;
    els.minBtn.addEventListener("click", () => w.minimize());
    els.closeBtn.addEventListener("click", () => w.close());
    // Maximize is intentionally disabled — fixed-size dialog.
  }

  function wireKeyboard() {
    // Enter inside URL field triggers Convert.
    els.url.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        onConvertClicked();
      }
    });
    // Escape cancels an in-flight job.
    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && currentJobId) {
        onCancelClicked();
      }
    });
  }

  // ---- Tauri IPC events ---------------------------------------------------
  function subscribeToConversionEvents() {
    listen("conversion-progress", (e) => {
      const p = e.payload || {};
      if (!currentJobId || p.job_id !== currentJobId) return;
      handleProgress(p);
    });
    listen("conversion-complete", (e) => {
      const p = e.payload || {};
      if (!currentJobId || p.job_id !== currentJobId) return;
      handleComplete(p);
    });
    listen("conversion-error", (e) => {
      const p = e.payload || {};
      if (!currentJobId || p.job_id !== currentJobId) return;
      handleError(p);
    });
    listen("conversion-cancelled", (e) => {
      const p = e.payload || {};
      if (!currentJobId || p.job_id !== currentJobId) return;
      handleCancelled(p);
    });
  }

  function handleProgress(p) {
    if (typeof p.percent === "number") {
      setProgressPercent(p.percent);
    }
    let line = "";
    switch (p.stage) {
      case "downloading":
        line = "Downloading";
        if (typeof p.percent === "number") line += " " + Math.round(p.percent) + "%";
        if (p.speed) line += " — " + p.speed;
        if (p.eta) line += " — ETA " + p.eta;
        break;
      case "downloaded":
        line = "Downloaded — preparing…";
        break;
      case "postprocessing":
        line = p.message ? compactPostprocess(p.message) : "Processing…";
        break;
      case "finished":
        line = "Finishing up…";
        setProgressPercent(100);
        break;
      default:
        line = p.message || p.stage || "Working";
    }
    setStatus(line, "info");
  }

  function handleComplete(p) {
    currentJobId = null;
    setProgressPercent(100);
    const path = p.file_path || "";
    if (path) {
      setStatus("Done: " + filenameOf(path), "success");
      setStatusBarPath(path);
    } else {
      setStatus("Done.", "success");
      setStatusBarPath("Saved to ~/Downloads/RetroTube/");
    }
    setRunningUI(false);
    setRightStatus("Done");
  }

  function handleError(p) {
    currentJobId = null;
    setRunningUI(false);
    setStatus("Error: " + truncate(p.message || "unknown error", 80), "error");
    setRightStatus("Error");
  }

  function handleCancelled() {
    currentJobId = null;
    setRunningUI(false);
    setProgressPercent(0);
    setStatus("Cancelled.", "info");
    setRightStatus("Idle");
  }

  // ---- UI state helpers ---------------------------------------------------
  function setRunningUI(running) {
    els.convertBtn.disabled = running;
    els.cancelBtn.disabled = !running;
    els.url.readOnly = running;
    els.quality.disabled = running || (QUALITY_OPTIONS[currentFormat()] || []).length <= 1;
    for (const r of els.formatRadios()) {
      r.disabled = running;
    }
  }

  function setStatus(text, kind) {
    els.statusLine.textContent = text;
    els.statusLine.classList.remove("is-error", "is-success");
    if (kind === "error") els.statusLine.classList.add("is-error");
    else if (kind === "success") els.statusLine.classList.add("is-success");
    els.statusMain.textContent = text;
  }

  function setRightStatus(text) {
    els.statusRight.textContent = text;
  }

  function setStatusBarPath(p) {
    els.statusMain.textContent = p;
  }

  // ---- Utilities ----------------------------------------------------------
  function filenameOf(path) {
    const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    return i >= 0 ? path.slice(i + 1) : path;
  }

  function truncate(s, n) {
    if (!s) return "";
    return s.length > n ? s.slice(0, n - 1) + "…" : s;
  }

  function formatError(err) {
    if (!err) return "unknown error";
    if (typeof err === "string") return truncate(err, 80);
    if (err.message) return truncate(err.message, 80);
    try { return truncate(JSON.stringify(err), 80); } catch (_) { return "unknown error"; }
  }

  function compactPostprocess(msg) {
    // yt-dlp tag prefixes are noisy. Strip the [Tag] and trim long file paths.
    const m = msg.match(/^\[(\w+)\]\s*(.*)$/);
    if (!m) return truncate(msg, 80);
    const tag = m[1];
    let rest = m[2] || "";
    // Replace embedded paths with just basenames.
    rest = rest.replace(/(["'])([^"']+\/)([^"']+)\1/g, "$1$3$1");
    return truncate(tag + ": " + rest, 80);
  }
})();
