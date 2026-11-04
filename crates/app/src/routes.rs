//! Route table. Each variant renders the same-named component from [`crate::views`].

use dioxus::prelude::*;

use crate::views::{Board, Settings, Setup};

#[derive(Routable, Clone, PartialEq)]
pub enum Route {
    /// First run / connection: organization + PAT, then project → team → board picker.
    #[route("/")]
    Setup {},
    #[route("/board/:project/:team/:board")]
    Board {
        project: String,
        team: String,
        board: String,
    },
    #[route("/settings")]
    Settings {},
}
