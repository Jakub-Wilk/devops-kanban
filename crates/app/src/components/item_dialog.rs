//! New / edit dialog for epics and work items (title, type, parent epic, area, priority,
//! assignee, target date, state).

use dioxus::prelude::*;
use kanban_core::{Card, CardId};

use crate::components::icons;
use crate::components::widgets::Icon;
use crate::services::{self, ItemFields};
use crate::state::{AppState, DialogState, ItemForm, use_app};

const PRIORITIES: [&str; 4] = ["Critical", "High", "Medium", "Low"];

pub fn open_new(
    mut st: AppState,
    is_root: bool,
    work_item_type: String,
    parent: Option<CardId>,
    area: String,
) {
    let assignee = st
        .workspace
        .peek()
        .as_ref()
        .and_then(|w| w.me.as_ref().map(|m| m.unique_name.clone()))
        .unwrap_or_default();
    st.dialog.set(Some(DialogState {
        editing: None,
        is_root,
        form: ItemForm {
            work_item_type,
            parent,
            area,
            priority: 2,
            assignee,
            ..ItemForm::default()
        },
    }));
}

pub fn open_edit(mut st: AppState, card: &Card, is_root: bool) {
    st.dialog.set(Some(DialogState {
        editing: Some(card.id),
        is_root,
        form: ItemForm {
            title: card.title.clone(),
            work_item_type: card.work_item_type.clone(),
            parent: card.parent,
            area: card.area_path.clone(),
            priority: card.priority.unwrap_or(2),
            assignee: card
                .assigned_to
                .as_ref()
                .map(|p| p.unique_name.clone())
                .unwrap_or_default(),
            target_date: card.target_date.map(|d| d.to_string()).unwrap_or_default(),
            state: card.state.clone(),
        },
    }));
}

fn parse_date(s: &str) -> Option<time::Date> {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    time::Date::parse(s.trim(), &format).ok()
}

#[component]
pub fn ItemDialog() -> Element {
    let mut st = use_app();
    let mut saving = use_signal(|| false);
    let Some(dialog) = st.dialog.read().clone() else {
        return rsx! {};
    };
    let ws_guard = st.workspace.read();
    let Some(ws) = ws_guard.as_ref() else {
        return rsx! {};
    };
    let form = dialog.form.clone();
    let editing = dialog.editing;
    let is_root = dialog.is_root;
    let types: Vec<String> = ws
        .kanban_levels()
        .iter()
        .filter_map(|l| {
            l.default_type
                .clone()
                .or_else(|| l.work_item_types.first().cloned())
        })
        .collect();
    let roots = services::root_titles(ws);
    let mut parents: Vec<(CardId, String)> = ws
        .hierarchy()
        .roots
        .iter()
        .filter_map(|id| roots.get(id).map(|t| (*id, t.clone())))
        .collect();
    if let Some(p) = form.parent
        && !parents.iter().any(|(id, _)| *id == p)
    {
        parents.push((
            p,
            ws.cards
                .get(&p)
                .map_or_else(|| format!("#{p}"), |c| c.title.clone()),
        ));
    }
    let mut areas: Vec<String> = ws
        .areas
        .iter()
        .cloned()
        .chain(ws.cards.values().map(|c| c.area_path.clone()))
        .chain(std::iter::once(form.area.clone()))
        .filter(|a| !a.is_empty())
        .collect();
    areas.sort();
    areas.dedup();
    let people = services::assignees(ws);
    let me = ws.me.clone();
    let states = ws.states.states(&form.work_item_type);
    let parent_areas: std::collections::HashMap<CardId, String> = parents
        .iter()
        .filter_map(|(id, _)| ws.cards.get(id).map(|c| (*id, c.area_path.clone())))
        .collect();
    let kind = if is_root {
        ws.root_types()
            .first()
            .cloned()
            .unwrap_or_else(|| "Epic".into())
    } else {
        "work item".into()
    };
    let sub = match editing {
        Some(id) => format!("{} {id}", form.work_item_type),
        None => format!("{} / {}", ws.project, ws.team),
    };
    drop(ws_guard);

    let title = match (editing.is_some(), is_root) {
        (true, true) => format!("Edit {}", kind.to_lowercase()),
        (true, false) => "Edit work item".to_owned(),
        (false, true) => format!("New {}", kind.to_lowercase()),
        (false, false) => "New work item".to_owned(),
    };
    let blocked = form.title.trim().is_empty() || (!is_root && form.parent.is_none()) || saving();
    let mut set = move |f: &dyn Fn(&mut ItemForm)| {
        if let Some(d) = st.dialog.write().as_mut() {
            f(&mut d.form);
        }
    };
    let mut close = move || {
        if !*saving.peek() {
            st.dialog.set(None);
        }
    };

    let submit = move |e: FormEvent| {
        e.prevent_default();
        let Some(d) = st.dialog.peek().clone() else {
            return;
        };
        let f = d.form;
        if f.title.trim().is_empty() || (!d.is_root && f.parent.is_none()) {
            return;
        }
        let assignee = st.workspace.peek().as_ref().and_then(|w| {
            services::assignees(w)
                .into_iter()
                .find(|p| p.unique_name == f.assignee)
        });
        let item = ItemFields {
            title: f.title.trim().to_owned(),
            area: f.area.clone(),
            priority: f.priority,
            assignee,
            target_date: parse_date(&f.target_date),
            state: d.editing.map(|_| f.state.clone()).filter(|s| !s.is_empty()),
        };
        saving.set(true);
        spawn(async move {
            let ok = services::save_item(st, d.editing, f.work_item_type, f.parent, item).await;
            saving.set(false);
            if ok {
                st.dialog.set(None);
            }
        });
    };

    rsx! {
        div {
            class: "overlay",
            onclick: move |_| close(),
            onkeydown: move |e| if e.key() == Key::Escape { close() },
            form {
                class: "dialog",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "dlg-title",
                onclick: move |e| e.stop_propagation(),
                onsubmit: submit,
                div { class: "head",
                    div { style: "display: flex; flex-direction: column; gap: 2px",
                        h2 { id: "dlg-title", "{title}" }
                        span { class: "sub", "{sub}" }
                    }
                    button { class: "twirl", r#type: "button", aria_label: "Close", onclick: move |_| close(), Icon { svg: icons::CLOSE } }
                }
                div { class: "body",
                    label { class: "f",
                        span { class: "lbl", if is_root { "Name" } else { "Title" } }
                        input {
                            class: "field",
                            r#type: "text",
                            required: true,
                            placeholder: if is_root { "What is this epic about?" } else { "What needs to be done?" },
                            value: "{form.title}",
                            onmounted: move |e| async move { let _ = e.set_focus(true).await; },
                            oninput: move |e| set(&|f| f.title = e.value()),
                        }
                    }
                    if !is_root && editing.is_none() && types.len() > 1 {
                        fieldset {
                            legend { "Type" }
                            div { class: "chips",
                                for t in types {
                                    button {
                                        key: "{t}",
                                        class: if form.work_item_type == t { "chip on" } else { "chip" },
                                        r#type: "button",
                                        aria_pressed: "{form.work_item_type == t}",
                                        onclick: {
                                            let t = t.clone();
                                            move |_| { let t = t.clone(); set(&move |f| f.work_item_type = t.clone()) }
                                        },
                                        "{t}"
                                    }
                                }
                            }
                        }
                    }
                    if !is_root {
                        label { class: "f",
                            span { class: "lbl", "Parent epic" }
                            span { class: "select-wrap",
                                select {
                                    class: "field",
                                    required: true,
                                    onchange: move |e| {
                                        let parent: Option<CardId> = e.value().parse().ok();
                                        let area = parent.and_then(|p| parent_areas.get(&p).cloned());
                                        set(&move |f| {
                                            f.parent = parent;
                                            if let Some(a) = &area { f.area = a.clone(); }
                                        });
                                    },
                                    option { value: "", selected: form.parent.is_none(), "Select an epic" }
                                    for (id, t) in parents {
                                        option { key: "{id}", value: "{id}", selected: form.parent == Some(id), "{id}  {t}" }
                                    }
                                }
                                span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
                            }
                        }
                    }
                    label { class: "f",
                        span { class: "lbl", "Area" }
                        span { class: "select-wrap",
                            select {
                                class: "field",
                                onchange: move |e| set(&|f| f.area = e.value()),
                                for a in areas {
                                    option { key: "{a}", value: "{a}", selected: form.area == a, "{a}" }
                                }
                            }
                            span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
                        }
                        if !is_root { span { class: "hint", "Follows the parent epic's area. Change it if this item belongs elsewhere." } }
                    }
                    fieldset {
                        legend { "Priority" }
                        div { class: "chips",
                            for (i, label) in PRIORITIES.iter().enumerate() {
                                {
                                    let p = i as u8 + 1;
                                    rsx! {
                                        button {
                                            key: "{p}",
                                            class: if form.priority == p { "chip on" } else { "chip" },
                                            r#type: "button",
                                            aria_pressed: "{form.priority == p}",
                                            onclick: move |_| set(&move |f| f.priority = p),
                                            span { class: "mono", "P{p}" }
                                            "{label}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "grid2",
                        label { class: "f",
                            span { class: "lbl", "Assigned to" }
                            span { class: "select-wrap",
                                select {
                                    class: "field",
                                    onchange: move |e| set(&|f| f.assignee = e.value()),
                                    for p in people {
                                        option {
                                            key: "{p.unique_name}",
                                            value: "{p.unique_name}",
                                            selected: form.assignee == p.unique_name,
                                            if me.as_ref().is_some_and(|m| m.unique_name == p.unique_name) { "Me ({p.display_name})" } else { "{p.display_name}" }
                                        }
                                    }
                                    option { value: "", selected: form.assignee.is_empty(), "Unassigned" }
                                }
                                span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
                            }
                        }
                        label { class: "f",
                            span { class: "lbl", "Target date" }
                            input {
                                class: "field",
                                r#type: "date",
                                value: "{form.target_date}",
                                onchange: move |e| set(&|f| f.target_date = e.value()),
                            }
                        }
                    }
                    if editing.is_some() && !states.is_empty() {
                        label { class: "f",
                            span { class: "lbl", "State" }
                            span { class: "select-wrap",
                                select {
                                    class: "field",
                                    onchange: move |e| set(&|f| f.state = e.value()),
                                    for s in states {
                                        option { key: "{s}", value: "{s}", selected: form.state == s, "{s}" }
                                    }
                                }
                                span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
                            }
                        }
                    }
                }
                div { class: "foot",
                    button { class: "ghost", r#type: "button", onclick: move |_| close(), "Cancel" }
                    button { class: "primary", r#type: "submit", disabled: blocked,
                        if saving() { span { class: "spinner" } }
                        match (editing.is_some(), is_root) {
                            (true, _) => "Save changes".to_owned(),
                            (false, true) => format!("Create {}", kind.to_lowercase()),
                            (false, false) => "Create work item".to_owned(),
                        }
                    }
                }
            }
        }
    }
}
