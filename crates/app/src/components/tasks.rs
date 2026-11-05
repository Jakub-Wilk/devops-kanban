//! Tasks under their parent: the data each view needs ([`TaskItem`]) and the checklist inside a
//! kanban card (the List draws tasks as ordinary tree rows). Tasks follow their parent: filters never hide one on its
//! own.

use dioxus::prelude::*;
use kanban_core::{Card, CardId, StateCategory, Workspace};

use crate::components::icons;
use crate::components::item_dialog::{open_edit, open_new};
use crate::components::widgets::{Avatar, Icon};
use crate::services;
use crate::state::use_app;

#[derive(Debug, Clone, PartialEq)]
pub struct TaskItem {
    pub card: Card,
    pub category: StateCategory,
    /// State colour (hex without `#`).
    pub color: String,
    /// Work item type colour, for the stripe.
    pub type_color: String,
}

/// The tasks of `parent`, in backlog order.
pub fn task_items(ws: &Workspace, tasks: Option<&Vec<CardId>>) -> Vec<TaskItem> {
    tasks
        .into_iter()
        .flatten()
        .filter_map(|id| ws.cards.get(id))
        .map(|c| TaskItem {
            card: c.clone(),
            category: ws.category(c),
            color: ws.state_color(c),
            type_color: ws.type_color(&c.work_item_type),
        })
        .collect()
}

/// "+ Task" on a row that can own tasks.
#[component]
pub fn AddTask(
    parent: CardId,
    area: String,
    task_type: String,
    #[props(default)] label: Option<String>,
) -> Element {
    let st = use_app();
    let text = label.unwrap_or_else(|| task_type.clone());
    rsx! {
        button {
            class: "link sm add-task",
            r#type: "button",
            draggable: "false",
            "data-tip": "Add a {task_type.to_lowercase()}",
            onclick: move |e| {
                e.stop_propagation();
                open_new(st, false, task_type.clone(), Some(parent), area.clone());
            },
            Icon { svg: icons::PLUS_14 }
            "{text}"
        }
    }
}

/// The checklist inside a kanban card: tick to close a task, untick to reopen it.
#[component]
pub fn CardTasks(parent: Card, tasks: Vec<TaskItem>, task_type: Option<String>) -> Element {
    let st = use_app();
    let pending = st.pending.read().clone();
    rsx! {
        div { class: "tasks",
            for t in tasks {
                {
                    let id = t.card.id;
                    let done = t.category.is_done();
                    let edit = t.card.clone();
                    let class = format!(
                        "task-row{}{}",
                        if done { " done" } else { "" },
                        if pending.contains(&id) { " pending" } else { "" },
                    );
                    rsx! {
                        div { key: "{id}", class: "{class}", style: "--tc: #{t.type_color}",
                            input {
                                r#type: "checkbox",
                                checked: done,
                                draggable: "false",
                                aria_label: if done { "Reopen {t.card.title}" } else { "Close {t.card.title}" },
                                onclick: move |e| e.stop_propagation(),
                                onchange: move |e| services::set_task_done(st, id, e.checked()),
                            }
                            button {
                                class: "task-title",
                                r#type: "button",
                                draggable: "false",
                                "data-tip": "{t.card.work_item_type} {id}: {t.card.title} ({t.card.state})",
                                onclick: move |e| {
                                    e.stop_propagation();
                                    open_edit(st, &edit, false);
                                },
                                "{t.card.title}"
                            }
                            if let Some(p) = t.card.assigned_to.clone() { Avatar { person: p, small: true } }
                        }
                    }
                }
            }
            if let Some(tt) = task_type {
                AddTask { parent: parent.id, area: parent.area_path.clone(), task_type: tt.clone(), label: format!("Add {}", tt.to_lowercase()) }
            }
        }
    }
}
