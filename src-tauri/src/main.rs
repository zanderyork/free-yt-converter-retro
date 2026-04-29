// Prevents an extra console window from appearing on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    retro_yt_mp3_lib::run();
}
