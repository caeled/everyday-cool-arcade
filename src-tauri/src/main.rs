// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

/// Returns true when `url` belongs to the app itself and may load inside
/// the window: the Tauri production scheme (`tauri://localhost/...` on
/// macOS), the Windows production origin (`http://tauri.localhost/...`),
/// and inline schemes. Everything else is not the app's own content.
/// (Same test the main Everyday Cool app uses.)
fn is_app_url(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" => true,
        "http" | "https" => url.host_str() == Some("tauri.localhost"),
        "about" | "data" | "blob" => true,
        _ => false,
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let nav_handle = handle.clone();
            let new_window_handle = handle.clone();
            WebviewWindowBuilder::new(&handle, "main", WebviewUrl::App("index.html".into()))
                .title("Everyday Cool Arcade")
                .inner_size(1280.0, 900.0)
                .on_navigation(move |url| {
                    if is_app_url(url) {
                        // The arcade itself: the interface and the
                        // bundled game and lesson pages stay inside
                        // the window.
                        true
                    } else if url.scheme() == "http" || url.scheme() == "https" {
                        // An outside page (a lesson's Wikipedia link,
                        // the collection's own site, ...): hand it to
                        // the OS default browser instead of navigating
                        // the arcade away — the window has no back
                        // button, so an outside page would strand the
                        // player. The arcade stays exactly where it was.
                        let _ = nav_handle.opener().open_url(url.as_str(), None::<&str>);
                        false
                    } else {
                        true
                    }
                })
                .on_new_window(move |url, _features| {
                    // An external link that asks for its own window
                    // gets the same treatment: the OS browser, never
                    // a second in-app window.
                    if url.scheme() == "http" || url.scheme() == "https" {
                        let _ = new_window_handle
                            .opener()
                            .open_url(url.as_str(), None::<&str>);
                    }
                    tauri::webview::NewWindowResponse::Deny
                })
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Everyday Cool Arcade");
}
