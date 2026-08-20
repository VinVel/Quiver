// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
fn configure_webkit() {
    // SAFETY: This runs before Tauri or any application threads are started.
    unsafe {
        if std::env::var_os("APPIMAGE").is_some() || std::env::var_os("APPDIR").is_some() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        } else {
            std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
        }
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    configure_webkit();

    let _ = fix_path_env::fix();
    quiver_lib::run();
}
