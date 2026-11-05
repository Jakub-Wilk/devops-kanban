//! devops-kanban desktop entry point: settings, logging, window config, root component.

// Release builds on Windows use the GUI subsystem so no console window opens alongside the app.
// Debug builds keep the console for `tracing` output.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use kanban_core::Settings;

mod app_icon;
mod components;
mod routes;
mod services;
mod state;
mod time_tracking;
mod views;

use components::avatars::AvatarServer;
use components::item_dialog::ItemDialog;
use components::time::TimeKeeper;
use components::toast::Toasts;
use components::tooltip::TooltipLayer;
use routes::Route;
use state::AppState;

/// Embedded rather than `asset!`: without `dx`, Dioxus looks for assets next to the
/// executable, so a plain `cargo run` would start unstyled.
/// The window's inner size on first launch, logical px.
const DEFAULT_SIZE: [u32; 2] = [1440, 980];
const MAIN_CSS: &str = include_str!("../assets/main.css");
const DIALOGS_CSS: &str = include_str!("../assets/dialogs.css");
const FONTS: &str = "https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@500&family=Inter:wght@400;500;600;700&display=swap";

fn main() {
    let settings = Settings::load().unwrap_or_else(|e| {
        eprintln!("devops-kanban: ignoring unreadable config: {e}");
        Settings::default()
    });
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&settings.log_level));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    // The icon goes on the window builder: `Config::with_icon` would be overwritten by
    // `with_window`.
    let window = WindowBuilder::new()
        .with_title("Board Buddy")
        .with_maximized(settings.window_maximized)
        // Hidden until the first frame is rendered (see `App`): otherwise the window shows at
        // its normal size and only then maximises, and briefly shows an empty webview.
        .with_visible(false)
        .with_window_icon(app_icon::icon())
        // Maximised: created at its maximised size (see `WindowKeeper`).
        .with_inner_size({
            let [w, h] = if settings.window_maximized {
                settings.maximized_size.or(settings.window_size)
            } else {
                settings.window_size
            }
            .unwrap_or(DEFAULT_SIZE);
            LogicalSize::new(f64::from(w), f64::from(h))
        })
        .with_min_inner_size(LogicalSize::new(960.0, 640.0));
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                .with_window(window)
                .with_menu(None)
                .with_background_color((14, 17, 22, 255))
                // wry's file-drop handler swallows HTML5 drag events on Linux and Windows.
                .with_disable_drag_drop_handler(true),
        )
        .launch(App);
}

#[component]
fn App() -> Element {
    use_context_provider(|| {
        let settings = Settings::load().unwrap_or_default();
        let pat = settings
            .is_configured()
            .then(|| kanban_core::config::load_pat(&settings.organization))
            .and_then(|r| {
                r.inspect_err(|e| tracing::warn!(error = %e, "keyring unavailable"))
                    .ok()
            })
            .flatten();
        let client = pat
            .as_deref()
            .and_then(|p| services::build_client(&settings, p).ok());
        AppState::new(settings, pat, client)
    });
    // Show the window once the first frame is in, maximised first when it was last time (the
    // window manager applies a maximise of a hidden window before showing it).
    let st = use_context::<AppState>();
    use_effect(move || {
        let window = dioxus::desktop::window();
        if st.settings.peek().window_maximized {
            window.set_maximized(true);
        }
        window.set_visible(true);
    });
    rsx! {
        document::Link { rel: "preconnect", href: "https://fonts.googleapis.com" }
        document::Stylesheet { href: FONTS }
        document::Style { {MAIN_CSS} }
        document::Style { {DIALOGS_CSS} }
        Router::<Route> {}
        ItemDialog {}
        Toasts {}
        TooltipLayer {}
        AvatarServer {}
        SelectionGuard {}
        TimeKeeper {}
        WindowKeeper {}
    }
}

/// Text regions inside draggable cards and rows (`.selectable`, the expanded card description)
/// can't be selected by default: pressing in them starts a drag of the card, whatever their own
/// `draggable` says. While the mouse is down in one, this turns the draggable ancestor off, so
/// the press selects text instead (turned back on at mouse up); a press on already-selected text
/// starts a new selection rather than dragging it, and any drag starting there is cancelled.
const SELECTION_JS: &str = r#"
if (window.__selectionGuard) return;
window.__selectionGuard = true;
const regionOf = (t) => {
  const el = t && (t.nodeType === 1 ? t : t.parentElement);
  return el && el.closest ? el.closest('.selectable, .card-desc') : null;
};
document.addEventListener('mousedown', (e) => {
  if (e.button !== 0) return;
  const region = regionOf(e.target);
  const host = region && region.closest('[draggable="true"]');
  if (!host) return;
  // Pressing on selected text would drag that text (and with it the card): a single press
  // starts a new selection instead. Double and triple clicks keep selecting words and lines.
  if (e.detail === 1) window.getSelection()?.removeAllRanges();
  host.setAttribute('draggable', 'false');
  const restore = () => {
    host.setAttribute('draggable', 'true');
    document.removeEventListener('mouseup', restore, true);
  };
  document.addEventListener('mouseup', restore, true);
}, true);
// Whatever drag still starts in such a region never reaches the card.
document.addEventListener('dragstart', (e) => {
  const region = regionOf(e.target);
  if (region && region.closest('[draggable]')) {
    e.preventDefault();
    e.stopPropagation();
  }
}, true);
"#;

#[component]
fn SelectionGuard() -> Element {
    use_hook(|| {
        let _ = document::eval(SELECTION_JS);
    });
    rsx! {}
}

/// Remembers the window: whether it is maximised, its normal size, and its maximised size
/// (see `maximized_size` in docs/settings.md). Sizes change on every step of a resize drag, so
/// they're saved at most every half second, and on close.
///
/// A window opened maximised is created at its maximised size; un-maximising it would then
/// restore to that size, so the first restore puts back the remembered normal size.
#[component]
fn WindowKeeper() -> Element {
    let mut st = use_context::<AppState>();
    let last_save = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(None::<std::time::Instant>)));
    dioxus::desktop::use_wry_event_handler(move |event, _| {
        let dioxus::desktop::tao::event::Event::WindowEvent { event, .. } = event else {
            return;
        };
        match event {
            dioxus::desktop::WindowEvent::Resized(_) => {
                let window = dioxus::desktop::window();
                if window.is_minimized() {
                    return;
                }
                let maximized = window.is_maximized();
                let logical = window.inner_size().to_logical::<f64>(window.scale_factor());
                let size = [logical.width.round() as u32, logical.height.round() as u32];
                let (was_maximized, normal, max_size) = {
                    let s = st.settings.peek();
                    (s.window_maximized, s.window_size, s.maximized_size)
                };
                let near = |a: [u32; 2], b: [u32; 2]| {
                    a[0].abs_diff(b[0]) <= 16 && a[1].abs_diff(b[1]) <= 16
                };
                if !maximized && was_maximized && max_size.is_some_and(|m| near(m, size)) {
                    // Restored to the size it was created at: use the normal size instead.
                    let [w, h] = normal.unwrap_or(DEFAULT_SIZE);
                    window.set_inner_size(LogicalSize::new(f64::from(w), f64::from(h)));
                    st.settings.write().window_maximized = false;
                    services::save_settings(st);
                    return;
                }
                let changed = {
                    let mut s = st.settings.write();
                    let before = (s.window_maximized, s.window_size, s.maximized_size);
                    s.window_maximized = maximized;
                    if maximized {
                        s.maximized_size = Some(size);
                    } else {
                        s.window_size = Some(size);
                    }
                    before != (s.window_maximized, s.window_size, s.maximized_size)
                };
                let due = last_save
                    .get()
                    .is_none_or(|t| t.elapsed() >= std::time::Duration::from_millis(500));
                if changed && (due || maximized != was_maximized) {
                    last_save.set(Some(std::time::Instant::now()));
                    services::save_settings(st);
                }
            }
            dioxus::desktop::WindowEvent::CloseRequested => {
                services::save_settings(st);
                last_save.set(None);
            }
            _ => {}
        }
    });
    rsx! {}
}
