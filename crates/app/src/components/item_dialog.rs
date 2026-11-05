//! New / edit dialog for epics and work items: writing on the left (title, parent, tags,
//! description), planning in a side column (type, state, priority, assignee, target date, area),
//! and a footer that says what will be created or lists the unsaved changes. Editing loads the
//! full item first (description, who changed it); a save that hits someone else's edit keeps the
//! dialog open with the user's changes rebased onto the new revision.

use dioxus::prelude::*;
use kanban_core::item::{ItemField, TITLE_MAX, add_tag, has_tag, remove_tag};
use kanban_core::{Card, CardId, ItemDraft, Settings, StateCategory, Workspace};
use time::OffsetDateTime;

use crate::components::icons;
use crate::components::item_fields::{
    DateField, DescriptionEditor, ParentOption, ParentPicker, PersonMenu, StateMenu, TagInput,
    offered_states, read_editor,
};
use crate::components::widgets::{Icon, short_date};
use crate::services::{self, ItemDetails, SaveOutcome, toast};
use crate::state::{AppState, Conflict, DialogState, ToastKind, use_app};

const PRIORITIES: [&str; 4] = ["Critical", "High", "Medium", "Low"];
const EDITOR_ID: &str = "item-description";
/// Parents in the "Recently changed" group of the picker.
const RECENT_PARENTS: usize = 3;

pub fn open_new(
    mut st: AppState,
    is_root: bool,
    work_item_type: String,
    parent: Option<CardId>,
    area: String,
) {
    let (assignee, state) = st
        .workspace
        .peek()
        .as_ref()
        .map(|w| {
            (
                w.me.as_ref()
                    .map(|m| m.unique_name.clone())
                    .unwrap_or_default(),
                offered_states(w, &st.settings.peek(), &work_item_type)
                    .first()
                    .map(|(s, _)| s.clone())
                    .unwrap_or_default(),
            )
        })
        .unwrap_or_default();
    let draft = ItemDraft {
        work_item_type,
        parent,
        area,
        priority: 2,
        assignee,
        state,
        ..ItemDraft::default()
    };
    st.dialog.set(Some(DialogState {
        editing: None,
        is_root,
        original: draft.clone(),
        form: draft,
        rev: 0,
        loaded: true,
        load_error: None,
        meta: None,
        desc_initial: String::new(),
        desc_key: 0,
        desc_dirty: false,
        conflict: None,
        create_another: false,
    }));
}

pub fn open_edit(mut st: AppState, card: &Card, is_root: bool) {
    let draft = ItemDraft::from_card(card, String::new());
    st.dialog.set(Some(DialogState {
        editing: Some(card.id),
        is_root,
        original: draft.clone(),
        form: draft,
        rev: card.rev,
        loaded: false,
        load_error: None,
        meta: None,
        desc_initial: String::new(),
        desc_key: 0,
        desc_dirty: false,
        conflict: None,
        create_another: false,
    }));
    load_details(st, card.id);
}

/// Loads the full item into the open edit dialog. Anything the user already changed stays.
fn load_details(mut st: AppState, id: CardId) {
    spawn(async move {
        let result = services::load_item(st, id).await;
        let mut guard = st.dialog.write();
        let Some(d) = guard.as_mut().filter(|d| d.editing == Some(id)) else {
            return;
        };
        match result {
            Ok(details) => {
                apply_details(d, details);
                d.loaded = true;
                d.load_error = None;
            }
            Err(e) => d.load_error = Some(e),
        }
    });
}

/// Moves the dialog onto `details` (a newer revision): untouched fields take its values.
/// Returns what changed there.
fn apply_details(d: &mut DialogState, details: ItemDetails) -> Vec<ItemField> {
    let new = ItemDraft::from_card(&details.card, details.description);
    let old = std::mem::replace(&mut d.original, new);
    let theirs = d.form.rebase(&old, &d.original);
    d.rev = details.card.rev;
    d.meta = Some(details.meta);
    if !d.desc_dirty && (theirs.contains(&ItemField::Description) || !d.loaded) {
        d.desc_initial = d.form.description.clone();
        d.desc_key += 1;
    }
    theirs
}

/// Unsaved changes, in dialog order. The description counts once the user typed in it.
fn changes_of(d: &DialogState) -> Vec<ItemField> {
    let mut out = d.form.changed_fields(&d.original);
    if d.desc_dirty && !out.contains(&ItemField::Description) {
        out.push(ItemField::Description);
        out.sort_by_key(|f| ItemField::ALL.iter().position(|x| x == f));
    }
    out
}

/// The one-off tag, when the setting is on and the item could have it: a backlog item below the
/// top level. Tasks are left out, since a task without a parent shows nowhere on the board.
fn one_off_tag(ws: &Workspace, settings: &Settings, d: &DialogState) -> Option<String> {
    let ty = &d.form.work_item_type;
    (!d.is_root && !ws.task_types.contains(ty) && !ws.parent_types(ty).is_empty())
        .then(|| settings.one_off_tag().map(str::to_owned))
        .flatten()
}

/// Checked one-off: the item is tagged with it and has no parent.
fn is_one_off(ws: &Workspace, settings: &Settings, d: &DialogState) -> bool {
    one_off_tag(ws, settings, d).is_some_and(|t| has_tag(&d.form.tags, &t))
}

fn needs_parent(ws: &Workspace, settings: &Settings, d: &DialogState) -> bool {
    !d.is_root
        && !ws.parent_types(&d.form.work_item_type).is_empty()
        && !is_one_off(ws, settings, d)
}

fn plural(word: &str) -> String {
    let w = word.to_lowercase();
    match w.strip_suffix('y') {
        Some(stem) if !stem.ends_with(['a', 'e', 'o', 'u']) => format!("{stem}ies"),
        _ => format!("{w}s"),
    }
}

fn article(word: &str) -> &'static str {
    if word.starts_with(['A', 'E', 'I', 'O', 'U', 'a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

/// `just now`, `12 min ago`, `3 h ago`, `yesterday`, `4 days ago`, else the date.
fn ago(t: OffsetDateTime) -> String {
    let mins = (OffsetDateTime::now_utc() - t).whole_minutes();
    match mins {
        m if m < 1 => "just now".into(),
        m if m < 60 => format!("{m} min ago"),
        m if m < 24 * 60 => format!("{} h ago", m / 60),
        m if m < 48 * 60 => "yesterday".into(),
        m if m < 7 * 24 * 60 => format!("{} days ago", m / (24 * 60)),
        _ => format!("on {}", short_date(t.date())),
    }
}

/// A field's value for the changes list.
fn shown(ws: &Workspace, d: &ItemDraft, field: ItemField) -> String {
    let clip = |s: &str| {
        let s = s.trim();
        if s.chars().count() > 48 {
            format!("{}…", s.chars().take(47).collect::<String>())
        } else {
            s.to_owned()
        }
    };
    match field {
        ItemField::Title => clip(&d.title),
        ItemField::Type => d.work_item_type.clone(),
        ItemField::Parent => d.parent.map_or_else(|| "none".into(), |p| format!("#{p}")),
        ItemField::Tags => d.tags.join(", "),
        ItemField::Description => String::new(),
        ItemField::State => d.state.clone(),
        ItemField::Priority => format!("P{}", d.priority),
        ItemField::Assignee => services::assignees(ws)
            .into_iter()
            .find(|p| p.unique_name == d.assignee)
            .map_or_else(|| "Unassigned".into(), |p| p.display_name),
        ItemField::TargetDate => d.target_date.map_or_else(|| "none".into(), short_date),
        ItemField::Area => d.area.rsplit('\\').next().unwrap_or(&d.area).to_owned(),
    }
}

/// Mounted once at the root; renders while a dialog is open.
#[component]
pub fn ItemDialog() -> Element {
    let st = use_app();
    if st.dialog.read().is_none() {
        return rsx! {};
    }
    rsx! { DialogBody {} }
}

#[component]
fn DialogBody() -> Element {
    let mut st = use_app();
    let mut saving = use_signal(|| false);
    let Some(d) = st.dialog.read().clone() else {
        return rsx! {};
    };
    let ws_guard = st.workspace.read();
    let Some(ws) = ws_guard.as_ref() else {
        return rsx! {};
    };
    let form = d.form.clone();
    let editing = d.editing;
    let is_root = d.is_root;
    let ty = form.work_item_type.clone();
    let type_color = ws.type_color(&ty);
    let is_task = ws.task_types.contains(&ty);

    // Types the item can be.
    let mut types: Vec<String> = if is_root {
        ws.root_types()
    } else if is_task {
        ws.task_types.clone()
    } else {
        ws.kanban_levels()
            .iter()
            .filter_map(|l| {
                l.default_type
                    .clone()
                    .or_else(|| l.work_item_types.first().cloned())
            })
            .collect()
    };
    if !types.contains(&ty) {
        types.push(ty.clone());
    }
    let types: Vec<(String, String)> = types
        .into_iter()
        .map(|t| {
            let c = ws.type_color(&t);
            (t, c)
        })
        .collect();

    // Parent candidates: open items of the levels above (or task owners), newest changes first
    // in "Recently changed", the rest in backlog order.
    let parent_types = ws.parent_types(&ty);
    let settings = st.settings.read().clone();
    let need_parent = needs_parent(ws, &settings, &d);
    let one_off = one_off_tag(ws, &settings, &d);
    let one_off_on = is_one_off(ws, &settings, &d);
    let mut candidates: Vec<&Card> = ws
        .cards
        .values()
        .filter(|c| parent_types.contains(&c.work_item_type))
        .filter(|c| {
            Some(c.id) == form.parent
                || !matches!(
                    ws.category(c),
                    StateCategory::Completed | StateCategory::Removed
                )
        })
        .collect();
    candidates.sort_by(|a, b| {
        a.order
            .partial_cmp(&b.order)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.id.cmp(&b.id))
    });
    let mut by_change: Vec<&Card> = candidates.clone();
    by_change.sort_by_key(|c| std::cmp::Reverse(c.changed_date));
    let recent: Vec<CardId> = if candidates.len() > RECENT_PARENTS * 2 {
        by_change
            .iter()
            .take(RECENT_PARENTS)
            .map(|c| c.id)
            .collect()
    } else {
        Vec::new()
    };
    let parent_options: Vec<ParentOption> = candidates
        .iter()
        .map(|c| {
            let context = match c.parent.and_then(|p| ws.cards.get(&p)) {
                Some(p) => format!("{} in #{} {}", c.work_item_type, p.id, p.title),
                None => {
                    let kids = ws.cards.values().filter(|k| k.parent == Some(c.id)).count();
                    format!(
                        "{} · {kids} {}",
                        c.area_path,
                        if kids == 1 { "item" } else { "items" }
                    )
                }
            };
            ParentOption {
                id: c.id,
                title: c.title.clone(),
                type_color: ws.type_color(&c.work_item_type),
                context,
                state: c.state.clone(),
                category: ws.category(c),
                state_color: ws.state_color(c),
                recent: recent.contains(&c.id),
            }
        })
        .collect();
    let parent_noun = parent_types
        .iter()
        .rev()
        .map(|t| plural(t))
        .collect::<Vec<_>>()
        .join(" or ");
    let parent_hint = parent_types
        .iter()
        .rev()
        .map(|t| t.to_lowercase())
        .collect::<Vec<_>>()
        .join(" or ");
    let parent_card = form.parent.and_then(|p| ws.cards.get(&p)).cloned();

    let known_tags = ws.tag_counts();
    let people = services::assignees(ws);
    let me = ws.me.clone();
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
    let area_from_parent = parent_card
        .as_ref()
        .filter(|p| p.area_path == form.area)
        .map(|p| p.id);
    let sprint = ws.current_iteration().cloned();

    let offered = offered_states(ws, &st.settings.read(), &ty);
    let state_column = offered
        .iter()
        .find(|(s, _)| *s == form.state)
        .and_then(|(_, c)| c.clone());
    let default_state = offered.first().map(|(s, _)| s.clone()).unwrap_or_default();
    let changes = changes_of(&d);
    let change_rows: Vec<(ItemField, String, String)> = changes
        .iter()
        .map(|f| match f {
            ItemField::Description => (*f, String::new(), "edited".to_owned()),
            ItemField::Tags => {
                let added: Vec<&String> = form
                    .tags
                    .iter()
                    .filter(|t| !d.original.tags.iter().any(|o| o.eq_ignore_ascii_case(t)))
                    .collect();
                let removed: Vec<&String> = d
                    .original
                    .tags
                    .iter()
                    .filter(|o| !form.tags.iter().any(|t| t.eq_ignore_ascii_case(o)))
                    .collect();
                let fmt =
                    |v: &[&String]| v.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
                let to = if added.is_empty() {
                    "removed".to_owned()
                } else {
                    format!("+ {}", fmt(&added))
                };
                (*f, fmt(&removed), to)
            }
            _ => (*f, shown(ws, &d.original, *f), shown(ws, &form, *f)),
        })
        .collect();
    let who = |p: &kanban_core::Person| {
        if me.as_ref().is_some_and(|m| m.unique_name == p.unique_name) {
            "you".to_owned()
        } else {
            p.display_name.clone()
        }
    };
    let meta_line = d.meta.as_ref().map(|m| {
        let changed = m.changed.map(|t| {
            let by = m.changed_by.as_ref().map(who).unwrap_or_default();
            if by.is_empty() {
                format!("Changed {}", ago(t))
            } else {
                format!("Changed {} by {by}", ago(t))
            }
        });
        let created = m.created.map(|t| {
            let by = m.created_by.as_ref().map(who).unwrap_or_default();
            if by.is_empty() {
                format!("Created {}", short_date(t.date()))
            } else {
                format!("Created {} by {by}", short_date(t.date()))
            }
        });
        (changed, created, m.web_url.clone())
    });
    let assignee_short = people
        .iter()
        .find(|p| p.unique_name == form.assignee)
        .map_or_else(|| "nobody".to_owned(), who);
    let eyebrow = match editing {
        Some(id) => (format!("{ty} · "), format!("#{id}")),
        None => (
            format!(
                "New {} · {} / {}{}",
                ty.to_lowercase(),
                ws.project,
                ws.team,
                sprint
                    .as_ref()
                    .map(|s| format!(" · {}", s.name))
                    .unwrap_or_default()
            ),
            String::new(),
        ),
    };
    drop(ws_guard);

    let title_len = form.title.trim().chars().count();
    let missing_parent = need_parent && form.parent.is_none();
    let blocked = form.title.trim().is_empty()
        || title_len > TITLE_MAX
        || missing_parent
        || saving()
        || (editing.is_some() && changes.is_empty());
    let changed = |f: ItemField| editing.is_some() && changes.contains(&f);
    let dirty = !changes.is_empty() || (editing.is_none() && !form.title.trim().is_empty());

    let mut set = move |f: &dyn Fn(&mut ItemDraft)| {
        if let Some(d) = st.dialog.write().as_mut() {
            f(&mut d.form);
        }
    };
    let mut close = move || {
        if !*saving.peek() {
            st.dialog.set(None);
        }
    };
    let save = move || {
        if *saving.peek() {
            return;
        }
        let Some(d) = st.dialog.peek().clone() else {
            return;
        };
        let need = st
            .workspace
            .peek()
            .as_ref()
            .is_some_and(|ws| needs_parent(ws, &st.settings.peek(), &d));
        let title = d.form.title.trim().chars().count();
        if title == 0
            || title > TITLE_MAX
            || (need && d.form.parent.is_none())
            || (d.editing.is_some() && changes_of(&d).is_empty())
        {
            return;
        }
        saving.set(true);
        spawn(async move {
            let mut form = d.form.clone();
            if (d.desc_dirty || d.editing.is_none())
                && let Some(html) = read_editor(EDITOR_ID).await
            {
                form.description = kanban_core::description::sanitize(&html);
            }
            match d.editing {
                None => {
                    let created = services::create_item(st, &form).await.is_some();
                    saving.set(false);
                    if created && d.create_another {
                        if let Some(dd) = st.dialog.write().as_mut() {
                            dd.form.title.clear();
                            dd.form.description.clear();
                            dd.desc_initial.clear();
                            dd.desc_dirty = false;
                            dd.desc_key += 1;
                        }
                    } else if created {
                        st.dialog.set(None);
                    }
                }
                Some(id) => match services::update_item(st, id, d.rev, &d.original, &form).await {
                    SaveOutcome::Saved => {
                        saving.set(false);
                        st.dialog.set(None);
                    }
                    SaveOutcome::Conflict => {
                        if let Some(dd) = st.dialog.write().as_mut() {
                            dd.form.description = form.description.clone();
                        }
                        match services::load_item(st, id).await {
                            Ok(details) => {
                                let by = details
                                    .meta
                                    .changed_by
                                    .as_ref()
                                    .map(|p| p.display_name.clone());
                                if let Some(dd) = st.dialog.write().as_mut() {
                                    let fields = apply_details(dd, details);
                                    dd.conflict = Some(Conflict { by, fields });
                                }
                            }
                            Err(e) => toast(st, ToastKind::Error, e),
                        }
                        saving.set(false);
                    }
                    SaveOutcome::Failed => saving.set(false),
                },
            }
        });
    };
    let mut save_key = save;
    let mut save_click = save;

    let sub_title = match editing {
        Some(id) => format!("Edit {} {id}", ty.to_lowercase()),
        None => format!("New {}", ty.to_lowercase()),
    };
    let seg_class = match types.len() {
        0..=2 => "seg2",
        3 => "seg2 c3",
        _ => "seg2 c4",
    };
    let state_hint = match (editing.is_some(), &state_column) {
        (false, _) if form.state == default_state => "Default".to_owned(),
        (false, Some(c)) => format!("Starts in {c}"),
        _ => String::new(),
    };
    let state_note = match (editing.is_some(), &state_column) {
        (true, Some(c)) if form.state != d.original.state => {
            Some(format!("Saving moves the card to the “{c}” column."))
        }
        (true, Some(_)) => Some("Changing the state moves the card on the board.".to_owned()),
        _ => None,
    };
    let me_name = me.as_ref().map(|m| m.unique_name.clone());
    let conflict = d.conflict.as_ref().map(|c| {
        (
            c.by.clone().unwrap_or_else(|| "Someone".into()),
            c.fields
                .iter()
                .map(|f| f.label())
                .collect::<Vec<_>>()
                .join(", "),
        )
    });
    let not_me = me_name.as_ref().is_some_and(|m| *m != form.assignee);

    rsx! {
        div {
            class: "overlay",
            // A stray click outside shouldn't throw away a half-written item.
            onclick: move |_| if !dirty { close() },
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    close();
                } else if e.key() == Key::Enter && (e.modifiers().ctrl() || e.modifiers().meta()) {
                    save_key();
                }
            },
            div {
                class: "dlg wide",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "dlg-title",
                onclick: move |e| e.stop_propagation(),
                div { class: "dlg-head",
                    div {
                        div { class: "eyebrow",
                            span { class: "tdot", style: "--tc: #{type_color}" }
                            "{eyebrow.0}"
                            if !eyebrow.1.is_empty() { span { class: "id", "{eyebrow.1}" } }
                        }
                        if let Some((changed_line, created_line, url)) = meta_line {
                            div { class: "meta",
                                if let Some(c) = changed_line { span { "{c}" } span { "·" } }
                                if let Some(c) = created_line { span { "{c}" } span { "·" } }
                                a { href: "{url}", target: "_blank", rel: "noreferrer",
                                    "Open in Azure DevOps"
                                    Icon { svg: icons::EXTERNAL }
                                }
                            }
                        }
                        h2 { id: "dlg-title", class: "sr-only", "{sub_title}" }
                    }
                    button { class: "x", r#type: "button", aria_label: "Close", onclick: move |_| close(), Icon { svg: icons::CLOSE } }
                }

                if let Some((by, fields)) = conflict {
                    div { class: "banner", role: "alert",
                        span { class: "bi", Icon { svg: icons::ALERT } }
                        span { class: "grow",
                            b { "{by} changed this item while you were editing" }
                            if !fields.is_empty() { " ({fields})" }
                            ". Nothing was saved; your edits are on top of their version. Check them and save again."
                        }
                        button {
                            class: "ghostbtn",
                            r#type: "button",
                            onclick: move |_| {
                                if let Some(dd) = st.dialog.write().as_mut() {
                                    dd.conflict = None;
                                }
                            },
                            "Got it"
                        }
                    }
                }
                if let Some(e) = d.load_error.clone() {
                    div { class: "banner err", role: "alert",
                        span { class: "bi", Icon { svg: icons::ALERT } }
                        span { class: "grow", "{e} The description can't be edited until the item loads." }
                        if let Some(id) = editing {
                            button { class: "ghostbtn", r#type: "button", onclick: move |_| load_details(st, id), "Retry" }
                        }
                    }
                }

                div { class: "dlg-grid",
                    div { class: "dlg-main",
                        div { class: "fld",
                            div { class: "fld-top",
                                label { class: "lbl", r#for: "dlg-title-in", if is_root { "Name" } else { "Title" } }
                                if changed(ItemField::Title) {
                                    span { class: "changed", "Changed" }
                                } else {
                                    span { class: if title_len > TITLE_MAX { "counter bad" } else { "counter" }, "{title_len} / {TITLE_MAX}" }
                                }
                            }
                            input {
                                key: "{d.desc_key}",
                                id: "dlg-title-in",
                                class: "in big",
                                r#type: "text",
                                placeholder: if is_root { "What is this epic about?" } else { "What needs to be done?" },
                                value: "{form.title}",
                                onmounted: move |e| async move { let _ = e.set_focus(true).await; },
                                oninput: move |e| set(&|f| f.title = e.value()),
                            }
                        }
                        if !is_root && !parent_types.is_empty() {
                            div { class: "fld",
                                div { class: "fld-top",
                                    span { class: "lbl", "Parent" }
                                    if changed(ItemField::Parent) {
                                        span { class: "changed", "Changed" }
                                    } else if one_off_on {
                                        span { class: "req", "None for one-off items" }
                                    } else if editing.is_none() {
                                        span { class: "req", "Required" }
                                    } else {
                                        span { class: "hint", "{parent_hint}" }
                                    }
                                }
                                if one_off_on {
                                    div { class: "picked locked", aria_disabled: "true",
                                        span { class: "ico", Icon { svg: icons::LOCK } }
                                        span { class: "t",
                                            b { "No parent" }
                                            small { "Tagged “{one_off.clone().unwrap_or_default()}”. Uncheck it to pick a parent." }
                                        }
                                    }
                                } else {
                                ParentPicker {
                                    options: parent_options,
                                    value: form.parent,
                                    noun: parent_noun,
                                    onpick: move |p: Option<CardId>| {
                                        let area = p.and_then(|p| {
                                            st.workspace.peek().as_ref().and_then(|w| w.cards.get(&p).map(|c| c.area_path.clone()))
                                        });
                                        set(&move |f| {
                                            f.parent = p;
                                            if let Some(a) = &area { f.area = a.clone(); }
                                        });
                                    },
                                }
                                }
                            }
                        }
                        div { class: "fld",
                            div { class: "fld-top",
                                span { class: "lbl", "Tags" }
                                if changed(ItemField::Tags) {
                                    span { class: "changed", "Changed" }
                                } else if editing.is_none() {
                                    span { class: "req", "Optional" }
                                }
                            }
                            div { class: "tagrow",
                                div { class: "grow",
                                    TagInput {
                                        tags: form.tags.clone(),
                                        original: editing.map(|_| d.original.tags.clone()),
                                        known: known_tags,
                                        onchange: move |tags: Vec<String>| set(&move |f| f.tags = tags.clone()),
                                    }
                                }
                                if let Some(tag) = one_off.clone() {
                                    label { class: if one_off_on { "oneoff on" } else { "oneoff" }, "data-tip": "One-off item: adds this tag and creates it without a parent",
                                        input {
                                            r#type: "checkbox",
                                            checked: one_off_on,
                                            onchange: move |e| {
                                                let on = e.checked();
                                                let tag = tag.clone();
                                                set(&move |f| {
                                                    if on {
                                                        add_tag(&mut f.tags, &tag);
                                                        f.parent = None;
                                                    } else {
                                                        remove_tag(&mut f.tags, &tag);
                                                    }
                                                });
                                            },
                                        }
                                        span { "{one_off.clone().unwrap_or_default()}" }
                                    }
                                }
                            }
                        }
                        div { class: "fld",
                            div { class: "fld-top",
                                span { class: "lbl", id: "dlg-desc-l", "Description" }
                                if changed(ItemField::Description) {
                                    span { class: "changed", "Changed" }
                                } else if editing.is_none() {
                                    span { class: "req", "Optional" }
                                }
                            }
                            if d.loaded {
                                DescriptionEditor {
                                    key: "{d.desc_key}",
                                    id: EDITOR_ID,
                                    initial: d.desc_initial.clone(),
                                    criteria: editing.is_none() && !is_task,
                                    labelledby: "dlg-desc-l",
                                    ondirty: move |_| {
                                        let dirty = st.dialog.peek().as_ref().is_some_and(|d| d.desc_dirty);
                                        if !dirty
                                            && let Some(dd) = st.dialog.write().as_mut()
                                        {
                                            dd.desc_dirty = true;
                                        }
                                    },
                                }
                            } else {
                                div { class: "rte",
                                    div { class: "rte-loading", if d.load_error.is_some() { "Not loaded." } else { "Loading…" } }
                                }
                            }
                        }
                    }

                    aside { class: "dlg-side", aria_label: "Planning",
                        if types.len() > 1 || editing.is_some() {
                            div { class: "fld",
                                div { class: "fld-top",
                                    span { class: "lbl", id: "dlg-type-l", "Type" }
                                    if editing.is_some() { span { class: "hint", "Can't be changed here" } }
                                }
                                div { class: "{seg_class}", role: "radiogroup", aria_labelledby: "dlg-type-l",
                                    for (t, c) in types {
                                        button {
                                            key: "{t}",
                                            class: if t == ty { "on" } else { "" },
                                            r#type: "button",
                                            role: "radio",
                                            aria_checked: "{t == ty}",
                                            disabled: editing.is_some(),
                                            onclick: {
                                                let t = t.clone();
                                                move |_| {
                                                    let t = t.clone();
                                                    let (states, parents) = st.workspace.peek().as_ref().map(|w| (offered_states(w, &st.settings.peek(), &t).into_iter().map(|(s, _)| s).collect::<Vec<_>>(), w.parent_types(&t))).unwrap_or_default();
                                                    let parent_type = st.dialog.peek().as_ref().and_then(|d| d.form.parent).and_then(|p| {
                                                        st.workspace.peek().as_ref().and_then(|w| w.cards.get(&p).map(|c| c.work_item_type.clone()))
                                                    });
                                                    set(&move |f| {
                                                        f.work_item_type = t.clone();
                                                        if !states.contains(&f.state) {
                                                            f.state = states.first().cloned().unwrap_or_default();
                                                        }
                                                        if parent_type.as_ref().is_some_and(|pt| !parents.contains(pt)) {
                                                            f.parent = None;
                                                        }
                                                    });
                                                }
                                            },
                                            span { class: "tdot", style: "--tc: #{c}" }
                                            span { "{t}" }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "fld",
                            div { class: "fld-top",
                                span { class: "lbl", "State" }
                                if changed(ItemField::State) {
                                    span { class: "changed", "Changed" }
                                } else if !state_hint.is_empty() {
                                    span { class: "hint", "{state_hint}" }
                                }
                            }
                            StateMenu {
                                work_item_type: ty.clone(),
                                value: form.state.clone(),
                                original: editing.map(|_| d.original.state.clone()),
                                onpick: move |s: String| set(&move |f| f.state = s.clone()),
                            }
                            if let Some(n) = state_note { span { class: "hint", "{n}" } }
                        }
                        div { class: "fld",
                            div { class: "fld-top",
                                span { class: "lbl", id: "dlg-prio-l", "Priority" }
                                if changed(ItemField::Priority) {
                                    span { class: "changed", "Changed" }
                                } else {
                                    span { class: "hint", "{PRIORITIES[usize::from(form.priority.clamp(1, 4)) - 1]}" }
                                }
                            }
                            div { class: "seg2 c4", role: "radiogroup", aria_labelledby: "dlg-prio-l",
                                for (i, label) in PRIORITIES.iter().enumerate() {
                                    {
                                        let p = i as u8 + 1;
                                        rsx! {
                                            button {
                                                key: "{p}",
                                                class: if form.priority == p { "on" } else { "" },
                                                r#type: "button",
                                                role: "radio",
                                                aria_checked: "{form.priority == p}",
                                                aria_label: "Priority {p}, {label}",
                                                "data-tip": "{label}",
                                                onclick: move |_| set(&move |f| f.priority = p),
                                                span { class: "pq p{p}", aria_hidden: "true", b {} b {} b {} b {} }
                                                span { class: "mono", "P{p}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "side-sep" }
                        div { class: "fld",
                            div { class: "fld-top",
                                span { class: "lbl", "Assigned to" }
                                if changed(ItemField::Assignee) {
                                    span { class: "changed", "Changed" }
                                } else if not_me {
                                    button {
                                        class: "linkbtn",
                                        r#type: "button",
                                        onclick: move |_| {
                                            if let Some(m) = me_name.clone() {
                                                set(&move |f| f.assignee = m.clone());
                                            }
                                        },
                                        "Assign to me"
                                    }
                                }
                            }
                            PersonMenu {
                                value: form.assignee.clone(),
                                people,
                                me,
                                onpick: move |u: String| set(&move |f| f.assignee = u.clone()),
                            }
                        }
                        div { class: "fld",
                            div { class: "fld-top",
                                span { class: "lbl", "Target date" }
                                if changed(ItemField::TargetDate) { span { class: "changed", "Changed" } }
                            }
                            DateField {
                                value: form.target_date,
                                placeholder: "No target date",
                                onpick: move |v: Option<time::Date>| set(&move |f| f.target_date = v),
                            }
                        }
                        div { class: "fld",
                            div { class: "fld-top",
                                label { class: "lbl", r#for: "dlg-area", "Area" }
                                if changed(ItemField::Area) {
                                    span { class: "changed", "Changed" }
                                } else if let Some(p) = area_from_parent {
                                    span { class: "follow", "From #{p}" }
                                }
                            }
                            span { class: "anchor select-wrap",
                                select {
                                    id: "dlg-area",
                                    class: "selectlike",
                                    onchange: move |e| set(&|f| f.area = e.value()),
                                    for a in areas {
                                        option { key: "{a}", value: "{a}", selected: form.area == a, "{a}" }
                                    }
                                }
                                span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
                            }
                        }
                    }
                }

                div { class: "dlg-foot",
                    div { class: "summary",
                        if editing.is_some() {
                            if change_rows.is_empty() {
                                "No changes yet."
                            } else {
                                div { class: "changes",
                                    span {
                                        span { class: "dot-unsaved" }
                                        " "
                                        b { if change_rows.len() == 1 { "1 unsaved change" } else { "{change_rows.len()} unsaved changes" } }
                                    }
                                    for (f, from, to) in change_rows {
                                        span { key: "{f.label()}", class: "chg",
                                            "{f.label()}"
                                            if !from.is_empty() { s { "{from}" } }
                                            span { class: "arrow", "→" }
                                            b { "{to}" }
                                        }
                                    }
                                }
                            }
                        } else if missing_parent {
                            span { class: "hint bad", "Pick a parent to create it." }
                        } else {
                            "Creates {article(&ty)} "
                            b { "{ty}" }
                            if let Some(p) = parent_card.as_ref() {
                                " under "
                                b { "#{p.id} {p.title}" }
                            } else if one_off_on {
                                " with "
                                b { "no parent" }
                            }
                            " in "
                            b { "{form.area}" }
                            ", assigned to "
                            b { "{assignee_short}" }
                            "."
                        }
                    }
                    if editing.is_none() {
                        label { class: "another",
                            input {
                                r#type: "checkbox",
                                checked: d.create_another,
                                onchange: move |e| {
                                    let on = e.checked();
                                    if let Some(dd) = st.dialog.write().as_mut() {
                                        dd.create_another = on;
                                    }
                                },
                            }
                            "Create another"
                        }
                    }
                    button { class: "btn", r#type: "button", onclick: move |_| close(), "Cancel" }
                    button {
                        class: "btn primary",
                        r#type: "button",
                        disabled: blocked,
                        onclick: move |_| save_click(),
                        if saving() { span { class: "spinner" } }
                        match editing {
                            Some(_) => "Save changes".to_owned(),
                            None => format!("Create {}", ty.to_lowercase()),
                        }
                        kbd { "Ctrl ↵" }
                    }
                }
            }
        }
    }
}
