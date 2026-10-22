//! scorebug desktop shell.
//!
//! A feed thread polls a source (ESPN, or the simulator with `--sim`), runs
//! the tracker and emits each poll to the web UI as a `frame` event. Two
//! windows load the same UI: `ticker` (transparent, borderless, always on top,
//! along the bottom of the primary monitor) and `grid` (normal window, opened
//! from the tray).
//!
//! `scorebug [--sim [--seed N] [--warp X]] [--date YYYYMMDD] [--poll SECS] [--grid]`

mod config;
mod feed;

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use config::Config;
use feed::{Feed, Source};

/// Ticker bar height in CSS pixels; matches `--tk-height` in the UI.
const TICKER_HEIGHT: f64 = 88.0;

fn main() {
    linux_env();
    let opts = match Source::from_args(std::env::args().skip(1)) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!(
                "{msg}\nusage: scorebug [--sim [--seed N] [--warp X]] [--date YYYYMMDD] [--poll SECS] [--grid]"
            );
            std::process::exit(2);
        }
    };

    tauri::Builder::default()
        .manage(Feed::default())
        .manage(ClickThrough(AtomicBool::new(true)))
        .invoke_handler(tauri::generate_handler![
            feed::current_frame,
            feed::feed_status
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            ticker_window(&handle)?;
            let grid = grid_window(&handle)?;
            if opts.grid {
                grid.show()?;
            }
            tray(&handle)?;
            let config = handle
                .path()
                .config_dir()
                .map(|dir| Config::load_or_init(&Config::path(&dir)))
                .unwrap_or_default();
            feed::spawn(
                handle,
                opts.source.clone(),
                std::time::Duration::from_secs(config.poll_secs),
                config.nba,
                config.nfl,
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running scorebug");
}

/// Must run before GTK starts.
fn linux_env() {
    #[cfg(target_os = "linux")]
    {
        let set_default = |k: &str, v: &str| {
            if std::env::var_os(k).is_none() {
                // SAFETY: called first thing in main, before any threads exist.
                unsafe { std::env::set_var(k, v) };
            }
        };
        // WebKitGTK's DMA-BUF renderer misbehaves on the proprietary NVIDIA driver.
        set_default("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        // Wayland gives clients no way to place a window or keep it on top, so
        // the overlay runs through XWayland. This overrides GDK_BACKEND because
        // desktops such as COSMIC export `wayland,x11` session-wide;
        // `SCOREBUG_GDK_BACKEND` picks a backend explicitly.
        let backend = std::env::var("SCOREBUG_GDK_BACKEND")
            .ok()
            .or_else(|| std::env::var_os("WAYLAND_DISPLAY").map(|_| "x11".to_string()));
        if let Some(b) = backend {
            // SAFETY: as above.
            unsafe { std::env::set_var("GDK_BACKEND", b) };
        }
    }
}

struct ClickThrough(AtomicBool);

fn ticker_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let width = app
        .primary_monitor()?
        .map_or(1920.0, |m| f64::from(m.size().width) / m.scale_factor());
    let win = WebviewWindowBuilder::new(app, "ticker", WebviewUrl::App("index.html".into()))
        .title("scorebug ticker")
        .inner_size(width, TICKER_HEIGHT)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        // Not `resizable(false)`: GTK then sizes the window to the webview's
        // natural height (200px) and ignores ours. Unmanaged, it can't be
        // resized by the user anyway.
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .focused(false)
        .visible(false)
        .build()?;
    #[cfg(target_os = "linux")]
    unmanage(&win)?;
    place_ticker(&win)?;
    show_ticker(&win, true)?;
    Ok(win)
}

/// Full width along the bottom edge of the primary (or current) monitor.
fn place_ticker(win: &WebviewWindow) -> tauri::Result<()> {
    let Some(mon) = win.primary_monitor()?.or(win.current_monitor()?) else {
        return Ok(());
    };
    let scale = mon.scale_factor();
    let (pos, size) = (mon.position(), mon.size());
    win.set_size(LogicalSize::new(
        f64::from(size.width) / scale,
        TICKER_HEIGHT,
    ))?;
    let height = (TICKER_HEIGHT * scale).round() as i32;
    win.set_position(PhysicalPosition::new(
        pos.x,
        pos.y + size.height as i32 - height,
    ))?;
    Ok(())
}

/// Click-through can only be set once the window is realized, i.e. shown
/// (tao panics otherwise), so it is (re)applied on every show.
fn show_ticker(win: &WebviewWindow, click_through: bool) -> tauri::Result<()> {
    win.show()?;
    // GTK may apply its default size on first map; nothing else moves an
    // unmanaged window, so placing it again sticks.
    place_ticker(win)?;
    win.set_ignore_cursor_events(click_through)
}

/// Takes the ticker out of the window manager's hands. Managed windows get
/// clamped and re-centered on map (COSMIC's XWayland WM does both, even for
/// docks); an override-redirect window sits exactly where it is put, above
/// managed windows, like a menu or tooltip. Must happen before the first map.
#[cfg(target_os = "linux")]
fn unmanage(win: &WebviewWindow) -> tauri::Result<()> {
    use gtk::prelude::{GtkWindowExt, WidgetExt};
    let gtk_win = win.gtk_window()?;
    gtk_win.set_type_hint(gtk::gdk::WindowTypeHint::Dock);
    if !gtk_win.is_realized() {
        gtk_win.realize();
    }
    if let Some(gdk_win) = gtk_win.window() {
        gdk_win.set_override_redirect(true);
    }
    Ok(())
}

fn grid_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let win = WebviewWindowBuilder::new(app, "grid", WebviewUrl::App("index.html".into()))
        .title("scorebug")
        .inner_size(1280.0, 800.0)
        .min_inner_size(320.0, 240.0)
        .visible(false)
        .build()?;
    // Closing the grid hides it; the app lives in the tray.
    let w = win.clone();
    win.on_window_event(move |e| {
        if let WindowEvent::CloseRequested { api, .. } = e {
            api.prevent_close();
            let _ = w.hide();
        }
    });
    Ok(win)
}

fn tray(app: &AppHandle) -> tauri::Result<()> {
    let grid = MenuItem::with_id(app, "grid", "Show grid", true, None::<&str>)?;
    let ticker = CheckMenuItem::with_id(app, "ticker", "Ticker", true, true, None::<&str>)?;
    let click = CheckMenuItem::with_id(
        app,
        "click",
        "Click-through ticker",
        true,
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&grid, &ticker, &click, &sep, &quit])?;

    let icon = app.default_window_icon().cloned();
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("scorebug")
        .menu(&menu)
        .on_menu_event(|app, e| {
            let win = |label| app.get_webview_window(label);
            match e.id().as_ref() {
                "grid" => {
                    if let Some(w) = win("grid") {
                        let _ = w.show();
                        let _ = w.unminimize();
                        let _ = w.set_focus();
                    }
                }
                "ticker" => {
                    if let Some(w) = win("ticker") {
                        if w.is_visible().unwrap_or(false) {
                            let _ = w.hide();
                        } else {
                            let _ = place_ticker(&w);
                            let on = app.state::<ClickThrough>().0.load(Ordering::SeqCst);
                            let _ = show_ticker(&w, on);
                        }
                    }
                }
                "click" => {
                    let on = !app
                        .state::<ClickThrough>()
                        .0
                        .fetch_xor(true, Ordering::SeqCst);
                    if let Some(w) = win("ticker").filter(|w| w.is_visible().unwrap_or(false)) {
                        let _ = w.set_ignore_cursor_events(on);
                    }
                }
                "quit" => app.exit(0),
                _ => {}
            }
        });
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}
