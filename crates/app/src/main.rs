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
const MAIN_CSS: &str = include_str!("../assets/main.css");
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
        .with_window_icon(app_icon::icon())
        .with_inner_size(LogicalSize::new(1440.0, 980.0))
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
    rsx! {
        document::Link { rel: "preconnect", href: "https://fonts.googleapis.com" }
        document::Stylesheet { href: FONTS }
        document::Style { {MAIN_CSS} }
        Router::<Route> {}
        ItemDialog {}
        Toasts {}
        TooltipLayer {}
        AvatarServer {}
        TimeKeeper {}
        WindowKeeper {}
    }
}

/// Remembers whether the window is maximised, so the next launch opens the same way. Saved when
/// it changes (maximising and restoring both resize the window), ignoring minimised states.
#[component]
fn WindowKeeper() -> Element {
    let mut st = use_context::<AppState>();
    dioxus::desktop::use_wry_event_handler(move |event, _| {
        if let dioxus::desktop::tao::event::Event::WindowEvent {
            event: dioxus::desktop::WindowEvent::Resized(_),
            ..
        } = event
        {
            let window = dioxus::desktop::window();
            if window.is_minimized() {
                return;
            }
            let maximized = window.is_maximized();
            if st.settings.peek().window_maximized != maximized {
                st.settings.write().window_maximized = maximized;
                services::save_settings(st);
            }
        }
    });
    rsx! {}
}
