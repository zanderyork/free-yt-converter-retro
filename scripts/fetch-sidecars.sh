#!/usr/bin/env bash
# fetch-sidecars.sh — download ARM64 yt-dlp and ffmpeg binaries into
# src-tauri/binaries/ with the target-triple suffix Tauri expects.
#
# Idempotent: skips downloads if files already exist. Pass --force to refresh.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="$ROOT/src-tauri/binaries"
TRIPLE="aarch64-apple-darwin"

YTDLP_DST="$BIN_DIR/yt-dlp-$TRIPLE"
FFMPEG_DST="$BIN_DIR/ffmpeg-$TRIPLE"

FORCE=false
if [[ "${1:-}" == "--force" ]]; then
  FORCE=true
fi

mkdir -p "$BIN_DIR"

echo "==> Target binary directory: $BIN_DIR"

# ---- yt-dlp ----
if $FORCE || [[ ! -x "$YTDLP_DST" ]]; then
  echo "==> Fetching yt-dlp (macos universal build)…"
  YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos"
  curl -L --fail --progress-bar -o "$YTDLP_DST" "$YTDLP_URL"
  chmod +x "$YTDLP_DST"
  echo "    ✓ $YTDLP_DST"
else
  echo "==> yt-dlp already present at $YTDLP_DST (skip; pass --force to refresh)"
fi

# ---- ffmpeg ----
# We need a real ARM64 build (not Intel/Rosetta) so M1 hardware acceleration
# (VideoToolbox) lights up. osxexperts.net hosts canonical static ARM64 builds.
if $FORCE || [[ ! -x "$FFMPEG_DST" ]] || ! file "$FFMPEG_DST" | grep -q arm64; then
  echo "==> Fetching ffmpeg (Apple Silicon static build from osxexperts.net)…"
  TMP="$(mktemp -d)"
  trap 'rm -rf "$TMP"' EXIT
  FFMPEG_URL="https://www.osxexperts.net/ffmpeg81arm.zip"
  curl -L --fail --progress-bar -o "$TMP/ffmpeg.zip" "$FFMPEG_URL"
  unzip -q "$TMP/ffmpeg.zip" -d "$TMP"
  if [[ ! -f "$TMP/ffmpeg" ]]; then
    echo "ERROR: extracted archive did not contain an 'ffmpeg' executable" >&2
    exit 1
  fi
  if ! file "$TMP/ffmpeg" | grep -q arm64; then
    echo "WARNING: extracted ffmpeg does not look like an arm64 binary" >&2
  fi
  mv "$TMP/ffmpeg" "$FFMPEG_DST"
  chmod +x "$FFMPEG_DST"
  echo "    ✓ $FFMPEG_DST"
else
  echo "==> ffmpeg already present at $FFMPEG_DST (skip; pass --force to refresh)"
fi

# Strip macOS quarantine attribute so binaries run cleanly inside the bundle.
xattr -d com.apple.quarantine "$YTDLP_DST" 2>/dev/null || true
xattr -d com.apple.quarantine "$FFMPEG_DST" 2>/dev/null || true

echo
echo "==> Versions:"
"$YTDLP_DST" --version || true
"$FFMPEG_DST" -version 2>&1 | head -1 || true
echo
echo "All sidecars in place."
