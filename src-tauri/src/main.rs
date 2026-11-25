// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
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

/// ------------------------------------------------------------------
/// Clear Cache (File menu). The same cure as the main Everyday Cool
/// app's v1.1.6 item: a persisted webview profile can keep serving
/// pre-update pages — a room's service worker answers cache-first out
/// of Cache Storage, and neither reinstalling nor restarting clears
/// that profile.
///
/// The item cannot clear the cache in place: the running webview owns
/// those files. So the click writes a flag file and restarts the app,
/// and at the very top of setup — before any window (and therefore any
/// webview) exists — the flag is consumed: only the cache-bearing
/// subdirectories of the profile are deleted (HTTP cache, code cache,
/// GPU cache, and the Service Worker directory, which holds the
/// service worker registrations and their Cache Storage). Local
/// Storage and IndexedDB are never touched, so game progress and
/// preferences survive a Clear Cache. The Arcade keeps no per-user
/// data, so there is exactly one profile to clear.
static CACHE_JUST_CLEARED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

const CLEAR_CACHE_FLAG: &str = "clear-cache.flag";

/// Path of a small flag file in the app local data directory.
fn flag_path(handle: &AppHandle, name: &str) -> Option<std::path::PathBuf> {
    handle
        .path()
        .app_local_data_dir()
        .ok()
        .map(|dir| dir.join(name))
}

fn write_flag(handle: &AppHandle, name: &str) {
    if let Some(flag) = flag_path(handle, name) {
        if let Some(parent) = flag.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&flag, b"1");
    }
}

/// File → Clear Cache: leave the flag for the next launch, then
/// restart. The deletion happens in setup of the new process, before
/// its webview is created (see consume_clear_cache_flag).
fn request_clear_cache(handle: &AppHandle) {
    write_flag(handle, CLEAR_CACHE_FLAG);
    handle.restart();
}

/// Consume a Clear Cache flag left by the previous run. Returns true
/// when a flag was present (the caller then arms the "cache cleared"
/// notice for the main window). Deletion is best-effort with a short
/// retry: the previous process may still be releasing file locks in
/// the first moments after a restart.
fn consume_clear_cache_flag(handle: &AppHandle) -> bool {
    let Some(flag) = flag_path(handle, CLEAR_CACHE_FLAG) else {
        return false;
    };
    if !flag.exists() {
        return false;
    }
    let _ = std::fs::remove_file(&flag);

    let mut targets: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(data_dir) = handle.path().app_local_data_dir() {
        // The WebView2 profile's Default folder lives under
        // <app local data>/EBWebView. Only cache directories are
        // listed — Local Storage and IndexedDB live in sibling
        // directories and are preserved.
        let profile = data_dir.join("EBWebView").join("Default");
        for sub in ["Service Worker", "Cache", "Code Cache", "GPUCache"] {
            targets.push(profile.join(sub));
        }
    }
    #[cfg(target_os = "macos")]
    {
        // WKWebView has no EBWebView; the app's system cache container
        // (~/Library/Caches/<bundle id>) is pure cache and regenerates.
        if let Ok(home) = handle.path().home_dir() {
            targets.push(
                home.join("Library")
                    .join("Caches")
                    .join(handle.config().identifier.as_str()),
            );
        }
    }
    for target in targets {
        for _ in 0..5 {
            if !target.exists() || std::fs::remove_dir_all(&target).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    }
    true
}

/// In-page notice shown once on the main window after a Clear Cache
/// restart, so the click never feels dead. Injected from Rust; the
/// bundled pages are untouched.
const CACHE_CLEARED_NOTICE_JS: &str = "(() => { const d = document.createElement('div'); d.textContent = 'Cache cleared \u{2014} the arcade now loads fresh copies of every game. Your saved progress was not touched.'; d.style.cssText = 'position:fixed;top:14px;left:50%;transform:translateX(-50%);z-index:99999;background:#1d3b2a;color:#eaf5ec;padding:10px 18px;border-radius:10px;font:14px system-ui,sans-serif;box-shadow:0 4px 18px rgba(0,0,0,.35)'; document.body.appendChild(d); setTimeout(() => d.remove(), 8000); })();";

/// Navigate the main window to a bundled page at the app root
/// (documents.html, about.html): resolve the path against the window's
/// current URL so the jump works from any game. It is an app URL, so
/// the navigation handler keeps it inside the window.
fn navigate_main(app: &AppHandle, path: &str) {
    if let Some(win) = app.get_webview_window("main") {
        if let Ok(current) = win.url() {
            if let Ok(target) = current.join(path) {
                let _ = win.navigate(target);
            }
        }
        let _ = win.set_focus();
    }
}

/// ------------------------------------------------------------------
/// The Baton Box (Steven's direction, 2026-11-24: the installer must
/// carry a way to start the relay). The box is a small stdlib-only
/// Python server, bundled as app resources (src-tauri/resources/
/// baton-box/, refreshed from the standalone checkout by
/// tools/sync-dist.sh). It serves the DOS Console with the
/// cross-origin isolation headers that console's core needs, and it
/// relays the pass-and-play games to the home network.
///
/// The installed resources live somewhere read-only (Program Files),
/// but the box writes sessions.json beside its script — so the app
/// runs a working copy from its local data directory, refreshed
/// whenever the bundled server file changes. The refresh copies file
/// by file and never touches sessions.json, so the family's games
/// survive app updates.
struct BatonState {
    child: std::sync::Mutex<Option<std::process::Child>>,
}

#[derive(serde::Serialize)]
struct BatonStatus {
    running: bool,
    url: String,
    message: String,
}

const BATON_URL: &str = "http://127.0.0.1:8765/";

/// Locate the bundled Baton Box inside the installed resources. The
/// resource layout has varied across Tauri versions, so try the known
/// shapes and say which one answered (the answer rides back in the
/// start error if the box dies instantly, where Steven can see it).
fn baton_resource_dir(handle: &AppHandle) -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(dir) = handle.path().resource_dir() {
        candidates.push(dir.join("resources").join("baton-box"));
        candidates.push(dir.join("baton-box"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("resources").join("baton-box"));
            candidates.push(dir.join("baton-box"));
        }
    }
    candidates
        .into_iter()
        .find(|d| d.join("baton_box.py").is_file())
}

fn copy_tree(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "__pycache__" || name == "sessions.json" {
            continue;
        }
        let from = entry.path();
        let to = dest.join(&name);
        if entry.file_type()?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// The working copy of the box, in the app's local data directory.
/// Refreshed only when the bundled baton_box.py differs from the copy
/// on disk, so a running family's sessions are never disturbed.
fn baton_runtime_dir(handle: &AppHandle) -> Result<std::path::PathBuf, String> {
    let src = baton_resource_dir(handle).ok_or_else(|| {
        "The bundled Baton Box files could not be found in this install.".to_string()
    })?;
    let dest = handle
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("baton-box");
    let bundled = std::fs::read(src.join("baton_box.py")).map_err(|e| e.to_string())?;
    let current = std::fs::read(dest.join("baton_box.py")).unwrap_or_default();
    if bundled != current {
        copy_tree(&src, &dest).map_err(|e| e.to_string())?;
    }
    Ok(dest)
}

/// Find a Python 3 on this machine: the py launcher first on Windows
/// (it is what the python.org installer registers), then python /
/// python3 on either platform. The box is stdlib-only, so any Python 3
/// the standalone box would run on is good enough here.
fn find_python() -> Option<String> {
    #[cfg(windows)]
    let names = ["py", "python", "python3"];
    #[cfg(not(windows))]
    let names = ["python3", "python"];
    for name in names {
        let ok = std::process::Command::new(name)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Some(name.to_string());
        }
    }
    None
}

/// Is anything already answering on the box's port? (A box Steven
/// started by hand counts: the shelf reports it as running rather
/// than launching a second one into a port fight.)
fn baton_port_up() -> bool {
    let Ok(addr) = "127.0.0.1:8765".parse::<std::net::SocketAddr>() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(300)).is_ok()
}

/// The current truth about the box: our child still alive, or the
/// port answering. A child that died on its own is reaped here.
fn baton_snapshot(state: &BatonState) -> bool {
    let mut guard = state.child.lock().unwrap();
    if let Some(child) = guard.as_mut() {
        match child.try_wait() {
            Ok(None) => return true,
            _ => *guard = None,
        }
    }
    baton_port_up()
}

#[tauri::command]
fn baton_status(state: tauri::State<'_, BatonState>) -> BatonStatus {
    let running = baton_snapshot(&state);
    BatonStatus {
        running,
        url: BATON_URL.into(),
        message: if running {
            "The Baton Box is running on this computer."
        } else {
            "The Baton Box is stopped."
        }
        .into(),
    }
}

#[tauri::command]
fn baton_start(
    handle: AppHandle,
    state: tauri::State<'_, BatonState>,
) -> Result<BatonStatus, String> {
    {
        let mut guard = state.child.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(None) => {
                    return Ok(BatonStatus {
                        running: true,
                        url: BATON_URL.into(),
                        message: "The Baton Box is already running.".into(),
                    })
                }
                _ => *guard = None,
            }
        }
    }
    if baton_port_up() {
        return Ok(BatonStatus {
            running: true,
            url: BATON_URL.into(),
            message: "Something is already answering on the box's port — a Baton Box started outside the app is probably already running.".into(),
        });
    }
    let dir = baton_runtime_dir(&handle)?;
    let python = find_python().ok_or_else(|| {
        "Python was not found on this computer — the Baton Box needs Python 3, the same one the standalone box uses.".to_string()
    })?;
    let mut cmd = std::process::Command::new(&python);
    cmd.arg("baton_box.py")
        .current_dir(&dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        // No console window flashing over the arcade: the box is a
        // background server, and its face is the browser page.
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start Python: {e}"))?;
    // Give it a moment to bind the port; if it died instantly, say so
    // plainly and name the folder where running it by hand shows why.
    std::thread::sleep(std::time::Duration::from_millis(700));
    match child.try_wait() {
        Ok(Some(status)) => Err(format!(
            "The Baton Box stopped as soon as it started ({status}). Its files live in {} — running baton_box.py there by hand will show the error.",
            dir.display()
        )),
        _ => {
            *state.child.lock().unwrap() = Some(child);
            Ok(BatonStatus {
                running: true,
                url: BATON_URL.into(),
                message: "The Baton Box is running on this computer. Open the box to host or join a game.".into(),
            })
        }
    }
}

#[tauri::command]
fn baton_stop(state: tauri::State<'_, BatonState>) -> BatonStatus {
    let mut killed = false;
    {
        let mut guard = state.child.lock().unwrap();
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
            killed = true;
        }
    }
    if baton_port_up() {
        BatonStatus {
            running: true,
            url: BATON_URL.into(),
            message: "The app's box is stopped, but something is still answering on the port — a Baton Box started outside the app is still running.".into(),
        }
    } else {
        BatonStatus {
            running: false,
            url: BATON_URL.into(),
            message: if killed {
                "The Baton Box is stopped."
            } else {
                "The Baton Box was not running."
            }
            .into(),
        }
    }
}

#[tauri::command]
fn baton_open(handle: AppHandle) -> Result<(), String> {
    handle
        .opener()
        .open_url(BATON_URL, None::<&str>)
        .map_err(|e| e.to_string())
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(BatonState {
            child: std::sync::Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            baton_start,
            baton_stop,
            baton_status,
            baton_open
        ])
        .menu(|handle| {
            // The main app's File | Help shape, adapted: the Arcade
            // keeps no per-user data, so the File menu carries only
            // Clear Cache and Exit; Help carries the Game Documents
            // page (every game's lesson, generated from the manifest
            // at sync time), the repository, and About.
            let clear_cache =
                MenuItem::with_id(handle, "clear-cache", "Clear Cache", true, None::<&str>)?;
            let game_documents = MenuItem::with_id(
                handle,
                "game-documents",
                "Game Documents",
                true,
                None::<&str>,
            )?;
            let github = MenuItem::with_id(
                handle,
                "github",
                "GitHub Repository",
                true,
                None::<&str>,
            )?;
            let about = MenuItem::with_id(
                handle,
                "about",
                "About Everyday Cool Arcade",
                true,
                None::<&str>,
            )?;
            let sep = PredefinedMenuItem::separator(handle)?;
            let help = Submenu::with_items(
                handle,
                "Help",
                true,
                &[&game_documents, &github, &sep, &about],
            )?;
            #[cfg(target_os = "macos")]
            {
                // macOS convention: the first submenu is the app menu,
                // and quitting lives there as the predefined Quit item.
                // Clear Cache rides in the app menu, above Quit — the
                // macOS counterpart of its File-menu place elsewhere.
                let quit = PredefinedMenuItem::quit(handle, None::<&str>)?;
                let app_sep = PredefinedMenuItem::separator(handle)?;
                let app_menu = Submenu::with_items(
                    handle,
                    "Everyday Cool Arcade",
                    true,
                    &[&clear_cache, &app_sep, &quit],
                )?;
                Menu::with_items(handle, &[&app_menu, &help])
            }
            #[cfg(not(target_os = "macos"))]
            {
                let exit = MenuItem::with_id(handle, "exit", "Exit", true, None::<&str>)?;
                let file_sep = PredefinedMenuItem::separator(handle)?;
                let file = Submenu::with_items(
                    handle,
                    "File",
                    true,
                    &[&clear_cache, &file_sep, &exit],
                )?;
                Menu::with_items(handle, &[&file, &help])
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "exit" => app.exit(0),
            "clear-cache" => request_clear_cache(app),
            "game-documents" => navigate_main(app, "/documents.html"),
            "github" => {
                // The arcade's source repository; like every external
                // link in this app, it opens in the OS browser.
                let _ = app.opener().open_url(
                    "https://github.com/caeled/everyday-cool-arcade",
                    None::<&str>,
                );
            }
            "about" => navigate_main(app, "/about.html"),
            _ => {}
        })
        .on_page_load(|webview, payload| {
            // If this launch consumed a Clear Cache flag, say so on
            // the page once it has loaded (once only).
            if webview.label() == "main"
                && matches!(payload.event(), PageLoadEvent::Finished)
                && CACHE_JUST_CLEARED.swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                let _ = webview.eval(CACHE_CLEARED_NOTICE_JS);
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();
            // Clear Cache lands here, before any window — and therefore
            // any webview — exists in this process: if the previous run
            // left the flag, delete the profile's cache directories now
            // (nothing holds their files yet) and arm the notice that
            // tells the user it happened once the shelf shows.
            if consume_clear_cache_flag(&handle) {
                CACHE_JUST_CLEARED.store(true, std::sync::atomic::Ordering::SeqCst);
            }
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
        .build(tauri::generate_context!())
        .expect("error while running Everyday Cool Arcade");
    app.run(|handle, event| {
        // The box belongs to the arcade evening: when the app goes
        // away (including the Clear Cache restart), the box it
        // launched goes with it. A box Steven started by hand is
        // untouched — only this process's own child is reaped.
        if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
            let state = handle.state::<BatonState>();
            let mut guard = state.child.lock().unwrap();
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
            }
        }
    });
}
