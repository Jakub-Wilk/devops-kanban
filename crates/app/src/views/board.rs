//! The kanban board for one project / team / board.

use dioxus::prelude::*;

#[component]
pub fn Board(project: String, team: String, board: String) -> Element {
    rsx! {
        h1 { "{project} / {team} / {board}" }
    }
}
