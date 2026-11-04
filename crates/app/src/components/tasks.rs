//! Tasks under their parent: the data each view needs ([`TaskItem`]), the List's task row and
//! the checklist inside a kanban card. Tasks follow their parent: filters never hide one on its
//! own.

use dioxus::prelude::*;
use kanban_core::{Card, CardId, StateCategory, Workspace};

use crate::components::icons;
use crate::components::item_dialog::{open_edit, open_new};
use crate::components::widgets::{Assignee, Avatar, DateLabel, Icon, Prio, StatePill};
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

/// A task in the epic list, one level below its parent. Not draggable.
#[component]
pub fn TaskRow(task: TaskItem, depth: u8) -> Element {
    let st = use_app();
    let card = task.card;
    let id = card.id;
    let pending = st.pending.read().contains(&id);
    let edit = card.clone();
    rsx! {
        div { class: "crow task depth-{depth.min(3)}", style: "--tc: #{task.type_color}",
            span { class: "grip-space" }
            span { class: "id", style: "width: 56px", "{id}" }
            Prio { priority: card.priority }
            span { class: "type", "{card.work_item_type}" }
            span { style: "flex: 1 1 240px; min-width: 0",
                button {
                    class: "etitle sm",
                    r#type: "button",
                    aria_label: "Edit {card.title}",
                    onclick: move |_| open_edit(st, &edit, false),
                    "{card.title}"
                }
            }
            span { style: "width: 104px", DateLabel { date: card.target_date, open: !task.category.is_done() } }
            StatePill { state: card.state.clone(), category: task.category, color: task.color }
            Assignee { id, person: card.assigned_to.clone(), title: card.title.clone() }
            if pending { span { class: "spinner", style: "color: var(--accent)" } }
        }
    }
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
