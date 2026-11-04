//! devops-kanban desktop entry point: logging, window config, root component.

use dioxus::desktop::{Config, WindowBuilder};
use dioxus::prelude::*;

mod components;
mod routes;
mod services;
mod state;
mod views;

use routes::Route;

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let window = WindowBuilder::new().with_title("devops-kanban");
    dioxus::LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window))
        .launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Stylesheet { href: MAIN_CSS }
        Router::<Route> {}
    }
}
